//! Reusable process-boundary harness for the workspace dispatcher
//! (`test-helpers` builds only; plan P4.2b item 11).
//!
//! It launches the real `g5d dispatch` child under a temporary root, speaks
//! the real authenticated mux + JSON-RPC protocol, stages the shipped module
//! catalog (optionally with the real `g5d dispatch-fixture-module` test module)
//! and pins its hash exactly as the host runtime bundle does. Later API and
//! provider suites build on this instead of a fake task service; nothing here
//! is a production fallback or evidence of VM isolation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use g5_internal_api::stream_mux::{Channel, Mux, MuxReader, MuxWriter};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

/// The incarnation epoch every harness dispatcher is started with.
pub const EPOCH: u64 = 7;
/// The catalog shipped in the host runtime bundle, verbatim.
pub const PRODUCTION_CATALOG: &str = include_str!("../../g5d-dispatch/modules/catalog.json");
/// Catalog identity of the test module process.
pub const FIXTURE_MODULE_ID: &str = "test.fixture";
pub const FIXTURE_MODULE_VERSION: &str = "1.0.0";
pub const FIXTURE_INPUT_SCHEMA: &str = "g5.test.fixture.run/v1";
pub const FIXTURE_WALL_CLOCK_MS: u64 = 3_000;
pub const FIXTURE_MAX_OUTPUT_BYTES: u64 = 16 * 1024;
pub const FIXTURE_MAX_LOG_BYTES: u64 = 16 * 1024;

fn new_listener_token() -> [u8; 32] {
    let mut token = [0; 32];
    getrandom::getrandom(&mut token).expect("dispatcher fixture listener entropy");
    token
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// An unsigned workspace JWT shape carrying `vm_id` and a ten-minute expiry;
/// the dispatcher only inspects those claims after host authentication.
pub fn workspace_token(vm_id: Uuid) -> String {
    workspace_token_with_expiry(vm_id, now_ms() / 1000 + 600)
}

/// The same shape with an explicit `exp` (seconds), for renewals that must
/// differ byte-for-byte from the token the dispatcher started with.
pub fn workspace_token_with_expiry(vm_id: Uuid, exp: u64) -> String {
    format!(
        "e30.{}.sig",
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(
                &json!({"vm_id":vm_id,"exp":exp,"token_use":"workspace_incarnation"})
            )
            .unwrap()
        )
    )
}

/// Run `git -C <repo> <args>` and return trimmed stdout, panicking on failure.
pub fn git(repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Harness")
        .env("GIT_AUTHOR_EMAIL", "harness@example.test")
        .env("GIT_COMMITTER_NAME", "Harness")
        .env("GIT_COMMITTER_EMAIL", "harness@example.test")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

/// Hex SHA-256 of a JSON value's canonical serialization; the digest the
/// control plane binds on `modules.input.respond`.
pub fn response_digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}

/// The catalog entry for the real test module process.
pub fn fixture_module_entry() -> Value {
    json!({
        "module_id": FIXTURE_MODULE_ID,
        "module_version": FIXTURE_MODULE_VERSION,
        "manifest_hash": "sha256:test.fixture/1.0.0",
        "lifecycle": "bounded_run",
        "entrypoint": ["g5d", "dispatch-fixture-module"],
        "builtin": false,
        "limits": {
            "wall_clock_ms": FIXTURE_WALL_CLOCK_MS,
            "cpu_cores": 1,
            "memory_mib": 4096,
            "max_output_bytes": FIXTURE_MAX_OUTPUT_BYTES,
            "max_log_bytes": FIXTURE_MAX_LOG_BYTES,
            "concurrency": 4
        },
        "operations": {
            "run": { "input_schema": FIXTURE_INPUT_SCHEMA, "event_schema": "g5.test.fixture.event/v1", "output_schema": "g5.test.fixture.outcome/v1" },
            "input.respond": { "input_schema": "g5.test.fixture.native-response/v1" }
        },
        "requested_capabilities": ["workspace.read"],
        "observability": { "logs": "bounded_redacted", "events": "journaled", "redaction": "configured_secrets" }
    })
}

#[derive(Clone, Debug)]
pub struct RepoSpec {
    pub root_path: String,
    pub role: Option<String>,
    pub branch: String,
    pub upstream_branch: Option<String>,
    /// The layout's `follow_branch`: a personal workspace's private repository.
    pub follow_branch: bool,
}

impl Default for RepoSpec {
    fn default() -> Self {
        Self {
            root_path: "code".into(),
            role: Some("code".into()),
            branch: "shared".into(),
            upstream_branch: Some("main".into()),
            follow_branch: false,
        }
    }
}

/// An edit applied to the staged catalog before it is hashed.
pub type CatalogEdit = Arc<dyn Fn(&mut Value) + Send + Sync>;

#[derive(Clone)]
pub struct DispatcherOptions {
    pub lease_ttl_secs: u64,
    /// Arm the test-only mutation gate (`G5D_DISPATCH_TEST_MUTATION_GATE`).
    pub mutation_gate: bool,
    /// Add the real test module process to the staged catalog.
    pub fixture_module: bool,
    /// Override the catalog's `bounded_execution_slots`.
    pub bounded_execution_slots: Option<u32>,
    pub repos: Vec<RepoSpec>,
    /// Reference checkouts beside the members (the layout's `references`):
    /// served by the Git origin like members, never flushed.
    pub references: Vec<RepoSpec>,
    /// Extra environment applied last (overrides harness values).
    pub env: Vec<(String, String)>,
    /// Pass the catalog pin (`G5D_MODULE_CATALOG_HASH`); `false` proves the
    /// dispatcher refuses to start unpinned.
    pub pin_catalog: bool,
    /// Serve an authenticated smart-HTTP Git origin as the internal control
    /// plane (`G5D_API_URL`), with one bare repository per member.
    pub git_origin: bool,
    /// Override the fixture module's wall clock (default
    /// [`FIXTURE_WALL_CLOCK_MS`]) for scenarios that need a long-lived module.
    pub fixture_wall_clock_ms: Option<u64>,
    /// Edit the staged catalog before it is hashed (limits, slots, entries).
    pub catalog_edit: Option<CatalogEdit>,
    /// Force the first bind attempt to a known port in collision tests.
    pub initial_port: Option<u16>,
}

impl Default for DispatcherOptions {
    fn default() -> Self {
        Self {
            lease_ttl_secs: 120,
            mutation_gate: false,
            fixture_module: false,
            bounded_execution_slots: None,
            repos: vec![RepoSpec::default()],
            references: Vec::new(),
            env: Vec::new(),
            pin_catalog: true,
            git_origin: false,
            fixture_wall_clock_ms: None,
            catalog_edit: None,
            initial_port: None,
        }
    }
}

/// A running `g5d dispatch` child and its identities.
pub struct TestDispatcher {
    pub root: TempDir,
    pub child: Child,
    pub port: u16,
    pub workspace: Uuid,
    pub vm: Uuid,
    pub owner: Uuid,
    /// A unique listener secret prevents a reused port from authenticating to
    /// another fixture and retiring its active control peer.
    pub listener_token: [u8; 32],
    /// The workspace JWT this incarnation was started with (a configured secret).
    pub workspace_token: String,
    /// The first member's repository id.
    pub repo: Uuid,
    pub repos: Vec<(Uuid, RepoSpec)>,
    pub references: Vec<(Uuid, RepoSpec)>,
    pub catalog: Value,
    pub catalog_hash: String,
    /// The authenticated Git origin, when `git_origin` was requested.
    pub origin: Option<GitHttpOrigin>,
    binary: PathBuf,
    env: Vec<(String, String)>,
    /// Keep the authenticated startup peer for the first caller rather than
    /// replacing a throwaway readiness connection immediately after startup.
    ready_rpc: Mutex<Option<Rpc>>,
}

impl TestDispatcher {
    /// Launch the dispatcher, panicking with its stderr when it does not start.
    pub async fn spawn(binary: &Path, options: DispatcherOptions) -> Self {
        Self::try_spawn(binary, options)
            .await
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Launch the dispatcher; `Err` carries the child's stderr when it exits
    /// before listening (for example a catalog pin mismatch).
    pub async fn try_spawn(binary: &Path, options: DispatcherOptions) -> Result<Self, String> {
        let root = tempfile::tempdir().unwrap();
        let workspace_root = root.path().join("work");
        std::fs::create_dir_all(workspace_root.join(".g5")).unwrap();
        let workspace = Uuid::new_v4();
        let vm = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let listener_token = new_listener_token();
        let mut repos = Vec::new();
        let mut layout = Vec::new();
        for spec in &options.repos {
            let repo = Uuid::new_v4();
            let member = workspace_root.join(&spec.root_path);
            std::fs::create_dir_all(&member).unwrap();
            git(&member, &["init", "-q", "-b", &spec.branch]);
            layout.push(json!({
                "repo_id": repo,
                "root_path": spec.root_path,
                "role": spec.role,
                "git_url": format!("http://127.0.0.1:1/git/{repo}"),
                "branch": spec.branch,
                "branch_oid": "a".repeat(40),
                "upstream_branch": spec.upstream_branch,
                "object_format": "sha1",
                "follow_branch": spec.follow_branch,
            }));
            repos.push((repo, spec.clone()));
        }
        let mut references = Vec::new();
        let mut reference_layout = Vec::new();
        for spec in &options.references {
            let repo = Uuid::new_v4();
            let checkout = workspace_root.join(&spec.root_path);
            std::fs::create_dir_all(&checkout).unwrap();
            git(&checkout, &["init", "-q", "-b", &spec.branch]);
            reference_layout.push(json!({
                "repo_id": repo,
                "root_path": spec.root_path,
                "role": spec.role,
                "git_url": format!("http://127.0.0.1:1/git/{repo}"),
                "branch": spec.branch,
                "commit": "a".repeat(40),
            }));
            references.push((repo, spec.clone()));
        }

        // Stage the catalog exactly where the host bundle mounts it.
        let mut catalog: Value = serde_json::from_str(PRODUCTION_CATALOG).unwrap();
        if options.fixture_module {
            let mut entry = fixture_module_entry();
            if let Some(wall_clock) = options.fixture_wall_clock_ms {
                entry["limits"]["wall_clock_ms"] = json!(wall_clock);
            }
            catalog["modules"].as_array_mut().unwrap().push(entry);
        }
        if let Some(slots) = options.bounded_execution_slots {
            catalog["bounded_execution_slots"] = json!(slots);
        }
        if let Some(edit) = &options.catalog_edit {
            edit(&mut catalog);
        }
        let catalog_bytes = serde_json::to_vec_pretty(&catalog).unwrap();
        let catalog_hash = format!("{:x}", Sha256::digest(&catalog_bytes));
        let catalog_dir = root.path().join("opt/g5/modules");
        std::fs::create_dir_all(&catalog_dir).unwrap();
        std::fs::write(catalog_dir.join("catalog.json"), &catalog_bytes).unwrap();

        let port = options.initial_port.unwrap_or_else(unused_port);
        let workspace_token = workspace_token(vm);
        let origin = if options.git_origin {
            let origin = GitHttpOrigin::start(
                root.path().join("origin"),
                workspace_token.clone(),
                workspace,
                vm,
                repos.iter().map(|(repo, _)| *repo).collect(),
            )
            .await;
            for (repo, _) in repos.iter().chain(&references) {
                let bare = origin.bare(*repo);
                std::fs::create_dir_all(&bare).unwrap();
                git(&bare, &["init", "-q", "--bare"]);
            }
            Some(origin)
        } else {
            None
        };
        let api_url = options
            .env
            .iter()
            .rev()
            .find(|(key, _)| key == "G5D_API_URL")
            .map(|(_, value)| value.clone())
            .or_else(|| origin.as_ref().map(|origin| origin.url.clone()))
            .unwrap_or_else(|| "http://127.0.0.1:1".to_owned());
        // The layout's member and reference URLs must be the internal origin's.
        let internal = |entries: Vec<Value>| {
            entries
                .into_iter()
                .map(|mut entry| {
                    entry["git_url"] = json!(format!(
                        "{api_url}/git/{}",
                        entry["repo_id"].as_str().unwrap()
                    ));
                    entry
                })
                .collect::<Vec<_>>()
        };
        std::fs::write(
            workspace_root.join(".g5/layout.json"),
            serde_json::to_vec(&json!({
                "repos": internal(layout),
                "restore": null,
                "references": internal(reference_layout),
            }))
            .unwrap(),
        )
        .unwrap();
        let mut env: Vec<(String, String)> = vec![
            ("PATH".into(), std::env::var("PATH").unwrap_or_default()),
            ("G5D_WORKSPACE_ID".into(), workspace.to_string()),
            ("G5D_VM_ID".into(), vm.to_string()),
            ("G5D_OWNER_USER_ID".into(), owner.to_string()),
            ("G5D_EPOCH".into(), EPOCH.to_string()),
            ("G5D_WORKSPACE_TOKEN".into(), workspace_token.clone()),
            ("G5D_DISPATCH_TOKEN".into(), hex::encode(listener_token)),
            ("G5D_API_URL".into(), api_url),
            ("G5D_DISPATCH_PORT".into(), port.to_string()),
            // Several harness dispatchers share one host: the Git transport
            // door must not claim the fixed guest port.
            ("G5D_GIT_DOOR_PORT".into(), "0".into()),
            (
                "G5D_DISPATCH_TEST_ROOT".into(),
                root.path().to_string_lossy().into_owned(),
            ),
            ("G5D_OWNER_GIT_NAME".into(), "Fixture Owner".into()),
            ("G5D_OWNER_GIT_EMAIL".into(), "fixture@example.test".into()),
            (
                "G5D_LEASE_TTL_SECS".into(),
                options.lease_ttl_secs.to_string(),
            ),
        ];
        if options.pin_catalog {
            env.push(("G5D_MODULE_CATALOG_HASH".into(), catalog_hash.clone()));
        }
        if options.mutation_gate {
            env.push((
                "G5D_DISPATCH_TEST_MUTATION_GATE".into(),
                root.path()
                    .join("mutation-gate")
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        // Preserve only dynamic-loader paths needed by native test binaries.
        for key in [
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "LD_LIBRARY_PATH",
        ] {
            if let Ok(value) = std::env::var(key) {
                env.push((key.to_string(), value));
            }
        }
        env.extend(options.env.iter().cloned());
        let child = launch(binary, &env, root.path());
        let mut fixture = Self {
            root,
            child,
            port,
            workspace,
            vm,
            owner,
            listener_token,
            workspace_token,
            repo: repos[0].0,
            repos,
            references,
            catalog,
            catalog_hash,
            origin,
            binary: binary.to_path_buf(),
            env,
            ready_rpc: Mutex::new(None),
        };
        fixture.wait_listening_with_bind_retry(0).await?;
        Ok(fixture)
    }

    async fn wait_listening_with_bind_retry(
        &mut self,
        mut stderr_start: usize,
    ) -> Result<(), String> {
        for attempt in 0..5 {
            match self.wait_listening().await {
                Ok(()) => return Ok(()),
                Err(_)
                    if attempt < 4
                        && self.child.try_wait().unwrap().is_some()
                        && self
                            .stderr()
                            .get(stderr_start..)
                            .is_some_and(|stderr| stderr.contains("Address already in use")) =>
                {
                    // Another parallel test can claim the ephemeral port after
                    // our probe closes and before the dispatcher binds it,
                    // including when restarting the same incarnation.
                    self.port = unused_port();
                    self.env
                        .iter_mut()
                        .rev()
                        .find(|(key, _)| key == "G5D_DISPATCH_PORT")
                        .unwrap()
                        .1 = self.port.to_string();
                    // Keep diagnostics, but classify only the current child's
                    // output so an old collision cannot hide a later error.
                    stderr_start = self.stderr().len();
                    self.child = launch(&self.binary, &self.env, self.root.path());
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("the final startup attempt always returns")
    }

    async fn wait_listening(&mut self) -> Result<(), String> {
        let mut last_error = "no connection attempted".to_string();
        timeout(Duration::from_secs(20), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return Err(format!("dispatcher exited {status}: {}", self.stderr()));
                }
                match timeout(Duration::from_secs(1), self.authenticated_rpc()).await {
                    Ok(Ok(rpc)) => {
                        *self.ready_rpc.lock().unwrap() = Some(rpc);
                        return Ok(());
                    }
                    Ok(Err(error)) => last_error = error,
                    Err(_) => last_error = "authenticated runtime.ready timed out".to_string(),
                }
                if let Some(status) = self.child.try_wait().unwrap() {
                    return Err(format!("dispatcher exited {status}: {}", self.stderr()));
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| format!("dispatcher listener startup timed out: {last_error}"))?
    }

    /// Kill the dispatcher process and start it again on the same root with
    /// the same incarnation identities: a restart within the incarnation.
    pub async fn restart(&mut self) {
        self.ready_rpc.lock().unwrap().take();
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        let stderr_start = self.stderr().len();
        self.child = launch(&self.binary, &self.env, self.root.path());
        self.wait_listening_with_bind_retry(stderr_start)
            .await
            .unwrap_or_else(|error| panic!("{error}"));
    }

    pub fn workspace_root(&self) -> PathBuf {
        self.root.path().join("work")
    }

    /// The first member's checkout.
    pub fn repo_root(&self) -> PathBuf {
        self.member_root(0)
    }

    pub fn member_root(&self, index: usize) -> PathBuf {
        self.workspace_root().join(&self.repos[index].1.root_path)
    }

    /// Dispatcher state (`/var/lib/g5d` mapped under the test root).
    pub fn state_dir(&self) -> PathBuf {
        self.root.path().join("var/lib/g5d")
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.root.path().join("opt/g5/modules/catalog.json")
    }

    pub fn stderr(&self) -> String {
        std::fs::read_to_string(self.root.path().join("stderr.log")).unwrap_or_default()
    }

    pub fn module_entry(&self, module_id: &str) -> Value {
        self.catalog["modules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["module_id"] == module_id)
            .cloned()
            .unwrap_or_else(|| panic!("catalog has no module {module_id}"))
    }

    /// An owner (`private == false`) or lifecycle (`private == true`) context.
    pub fn context(&self, private: bool) -> Value {
        let mut context = json!({"workspace_id":self.workspace,"expected_vm_id":self.vm,"expected_epoch":EPOCH,"deadline_ms":now_ms()+120_000,"idempotency_key":Uuid::new_v4()});
        if private {
            context["lifecycle"] = json!(true);
        } else {
            context["actor_user_id"] = json!(self.owner);
        }
        context
    }

    /// A facade-validated operator support grant context: no actor, no lifecycle.
    pub fn inspection_context(&self) -> Value {
        json!({"workspace_id":self.workspace,"expected_vm_id":self.vm,"expected_epoch":EPOCH,"inspection_grant_id":Uuid::new_v4(),"deadline_ms":now_ms()+120_000,"idempotency_key":Uuid::new_v4()})
    }

    pub fn params(&self, private: bool, mut params: Value) -> Value {
        params["context"] = self.context(private);
        params
    }

    pub fn file(&self, path: &str) -> Value {
        json!({"repo_id":self.repo,"path":path})
    }

    /// Canonical `task.<uuid>.<attempt>.<vm_uuid>` identity for this incarnation.
    pub fn task_exec_id(&self, task: Uuid, attempt: u32) -> String {
        format!("task.{task}.{attempt}.{}", self.vm)
    }

    /// Canonical `operation.<uuid>.1.<vm_uuid>` identity for this incarnation.
    pub fn operation_exec_id(&self, operation: Uuid) -> String {
        format!("operation.{operation}.1.{}", self.vm)
    }

    /// A facade-asserted capability grant.
    pub fn grant(&self, capabilities: &[&str]) -> Value {
        json!({"grant_id":Uuid::new_v4(),"capabilities":capabilities})
    }

    /// Authenticate, open the RPC stream and consume `runtime.ready`.
    pub async fn connect(&self) -> Rpc {
        if let Some(rpc) = self.ready_rpc.lock().unwrap().take() {
            return rpc;
        }
        timeout(Duration::from_secs(20), self.authenticated_rpc())
            .await
            .expect("dispatcher authenticated connection timed out")
            .unwrap_or_else(|error| panic!("{error}"))
    }

    async fn authenticated_rpc(&self) -> Result<Rpc, String> {
        let mut tcp = TcpStream::connect(("127.0.0.1", self.port))
            .await
            .map_err(|error| format!("dispatcher connection failed: {error}"))?;
        tcp.write_all(&self.listener_token)
            .await
            .map_err(|error| format!("dispatcher authentication write failed: {error}"))?;
        let mux = Mux::new(tcp, true);
        let (writer, mut reader) = mux
            .open(Channel::Rpc)
            .await
            .map_err(|error| format!("dispatcher RPC open failed: {error}"))?
            .into_split();
        let line = reader
            .read_line_with_limit(64 * 1024)
            .await
            .ok_or("dispatcher closed before runtime.ready")?;
        let ready: Value = serde_json::from_str(&line)
            .map_err(|_| "dispatcher runtime.ready is not valid JSON")?;
        if ready["method"] != "runtime.ready" {
            return Err("dispatcher did not send runtime.ready first".into());
        }
        for (field, expected) in [
            ("workspace_id", json!(self.workspace)),
            ("vm_id", json!(self.vm)),
            ("epoch", json!(EPOCH)),
            ("module_catalog_hash", json!(self.catalog_hash)),
            (
                "module_catalog_version",
                self.catalog["catalog_version"].clone(),
            ),
        ] {
            if ready["params"][field] != expected {
                return Err(format!("dispatcher runtime.ready {field} mismatch"));
            }
        }
        if !ready["params"]["dispatcher_contract_version"].is_number() {
            return Err("dispatcher runtime.ready has no contract version".into());
        }
        Ok(Rpc {
            _mux: mux,
            writer,
            reader,
            pending: Vec::new(),
            buffered: Vec::new(),
            id: 0,
            ready: ready["params"].clone(),
        })
    }
}

impl Drop for TestDispatcher {
    fn drop(&mut self) {
        // A failing test gets the dispatcher's own stderr next to the panic.
        if std::thread::panicking() {
            let alive = match self.child.try_wait() {
                Ok(None) => "still running".to_owned(),
                Ok(Some(status)) => format!("exited: {status}"),
                Err(error) => format!("unknown: {error}"),
            };
            eprintln!(
                "--- dispatcher stderr ({}, process {alive}) ---\n{}\n--- end dispatcher stderr ---",
                self.vm,
                self.stderr()
            );
        }
    }
}

/// One authenticated RPC stream.
pub struct Rpc {
    pub _mux: Mux,
    pub writer: MuxWriter,
    pub reader: MuxReader,
    /// Notifications and replies received while waiting for something else.
    pub pending: Vec<Value>,
    pub buffered: Vec<u8>,
    pub id: u64,
    /// The `runtime.ready` params this stream opened with.
    pub ready: Value,
}

impl Rpc {
    pub async fn send(&self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        timeout(Duration::from_secs(20), self.writer.send(&bytes))
            .await
            .unwrap()
            .unwrap();
    }

    pub async fn recv(&mut self) -> Value {
        timeout(Duration::from_secs(30), async {
            let mut scanned = 0;
            loop {
                if let Some(index) = self.buffered[scanned..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                {
                    let index = scanned + index;
                    let line: Vec<_> = self.buffered.drain(..=index).collect();
                    return serde_json::from_slice(&line).unwrap();
                }
                scanned = self.buffered.len();
                let Some(chunk) = self.reader.recv().await else {
                    // Give a close reason logged on the dispatcher side time
                    // to land in its stderr before the panic hook prints it.
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    panic!("dispatcher RPC stream closed");
                };
                self.buffered.extend(chunk);
            }
        })
        .await
        .expect("dispatcher RPC response timeout")
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let id = self.id;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await;
        loop {
            let message = self.recv().await;
            if message["id"] == id {
                return message;
            }
            self.pending.push(message);
        }
    }

    /// Send a probe on a replaced peer and assert the stream closes without
    /// answering it.
    pub async fn assert_retired(&mut self, probe: Value) {
        let mut bytes = serde_json::to_vec(&probe).unwrap();
        bytes.push(b'\n');
        let _ = self.writer.send(&bytes).await;
        let mut remaining = Vec::new();
        timeout(Duration::from_secs(3), async {
            while let Some(chunk) = self.reader.recv().await {
                remaining.extend(chunk);
            }
        })
        .await
        .expect("replaced authenticated connection must close");
        assert!(
            !String::from_utf8_lossy(&remaining).contains("retired-probe"),
            "retired peer received a new RPC response"
        );
    }

    /// The next notification with this method, pending first.
    pub async fn event(&mut self, method: &str) -> Value {
        self.event_where(method, |_| true).await
    }

    /// The next notification with this method whose params satisfy `accept`.
    pub async fn event_where(&mut self, method: &str, accept: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self
            .pending
            .iter()
            .position(|message| message["method"] == method && accept(&message["params"]))
        {
            return self.pending.remove(index);
        }
        loop {
            let message = self.recv().await;
            if message["method"] == method && accept(&message["params"]) {
                return message;
            }
            self.pending.push(message);
        }
    }

    /// The next journaled `module.event` for `exec_id` of the given kind.
    pub async fn module_event(&mut self, exec_id: &str, kind: &str) -> Value {
        self.event_where("module.event", |params| {
            params["exec_id"] == exec_id && params["kind"] == kind
        })
        .await["params"]
            .clone()
    }

    /// Wait for the terminal `invocation.outcome` event and return its outcome.
    pub async fn wait_outcome(&mut self, exec_id: &str) -> Value {
        let event = self
            .event_where("module.event", |params| {
                params["exec_id"] == exec_id && params["terminal"] == true
            })
            .await;
        event["params"]["payload"].clone()
    }
}

/// Unwrap a successful JSON-RPC reply.
pub fn result(reply: Value) -> Value {
    assert!(reply.get("error").is_none(), "RPC error: {reply}");
    reply.get("result").expect("RPC result").clone()
}

/// Assert a JSON-RPC error with this name and return the error object.
pub fn error(reply: Value, name: &str) -> Value {
    assert_eq!(
        reply["error"]["message"], name,
        "unexpected RPC error: {reply}"
    );
    reply["error"].clone()
}

fn unused_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn launch(binary: &Path, env: &[(String, String)], root: &Path) -> Child {
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("stderr.log"))
        .unwrap();
    let mut command = Command::new(binary);
    command.arg("dispatch").env_clear();
    // macOS test binaries may depend on the Homebrew loader path. Keep the
    // child launch environment hermetic while preserving that path; the
    // preview customer process deliberately scrubs it at its own boundary.
    for key in ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[cfg(all(test, unix))]
mod readiness_tests {
    use super::*;

    fn fixture_at(port: u16) -> TestDispatcher {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("stderr.log"), []).unwrap();
        let vm = Uuid::new_v4();
        TestDispatcher {
            root,
            child: Command::new("/bin/sleep")
                .arg("30")
                .kill_on_drop(true)
                .spawn()
                .unwrap(),
            port,
            workspace: Uuid::new_v4(),
            vm,
            owner: Uuid::new_v4(),
            listener_token: new_listener_token(),
            workspace_token: workspace_token(vm),
            repo: Uuid::new_v4(),
            repos: Vec::new(),
            references: Vec::new(),
            catalog: json!({"catalog_version":"readiness-test"}),
            catalog_hash: "c".repeat(64),
            origin: None,
            binary: PathBuf::new(),
            env: Vec::new(),
            ready_rpc: Mutex::new(None),
        }
    }

    fn ready(fixture: &TestDispatcher) -> Value {
        json!({"jsonrpc":"2.0","method":"runtime.ready","params":{
            "workspace_id":fixture.workspace,
            "vm_id":fixture.vm,
            "epoch":EPOCH,
            "dispatcher_contract_version":1,
            "module_catalog_version":"readiness-test",
            "module_catalog_hash":fixture.catalog_hash,
        }})
    }

    /// A real mux/authentication exchange, with controlled runtime identity.
    fn endpoint(
        listener: TcpListener,
        expected_token: [u8; 32],
        ready: Option<Value>,
    ) -> (tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
        let authenticated = Arc::new(AtomicUsize::new(0));
        let authenticated_count = authenticated.clone();
        let task = tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    connection = listener.accept() => {
                        let (mut socket, _) = connection.unwrap();
                        let ready = ready.clone();
                        let authenticated = authenticated_count.clone();
                        clients.spawn(async move {
                            let Some(ready) = ready else {
                                std::future::pending::<()>().await;
                                return;
                            };
                            let mut token = [0; 32];
                            if socket.read_exact(&mut token).await.is_err() || token != expected_token {
                                return;
                            }
                            authenticated.fetch_add(1, Ordering::SeqCst);
                            let mut mux = Mux::new(socket, false);
                            let mut stream = mux.accept().await.unwrap();
                            let mut bytes = serde_json::to_vec(&ready).unwrap();
                            bytes.push(b'\n');
                            // Exercise read_line's leftover buffer when startup
                            // and a following notification share one mux frame.
                            bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"method\":\"test.next\"}\n");
                            stream.send(&bytes).await.unwrap();
                            while stream.recv().await.is_some() {}
                        });
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {}
                }
            }
        });
        (task, authenticated)
    }

    #[tokio::test]
    async fn readiness_rejects_plain_tcp_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut fixture = fixture_at(listener.local_addr().unwrap().port());
        let (server, _) = endpoint(listener, fixture.listener_token, None);
        let observed = timeout(Duration::from_millis(250), fixture.wait_listening()).await;
        server.abort();
        fixture.child.kill().await.unwrap();
        assert!(
            !matches!(observed, Ok(Ok(()))),
            "an unrelated TCP listener cannot prove dispatcher readiness"
        );
    }

    #[tokio::test]
    async fn readiness_rejects_authenticated_foreign_runtime_identity() {
        for field in [
            "workspace_id",
            "vm_id",
            "epoch",
            "module_catalog_hash",
            "module_catalog_version",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut fixture = fixture_at(listener.local_addr().unwrap().port());
            let mut wrong = ready(&fixture);
            wrong["params"][field] = if field == "epoch" {
                json!(EPOCH + 1)
            } else {
                json!("different-runtime")
            };
            let (server, _) = endpoint(listener, fixture.listener_token, Some(wrong));
            let observed = timeout(Duration::from_millis(250), fixture.wait_listening()).await;
            server.abort();
            fixture.child.kill().await.unwrap();
            assert!(
                !matches!(observed, Ok(Ok(()))),
                "authenticated peer with another {field} must not satisfy readiness"
            );
        }
    }

    #[tokio::test]
    async fn readiness_reuses_verified_peer_and_preserves_coalesced_next_message() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut fixture = fixture_at(listener.local_addr().unwrap().port());
        let (server, authenticated) =
            endpoint(listener, fixture.listener_token, Some(ready(&fixture)));
        timeout(Duration::from_secs(1), fixture.wait_listening())
            .await
            .unwrap()
            .unwrap();
        let mut rpc = fixture.connect().await;
        assert_eq!(rpc.ready, ready(&fixture)["params"]);
        let next = timeout(Duration::from_secs(1), rpc.recv()).await.unwrap();
        assert_eq!(next["method"], "test.next");
        assert_eq!(authenticated.load(Ordering::SeqCst), 1);
        server.abort();
        fixture.child.kill().await.unwrap();
    }

    #[tokio::test]
    async fn readiness_does_not_authenticate_to_another_fixture() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut fixture = fixture_at(listener.local_addr().unwrap().port());
        let foreign_token = new_listener_token();
        assert_ne!(foreign_token, fixture.listener_token);
        let (server, authenticated) = endpoint(listener, foreign_token, Some(ready(&fixture)));
        let observed = timeout(Duration::from_millis(250), fixture.wait_listening()).await;
        server.abort();
        fixture.child.kill().await.unwrap();
        assert!(!matches!(observed, Ok(Ok(()))));
        assert_eq!(authenticated.load(Ordering::SeqCst), 0);
    }
}

/// An authenticated smart-HTTP Git origin standing in for the internal
/// control plane: `git http-backend` behind HTTP basic auth with the
/// workspace token, one bare repository per member under `root`.
pub struct GitHttpOrigin {
    pub url: String,
    pub root: PathBuf,
    expected_auth: Arc<Mutex<(String, String)>>,
    unauthorized: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    flushes: Arc<Mutex<HashMap<String, Uuid>>>,
    codex_auth_events: Arc<Mutex<Vec<Value>>>,
    pending_session_files_unavailable: Arc<AtomicBool>,
    pending_session_file_bundle: Arc<Mutex<Option<Value>>>,
    published_session_file_bundle: Arc<Mutex<Option<Value>>>,
    session_file_requests: Arc<Mutex<Vec<String>>>,
    session_file_begin_forbidden: Arc<AtomicBool>,
    session_file_manifest_delay_ms: Arc<AtomicU64>,
    _server: tokio::task::JoinHandle<()>,
}

impl GitHttpOrigin {
    pub async fn start(
        root: PathBuf,
        token: String,
        workspace: Uuid,
        vm: Uuid,
        repo_ids: Vec<Uuid>,
    ) -> Self {
        std::fs::create_dir_all(&root).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"))
        );
        let expected_auth = Arc::new(Mutex::new((expected, format!("Bearer {token}"))));
        let unauthorized = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let flushes = Arc::new(Mutex::new(HashMap::new()));
        let codex_auth_events = Arc::new(Mutex::new(Vec::new()));
        let pending_session_files_unavailable = Arc::new(AtomicBool::new(false));
        let pending_session_file_bundle = Arc::new(Mutex::new(None));
        let published_session_file_bundle = Arc::new(Mutex::new(None));
        let session_file_requests = Arc::new(Mutex::new(Vec::new()));
        let session_file_begin_forbidden = Arc::new(AtomicBool::new(false));
        let session_file_manifest_delay_ms = Arc::new(AtomicU64::new(0));
        let server = tokio::spawn({
            let root = root.clone();
            let expected_auth = expected_auth.clone();
            let unauthorized = unauthorized.clone();
            let requests = requests.clone();
            let flushes = flushes.clone();
            let codex_auth_events = codex_auth_events.clone();
            let pending_session_files_unavailable = pending_session_files_unavailable.clone();
            let pending_session_file_bundle = pending_session_file_bundle.clone();
            let published_session_file_bundle = published_session_file_bundle.clone();
            let session_file_requests = session_file_requests.clone();
            let session_file_begin_forbidden = session_file_begin_forbidden.clone();
            let session_file_manifest_delay_ms = session_file_manifest_delay_ms.clone();
            let repo_ids = repo_ids.clone();
            async move {
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        break;
                    };
                    tokio::spawn(serve_git_connection(
                        socket,
                        root.clone(),
                        expected_auth.clone(),
                        unauthorized.clone(),
                        requests.clone(),
                        flushes.clone(),
                        workspace,
                        vm,
                        repo_ids.clone(),
                        codex_auth_events.clone(),
                        pending_session_files_unavailable.clone(),
                        pending_session_file_bundle.clone(),
                        published_session_file_bundle.clone(),
                        session_file_requests.clone(),
                        session_file_begin_forbidden.clone(),
                        session_file_manifest_delay_ms.clone(),
                    ));
                }
            }
        });
        Self {
            url: format!("http://127.0.0.1:{port}"),
            root,
            expected_auth,
            unauthorized,
            requests,
            flushes,
            codex_auth_events,
            pending_session_files_unavailable,
            pending_session_file_bundle,
            published_session_file_bundle,
            session_file_requests,
            session_file_begin_forbidden,
            session_file_manifest_delay_ms,
            _server: server,
        }
    }

    /// Fence old credentials at the test control-plane boundary after renewal.
    pub fn rotate_token(&self, token: &str) {
        *self.expected_auth.lock().unwrap() = (
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"))
            ),
            format!("Bearer {token}"),
        );
    }

    pub fn bare(&self, repo: Uuid) -> PathBuf {
        self.root.join(format!("{repo}.git"))
    }

    /// Requests refused for a missing or wrong credential.
    pub fn unauthorized(&self) -> usize {
        self.unauthorized.load(Ordering::SeqCst)
    }

    /// Requests served by `git http-backend`.
    pub fn served(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    pub fn flush_count(&self) -> usize {
        self.flushes.lock().unwrap().len()
    }

    /// Authenticated device-login reports received by the fixture control plane.
    pub fn codex_auth_events(&self) -> Vec<Value> {
        self.codex_auth_events.lock().unwrap().clone()
    }

    pub fn set_pending_session_files_unavailable(&self, unavailable: bool) {
        self.pending_session_files_unavailable
            .store(unavailable, Ordering::SeqCst);
    }

    pub fn set_pending_session_file_bundle(&self, bundle: Value) {
        *self.pending_session_file_bundle.lock().unwrap() = Some(bundle);
    }

    pub fn session_file_requests(&self) -> Vec<String> {
        self.session_file_requests.lock().unwrap().clone()
    }

    pub fn published_session_file_bundle(&self) -> Option<Value> {
        self.published_session_file_bundle.lock().unwrap().clone()
    }

    pub fn set_published_session_file_bundle(&self, bundle: Value) {
        *self.published_session_file_bundle.lock().unwrap() = Some(bundle);
    }

    pub fn set_session_file_begin_forbidden(&self, forbidden: bool) {
        self.session_file_begin_forbidden
            .store(forbidden, Ordering::SeqCst);
    }

    /// Hold every session-file manifest response this long (stalls harvest).
    pub fn set_session_file_manifest_delay(&self, delay: std::time::Duration) {
        self.session_file_manifest_delay_ms
            .store(delay.as_millis() as u64, Ordering::SeqCst);
    }
}

#[allow(clippy::too_many_arguments)]
async fn serve_git_connection(
    mut socket: TcpStream,
    root: PathBuf,
    expected_credentials: Arc<Mutex<(String, String)>>,
    unauthorized: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    flushes: Arc<Mutex<HashMap<String, Uuid>>>,
    workspace: Uuid,
    vm: Uuid,
    repo_ids: Vec<Uuid>,
    codex_auth_events: Arc<Mutex<Vec<Value>>>,
    pending_session_files_unavailable: Arc<AtomicBool>,
    pending_session_file_bundle: Arc<Mutex<Option<Value>>>,
    published_session_file_bundle: Arc<Mutex<Option<Value>>>,
    session_file_requests: Arc<Mutex<Vec<String>>>,
    session_file_begin_forbidden: Arc<AtomicBool>,
    session_file_manifest_delay_ms: Arc<AtomicU64>,
) {
    let mut pending: Vec<u8> = Vec::new();
    loop {
        // Request head.
        let head_end = loop {
            if let Some(index) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                break index;
            }
            let mut chunk = [0u8; 8192];
            match socket.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(read) => pending.extend_from_slice(&chunk[..read]),
            }
        };
        let head = String::from_utf8_lossy(&pending[..head_end]).into_owned();
        pending.drain(..head_end + 4);
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default().to_owned();
        let mut headers: HashMap<String, String> = HashMap::new();
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let target = parts.next().unwrap_or_default().to_owned();
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path.to_owned(), query.to_owned()),
            None => (target.clone(), String::new()),
        };
        // Body: Content-Length or chunked.
        let mut body = Vec::new();
        if let Some(length) = headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok())
        {
            while pending.len() < length {
                let mut chunk = [0u8; 65536];
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => pending.extend_from_slice(&chunk[..read]),
                }
            }
            body = pending.drain(..length).collect();
        } else if headers
            .get("transfer-encoding")
            .is_some_and(|v| v.contains("chunked"))
        {
            loop {
                let line_end = loop {
                    if let Some(index) = pending.windows(2).position(|w| w == b"\r\n") {
                        break index;
                    }
                    let mut chunk = [0u8; 65536];
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => pending.extend_from_slice(&chunk[..read]),
                    }
                };
                let size_line = String::from_utf8_lossy(&pending[..line_end]).into_owned();
                pending.drain(..line_end + 2);
                let size =
                    usize::from_str_radix(size_line.split(';').next().unwrap_or("0").trim(), 16)
                        .unwrap_or(0);
                while pending.len() < size + 2 {
                    let mut chunk = [0u8; 65536];
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => pending.extend_from_slice(&chunk[..read]),
                    }
                }
                body.extend(pending.drain(..size));
                pending.drain(..2);
                if size == 0 {
                    break;
                }
            }
        }
        let is_flush = path.starts_with("/api/internal/workspace-incarnations/");
        let is_codex_auth = path.starts_with("/api/internal/chat-sessions/")
            && path.ends_with("/codex-auth")
            && method == "POST";
        let is_session_file_begin = path.starts_with("/api/internal/chat-sessions/")
            && path.ends_with("/session-files/begin")
            && method == "POST";
        let is_pending_session_files = path.starts_with("/api/internal/workspaces/")
            && path.ends_with("/session-files/pending")
            && method == "GET";
        let is_session_files =
            path.starts_with("/api/internal/chat-sessions/") && path.contains("/session-files");
        let is_workspace_agent = path.starts_with("/api/internal/workspace-agent/");
        let (expected_auth, expected_bearer) = expected_credentials.lock().unwrap().clone();
        let expected_for_route = if is_flush
            || is_codex_auth
            || is_pending_session_files
            || is_session_files
            || is_workspace_agent
        {
            &expected_bearer
        } else {
            &expected_auth
        };
        let (status, response_headers, response_body) = if headers.get("authorization")
            != Some(expected_for_route)
        {
            unauthorized.fetch_add(1, Ordering::SeqCst);
            (
                "401 Unauthorized".to_owned(),
                vec![(
                    "WWW-Authenticate".to_owned(),
                    "Basic realm=\"g5\"".to_owned(),
                )],
                b"authentication required".to_vec(),
            )
        } else if is_workspace_agent {
            // No chat-task store here: a turn's ontology start is refused, so
            // the turn runs without a changes card (`g5.chat.ontology-card`).
            (
                "404 Not Found".to_owned(),
                vec![("Content-Type".to_owned(), "application/json".to_owned())],
                br#"{"error":{"code":"NOT_FOUND","message":"no chat task"}}"#.to_vec(),
            )
        } else if is_codex_auth {
            let report: Value = serde_json::from_slice(&body).unwrap();
            codex_auth_events.lock().unwrap().push(report);
            (
                "200 OK".to_owned(),
                vec![("Content-Type".to_owned(), "application/json".to_owned())],
                br#"{"proceed":true}"#.to_vec(),
            )
        } else if is_pending_session_files
            && pending_session_files_unavailable.load(Ordering::SeqCst)
        {
            (
                "503 Service Unavailable".to_owned(),
                vec![],
                b"unavailable".to_vec(),
            )
        } else if is_session_file_begin && session_file_begin_forbidden.load(Ordering::SeqCst) {
            (
                "403 Forbidden".to_owned(),
                vec![("Content-Type".to_owned(), "application/json".to_owned())],
                br#"{"error":{"code":"FORBIDDEN","message":"workspace lease expired"}}"#.to_vec(),
            )
        } else if is_session_file_begin || is_pending_session_files || is_session_files {
            // This dispatcher fixture has no durable session store. Admit the
            // fence and optionally expose one ended bundle for flush tests.
            requests.fetch_add(1, Ordering::SeqCst);
            session_file_requests
                .lock()
                .unwrap()
                .push(format!("{method} {target}"));
            let response = if is_pending_session_files {
                pending_session_file_bundle
                    .lock()
                    .unwrap()
                    .clone()
                    .map_or_else(|| json!([]), |bundle| json!([bundle]))
            } else if path.ends_with("/manifest") {
                let delay = session_file_manifest_delay_ms.load(Ordering::SeqCst);
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                json!({"files": []})
            } else if path.ends_with("/published") {
                published_session_file_bundle
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or(Value::Null)
            } else if path.ends_with("/publish") {
                let publication: Value = serde_json::from_slice(&body).unwrap();
                *published_session_file_bundle.lock().unwrap() = Some(publication);
                json!({})
            } else if method == "PUT" {
                let bundle = pending_session_file_bundle.lock().unwrap();
                let provider_session_id = bundle
                    .as_ref()
                    .and_then(|value| value["providerSessionId"].as_str())
                    .unwrap_or_default();
                let provider = bundle
                    .as_ref()
                    .and_then(|value| value["provider"].as_str())
                    .unwrap_or("codex");
                json!({
                    "provider": provider, "providerSessionId": provider_session_id,
                    "relativePath": "fixture-rollout", "sha256": "0".repeat(64),
                    "size": 1, "changed": true, "updatedAt": "2026-10-03T00:00:00Z"
                })
            } else {
                json!({})
            };
            (
                "200 OK".to_owned(),
                vec![("Content-Type".to_owned(), "application/json".to_owned())],
                serde_json::to_vec(&response).unwrap(),
            )
        } else if is_flush {
            requests.fetch_add(1, Ordering::SeqCst);
            if path.ends_with("/flushes/start") && method == "POST" {
                session_file_requests
                    .lock()
                    .unwrap()
                    .push(format!("{method} {target}"));
            }
            let parsed: Value = serde_json::from_slice(&body).unwrap_or_default();
            let response = if path.ends_with("/flushes/start") && method == "POST" {
                let key = parsed["idempotency_key"].as_str().unwrap_or_default();
                let flush_id = *flushes
                    .lock()
                    .unwrap()
                    .entry(key.to_owned())
                    .or_insert_with(Uuid::new_v4);
                json!({
                    "flush_id": flush_id,
                    "workspace_id": workspace,
                    "vm_id": vm,
                    "epoch": EPOCH,
                    "reason": "shutdown",
                    "required_repo_ids": repo_ids,
                    "started_at": "2026-09-20T00:00:00Z"
                })
            } else if path.ends_with("/complete") && method == "POST" {
                let flush_id = path
                    .split('/')
                    .rev()
                    .nth(1)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .unwrap();
                json!({
                    "flush_id": flush_id,
                    "complete_at": "2026-09-20T00:00:01Z",
                    "repos": parsed["repos"]
                })
            } else if path.ends_with("/fail") && method == "POST" {
                let flush_id = path
                    .split('/')
                    .rev()
                    .nth(1)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .unwrap();
                json!({"flush_id":flush_id,"blocked_since":"2026-09-20T00:00:01Z"})
            } else {
                json!({"error":{"code":"NOT_FOUND"}})
            };
            (
                "200 OK".to_owned(),
                vec![("Content-Type".to_owned(), "application/json".to_owned())],
                serde_json::to_vec(&response).unwrap(),
            )
        } else {
            requests.fetch_add(1, Ordering::SeqCst);
            // /git/<repo_id>/<rest> -> <root>/<repo_id>.git/<rest>
            let path_info = match path
                .strip_prefix("/git/")
                .and_then(|rest| rest.split_once('/'))
            {
                Some((repo, rest)) => format!("/{repo}.git/{rest}"),
                None => "/missing".to_owned(),
            };
            let mut backend = std::process::Command::new("git");
            backend
                .arg("http-backend")
                .env_clear()
                .env("PATH", std::env::var("PATH").unwrap_or_default())
                .env("GIT_PROJECT_ROOT", &root)
                .env("GIT_HTTP_EXPORT_ALL", "1")
                .env("REQUEST_METHOD", &method)
                .env("PATH_INFO", &path_info)
                .env("QUERY_STRING", &query)
                .env("REMOTE_ADDR", "127.0.0.1")
                .env("REMOTE_USER", "x-access-token")
                .env("CONTENT_LENGTH", body.len().to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            if let Some(value) = headers.get("content-type") {
                backend.env("CONTENT_TYPE", value);
            }
            if let Some(value) = headers.get("content-encoding") {
                backend.env("HTTP_CONTENT_ENCODING", value);
            }
            if let Some(value) = headers.get("git-protocol") {
                backend.env("GIT_PROTOCOL", value);
            }
            let output = tokio::task::spawn_blocking(move || {
                use std::io::Write;
                let mut child = backend.spawn().unwrap();
                child.stdin.take().unwrap().write_all(&body).unwrap();
                child.wait_with_output().unwrap()
            })
            .await
            .unwrap();
            let raw = output.stdout;
            let split = raw
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|i| (i, 4))
                .or_else(|| raw.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2)))
                .unwrap_or((raw.len(), 0));
            let cgi_head = String::from_utf8_lossy(&raw[..split.0]).into_owned();
            let cgi_body = raw[(split.0 + split.1).min(raw.len())..].to_vec();
            let mut status = "200 OK".to_owned();
            let mut response_headers = Vec::new();
            for line in cgi_head.lines() {
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("status") {
                        status = value.trim().to_owned();
                    } else {
                        response_headers.push((name.trim().to_owned(), value.trim().to_owned()));
                    }
                }
            }
            (status, response_headers, cgi_body)
        };
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n",
            response_body.len()
        );
        for (name, value) in &response_headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        if socket.write_all(response.as_bytes()).await.is_err()
            || socket.write_all(&response_body).await.is_err()
        {
            return;
        }
        if headers
            .get("connection")
            .is_some_and(|v| v.eq_ignore_ascii_case("close"))
        {
            return;
        }
    }
}
