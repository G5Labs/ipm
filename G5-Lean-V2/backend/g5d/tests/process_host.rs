//! Process-mode host boot acceptance (`g5.runtime.boot.backend.process-local`).
//!
//! The real `ProcessVmLauncher` boots the real `g5d dispatch` as a host
//! process group through the workspace runtime, reaches `runtime.ready` over
//! the host loopback, serves a preview whose command binds `PORT`, runs the
//! profile cleanup on Close, and a lifecycle stop kills the group without
//! cleanup. Workspace materialization needs a reflink-capable filesystem
//! (`TMPDIR` on XFS or APFS); elsewhere the test skips loudly.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use g5_internal_api::host_ctl::{CommandResult, HostVmState, StopMode, VmStatus, VmStop};
use g5_internal_api::stream_mux::{Channel, MuxReader, MuxWriter};
use g5d::dispatch_test_support::{now_ms, workspace_token, GitHttpOrigin, PRODUCTION_CATALOG};
use g5d_host::ctl::LeaseRenewalBroker;
use g5d_host::runtime::ProcessVmLauncher;
use g5d_host::test_support::{git, materializer_available, HostFixture, FIXTURE_BRANCH};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

fn bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_g5d"))
}

/// A preview manifest whose command runs the test module's tiny HTTP server
/// on `$PORT`, with absolute marker files for its pid, each served request
/// and the profile cleanup.
fn unused_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn preview_manifest(
    port: u16,
    pid_file: &Path,
    request_file: &Path,
    cleanup_file: &Path,
) -> String {
    preview_manifest_with_prelude(port, pid_file, request_file, cleanup_file, "")
}

/// Like [`preview_manifest`], with a shell prelude the profile's command runs
/// before the server, so a test can observe the preview's environment.
fn preview_manifest_with_prelude(
    port: u16,
    pid_file: &Path,
    request_file: &Path,
    cleanup_file: &Path,
    prelude: &str,
) -> String {
    let loader_prefix = ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"]
        .into_iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| format!("{key}='{}'", value.replace('\'', "'\"'\"'")))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let command = format!(
        "{prelude}{loader_prefix} '{}' dispatch-fixture-module",
        bin().display()
    );
    let cleanup_command = format!(
        "printf cleanup > '{}'",
        cleanup_file.to_string_lossy().replace('\'', "'\"'\"'")
    );
    format!(
        "command: {}\nport: {port}\ncleanup_command: {}\nenv:\n  G5_TEST_PREVIEW_SERVER: \"1\"\n  G5_TEST_PREVIEW_PID_FILE: {}\n  G5_TEST_PREVIEW_WRITE_FILE: {}\n",
        serde_json::to_string(&command).unwrap(),
        serde_json::to_string(&cleanup_command).unwrap(),
        serde_json::to_string(&pid_file.to_string_lossy()).unwrap(),
        serde_json::to_string(&request_file.to_string_lossy()).unwrap(),
    )
}

struct Rpc {
    writer: MuxWriter,
    reader: MuxReader,
    id: u64,
}

impl Rpc {
    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let id = self.id;
        let mut bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .unwrap();
        bytes.push(b'\n');
        timeout(Duration::from_secs(20), self.writer.send(&bytes))
            .await
            .unwrap()
            .unwrap();
        loop {
            let message = self.recv().await;
            if message["id"] == id {
                assert!(message.get("error").is_none(), "RPC error: {message}");
                return message["result"].clone();
            }
        }
    }

    async fn recv(&mut self) -> Value {
        timeout(Duration::from_secs(60), async {
            let line = self
                .reader
                .read_line_with_limit(4 * 1024 * 1024)
                .await
                .expect("dispatcher RPC stream closed");
            serde_json::from_str::<Value>(&line).unwrap()
        })
        .await
        .expect("dispatcher RPC response timeout")
    }

    /// Journaled events for `exec_id`, by replay: only the host's control
    /// stream receives pushed module events, so an owner stream polls the
    /// journal exactly as the control plane does.
    async fn wait_event(
        &mut self,
        workspace: Uuid,
        vm: Uuid,
        exec_id: &str,
        accept: impl Fn(&Value) -> bool,
    ) -> Value {
        timeout(Duration::from_secs(60), async {
            loop {
                let replay = self
                    .call(
                        "events.replay",
                        json!({
                            "exec_id": exec_id,
                            "from_seq": 1,
                            "context": {
                                "workspace_id": workspace,
                                "expected_vm_id": vm,
                                "expected_epoch": 1,
                                "deadline_ms": now_ms() + 120_000,
                                "idempotency_key": Uuid::new_v4(),
                                "lifecycle": true,
                            },
                        }),
                    )
                    .await;
                if let Some(event) = replay["events"]
                    .as_array()
                    .and_then(|events| events.iter().find(|event| accept(event)))
                {
                    return event.clone();
                }
                sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("preview event did not arrive")
    }

    async fn ready_or_terminal(&mut self, workspace: Uuid, vm: Uuid, exec_id: &str) -> Value {
        self.wait_event(workspace, vm, exec_id, |event| {
            event["kind"] == "module.preview.ready" || event["terminal"] == true
        })
        .await
    }

    async fn outcome(&mut self, workspace: Uuid, vm: Uuid, exec_id: &str) -> Value {
        self.wait_event(workspace, vm, exec_id, |event| event["terminal"] == true)
            .await["payload"]
            .clone()
    }
}

async fn wait_for_pid(path: &Path) -> i32 {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                break pid;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("preview fixture did not publish its pid")
}

fn process_exists(pid: i32) -> bool {
    // SAFETY: signal zero only probes process existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

async fn wait_for_exit(pid: i32, what: &str) {
    timeout(Duration::from_secs(15), async {
        while process_exists(pid) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} (pid {pid}) is still running"));
}

fn preview_invoke(
    workspace: Uuid,
    vm: Uuid,
    owner: Uuid,
    exec_id: &str,
    catalog: &Value,
    input: Value,
) -> Value {
    let module = catalog["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|module| module["module_id"] == "workspace.preview")
        .expect("shipped catalog has the preview module");
    json!({
        "exec_id": exec_id,
        "module_id": "workspace.preview",
        "operation": "start",
        "input": input,
        "grant": {"grant_id": Uuid::new_v4(), "capabilities": module["requested_capabilities"]},
        "context": {
            "workspace_id": workspace,
            "expected_vm_id": vm,
            "expected_epoch": 1,
            "deadline_ms": now_ms() + 120_000,
            "idempotency_key": Uuid::new_v4(),
            "actor_user_id": owner,
        },
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial(dispatcher_process)]
async fn process_host_boots_a_real_dispatcher_and_serves_a_preview() {
    let temp = tempfile::tempdir().unwrap();
    // Review R2: a developer checkout and host state have no length bound; the
    // dispatcher's Git socket must still bind. Push every host path well past
    // the Unix socket path limit.
    let long =
        "a-very-long-checkout-directory-name-that-pushes-every-host-path-past-the-socket-bound";
    let root = temp.path().join(long).join(long);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    assert!(root.as_os_str().len() > 110, "{}", root.display());
    let mut fixture = HostFixture::new(&root);
    if !materializer_available(&fixture).await {
        eprintln!("SKIP: process host boot needs a reflink-capable TMPDIR (XFS/APFS)");
        return;
    }

    // Commit the preview manifests the host will materialize: a `preview:`
    // block for the Close case and a named profile for the lifecycle-stop
    // case, each with its own marker files.
    let markers = root.join("markers");
    std::fs::create_dir_all(&markers).unwrap();
    let close_pid = markers.join("close.pid");
    let close_request = markers.join("close.request");
    let close_cleanup = markers.join("close.cleanup");
    let stop_pid = markers.join("stop.pid");
    let stop_request = markers.join("stop.request");
    let stop_cleanup = markers.join("stop.cleanup");
    let close_port = unused_tcp_port();
    let stop_port = unused_tcp_port();
    // Review R5: the host's Docker connection selection reaches the preview
    // through both environment boundaries (host → dispatcher → module).
    let docker_config = root.join("host-home/.docker");
    std::fs::create_dir_all(&docker_config).unwrap();
    let previous_docker = (
        std::env::var_os("DOCKER_CONFIG"),
        std::env::var_os("DOCKER_HOST"),
        std::env::var_os("DOCKER_CONTEXT"),
    );
    std::env::set_var("DOCKER_CONFIG", &docker_config);
    std::env::set_var("DOCKER_HOST", "unix:///tmp/review-host-docker.sock");
    std::env::remove_var("DOCKER_CONTEXT");
    let docker_seen = markers.join("docker.env");
    let prelude = format!(
        "printf '%s\\n%s\\n%s\\n' \"$DOCKER_CONFIG\" \"$DOCKER_HOST\" \"$G5D_PROCESS_INCARNATION\" > '{}'; ",
        docker_seen.to_string_lossy()
    );
    let source = root.join("source");
    std::fs::create_dir_all(source.join(".g5/preview_profiles")).unwrap();
    std::fs::write(
        source.join(".g5/workspace.yaml"),
        format!(
            "preview:\n{}",
            preview_manifest_with_prelude(
                close_port,
                &close_pid,
                &close_request,
                &close_cleanup,
                &prelude
            )
            .lines()
            .map(|line| format!("  {line}\n"))
            .collect::<String>()
        ),
    )
    .unwrap();
    std::fs::write(
        source.join(".g5/preview_profiles/stop.yaml"),
        preview_manifest(stop_port, &stop_pid, &stop_request, &stop_cleanup),
    )
    .unwrap();
    git(&source, &["add", "-A"]);
    git(&source, &["commit", "-q", "-m", "preview manifests"]);
    git(&source, &["push", "-q", "origin", FIXTURE_BRANCH]);
    fixture.branch_oid = git(&source, &["rev-parse", "HEAD"]);

    // The host-native runtime directory: the shipped catalog, pinned exactly
    // as the host does, and the harness; the dispatcher's `g5d` entrypoints
    // run the host binary itself.
    let catalog: Value = serde_json::from_str(PRODUCTION_CATALOG).unwrap();
    let catalog_bytes = serde_json::to_vec_pretty(&catalog).unwrap();
    std::fs::write(
        fixture.runtime_root.join("modules/catalog.json"),
        &catalog_bytes,
    )
    .unwrap();
    fixture.catalog_hash = format!("{:x}", Sha256::digest(&catalog_bytes));
    fixture.catalog_version = catalog["catalog_version"].as_str().unwrap().to_string();
    std::fs::write(
        fixture.runtime_root.join("modules/catalog.sha256"),
        format!("{}\n", fixture.catalog_hash),
    )
    .unwrap();
    std::fs::create_dir_all(fixture.runtime_root.join("assets/harness")).unwrap();
    std::fs::write(
        fixture.runtime_root.join("assets/harness/AGENTS.md"),
        b"# test harness\n",
    )
    .unwrap();

    // The internal Git origin the incarnation's member URL must name; the
    // host materializes from it with the incarnation token.
    let workspace = Uuid::new_v4();
    let vm = Uuid::new_v4();
    let token = workspace_token(vm);
    let origin = GitHttpOrigin::start(
        root.join("origin-http"),
        token.clone(),
        workspace,
        vm,
        vec![fixture.repo_id],
    )
    .await;
    let bare = origin.bare(fixture.repo_id);
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare"]);
    git(
        &fixture.bare_repo,
        &["push", "-q", "--mirror", bare.to_str().unwrap()],
    );

    // Review R20: the first dispatcher attempt never becomes ready; the host
    // proves its exit, restores the checkout and starts the same incarnation
    // again, whose dispatcher is the real one. The wrapper records every
    // launch and the first attempt's pid.
    let launches = markers.join("launches");
    let first_attempt_pid = markers.join("first-attempt.pid");
    let wrapper = root.join("dispatcher-wrapper.sh");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho \"$G5D_PROCESS_INCARNATION\" >> '{launches}'\nif [ \"$(wc -l < '{launches}')\" -eq 1 ]; then echo $$ > '{pid}'; exec sleep 600; fi\nexec '{g5d}' \"$@\"\n",
            launches = launches.to_string_lossy(),
            pid = first_attempt_pid.to_string_lossy(),
            g5d = bin().to_string_lossy()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = fixture.config(1);
    config.api_url = origin.url.clone();
    config.dispatcher_ready_timeout = Duration::from_secs(20);
    config.process_host_executable = Some(wrapper.clone());
    let launcher = Arc::new(ProcessVmLauncher::new(root.join("host-state")));
    let journal = fixture.journal().await;
    let runtime = fixture.runtime_from_config(
        config,
        launcher.clone(),
        journal,
        Arc::new(LeaseRenewalBroker::default()),
    );

    let mut start = fixture.start(workspace, vm, 1);
    start.spec.repos[0].git_url = format!("{}/git/{}", origin.url, fixture.repo_id);
    // The API stamps the owner's Git identity and the module identity into
    // every spec; process mode must drop the identity and keep the rest.
    start
        .spec
        .env
        .insert("G5D_OWNER_GIT_NAME".into(), "Process Host Test".into());
    start
        .spec
        .env
        .insert("G5D_OWNER_GIT_EMAIL".into(), "process@example.test".into());
    start
        .spec
        .env
        .insert("G5D_MODULE_UID".into(), "1000".into());
    start
        .spec
        .env
        .insert("G5D_MODULE_GID".into(), "1000".into());
    start.workspace_token = token.clone();
    let started = std::time::Instant::now();
    let result = runtime.start(start).await;
    let dispatch_log =
        || std::fs::read_to_string(launcher.vm_dir(vm).join("dispatch.log")).unwrap_or_default();
    assert!(
        matches!(result, CommandResult::Started),
        "{result:?}\n--- dispatch.log ---\n{}",
        dispatch_log()
    );
    eprintln!(
        "process-mode dispatcher ready after {:?}",
        started.elapsed()
    );
    assert_eq!(runtime.active_count().await, 1);
    // Review R20: two launches of the same incarnation, the unready first
    // attempt dead and retired with its exit proof, the live attempt recorded
    // without one.
    let launched = std::fs::read_to_string(&launches).unwrap();
    assert_eq!(launched.lines().count(), 2, "{launched}");
    assert!(
        launched.lines().all(|line| line == vm.to_string()),
        "{launched}"
    );
    let first_pid: i32 = std::fs::read_to_string(&first_attempt_pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        !process_exists(first_pid),
        "the unready first attempt is gone"
    );
    let registry = root.join("host-state/process-vms");
    let retired: Vec<_> = std::fs::read_dir(&registry)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".retired-"))
        })
        .collect();
    assert_eq!(retired.len(), 1, "{retired:?}");
    assert!(retired[0].join("settled").exists());
    assert!(!launcher.vm_dir(vm).join("settled").exists());
    let record: Value = serde_json::from_slice(
        &std::fs::read(launcher.vm_dir(vm).join("incarnation.json")).unwrap(),
    )
    .unwrap();
    assert!(record["leader_pid"].as_u64().is_some());
    let workspace_root = fixture.workspaces_root.join(workspace.to_string());
    assert!(
        workspace_root.join("app/.g5/workspace.yaml").is_file(),
        "the checkout is the dispatcher's workspace root"
    );
    assert!(
        !workspace_root.join(".g5-process-home").exists(),
        "no scratch home inside the checkout"
    );
    let vm_dir = launcher.vm_dir(vm);
    assert!(vm_dir.join("home").is_dir() && vm_dir.join("state").is_dir());
    assert!(
        vm_dir.join("state/workspace-token").is_file(),
        "the dispatcher's state lives beside the host state"
    );

    // Owner RPC through the host's authenticated mux, like the API relay.
    let endpoint = runtime
        .endpoint(vm)
        .await
        .expect("ready VM has an endpoint");
    let (stream, _guard) = endpoint.open(Channel::Rpc).await.unwrap();
    let (writer, reader) = stream.into_split();
    let mut rpc = Rpc {
        writer,
        reader,
        id: 0,
    };

    // Preview through the `preview:` block: ready on a host loopback port the
    // profile's `PORT` named, served through the dispatcher's HTTP bridge.
    let close_exec = format!("task.{}.1.{vm}", Uuid::new_v4());
    let disposition = rpc
        .call(
            "modules.invoke",
            preview_invoke(
                workspace,
                vm,
                fixture.owner_user_id,
                &close_exec,
                &catalog,
                json!({"repo_id": fixture.repo_id}),
            ),
        )
        .await;
    assert_eq!(disposition["disposition"], "started", "{disposition}");
    let ready = rpc.ready_or_terminal(workspace, vm, &close_exec).await;
    assert_eq!(
        ready["kind"],
        "module.preview.ready",
        "{ready}\n--- dispatch.log ---\n{}",
        dispatch_log()
    );
    let port = close_port;
    let pid = wait_for_pid(&close_pid).await;
    for (key, value) in [
        ("DOCKER_CONFIG", previous_docker.0.clone()),
        ("DOCKER_HOST", previous_docker.1.clone()),
        ("DOCKER_CONTEXT", previous_docker.2.clone()),
    ] {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
    let seen = std::fs::read_to_string(&docker_seen).unwrap();
    let mut seen = seen.lines();
    assert_eq!(seen.next(), Some(docker_config.to_string_lossy().as_ref()));
    assert_eq!(seen.next(), Some("unix:///tmp/review-host-docker.sock"));
    assert_eq!(
        seen.next(),
        Some(vm.to_string().as_str()),
        "the preview carries the incarnation marker"
    );
    assert!(
        TcpStream::connect(("127.0.0.1", port)).await.is_ok(),
        "the preview listens on the host loopback"
    );
    let (http, _http_guard) = endpoint.open(Channel::Http).await.unwrap();
    let (http_writer, mut http_reader) = http.into_split();
    let mut request = port.to_be_bytes().to_vec();
    request.extend_from_slice(b"GET / HTTP/1.1\r\nHost: preview\r\nConnection: close\r\n\r\n");
    http_writer.send(&request).await.unwrap();
    let response = timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        while let Some(chunk) = http_reader.recv().await {
            bytes.extend(chunk);
            if String::from_utf8_lossy(&bytes).contains("preview-fixture") {
                break;
            }
        }
        bytes
    })
    .await
    .expect("preview HTTP bridge response timeout");
    assert!(String::from_utf8_lossy(&response).contains("preview-fixture"));
    assert_eq!(std::fs::read(&close_request).unwrap(), b"preview-request\n");

    // Close runs the profile's cleanup with the same shell and environment.
    let cancelled = rpc
        .call(
            "modules.cancel",
            json!({
                "exec_id": close_exec,
                "reason": "owner_cancel",
                "context": {
                    "workspace_id": workspace,
                    "expected_vm_id": vm,
                    "expected_epoch": 1,
                    "deadline_ms": now_ms() + 120_000,
                    "idempotency_key": Uuid::new_v4(),
                    "lifecycle": true,
                },
            }),
        )
        .await;
    assert_eq!(cancelled["disposition"], "signalling", "{cancelled}");
    let outcome = rpc.outcome(workspace, vm, &close_exec).await;
    assert_eq!(outcome["status"], "cancelled", "{outcome}");
    assert_eq!(outcome["detail"]["close"], "clean", "{outcome}");
    wait_for_exit(pid, "closed preview").await;
    assert_eq!(std::fs::read(&close_cleanup).unwrap(), b"cleanup");

    // A second preview from a named profile, then a lifecycle stop: the
    // whole process group dies without running cleanup, the host proves the
    // exit and releases the slot and root.
    let stop_exec = format!("task.{}.1.{vm}", Uuid::new_v4());
    let disposition = rpc
        .call(
            "modules.invoke",
            preview_invoke(
                workspace,
                vm,
                fixture.owner_user_id,
                &stop_exec,
                &catalog,
                json!({"repo_id": fixture.repo_id, "profile": "stop"}),
            ),
        )
        .await;
    assert_eq!(disposition["disposition"], "started", "{disposition}");
    let ready = rpc.ready_or_terminal(workspace, vm, &stop_exec).await;
    assert_eq!(ready["kind"], "module.preview.ready", "{ready}");
    let stop_preview_pid = wait_for_pid(&stop_pid).await;
    drop(rpc);
    drop(_guard);
    let stopped = runtime
        .stop(VmStop {
            workspace_id: workspace,
            vm_id: vm,
            mode: StopMode::Graceful,
            grace_secs: 20,
        })
        .await;
    assert!(
        matches!(stopped, CommandResult::Stopped { .. }),
        "{stopped:?}\n--- dispatch.log ---\n{}",
        dispatch_log()
    );
    wait_for_exit(stop_preview_pid, "preview of a stopped workspace").await;
    assert!(
        !stop_cleanup.exists(),
        "a lifecycle stop tears the preview down without running its cleanup"
    );
    let status = runtime
        .status(VmStatus {
            workspace_id: workspace,
            vm_id: vm,
        })
        .await;
    match status {
        CommandResult::Status { vm } => assert_eq!(vm.state, HostVmState::Exited, "{vm:?}"),
        CommandResult::NotFound => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(runtime.active_count().await, 0);
    timeout(Duration::from_secs(15), async {
        while workspace_root.exists() {
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("a released workspace root is deleted");
    assert!(
        vm_dir.join("dispatch.log").is_file(),
        "the incarnation's log stays with the host state"
    );
    let _ = PathBuf::from(&root);
}
