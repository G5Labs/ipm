//! P4.2b module-runtime acceptance at the production boundary: the real
//! `g5d dispatch` child, the real mux/JSON-RPC protocol, the shipped catalog,
//! the real `g5d dispatch-fixture-module` and `g5d bootstrap` processes, and an
//! authenticated smart-HTTP Git origin behind the token-isolated transport.
//! No live API, provider or VM; nothing here is an isolation claim.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use g5_internal_api::stream_mux::Channel;
use g5d::dispatch_test_support::{
    error, git, now_ms, response_digest, result, workspace_token_with_expiry, DispatcherOptions,
    RepoSpec, TestDispatcher, FIXTURE_INPUT_SCHEMA, FIXTURE_MAX_LOG_BYTES,
    FIXTURE_MAX_OUTPUT_BYTES, FIXTURE_MODULE_ID, FIXTURE_WALL_CLOCK_MS,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::time::{sleep, timeout};
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[cfg(unix)]
#[path = "dispatch_modules/codex_app_server.rs"]
mod codex_app_server;
#[cfg(unix)]
#[path = "dispatch_modules/formal_review.rs"]
mod formal_review;
#[cfg(unix)]
#[path = "dispatch_modules/provider_catalog.rs"]
mod provider_catalog;
#[cfg(unix)]
#[path = "dispatch_modules/steering.rs"]
mod steering;

fn bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_g5d"))
}

async fn fixture_dispatcher() -> TestDispatcher {
    TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            env: inherited_loader_env(),
            ..DispatcherOptions::default()
        },
    )
    .await
}

fn fixture_invoke(d: &TestDispatcher, exec_id: &str, input: Value) -> Value {
    d.params(
        false,
        json!({
            "exec_id": exec_id,
            "module_id": FIXTURE_MODULE_ID,
            "operation": "run",
            "input_schema": FIXTURE_INPUT_SCHEMA,
            "input": input,
            "grant": d.grant(&["workspace.read"]),
        }),
    )
}

fn builtin_invoke(
    d: &TestDispatcher,
    exec_id: &str,
    module_id: &str,
    operation: &str,
    input: Value,
) -> Value {
    let entry = d.module_entry(module_id);
    let capabilities: Vec<&str> = entry["requested_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    d.params(
        false,
        json!({
            "exec_id": exec_id,
            "module_id": module_id,
            "operation": operation,
            "input": input,
            "grant": d.grant(&capabilities),
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn agent_turn_invoke(
    d: &TestDispatcher,
    task: Uuid,
    attempt: u32,
    provider_id: &str,
    input_origin: &str,
    prompt: &str,
    policy: Value,
    provider_path_key: &str,
    provider_path: &Path,
) -> Value {
    let entry = d.module_entry("agent.turn");
    let capabilities: Vec<&str> = entry["requested_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|capability| capability.as_str().unwrap())
        .collect();
    let input_id = Uuid::new_v4();
    let reservation_id = Uuid::new_v4();
    // Codex attempts always freeze App Server (`g5.chat.codex-exec-retirement`).
    let codex = provider_id == "codex";
    let mut options = json!({"fake_provider": true});
    if codex {
        options["codex_transport"] = json!("app_server_stdio_v1");
    }
    d.params(
        false,
        json!({
            "exec_id": d.task_exec_id(task, attempt),
            "module_id": "agent.turn",
            "version": if codex { json!("1.0.8") } else { entry["module_version"].clone() },
            "operation": "run",
            "input_schema": "g5.agent.turn.input/v1",
            "input": {
                "schema_version": "g5.agent.turn.input/v1",
                "task": {
                    "task_id": task,
                    "attempt": attempt,
                    "session_id": Uuid::new_v4(),
                    "input_origin": input_origin,
                    "initial_input": {
                        "reservation_id": reservation_id,
                        "input_id": input_id,
                        "revision": 1,
                        "delivery_generation": 1,
                        "input_origin": input_origin,
                        "system_notice_ids": [],
                    },
                },
                "provider": {
                    "provider_id": provider_id,
                    "model_id": "fixture-model",
                    "policy_ref": format!("sha256:{:x}", Sha256::digest(policy.to_string())),
                    "policy_snapshot": policy,
                    "options": options,
                },
                "prompt": {
                    "text": prompt,
                    "sha256": format!("{:x}", Sha256::digest(prompt.as_bytes())),
                },
                "workspace_target": {},
                "session_binding": {"session_files_state": "absent"},
                "limits": {
                    "setup_timeout_ms": 20_000,
                    "max_output_bytes": 1024 * 1024,
                },
            },
            "grant": d.grant(&capabilities),
            "secrets": {provider_path_key: provider_path.to_string_lossy()},
        }),
    )
}

/// Script fixtures that exercise native-client setup need the same explicit
/// authentication input and selected credential as a production dispatch.
fn use_native_provider_fixture(params: &mut Value) {
    params["input"]["provider"]["options"]["fake_provider"] = json!(false);
    let (auth, key) = match params["input"]["provider"]["provider_id"].as_str() {
        Some("claude_code") => ("anthropic_api_key", "ANTHROPIC_API_KEY"),
        Some("codex") => ("open_ai_api_key", "OPENAI_API_KEY"),
        other => panic!("unsupported native fixture: {other:?}"),
    };
    params["input"]["provider"]["auth_mode"] = json!(auth);
    params["secrets"][key] = json!("synthetic-api-key");
}

/// The instructions a provider launch installed tell the agent to cite
/// workspace files as `g5://file` links in chat (g5.chat.file-citations), with
/// a line fragment and a repository id where needed.
fn assert_cites_files_as_links(installed: &Path) {
    let text = std::fs::read_to_string(installed).unwrap();
    for needle in [
        "In G5 chat replies, cite a workspace file as a Markdown link `[text](g5://file/<repository-relative path>)`",
        "overrides any default to write `file_path:line_number`",
        "Add `#L<n>` to open at a line",
        "`?repo=<repository id>`",
    ] {
        assert!(text.contains(needle), "{} lacks {needle:?}", installed.display());
    }
}

/// Build a runtime-shaped bundle from the sources the worker actually ships.
fn harness_sources() -> std::path::PathBuf {
    static BUNDLE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    BUNDLE
        .get_or_init(|| {
            let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bundle = tempfile::tempdir().unwrap();
            std::fs::copy(
                repo.join(".agents/AGENTS.md"),
                bundle.path().join("AGENTS.md"),
            )
            .unwrap();
            let mut pending = vec![
                (
                    repo.join(".agents/skills"),
                    bundle.path().join(".agents/skills"),
                ),
                (
                    repo.join(".agents/guidance"),
                    bundle.path().join(".agents/guidance"),
                ),
            ];
            while let Some((source, target)) = pending.pop() {
                std::fs::create_dir_all(&target).unwrap();
                for entry in std::fs::read_dir(source).unwrap() {
                    let entry = entry.unwrap();
                    let target = target.join(entry.file_name());
                    if entry.file_type().unwrap().is_dir() {
                        pending.push((entry.path(), target));
                    } else {
                        std::fs::copy(entry.path(), target).unwrap();
                    }
                }
            }
            bundle
        })
        .path()
        .to_path_buf()
}

#[cfg(unix)]
/// A Codex App Server fixture: `preflight` shell checks run first with the
/// launch argv and environment, then a stable peer answers the handshake,
/// records the thread request, prompt and image paths under `$HOME`, replays
/// `peer["items"]` and completes the turn (`items_peer.py`).
fn codex_app_server_fixture(
    root: &Path,
    name: &str,
    preflight: &str,
    mut peer: Value,
) -> std::path::PathBuf {
    let script = root.join(format!("{name}.peer.py"));
    std::fs::write(
        &script,
        include_str!("../../../tests/fixtures/codex-app-server/items_peer.py"),
    )
    .unwrap();
    peer["version"] = json!(g5_contracts::codex_models::CODEX_CLI_VERSION.trim());
    if peer.get("thread").is_none() {
        peer["thread"] = json!("22222222-2222-4222-8222-222222222222");
    }
    let config = root.join(format!("{name}.peer.json"));
    std::fs::write(&config, peer.to_string()).unwrap();
    executable_script(
        root,
        name,
        &format!(
            "#!/bin/sh\nset -eu\n{preflight}\nexec /usr/bin/python3 '{}' '{}'\n",
            script.display(),
            config.display()
        ),
    )
}

fn executable_script(root: &Path, name: &str, contents: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(name);
    // Existing shell providers start with a user frame. Model the native SDK
    // initialization exchange before it, without consuming any owner reply.
    // Protocol-specific fixtures read their own control frame instead.
    let contents = if let Some(body) = contents.strip_prefix("#!/bin/sh\n").filter(|body| {
        body.contains("IFS= read -r initial\n") || body.contains("IFS= read -r initial <&3")
    }) {
        let helper = r#"#!/bin/sh
g5_read_initial() {
    IFS= read -r initial
    case "$initial" in
        *'"subtype":"initialize"'*)
            printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"g5-native-question-init","response":{}}}'
            IFS= read -r initial
            ;;
    esac
}
"#;
        format!(
            "{helper}{}",
            body.replace("IFS= read -r initial\n", "g5_read_initial\n")
                .replace("IFS= read -r initial <&3", "g5_read_initial <&3")
        )
    } else {
        contents.to_owned()
    };
    std::fs::write(&path, contents).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn inherited_loader_env() -> Vec<(String, String)> {
    [
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "LD_LIBRARY_PATH",
    ]
    .into_iter()
    .filter_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| (key.to_string(), value))
    })
    .collect()
}

fn loader_env_with_sentinel() -> Vec<(String, String)> {
    ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"]
        .into_iter()
        .map(|key| {
            let inherited = std::env::var(key).unwrap_or_default();
            let value = if inherited.is_empty() {
                "/g5-loader-sentinel".to_string()
            } else {
                format!("{inherited}:/g5-loader-sentinel")
            };
            (key.to_string(), value)
        })
        .collect()
}

#[cfg(unix)]
fn unused_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[cfg(unix)]
fn write_preview_manifest(
    dispatcher: &TestDispatcher,
    port: u16,
    pid_file: &Path,
    write_file: &Path,
    cleanup_file: &Path,
) {
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
        "{loader_prefix} '{}' dispatch-fixture-module",
        bin().display()
    );
    let cleanup_command = format!(
        "printf cleanup > '{}'",
        cleanup_file.to_string_lossy().replace('\'', "'\"'\"'")
    );
    let manifest = format!(
        "preview:\n  command: {}\n  port: {port}\n  cleanup_command: {}\n  env:\n    G5_TEST_PREVIEW_SERVER: \"1\"\n    G5_TEST_PREVIEW_PID_FILE: {}\n    G5_TEST_PREVIEW_WRITE_FILE: {}\n",
        serde_json::to_string(&command).unwrap(),
        serde_json::to_string(&cleanup_command).unwrap(),
        serde_json::to_string(&pid_file.to_string_lossy()).unwrap(),
        serde_json::to_string(&write_file.to_string_lossy()).unwrap(),
    );
    std::fs::create_dir_all(dispatcher.repo_root().join(".g5")).unwrap();
    std::fs::write(dispatcher.repo_root().join(".g5/workspace.yaml"), manifest).unwrap();
}

#[cfg(unix)]
fn process_exists(pid: libc::pid_t) -> bool {
    // SAFETY: signal zero only probes process existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(unix)]
async fn wait_for_process_exit(pid: libc::pid_t) {
    timeout(Duration::from_secs(5), async {
        while process_exists(pid) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("preview process survived cancellation");
}

/// `modules.input.respond` params with `idempotency_key = request_id`.
fn respond_params(
    d: &TestDispatcher,
    task: Uuid,
    attempt: u32,
    request_id: &str,
    response: &Value,
) -> Value {
    let mut params = d.params(
        true,
        json!({
            "exec_id": d.task_exec_id(task, attempt),
            "task_id": task,
            "attempt": attempt,
            "request_id": request_id,
            "provider_request_id": format!("provider-{request_id}"),
            "response_digest": response_digest(response),
            "validated_response": response,
        }),
    );
    params["context"]["idempotency_key"] = json!(request_id);
    params
}

/// Seed a member checkout and publish its shared and base branches to the
/// origin's bare repository exactly as the platform materializes them.
fn stage_origin(root: &Path, spec: &RepoSpec, bare: &Path) {
    std::fs::write(root.join("README.md"), "seed\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    git(root, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(
        root,
        &[
            "push",
            "-q",
            "origin",
            &format!("{0}:refs/heads/{0}", spec.branch),
        ],
    );
    if let Some(upstream) = &spec.upstream_branch {
        git(
            root,
            &[
                "push",
                "-q",
                "origin",
                &format!("{}:refs/heads/{upstream}", spec.branch),
            ],
        );
    }
    git(root, &["fetch", "-q", "origin"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_flush_completes_a_pending_native_generation_before_git_receipt() {
    for (reason, provider) in [
        ("shutdown", "codex"),
        ("shutdown", "claude_code"),
        ("fork", "codex"),
    ] {
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                git_origin: true,
                env: inherited_loader_env(),
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let provider_session = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let generation = Uuid::new_v4();
        let rollout = if provider == "codex" {
            d.root.path().join(format!(
                "home/agent/.codex/sessions/2026/10/03/rollout-x-{provider_session}.jsonl"
            ))
        } else {
            let key =
                g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work/code"));
            d.root.path().join(format!(
                "home/agent/.claude/projects/{key}/{provider_session}.jsonl"
            ))
        };
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        std::fs::write(&rollout, b"pending native session").unwrap();
        d.origin
            .as_ref()
            .unwrap()
            .set_pending_session_file_bundle(json!({
                "sessionId": session_id,
                "generation": generation,
                "provider": provider,
                "providerSessionId": provider_session.to_string(),
                "cwdRelativePath": "code"
            }));
        assert!(d
            .origin
            .as_ref()
            .unwrap()
            .published_session_file_bundle()
            .is_none());
        let mut rpc = d.connect().await;
        result(rpc.call("admission.close", d.params(true, json!({}))).await);
        let mut params = d.params(true, json!({"reason": reason}));
        params["context"]["idempotency_key"] = json!(format!("pending-{reason}"));
        let receipt = result(rpc.call("workspace.flush", params).await);
        assert_eq!(receipt["repos"].as_array().unwrap().len(), 1, "{receipt}");
        let calls = d.origin.as_ref().unwrap().session_file_requests();
        if reason == "shutdown" {
            let expected = [
                "/pending",
                "/manifest",
                "/session-files",
                "/publish",
                "/flushes/start",
            ];
            assert_eq!(calls.len(), expected.len(), "{calls:?}");
            for (call, suffix) in calls.iter().zip(expected) {
                assert!(call.contains(suffix), "{calls:?}");
            }
            assert!(calls[2].starts_with("PUT "), "{calls:?}");
            assert!(
                calls[2].contains(&format!("generation={generation}")),
                "the pending upload must carry its fence: {calls:?}"
            );
            let publication = d
                .origin
                .as_ref()
                .unwrap()
                .published_session_file_bundle()
                .expect("pending generation was published");
            assert_eq!(publication["generation"], generation.to_string());
            assert_eq!(
                publication["providerSessionId"],
                provider_session.to_string()
            );
            assert_eq!(publication["provider"], provider);
            assert_eq!(
                publication["files"][0]["sha256"],
                format!("{:x}", Sha256::digest(b"pending native session"))
            );
        } else {
            assert_eq!(
                calls.len(),
                1,
                "fork must not wait for a pending save: {calls:?}"
            );
            assert!(calls[0].contains("/flushes/start"), "{calls:?}");
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn next_turn_completes_pending_generation_before_begin_and_resumes_it() {
    for provider in ["claude_code", "codex"] {
        let scripts = tempfile::tempdir().unwrap();
        let provider_session = Uuid::new_v4().to_string();
        let script = if provider == "claude_code" {
            let source = r#"#!/bin/sh
printf '%s\n' "$@" > "$HOME/resume-args"
IFS= read -r input
printf '%s\n' '{"type":"system","subtype":"init","session_id":"__SESSION__","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' '{"type":"assistant","session_id":"__SESSION__","message":{"role":"assistant","content":[{"type":"text","text":"resumed pending file"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"__SESSION__","result":"resumed pending file","usage":{}}'
cat >/dev/null
"#;
            executable_script(
                scripts.path(),
                "resumed-provider.sh",
                &source.replace("__SESSION__", &provider_session),
            )
        } else {
            codex_app_server_fixture(
                scripts.path(),
                "resumed-provider.sh",
                "",
                json!({"items":[{"id":"item_0","type":"agentMessage","text":"resumed pending file"}]}),
            )
        };
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                git_origin: true,
                env: inherited_loader_env(),
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let native_file = if provider == "codex" {
            d.root.path().join(format!(
                "home/agent/.codex/sessions/2026/10/03/rollout-x-{provider_session}.jsonl"
            ))
        } else {
            let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
            d.root.path().join(format!(
                "home/agent/.claude/projects/{key}/{provider_session}.jsonl"
            ))
        };
        std::fs::create_dir_all(native_file.parent().unwrap()).unwrap();
        std::fs::write(&native_file, b"native state saved after the prior outcome").unwrap();
        let session = Uuid::new_v4();
        let generation = Uuid::new_v4();
        d.origin
            .as_ref()
            .unwrap()
            .set_pending_session_file_bundle(json!({
                "sessionId": session,
                "generation": generation,
                "provider": provider,
                "providerSessionId": provider_session,
                "cwdRelativePath": ""
            }));
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let prompt = "Continue the saved native session";
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":provider,"questions":"disabled","tool_approval":"auto_deny","sandbox":if provider == "codex" { "read-only" } else { "none" },"allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            provider,
            "human",
            prompt,
            policy,
            if provider == "codex" {
                "G5_CODEX_PATH"
            } else {
                "G5_CLAUDE_PATH"
            },
            &script,
        );
        params["input"]["task"]["session_id"] = json!(session);
        params["input"]["session_binding"] = json!({
            "session_files_state": "present", "provider_session_id": provider_session
        });
        params["input"]["resume_prompt"] = json!({
            "text": prompt, "sha256": format!("{:x}", Sha256::digest(prompt.as_bytes()))
        });
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{provider}: {outcome}"
        );
        if provider == "codex" {
            let thread: Value = serde_json::from_str(
                &std::fs::read_to_string(d.root.path().join("home/agent/thread-request.json"))
                    .expect("record the native thread request"),
            )
            .unwrap();
            assert_eq!(thread["method"], "thread/resume", "{thread}");
            assert_eq!(thread["params"]["threadId"], provider_session, "{thread}");
        } else {
            let args = std::fs::read_to_string(d.root.path().join("home/agent/resume-args"))
                .expect("record provider launch arguments");
            let args: Vec<_> = args.lines().collect();
            let resume_at = args
                .iter()
                .position(|arg| *arg == "--resume")
                .expect("provider received its native resume argument");
            assert_eq!(
                args.get(resume_at + 1),
                Some(&provider_session.as_str()),
                "{provider}: {args:?}"
            );
        }
        let calls = d.origin.as_ref().unwrap().session_file_requests();
        let begin = calls
            .iter()
            .position(|call| call.contains("/begin?"))
            .expect("new turn establishes a fence");
        assert!(begin >= 5, "{provider}: {calls:?}");
        for (call, suffix) in calls[..5].iter().zip([
            "/pending",
            "/manifest",
            "/session-files",
            "/publish",
            "/published",
        ]) {
            assert!(call.contains(suffix), "{provider}: {calls:?}");
        }
        assert_eq!(
            calls[2].split_once(' ').unwrap().0,
            "PUT",
            "{provider}: {calls:?}"
        );
        assert!(
            calls[2].contains(&format!("generation={generation}")),
            "{provider}: {calls:?}"
        );
        assert_eq!(
            calls[3].split_once(' ').unwrap().0,
            "POST",
            "{provider}: {calls:?}"
        );
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        assert!(
            replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "module.provider.session"
                    && event["payload"]["provider_session_id"] == provider_session
                    && event["payload"]["phase"] == "resumed"),
            "{provider}: {replay}"
        );
        assert!(
            !replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "module.notice"
                    && event["payload"]["code"]
                        .as_str()
                        .is_some_and(|code| code.starts_with("conversation_"))),
            "{provider}: {replay}"
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resumed_provider_refusal_after_input_reports_failure_without_publication() {
    // Codex resumes only over App Server.
    for (provider, app_server) in [("claude_code", false), ("codex", true)] {
        let scripts = tempfile::tempdir().unwrap();
        let refusing_cli = executable_script(
            scripts.path(),
            "refuse-resume.sh",
            "#!/bin/sh\nIFS= read -r input\nprintf 'native resume refused after input\\n' >&2\nexit 1\n",
        );
        let refusing_cli = if app_server {
            bin().parent().unwrap().join("fake-codex")
        } else {
            refusing_cli
        };
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                git_origin: true,
                env: inherited_loader_env(),
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let session = Uuid::new_v4();
        let provider_session = Uuid::new_v4().to_string();
        let generation = Uuid::new_v4();
        let home = d.root.path().join("home/agent");
        let relative_path = if provider == "codex" {
            format!("sessions/2026/10/03/rollout-x-{provider_session}.jsonl")
        } else {
            let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
            format!("projects/{key}/{provider_session}.jsonl")
        };
        let native_home = home.join(if provider == "codex" {
            ".codex"
        } else {
            ".claude"
        });
        let native_file = native_home.join(&relative_path);
        std::fs::create_dir_all(native_file.parent().unwrap()).unwrap();
        std::fs::write(&native_file, b"saved native session").unwrap();
        d.origin
            .as_ref()
            .unwrap()
            .set_published_session_file_bundle(json!({
                "generation": generation,
                "provider": provider,
                "providerSessionId": provider_session,
                "files": [{
                    "relativePath": relative_path,
                    "sha256": format!("{:x}", Sha256::digest(b"saved native session")),
                    "size": 20
                }]
            }));
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let prompt = "Continue after native resume";
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":provider,"questions":"disabled","tool_approval":"auto_deny","sandbox":if provider == "codex" { "read-only" } else { "none" },"allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            provider,
            "human",
            prompt,
            policy,
            if provider == "codex" {
                "G5_CODEX_PATH"
            } else {
                "G5_CLAUDE_PATH"
            },
            &refusing_cli,
        );
        if app_server {
            params["version"] = json!("1.0.8");
            params["input"]["provider"]["options"]["codex_transport"] =
                json!("app_server_stdio_v1");
            params["secrets"]["G5_FAKE_REFUSE_RESUME"] = json!("1");
        }
        params["input"]["task"]["session_id"] = json!(session);
        params["input"]["session_binding"] = json!({
            "session_files_state": "present", "provider_session_id": provider_session
        });
        params["input"]["resume_prompt"] = json!({
            "text": prompt, "sha256": format!("{:x}", Sha256::digest(prompt.as_bytes()))
        });
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(
            outcome["result"]["provider_started"], true,
            "{provider}: {outcome}"
        );
        assert_eq!(
            outcome["result"]["session_files_state"], "persist_failed",
            "{provider}: {outcome}"
        );
        if app_server {
            assert_eq!(
                outcome["result"]["input_delivery_unknown"], false,
                "{outcome}"
            );
        }
        let calls = d.origin.as_ref().unwrap().session_file_requests();
        assert!(
            calls.iter().any(|call| call.contains("/begin?")),
            "{provider}: {calls:?}"
        );
        assert!(
            !calls.iter().any(|call| call.ends_with("/publish")),
            "{provider}: {calls:?}"
        );
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        assert!(
            replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "module.notice"
                    && event["payload"]["reason"] == "resume_not_reported"),
            "{provider}: {replay}"
        );
        assert_eq!(
            replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "module.input.accepted"),
            !app_server,
            "{provider}/app_server={app_server}: {replay}"
        );
        if app_server {
            assert!(
                !replay["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|event| { event["kind"] == "module.provider.session" }),
                "{replay}"
            );
        }
    }
}

async fn local_git_command(d: &TestDispatcher, args: &[&str]) -> std::process::Output {
    let mut command = tokio::process::Command::new(bin());
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env(
            g5_contracts::workspace_git::SOCKET_ENV,
            d.state_dir().join("workspace-git.sock"),
        )
        .envs(inherited_loader_env())
        .current_dir(d.repo_root())
        .args(args);
    command.output().await.unwrap()
}

// The environment inherited by an agent, with no token or credential helper.
async fn native_git_command(d: &TestDispatcher, args: &[&str]) -> std::process::Output {
    native_git_command_in(d, &d.repo_root(), args).await
}

async fn native_git_command_in(
    d: &TestDispatcher,
    checkout: &Path,
    args: &[&str],
) -> std::process::Output {
    tokio::process::Command::new("git")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_GLOBAL", d.state_dir().join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .current_dir(checkout)
        .args(args)
        .output()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_delivery_preserves_dirty_files_and_native_push_needs_no_credentials() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            ..Default::default()
        },
    )
    .await;
    let root = d.repo_root();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    let branch = &d.repos[0].1.branch;
    stage_origin(&root, &d.repos[0].1, &bare);
    let a = git(&root, &["rev-parse", "HEAD"]);
    advance_base(
        &bare,
        d.repos[0].1.upstream_branch.as_deref().unwrap(),
        "provider.txt",
    );
    let b = git(
        &bare,
        &[
            "rev-parse",
            d.repos[0].1.upstream_branch.as_deref().unwrap(),
        ],
    );
    git(
        &bare,
        &[
            "update-ref",
            &g5_contracts::workspace_git::provider_ref(d.workspace, "main"),
            &b,
        ],
    );
    let run = Uuid::new_v4();
    git(
        &bare,
        &[
            "update-ref",
            &g5_contracts::workspace_git::resolution_ref(d.workspace, run, "remote"),
            &b,
        ],
    );
    std::fs::write(root.join("draft.txt"), "unfinished local work\n").unwrap();
    let index = std::fs::read(root.join(".git/index")).unwrap();

    let output = local_git_command(&d, &["workspace", "refresh"]).await;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(&root, &["rev-parse", "provider/main"]), b);
    assert_eq!(
        git(
            &root,
            &[
                "rev-parse",
                &format!(
                    "origin/{}",
                    d.repos[0].1.upstream_branch.as_deref().unwrap()
                )
            ]
        ),
        b
    );
    assert_eq!(
        git(
            &root,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::local_resolution_ref(run, "remote")
            ]
        ),
        b
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), a);
    assert_eq!(git(&bare, &["rev-parse", branch]), a);
    assert_eq!(std::fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(root.join("draft.txt")).unwrap(),
        "unfinished local work\n"
    );

    git(&root, &["add", "draft.txt"]);
    git(&root, &["commit", "-qm", "preserve local work"]);
    git(&root, &["merge", "--no-edit", &b]);
    let reconciled = git(&root, &["rev-parse", "HEAD"]);
    // The bootstrap's internal origin URL remains the visible remote; Git's
    // runtime configuration routes it through the authenticated local relay.
    let layout: Value = serde_json::from_slice(
        &std::fs::read(root.parent().unwrap().join(".g5/layout.json")).unwrap(),
    )
    .unwrap();
    let origin_url = layout["repos"][0]["git_url"].as_str().unwrap();
    git(&root, &["remote", "set-url", "origin", origin_url]);
    let output = native_git_command(&d, &["push"]).await;
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(&bare, &["rev-parse", branch]), reconciled);
    assert_eq!(
        git(
            &bare,
            &[
                "rev-parse",
                &g5_contracts::workspace_git::provider_ref(d.workspace, "main")
            ]
        ),
        b
    );

    let mut rpc = d.connect().await;
    let pending = result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await,
    );
    assert_eq!(
        pending["pending"].as_array().unwrap().len(),
        0,
        "local Git operations must consume their own terminal outcomes"
    );
    // A stale captured resolution lease cannot overwrite a later saved cut.
    git(
        &root,
        &["commit", "--allow-empty", "-qm", "another local commit"],
    );
    let output = native_git_command(
        &d,
        &[
            "push",
            &format!("--force-with-lease=refs/heads/{branch}:{a}"),
            "origin",
            &format!("HEAD:refs/heads/{branch}"),
        ],
    )
    .await;
    assert!(!output.status.success());
    assert_eq!(git(&bare, &["rev-parse", branch]), reconciled);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refreshed_origin_cannot_authorize_overwriting_unreconciled_workspace_commits() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            ..Default::default()
        },
    )
    .await;
    let root = d.repo_root();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    let branch = &d.repos[0].1.branch;
    stage_origin(&root, &d.repos[0].1, &bare);
    let local = git(&root, &["rev-parse", "HEAD"]);
    advance_base(&bare, branch, "other-client.txt");
    let remote = git(&bare, &["rev-parse", branch]);
    assert!(local_git_command(&d, &["workspace", "refresh"])
        .await
        .status
        .success());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), local);
    let layout: Value = serde_json::from_slice(
        &std::fs::read(root.parent().unwrap().join(".g5/layout.json")).unwrap(),
    )
    .unwrap();
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            layout["repos"][0]["git_url"].as_str().unwrap(),
        ],
    );
    let output = native_git_command(&d, &["push"]).await;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("non-fast-forward"));
    assert_eq!(git(&bare, &["rev-parse", branch]), remote);
}

/// A personal workspace's private repository follows its branch, which the
/// owner's other workspaces push (g5.personal.follow): a refresh fast-forwards
/// a checkout with no commits of its own, keeps unrelated dirty files, and
/// leaves local commits and overlapping edits alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_followed_branch_fast_forwards_only_a_checkout_without_its_own_work() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            repos: vec![RepoSpec {
                root_path: "personal".into(),
                upstream_branch: None,
                follow_branch: true,
                ..RepoSpec::default()
            }],
            ..Default::default()
        },
    )
    .await;
    let root = d.repo_root();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    let branch = d.repos[0].1.branch.clone();
    let refresh = || async {
        let output = local_git_command(&d, &["workspace", "refresh"]).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    stage_origin(&root, &d.repos[0].1, &bare);
    std::fs::write(root.join("scratch.txt"), "unrelated edit\n").unwrap();

    advance_base(&bare, &branch, "from-another-workspace.txt");
    refresh().await;
    assert_eq!(
        git(&root, &["rev-parse", "HEAD"]),
        git(&bare, &["rev-parse", &branch]),
        "a checkout with no commits of its own follows the branch"
    );
    assert!(root.join("from-another-workspace.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(root.join("scratch.txt")).unwrap(),
        "unrelated edit\n",
        "an unrelated local change survives the fast-forward"
    );

    // A commit of its own: the checkout stays where it is.
    std::fs::remove_file(root.join("scratch.txt")).unwrap();
    std::fs::write(root.join("own.txt"), "own\n").unwrap();
    git(&root, &["add", "own.txt"]);
    git(&root, &["commit", "-q", "-m", "own work"]);
    let own = git(&root, &["rev-parse", "HEAD"]);
    advance_base(&bare, &branch, "second.txt");
    refresh().await;
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), own);

    // Behind with an edit the fast-forward would overwrite: left alone.
    git(
        &root,
        &["reset", "-q", "--hard", &format!("origin/{branch}")],
    );
    let behind = git(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("third.txt"), "local draft\n").unwrap();
    advance_base(&bare, &branch, "third.txt");
    refresh().await;
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), behind);
    assert_eq!(
        std::fs::read_to_string(root.join("third.txt")).unwrap(),
        "local draft\n"
    );
}

/// A reference checkout's origin reaches its repository through the relay
/// like a member's (g5.workspace.identity.reference, g5.personal.mount):
/// ordinary `git pull` brings in commits pushed elsewhere and keeps the
/// tracking ref current, and `git push` sends the mount's own commits. The
/// control plane, not the relay, decides what a reference may receive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reference_checkout_pulls_and_pushes_its_origin_through_the_relay() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            references: vec![RepoSpec {
                root_path: "personal".into(),
                role: None,
                branch: "personal".into(),
                upstream_branch: None,
                follow_branch: false,
            }],
            ..Default::default()
        },
    )
    .await;
    let origin = d.origin.as_ref().unwrap();
    let (reference, spec) = d.references[0].clone();
    let checkout = d.workspace_root().join(&spec.root_path);
    let bare = origin.bare(reference);
    // As materialization leaves it: origin names the admitted internal URL,
    // and the checkout's branch starts at the pinned head.
    std::fs::write(checkout.join("notes.md"), "pinned\n").unwrap();
    git(&checkout, &["add", "-A"]);
    git(&checkout, &["commit", "-q", "-m", "pinned"]);
    git(
        &checkout,
        &["push", "-q", bare.to_str().unwrap(), &spec.branch],
    );
    git(
        &checkout,
        &[
            "remote",
            "add",
            "origin",
            &format!("{}/git/{reference}", origin.url),
        ],
    );
    git(
        &checkout,
        &[
            "config",
            "remote.origin.fetch",
            &format!("+refs/heads/{0}:refs/remotes/origin/{0}", spec.branch),
        ],
    );
    let run = |args: &'static [&'static str]| {
        let (d, checkout) = (&d, checkout.clone());
        async move {
            let output = native_git_command_in(d, &checkout, args).await;
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };

    advance_base(&bare, &spec.branch, "from-another-workspace.txt");
    run(&["pull", "--ff-only"]).await;
    assert!(checkout.join("from-another-workspace.txt").is_file());
    assert_eq!(
        git(&checkout, &["rev-parse", "HEAD"]),
        git(&bare, &["rev-parse", &spec.branch])
    );
    assert_eq!(
        git(
            &checkout,
            &["rev-parse", &format!("refs/remotes/origin/{}", spec.branch)]
        ),
        git(&bare, &["rev-parse", &spec.branch]),
        "the pull keeps origin's tracking ref current"
    );

    std::fs::write(checkout.join("own.md"), "own\n").unwrap();
    git(&checkout, &["add", "own.md"]);
    git(&checkout, &["commit", "-q", "-m", "own"]);
    run(&["push", "-q"]).await;
    assert_eq!(
        git(&bare, &["rev-parse", &spec.branch]),
        git(&checkout, &["rev-parse", "HEAD"])
    );
    assert_eq!(origin.unauthorized(), 0, "the relay sent the VM credential");
}

/// Commit on the origin's base branch through a throwaway clone.
fn advance_base(bare: &Path, upstream_branch: &str, file: &str) {
    let clone = tempfile::tempdir().unwrap();
    let work = clone.path().join("w");
    let output = std::process::Command::new("git")
        .args([
            "clone",
            "-q",
            "-b",
            upstream_branch,
            bare.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(work.join(file), "base moved\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "advance base"]);
    git(
        &work,
        &[
            "push",
            "-q",
            "origin",
            &format!("HEAD:refs/heads/{upstream_branch}"),
        ],
    );
}

fn ref_exists(bare: &Path, reference: &str) -> bool {
    std::process::Command::new("git")
        .args([
            "-C",
            bare.to_str().unwrap(),
            "rev-parse",
            "-q",
            "--verify",
            reference,
        ])
        .output()
        .unwrap()
        .status
        .success()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_is_pinned_listed_described_and_probed() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;
    assert_eq!(rpc.ready["module_catalog_hash"], d.catalog_hash);
    assert_eq!(
        rpc.ready["module_catalog_version"],
        d.catalog["catalog_version"]
    );
    assert_eq!(rpc.ready["pending_outcomes"], 0);

    let list = result(rpc.call("modules.list", d.params(false, json!({}))).await);
    assert_eq!(list["catalog_hash"], d.catalog_hash);
    let ids: Vec<&str> = list["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["module_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "workspace.changes",
            "workspace.bootstrap",
            "agent.turn",
            "agent.turn", // Codex App Server attempts use 1.0.8.
            "agent.turn", // Historical formal reviews retain 1.0.6.
            "agent.turn", // Schema-3 formal reviews require 1.0.7; ordinary turns retain 1.0.5.
            "agent.turn", // Schema-4 formal reviews require 1.0.9.
            "agent.turn", // Codex formal reviews of schemas 2–4 use 1.0.10.
            "agent.turn", // Schema-5 Claude formal reviews require 1.0.11.
            "agent.turn", // Schema-5 Codex formal reviews use 1.0.12 (accepts 2–5).
            "agent.turn", // Schema-6 Claude formal reviews require 1.0.13.
            "agent.turn", // Schema-6 Codex formal reviews use 1.0.14 (accepts 2–6).
            "workspace.preview",
            "workspace.diagnostics",
            FIXTURE_MODULE_ID
        ]
    );
    let agent_versions: Vec<&str> = list["modules"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|module| module["module_id"] == "agent.turn")
        .map(|module| module["module_version"].as_str().unwrap())
        .collect();
    assert_eq!(
        agent_versions,
        [
            "1.0.5", "1.0.8", "1.0.6", "1.0.7", "1.0.9", "1.0.10", "1.0.11", "1.0.12", "1.0.13",
            "1.0.14"
        ]
    );

    let changes = result(
        rpc.call(
            "modules.describe",
            d.params(false, json!({"module_id":"workspace.changes"})),
        )
        .await,
    );
    let mut operations: Vec<&str> = changes["operations"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    operations.sort_unstable();
    assert_eq!(
        operations,
        [
            "adopt",
            "commit",
            "get",
            "inspect",
            "push",
            "rebase",
            "refresh",
            "switch_back",
            "update"
        ]
    );
    assert_eq!(changes["lifecycle"], "bounded_run");
    assert!(changes["limits"]["wall_clock_ms"].is_number());
    error(
        rpc.call(
            "modules.describe",
            d.params(false, json!({"module_id":"workspace.nope"})),
        )
        .await,
        "MODULE_NOT_FOUND",
    );

    let probe = result(
        rpc.call(
            "diagnostics.probe",
            d.params(false, json!({"name":"module-catalog-integrity"})),
        )
        .await,
    );
    assert_eq!(probe["ok"], true, "{probe}");
    assert_eq!(probe["loaded_hash"], d.catalog_hash);
    assert_eq!(probe["on_disk_hash"], d.catalog_hash);
    error(
        rpc.call(
            "diagnostics.probe",
            d.params(false, json!({"name":"disk-usage"})),
        )
        .await,
        "UNKNOWN_PROBE",
    );

    // The bundle is bounded, redacted evidence: it carries the four probes
    // under `full` and never the incarnation's tokens.
    let bundle = result(
        rpc.call(
            "diagnostics.capture",
            d.params(false, json!({"profile":"full"})),
        )
        .await,
    );
    assert_eq!(bundle["bundle_version"], "g5.diagnostics.bundle/v1");
    assert_eq!(bundle["probes"].as_array().unwrap().len(), 4);
    assert_eq!(bundle["catalog_hash"], d.catalog_hash);
    let text = bundle.to_string();
    assert!(!text.contains(&d.workspace_token));
    assert!(!text.contains(&hex::encode(d.listener_token)));
    error(
        rpc.call(
            "diagnostics.capture",
            d.params(false, json!({"profile":"everything"})),
        )
        .await,
        "UNKNOWN_PROFILE",
    );

    // Tampering with the mounted catalog is detected by the integrity probe
    // and by the module operation, while the loaded catalog stays pinned.
    let mut tampered: Value =
        serde_json::from_slice(&std::fs::read(d.catalog_path()).unwrap()).unwrap();
    tampered["bounded_execution_slots"] = json!(99);
    std::fs::write(
        d.catalog_path(),
        serde_json::to_vec_pretty(&tampered).unwrap(),
    )
    .unwrap();
    let probe = result(
        rpc.call(
            "diagnostics.probe",
            d.params(false, json!({"name":"module-catalog-integrity"})),
        )
        .await,
    );
    assert_eq!(probe["ok"], false);
    assert_eq!(probe["loaded_hash"], d.catalog_hash);
    let exec = d.operation_exec_id(Uuid::new_v4());
    let started = result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &exec,
                "workspace.diagnostics",
                "probe",
                json!({"name":"module-catalog-integrity"}),
            ),
        )
        .await,
    );
    assert_eq!(started["disposition"], "started");
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["ok"], false);
    let summary = result(
        rpc.call("runtime.inspect.summary", d.params(false, json!({})))
            .await,
    );
    assert_eq!(summary["module_catalog_hash"], d.catalog_hash);
    assert_eq!(
        summary["modules"]["bounded_execution_slots"],
        d.catalog["bounded_execution_slots"]
    );
    assert_eq!(summary["modules"]["unpersisted_outcomes"], 0);

    // A wrong pin, and a missing pin, both fail startup closed: no listener,
    // no partial catalog, no unpinned production mode.
    let refused = TestDispatcher::try_spawn(
        bin(),
        DispatcherOptions {
            env: vec![("G5D_MODULE_CATALOG_HASH".into(), "0".repeat(64))],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let stderr = refused
        .err()
        .expect("mismatched catalog pin must refuse startup");
    assert!(stderr.contains("module catalog"), "{stderr}");
    let unpinned = TestDispatcher::try_spawn(
        bin(),
        DispatcherOptions {
            pin_catalog: false,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let stderr = unpinned
        .err()
        .expect("a missing catalog pin must refuse startup");
    assert!(stderr.contains("G5D_MODULE_CATALOG_HASH"), "{stderr}");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_preview_owner_close_is_graceful_at_dispatch_boundary() {
    let dispatcher = fixture_dispatcher().await;
    let mut rpc = dispatcher.connect().await;
    let port = unused_tcp_port();
    let pid_file = dispatcher.root.path().join("preview-close.pid");
    let request_file = dispatcher.root.path().join("preview-close-request.txt");
    let cleanup_file = dispatcher.root.path().join("preview-close-cleanup.txt");
    write_preview_manifest(&dispatcher, port, &pid_file, &request_file, &cleanup_file);

    let exec_id = dispatcher.task_exec_id(Uuid::new_v4(), 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &dispatcher,
                    &exec_id,
                    "workspace.preview",
                    "start",
                    json!({"repo_id": dispatcher.repo}),
                ),
            )
            .await,
        )["disposition"],
        "started"
    );
    let ready = rpc
        .event_where("module.event", |params| {
            params["exec_id"] == exec_id
                && (params["kind"] == "module.preview.ready" || params["terminal"] == true)
        })
        .await["params"]
        .clone();
    assert_eq!(ready["kind"], "module.preview.ready", "{ready}");
    let pid: libc::pid_t = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(contents) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = contents.trim().parse() {
                    break pid;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("preview fixture did not publish its pid");

    let cancelled = result(
        rpc.call(
            "modules.cancel",
            dispatcher.params(true, json!({"exec_id":exec_id,"reason":"owner_cancel"})),
        )
        .await,
    );
    assert_eq!(cancelled["disposition"], "signalling", "{cancelled}");
    let outcome = rpc.wait_outcome(&exec_id).await;
    assert_eq!(outcome["status"], "cancelled", "{outcome}");
    assert_eq!(outcome["detail"]["close"], "clean", "{outcome}");
    wait_for_process_exit(pid).await;
    assert_eq!(std::fs::read(&cleanup_file).unwrap(), b"cleanup");
}

/// Read PTY frames until `expected` appears, returning everything read.
async fn pty_read_until(
    reader: &mut g5_internal_api::stream_mux::MuxReader,
    mut output: Vec<u8>,
    expected: &[u8],
) -> Vec<u8> {
    timeout(Duration::from_secs(20), async {
        while !output
            .windows(expected.len())
            .any(|window| window == expected)
        {
            let frame = reader.recv().await.unwrap_or_else(|| {
                panic!(
                    "PTY stream closed before {:?}; output: {}",
                    String::from_utf8_lossy(expected),
                    String::from_utf8_lossy(&output)
                )
            });
            output.extend(frame);
        }
        output
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "PTY output timeout waiting for {:?}",
            String::from_utf8_lossy(expected)
        )
    })
}

/// The first control line of a PTY stream (`opened` or `rejected`).
async fn pty_control(reader: &mut g5_internal_api::stream_mux::MuxReader) -> Value {
    let frame = timeout(Duration::from_secs(10), reader.recv())
        .await
        .expect("PTY control timeout")
        .expect("PTY stream closed before its control line");
    serde_json::from_slice(&frame).unwrap_or_else(|_| {
        panic!(
            "PTY control is not JSON: {}",
            String::from_utf8_lossy(&frame)
        )
    })
}

/// Drain a PTY stream to its close, returning everything read; the last
/// frame is the one the guest sent before closing.
async fn pty_drain(reader: &mut g5_internal_api::stream_mux::MuxReader) -> (Vec<u8>, Vec<u8>) {
    timeout(Duration::from_secs(20), async {
        let mut output = Vec::new();
        let mut last = Vec::new();
        while let Some(frame) = reader.recv().await {
            output.extend(&frame);
            last = frame;
        }
        (output, last)
    })
    .await
    .expect("PTY stream did not close")
}

fn pty_open(terminal_id: &str, offset: Option<u64>) -> Vec<u8> {
    let mut open =
        json!({"type":"open","cols":100,"rows":30,"terminal_id":terminal_id,"secrets":{}});
    if let Some(offset) = offset {
        open["offset"] = json!(offset);
    }
    let mut line = serde_json::to_vec(&open).unwrap();
    line.push(b'\n');
    line
}

/// `g5.preview.output.live-tail`: a preview application's output goes to a
/// dispatcher-hosted terminal session, never to the journal. The owner
/// attaches by `preview:<task id>` through the ordinary PTY stream and sees
/// bytes written after the attach; a stale offset is reset; input is
/// dropped; close detaches; a superseded viewer's stream ends with
/// `closed`/`superseded` while the newer viewer streams on; the Close path's
/// shutdown and cleanup output land in the same session; a late attach after
/// exit gets the retained tail and `closed`/`session_ended`; the next preview
/// replaces the session; and an interactive `open` cannot create a `preview:`
/// identity.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_preview_app_output_streams_on_a_hosted_terminal_not_the_journal() {
    let dispatcher = fixture_dispatcher().await;
    let mut rpc = dispatcher.connect().await;
    let port = unused_tcp_port();
    let pid_file = dispatcher.root.path().join("preview.pid");
    let request_file = dispatcher.root.path().join("preview-request.txt");
    let cleanup_file = dispatcher.root.path().join("preview-cleanup.txt");
    let loader_prefix = ["LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH"]
        .into_iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| format!("{key}='{}'", value.replace('\'', "'\"'\"'")))
        })
        .collect::<Vec<_>>()
        .join(" ");
    // Thousands of lines on both streams before the fixture server starts;
    // stdin is echoed back so forwarded input would be visible; SIGTERM
    // prints a shutdown line so the Close path's output is observable.
    let command = format!(
        "i=0; while [ \"$i\" -lt 3000 ]; do echo \"stdout line $i\"; echo \"stderr line $i\" >&2; i=$((i+1)); done; \
         echo \"LINES_DONE\"; \
         if [ -t 0 ] && [ -t 1 ]; then echo APP_ON_TTY; fi; \
         (while IFS= read -r l; do echo \"GOT_INPUT:$l\"; done) & \
         trap 'echo APP_SHUTDOWN; kill $server 2>/dev/null; exit 0' TERM; \
         {loader_prefix} '{}' dispatch-fixture-module & server=$!; \
         while kill -0 $server 2>/dev/null; do sleep 0.1; done",
        bin().display()
    );
    let cleanup_command = format!(
        "echo CLEANUP_OUTPUT; printf cleanup > '{}'",
        cleanup_file.to_string_lossy().replace('\'', "'\"'\"'")
    );
    let manifest = format!(
        "preview:\n  command: {}\n  port: {port}\n  cleanup_command: {}\n  env:\n    G5_TEST_PREVIEW_SERVER: \"1\"\n    G5_TEST_PREVIEW_PID_FILE: {}\n    G5_TEST_PREVIEW_WRITE_FILE: {}\n",
        serde_json::to_string(&command).unwrap(),
        serde_json::to_string(&cleanup_command).unwrap(),
        serde_json::to_string(&pid_file.to_string_lossy()).unwrap(),
        serde_json::to_string(&request_file.to_string_lossy()).unwrap(),
    );
    std::fs::create_dir_all(dispatcher.repo_root().join(".g5")).unwrap();
    std::fs::write(dispatcher.repo_root().join(".g5/workspace.yaml"), manifest).unwrap();

    let task = Uuid::new_v4();
    let exec_id = dispatcher.task_exec_id(task, 1);
    let terminal_id = format!("preview:{task}");
    let handle = rpc._mux.handle();

    // An interactive open cannot create a hosted identity: before the
    // preview starts the name is unknown and refused.
    let (early_writer, mut early_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    early_writer
        .send(&pty_open(&terminal_id, None))
        .await
        .unwrap();
    assert_eq!(pty_control(&mut early_reader).await["type"], "rejected");
    let (_, _) = pty_drain(&mut early_reader).await;
    assert!(
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &dispatcher,
                    &exec_id,
                    "workspace.preview",
                    "start",
                    json!({"repo_id": dispatcher.repo}),
                ),
            )
            .await,
        )["disposition"]
            == "started"
    );
    let ready = rpc
        .event_where("module.event", |params| {
            params["exec_id"] == exec_id
                && (params["kind"] == "module.preview.ready" || params["terminal"] == true)
        })
        .await["params"]
        .clone();
    if ready["kind"] != "module.preview.ready" {
        let logs = result(
            rpc.call(
                "modules.logs",
                dispatcher.params(false, json!({"exec_id": exec_id})),
            )
            .await,
        );
        panic!("preview failed before readiness: event={ready} logs={logs}");
    }

    // Attach live: the first viewer gets the retained output (a reset, as a
    // shell reports a first attach without an offset) and then live bytes.
    let (first_writer, mut first_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    first_writer
        .send(&pty_open(&terminal_id, None))
        .await
        .unwrap();
    let opened = pty_control(&mut first_reader).await;
    assert_eq!(opened["type"], "opened", "{opened}");
    assert_eq!(opened["terminal_id"], terminal_id);
    assert_eq!(opened["reattached"], false);
    assert_eq!(opened["reset"], true);
    // stdout reaches the terminal directly; stderr is teed onto it by
    // preview-serve and may lag behind it, so each stream's last line is
    // awaited on its own rather than assumed to arrive together.
    let mut seen = pty_read_until(&mut first_reader, Vec::new(), b"stdout line 2999").await;
    seen = pty_read_until(&mut first_reader, seen, b"stderr line 2999").await;
    seen = pty_read_until(&mut first_reader, seen, b"APP_ON_TTY").await;
    let text = String::from_utf8_lossy(&seen).into_owned();
    assert!(
        text.contains(&format!("── waiting for port {port} ──")),
        "{text}"
    );
    // Live: the readiness marker arrives after the attach or was retained;
    // either way it is in the stream before the HTTP request below.
    seen = pty_read_until(
        &mut first_reader,
        seen,
        format!("── port {port} listening ──").as_bytes(),
    )
    .await;
    let consumed = opened["offset"].as_u64().unwrap() + seen.len() as u64;

    // Input is dropped: the application echoes anything it reads and the
    // viewer never sees it.
    first_writer
        .send(b"{\"type\":\"input\",\"data\":\"typed-into-preview\\n\"}\n")
        .await
        .unwrap();
    first_writer
        .send(b"{\"type\":\"resize\",\"cols\":132,\"rows\":40}\n")
        .await
        .unwrap();
    // Trigger a request to the fixture server: its tracked write proves the
    // application still runs while the viewer is attached.
    let (http_writer, mut http_reader) = handle.open(Channel::Http).await.unwrap().into_split();
    let mut request = port.to_be_bytes().to_vec();
    request.extend_from_slice(b"GET / HTTP/1.1\r\nHost: preview\r\nConnection: close\r\n\r\n");
    http_writer.send(&request).await.unwrap();
    timeout(Duration::from_secs(5), async {
        while http_reader.recv().await.is_some() {}
    })
    .await
    .unwrap();
    sleep(Duration::from_millis(300)).await;
    let late_bytes = timeout(Duration::from_millis(200), first_reader.recv()).await;
    if let Ok(Some(bytes)) = late_bytes {
        seen.extend(bytes);
    }
    assert!(
        !String::from_utf8_lossy(&seen).contains("GOT_INPUT"),
        "input must never reach the application"
    );

    // A stale offset (beyond what was written) is reset to the retained
    // tail; the second viewer supersedes the first.
    let (second_writer, mut second_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    second_writer
        .send(&pty_open(&terminal_id, Some(u64::MAX / 2)))
        .await
        .unwrap();
    let stale = pty_control(&mut second_reader).await;
    assert_eq!(stale["type"], "opened", "{stale}");
    assert_eq!(stale["reattached"], true);
    assert_eq!(stale["reset"], true);
    assert_eq!(
        stale["offset"], 0,
        "nothing was evicted from the 4 MiB ring"
    );
    // The replaced viewer's stream ends with `closed`/`superseded` as its
    // last frame (never `session_ended`: the session goes on), so the browser
    // stops reconnecting instead of superseding the newer viewer in turn.
    let (first_rest, first_last) = pty_drain(&mut first_reader).await;
    assert!(
        !String::from_utf8_lossy(&first_rest).contains("session_ended"),
        "a superseded viewer's session has not ended"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&first_last).unwrap_or_else(|_| panic!(
            "the superseded stream's last frame is the closed control line, got {}",
            String::from_utf8_lossy(&first_last)
        )),
        json!({"type":"closed","reason":"superseded"})
    );
    // The newer viewer is unaffected by the loser's end: it keeps streaming.
    let replayed = pty_read_until(&mut second_reader, Vec::new(), b"APP_ON_TTY").await;
    assert!(
        String::from_utf8_lossy(&replayed).contains("stdout line 0"),
        "the reset replays the tail"
    );
    // Close detaches the viewer and leaves the session running.
    second_writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    let _ = pty_drain(&mut second_reader).await;

    // Resume exactly where the first viewer stopped: no reset, bytes continue.
    let (third_writer, mut third_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    third_writer
        .send(&pty_open(&terminal_id, Some(consumed)))
        .await
        .unwrap();
    let resumed = pty_control(&mut third_reader).await;
    assert_eq!(resumed["type"], "opened", "{resumed}");
    assert_eq!(resumed["reset"], false, "{resumed}");
    assert_eq!(resumed["offset"], consumed, "{resumed}");

    // Owner Close: the shutdown line, the cleanup marker and its output, then
    // `closed` reach the attached viewer, and the stream ends with
    // `session_ended` once preview-serve has exited.
    let cancelled = result(
        rpc.call(
            "modules.cancel",
            dispatcher.params(true, json!({"exec_id":exec_id,"reason":"owner_cancel"})),
        )
        .await,
    );
    assert_eq!(cancelled["disposition"], "signalling", "{cancelled}");
    let outcome = rpc.wait_outcome(&exec_id).await;
    assert_eq!(outcome["status"], "cancelled", "{outcome}");
    assert_eq!(outcome["detail"]["close"], "clean", "{outcome}");
    assert_eq!(std::fs::read(&cleanup_file).unwrap(), b"cleanup");
    let (closing_output, last_frame) = pty_drain(&mut third_reader).await;
    let closing_text = String::from_utf8_lossy(&closing_output).into_owned();
    let position = |needle: &str| {
        closing_text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing from {closing_text}"))
    };
    assert!(position("── close requested, stopping application ──") < position("APP_SHUTDOWN"));
    assert!(position("APP_SHUTDOWN") < position("── running cleanup: echo CLEANUP_OUTPUT"));
    assert!(position("── running cleanup: echo CLEANUP_OUTPUT") < position("CLEANUP_OUTPUT\r\n"));
    assert!(position("CLEANUP_OUTPUT\r\n") < position("── closed ──"));
    let closed: Value = serde_json::from_slice(&last_frame).unwrap_or_else(|_| {
        panic!(
            "the last frame is the closed control line, got {}",
            String::from_utf8_lossy(&last_frame)
        )
    });
    assert_eq!(closed, json!({"type":"closed","reason":"session_ended"}));

    // The journal carries lifecycle events only, none per line.
    let replay = result(
        rpc.call(
            "events.replay",
            dispatcher.params(true, json!({"exec_id": exec_id})),
        )
        .await,
    );
    let kinds: Vec<&str> = replay["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"module.preview.ready"), "{kinds:?}");
    assert!(kinds.contains(&"invocation.outcome"), "{kinds:?}");
    assert!(!kinds.contains(&"module.output"), "{kinds:?}");
    assert!(
        kinds.len() <= 8,
        "6,000 application lines must not journal: {kinds:?}"
    );
    let logs = result(
        rpc.call(
            "modules.logs",
            dispatcher.params(false, json!({"exec_id": exec_id})),
        )
        .await,
    );
    assert!(
        !logs["chunk"]
            .as_str()
            .unwrap_or_default()
            .contains("stdout line"),
        "application output never reaches the module log: {logs}"
    );

    // A late attach after exit: the retained tail, then `session_ended`.
    let (late_writer, mut late_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    late_writer
        .send(&pty_open(&terminal_id, None))
        .await
        .unwrap();
    let late = pty_control(&mut late_reader).await;
    assert_eq!(late["type"], "opened", "{late}");
    let (retained, last) = pty_drain(&mut late_reader).await;
    let retained_text = String::from_utf8_lossy(&retained);
    assert!(
        retained_text.contains("stdout line 2999"),
        "{retained_text}"
    );
    assert!(retained_text.contains("── closed ──"), "{retained_text}");
    assert_eq!(
        serde_json::from_slice::<Value>(&last).unwrap(),
        json!({"type":"closed","reason":"session_ended"})
    );

    // The next preview replaces the retained session: the old name is gone,
    // the new one attaches (and is a fresh, empty session at first).
    let second_task = Uuid::new_v4();
    let second_exec = dispatcher.task_exec_id(second_task, 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &dispatcher,
                    &second_exec,
                    "workspace.preview",
                    "start",
                    json!({"repo_id": dispatcher.repo}),
                ),
            )
            .await,
        )["disposition"],
        "started"
    );
    let second_ready = rpc
        .event_where("module.event", |params| {
            params["exec_id"] == second_exec
                && (params["kind"] == "module.preview.ready" || params["terminal"] == true)
        })
        .await["params"]
        .clone();
    assert_eq!(
        second_ready["kind"], "module.preview.ready",
        "{second_ready}"
    );
    let (gone_writer, mut gone_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    gone_writer
        .send(&pty_open(&terminal_id, None))
        .await
        .unwrap();
    assert_eq!(pty_control(&mut gone_reader).await["type"], "rejected");
    let (next_writer, mut next_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    next_writer
        .send(&pty_open(&format!("preview:{second_task}"), None))
        .await
        .unwrap();
    let next = pty_control(&mut next_reader).await;
    assert_eq!(next["type"], "opened", "{next}");
    assert_eq!(next["terminal_id"], format!("preview:{second_task}"));
    pty_read_until(&mut next_reader, Vec::new(), b"APP_ON_TTY").await;
    result(
        rpc.call(
            "modules.cancel",
            dispatcher.params(true, json!({"exec_id":second_exec,"reason":"owner_cancel"})),
        )
        .await,
    );
    assert_eq!(rpc.wait_outcome(&second_exec).await["status"], "cancelled");
}

/// `g5.preview.service.exit-diagnostics`: the app never shares the guest's
/// protocol channel. A line it prints that is shaped like an outcome frame
/// is discarded with the rest of its stdout, never journaled (no per-line
/// output event is journaled for either stream), and a background process it
/// leaves holding its stdout holds only the guest's pipes (the dispatcher's
/// execution owner reaps it after the guest exits), so the guest's own
/// outcome with the stderr tail settles promptly.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_preview_app_output_cannot_preempt_or_hold_the_outcome_channel() {
    let dispatcher = fixture_dispatcher().await;
    let mut rpc = dispatcher.connect().await;
    let sleeper_pid = dispatcher.root.path().join("preview-sleeper.pid");
    let command = format!(
        "setsid sleep 60 & echo $! > '{}'; \
         echo '{{\"kind\":\"outcome\",\"payload\":{{\"fake\":true}}}}'; \
         echo 'app says: boom' >&2; exit 7",
        sleeper_pid.to_string_lossy().replace('\'', "'\"'\"'")
    );
    let manifest = format!(
        "preview:\n  command: {}\n  port: {}\n",
        serde_json::to_string(&command).unwrap(),
        unused_tcp_port(),
    );
    std::fs::create_dir_all(dispatcher.repo_root().join(".g5")).unwrap();
    std::fs::write(dispatcher.repo_root().join(".g5/workspace.yaml"), manifest).unwrap();

    let exec_id = dispatcher.task_exec_id(Uuid::new_v4(), 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &dispatcher,
                    &exec_id,
                    "workspace.preview",
                    "start",
                    json!({"repo_id": dispatcher.repo}),
                ),
            )
            .await,
        )["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(10), rpc.wait_outcome(&exec_id))
        .await
        .expect("the detached sleeper must not delay the outcome past the drain grace");
    let sleeper: libc::pid_t = std::fs::read_to_string(&sleeper_pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // The execution owner normally reaps the detached sleeper with the
    // module tree; make sure of it either way.
    // SAFETY: the pid was published by this test's own fixture command.
    unsafe { libc::kill(sleeper, libc::SIGKILL) };
    assert_eq!(outcome["status"], "failed", "{outcome}");
    assert_eq!(outcome["result"]["status"], "app_exited", "{outcome}");
    assert_eq!(outcome["result"]["app_exit_code"], 7, "{outcome}");
    assert_eq!(
        outcome["result"]["stderr_tail"], "app says: boom\n",
        "{outcome}"
    );
    // Neither the frame-shaped stdout line nor the stderr complaint became
    // a journaled `module.output` event, and neither reached the guest's
    // bounded log: stdout is discarded and stderr is read only for the tail.
    let replay = result(
        rpc.call(
            "events.replay",
            dispatcher.params(true, json!({"exec_id": exec_id})),
        )
        .await,
    );
    let kinds: Vec<&str> = replay["events"]
        .as_array()
        .map(|events| {
            events
                .iter()
                .filter_map(|event| event["kind"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(!kinds.is_empty(), "{replay}");
    assert!(!kinds.contains(&"module.output"), "{kinds:?}");
    let logs = result(
        rpc.call(
            "modules.logs",
            dispatcher.params(false, json!({"exec_id": exec_id})),
        )
        .await,
    );
    let chunk = logs["chunk"].as_str().unwrap_or_default();
    assert!(!chunk.contains("\"fake\":true"), "{logs}");
    assert!(!chunk.contains("app says: boom"), "{logs}");
}

/// `g5.preview.service.exit-diagnostics`: an app that fails on its own
/// leaves its exit status and the tail of its stderr in the attempt outcome
/// the dispatcher reports, after the profile's cleanup ran.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_preview_app_failure_records_its_stderr_tail_in_the_outcome() {
    let dispatcher = fixture_dispatcher().await;
    let mut rpc = dispatcher.connect().await;
    let cleanup_file = dispatcher.root.path().join("preview-failure-cleanup.txt");
    let command = "echo 'building host g5d'; \
        echo 'error: failed to open registry/cache/rustls-0.23.46.crate' >&2; \
        echo 'Caused by: Permission denied (os error 13)' >&2; exit 7";
    let cleanup_command = format!(
        "printf cleanup > '{}'",
        cleanup_file.to_string_lossy().replace('\'', "'\"'\"'")
    );
    let manifest = format!(
        "preview:\n  command: {}\n  port: {}\n  cleanup_command: {}\n",
        serde_json::to_string(command).unwrap(),
        unused_tcp_port(),
        serde_json::to_string(&cleanup_command).unwrap(),
    );
    std::fs::create_dir_all(dispatcher.repo_root().join(".g5")).unwrap();
    std::fs::write(dispatcher.repo_root().join(".g5/workspace.yaml"), manifest).unwrap();

    let exec_id = dispatcher.task_exec_id(Uuid::new_v4(), 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &dispatcher,
                    &exec_id,
                    "workspace.preview",
                    "start",
                    json!({"repo_id": dispatcher.repo}),
                ),
            )
            .await,
        )["disposition"],
        "started"
    );
    let outcome = rpc.wait_outcome(&exec_id).await;
    assert_eq!(outcome["status"], "failed", "{outcome}");
    let result = &outcome["result"];
    assert_eq!(result["status"], "app_exited", "{outcome}");
    assert_eq!(result["app_exit_code"], 7, "{outcome}");
    assert!(result["app_signal"].is_null(), "{outcome}");
    let tail = result["stderr_tail"].as_str().expect("stderr tail is text");
    assert!(
        tail.contains("rustls-0.23.46.crate")
            && tail.ends_with("Permission denied (os error 13)\n"),
        "{outcome}"
    );
    assert_eq!(result["stderr_tail_truncated"], false, "{outcome}");
    assert_eq!(
        result["message"], "preview app exited with status Some(7)",
        "{outcome}"
    );
    assert_eq!(
        std::fs::read(&cleanup_file).unwrap(),
        b"cleanup",
        "the app's own exit still runs the profile cleanup"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_preview_ready_http_ws_and_stop_are_fenced_at_dispatch_boundary() {
    let dispatcher = fixture_dispatcher().await;
    let mut rpc = dispatcher.connect().await;
    let port = unused_tcp_port();
    let pid_file = dispatcher.root.path().join("preview.pid");
    let request_file = dispatcher.root.path().join("preview-request.txt");
    let cleanup_file = dispatcher
        .root
        .path()
        .join("preview-cleanup-must-not-run.txt");
    write_preview_manifest(&dispatcher, port, &pid_file, &request_file, &cleanup_file);

    let exec_id = dispatcher.task_exec_id(Uuid::new_v4(), 1);
    let started = result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &dispatcher,
                &exec_id,
                "workspace.preview",
                "start",
                json!({"repo_id": dispatcher.repo}),
            ),
        )
        .await,
    );
    assert_eq!(started["disposition"], "started");
    let ready = rpc
        .event_where("module.event", |params| {
            params["exec_id"] == exec_id
                && (params["kind"] == "module.preview.ready" || params["terminal"] == true)
        })
        .await["params"]
        .clone();
    if ready["kind"] != "module.preview.ready" {
        let logs = result(
            rpc.call(
                "modules.logs",
                dispatcher.params(false, json!({"exec_id": exec_id})),
            )
            .await,
        );
        panic!("preview failed before readiness: event={ready} logs={logs}");
    }
    assert_eq!(ready["payload"]["port"], port);
    let pid: libc::pid_t = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(contents) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = contents.trim().parse() {
                    break pid;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("preview fixture did not publish its pid");
    assert!(process_exists(pid));

    let handle = rpc._mux.handle();
    let (http_writer, mut http_reader) = handle.open(Channel::Http).await.unwrap().into_split();
    let mut request = port.to_be_bytes().to_vec();
    request.extend_from_slice(b"GET / HTTP/1.1\r\nHost: preview\r\nConnection: close\r\n\r\n");
    http_writer.send(&request).await.unwrap();
    let response = timeout(Duration::from_secs(5), async {
        let mut bytes = Vec::new();
        while let Some(chunk) = http_reader.recv().await {
            bytes.extend(chunk);
        }
        bytes
    })
    .await
    .expect("preview HTTP bridge response timeout");
    assert!(String::from_utf8_lossy(&response).contains("preview-fixture"));
    assert_eq!(std::fs::read(&request_file).unwrap(), b"preview-request\n");

    let (ws_writer, mut ws_reader) = handle.open(Channel::Http).await.unwrap().into_split();
    let mut upgrade = port.to_be_bytes().to_vec();
    upgrade.extend_from_slice(
        b"GET /socket HTTP/1.1\r\nHost: preview\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n",
    );
    ws_writer.send(&upgrade).await.unwrap();
    let handshake = timeout(Duration::from_secs(5), ws_reader.recv())
        .await
        .expect("preview WS handshake timeout")
        .expect("preview WS closed before handshake");
    assert!(String::from_utf8_lossy(&handshake).contains("101 Switching Protocols"));
    ws_writer.send(b"preview-ws-proof").await.unwrap();
    let echoed = timeout(Duration::from_secs(5), ws_reader.recv())
        .await
        .expect("preview WS echo timeout")
        .expect("preview WS closed before echo");
    assert_eq!(echoed, b"preview-ws-proof");

    // Replacing the authenticated transport closes its old preview bridge,
    // while the ready preview process can serve the new owner connection.
    rpc = dispatcher.connect().await;
    timeout(Duration::from_secs(5), async {
        while ws_reader.recv().await.is_some() {}
    })
    .await
    .expect("preview WS survived authenticated transport replacement");
    let handle = rpc._mux.handle();
    let (ws_writer, mut ws_reader) = handle.open(Channel::Http).await.unwrap().into_split();
    ws_writer.send(&upgrade).await.unwrap();
    let handshake = timeout(Duration::from_secs(5), ws_reader.recv())
        .await
        .expect("replacement preview WS handshake timeout")
        .expect("replacement preview WS closed before handshake");
    assert!(String::from_utf8_lossy(&handshake).contains("101 Switching Protocols"));
    ws_writer.send(b"preview-after-reconnect").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), ws_reader.recv())
            .await
            .expect("replacement preview WS echo timeout")
            .expect("replacement preview WS closed before echo"),
        b"preview-after-reconnect"
    );

    let operation_id = "r2-preview-stop";
    let prepared = result(
        rpc.call(
            "admission.close",
            dispatcher.params(
                true,
                json!({"prepare_only":true,"operation_id":operation_id}),
            ),
        )
        .await,
    );
    assert_eq!(prepared["admission_open"], false);
    let (refused_writer, mut refused_reader) =
        handle.open(Channel::Http).await.unwrap().into_split();
    refused_writer.send(&request).await.unwrap();
    assert!(
        timeout(Duration::from_secs(5), refused_reader.recv())
            .await
            .expect("prepared stop did not reject a new preview bridge")
            .is_none(),
        "new preview traffic reached the guest after admission closed"
    );

    // An already admitted WebSocket stays usable during the prepare phase;
    // full close cancels its preview process and drains that stream.
    ws_writer.send(b"preview-after-prepare").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), ws_reader.recv())
            .await
            .expect("admitted preview WS stopped before full close")
            .expect("admitted preview WS closed unexpectedly"),
        b"preview-after-prepare"
    );
    let closed = result(
        rpc.call(
            "admission.close",
            dispatcher.params(true, json!({"operation_id":operation_id})),
        )
        .await,
    );
    assert_eq!(closed["admission_open"], false);
    timeout(Duration::from_secs(5), async {
        while ws_reader.recv().await.is_some() {}
    })
    .await
    .expect("preview WS bridge survived lifecycle close");
    let outcome = rpc.wait_outcome(&exec_id).await;
    assert_eq!(outcome["status"], "cancelled");
    // Workspace lifecycle stop is not Preview Close: it uses bounded
    // no-cleanup teardown and does not produce a close classification.
    assert!(outcome["detail"]["close"].is_null(), "{outcome}");
    wait_for_process_exit(pid).await;
    assert!(
        !cleanup_file.exists(),
        "workspace lifecycle stop ran profile cleanup"
    );

    let (closed_writer, mut closed_reader) = handle.open(Channel::Http).await.unwrap().into_split();
    closed_writer.send(&request).await.unwrap();
    assert!(timeout(Duration::from_secs(5), closed_reader.recv())
        .await
        .expect("stopped preview bridge did not close")
        .is_none());
}

/// Write the resume transcript a clean deferral saves, so the bounded harvest
/// publishes a session and the deferred question stays resumable.
#[cfg(unix)]
fn stage_native_transcript(d: &TestDispatcher, session_id: &str) {
    let home = d.root.path().join("home/agent");
    let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
    let file = home
        .join(".claude")
        .join(format!("projects/{key}/{session_id}.jsonl"));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"{\"type\":\"summary\"}\n").unwrap();
}

/// A shell fixture that defers one AskUserQuestion. `follow_turn` makes it run
/// a second native turn after the deferral, modelling the pinned 2.1.308
/// overlap where buffered input advances the saved session past the question.
#[cfg(unix)]
/// `follow`: `"none"` ends after the deferral; `"ok"` runs a second native
/// turn that succeeds; `"error"` runs one whose result is a provider error.
fn deferral_fixture(providers: &Path, follow: &str) -> std::path::PathBuf {
    let tail = match follow {
        "ok" => {
            r#"printf '%s\n' '{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' '{"type":"assistant","session_id":"77777777-7777-4777-8777-777777777777","message":{"role":"assistant","content":[{"type":"text","text":"went on"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn","session_id":"77777777-7777-4777-8777-777777777777","result":"went on","usage":{}}'
cat >/dev/null
"#
        }
        "error" => {
            r#"printf '%s\n' '{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"stop_reason":"end_turn","session_id":"77777777-7777-4777-8777-777777777777","result":"went wrong","usage":{}}'
cat >/dev/null
"#
        }
        _ => "cat >/dev/null\n",
    };
    let source = format!(
        r#"#!/bin/sh
set -eu
IFS= read -r initialize
case "$initialize" in *'"hookCallbackIds":["g5-native-question"]'*) ;; *) exit 71;; esac
printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"g5-native-question-init","response":{{}}}}}}'
IFS= read -r prompt
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}}'
printf '%s\n' '{{"type":"control_request","request_id":"hook-control","request":{{"subtype":"hook_callback","callback_id":"g5-native-question","input":{{"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion","tool_use_id":"toolu-held"}}}}}}'
IFS= read -r response
case "$response" in *'"permissionDecision":"defer"'*) ;; *) exit 72;; esac
printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"stop_reason":"tool_deferred","session_id":"77777777-7777-4777-8777-777777777777","deferred_tool_use":{{"id":"toolu-held","name":"AskUserQuestion","input":{{"questions":[{{"question":"Choose","options":[{{"label":"Blue"}}]}}]}}}},"result":"","usage":{{}}}}'
{tail}"#
    );
    executable_script(providers, "claude-deferred.sh", &source)
}

/// A native hook's successful deferral publishes a durable question and holds
/// the task instead of finishing it. No expired process callback is published.
/// With the session saved, the question stays resumable (`native_question_deferred`).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_defers_a_question_with_native_identity() {
    let providers = tempfile::tempdir().unwrap();
    let claude = deferral_fixture(providers.path(), "none");
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    stage_native_transcript(&d, "77777777-7777-4777-8777-777777777777");
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut policy: Value = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
    policy["tool_approval"] = json!("auto_allow");
    let params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "ask",
        policy,
        "G5_CLAUDE_PATH",
        &claude,
    );
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(40), rpc.wait_outcome(&exec))
        .await
        .expect("deferral settles");
    assert_eq!(outcome["result"]["status"], "suspended", "{outcome}");
    assert_eq!(
        outcome["result"]["code"], "native_question_deferred",
        "{outcome}"
    );
    let events = replay_events(&mut rpc, &d, &exec).await;
    assert!(events_of(&events, "module.provider_request.invalidated").is_empty());
    let opened = events_of(&events, "module.provider_request.opened");
    assert_eq!(opened.len(), 1, "{events:?}");
    assert_eq!(opened[0]["payload"]["provider_request_id"], "toolu-held");
    assert_eq!(
        opened[0]["payload"]["resume_session_id"],
        "77777777-7777-4777-8777-777777777777"
    );
    assert!(opened[0]["payload"]["deadline_unix_ms"].is_null());
}

/// A deferral is resumable only while the saved continuation still awaits it.
/// When the process runs another turn after the deferral (the 2.1.308 overlap)
/// or its session is not saved, the question is invalidated and the turn is
/// held for the owner, never left answerable (`g5.chat.native-question-recovery`).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_deferred_question_lost_when_session_cannot_restore_it() {
    for scenario in ["advanced", "not_saved", "advanced_error"] {
        let providers = tempfile::tempdir().unwrap();
        let claude = deferral_fixture(
            providers.path(),
            match scenario {
                "advanced" => "ok",
                "advanced_error" => "error",
                _ => "none",
            },
        );
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        // The advanced processes save their later turn; "not_saved" never
        // publishes a resume file.
        if scenario != "not_saved" {
            stage_native_transcript(&d, "77777777-7777-4777-8777-777777777777");
        }
        let mut rpc = d.connect().await;
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut policy: Value = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
        policy["tool_approval"] = json!("auto_allow");
        let params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "ask",
            policy,
            "G5_CLAUDE_PATH",
            &claude,
        );
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = timeout(Duration::from_secs(40), rpc.wait_outcome(&exec))
            .await
            .expect("deferral settles");
        // A provider error after the advance keeps its own failure; the
        // question is still withdrawn so nothing answerable outlives it.
        let (status, code) = if scenario == "advanced_error" {
            ("failed", "provider_error")
        } else {
            ("suspended", "native_question_lost")
        };
        assert_eq!(outcome["result"]["status"], status, "{scenario}: {outcome}");
        assert_eq!(outcome["result"]["code"], code, "{scenario}: {outcome}");
        let events = replay_events(&mut rpc, &d, &exec).await;
        let opened = events_of(&events, "module.provider_request.opened");
        assert_eq!(
            opened.len(),
            1,
            "{scenario}: the question still opens so the owner sees it"
        );
        let invalidated = events_of(&events, "module.provider_request.invalidated");
        assert_eq!(invalidated.len(), 1, "{scenario}: {events:?}");
        assert_eq!(
            invalidated[0]["payload"]["provider_request_id"],
            "toolu-held"
        );
        assert_eq!(
            invalidated[0]["payload"]["reason"],
            if scenario != "not_saved" {
                "native_session_advanced"
            } else {
                "session_not_saved"
            },
            "{scenario}"
        );
    }
}

/// Native resume sends no replacement prompt, matches the saved tool identity,
/// and settles both the original question and reserved answer with one write fact.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_restores_a_deferred_question_and_sends_the_native_answer() {
    for matching_tool in [true, false] {
        let providers = tempfile::tempdir().unwrap();
        let native_session = Uuid::new_v4().to_string();
        let source = r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$HOME/answer-args"
IFS= read -r sdk_frame
case "$sdk_frame" in *'"subtype":"initialize"'*) ;; *) exit 71;; esac
printf '%s\n' '{"type":"system","subtype":"init","session_id":"__SESSION__","model":"fixture-model","mcp_servers":[]}'
# The pinned CLI may reopen the native callback before acknowledging initialize.
printf '%s\n' '{"type":"control_request","request_id":"fresh-control","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","tool_use_id":"__TOOL__","input":{"questions":[{"question":"Choose original","options":[{"label":"Red"},{"label":"Blue"}]}]}}}'
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"g5-native-question-init","response":{}}}'
IFS= read -r answer_frame
printf '%s\n' "$answer_frame" > "$HOME/answer-frame"
case "$answer_frame" in *'"request_id":"fresh-control"'*'"subtype":"success"'*) ;; *) exit 72;; esac
case "$answer_frame" in *'"Choose original":"Blue"'*) ;; *) exit 73;; esac
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"session_id":"__SESSION__","result":"used Blue","usage":{}}'
cat > "$HOME/remaining-input"
"#.replace("__SESSION__", &native_session).replace("__TOOL__", if matching_tool { "toolu-held" } else { "toolu-wrong" });
        let claude = executable_script(providers.path(), "claude-resume-answer.sh", &source);
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let home = d.root.path().join("home/agent");
        let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
        let relative_path = format!("projects/{key}/{native_session}.jsonl");
        let native_file = home.join(".claude").join(&relative_path);
        std::fs::create_dir_all(native_file.parent().unwrap()).unwrap();
        std::fs::write(&native_file, b"saved native session").unwrap();
        d.origin.as_ref().unwrap().set_published_session_file_bundle(json!({
            "generation":Uuid::new_v4(),"provider":"claude_code","providerSessionId":native_session,
            "files":[{"relativePath":relative_path,"sha256":format!("{:x}",Sha256::digest(b"saved native session")),"size":20}]
        }));
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 2);
        let answer = json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            2,
            "claude_code",
            "continuation",
            "",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        params["input"]["task"]["session_id"] = json!(Uuid::new_v4());
        params["input"]["session_binding"]["provider_session_id"] = json!(native_session);
        params["input"]["resume_prompt"] = params["input"]["prompt"].clone();
        params["input"]["continuation"] =
            json!({"prior_exec_id":d.task_exec_id(task,1),"reason":"suspended"});
        params["input"]["native_question_reply"] = json!({"source_attempt":1,"provider_request_id":"toolu-held",
            "provider_session_id":native_session,"request_id":"owner-answer","response_digest":response_digest(&answer),"validated_response":answer});
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = timeout(Duration::from_secs(40), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        let events = replay_events(&mut rpc, &d, &exec).await;
        let replies = events_of(&events, "module.provider_request.replied");
        if matching_tool {
            assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
            assert_eq!(replies.len(), 1, "{events:?}");
            assert_eq!(replies[0]["payload"]["provider_request_id"], "toolu-held");
            assert_eq!(replies[0]["payload"]["source_attempt"], 1);
            assert!(replies[0]["payload"]["input_acceptance"]["reservation_id"].is_string());
            assert!(events_of(&events, "module.input.accepted").is_empty());
            assert_eq!(
                std::fs::read_to_string(home.join("remaining-input")).unwrap(),
                ""
            );
            let args = std::fs::read_to_string(home.join("answer-args")).unwrap();
            assert!(
                args.contains(&format!("--resume\n{native_session}\n")),
                "{args}"
            );
        } else {
            assert_ne!(outcome["result"]["status"], "succeeded", "{outcome}");
            assert!(replies.is_empty(), "{events:?}");
            assert!(!home.join("answer-frame").exists());
        }
    }
}

/// A follow-up admitted to the process before the question hook keeps the
/// question live: the adapter answers the hook with `{}` instead of deferring,
/// the question arrives as a live request of the running attempt, and both the
/// answer and the follow-up are delivered in the same process. Steering after a
/// deferring reply would be rejected; here no deferral happens at all
/// (`g5.chat.native-question-recovery`, `g5.conversation.delivery.steering`).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_admitted_steering_keeps_the_question_live() {
    // The fixture waits for the steered user frame before opening the hook, so
    // steering is admitted when the adapter decides whether to defer. It then
    // requires a non-deferring reply, opens the live request, takes the answer
    // and reads the follow-up.
    const PROVIDER: &str = r#"#!/usr/bin/env python3
import json, os, pathlib, sys
home = pathlib.Path(os.environ['HOME'])
def emit(frame): print(json.dumps(frame, separators=(',', ':')), flush=True)
initialize = json.loads(sys.stdin.readline())
assert initialize['request']['subtype'] == 'initialize'
emit({'type':'control_response','response':{'subtype':'success','request_id':'g5-native-question-init','response':{}}})
prompt = json.loads(sys.stdin.readline())
emit({'type':'system','subtype':'init','session_id':'88888888-8888-4888-8888-888888888888','model':'fixture-model','mcp_servers':[]})
# Block for the steered follow-up before opening the question.
steered = json.loads(sys.stdin.readline())
(home/'steered.json').write_text(json.dumps(steered))
emit({'type':'control_request','request_id':'hook-live','request':{'subtype':'hook_callback','callback_id':'g5-native-question','input':{'hook_event_name':'PreToolUse','tool_name':'AskUserQuestion','tool_use_id':'toolu-live'}}})
hook_reply = json.loads(sys.stdin.readline())
decision = hook_reply.get('response',{}).get('response',{}).get('hookSpecificOutput',{}).get('permissionDecision')
assert decision != 'defer', hook_reply
emit({'type':'control_request','request_id':'q-live','request':{'subtype':'can_use_tool','tool_use_id':'toolu-live','tool_name':'AskUserQuestion','input':{'questions':[{'question':'Choose','options':[{'label':'Red'},{'label':'Blue'}]}]}}})
answer = json.loads(sys.stdin.readline())
(home/'answer.json').write_text(json.dumps(answer))
emit({'type':'assistant','session_id':'88888888-8888-4888-8888-888888888888','message':{'role':'assistant','content':[{'type':'text','text':steered['message']['content']}]}})
emit({'type':'result','subtype':'success','is_error':False,'session_id':'88888888-8888-4888-8888-888888888888','result':'done','usage':{}})
sys.stdin.read()
"#;
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(providers.path(), "claude-live-question.py", PROVIDER);
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let policy: Value = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "initial",
        policy,
        "G5_CLAUDE_PATH",
        &claude,
    );
    params["input"]["provider"]["options"]["native_mid_turn_steering"] = json!(true);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    timeout(
        Duration::from_secs(15),
        rpc.module_event(&exec, "module.provider.session"),
    )
    .await
    .expect("session starts");
    // Steer a follow-up into the running attempt; the fixture is blocked
    // waiting for it before it opens the question.
    let input = Uuid::new_v4();
    let steer = steering::steer_params(&d, task, input, Uuid::new_v4(), "steered follow-up");
    assert_eq!(
        result(rpc.call("modules.input.steer", steer).await)["disposition"],
        "shipped"
    );
    // The question now opens as a live request; answer it.
    let opened = timeout(
        Duration::from_secs(15),
        rpc.module_event(&exec, "module.provider_request.opened"),
    )
    .await
    .expect("live question opens");
    assert!(
        opened["payload"]["resume_session_id"].is_null(),
        "a live question has no resume session: {opened}"
    );
    let answer = json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}});
    let mut respond = d.params(
        true,
        json!({
            "exec_id": exec, "task_id": task, "attempt": 1,
            "request_id": "owner-live", "provider_request_id": "q-live",
            "response_digest": response_digest(&answer), "validated_response": answer,
        }),
    );
    respond["context"]["idempotency_key"] = json!("owner-live");
    result(rpc.call("modules.input.respond", respond).await);
    let outcome = timeout(Duration::from_secs(25), rpc.wait_outcome(&exec))
        .await
        .expect("the live turn completes");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let events = replay_events(&mut rpc, &d, &exec).await;
    // The answer is delivered as a live reply, not a resume, and the follow-up
    // is shipped in the same process.
    let replied = events_of(&events, "module.provider_request.replied");
    assert_eq!(replied.len(), 1, "{events:?}");
    assert_eq!(replied[0]["payload"]["provider_request_id"], "q-live");
    assert!(replied[0]["payload"]["source_attempt"].is_null());
    assert_eq!(
        events_of(&events, "module.input.shipped").len(),
        1,
        "{events:?}"
    );
}

/// A frozen answer whose resumed session acknowledges initialization but never
/// reissues the question fails that attempt promptly as a restoration failure,
/// not a 300-second setup timeout, keeping the accepted answer for recovery
/// (`g5.chat.native-question-recovery`).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_restore_fails_when_the_session_does_not_reissue_the_question() {
    // "idles": acknowledge initialize, then idle until the restore grace
    // expires. "exits": acknowledge, print an unrelated result and exit 0
    // before any clock runs; a clean exit without the question is the same
    // restoration failure, never a success.
    const IDLES: &str = r#"#!/usr/bin/env python3
import json, sys
initialize = json.loads(sys.stdin.readline())
assert initialize['request']['subtype'] == 'initialize'
print(json.dumps({'type':'system','subtype':'init','session_id':'99999999-9999-4999-8999-999999999999','model':'fixture-model','mcp_servers':[]}), flush=True)
print(json.dumps({'type':'control_response','response':{'subtype':'success','request_id':'g5-native-question-init','response':{}}}), flush=True)
sys.stdin.read()
"#;
    const EXITS: &str = r#"#!/usr/bin/env python3
import json, sys
initialize = json.loads(sys.stdin.readline())
assert initialize['request']['subtype'] == 'initialize'
print(json.dumps({'type':'system','subtype':'init','session_id':'99999999-9999-4999-8999-999999999999','model':'fixture-model','mcp_servers':[]}), flush=True)
print(json.dumps({'type':'control_response','response':{'subtype':'success','request_id':'g5-native-question-init','response':{}}}), flush=True)
print(json.dumps({'type':'result','subtype':'success','is_error':False,'stop_reason':'end_turn','session_id':'99999999-9999-4999-8999-999999999999','result':'nothing to resume','usage':{}}), flush=True)
"#;
    for scenario in ["idles", "exits"] {
        let providers = tempfile::tempdir().unwrap();
        let native_session = "99999999-9999-4999-8999-999999999999";
        let claude = executable_script(
            providers.path(),
            "claude-no-reissue.py",
            if scenario == "idles" { IDLES } else { EXITS },
        );
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let home = d.root.path().join("home/agent");
        let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
        let relative_path = format!("projects/{key}/{native_session}.jsonl");
        let native_file = home.join(".claude").join(&relative_path);
        std::fs::create_dir_all(native_file.parent().unwrap()).unwrap();
        std::fs::write(&native_file, b"saved native session").unwrap();
        d.origin.as_ref().unwrap().set_published_session_file_bundle(json!({
        "generation":Uuid::new_v4(),"provider":"claude_code","providerSessionId":native_session,
        "files":[{"relativePath":relative_path,"sha256":format!("{:x}",Sha256::digest(b"saved native session")),"size":20}]
    }));
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 2);
        let answer = json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            2,
            "claude_code",
            "continuation",
            "",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        params["input"]["task"]["session_id"] = json!(Uuid::new_v4());
        params["input"]["session_binding"]["provider_session_id"] = json!(native_session);
        params["input"]["resume_prompt"] = params["input"]["prompt"].clone();
        params["input"]["continuation"] =
            json!({"prior_exec_id":d.task_exec_id(task,1),"reason":"suspended"});
        params["input"]["native_question_reply"] = json!({"source_attempt":1,"provider_request_id":"toolu-held",
        "provider_session_id":native_session,"request_id":"owner-answer","response_digest":response_digest(&answer),"validated_response":answer});
        // While restoring, both the restore grace and the setup budget resolve to
        // a definite restoration failure, never a plain wall-clock timeout.
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = timeout(Duration::from_secs(60), rpc.wait_outcome(&exec))
            .await
            .expect("restore fails within the grace, not the 300s setup budget");
        assert_eq!(
            outcome["result"]["status"], "failed",
            "{scenario}: {outcome}"
        );
        assert_eq!(
            outcome["result"]["code"], "native_question_restore_failed",
            "{scenario}: {outcome}"
        );
        assert_eq!(
            outcome["result"]["retryable"], false,
            "{scenario}: {outcome}"
        );
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert!(events_of(&events, "module.provider_request.replied").is_empty());
        assert!(
            events_of(&events, "module.notice")
                .iter()
                .any(|notice| notice["payload"]["code"] == "native_question_restore_failed"),
            "{scenario}: the owner is told why the answer was not delivered: {events:?}"
        );
    }
}

/// The journal may redact distinct question texts and option labels to the
/// same strings. Answers must still reach the original provider positions.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_native_questions_correlate_after_redaction() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-redacted-questions.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
printf '%s\n' '{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' '{"type":"control_request","request_id":"provider-q-redacted","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":"Choose fixture-question-secret-one","options":[{"label":"option fixture-question-secret-one"},{"label":"option fixture-question-secret-two"}]},{"question":"Choose fixture-question-secret-two","options":[{"label":"option fixture-question-secret-one"},{"label":"option fixture-question-secret-two"}]}]}}}'
IFS= read -r response
printf '%s\n' "$response" > "$HOME/native-response.json"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"original answers verified","usage":{}}'
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut policy: Value = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
    policy["tool_approval"] = json!("auto_allow");
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "ask two questions",
        policy,
        "G5_CLAUDE_PATH",
        &claude,
    );
    params["secrets"]["HOME"] = json!(providers.path());
    params["secrets"]["G5_TEST_SECRET_ONE"] = json!("fixture-question-secret-one");
    params["secrets"]["G5_TEST_SECRET_TWO"] = json!("fixture-question-secret-two");
    params["input"]["provider"]["policy_snapshot"]["limits"]["native_response_timeout_ms"] =
        json!(1);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let opened = rpc
        .module_event(&exec, "module.provider_request.opened")
        .await;
    assert!(opened["payload"]["deadline_unix_ms"].is_null());
    sleep(Duration::from_millis(30)).await; // Beyond the permission deadline; questions stay open.
    let questions = &opened["payload"]["display"]["questions"];
    assert_eq!(questions[0]["question"], "Choose [REDACTED]");
    assert_eq!(questions[0], questions[1]);
    assert_eq!(questions[0]["options"][0], questions[0]["options"][1]);
    let answer = json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]},"1":{"selectedOptionIndexes":[],"otherText":"custom note"}}});
    let reply = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-redacted", &answer),
        )
        .await,
    );
    assert_eq!(reply["disposition"], "delivered");
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let envelope: Value = serde_json::from_slice(
        &std::fs::read(providers.path().join("native-response.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        envelope["response"]["response"]["updatedInput"]["answers"],
        json!({
            "Choose fixture-question-secret-one": "option fixture-question-secret-two",
            "Choose fixture-question-secret-two": "custom note",
        })
    );
    let events = replay_events(&mut rpc, &d, &exec).await;
    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains("fixture-question-secret-one"));
    assert!(!serialized.contains("fixture-question-secret-two"));
    assert!(events_of(&events, "module.provider_request.replied")
        .iter()
        .any(|event| event["payload"]["evidence"] == "transport_write"));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_claude_native_question_and_permission_round_trips() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-native-fixture.sh",
        r#"#!/bin/sh
set -eu
[ -z "${LD_LIBRARY_PATH-}" ] && [ -z "${DYLD_LIBRARY_PATH-}" ] || exit 44
IFS= read -r initial
session_id="11111111-1111-4111-8111-111111111111"
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$session_id\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
case "$initial" in
  *permission-allow*)
    request_id="provider-q-allow"
    request_kind="permission-allow"
    printf '%s\n' "{\"type\":\"control_request\",\"request_id\":\"$request_id\",\"request\":{\"subtype\":\"can_use_tool\",\"tool_name\":\"Bash\",\"input\":{\"command\":\"touch allowed\"}}}"
    ;;
  *permission-deny*)
    request_id="provider-q-deny"
    request_kind="permission-deny"
    printf '%s\n' "{\"type\":\"control_request\",\"request_id\":\"$request_id\",\"request\":{\"subtype\":\"can_use_tool\",\"tool_name\":\"Bash\",\"input\":{\"command\":\"touch denied\"}}}"
    ;;
  *)
    request_id="provider-q-question"
    request_kind="question"
    printf '%s\n' "{\"type\":\"control_request\",\"request_id\":\"$request_id\",\"request\":{\"subtype\":\"can_use_tool\",\"tool_name\":\"AskUserQuestion\",\"input\":{\"questions\":[{\"question\":\"Choose\",\"options\":[{\"label\":\"A\"},{\"label\":\"B\"}]}]}}}"
    ;;
esac
IFS= read -r response
case "$response" in
  *"\"request_id\":\"$request_id\""*) ;;
  *) exit 41 ;;
esac
case "$request_kind:$response" in
  question:*'"answers"'*) ;;
  permission-allow:*'"behavior":"allow"'*) ;;
  permission-deny:*'"behavior":"deny"'*) ;;
  *) exit 42 ;;
esac
printf '%s\n' "{\"type\":\"assistant\",\"session_id\":\"$session_id\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"native response received\"}]}}"
printf '%s\n' "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"session_id\":\"$session_id\",\"result\":\"native response received\",\"usage\":{}}"
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: loader_env_with_sentinel(),
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({
        "schema_version": "g5.agent.permission-policy/v1",
        "provider": "claude_code",
        "questions": "enabled",
        "tool_approval": "prompt",
        "sandbox": "none",
        "allowed_tools": [],
        "limits": {
            "setup_timeout_ms": 20_000,
            "max_output_bytes": 1024 * 1024,
            "native_response_timeout_ms": 5_000,
        },
    });
    let cases = [
        (
            "question",
            "q-question",
            json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[1]}}}),
            "answered",
        ),
        (
            "bypass-question",
            "q-question",
            json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[],"otherText":"A custom answer"}}}),
            "answered",
        ),
        (
            "permission-allow",
            "q-allow",
            json!({"behavior":"allow"}),
            "allowed",
        ),
        (
            "permission-deny",
            "q-deny",
            json!({"behavior":"deny","message":"not now"}),
            "denied",
        ),
        (
            "bypass-permission-deny",
            "q-deny",
            json!({"behavior":"deny","message":"not now"}),
            "denied",
        ),
    ];

    for (prompt, request_id, answer, expected_disposition) in cases {
        let mut policy = policy.clone();
        if prompt.starts_with("bypass-") {
            policy["tool_approval"] = json!("auto_allow");
        }
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let started = result(
            rpc.call(
                "modules.invoke",
                agent_turn_invoke(
                    &d,
                    task,
                    1,
                    "claude_code",
                    "human",
                    prompt,
                    policy.clone(),
                    "G5_CLAUDE_PATH",
                    &claude,
                ),
            )
            .await,
        );
        assert_eq!(started["disposition"], "started");
        let opened = rpc
            .module_event(&exec, "module.provider_request.opened")
            .await;
        assert_eq!(
            opened["payload"]["provider_request_id"],
            format!("provider-{request_id}")
        );
        assert_eq!(
            opened["payload"]["request_kind"],
            if prompt.ends_with("question") {
                "question"
            } else {
                "permission"
            }
        );
        let delivered = result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, task, 1, request_id, &answer),
            )
            .await,
        );
        assert_eq!(delivered["disposition"], "delivered");
        let replied = rpc
            .module_event(&exec, "module.provider_request.replied")
            .await;
        assert_eq!(replied["payload"]["disposition"], expected_disposition);
        assert_eq!(replied["payload"]["evidence"], "transport_write");
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["status"], "succeeded", "{outcome}");
        assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
        assert_eq!(outcome["result"]["code"], "exit_code", "{outcome}");

        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        assert_eq!(
            replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["kind"] == "module.provider_request.replied")
                .count(),
            1,
            "{replay}"
        );
        assert_eq!(
            replay["events"].as_array().unwrap().last().unwrap()["terminal"],
            true
        );
    }
}

/// A permission request whose input is larger than the API stores (a big
/// Write) is journaled with its payload and display cut to the API's bound,
/// beside the digest of the full input, so the owner can answer it. A plain
/// Allow then hands the provider its complete original input, not the cut
/// copy (`g5.agents.output-bounds`).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_answers_a_permission_request_too_large_to_store_with_its_full_input() {
    let providers = tempfile::tempdir().unwrap();
    let response_file = providers.path().join("response.json");
    let claude = executable_script(
        providers.path(),
        "claude-big-permission.sh",
        &format!(
            r#"#!/bin/sh
set -eu
IFS= read -r initial
session_id="11111111-1111-4111-8111-111111111111"
printf '%s\n' "{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$session_id\",\"model\":\"fixture-model\",\"mcp_servers\":[]}}"
content="$(head -c 200000 /dev/zero | tr '\0' w)"
printf '%s\n' "{{\"type\":\"control_request\",\"request_id\":\"provider-q-big\",\"request\":{{\"subtype\":\"can_use_tool\",\"tool_name\":\"Write\",\"input\":{{\"file_path\":\"/work/big.txt\",\"content\":\"$content\"}}}}}}"
IFS= read -r response
printf '%s\n' "$response" > '{response}'
printf '%s\n' "{{\"type\":\"assistant\",\"session_id\":\"$session_id\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"wrote it\"}}]}}}}"
printf '%s\n' "{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"session_id\":\"$session_id\",\"result\":\"wrote it\",\"usage\":{{}}}}"
"#,
            response = response_file.display()
        ),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let started = result(
        rpc.call(
            "modules.invoke",
            agent_turn_invoke(
                &d,
                task,
                1,
                "claude_code",
                "human",
                "write a big file",
                serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
                "G5_CLAUDE_PATH",
                &claude,
            ),
        )
        .await,
    );
    assert_eq!(started["disposition"], "started");
    let opened = rpc
        .module_event(&exec, "module.provider_request.opened")
        .await;
    let full_input = json!({"file_path": "/work/big.txt", "content": "w".repeat(200_000)});
    let payload = &opened["payload"];
    assert_eq!(payload["request_kind"], "permission");
    assert_eq!(payload["tool_name"], "Write");
    assert_eq!(payload["payload_truncated"], true);
    assert_eq!(
        payload["payload_ref"],
        format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&full_input).unwrap())
        ),
        "the digest names the full input"
    );
    assert_eq!(payload["payload"]["file_path"], "/work/big.txt");
    assert_eq!(payload["display"]["input"]["file_path"], "/work/big.txt");
    // Within the API's per-field bound (`MAX_NATIVE_REQUEST_BYTES`).
    for field in ["payload", "display"] {
        let len = serde_json::to_vec(&payload[field]).unwrap().len();
        assert!(len <= 64 * 1024, "{field}: {len}");
    }

    let delivered = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-big", &json!({"behavior":"allow"})),
        )
        .await,
    );
    assert_eq!(delivered["disposition"], "delivered");
    let replied = rpc
        .module_event(&exec, "module.provider_request.replied")
        .await;
    assert_eq!(replied["payload"]["disposition"], "allowed");
    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
        .await
        .expect("the turn settles");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let response: Value =
        serde_json::from_str(std::fs::read_to_string(&response_file).unwrap().trim()).unwrap();
    assert_eq!(response["response"]["request_id"], "provider-q-big");
    assert_eq!(response["response"]["response"]["behavior"], "allow");
    assert_eq!(
        response["response"]["response"]["updatedInput"], full_input,
        "the provider receives its complete input, not the journaled copy"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_malformed_frame_and_early_exit_settle() {
    let providers = tempfile::tempdir().unwrap();
    let provider = executable_script(
        providers.path(),
        "claude-malformed-or-exit.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
case "$initial" in
  *early-exit*) exit 9 ;;
  *max-turns*)
    case " $* " in *" --max-turns 2 "*) ;; *) exit 8 ;; esac
    printf '%s\n' '{"type":"result","subtype":"error_max_turns","is_error":true,"result":"Reached the turn budget","usage":{}}'
    ;;
  *)
    printf '%s\n' '{broken'
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"continued after malformed frame","usage":{}}'
    ;;
esac
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    for (prompt, expected_status, expected_code) in [
        ("malformed-frame", "succeeded", "exit_code"),
        ("early-exit", "failed", "provider_error"),
        ("max-turns", "succeeded", "max_turns_reached"),
        ("policy-only-max-turns", "succeeded", "max_turns_reached"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            policy.clone(),
            "G5_CLAUDE_PATH",
            &provider,
        );
        if prompt.contains("max-turns") {
            params["input"]["provider"]["policy_snapshot"]["limits"]["max_turns"] = json!(2);
            if prompt == "max-turns" {
                params["input"]["limits"]["max_turns"] = json!(2);
            }
        }
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = timeout(Duration::from_secs(10), rpc.wait_outcome(&exec))
            .await
            .expect("malformed or exited provider settles");
        assert_eq!(outcome["result"]["status"], expected_status, "{outcome}");
        assert_eq!(outcome["result"]["code"], expected_code, "{outcome}");
        if prompt == "malformed-frame" {
            let replay = result(
                rpc.call(
                    "events.replay",
                    d.params(true, json!({"exec_id":exec,"from_seq":1})),
                )
                .await,
            );
            assert!(replay["events"].as_array().unwrap().iter().any(|event| {
                event["kind"] == "module.notice"
                    && event["payload"]["code"] == "provider_non_json_frame"
            }));
        }
    }
}

/// Every journaled event of `exec`, read page by page: `(count, payload
/// bytes)` per kind, with each notice under `module.notice:<code>`. A turn's
/// journal can be larger than one replay page or than is worth holding in
/// the test.
async fn journaled_event_sizes(
    rpc: &mut g5d::dispatch_test_support::Rpc,
    d: &TestDispatcher,
    exec: &str,
) -> BTreeMap<String, (usize, usize)> {
    let mut sizes: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut from_seq = 1u64;
    loop {
        let page = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":from_seq})),
            )
            .await,
        );
        for event in page["events"].as_array().unwrap() {
            let mut kind = event["kind"].as_str().unwrap().to_owned();
            if kind == "module.notice" {
                kind = format!("{kind}:{}", event["payload"]["code"].as_str().unwrap());
            }
            let entry = sizes.entry(kind).or_default();
            entry.0 += 1;
            entry.1 += serde_json::to_vec(&event["payload"]).unwrap().len();
        }
        if page["has_more"] != true {
            return sizes;
        }
        from_seq = page["next_seq"].as_u64().unwrap();
    }
}

/// The retired limit as the API still sends it: 64 MiB in the request and in
/// the frozen policy (`g5.agents.output-bounds`).
const RETIRED_OUTPUT_LIMIT: u64 = 64 * 1024 * 1024;

fn claude_auto_deny_policy(max_output_bytes: Option<u64>) -> Value {
    let mut policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000}});
    if let Some(bytes) = max_output_bytes {
        policy["limits"]["max_output_bytes"] = json!(bytes);
    }
    policy
}

/// The QA failure of #1161: a long turn whose background sub-agents print far
/// more than the turn records. Their frames, and their streaming partials,
/// are ignored by the adapter and used to be charged at full size, so the
/// turn ended `limit_exceeded` / `max_output_bytes` with its session unsaved.
/// A turn has no cumulative output limit (`g5.agents.output-bounds`): more
/// than 64 MiB of ignored frames, under the limit the API still sends, leaves
/// a turn that succeeds, records little and saves its session files.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_ignoring_over_64_mib_of_background_agent_frames_succeeds_and_saves_its_session()
{
    const SESSION: &str = "99999999-9999-4999-8999-999999999999";
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-background-agents.py",
        r#"#!/usr/bin/env python3
import json, sys
out = sys.stdout
def emit(frame):
    out.write(json.dumps(frame, separators=(',', ':')) + '\n')
    out.flush()
sys.stdin.readline()
init = {'type':'system','subtype':'init','session_id':'99999999-9999-4999-8999-999999999999','model':'fixture-model','mcp_servers':[]}
emit(init)
emit({'type':'assistant','message':{'id':'msg-1','role':'assistant','content':[{'type':'tool_use','id':'toolu_agent','name':'Agent','input':{'description':'survey','run_in_background':True}}]}})
emit({'type':'system','subtype':'task_started','task_id':'ag1','tool_use_id':'toolu_agent','description':'survey','task_type':'local_agent','is_backgrounded':True})
emit({'type':'system','subtype':'background_tasks_changed','tasks':[{'task_id':'ag1','task_type':'local_agent','description':'survey'}]})
emit({'type':'user','message':{'role':'user','content':[{'type':'tool_result','tool_use_id':'toolu_agent','content':'launched'}]}})
emit({'type':'result','subtype':'success','is_error':False,'result':'launched','usage':{}})
# The background sub-agent's own transcript and streaming partials: nothing
# of it is recorded for the turn.
body = 'x' * (512 * 1024)
sent = 0
n = 0
while sent <= 66 * 1024 * 1024:
    for frame in (
        {'type':'stream_event','parent_tool_use_id':'toolu_agent','event':{'type':'content_block_delta','index':0,'delta':{'type':'text_delta','text':body[:4096]}}},
        {'type':'assistant','parent_tool_use_id':'toolu_agent','message':{'id':'sub-%d' % n,'role':'assistant','content':[{'type':'text','text':body}]}},
    ):
        line = json.dumps(frame, separators=(',', ':')) + '\n'
        out.write(line)
        sent += len(line)
    n += 1
out.flush()
emit({'type':'system','subtype':'background_tasks_changed','tasks':[]})
emit({'type':'system','subtype':'task_notification','task_id':'ag1','tool_use_id':'toolu_agent','status':'completed','summary':'survey done'})
emit(init)
emit({'type':'assistant','message':{'id':'msg-2','role':'assistant','content':[{'type':'text','text':'the survey is done after %d ignored bytes' % sent}]}})
emit({'type':'result','subtype':'success','is_error':False,'result':'the survey is done after %d ignored bytes' % sent,'usage':{}})
sys.stdin.read()
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    // The session file Claude would have written for this session.
    let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
    let relative_path = format!("projects/{key}/{SESSION}.jsonl");
    let native_file = d
        .root
        .path()
        .join("home/agent/.claude")
        .join(&relative_path);
    std::fs::create_dir_all(native_file.parent().unwrap()).unwrap();
    std::fs::write(&native_file, b"long turn native session").unwrap();
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "survey the repository with a background agent",
        claude_auto_deny_policy(Some(RETIRED_OUTPUT_LIMIT)),
        "G5_CLAUDE_PATH",
        &claude,
    );
    params["input"]["limits"]["max_output_bytes"] = json!(RETIRED_OUTPUT_LIMIT);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(300), rpc.wait_outcome(&exec))
        .await
        .expect("the turn settles after its report turn");
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["code"], "exit_code", "{outcome}");
    assert_eq!(
        outcome["result"]["session_files_state"], "present",
        "{outcome}"
    );
    let publication = d
        .origin
        .as_ref()
        .unwrap()
        .published_session_file_bundle()
        .expect("the turn's session files were published");
    assert_eq!(publication["providerSessionId"], SESSION, "{publication}");
    assert_eq!(
        publication["files"][0]["sha256"],
        format!("{:x}", Sha256::digest(b"long turn native session")),
        "{publication}"
    );

    let events = replay_events(&mut rpc, &d, &exec).await;
    let reply = events_of(&events, "module.message.final")
        .last()
        .map(|event| event["payload"]["content"].as_str().unwrap().to_owned())
        .unwrap_or_else(|| panic!("the report turn's reply is recorded: {events:?}"));
    let ignored: u64 = reply
        .strip_prefix("the survey is done after ")
        .and_then(|rest| rest.strip_suffix(" ignored bytes"))
        .and_then(|bytes| bytes.parse().ok())
        .unwrap_or_else(|| panic!("the report names the ignored bytes: {reply}"));
    assert!(
        ignored > RETIRED_OUTPUT_LIMIT,
        "the ignored frames pass the retired limit: {ignored}"
    );
    let recorded: usize = events
        .iter()
        .map(|event| serde_json::to_vec(&event["payload"]).unwrap().len())
        .sum();
    assert!(
        recorded < 64 * 1024,
        "nothing of the sub-agent's output is recorded ({recorded} bytes)"
    );
    assert!(
        !events_of(&events, "module.notice").iter().any(|notice| {
            matches!(
                notice["payload"]["code"].as_str(),
                Some("provider_line_skipped" | "provider_non_json_frame")
            ) || notice["payload"]["level"] != "info"
        }),
        "an ignored frame is ordinary output: {events:?}"
    );
    assert_eq!(
        events_of(&events, "module.subtask.completed").len(),
        1,
        "{events:?}"
    );
}

/// A turn that records more than 64 MiB of events completes: every event is
/// written and journaled, with or without the retired limit in its request
/// (`g5.agents.output-bounds`). Each tool result is still trimmed to its
/// inline bound, so the turn's size comes from their number.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_recording_over_64_mib_of_events_completes_with_or_without_the_retired_limit() {
    const RESULTS: usize = 1100;
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-many-results.py",
        r#"#!/usr/bin/env python3
import json, sys
out = sys.stdout
def emit(frame): out.write(json.dumps(frame, separators=(',', ':')) + '\n')
initial = sys.stdin.readline()
results = 1100 if 'whole log' in initial else 3
emit({'type':'system','subtype':'init','session_id':'88888888-8888-4888-8888-888888888888','model':'fixture-model','mcp_servers':[]})
body = 'r' * 40000
for n in range(results):
    emit({'type':'assistant','message':{'id':'msg-%d' % n,'role':'assistant','content':[{'type':'tool_use','id':'toolu_%d' % n,'name':'Bash','input':{'command':'sed -n %dp big.log' % n}}]}})
    emit({'type':'user','message':{'role':'user','content':[{'type':'tool_result','tool_use_id':'toolu_%d' % n,'content':body}]}})
emit({'type':'assistant','message':{'id':'msg-done','role':'assistant','content':[{'type':'text','text':'read %d results' % results}]}})
emit({'type':'result','subtype':'success','is_error':False,'result':'read %d results' % results,'usage':{}})
out.flush()
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    // A turn from an API that still sends the limit records past it, and a
    // turn without the field is accepted alike; so are the values an older
    // worker refused, a request above its policy and zero.
    for (prompt, request, policy, results) in [
        (
            "read the whole log",
            Some(RETIRED_OUTPUT_LIMIT),
            Some(RETIRED_OUTPUT_LIMIT),
            RESULTS,
        ),
        ("read three lines", None, None, 3),
        ("read three lines", Some(2048), Some(1024), 3),
        ("read three lines", Some(0), Some(0), 3),
    ] {
        let case = format!("{prompt} request={request:?} policy={policy:?}");
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            claude_auto_deny_policy(policy),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let limits = params["input"]["limits"].as_object_mut().unwrap();
        match request {
            Some(bytes) => limits.insert("max_output_bytes".into(), json!(bytes)),
            None => limits.remove("max_output_bytes"),
        };
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{case}"
        );
        // Generous: an unoptimized dispatcher journals a few megabytes a
        // second.
        let outcome = timeout(Duration::from_secs(600), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{case}: the turn settles"));
        assert_eq!(outcome["status"], "succeeded", "{case}: {outcome}");
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{case}: {outcome}"
        );
        assert_eq!(outcome["result"]["code"], "exit_code", "{case}: {outcome}");
        // The live stream of a large turn is not needed again.
        drop(rpc);
        rpc = d.connect().await;
        let sizes = journaled_event_sizes(&mut rpc, &d, &exec).await;
        assert_eq!(
            sizes.get("module.tool.completed").map(|size| size.0),
            Some(results),
            "{case}: every tool result is journaled: {sizes:?}"
        );
        assert_eq!(
            sizes.get("module.message.final").map(|size| size.0),
            Some(1),
            "{case}: {sizes:?}"
        );
        assert!(
            !["provider_line_skipped", "provider_non_json_frame"]
                .iter()
                .any(|code| sizes.contains_key(&format!("module.notice:{code}"))),
            "{case}: nothing was skipped or cut short: {sizes:?}"
        );
        if results == RESULTS {
            let recorded: usize = sizes.values().map(|size| size.1).sum();
            assert!(
                recorded as u64 > RETIRED_OUTPUT_LIMIT,
                "{case}: the turn recorded {recorded} bytes of events"
            );
        }
    }
}

/// A runaway turn is ended by the owner, not by an output limit
/// (`g5.agents.output-bounds`): a provider that keeps producing recorded
/// events, frames the adapter ignores, or one endless line runs on past the
/// retired limit its request still carries, and Stop then settles it with
/// what it recorded.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_floods_run_until_the_owner_stops_them() {
    let providers = tempfile::tempdir().unwrap();
    let provider = executable_script(
        providers.path(),
        "claude-floods.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
printf '%s\n' '{"type":"assistant","message":{"id":"kept","role":"assistant","content":[{"type":"text","text":"partial"}]}}'
case "$initial" in
  *newline-free-flood*) yes | tr -d '\n' ;;
  *ignored-frame-flood*)
    i=0
    while :; do
      printf '%s\n' '{"type":"stream_event","event":{"type":"ping"}}'
      i=$((i + 1))
      # Well past the retired 64 KiB: about 3 MB of ignored frames.
      if [ "$i" -eq 60000 ]; then
        printf '%s\n' '{"type":"assistant","message":{"id":"later","role":"assistant","content":[{"type":"text","text":"still running"}]}}'
      fi
    done
    ;;
  *)
    body="$(head -c 3000 /dev/zero | tr '\0' x)"
    while :; do
      printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_f","content":"'"$body"'"}]}}'
    done
    ;;
esac
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    for prompt in ["event-flood", "newline-free-flood", "ignored-frame-flood"] {
        let mut rpc = d.connect().await;
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            claude_auto_deny_policy(Some(2_000_000)),
            "G5_CLAUDE_PATH",
            &provider,
        );
        params["input"]["limits"]["max_output_bytes"] = json!(64 * 1024);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        // Each flood is observed past the 64 KiB its request still names.
        match prompt {
            "event-flood" => {
                // Forty 3 KB tool results are twice that.
                for _ in 0..40 {
                    timeout(
                        Duration::from_secs(120),
                        rpc.module_event(&exec, "module.tool.completed"),
                    )
                    .await
                    .unwrap_or_else(|_| panic!("{prompt}: recording goes on"));
                }
            }
            "newline-free-flood" => {
                // The line is dropped once it passes the 16 MiB ceiling.
                timeout(
                    Duration::from_secs(120),
                    rpc.event_where("module.event", |params| {
                        params["exec_id"] == exec.as_str()
                            && params["kind"] == "module.notice"
                            && params["payload"]["code"] == "provider_line_skipped"
                    }),
                )
                .await
                .unwrap_or_else(|_| panic!("{prompt}: the dropped line is noted"));
            }
            _ => {
                timeout(
                    Duration::from_secs(120),
                    rpc.event_where("module.event", |params| {
                        params["exec_id"] == exec.as_str()
                            && params["kind"] == "module.message.final"
                            && params["payload"]["content"] == "still running"
                    }),
                )
                .await
                .unwrap_or_else(|_| panic!("{prompt}: the turn reads on past ignored frames"));
            }
        }
        assert_eq!(
            result(
                rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
                    .await
            )["status"],
            "running",
            "{prompt}"
        );
        result(
            rpc.call(
                "modules.cancel",
                d.params(true, json!({"exec_id":exec,"reason":"owner_stop"})),
            )
            .await,
        );
        let outcome = timeout(Duration::from_secs(120), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{prompt}: Stop ends the flood"));
        assert_eq!(outcome["status"], "cancelled", "{prompt}: {outcome}");
        drop(rpc);
        let mut rpc = d.connect().await;
        let sizes = journaled_event_sizes(&mut rpc, &d, &exec).await;
        assert!(
            sizes.contains_key("module.message.final"),
            "{prompt}: what the turn recorded is retained: {sizes:?}"
        );
        if prompt == "event-flood" {
            assert!(
                sizes["module.tool.completed"].1 > 64 * 1024,
                "{prompt}: recording passed the retired limit: {sizes:?}"
            );
        }
    }
}

/// The control-reply queue has its own fixed bound
/// (`g5.agents.output-bounds`). A provider that floods control requests the
/// runtime rejects, each echoing a 100 KiB request id, and never reads the
/// replies, cannot make the supervisor hold them: at the reply that would
/// take the waiting replies past 1 MiB, the turn ends as a provider error,
/// after a handful of the requests rather than the provider's whole flood. Question hooks, which
/// are answered rather than rejected, are bounded the same way.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_flood_of_unread_control_replies_ends_as_a_provider_error() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-control-flood.py",
        r#"#!/usr/bin/env python3
import json, os, pathlib, sys
home = pathlib.Path(os.environ['HOME'])
out = sys.stdout
def emit(frame):
    out.write(json.dumps(frame, separators=(',', ':')) + '\n')
    out.flush()
# Only the prompt (and, with questions enabled, the initialize request before
# it) is ever read: no reply is.
initial = sys.stdin.readline()
while '"type":"user"' not in initial:
    initial = sys.stdin.readline()
emit({'type':'system','subtype':'init','session_id':'66666666-6666-4666-8666-666666666666','model':'fixture-model','mcp_servers':[]})
emit({'type':'assistant','message':{'id':'kept','role':'assistant','content':[{'type':'text','text':'partial'}]}})
# Longer than the pipe to the provider's input holds, so no reply is ever
# completely written.
pad = 'i' * (100 * 1024)
sent = 0
while True:
    request_id = '%s-%d' % (pad, sent)
    if 'question-hooks' in initial:
        request = {'subtype':'hook_callback','callback_id':'g5-native-question','input':{'hook_event_name':'PreToolUse','tool_name':'AskUserQuestion'}}
    else:
        request = {'subtype':'set_permission_mode','mode':'acceptEdits'}
    # Counted before it is sent and replaced whole: the provider is stopped
    # at an arbitrary point after the request that overruns the bound.
    sent += 1
    (home/'sent.tmp').write_text(str(sent))
    os.replace(home/'sent.tmp', home/'sent')
    emit({'type':'control_request','request_id':request_id,'request':request})
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for prompt in ["unsupported-requests", "question-hooks"] {
        let home = providers.path().join(prompt);
        std::fs::create_dir(&home).unwrap();
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        params["secrets"]["HOME"] = json!(home);
        // Far beyond the test: only the queue's bound can end this turn.
        params["input"]["limits"]["setup_timeout_ms"] = json!(120_000);
        params["input"]["provider"]["policy_snapshot"]["limits"]["setup_timeout_ms"] =
            json!(120_000);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{prompt}"
        );
        // Without the bound, the write-stall rule would close the input
        // after ten seconds and hold the task for the owner (`suspended` /
        // `native_response_undeliverable`); the outcome below tells them
        // apart, so the deadline is only a hang guard.
        let outcome = timeout(Duration::from_secs(120), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{prompt}: the reply queue's bound ends the turn"));
        assert_eq!(outcome["status"], "succeeded", "{prompt}: {outcome}");
        assert_eq!(outcome["result"]["status"], "failed", "{prompt}: {outcome}");
        assert_eq!(
            outcome["result"]["code"], "provider_error",
            "{prompt}: {outcome}"
        );
        let events = replay_events(&mut rpc, &d, &exec).await;
        let notices = events_of(&events, "module.notice");
        assert_eq!(
            notices
                .iter()
                .filter(|notice| notice["payload"]["code"] == "provider_control_backlog")
                .count(),
            1,
            "{prompt}: {notices:?}"
        );
        assert!(
            !notices
                .iter()
                .any(|notice| notice["payload"]["code"] == "provider_start_failed"),
            "{prompt}: the backlog notice is the turn's one explanation: {notices:?}"
        );
        // 100 KiB of request id in each reply: ten fit the bound and the
        // eleventh overruns it. The provider is stopped there, so it began
        // at most a few requests beyond it.
        let sent: usize = std::fs::read_to_string(home.join("sent"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            (11..=64).contains(&sent),
            "{prompt}: the provider was stopped at the bound, after {sent} requests"
        );
        if prompt == "unsupported-requests" {
            let rejected = notices
                .iter()
                .filter(|notice| notice["payload"]["code"] == "unsupported_provider_control_frame")
                .count();
            assert!(
                (11..=sent).contains(&rejected),
                "{prompt}: the supervisor handled {rejected} of the {sent} requests begun"
            );
        }
        assert!(
            events_of(&events, "module.message.final")
                .iter()
                .any(|event| event["payload"]["content"] == "partial"),
            "{prompt}: output recorded before the flood is retained: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|event| event.to_string().len() > 64 * 1024),
            "{prompt}: no event echoes a request id"
        );
    }
}

/// The 2026-10-12 QA failure shape: Codex caps a command's output near 1 MiB
/// before JSON escaping, so its `item.completed` line is over 1 MiB. The line
/// is parsed and the result recorded as a trimmed `tool.completed` beside the
/// digest of the full output; the turn succeeds, under a 1 MiB output limit.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_records_a_trimmed_tool_result_from_a_codex_line_over_1_mib() {
    let providers = tempfile::tempdir().unwrap();
    let provider = codex_app_server_fixture(
        providers.path(),
        "codex-big-result.sh",
        "",
        json!({"items":[
            {"id":"item_0","type":"commandExecution","command":"cat big.log",
             "aggregatedOutput":"x".repeat(1_250_000),"exitCode":0,"status":"completed"},
            {"id":"message-1","type":"agentMessage","text":"read the log"},
        ]}),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let params = agent_turn_invoke(
        &d,
        task,
        1,
        "codex",
        "human",
        "show me the log",
        policy,
        "G5_CODEX_PATH",
        &provider,
    );
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
        .await
        .expect("the turn settles");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    let events = replay["events"].as_array().unwrap();
    let completed = events
        .iter()
        .find(|event| event["kind"] == "module.tool.completed")
        .unwrap_or_else(|| panic!("the trimmed tool result is journaled: {replay}"));
    let full = json!("x".repeat(1_250_000));
    assert_eq!(completed["payload"]["call_id"], "item_0");
    assert_eq!(completed["payload"]["status"], "ok");
    assert_eq!(completed["payload"]["output_truncated"], true);
    assert_eq!(
        completed["payload"]["output_ref"],
        format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&full).unwrap())
        ),
        "the digest names the full output"
    );
    let output = completed["payload"]["output"].as_str().unwrap();
    assert!(output.len() <= 32 * 1024, "{}", output.len());
    assert!(output.starts_with("xxxx"));
    assert!(
        events.iter().any(|event| {
            event["kind"] == "module.message.final" && event["payload"]["content"] == "read the log"
        }),
        "{replay}"
    );
    assert!(
        !events.iter().any(|event| event["kind"] == "module.notice"
            && matches!(
                event["payload"]["code"].as_str(),
                Some("provider_line_skipped" | "provider_non_json_frame")
            )),
        "a line over 1 MiB is ordinary output: {replay}"
    );
}

/// Claude lines over 1 MiB (an image read carries the image twice) are
/// recorded like any other; only a line past the 16 MiB ceiling is dropped,
/// with a notice, and the turn goes on. A final message longer than its
/// recorded bound is cut beside the digest of the full text.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_records_claude_lines_over_1_mib_and_skips_lines_over_the_ceiling() {
    let providers = tempfile::tempdir().unwrap();
    let provider = executable_script(
        providers.path(),
        "claude-long-lines.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
case "$initial" in
  *image-read*)
    printf '%s\n' '{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"tool_use","id":"toolu_img","name":"Read","input":{"file_path":"/work/shot.png"}}]}}'
    data="$(head -c 400000 /dev/zero | base64 | tr -d '\n')"
    printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_img","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"'"$data"'"}}]}]},"tool_use_result":{"type":"image","file":{"base64":"'"$data"'","type":"image/png"}}}'
    printf '%s\n' '{"type":"assistant","message":{"id":"msg_2","role":"assistant","content":[{"type":"text","text":"a screenshot"}]}}'
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"a screenshot","usage":{}}'
    ;;
  *over-ceiling*)
    printf '%s\n' '{"type":"assistant","message":{"id":"msg_a","role":"assistant","content":[{"type":"text","text":"before"}]}}'
    head -c 17000000 /dev/zero | tr '\0' x
    printf '\n'
    printf '%s\n' '{"type":"assistant","message":{"id":"msg_b","role":"assistant","content":[{"type":"text","text":"after"}]}}'
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"after","usage":{}}'
    ;;
  *long-final*)
    printf '%s' '{"type":"assistant","message":{"id":"msg_long","role":"assistant","content":[{"type":"text","text":"'
    head -c 300000 /dev/zero | tr '\0' y
    printf '%s\n' '"}]}}'
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"done","usage":{}}'
    ;;
esac
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":67108864}});
    for prompt in ["image-read", "over-ceiling", "long-final"] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            policy.clone(),
            "G5_CLAUDE_PATH",
            &provider,
        );
        // The API default: the discarded 17 MB line counts against it.
        params["input"]["limits"]["max_output_bytes"] = json!(67108864);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{prompt}: the turn settles"));
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{prompt}: {outcome}"
        );
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        let events = replay["events"].as_array().unwrap();
        let finals: Vec<&Value> = events
            .iter()
            .filter(|event| event["kind"] == "module.message.final")
            .collect();
        let notices: Vec<&Value> = events
            .iter()
            .filter(|event| event["kind"] == "module.notice")
            .collect();
        assert!(
            !notices
                .iter()
                .any(|notice| notice["payload"]["code"] == "provider_non_json_frame"),
            "{prompt}: no line is parsed in part: {replay}"
        );
        match prompt {
            "image-read" => {
                let completed = events
                    .iter()
                    .find(|event| event["kind"] == "module.tool.completed")
                    .unwrap_or_else(|| panic!("{prompt}: the image read is journaled: {replay}"));
                assert_eq!(completed["payload"]["call_id"], "toolu_img");
                assert_eq!(completed["payload"]["name"], "Read");
                assert_eq!(completed["payload"]["content"], "[image]");
                assert_eq!(completed["payload"]["output_truncated"], false);
                assert_eq!(finals.len(), 1, "{replay}");
                assert_eq!(finals[0]["payload"]["content"], "a screenshot");
                assert!(
                    !notices
                        .iter()
                        .any(|notice| notice["payload"]["code"] == "provider_line_skipped"),
                    "{prompt}: a 1 MiB line is ordinary output: {replay}"
                );
            }
            "over-ceiling" => {
                assert_eq!(finals.len(), 2, "{replay}");
                assert_eq!(finals[0]["payload"]["content"], "before");
                assert_eq!(finals[1]["payload"]["content"], "after");
                let skipped = notices
                    .iter()
                    .find(|notice| notice["payload"]["code"] == "provider_line_skipped")
                    .unwrap_or_else(|| panic!("{prompt}: the dropped line is noted: {replay}"));
                assert_eq!(skipped["payload"]["level"], "warning");
                assert!(
                    skipped["seq"].as_u64() > finals[0]["seq"].as_u64()
                        && skipped["seq"].as_u64() < finals[1]["seq"].as_u64(),
                    "{prompt}: the notice sits where the line was: {replay}"
                );
            }
            _ => {
                let long = finals
                    .iter()
                    .find(|event| event["payload"]["block_id"] == "msg_long:0")
                    .unwrap_or_else(|| panic!("{prompt}: the long text is journaled: {replay}"));
                let full = "y".repeat(300_000);
                assert_eq!(long["payload"]["content_truncated"], true);
                assert_eq!(
                    long["payload"]["content_ref"],
                    format!("sha256:{:x}", Sha256::digest(full.as_bytes())),
                    "the digest names the full text"
                );
                let content = long["payload"]["content"].as_str().unwrap();
                assert!(content.len() <= 256 * 1024, "{}", content.len());
                assert!(content.starts_with("yyyy") && content.ends_with("[truncated]"));
            }
        }
    }
}

/// A command that prints a binary file hands the provider a tool result with
/// U+0000, which the control plane cannot store. The journal the API replays
/// carries U+FFFD in its place, in the tool result and in everything else the
/// turn reports.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_journal_replaces_nul_in_provider_output() {
    let providers = tempfile::tempdir().unwrap();
    let provider = executable_script(
        providers.path(),
        "claude-binary-tool-output.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_bin","name":"Bash","input":{"command":"head -c 8 /bin/sh"}}]}}'
printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_bin","content":"\u007fELF\u0002\u0001\u0001\u0000\u0000"}]}}'
printf '%s\n' '{"type":"assistant","message":{"id":"msg_bin","role":"assistant","content":[{"type":"text","text":"It starts with \u0000 bytes"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"It starts with \u0000 bytes","usage":{}}'
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "binary-tool-output",
        policy,
        "G5_CLAUDE_PATH",
        &provider,
    );
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(10), rpc.wait_outcome(&exec))
        .await
        .expect("the turn settles");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    let events = replay["events"].as_array().unwrap();
    let completed = events
        .iter()
        .find(|event| event["kind"] == "module.tool.completed")
        .unwrap_or_else(|| panic!("the tool result is journaled: {replay}"));
    assert_eq!(
        completed["payload"]["content"],
        "\u{7f}ELF\u{2}\u{1}\u{1}\u{FFFD}\u{FFFD}"
    );
    let reply = events
        .iter()
        .find(|event| event["kind"] == "module.message.final")
        .unwrap_or_else(|| panic!("the reply is journaled: {replay}"));
    assert_eq!(reply["payload"]["content"], "It starts with \u{FFFD} bytes");
    assert!(
        !replay.to_string().contains("\\u0000"),
        "no journaled event carries U+0000: {replay}"
    );
}

/// The optional title call has its own fixed output cap: a title provider
/// that floods its stdout is stopped at it, promptly, and the turn keeps its
/// answer without a summary title.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_bounds_optional_title_provider_output() {
    let providers = tempfile::tempdir().unwrap();
    let provider = executable_script(
        providers.path(),
        "claude-title-flood.sh",
        r#"#!/bin/sh
set -eu
case " $* " in
  *" --output-format json "*)
    while :; do printf 'title output without a boundary\n'; done
    ;;
  *)
    IFS= read -r initial
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"ordinary answer","usage":{}}'
    ;;
esac
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let policy = claude_auto_deny_policy(Some(RETIRED_OUTPUT_LIMIT));
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "give a title",
        policy,
        "G5_CLAUDE_PATH",
        &provider,
    );
    use_native_provider_fixture(&mut params);
    params["secrets"]["HOME"] = json!(providers.path().join("home"));
    params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
    params["input"]["provider"]["options"]["session_title"] = json!(true);
    params["input"]["limits"]["max_output_bytes"] = json!(RETIRED_OUTPUT_LIMIT);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    // Far inside the call's 60-second timeout: its 1 MiB cap ended it.
    let outcome = timeout(Duration::from_secs(8), rpc.wait_outcome(&exec))
        .await
        .expect("title stdout limit ends the optional call promptly");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    assert!(!replay["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| { event["kind"] == "module.session.title" }));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_setup_deadline_covers_blocked_input() {
    let providers = tempfile::tempdir().unwrap();
    let provider = executable_script(
        providers.path(),
        "claude-deadline.sh",
        r#"#!/bin/sh
set -eu
sleep 10
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        &"x".repeat(256 * 1024),
        policy,
        "G5_CLAUDE_PATH",
        &provider,
    );
    let home = providers.path().join("blocked");
    std::fs::create_dir(&home).unwrap();
    params["secrets"]["HOME"] = json!(home);
    params["input"]["limits"]["setup_timeout_ms"] = json!(1500);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(15), rpc.wait_outcome(&exec))
        .await
        .expect("bounded turn settles");
    assert!(
        outcome["status"] == "timed_out" || outcome["result"]["status"] == "timed_out",
        "{outcome}"
    );
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    assert!(
        !replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "module.input.accepted"),
        "{replay}"
    );
    assert_eq!(outcome["result"]["input_delivery_unknown"], true);
}

/// Every journaled event of one execution, in order.
async fn replay_events(
    rpc: &mut g5d::dispatch_test_support::Rpc,
    d: &TestDispatcher,
    exec: &str,
) -> Vec<Value> {
    result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    )["events"]
        .as_array()
        .unwrap()
        .clone()
}

fn events_of<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["kind"] == kind)
        .collect()
}

/// `modules.background.end` params with `idempotency_key = request_id`.
fn end_wait_params(d: &TestDispatcher, task: Uuid, attempt: u32, request_id: &str) -> Value {
    let mut params = d.params(
        true,
        json!({
            "exec_id": d.task_exec_id(task, attempt),
            "task_id": task,
            "attempt": attempt,
            "request_id": request_id,
        }),
    );
    params["context"]["idempotency_key"] = json!(request_id);
    params
}

const CLAUDE_PROMPT_POLICY: &str = r#"{"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"enabled","tool_approval":"prompt","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20000,"max_output_bytes":1048576,"native_response_timeout_ms":5000}}"#;

/// Without a requested limit the default deadline bounds only setup and
/// input delivery: a Claude or Codex turn that outlives it still succeeds,
/// while input that is never accepted still times out.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_default_limit_stops_at_input_acceptance() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-slow.sh",
        r#"#!/bin/sh
set -eu
case "$HOME" in
  *blocked*) sleep 10; exit 0 ;;
esac
IFS= read -r initial
printf '%s\n' '{"type":"system","subtype":"init","session_id":"33333333-3333-4333-8333-333333333333","model":"fixture-model","mcp_servers":[]}'
sleep 3
printf '%s\n' '{"type":"assistant","message":{"id":"slow-1","role":"assistant","content":[{"type":"text","text":"finished after the default deadline"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"finished after the default deadline","usage":{"input_tokens":1,"output_tokens":1}}'
cat >/dev/null
"#,
    );
    let codex = codex_app_server_fixture(
        providers.path(),
        "codex-slow.sh",
        "",
        json!({
            "thread": "44444444-4444-4444-8444-444444444444",
            "delay_after_acceptance": 3,
            "items": [{"id":"item_0","type":"agentMessage","text":"codex finished after the default deadline"}],
        }),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let claude_policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let codex_policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    for (name, provider_id, policy, key, provider, prompt, expected) in [
        (
            "claude-slow",
            "claude_code",
            claude_policy.clone(),
            "G5_CLAUDE_PATH",
            &claude,
            "slow turn".to_owned(),
            "succeeded",
        ),
        (
            "codex-slow",
            "codex",
            codex_policy,
            "G5_CODEX_PATH",
            &codex,
            "slow turn".to_owned(),
            "succeeded",
        ),
        (
            "blocked",
            "claude_code",
            claude_policy,
            "G5_CLAUDE_PATH",
            &claude,
            "x".repeat(256 * 1024),
            "timed_out",
        ),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            provider_id,
            "human",
            &prompt,
            policy,
            key,
            provider,
        );
        let home = providers.path().join(name);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        params["input"]["limits"]["setup_timeout_ms"] = json!(1500);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{name}"
        );
        let outcome = timeout(Duration::from_secs(25), rpc.wait_outcome(&exec))
            .await
            .expect("turn settles");
        if expected == "succeeded" {
            assert_eq!(outcome["status"], "succeeded", "{name}: {outcome}");
            assert_eq!(
                outcome["result"]["status"], "succeeded",
                "{name}: {outcome}"
            );
            let events = replay_events(&mut rpc, &d, &exec).await;
            assert!(
                events_of(&events, "module.message.final")
                    .iter()
                    .any(|event| event["payload"]["content"]
                        .as_str()
                        .is_some_and(|text| text.contains("after the default deadline"))),
                "{name}: {events:?}"
            );
        } else {
            assert!(
                outcome["status"] == "timed_out" || outcome["result"]["status"] == "timed_out",
                "{name}: {outcome}"
            );
            // Blocked input never reached the provider.
            assert_eq!(
                outcome["result"]["input_delivery_unknown"],
                name == "blocked",
                "{name}: {outcome}"
            );
        }
    }
}

/// Claude keeps its input open while it reports background work, then runs
/// its own report turn in the same attempt and reply; input closes only
/// after that turn, and nothing but the prompt is ever written as input.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_claude_background_report_turn_stays_in_the_attempt() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-background.sh",
        r#"#!/bin/sh
set -eu
exec 3<&0
IFS= read -r initial <&3
printf '%s\n' "$initial" > "$HOME/stdin.log"
sid="22222222-2222-4222-8222-222222222222"
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' '{"type":"assistant","message":{"id":"msg-1","role":"assistant","content":[{"type":"text","text":"started"}]}}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"bg1","task_type":"local_bash","description":"sleep 4"}]}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"bg1","tool_use_id":"toolu_1","description":"sleep 4","task_type":"local_bash","is_backgrounded":true}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"started","usage":{"input_tokens":18,"output_tokens":293,"cache_read_input_tokens":100,"cache_creation_input_tokens":10},"total_cost_usd":0.02,"modelUsage":{"fixture-model":{}}}'
# Record any further input and when it ends: the platform must keep it open.
( while IFS= read -r line; do printf '%s\n' "$line" >> "$HOME/stdin.log"; done; echo eof > "$HOME/eof" ) <&3 &
# Longer than the 3-second settle window after a background set empties.
sleep 4
if [ -e "$HOME/eof" ]; then
  printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"input closed during background work","usage":{}}'
  exit 3
fi
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bg1","tool_use_id":"toolu_1","status":"completed","summary":"Background command completed (exit code 0)"}'
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"Background command completed (exit code 0).","usage":{"input_tokens":10,"output_tokens":131},"total_cost_usd":0.05}'
wait
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "start a background job",
        serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
        "G5_CLAUDE_PATH",
        &claude,
    );
    let home = providers.path().join("home");
    std::fs::create_dir(&home).unwrap();
    params["secrets"]["HOME"] = json!(home);
    // The four-second wait outlives the default deadline, which lifted once
    // the provider took the input.
    params["input"]["limits"]["setup_timeout_ms"] = json!(1500);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = timeout(Duration::from_secs(25), rpc.wait_outcome(&exec))
        .await
        .expect("attempt settles after the report turn");
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["code"], "exit_code", "{outcome}");
    let usage = &outcome["result"]["usage"];
    assert_eq!(usage["input_tokens"], 28, "{outcome}");
    assert_eq!(usage["output_tokens"], 424, "{outcome}");
    assert_eq!(usage["total_cost_usd"], 0.05, "{outcome}");

    let events = replay_events(&mut rpc, &d, &exec).await;
    let finals = events_of(&events, "module.message.final");
    let blocks: Vec<_> = finals
        .iter()
        .map(|event| event["payload"]["block_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(blocks, ["msg-1:0", "g5:terminal-result:2"], "{events:?}");
    assert_eq!(
        finals[1]["payload"]["content"],
        "Background command completed (exit code 0)."
    );
    let sets = events_of(&events, "module.background_tasks.replaced");
    assert_eq!(sets.len(), 2, "{events:?}");
    assert_eq!(sets[0]["payload"]["count"], 1);
    assert_eq!(sets[0]["payload"]["tasks"][0]["task_id"], "bg1");
    assert_eq!(sets[0]["payload"]["tasks"][0]["kind"], "shell");
    assert!(sets[0]["payload"]["tasks"][0]["started_at_unix_ms"]
        .as_u64()
        .is_some());
    assert_eq!(sets[1]["payload"]["count"], 0);
    let started = events_of(&events, "module.subtask.started");
    assert_eq!(started.len(), 1, "{events:?}");
    assert_eq!(started[0]["payload"]["task_id"], "bg1");
    let completed = events_of(&events, "module.subtask.completed");
    assert_eq!(completed.len(), 1, "{events:?}");
    assert_eq!(completed[0]["payload"]["status"], "ok");

    // Input stayed open through the wait and closed after the report turn;
    // the only input line was the prompt.
    assert!(home.join("eof").exists(), "input never closed");
    let stdin = std::fs::read_to_string(home.join("stdin.log")).unwrap();
    let lines: Vec<_> = stdin.lines().collect();
    assert_eq!(lines.len(), 1, "{stdin}");
    assert!(lines[0].contains("\"type\":\"user\""), "{stdin}");
}

/// A native request opened by Claude's report turn is still answerable:
/// it holds the input open and keeps its own deadline.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_claude_report_turn_native_request_is_answered() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-report-request.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
sid="55555555-5555-4555-8555-555555555555"
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{"input_tokens":1,"output_tokens":1}}'
sleep 1
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' '{"type":"control_request","request_id":"provider-q-report","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"touch report"}}}'
IFS= read -r response
case "$response" in
  *'"request_id":"provider-q-report"'*'"behavior":"allow"'*|*'"behavior":"allow"'*'"request_id":"provider-q-report"'*) ;;
  *) exit 41 ;;
esac
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"report written","usage":{"input_tokens":1,"output_tokens":1}}'
cat >/dev/null
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "report later",
        serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
        "G5_CLAUDE_PATH",
        &claude,
    );
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let opened = rpc
        .module_event(&exec, "module.provider_request.opened")
        .await;
    assert_eq!(
        opened["payload"]["provider_request_id"],
        "provider-q-report"
    );
    let delivered = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-report", &json!({"behavior":"allow"})),
        )
        .await,
    );
    assert_eq!(delivered["disposition"], "delivered");
    let outcome = timeout(Duration::from_secs(25), rpc.wait_outcome(&exec))
        .await
        .expect("attempt settles");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let events = replay_events(&mut rpc, &d, &exec).await;
    assert!(
        events_of(&events, "module.message.final")
            .iter()
            .any(|event| event["payload"]["content"] == "report written"),
        "{events:?}"
    );
}

/// The owner's End interrupts Claude's background work between turns. A
/// clean exit then succeeds with a visible notice; a provider that keeps
/// running after its input closes is stopped and the attempt is interrupted.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_owner_end_stops_claude_background_wait() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-end.sh",
        r#"#!/bin/sh
set -eu
# Detached work the agent started: End stops the background wait, so it is
# reaped with the turn by the Linux execution owner. Only Linux has one.
if [ "$(uname -s)" = Linux ]; then
  setsid sh -c 'echo $$ > "$HOME/daemon.pid"; trap "" INT TERM; while :; do sleep 0.05; done' </dev/null >/dev/null 2>&1 &
fi
IFS= read -r initial
sid="66666666-6666-4666-8666-666666666666"
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' '{"type":"assistant","message":{"id":"end-1","role":"assistant","content":[{"type":"text","text":"launched"}]}}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"long review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{"input_tokens":1,"output_tokens":1}}'
case "$HOME" in
  # Stops reading its input while background work is still reported.
  *deaf*) exec 0<&-; trap '' INT; while :; do sleep 1; done ;;
esac
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"interrupt"'*)
      id=$(printf '%s' "$line" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
      # A slow answer arrives after the platform stopped waiting for it.
      case "$HOME" in *late*) sleep 11 ;; esac
      printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
      printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"ag1","status":"stopped","summary":"long review"}'
      printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{\"still_queued\":[]}}}"
      ;;
  esac
done
case "$HOME" in
  *stubborn*) trap '' INT; while :; do sleep 1; done ;;
esac
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for (name, status, code) in [
        ("clean", "succeeded", "exit_code"),
        ("late", "succeeded", "exit_code"),
        ("stubborn", "interrupted", "background_wait_ended"),
        ("deaf", "interrupted", "background_wait_ended"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "launch a background agent",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(name);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let waiting = rpc
            .module_event(&exec, "module.background_tasks.replaced")
            .await;
        assert_eq!(waiting["payload"]["count"], 1, "{name}");
        let request_id = Uuid::new_v4().to_string();
        let ended = result(
            rpc.call(
                "modules.background.end",
                end_wait_params(&d, task, 1, &request_id),
            )
            .await,
        );
        assert_eq!(ended["disposition"], "delivered", "{name}: {ended}");
        let outcome = timeout(Duration::from_secs(28), rpc.wait_outcome(&exec))
            .await
            .expect("ended attempt settles");
        assert_eq!(outcome["result"]["status"], status, "{name}: {outcome}");
        assert_eq!(outcome["result"]["code"], code, "{name}: {outcome}");
        if cfg!(target_os = "linux") {
            let daemon: libc::pid_t = std::fs::read_to_string(home.join("daemon.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            sleep(Duration::from_millis(200)).await;
            assert!(
                !process_exists(daemon),
                "{name}: an ended background wait released the agent's detached work"
            );
        }
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert!(
            events_of(&events, "module.notice").iter().any(|event| {
                event["payload"]["code"] == "background_wait_ended"
                    && event["payload"]["level"] == "info"
            }),
            "{name}: {events:?}"
        );
        if name != "deaf" {
            let completed = events_of(&events, "module.subtask.completed");
            assert_eq!(completed.len(), 1, "{name}: {events:?}");
            assert_eq!(completed[0]["payload"]["status"], "stopped");
        }
        // End after settlement reaches no execution.
        let late = result(
            rpc.call(
                "modules.background.end",
                end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
            )
            .await,
        );
        assert_ne!(late["disposition"], "delivered", "{name}: {late}");
    }
}

/// Claude's input window at its edges: a report turn that starts after the
/// set empties, a failed report turn, a report turn the provider never
/// finishes, a descendant that keeps the provider's output open after exit,
/// and a requested limit ending a wait.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_claude_window_edges() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-edges.sh",
        r#"#!/bin/sh
set -eu
exec 3<&0
IFS= read -r initial <&3
mode=$(basename "$HOME")
init='{"type":"system","subtype":"init","session_id":"88888888-8888-4888-8888-888888888888","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' "$init"
if [ "$mode" = drain ]; then
  printf '%s\n' '{"type":"assistant","message":{"id":"d-1","role":"assistant","content":[{"type":"text","text":"done"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"done","usage":{"input_tokens":1,"output_tokens":1}}'
  # Left running with the provider's own output.
  sleep 600 &
  echo $! > "$HOME/sleeper.pid"
  cat <&3 >/dev/null
  exit 0
fi
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"bg1","task_type":"local_bash","description":"job"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"started","usage":{"input_tokens":1,"output_tokens":1}}'
( while IFS= read -r line; do :; done; echo eof > "$HOME/eof" ) <&3 &
sleep 4
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bg1","status":"completed","summary":"job done"}'
case "$mode" in
  settle)
    sleep 1
    # No report turn once input has closed.
    [ -e "$HOME/eof" ] && exit 0
    printf '%s\n' "$init"
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"job reported","usage":{}}'
    ;;
  second_error)
    printf '%s\n' "$init"
    printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"report failed","usage":{}}'
    ;;
  open_turn)
    printf '%s\n' "$init"
    printf '%s\n' '{"type":"assistant","message":{"id":"o-1","role":"assistant","content":[{"type":"text","text":"partial report"}]}}'
    exit 0
    ;;
esac
wait
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for (mode, wall, status, code) in [
        ("settle", 20_000, "succeeded", "exit_code"),
        ("second_error", 20_000, "failed", "provider_error"),
        ("open_turn", 20_000, "failed", "provider_error"),
        ("drain", 20_000, "succeeded", "exit_code"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "start a job",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(mode);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        params["input"]["limits"]["setup_timeout_ms"] = json!(wall);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        let outcome = timeout(Duration::from_secs(28), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: attempt never settled"));
        assert!(
            outcome["status"] == status || outcome["result"]["status"] == status,
            "{mode}: {outcome}"
        );
        if status != "timed_out" {
            assert_eq!(outcome["result"]["code"], code, "{mode}: {outcome}");
        }
        let events = replay_events(&mut rpc, &d, &exec).await;
        match mode {
            "settle" => assert!(
                events_of(&events, "module.message.final")
                    .iter()
                    .any(|event| event["payload"]["content"] == "job reported"),
                "{mode}: {events:?}"
            ),
            "open_turn" => assert!(
                events_of(&events, "module.notice")
                    .iter()
                    .any(|event| event["payload"]["code"] == "provider_turn_incomplete"),
                "{mode}: {events:?}"
            ),
            "drain" => {
                // The bounded drain only stops waiting: after a normal finish
                // the descendant still holding the provider's output is the
                // agent's to stop.
                let pid: libc::pid_t = std::fs::read_to_string(home.join("sleeper.pid"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                assert!(
                    process_exists(pid),
                    "{mode}: a normal finish stopped the descendant"
                );
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            _ => {}
        }
    }
}

/// What remains of the provider's process group is settled when the provider
/// exits, not after the bounded harvest: a failed turn stops it before
/// harvesting. After a successful provider it is the agent's until a later
/// interruption vetoes the release; on Linux the execution owner then reaps it
/// (macOS has no owner and keeps it).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_provider_group_cleanup_does_not_wait_for_harvest() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-leaves-output-holder.sh",
        r#"#!/bin/sh
set -eu
echo "$PPID" > "$HOME/supervisor"
IFS= read -r initial
printf '%s\n' '{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}'
# Left running in the provider's process group, holding its output.
( trap '' INT TERM; while :; do echo beat >> "$HOME/beats"; sleep 0.05; done ) &
echo $! > "$HOME/holder.pid"
if [ -f "$HOME/succeed" ]; then
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"done","usage":{}}'
  exit 0
fi
exit 3
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    d.origin
        .as_ref()
        .unwrap()
        .set_session_file_manifest_delay(Duration::from_secs(60));
    let mut rpc = d.connect().await;
    for mode in ["failed", "succeeded"] {
        let home = providers.path().join(format!("holder-{mode}"));
        std::fs::create_dir(&home).unwrap();
        if mode == "succeeded" {
            std::fs::write(home.join("succeed"), "1").unwrap();
        }
        // A resume file makes harvest reach the (stalled) manifest request.
        let key = g5d_agent::runner::claude::project_key_for_cwd(&d.root.path().join("work"));
        let native = home.join(format!(
            ".claude/projects/{key}/77777777-7777-4777-8777-777777777777.jsonl"
        ));
        std::fs::create_dir_all(native.parent().unwrap()).unwrap();
        std::fs::write(&native, b"native session").unwrap();
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "leave a writer behind",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        params["secrets"]["HOME"] = json!(home);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        let progress = timeout(
            Duration::from_secs(20),
            rpc.module_event(&exec, "module.terminal.progress"),
        )
        .await
        .unwrap_or_else(|_| panic!("{mode}: harvest never started"));
        assert_eq!(progress["payload"]["phase"], "harvesting", "{mode}");
        let beats = || std::fs::metadata(home.join("beats")).map_or(0, |meta| meta.len());
        let before = beats();
        sleep(Duration::from_millis(400)).await;
        let during = beats();
        if mode == "failed" {
            assert_eq!(
                before, during,
                "{mode}: the provider group kept writing while harvest stalled"
            );
        } else {
            assert!(
                during > before,
                "{mode}: a normal finish stopped the provider group"
            );
        }
        let supervisor: libc::pid_t = std::fs::read_to_string(home.join("supervisor"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        unsafe {
            libc::kill(supervisor, libc::SIGTERM);
        }
        let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: the interrupted turn never settled"));
        assert_eq!(
            outcome["result"]["status"], "suspended",
            "{mode}: {outcome}"
        );
        assert_eq!(
            outcome["result"]["code"], "interrupted",
            "{mode}: {outcome}"
        );
        if cfg!(target_os = "linux") || mode == "failed" {
            let settled = beats();
            sleep(Duration::from_millis(400)).await;
            assert_eq!(
                beats(),
                settled,
                "{mode}: the provider group outlived the interrupted turn"
            );
        } else {
            let holder: libc::pid_t = std::fs::read_to_string(home.join("holder.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            unsafe {
                libc::kill(holder, libc::SIGKILL);
            }
        }
    }
}

/// The owner's End at its edges: a background sub-agent's request is still
/// pending, Claude's input already closed, the interrupt lands on a report
/// turn that had just started, a report turn runs past End's grace, the
/// background work drained before End arrived, End reaches a turn Claude runs
/// after its input closed with nothing left in the background, and Claude
/// refuses the interrupt and then fails on its own.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_owner_end_edges() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-end-edges.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
mode=$(basename "$HOME")
init='{"type":"system","subtype":"init","session_id":"99999999-9999-4999-8999-999999999999","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' "$init"
if [ "$mode" = late_report ]; then
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"quick","usage":{}}'
  # Input closes now; Claude then reports in a turn of its own, with no
  # background work left, for longer than the exit grace.
  cat >/dev/null
  printf '%s\n' "$init"
  printf '%s\n' '{"type":"assistant","message":{"id":"l-1","role":"assistant","content":[{"type":"text","text":"late report"}]}}'
  sleep 18
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"late report","usage":{}}'
  exit 0
fi
if [ "$mode" = after_close ]; then
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"quick","usage":{}}'
  # Input closes now; Claude then starts its own turn and a background agent.
  cat >/dev/null
  printf '%s\n' "$init"
  printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag9","task_type":"local_agent","description":"late agent"}]}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched late","usage":{}}'
  sleep 40
  exit 0
fi
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{}}'
if [ "$mode" = drained ]; then
  # The agent finishes and Claude reports it in its own turn while the
  # owner's End is on its way.
  sleep 1
  printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
  printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"ag1","status":"completed","summary":"review"}'
  printf '%s\n' "$init"
  sleep 4
  printf '%s\n' '{"type":"assistant","message":{"id":"r-1","role":"assistant","content":[{"type":"text","text":"review finished"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"review finished","usage":{}}'
  cat >/dev/null
  exit 0
fi
if [ "$mode" = pending ]; then
  # The background sub-agent asks for permission; nobody answers it.
  printf '%s\n' '{"type":"control_request","request_id":"provider-q-sub","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"make"}}}'
fi
if [ "$mode" = busy_turn ]; then
  # Claude starts its own turn and stays in it while the agent runs.
  printf '%s\n' "$init"
  printf '%s\n' '{"type":"assistant","message":{"id":"r-1","role":"assistant","content":[{"type":"text","text":"checking the agent"}]}}'
fi
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"interrupt"'*)
      id=$(printf '%s' "$line" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
      if [ "$mode" = refused ]; then
        # Claude refuses the interrupt, then fails for an unrelated reason.
        printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"error\",\"request_id\":\"$id\",\"error\":\"interrupt refused\"}}"
        printf '%s\n' "$init"
        printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"upstream failure","terminal_reason":"api_error","usage":{}}'
        printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
        continue
      fi
      if [ "$mode" = race ]; then
        # A report turn had just started.
        printf '%s\n' "$init"
      fi
      printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
      printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"ag1","status":"stopped","summary":"review"}'
      printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{\"still_queued\":[]}}}"
      if [ "$mode" = race ] || [ "$mode" = busy_turn ]; then
        # The interrupt aborts the turn in progress; Claude 2.1.285 answers
        # the request first and marks the result aborted.
        printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"","terminal_reason":"aborted_streaming","usage":{}}'
      fi
      ;;
  esac
done
if [ "$mode" = race ] || [ "$mode" = busy_turn ] || [ "$mode" = refused ]; then
  # Claude 2.1.285 exits 1 at input EOF after a turn ended in an error.
  exit 1
fi
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    // (mode, status, code, End notices, End first lets the turn run)
    for (mode, status, code, notices, turn_grace) in [
        ("pending", "succeeded", "exit_code", 1, false),
        ("race", "succeeded", "exit_code", 1, false),
        (
            "after_close",
            "interrupted",
            "background_wait_ended",
            1,
            false,
        ),
        ("busy_turn", "succeeded", "exit_code", 1, true),
        ("drained", "succeeded", "exit_code", 0, false),
        ("late_report", "succeeded", "exit_code", 0, false),
        ("refused", "failed", "provider_error", 1, false),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "launch a background agent",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(mode);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        if mode == "pending" {
            rpc.module_event(&exec, "module.provider_request.opened")
                .await;
        } else if mode != "late_report" {
            let waiting = rpc
                .module_event(&exec, "module.background_tasks.replaced")
                .await;
            assert_eq!(waiting["payload"]["count"], 1, "{mode}");
        }
        match mode {
            // End arrives during Claude's turn after its input closed, with
            // no background work: the turn finishes normally.
            "late_report" => {
                rpc.event_where("module.event", |params| {
                    params["exec_id"] == exec.as_str()
                        && params["kind"] == "module.message.final"
                        && params["payload"]["content"] == "late report"
                })
                .await;
            }
            // End arrives while Claude's own turn is running.
            "busy_turn" => {
                rpc.event_where("module.event", |params| {
                    params["exec_id"] == exec.as_str()
                        && params["kind"] == "module.message.final"
                        && params["payload"]["content"] == "checking the agent"
                })
                .await;
            }
            // End arrives after the work drained, during Claude's report.
            "drained" => {
                let drained = rpc
                    .module_event(&exec, "module.background_tasks.replaced")
                    .await;
                assert_eq!(drained["payload"]["count"], 0, "{mode}");
            }
            _ => {}
        }
        let end_sent = std::time::Instant::now();
        let ended = result(
            rpc.call(
                "modules.background.end",
                end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
            )
            .await,
        );
        assert_eq!(ended["disposition"], "delivered", "{mode}: {ended}");
        let outcome = timeout(Duration::from_secs(28), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: ended attempt never settled"));
        assert_eq!(outcome["result"]["status"], status, "{mode}: {outcome}");
        assert_eq!(outcome["result"]["code"], code, "{mode}: {outcome}");
        if turn_grace {
            // End let the turn run for its ten-second grace, then interrupted
            // it; the aborted turn and Claude's exit status are its effect.
            assert!(
                end_sent.elapsed() >= Duration::from_secs(9),
                "{mode}: End interrupted the turn without its grace"
            );
        }
        let events = replay_events(&mut rpc, &d, &exec).await;
        if turn_grace {
            // The notice says that the running turn was stopped as well.
            assert!(
                events_of(&events, "module.notice").iter().any(|event| {
                    event["payload"]["code"] == "background_wait_ended"
                        && event["payload"]["message"]
                            .as_str()
                            .is_some_and(|message| message.contains("current turn"))
                }),
                "{mode}: {events:?}"
            );
        }
        assert_eq!(
            events_of(&events, "module.notice")
                .iter()
                .filter(|event| event["payload"]["code"] == "background_wait_ended")
                .count(),
            notices,
            "{mode}: {events:?}"
        );
        if mode == "drained" {
            // With nothing left to stop, the report turn finishes the reply.
            assert!(
                events_of(&events, "module.message.final")
                    .iter()
                    .any(|event| event["payload"]["content"] == "review finished"),
                "{mode}: {events:?}"
            );
        }
        if mode == "pending" {
            assert!(
                events_of(&events, "module.provider_request.invalidated")
                    .iter()
                    .any(|event| event["payload"]["reason"] == "background_wait_ended"),
                "{mode}: {events:?}"
            );
        }
    }
}

/// Claude keeps its input open but stops reading it: the owner's native
/// replies fill the pipe and the blocked write times out. The pending
/// requests end, and a provider still running when the exit grace ends is
/// stopped, settling like an unanswered request, or as End's bounded stop
/// when the owner ended the wait.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_provider_that_stops_reading_cannot_hold_replies_or_end() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-stops-reading.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
printf '%s\n' '{"type":"system","subtype":"init","session_id":"88888888-8888-4888-8888-888888888888","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{}}'
# The background agent asks three times with large inputs, so the replies
# carry them back and overflow the input pipe.
big=$(head -c 60000 /dev/zero | tr '\0' x)
for n in 1 2 3; do
  printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/tmp/f%s","content":"%s"}}}\n' "$n" "$n" "$big"
done
# Keep the input open without ever reading it again.
sleep 120
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for (mode, status, code) in [
        ("end", "interrupted", "background_wait_ended"),
        ("replies", "suspended", "native_response_undeliverable"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "launch a background agent",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        // Requests outlive the write bound, so the transport bound decides.
        params["input"]["provider"]["policy_snapshot"]["limits"]["native_response_timeout_ms"] =
            json!(60_000);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        for _ in 0..3 {
            rpc.module_event(&exec, "module.provider_request.opened")
                .await;
        }
        if mode == "end" {
            let ended = result(
                rpc.call(
                    "modules.background.end",
                    end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
                )
                .await,
            );
            assert_eq!(ended["disposition"], "delivered", "{ended}");
        }
        let replies_sent = std::time::Instant::now();
        for n in 1..=3 {
            let replied = result(
                rpc.call(
                    "modules.input.respond",
                    respond_params(&d, task, 1, &format!("q-{n}"), &json!({"behavior":"allow"})),
                )
                .await,
            );
            assert_eq!(replied["disposition"], "delivered", "{mode}: {replied}");
        }
        let outcome = timeout(Duration::from_secs(60), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: a provider that stopped reading held the attempt"));
        assert_eq!(outcome["result"]["status"], status, "{mode}: {outcome}");
        assert_eq!(outcome["result"]["code"], code, "{mode}: {outcome}");
        // Bounded by the write limit and the exit grace, not the request
        // deadline.
        assert!(
            replies_sent.elapsed() < Duration::from_secs(40),
            "{mode}: settled only after {:?}",
            replies_sent.elapsed()
        );
        let events = replay_events(&mut rpc, &d, &exec).await;
        let ended_requests: BTreeSet<_> = events_of(&events, "module.provider_request.invalidated")
            .iter()
            .chain(events_of(&events, "module.provider_request.replied").iter())
            .map(|event| {
                event["payload"]["provider_request_id"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            ended_requests,
            BTreeSet::from([
                "provider-q-1".into(),
                "provider-q-2".into(),
                "provider-q-3".into()
            ]),
            "{mode}: {events:?}"
        );
        // Without End, the blocked write is what fails. With End, its own
        // reply wait may close the input first; either way every request
        // ended.
        if mode == "replies" {
            assert!(
                events_of(&events, "module.notice")
                    .iter()
                    .any(|event| event["payload"]["code"] == "provider_input_failed"),
                "{mode}: {events:?}"
            );
        }
    }
}

/// Claude finishes on its own while the owner's replies cannot be written:
/// its input is gone, but its report, result and usage still settle the
/// attempt, with or without a pending End.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_claude_finishing_while_a_reply_fails_keeps_its_result() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-finishes-without-input.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
init='{"type":"system","subtype":"init","session_id":"66666666-6666-4666-8666-666666666666","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' "$init"
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{"input_tokens":1,"output_tokens":1}}'
printf '%s\n' "$init"
printf '%s\n' '{"type":"assistant","message":{"id":"r-1","role":"assistant","content":[{"type":"text","text":"checking"}]}}'
for n in 1 2 3; do
  printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"make %s"}}}\n' "$n" "$n"
done
# Claude stops taking input and then finishes on its own.
exec 0<&-
sleep 2
for n in 1 2 3; do
  printf '{"type":"control_cancel_request","request_id":"provider-q-%s"}\n' "$n"
done
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"assistant","message":{"id":"r-2","role":"assistant","content":[{"type":"text","text":"review done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"review done","usage":{"input_tokens":11,"output_tokens":22}}'
exit 0
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for (mode, notice) in [
        ("replies", "provider_input_failed"),
        ("end_and_replies", "background_wait_ended"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "launch a background agent",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        for _ in 0..3 {
            rpc.module_event(&exec, "module.provider_request.opened")
                .await;
        }
        if mode == "end_and_replies" {
            // Claude is in its report turn, so End waits for the turn.
            let ended = result(
                rpc.call(
                    "modules.background.end",
                    end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
                )
                .await,
            );
            assert_eq!(ended["disposition"], "delivered", "{ended}");
        }
        for n in 1..=3 {
            let replied = result(
                rpc.call(
                    "modules.input.respond",
                    respond_params(&d, task, 1, &format!("q-{n}"), &json!({"behavior":"allow"})),
                )
                .await,
            );
            assert_eq!(replied["disposition"], "delivered", "{mode}: {replied}");
        }
        let outcome = timeout(Duration::from_secs(40), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: attempt never settled"));
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{mode}: {outcome}"
        );
        assert_eq!(outcome["result"]["code"], "exit_code", "{mode}: {outcome}");
        let usage = &outcome["result"]["usage"];
        assert_eq!(usage["input_tokens"], 12, "{mode}: {outcome}");
        assert_eq!(usage["output_tokens"], 23, "{mode}: {outcome}");
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert!(
            events_of(&events, "module.message.final")
                .iter()
                .any(|event| event["payload"]["content"] == "review done"),
            "{mode}: {events:?}"
        );
        assert_eq!(
            events_of(&events, "module.provider_request.invalidated").len(),
            3,
            "{mode}: {events:?}"
        );
        assert!(
            events_of(&events, "module.notice")
                .iter()
                .any(|event| event["payload"]["code"] == notice),
            "{mode}: {events:?}"
        );
    }
}

/// A reply still being written when its request's deadline passes does not
/// count as answered: the request times out as if no reply had arrived.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_native_reply_cannot_outlive_its_request_deadline() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-reads-late.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
printf '%s\n' '{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}'
big=$(head -c 60000 /dev/zero | tr '\0' x)
for n in 1 2; do
  printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/tmp/f%s","content":"%s"}}}\n' "$n" "$n" "$big"
done
# Read the replies only after the two-second request deadline.
sleep 4
IFS= read -r first
IFS= read -r second
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"wrote both","usage":{}}'
cat >/dev/null
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "write two files",
        serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
        "G5_CLAUDE_PATH",
        &claude,
    );
    params["input"]["provider"]["policy_snapshot"]["limits"]["native_response_timeout_ms"] =
        json!(2000);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    for _ in 0..2 {
        rpc.module_event(&exec, "module.provider_request.opened")
            .await;
    }
    for n in 1..=2 {
        let replied = result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, task, 1, &format!("q-{n}"), &json!({"behavior":"allow"})),
            )
            .await,
        );
        assert_eq!(replied["disposition"], "delivered", "{replied}");
    }
    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
        .await
        .expect("attempt settles");
    assert_eq!(outcome["result"]["status"], "suspended", "{outcome}");
    assert_eq!(
        outcome["result"]["code"], "native_response_timeout",
        "{outcome}"
    );
    let events = replay_events(&mut rpc, &d, &exec).await;
    // At most the reply that fit in the pipe before the deadline counts.
    assert!(
        events_of(&events, "module.provider_request.replied").len() < 2,
        "{events:?}"
    );
}

/// While the owner's replies wait for Claude to read them, the supervisor
/// keeps reading Claude's output and running every request deadline: Claude
/// can cancel the answered requests and finish, and an unanswered request
/// still times out.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_blocked_replies_leave_cancellation_and_deadlines_running() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-blocked-replies.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
mode=$(basename "$HOME")
printf '%s\n' '{"type":"system","subtype":"init","session_id":"44444444-4444-4444-8444-444444444444","model":"fixture-model","mcp_servers":[]}'
big=$(head -c 60000 /dev/zero | tr '\0' x)
if [ "$mode" = expiry ]; then
  # An unanswered request first; the answered ones a second later.
  printf '%s\n' '{"type":"control_request","request_id":"provider-q-0","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"make"}}}'
  sleep 1
fi
for n in 1 2; do
  printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/tmp/f%s","content":"%s"}}}\n' "$n" "$n" "$big"
done
if [ "$mode" = cancel ]; then
  # Claude moves on before the two-second deadline, without reading.
  sleep 1
  printf '%s\n' '{"type":"control_cancel_request","request_id":"provider-q-1"}'
  printf '%s\n' '{"type":"control_cancel_request","request_id":"provider-q-2"}'
  printf '%s\n' '{"type":"assistant","message":{"id":"c-1","role":"assistant","content":[{"type":"text","text":"cancelled both"}]}}'
  # Stdout and stdin have no cross-pipe ordering. Keep replies blocked until
  # the test observes the message following both cancellations.
  while [ ! -f "$HOME/drain-input" ]; do sleep 0.01; done
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"cancelled both","usage":{"input_tokens":11,"output_tokens":22}}'
  cat >/dev/null
  exit 0
fi
# expiry: retract the unanswered request just after its deadline, while the
# replies still wait, then read them before their own deadline and finish.
sleep 1.3
printf '%s\n' '{"type":"control_cancel_request","request_id":"provider-q-0"}'
sleep 0.2
IFS= read -r first
IFS= read -r second
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"done","usage":{}}'
cat >/dev/null
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for (mode, status, code) in [
        ("cancel", "succeeded", "exit_code"),
        ("expiry", "suspended", "native_response_timeout"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "write two files",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(mode);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        params["input"]["provider"]["policy_snapshot"]["limits"]["native_response_timeout_ms"] =
            json!(2000);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        let opened = if mode == "expiry" { 3 } else { 2 };
        for _ in 0..opened {
            rpc.module_event(&exec, "module.provider_request.opened")
                .await;
        }
        for n in 1..=2 {
            let replied = result(
                rpc.call(
                    "modules.input.respond",
                    respond_params(&d, task, 1, &format!("q-{n}"), &json!({"behavior":"allow"})),
                )
                .await,
            );
            assert_eq!(replied["disposition"], "delivered", "{mode}: {replied}");
        }
        if mode == "cancel" {
            let message = timeout(
                Duration::from_secs(10),
                rpc.module_event(&exec, "module.message.final"),
            )
            .await
            .expect("cancellation is processed while provider input stays blocked");
            assert_eq!(message["payload"]["content"], "cancelled both");
            std::fs::write(home.join("drain-input"), []).unwrap();
        }
        let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: attempt never settled"));
        assert_eq!(outcome["result"]["status"], status, "{mode}: {outcome}");
        assert_eq!(outcome["result"]["code"], code, "{mode}: {outcome}");
        let events = replay_events(&mut rpc, &d, &exec).await;
        if mode == "cancel" {
            // Claude's own finish settles the attempt with its usage.
            assert_eq!(outcome["result"]["usage"]["output_tokens"], 22, "{outcome}");
            assert!(
                events_of(&events, "module.message.final")
                    .iter()
                    .any(|event| event["payload"]["content"] == "cancelled both"),
                "{events:?}"
            );
            // The reply that fit in the pipe was delivered before Claude
            // cancelled; the blocked one never counts as delivered.
            assert!(
                !events_of(&events, "module.provider_request.replied")
                    .iter()
                    .any(|event| event["payload"]["provider_request_id"] == "provider-q-2"),
                "{events:?}"
            );
            assert!(
                events_of(&events, "module.provider_request.invalidated")
                    .iter()
                    .any(|event| {
                        event["payload"]["provider_request_id"] == "provider-q-2"
                            && event["payload"]["reason"] == "provider_cancelled"
                    }),
                "{events:?}"
            );
        }
    }
}

/// End queued behind replies Claude is slow to read takes effect only when
/// its interrupt is written. If the background work ends first, End is
/// withdrawn and Claude's report turn finishes.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_owner_end_queued_behind_replies_yields_to_finished_work() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-end-behind-replies.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
# Keep Claude's input reachable for a background reader: a non-interactive
# POSIX shell gives background lists /dev/null as their stdin.
exec 3<&0
init='{"type":"system","subtype":"init","session_id":"33333333-3333-4333-8333-333333333333","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' "$init"
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{}}'
big=$(head -c 60000 /dev/zero | tr '\0' x)
for n in 1 2 3; do
  printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/tmp/f%s","content":"%s"}}}\n' "$n" "$n" "$big"
done
# The owner's replies fill the input and End waits behind them. Meanwhile
# the agent finishes: its requests are withdrawn and Claude reports.
sleep 2
for n in 1 2 3; do
  printf '{"type":"control_cancel_request","request_id":"provider-q-%s"}\n' "$n"
done
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' "$init"
# Keep End blocked until the supervisor has observed the work finishing.
while [ ! -f "$HOME/drain-input" ]; do sleep 0.01; done
# Read the input from now on; an interrupt would abort the report.
( while IFS= read -r line; do printf '%s\n' "$line" >> "$HOME/input.log"; done ) <&3 &
sleep 1
if grep -q '"subtype":"interrupt"' "$HOME/input.log" 2>/dev/null; then
  id=$(grep '"subtype":"interrupt"' "$HOME/input.log" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
  printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{\"still_queued\":[]}}}"
  printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"","terminal_reason":"aborted_streaming","usage":{}}'
else
  printf '%s\n' '{"type":"assistant","message":{"id":"r-1","role":"assistant","content":[{"type":"text","text":"review report"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"review report","usage":{}}'
fi
wait
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "launch a background agent",
        serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
        "G5_CLAUDE_PATH",
        &claude,
    );
    let home = providers.path().join("home");
    std::fs::create_dir(&home).unwrap();
    params["secrets"]["HOME"] = json!(home);
    params["input"]["provider"]["policy_snapshot"]["limits"]["native_response_timeout_ms"] =
        json!(60_000);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    for _ in 0..3 {
        rpc.module_event(&exec, "module.provider_request.opened")
            .await;
    }
    for n in 1..=3 {
        let replied = result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, task, 1, &format!("q-{n}"), &json!({"behavior":"allow"})),
            )
            .await,
        );
        assert_eq!(replied["disposition"], "delivered", "{replied}");
    }
    let ended = result(
        rpc.call(
            "modules.background.end",
            end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
        )
        .await,
    );
    assert_eq!(ended["disposition"], "delivered", "{ended}");
    for expected_tasks in [1, 0] {
        let background = timeout(
            Duration::from_secs(10),
            rpc.module_event(&exec, "module.background_tasks.replaced"),
        )
        .await
        .expect("background work finishes while provider input stays blocked");
        assert_eq!(
            background["payload"]["tasks"].as_array().map(Vec::len),
            Some(expected_tasks),
            "{background}"
        );
    }
    std::fs::write(home.join("drain-input"), []).unwrap();
    let outcome = timeout(Duration::from_secs(40), rpc.wait_outcome(&exec))
        .await
        .expect("attempt settles");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["code"], "exit_code", "{outcome}");
    let events = replay_events(&mut rpc, &d, &exec).await;
    assert!(
        events_of(&events, "module.message.final")
            .iter()
            .any(|event| event["payload"]["content"] == "review report"),
        "{events:?}"
    );
    // End never took effect, so it claims nothing.
    assert!(
        !events_of(&events, "module.notice")
            .iter()
            .any(|event| event["payload"]["code"] == "background_wait_ended"),
        "{events:?}"
    );
    let input_log = std::fs::read_to_string(home.join("input.log")).unwrap_or_default();
    // Claude drained the replies that were already on their way...
    assert!(input_log.contains("provider-q-1"), "{input_log}");
    // ...and the withdrawn interrupt never followed them.
    assert!(
        !input_log.contains("\"subtype\":\"interrupt\""),
        "the withdrawn interrupt reached Claude"
    );
}

/// End queued behind replies that Claude reads slowly but steadily takes
/// effect about ten seconds after it was sent, whether Claude is between
/// turns or in a turn of its own: the requests end and the input is dropped
/// before the notice is published, and the bounded stop ends the work.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_owner_end_is_bounded_behind_slowly_read_replies() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-slow-reader.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
mode=$(basename "$HOME")
init='{"type":"system","subtype":"init","session_id":"11111111-1111-4111-8111-111111111111","model":"fixture-model","mcp_servers":[]}'
printf '%s\n' "$init"
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{}}'
big=$(head -c 60000 /dev/zero | tr '\0' x)
n=1
while [ "$n" -le 10 ]; do
  printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/tmp/f%s","content":"%s"}}}\n' "$n" "$n" "$big"
  n=$((n + 1))
done
if [ "$mode" = turn ]; then
  # Claude starts a turn of its own and stays in it.
  printf '%s\n' "$init"
  printf '%s\n' '{"type":"assistant","message":{"id":"t-1","role":"assistant","content":[{"type":"text","text":"checking"}]}}'
fi
# Read 4 KiB a second, forever: the writes keep progressing slowly.
while :; do
  dd bs=4096 count=1 of=/dev/null 2>/dev/null || true
  sleep 1
done
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for mode in ["idle", "turn"] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "launch a background agent",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(mode);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        // Keep setup and permission-response deadlines far beyond End's
        // bound so only End can settle the attempt.
        params["input"]["limits"]["setup_timeout_ms"] = json!(120_000);
        params["input"]["provider"]["policy_snapshot"]["limits"]["setup_timeout_ms"] =
            json!(120_000);
        params["input"]["provider"]["policy_snapshot"]["limits"]["native_response_timeout_ms"] =
            json!(120_000);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        for _ in 0..10 {
            rpc.module_event(&exec, "module.provider_request.opened")
                .await;
        }
        if mode == "turn" {
            rpc.event_where("module.event", |params| {
                params["exec_id"] == exec.as_str()
                    && params["kind"] == "module.message.final"
                    && params["payload"]["content"] == "checking"
            })
            .await;
        }
        for n in 1..=10 {
            let replied = result(
                rpc.call(
                    "modules.input.respond",
                    respond_params(&d, task, 1, &format!("q-{n}"), &json!({"behavior":"allow"})),
                )
                .await,
            );
            assert_eq!(replied["disposition"], "delivered", "{mode}: {replied}");
        }
        let ended = result(
            rpc.call(
                "modules.background.end",
                end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
            )
            .await,
        );
        assert_eq!(ended["disposition"], "delivered", "{mode}: {ended}");
        // End takes effect about ten seconds after it was sent, also when
        // Claude is in a turn and the turn grace and the queue overlap.
        timeout(
            Duration::from_secs(13),
            rpc.event_where("module.event", |params| {
                params["exec_id"] == exec.as_str()
                    && params["kind"] == "module.notice"
                    && params["payload"]["code"] == "background_wait_ended"
            }),
        )
        .await
        .unwrap_or_else(|_| panic!("{mode}: End did not take effect within its bound"));
        let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: the bounded stop did not settle"));
        assert_eq!(
            outcome["result"]["status"], "interrupted",
            "{mode}: {outcome}"
        );
        assert_eq!(
            outcome["result"]["code"], "background_wait_ended",
            "{mode}: {outcome}"
        );
        // The requests ended before End's notice was published.
        let events = replay_events(&mut rpc, &d, &exec).await;
        let notice = events
            .iter()
            .position(|event| {
                event["kind"] == "module.notice"
                    && event["payload"]["code"] == "background_wait_ended"
            })
            .unwrap();
        let invalidated: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, event)| {
                event["kind"] == "module.provider_request.invalidated"
                    && event["payload"]["reason"] == "background_wait_ended"
            })
            .map(|(index, _)| index)
            .collect();
        assert!(!invalidated.is_empty(), "{mode}: {events:?}");
        assert!(
            invalidated.iter().all(|index| *index < notice),
            "{mode}: {events:?}"
        );
    }
}

/// Run the supervisor binary directly, the way the dispatcher does, so a
/// test can pause it or hold back its event output.
async fn direct_supervisor(
    home: &Path,
    claude: &Path,
    native_response_timeout_ms: u64,
) -> tokio::process::Child {
    let mut policy: Value = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
    policy["limits"]["native_response_timeout_ms"] = json!(native_response_timeout_ms);
    let prompt = "start";
    let record = json!({"input": {
        "schema_version": "g5.agent.turn.input/v1",
        "task": {
            "task_id": "task-1",
            "attempt": 1,
            "session_id": "session-1",
            "input_origin": "human",
            "initial_input": {
                "reservation_id": "r1",
                "input_id": "i1",
                "revision": 1,
                "delivery_generation": 1,
                "input_origin": "human",
                "system_notice_ids": []
            }
        },
        "provider": {
            "provider_id": "claude_code",
            "model_id": "fixture-model",
            "policy_ref": "policy:1",
            "policy_snapshot": policy,
            "options": {"fake_provider": true}
        },
        "prompt": {"text": prompt, "sha256": format!("{:x}", Sha256::digest(prompt.as_bytes()))},
        "workspace_target": {},
        "session_binding": {"session_files_state": "absent"},
        "limits": {
            "setup_timeout_ms": 20_000,
            "max_output_bytes": 1_048_576
        }
    }});
    let mut command = tokio::process::Command::new(bin());
    command
        .arg("agent-turn")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap())
        .env("HOME", home)
        .env("G5D_WORKSPACE_ROOT", home)
        .env("G5_CLAUDE_PATH", claude)
        .envs(inherited_loader_env())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(format!("{record}\n").as_bytes())
        .await
        .unwrap();
    child
}

/// A control line answering `provider-q-<n>` with an allow.
fn direct_reply(n: u32) -> String {
    let response = json!({"behavior":"allow"});
    format!(
        "{}\n",
        json!({
            "kind": "input.respond",
            "request_id": format!("owner-{n}"),
            "provider_request_id": format!("provider-q-{n}"),
            "response_digest": response_digest(&response),
            "validated_response": response,
        })
    )
}

/// The supervisor's events up to and including its outcome.
async fn direct_events(stdout: tokio::process::ChildStdout) -> Vec<Value> {
    let mut lines = tokio::io::BufReader::new(stdout).lines();
    let mut events = Vec::new();
    while let Some(line) = lines.next_line().await.unwrap() {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let done = event["kind"] == "outcome";
        events.push(event);
        if done {
            break;
        }
    }
    events
}

/// A native request's deadline is settled before anything that would erase
/// it: after the supervisor was paused across the deadline, the late reply
/// is not written, and while its event output was held back, a cancellation
/// Claude sent after the deadline does not save the request.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_expired_request_is_settled_before_late_events() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-late-events.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
mode=$(basename "$HOME")
printf '%s\n' '{"type":"system","subtype":"init","session_id":"22222222-2222-4222-8222-222222222222","model":"fixture-model","mcp_servers":[]}'
if [ "$mode" = paused ]; then
  big=$(head -c 60000 /dev/zero | tr '\0' x)
  for n in 1 2; do
    printf '{"type":"control_request","request_id":"provider-q-%s","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/tmp/f%s","content":"%s"}}}\n' "$n" "$n" "$big"
  done
  # Read the replies only after the test paused the supervisor.
  sleep 1
  IFS= read -r first
  IFS= read -r second
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"wrote","usage":{}}'
  cat >/dev/null
  exit 0
fi
if [ "$mode" = end_buffered ]; then
  # Background work and an unanswered request; End's interrupt arrives
  # and goes unanswered while a large unit holds the event output.
  printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"ag1","task_type":"local_agent","description":"review"}]}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"launched","usage":{}}'
  printf '%s\n' '{"type":"control_request","request_id":"provider-q-0","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"make"}}}'
  sleep 0.5
  unit=$(head -c 500000 /dev/zero | tr '\0' y)
  printf '{"type":"assistant","message":{"id":"m-1","role":"assistant","content":[{"type":"text","text":"%s"}]}}\n' "$unit"
  cat >/dev/null
  exit 0
fi
# buffered: an unanswered request, then one large unit whose event the
# test does not drain; Claude retracts the request after its deadline and
# finishes.
printf '%s\n' '{"type":"control_request","request_id":"provider-q-0","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"make"}}}'
unit=$(head -c 500000 /dev/zero | tr '\0' y)
printf '{"type":"assistant","message":{"id":"m-1","role":"assistant","content":[{"type":"text","text":"%s"}]}}\n' "$unit"
sleep 1.5
printf '%s\n' '{"type":"control_cancel_request","request_id":"provider-q-0"}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"done","usage":{}}'
cat >/dev/null
"#,
    );

    // Paused across the deadline: the clock settles before the late write.
    let home = providers.path().join("paused");
    std::fs::create_dir(&home).unwrap();
    let mut child = direct_supervisor(&home, &claude, 1_000).await;
    let stdout = child.stdout.take().unwrap();
    let events = tokio::spawn(direct_events(stdout));
    tokio::time::sleep(Duration::from_millis(300)).await;
    {
        let stdin = child.stdin.as_mut().unwrap();
        for n in 1..=2 {
            stdin.write_all(direct_reply(n).as_bytes()).await.unwrap();
        }
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let pid = child.id().unwrap().to_string();
    let signal = |name: &str| {
        assert!(std::process::Command::new("kill")
            .args([name, pid.as_str()])
            .status()
            .unwrap()
            .success());
    };
    signal("-STOP");
    // Claude reads the input meanwhile; the deadline passes.
    tokio::time::sleep(Duration::from_millis(1_800)).await;
    signal("-CONT");
    let events = timeout(Duration::from_secs(30), events)
        .await
        .expect("paused supervisor settles")
        .unwrap();
    let outcome = events.last().unwrap();
    assert_eq!(outcome["payload"]["status"], "suspended", "{events:?}");
    assert_eq!(
        outcome["payload"]["code"], "native_response_timeout",
        "{events:?}"
    );
    let replied: Vec<_> = events
        .iter()
        .filter(|event| event["kind"] == "provider_request.replied")
        .collect();
    assert!(
        replied.len() < 2,
        "a reply was written after its deadline: {replied:?}"
    );
    drop(child);

    // Event output held back across the deadline: the expiry wins over the
    // cancellation buffered behind it.
    let home = providers.path().join("buffered");
    std::fs::create_dir(&home).unwrap();
    let mut child = direct_supervisor(&home, &claude, 1_000).await;
    let stdout = child.stdout.take().unwrap();
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let events = timeout(Duration::from_secs(30), direct_events(stdout))
        .await
        .expect("held-back supervisor settles");
    let outcome = events.last().unwrap();
    assert_eq!(outcome["payload"]["status"], "suspended", "{events:?}");
    assert_eq!(
        outcome["payload"]["code"], "native_response_timeout",
        "{events:?}"
    );
    drop(child);

    // End awaiting its reply while event output is held back past both the
    // request's deadline and End's reply wait: the expired request decides,
    // instead of End invalidating it.
    let home = providers.path().join("end_buffered");
    std::fs::create_dir(&home).unwrap();
    let mut child = direct_supervisor(&home, &claude, 2_000).await;
    let mut lines = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
    // End once Claude reported its background work and opened the request.
    while let Some(line) = lines.next_line().await.unwrap() {
        if line.contains("\"provider_request.opened\"") {
            break;
        }
    }
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"{\"kind\":\"background.end\",\"request_id\":\"end-1\"}\n")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(13)).await;
    let mut events = Vec::new();
    let settled = timeout(Duration::from_secs(30), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            let Ok(event) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let done = event["kind"] == "outcome";
            events.push(event);
            if done {
                break;
            }
        }
    })
    .await;
    assert!(settled.is_ok(), "held-back supervisor settles after End");
    // End took effect before the output was held back.
    assert!(
        events.iter().any(|event| {
            event["kind"] == "notice" && event["payload"]["code"] == "background_wait_ended"
        }),
        "{events:?}"
    );
    let outcome = events.last().unwrap();
    assert_eq!(outcome["payload"]["status"], "suspended", "{events:?}");
    assert_eq!(
        outcome["payload"]["code"], "native_response_timeout",
        "{events:?}"
    );
    drop(child);
}

/// A recorded exact-pin Claude stream and the 1-based line numbers that
/// split it: the first `result`, and the first emptied background set.
fn recorded_claude_stream(name: &str) -> (std::path::PathBuf, usize, usize) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/agent-turn-native/claude-2.1.321")
        .join(format!("{name}.jsonl"));
    let frames: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let first_result = frames
        .iter()
        .position(|frame| frame["type"] == "result")
        .unwrap()
        + 1;
    let emptied = frames
        .iter()
        .position(|frame| {
            frame["subtype"] == "background_tasks_changed"
                && frame["tasks"].as_array().is_some_and(Vec::is_empty)
        })
        .unwrap()
        + 1;
    (path, first_result, emptied)
}

/// The real dispatcher and supervisor replaying recorded Claude 2.1.321
/// background sessions: a report turn after a background command, and the
/// owner's End interrupting a background sub-agent between turns and during
/// a turn that runs a foreground command.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_replays_recorded_claude_background_sessions() {
    let providers = tempfile::tempdir().unwrap();
    let (report, report_result, _) = recorded_claude_stream("background-bash-report");
    let report_provider = executable_script(
        providers.path(),
        "claude-replay-report.sh",
        &format!(
            r#"#!/bin/sh
set -eu
exec 3<&0
IFS= read -r initial <&3
sed -n '1,{report_result}p' '{fixture}'
( while IFS= read -r line; do :; done; echo eof > "$HOME/eof" ) <&3 &
sleep 4
[ -e "$HOME/eof" ] && exit 3
sed -n '{after},$p' '{fixture}'
wait
"#,
            fixture = report.display(),
            after = report_result + 1,
        ),
    );
    let (interrupt, _, interrupt_effect) = recorded_claude_stream("background-agent-interrupt");
    let interrupt_provider = executable_script(
        providers.path(),
        "claude-replay-interrupt.sh",
        &format!(
            r#"#!/bin/sh
set -eu
IFS= read -r initial
sed -n '1,{before}p' '{fixture}'
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"interrupt"'*)
      id=$(printf '%s' "$line" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
      sed -n '{interrupt_effect},$p' '{fixture}' | sed "s/g5-end-fixture/$id/g"
      ;;
  esac
done
"#,
            fixture = interrupt.display(),
            before = interrupt_effect - 1,
        ),
    );
    let (interrupt_turn, _, interrupt_turn_effect) =
        recorded_claude_stream("background-agent-interrupt-turn");
    let interrupt_turn_provider = executable_script(
        providers.path(),
        "claude-replay-interrupt-turn.sh",
        &format!(
            r#"#!/bin/sh
set -eu
IFS= read -r initial
sed -n '1,{before}p' '{fixture}'
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"interrupt"'*)
      id=$(printf '%s' "$line" | sed 's/.*"request_id":"\([^"]*\)".*/\1/')
      sed -n '{interrupt_turn_effect},$p' '{fixture}' | sed "s/g5-end-fixture/$id/g"
      ;;
  esac
done
# Recorded: Claude exits 1 at input EOF after the interrupt aborted its turn.
exit 1
"#,
            fixture = interrupt_turn.display(),
            before = interrupt_turn_effect - 1,
        ),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let recorded: Vec<Value> = std::fs::read_to_string(&report)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let results: Vec<&Value> = recorded
        .iter()
        .filter(|frame| frame["type"] == "result")
        .collect();
    for (name, provider) in [
        ("report", &report_provider),
        ("interrupt", &interrupt_provider),
        ("interrupt_turn", &interrupt_turn_provider),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "start background work",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            provider,
        );
        let home = providers.path().join(name);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let mut end_sent = std::time::Instant::now();
        if name != "report" {
            let waiting = rpc
                .module_event(&exec, "module.background_tasks.replaced")
                .await;
            assert_eq!(waiting["payload"]["tasks"][0]["kind"], "agent");
            end_sent = std::time::Instant::now();
            let ended = result(
                rpc.call(
                    "modules.background.end",
                    end_wait_params(&d, task, 1, &Uuid::new_v4().to_string()),
                )
                .await,
            );
            assert_eq!(ended["disposition"], "delivered");
        }
        let outcome = timeout(Duration::from_secs(28), rpc.wait_outcome(&exec))
            .await
            .expect("replayed attempt settles");
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{name}: {outcome}"
        );
        let events = replay_events(&mut rpc, &d, &exec).await;
        let sets = events_of(&events, "module.background_tasks.replaced");
        assert_eq!(sets.len(), 2, "{name}: {events:?}");
        assert_eq!(sets[1]["payload"]["count"], 0, "{name}");
        if name == "report" {
            assert!(home.join("eof").exists());
            let usage = &outcome["result"]["usage"];
            assert_eq!(
                usage["output_tokens"].as_u64().unwrap(),
                results
                    .iter()
                    .map(|frame| frame["usage"]["output_tokens"].as_u64().unwrap())
                    .sum::<u64>(),
                "{outcome}"
            );
            assert_eq!(
                usage["total_cost_usd"], results[1]["total_cost_usd"],
                "{outcome}"
            );
            let finals = events_of(&events, "module.message.final");
            let report_text = results[1]["result"].as_str().unwrap();
            assert!(
                finals
                    .iter()
                    .any(|event| event["payload"]["content"] == report_text),
                "{events:?}"
            );
        } else {
            assert!(events_of(&events, "module.notice")
                .iter()
                .any(|event| event["payload"]["code"] == "background_wait_ended"));
            assert!(events_of(&events, "module.subtask.completed")
                .iter()
                .any(|event| event["payload"]["status"] == "stopped"));
        }
        if name == "interrupt_turn" {
            // End let the running turn have its grace before interrupting it;
            // the aborted turn and Claude's exit status 1 are End's effect.
            assert!(end_sent.elapsed() >= Duration::from_secs(9), "{outcome}");
            // The turn's foreground command, which Claude also registers as
            // a task, is not reported as background work.
            let agent = &sets[0]["payload"]["tasks"][0]["task_id"];
            for kind in [
                "module.subtask.started",
                "module.subtask.progress",
                "module.subtask.completed",
            ] {
                for event in events_of(&events, kind) {
                    assert_eq!(&event["payload"]["task_id"], agent, "{kind}: {event}");
                }
            }
        }
    }
}

/// Stop (cancel) ends a Claude attempt in every state of a background wait:
/// while it waits, during Claude's own report turn, and while that turn waits
/// on a native request. The provider ignores SIGINT; the reap does not need it.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_cancel_ends_a_background_wait() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-wait.sh",
        r#"#!/bin/sh
set -eu
trap '' INT
IFS= read -r initial
echo $$ > "$HOME/provider.pid"
sid="77777777-7777-4777-8777-777777777777"
init="{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' "$init"
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"bg9","task_type":"local_agent","description":"docker-ci"}]}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"running docker-ci","usage":{"input_tokens":1,"output_tokens":1}}'
case "$(basename "$HOME")" in
  report_turn|report_request)
    sleep 1
    printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
    printf '%s\n' "$init"
    ;;
esac
case "$(basename "$HOME")" in
  report_request)
    printf '%s\n' '{"type":"control_request","request_id":"provider-q-stop","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"report"}}}'
    ;;
esac
cat >/dev/null
while :; do sleep 1; done
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for mode in ["wait", "report_turn", "report_request"] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "run docker-ci",
            serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(mode);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        match mode {
            "wait" => {
                rpc.module_event(&exec, "module.background_tasks.replaced")
                    .await;
            }
            "report_turn" => {
                // The second report empties the set; the report turn follows.
                rpc.module_event(&exec, "module.background_tasks.replaced")
                    .await;
                rpc.module_event(&exec, "module.background_tasks.replaced")
                    .await;
                sleep(Duration::from_millis(300)).await;
            }
            _ => {
                rpc.module_event(&exec, "module.provider_request.opened")
                    .await;
            }
        }
        result(
            rpc.call(
                "modules.cancel",
                d.params(true, json!({"exec_id":exec,"reason":"owner_stop"})),
            )
            .await,
        );
        let outcome = timeout(Duration::from_secs(20), rpc.wait_outcome(&exec))
            .await
            .expect("stopped attempt settles");
        assert_eq!(outcome["status"], "cancelled", "{mode}: {outcome}");
        #[cfg(target_os = "linux")]
        {
            let pid: libc::pid_t = std::fs::read_to_string(home.join("provider.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert!(!process_exists(pid), "{mode}: provider survived Stop");
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_journals_distinct_blocks_and_result_only_answers() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "claude-transcript-fixture.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
printf '%s\n' '{"type":"system","subtype":"init","session_id":"11111111-1111-4111-8111-111111111111","model":"fixture-model","mcp_servers":[]}'
case "$initial" in
  *result-only*)
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"Result only answer","usage":{}}'
    ;;
  *)
    printf '%s\n' '{"type":"assistant","message":{"id":"native-1","role":"assistant","content":[{"type":"text","text":"First block."},{"type":"text","text":"Second block."}]}}'
    printf '%s\n' '{"type":"assistant","message":{"id":"native-2","role":"assistant","content":[{"type":"text","text":"Final answer."}]}}'
    printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"Final answer.","usage":{}}'
    ;;
esac
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({
        "schema_version":"g5.agent.permission-policy/v1",
        "provider":"claude_code","questions":"enabled","tool_approval":"prompt",
        "sandbox":"none","allowed_tools":[],
        "limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576,"native_response_timeout_ms":5000}
    });
    for (prompt, expected_ids, expected_text) in [
        (
            "multiple-blocks",
            vec!["native-1:0", "native-1:1", "native-2:0"],
            vec!["First block.", "Second block.", "Final answer."],
        ),
        (
            "result-only",
            vec!["g5:terminal-result"],
            vec!["Result only answer"],
        ),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            policy.clone(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        let finals: Vec<&Value> = replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "module.message.final")
            .collect();
        assert_eq!(
            finals
                .iter()
                .map(|event| event["payload"]["block_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_ids,
            "{replay}"
        );
        assert_eq!(
            finals
                .iter()
                .map(|event| event["payload"]["content"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_text,
            "{replay}"
        );
        assert_eq!(
            outcome["result"]["final_message_ref"],
            finals.last().unwrap()["payload"]["content_ref"]
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_claude_reads_the_provided_harness_outside_the_checkout() {
    let providers = tempfile::tempdir().unwrap();
    let home = providers.path().join("home");
    let claude = executable_script(
        providers.path(),
        "claude-harness-fixture.sh",
        r#"#!/bin/sh
set -eu
[ -z "${CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD-}" ] || exit 45
[ "$CLAUDE_CONFIG_DIR" = "$HOME/.claude" ] || exit 49
previous=""
for arg in "$@"; do
  [ "$arg" != --add-dir ] || exit 50
  if [ "$previous" = --setting-sources ]; then
    [ "$arg" = user,project ] || exit 51
  fi
  previous="$arg"
done
[ -s "$HOME/.claude/CLAUDE.md" ] || exit 46
[ -s "$HOME/.claude/skills/ontology-planning/SKILL.md" ] || exit 47
cmp "$G5_HARNESS_DIR/.agents/guidance/ontology.md" "$HOME/.agents/guidance/ontology.md" || exit 48
cmp "$G5_HARNESS_DIR/AGENTS.md" "$HOME/.claude/CLAUDE.md"
g5d pr --help >/dev/null
g5d pr create --json | grep -q providerStatus
[ -n "$G5D_API_URL" ] && [ -n "$G5D_WORKSPACE_ID" ] && [ -s "$G5D_WORKSPACE_TOKEN_FILE" ]
IFS= read -r initial
session_id="11111111-1111-4111-8111-111111111111"
printf '%s\n' "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$session_id\",\"model\":\"fixture-model\",\"mcp_servers\":[]}"
printf '%s\n' "{\"type\":\"assistant\",\"session_id\":\"$session_id\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"harness read\"}]}}"
printf '%s\n' "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"session_id\":\"$session_id\",\"result\":\"harness read\",\"usage\":{}}"
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let checkout_status = || {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(d.repo_root())
            .args([
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--ignored",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    let before = checkout_status();
    let mut rpc = d.connect().await;
    let policy = json!({
        "schema_version": "g5.agent.permission-policy/v1",
        "provider": "claude_code",
        "questions": "disabled",
        "tool_approval": "auto_deny",
        "sandbox": "none",
        "allowed_tools": [],
        "limits": {"setup_timeout_ms": 20_000, "max_output_bytes": 1024 * 1024},
    });

    // Without the runtime's harness the turn is refused before the provider
    // starts, instead of launching a bare client.
    let refused_task = Uuid::new_v4();
    let refused_exec = d.task_exec_id(refused_task, 1);
    let mut refused = agent_turn_invoke(
        &d,
        refused_task,
        1,
        "claude_code",
        "workflow",
        "must-not-spawn",
        policy.clone(),
        "G5_CLAUDE_PATH",
        &claude,
    );
    use_native_provider_fixture(&mut refused);
    refused["secrets"]["HOME"] = json!(home);
    refused["secrets"]["G5_HARNESS_DIR"] = json!(providers.path().join("missing"));
    result(rpc.call("modules.invoke", refused).await);
    let outcome = rpc.wait_outcome(&refused_exec).await;
    assert_eq!(outcome["result"]["status"], "failed", "{outcome}");
    assert_eq!(
        outcome["result"]["code"], "harness_unavailable",
        "{outcome}"
    );

    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "workflow",
        "read the harness",
        policy,
        "G5_CLAUDE_PATH",
        &claude,
    );
    use_native_provider_fixture(&mut params);
    params["secrets"]["HOME"] = json!(home);
    params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
    result(rpc.call("modules.invoke", params).await);
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");

    // The packaged instructions are copied into the native user location;
    // nothing reaches the member checkout.
    assert_eq!(
        std::fs::read(home.join(".claude/CLAUDE.md")).unwrap(),
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.agents/AGENTS.md"))
            .unwrap()
    );
    assert_cites_files_as_links(&home.join(".claude/CLAUDE.md"));
    assert_eq!(checkout_status(), before);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_codex_rejects_max_turns_before_provider_spawn() {
    let providers = tempfile::tempdir().unwrap();
    let marker = providers.path().join("codex-started");
    let codex = executable_script(
        providers.path(),
        "codex-unenforceable-limit.sh",
        &format!("#!/bin/sh\nprintf started > '{}'\n", marker.display()),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_turns":2,"max_output_bytes":1048576}});
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "codex",
        "workflow",
        "unsupported-turn-limit",
        policy,
        "G5_CODEX_PATH",
        &codex,
    );
    params["input"]["limits"]["max_turns"] = json!(2);
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let refused = rpc.wait_outcome(&exec).await;
    assert_eq!(refused["result"]["status"], "failed", "{refused}");
    assert_eq!(
        refused["result"]["code"], "unsupported_limit_max_turns",
        "{refused}"
    );
    assert!(!marker.exists(), "unsupported Codex limit spawned the CLI");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_codex_rejects_interactivity_before_spawn_and_runs_over_app_server() {
    let providers = tempfile::tempdir().unwrap();
    let marker = providers.path().join("codex-started");
    let preflight = format!(
        r#"printf started > '{}'
[ -s "$CODEX_HOME/AGENTS.md" ] || exit 46
[ -s "$CODEX_HOME/skills/ontology-planning/SKILL.md" ] || exit 47
cmp "$G5_HARNESS_DIR/.agents/guidance/ontology.md" "$HOME/.agents/guidance/ontology.md" || exit 48
cmp "$G5_HARNESS_DIR/AGENTS.md" "$CODEX_HOME/AGENTS.md"
g5d pr --help >/dev/null
g5d pr create --json | grep -q providerStatus
[ -n "$G5D_API_URL" ] && [ -n "$G5D_WORKSPACE_ID" ] && [ -s "$G5D_WORKSPACE_TOKEN_FILE" ]
printf '%s\n' "$@" > "$HOME/launch-args""#,
        marker.display()
    );
    let codex = codex_app_server_fixture(
        providers.path(),
        "codex-app-server-fixture.sh",
        &preflight,
        json!({"items":[{"id":"message-1","type":"agentMessage","text":"codex completed"}]}),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = |questions: &str, approval: &str| {
        json!({
            "schema_version": "g5.agent.permission-policy/v1",
            "provider": "codex",
            "questions": questions,
            "tool_approval": approval,
            "sandbox": "read-only",
            "allowed_tools": [],
            "limits": {
                "setup_timeout_ms": 20_000,
                "max_output_bytes": 1024 * 1024,
                "native_response_timeout_ms": 5_000,
            },
        })
    };

    let refused_task = Uuid::new_v4();
    let refused_exec = d.task_exec_id(refused_task, 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                agent_turn_invoke(
                    &d,
                    refused_task,
                    1,
                    "codex",
                    "workflow",
                    "must-not-spawn",
                    policy("enabled", "prompt"),
                    "G5_CODEX_PATH",
                    &codex,
                ),
            )
            .await,
        )["disposition"],
        "started"
    );
    let refused = rpc.wait_outcome(&refused_exec).await;
    assert_eq!(refused["status"], "succeeded", "{refused}");
    assert_eq!(refused["result"]["status"], "failed", "{refused}");
    assert_eq!(
        refused["result"]["code"], "unsupported_permission_policy",
        "{refused}"
    );
    assert!(!marker.exists(), "unsupported Codex policy spawned the CLI");

    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let home = providers.path().join("owner home with \"quotes\"");
    // A home retained from before the harness replaced the shipped profile.
    let codex_home = home.join(".codex");
    std::fs::create_dir_all(codex_home.join("skills/g5-rebase-workflow")).unwrap();
    std::fs::write(
        codex_home.join("skills/g5-rebase-workflow/SKILL.md"),
        "legacy",
    )
    .unwrap();
    std::fs::write(codex_home.join("profile_manifest.json"), "{}").unwrap();
    std::fs::write(
        codex_home.join("config.toml"),
        "model = \"owner-model\"\n[mcp_servers.g5-tools]\ncommand = \"g5d\"\nargs = [\"mcp\", \"intent-grounding\"]\nrequired = true\n",
    )
    .unwrap();
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "codex",
        "workflow",
        "run-with-stdin-eof",
        policy("disabled", "auto_deny"),
        "G5_CODEX_PATH",
        &codex,
    );
    use_native_provider_fixture(&mut params);
    params["secrets"]["HOME"] = json!(home);
    params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = rpc.wait_outcome(&exec).await;
    assert!(
        marker.exists(),
        "admitted Codex policy did not spawn the CLI"
    );
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    assert_eq!(
        std::fs::read_to_string(home.join("received-prompt")).unwrap(),
        "run-with-stdin-eof"
    );
    // The launch removed what the retained home kept from the legacy profile
    // and removed its retired MCP server.
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(codex_home.join("config.toml")).unwrap()).unwrap();
    assert_eq!(config["model"].as_str(), Some("owner-model"), "{config}");
    assert!(config
        .get("mcp_servers")
        .and_then(|servers| servers.get("g5-tools"))
        .is_none());
    assert!(!codex_home.join("skills/g5-rebase-workflow").exists());
    assert!(!codex_home.join("profile_manifest.json").exists());
    let launch_args = std::fs::read_to_string(home.join("launch-args")).unwrap();
    assert!(!launch_args.contains("g5-tools"), "{launch_args}");
    assert!(!std::fs::read_dir(home.join(".g5")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("codex-g5-tools-env-")
    }));
    assert_eq!(
        std::fs::read(codex_home.join("AGENTS.md")).unwrap(),
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.agents/AGENTS.md"))
            .unwrap()
    );
    assert_cites_files_as_links(&codex_home.join("AGENTS.md"));
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    assert!(
        replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| !event["kind"]
                .as_str()
                .unwrap_or_default()
                .contains("provider_request")),
        "Codex emitted an interactive request path: {replay}"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_device_auth_preserves_login_window_then_resumes_setup() {
    for (initially_logged_in, blocked_input) in [(false, false), (true, false), (false, true)] {
        let providers = tempfile::tempdir().unwrap();
        let home = providers.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        if initially_logged_in {
            std::fs::create_dir_all(home.join(".codex")).unwrap();
            std::fs::write(home.join(".codex/auth.json"), "fake-credential").unwrap();
        }
        let prompt = if blocked_input {
            "x".repeat(256 * 1024)
        } else {
            "original prompt".into()
        };
        let codex = codex_app_server_fixture(
            providers.path(),
            "codex-device-fixture.sh",
            r#"[ -s "$CODEX_HOME/AGENTS.md" ] || exit 46
if [ "$1" = login ]; then
  # Like the pinned CLI, explicit CODEX_HOME must already exist even for status.
  [ -d "$CODEX_HOME" ] || { echo 'Error loading configuration: CODEX_HOME does not exist' >&2; exit 1; }
  if [ "$2" = status ]; then
    if [ -f "$CODEX_HOME/auth.json" ]; then echo 'Logged in using ChatGPT'; exit 0; fi
    echo 'Not logged in' >&2; exit 1
  fi
  [ "$2" = --device-auth ] || exit 42
  printf 'https://auth.openai.com/codex/device\n\n  TEST-12345\n'
  sleep 6 # Owner response exceeds the five-second setup budget.
  umask 077
  printf fake-credential > "$CODEX_HOME/auth.json"
  exit 0
fi
[ "$1" = app-server ] || exit 43
[ -f "$CODEX_HOME/auth.json" ] || exit 44"#,
            json!({
                // Blocked input is written but never accepted.
                "block_if_prompt_contains": if blocked_input { json!("xxxxxxxx") } else { Value::Null },
                "block_seconds": 15,
                "items": [{"id":"message-1","type":"agentMessage","text":"signed in and completed"}],
            }),
        );
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "codex",
            "human",
            &prompt,
            policy,
            "G5_CODEX_PATH",
            &codex,
        );
        params["input"]["limits"]["setup_timeout_ms"] = json!(5000);
        params["input"]["provider"]["auth_mode"] = json!("chat_gpt_subscription");
        params["input"]["provider"]["options"]["fake_provider"] = json!(false);
        params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
        params["secrets"]["HOME"] = json!(home.to_string_lossy());
        params["secrets"]["CODEX_AUTH_JSON"] = json!("must-never-be-materialized");
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        if blocked_input {
            assert_eq!(outcome["result"]["status"], "timed_out", "{outcome}");
            assert_eq!(outcome["result"]["input_delivery_unknown"], true);
        } else {
            assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
            assert_eq!(
                std::fs::read_to_string(home.join("received-prompt")).unwrap(),
                prompt
            );
        }
        assert_eq!(
            std::fs::read_to_string(home.join(".codex/auth.json")).unwrap(),
            "fake-credential"
        );
        let phases = d.origin.as_ref().unwrap().codex_auth_events();
        assert_eq!(phases.first().unwrap()["phase"], "checking");
        assert_eq!(phases.last().unwrap()["phase"], "ready");
        assert_eq!(
            phases.iter().any(|phase| phase["phase"] == "waiting"),
            !initially_logged_in
        );
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        assert!(!replay.to_string().contains("TEST-12345"));
        assert!(!replay.to_string().contains("fake-credential"));
        assert_eq!(
            replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "module.input.accepted"),
            !blocked_input
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_device_auth_does_not_capture_api_key_or_openrouter_turns() {
    for (auth_mode, key, cached_subscription) in [
        ("open_ai_api_key", "OPENAI_API_KEY", false),
        ("open_ai_api_key", "OPENAI_API_KEY", true),
        ("open_router_api_key", "OPENROUTER_API_KEY", false),
        ("open_router_api_key", "OPENROUTER_API_KEY", true),
    ] {
        let providers = tempfile::tempdir().unwrap();
        let home = providers.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        if cached_subscription {
            std::fs::create_dir_all(home.join(".codex")).unwrap();
            std::fs::write(
                home.join(".codex/auth.json"),
                "saved-subscription-credential",
            )
            .unwrap();
        }
        let codex = codex_app_server_fixture(
            providers.path(),
            "codex-api-fixture.sh",
            r#"[ "$1" = app-server ] || exit 42
[ -s "$CODEX_HOME/skills/ontology-planning/SKILL.md" ] || exit 46
[ "$CODEX_API_KEY" = synthetic-api-key ] || exit 43
[ "$OPENAI_BASE_URL" = https://selected-provider.example/api/v1 ] || exit 44
case " $* " in *' model_provider="g5_selected" '*) ;; *) exit 45 ;; esac"#,
            json!({"items":[{"id":"message-1","type":"agentMessage","text":"API response"}]}),
        );
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "codex",
            "human",
            "API prompt",
            policy,
            "G5_CODEX_PATH",
            &codex,
        );
        params["input"]["provider"]["auth_mode"] = json!(auth_mode);
        params["input"]["provider"]["base_url"] = json!("https://selected-provider.example/api/v1");
        params["input"]["provider"]["options"]["fake_provider"] = json!(false);
        params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
        params["secrets"][key] = json!("synthetic-api-key");
        params["secrets"]["HOME"] = json!(home.to_string_lossy());
        let mut rpc = d.connect().await;
        result(rpc.call("modules.invoke", params).await);
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{auth_mode}: {outcome}"
        );
        assert_eq!(
            std::fs::read_to_string(home.join("received-prompt")).unwrap(),
            "API prompt"
        );
        if cached_subscription {
            assert_eq!(
                std::fs::read_to_string(home.join(".codex/auth.json")).unwrap(),
                "saved-subscription-credential"
            );
        } else {
            assert!(!home.join(".codex/auth.json").exists());
        }
    }
}

/// The owner's Stop during device login. A follow-up message no longer
/// cancels a pending login (`g5.chat.login-before-prompt`), so Stop is the
/// owner action that must kill the login's descendants before any prompt
/// could be delivered.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_device_auth_stop_kills_login_descendants_before_any_prompt_delivery() {
    let providers = tempfile::tempdir().unwrap();
    let home = providers.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let codex = executable_script(
        providers.path(),
        "codex-wait-fixture.sh",
        r#"#!/bin/sh
set -eu
if [ "$1" = login ]; then
  if [ "$2" = status ]; then echo 'Not logged in' >&2; exit 1; fi
  printf 'https://auth.openai.com/codex/device\n\n  TEST-12345\n'
  sleep 200 &
  echo $! > "$HOME/login-child-pid"
  wait
  exit 0
fi
echo forbidden > "$HOME/received-prompt"
exit 44
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "codex",
        "human",
        "never delivered",
        policy,
        "G5_CODEX_PATH",
        &codex,
    );
    params["input"]["provider"]["auth_mode"] = json!("chat_gpt_subscription");
    params["secrets"]["HOME"] = json!(home.to_string_lossy());
    let mut rpc = d.connect().await;
    result(rpc.call("modules.invoke", params).await);
    let pid = timeout(Duration::from_secs(10), async {
        loop {
            let pid = std::fs::read_to_string(home.join("login-child-pid"))
                .ok()
                .and_then(|contents| contents.trim().parse::<i32>().ok());
            let waiting = d
                .origin
                .as_ref()
                .unwrap()
                .codex_auth_events()
                .iter()
                .any(|report| report["phase"] == "waiting");
            if waiting {
                if let Some(pid) = pid {
                    break pid;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    result(
        rpc.call(
            "modules.cancel",
            d.params(true, json!({"exec_id":exec,"reason":"owner_cancel"})),
        )
        .await,
    );
    assert_eq!(rpc.wait_outcome(&exec).await["status"], "cancelled");
    timeout(Duration::from_secs(5), async {
        while unsafe { libc::kill(pid, 0) } == 0 {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("login descendant survived module cancellation");
    assert!(!home.join("received-prompt").exists());
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    assert!(!replay.to_string().contains("TEST-12345"));
    assert!(!replay.to_string().contains("input.accepted"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invoke_dispositions_are_idempotent_and_cancellation_survives_pruning() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;

    // Simultaneous duplicate invokes of one identity: exactly one launch,
    // every other handler observes `already_running`.
    let a = d.task_exec_id(Uuid::new_v4(), 1);
    let ids: Vec<u64> = (1000..1006).collect();
    for id in &ids {
        rpc.send(json!({"jsonrpc":"2.0","id":id,"method":"modules.invoke","params":fixture_invoke(&d, &a, json!({"behavior":"sleep","ms":20000}))})).await;
    }
    let mut dispositions = Vec::new();
    while dispositions.len() < ids.len() {
        let message = rpc.recv().await;
        if message["id"].as_u64().is_some_and(|id| ids.contains(&id)) {
            dispositions.push(result(message)["disposition"].as_str().unwrap().to_owned());
        } else {
            rpc.pending.push(message);
        }
    }
    assert_eq!(
        dispositions.iter().filter(|d| *d == "started").count(),
        1,
        "{dispositions:?}"
    );
    assert_eq!(
        dispositions
            .iter()
            .filter(|d| *d == "already_running")
            .count(),
        5,
        "{dispositions:?}"
    );
    let started_event = rpc.module_event(&a, "invocation.started").await;
    assert_eq!(started_event["seq"], 1);
    assert_eq!(started_event["payload"]["module_id"], FIXTURE_MODULE_ID);
    assert_eq!(
        started_event["payload"]["actor_user_id"],
        d.owner.to_string()
    );
    assert_eq!(
        result(
            rpc.call("modules.status", d.params(false, json!({"exec_id":a})))
                .await
        )["status"],
        "running"
    );
    let history = result(
        rpc.call("modules.history", d.params(false, json!({})))
            .await,
    );
    assert_eq!(history["executions"].as_array().unwrap().len(), 1);

    // Cancel signals the live run; the identity is `cancelled` thereafter on
    // every surface, while status still exposes the recorded outcome.
    let cancelled = result(
        rpc.call(
            "modules.cancel",
            d.params(false, json!({"exec_id":a,"reason":"owner stop"})),
        )
        .await,
    );
    assert_eq!(cancelled["disposition"], "signalling");
    let outcome = rpc.wait_outcome(&a).await;
    assert_eq!(outcome["status"], "cancelled", "{outcome}");
    let finished = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &a, json!({"behavior":"sleep","ms":1})),
        )
        .await,
    );
    assert_eq!(finished["disposition"], "cancelled");
    assert_eq!(finished["reason"], "owner stop");
    assert_eq!(
        result(
            rpc.call("modules.cancel", d.params(false, json!({"exec_id":a})))
                .await
        )["disposition"],
        "cancelled"
    );
    let status = result(
        rpc.call("modules.status", d.params(false, json!({"exec_id":a})))
            .await,
    );
    assert_eq!(status["status"], "finished");
    assert_eq!(status["outcome"]["status"], "cancelled");
    assert_eq!(status["persistence"], "journaled");

    // Acknowledging through the terminal event prunes the journal file; the
    // disposition survives and the identity still cannot be relaunched.
    let terminal_seq = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":a,"from_seq":1})),
        )
        .await,
    )["events"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["seq"]
        .as_u64()
        .unwrap();
    let pending = result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await,
    );
    assert!(pending["pending"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["exec_id"] == a && p["source"] == "journal"));
    let acked = result(
        rpc.call(
            "events.ack",
            d.params(true, json!({"exec_id":a,"up_to_seq":terminal_seq})),
        )
        .await,
    );
    assert_eq!(acked["acked_through"], terminal_seq);
    assert!(!d.state_dir().join("journal").join(&a).exists());
    assert!(d.state_dir().join("dispositions").join(&a).exists());
    let pending = result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await,
    );
    assert!(!pending["pending"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["exec_id"] == a));
    assert_eq!(
        result(
            rpc.call("events.replay", d.params(true, json!({"exec_id":a})))
                .await
        )["events"],
        json!([])
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &a, json!({"behavior":"echo"}))
            )
            .await
        )["disposition"],
        "cancelled"
    );

    // Cancel before invoke, for an identity the runtime has never seen: the
    // later invoke returns `cancelled` and spawns nothing.
    let b = d.task_exec_id(Uuid::new_v4(), 2);
    let recorded = result(
        rpc.call(
            "modules.cancel",
            d.params(false, json!({"exec_id":b,"reason":"stopped before start"})),
        )
        .await,
    );
    assert_eq!(recorded["disposition"], "cancelled");
    assert_eq!(
        result(
            rpc.call("modules.status", d.params(false, json!({"exec_id":b})))
                .await
        )["status"],
        "cancelled"
    );
    let refused = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &b, json!({"behavior":"echo"})),
        )
        .await,
    );
    assert_eq!(refused["disposition"], "cancelled");
    assert_eq!(refused["reason"], "stopped before start");
    assert!(!d.state_dir().join("journal").join(&b).exists());
    let history = result(
        rpc.call("modules.history", d.params(false, json!({})))
            .await,
    );
    assert!(!history["executions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["exec_id"] == b));

    // A completed identity replays its outcome with a digest.
    let c = d.task_exec_id(Uuid::new_v4(), 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &c, json!({"behavior":"echo","note":"hello"}))
            )
            .await
        )["disposition"],
        "started"
    );
    let outcome = rpc.wait_outcome(&c).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["echo"]["note"], "hello");
    assert_eq!(outcome["result"]["capabilities"], json!(["workspace.read"]));
    assert!(outcome["result"].get("has_supervisor_capability").is_none());
    let replay = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &c, json!({"behavior":"echo"})),
        )
        .await,
    );
    assert_eq!(replay["disposition"], "already_finished");
    assert_eq!(replay["outcome"], outcome);
    let status = result(
        rpc.call("modules.status", d.params(false, json!({"exec_id":c})))
            .await,
    );
    assert_eq!(status["status"], "finished");
    assert_eq!(status["outcome"]["status"], "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn journal_replay_after_disconnect_is_complete_and_ordered() {
    let d = fixture_dispatcher().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    {
        let mut rpc = d.connect().await;
        assert_eq!(
            result(
                rpc.call(
                    "modules.invoke",
                    fixture_invoke(
                        &d,
                        &exec,
                        json!({"behavior":"await_reply","request_id":"q-1"})
                    )
                )
                .await
            )["disposition"],
            "started"
        );
        let request = rpc.module_event(&exec, "module.input.request").await;
        assert_eq!(request["seq"], 2);
        assert_eq!(request["payload"]["request_id"], "q-1");
        // The peer goes away mid-run.
    }
    let mut rpc = d.connect().await;
    assert_eq!(rpc.ready["pending_outcomes"], 0);
    let answer = json!({"decision":"approve","note":"after reconnect"});
    let delivered = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-1", &answer),
        )
        .await,
    );
    assert_eq!(delivered["disposition"], "delivered");
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(
        outcome["result"]["replies"][0]["validated_response"],
        answer
    );

    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    let events = replay["events"].as_array().unwrap();
    let seqs: Vec<u64> = events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>());
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.first(), Some(&"invocation.started"));
    assert_eq!(kinds.get(1), Some(&"module.input.request"));
    assert_eq!(kinds.last(), Some(&"invocation.outcome"));
    // Once the control transport flushes the reply, the module may emit its
    // response before the delivery task journals `input.respond`. Both events
    // must be present exactly once and durably ordered before settlement, but
    // their relative order belongs to the runtime scheduler.
    let mut concurrent = kinds[2..4].to_vec();
    concurrent.sort_unstable();
    assert_eq!(concurrent, ["input.respond", "module.reply"]);
    assert!(events.iter().rev().skip(1).all(|e| e["terminal"] == false));
    assert_eq!(events.last().unwrap()["terminal"], true);
    assert_eq!(
        events
            .iter()
            .find(|event| event["kind"] == "input.respond")
            .unwrap()["payload"]["disposition"],
        "delivered"
    );
    let tail = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":4})),
        )
        .await,
    );
    assert_eq!(tail["events"].as_array().unwrap().len(), 2);
    assert_eq!(tail["has_more"], false);
    assert_eq!(tail["next_seq"], 6);
    // Replay serves bounded pages (`g5.workspace.module-execution`): a
    // consumer walks `next_seq` while `has_more` holds and sees every event
    // exactly once, in order.
    let mut paged = Vec::new();
    let mut from_seq = 1_u64;
    loop {
        let page = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":from_seq,"limit":2})),
            )
            .await,
        );
        let page_events = page["events"].as_array().unwrap();
        assert!(page_events.len() <= 2);
        paged.extend(page_events.iter().map(|e| e["seq"].as_u64().unwrap()));
        if page["has_more"] != true {
            break;
        }
        from_seq = page["next_seq"].as_u64().unwrap();
        assert_eq!(from_seq, paged.last().unwrap() + 1);
    }
    assert_eq!(paged, seqs);
    // `modules.events` is the owner-scoped read of the same journal.
    let owner_view = result(
        rpc.call(
            "modules.events",
            d.params(false, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    assert_eq!(owner_view["events"], replay["events"]);

    let pending = result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await,
    );
    let entry = pending["pending"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["exec_id"] == exec)
        .expect("unacknowledged outcome is pending");
    assert_eq!(entry["seq"], 5);
    assert_eq!(entry["outcome"]["status"], "succeeded");
    assert_eq!(entry["source"], "journal");
    // A partial ack keeps the outcome pending and the tail replayable.
    result(
        rpc.call(
            "events.ack",
            d.params(true, json!({"exec_id":exec,"up_to_seq":3})),
        )
        .await,
    );
    let remaining = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
            .await,
    );
    assert_eq!(
        remaining["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["seq"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [4, 5]
    );
    assert!(result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await
    )["pending"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["exec_id"] == exec));
    // Acknowledging beyond the journal's end is clamped to what exists.
    let acked = result(
        rpc.call(
            "events.ack",
            d.params(true, json!({"exec_id":exec,"up_to_seq":99})),
        )
        .await,
    );
    assert_eq!(acked["acked_through"], 5);
    assert!(!result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await
    )["pending"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["exec_id"] == exec));
    // Journal reads are private lifecycle protocol; an owner context cannot ack.
    error(
        rpc.call(
            "events.ack",
            d.params(false, json!({"exec_id":exec,"up_to_seq":5})),
        )
        .await,
        "LIFECYCLE_AUTHORITY_REQUIRED",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_events_are_queued_behind_an_in_flight_replay() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &exec,
                json!({"behavior":"await_reply","request_id":"q-1"}),
            ),
        )
        .await,
    );
    rpc.module_event(&exec, "module.input.request").await;
    // Two overlapping replays and a delivery race on the same connection:
    // whatever each replay snapshot contains, every live event that follows
    // its reply on the wire carries a higher sequence than the snapshot's
    // last event, and the stream itself stays in sequence order.
    rpc.send(json!({"jsonrpc":"2.0","id":"replay","method":"events.replay","params":d.params(true, json!({"exec_id":exec,"from_seq":1}))})).await;
    rpc.send(json!({"jsonrpc":"2.0","id":"replay-2","method":"events.replay","params":d.params(true, json!({"exec_id":exec,"from_seq":2}))})).await;
    rpc.send(json!({"jsonrpc":"2.0","id":"respond","method":"modules.input.respond","params":respond_params(&d, task, 1, "q-1", &json!({"decision":"go"}))})).await;
    let mut ordered = Vec::new();
    let mut replied = 0;
    while replied < 3
        || !ordered
            .iter()
            .any(|m: &Value| m["params"]["terminal"] == true)
    {
        let message = rpc.recv().await;
        if ["replay", "replay-2", "respond"].contains(&message["id"].as_str().unwrap_or("")) {
            replied += 1;
        }
        ordered.push(message);
    }
    for id in ["replay", "replay-2"] {
        let replay_index = ordered.iter().position(|m| m["id"] == id).unwrap();
        let snapshot_last = ordered[replay_index]["result"]["events"]
            .as_array()
            .unwrap()
            .last()
            .map(|e| e["seq"].as_u64().unwrap())
            .unwrap_or(0);
        for live in ordered
            .iter()
            .skip(replay_index + 1)
            .filter(|m| m["method"] == "module.event" && m["params"]["exec_id"] == exec)
        {
            assert!(
                live["params"]["seq"].as_u64().unwrap() > snapshot_last,
                "live event {live} was delivered after replay {id} already covered it"
            );
        }
    }
    let seqs: Vec<u64> = ordered
        .iter()
        .filter(|m| m["method"] == "module.event" && m["params"]["exec_id"] == exec)
        .map(|m| m["params"]["seq"].as_u64().unwrap())
        .collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "stream order {seqs:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_reply_delivery_duplicate_conflict_gone_and_stale() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(
                    &d,
                    &exec,
                    json!({"behavior":"await_reply","request_id":"q-1","replies_expected":2})
                )
            )
            .await
        )["disposition"],
        "started"
    );
    rpc.module_event(&exec, "module.input.request").await;

    // Before any delivery: a wrong attempt, an unknown identity, a bad
    // digest and a mismatched idempotency key are refused without touching
    // the module.
    let answer = json!({"decision":"approve"});
    assert_eq!(
        result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, task, 2, "q-1", &answer)
            )
            .await
        )["disposition"],
        "stale_execution"
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, Uuid::new_v4(), 1, "q-1", &answer)
            )
            .await
        )["disposition"],
        "stale_execution"
    );
    let mut wrong_digest = respond_params(&d, task, 1, "q-1", &answer);
    wrong_digest["response_digest"] = json!("0".repeat(64));
    error(
        rpc.call("modules.input.respond", wrong_digest).await,
        "RESPONSE_DIGEST_MISMATCH",
    );
    let mut wrong_key = respond_params(&d, task, 1, "q-1", &answer);
    wrong_key["context"]["idempotency_key"] = json!("unrelated");
    error(
        rpc.call("modules.input.respond", wrong_key).await,
        "INVALID_IDEMPOTENCY_KEY",
    );
    error(
        rpc.call(
            "modules.input.respond",
            d.params(false, json!({"exec_id":exec})),
        )
        .await,
        "LIFECYCLE_AUTHORITY_REQUIRED",
    );

    let first = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-1", &answer),
        )
        .await,
    );
    assert_eq!(first["disposition"], "delivered");
    let reply = rpc.module_event(&exec, "module.reply").await;
    assert_eq!(reply["payload"]["validated_response"], answer);
    assert_eq!(reply["payload"]["provider_request_id"], "provider-q-1");
    // An identical retry is deduplicated without redelivery.
    let retry = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-1", &answer),
        )
        .await,
    );
    assert_eq!(retry["disposition"], "already_delivered");
    // A different answer for the same request is a conflict.
    let conflict = error(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-1", &json!({"decision":"deny"})),
        )
        .await,
        "RESPONSE_CONFLICT",
    );
    assert_eq!(
        conflict["data"]["accepted_digest"],
        response_digest(&answer)
    );
    // The same answer under a different provider request identity is a
    // conflict too: the first accepted binding is immutable.
    let mut rebound = respond_params(&d, task, 1, "q-1", &answer);
    rebound["provider_request_id"] = json!("provider-other");
    let conflict = error(
        rpc.call("modules.input.respond", rebound).await,
        "RESPONSE_CONFLICT",
    );
    assert_eq!(
        conflict["data"]["accepted_provider_request_id"],
        "provider-q-1"
    );
    // Overlapping retries of one request: at most one delivery.
    for id in ["r1", "r2", "r3"] {
        rpc.send(json!({"jsonrpc":"2.0","id":id,"method":"modules.input.respond","params":respond_params(&d, task, 1, "q-2", &json!({"decision":"continue"}))})).await;
    }
    let mut dispositions = Vec::new();
    while dispositions.len() < 3 {
        let message = rpc.recv().await;
        if ["r1", "r2", "r3"].contains(&message["id"].as_str().unwrap_or("")) {
            dispositions.push(result(message)["disposition"].as_str().unwrap().to_owned());
        } else {
            rpc.pending.push(message);
        }
    }
    assert_eq!(
        dispositions.iter().filter(|d| *d == "delivered").count(),
        1,
        "{dispositions:?}"
    );
    assert_eq!(
        dispositions
            .iter()
            .filter(|d| *d == "already_delivered")
            .count(),
        2,
        "{dispositions:?}"
    );
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["replies"].as_array().unwrap().len(), 2);

    // After the execution ended the request is gone, the earlier delivery
    // result is retained for the attempt's lifetime, and nothing is appended
    // after the terminal event.
    let gone = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-3", &json!({"late":true})),
        )
        .await,
    );
    assert_eq!(gone["disposition"], "request_gone");
    assert_eq!(
        result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, task, 1, "q-1", &answer)
            )
            .await
        )["disposition"],
        "already_delivered"
    );
    let journal = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
            .await,
    );
    let dispositions: Vec<&str> = journal["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "input.respond")
        .map(|e| e["payload"]["disposition"].as_str().unwrap())
        .collect();
    assert_eq!(dispositions, ["delivered", "delivered"]);
    assert_eq!(
        journal["events"].as_array().unwrap().last().unwrap()["terminal"],
        true
    );

    // A module that declares no `input.respond` operation has no channel.
    let plain_task = Uuid::new_v4();
    let plain = d.task_exec_id(plain_task, 1);
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &plain,
                "workspace.diagnostics",
                "probe",
                json!({"name":"runtime-health"}),
            ),
        )
        .await,
    );
    rpc.wait_outcome(&plain).await;
    error(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, plain_task, 1, "q-1", &answer),
        )
        .await,
        "INPUT_CHANNEL_UNDECLARED",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reply_delivery_is_owned_by_the_execution_and_a_timed_out_write_never_lands() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            fixture_wall_clock_ms: Some(40_000),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;

    // The handler's deadline expires while the module is not yet reading:
    // the delivery still completes once, and a retry sees `already_delivered`.
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &exec,
                json!({"behavior":"slow_reader","request_id":"q-1","delay_ms":1500}),
            ),
        )
        .await,
    );
    rpc.module_event(&exec, "module.input.request").await;
    // A response larger than the pipe buffer: the write itself only
    // completes once the module reads, so the RPC deadline expires mid-write.
    let answer = json!({"decision":"approve","padding":"x".repeat(200 * 1024)});
    let mut short = respond_params(&d, task, 1, "q-1", &answer);
    short["context"]["deadline_ms"] = json!(now_ms() + 300);
    error(
        rpc.call("modules.input.respond", short).await,
        "DEADLINE_EXCEEDED",
    );
    sleep(Duration::from_millis(2000)).await;
    let retry = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-1", &answer),
        )
        .await,
    );
    assert_eq!(retry["disposition"], "already_delivered");
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert_eq!(outcome["result"]["replies"].as_array().unwrap().len(), 1);
    let journal = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
            .await,
    );
    let dispositions: Vec<&str> = journal["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "input.respond")
        .map(|e| e["payload"]["disposition"].as_str().unwrap())
        .collect();
    assert_eq!(dispositions, ["delivered"]);

    // The module does not take the bytes within the acknowledgement bound:
    // the disposition is `request_gone`, the channel is closed, and the
    // late reader finds no frame — it never receives a "gone" response.
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &exec,
                json!({"behavior":"slow_reader","request_id":"q-late","delay_ms":13_000}),
            ),
        )
        .await,
    );
    rpc.module_event(&exec, "module.input.request").await;
    let started = std::time::Instant::now();
    let gone = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, task, 1, "q-late", &answer),
        )
        .await,
    );
    assert_eq!(gone["disposition"], "request_gone", "{gone}");
    assert!(
        started.elapsed() >= Duration::from_secs(9),
        "the negative disposition must wait out the write bound"
    );
    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
        .await
        .expect("late reader settles");
    assert_eq!(outcome["status"], "failed", "{outcome}");
    assert_eq!(outcome["result"]["reason"], "control_closed");
    assert_eq!(outcome["result"]["replies"], json!([]));
    assert_eq!(
        result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, task, 1, "q-late", &answer)
            )
            .await
        )["disposition"],
        "request_gone"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn running_and_new_agent_cli_requests_use_only_accepted_lease_renewals() {
    use std::os::unix::fs::PermissionsExt;
    let api = MockServer::start().await;
    let providers = tempfile::tempdir().unwrap();
    let home = providers.path().join("home");
    let provider = executable_script(
        providers.path(),
        "claude-renewal.sh",
        r#"#!/bin/sh
set -eu
IFS= read -r initial
[ -z "${G5D_WORKSPACE_TOKEN-}" ]
[ -s "$G5D_WORKSPACE_TOKEN_FILE" ]
printf ready > "$HOME/ready"
while [ ! -f "$HOME/continue" ]; do sleep 0.02; done
g5d --api-url "$ROTATION_TEST_API" question list --json > "$HOME/questions.json"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"renewed request succeeded","usage":{}}'
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            lease_ttl_secs: 10,
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "wait for renewal",
        policy.clone(),
        "G5_CLAUDE_PATH",
        &provider,
    );
    params["secrets"]["HOME"] = json!(home);
    params["secrets"]["ROTATION_TEST_API"] = json!(api.uri());
    result(rpc.call("modules.invoke", params).await);
    let ready = timeout(Duration::from_secs(4), async {
        while !home.join("ready").exists() {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if ready.is_err() {
        let outcome = timeout(Duration::from_secs(2), rpc.wait_outcome(&exec)).await;
        panic!("provider did not start: {outcome:?}");
    }
    let credential = d.state_dir().join("workspace-token");
    assert_eq!(
        std::fs::metadata(&credential).unwrap().permissions().mode() & 0o777,
        0o440
    );
    assert_eq!(
        std::fs::read_to_string(&credential).unwrap(),
        d.workspace_token
    );
    let renewal = rpc.event("lease.renew").await;
    let rotated = workspace_token_with_expiry(d.vm, now_ms() / 1000 + 900);
    let wrong_vm = workspace_token_with_expiry(Uuid::new_v4(), now_ms() / 1000 + 900);
    for token in [&wrong_vm, &rotated, &d.workspace_token] {
        rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"result":{"seq":renewal["params"]["seq"],"token":token,"lease_ttl_secs":120}})).await;
        result(rpc.call("health", d.params(true, json!({}))).await);
        let expected = if token == &wrong_vm {
            &d.workspace_token
        } else {
            &rotated
        };
        assert_eq!(std::fs::read_to_string(&credential).unwrap(), *expected);
    }
    d.origin.as_ref().unwrap().rotate_token(&rotated);
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{}/questions",
            d.workspace
        )))
        .and(header("authorization", format!("Bearer {rotated}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"sets":[]}})))
        .expect(2)
        .mount(&api)
        .await;
    std::fs::write(home.join("continue"), "go").unwrap();
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "new turn after renewal",
        policy,
        "G5_CLAUDE_PATH",
        &provider,
    );
    params["secrets"]["HOME"] = json!(home);
    params["secrets"]["ROTATION_TEST_API"] = json!(api.uri());
    result(rpc.call("modules.invoke", params).await);
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    api.verify().await;
    assert_eq!(
        d.origin.as_ref().unwrap().unauthorized(),
        0,
        "session saves and new Git admission use the renewed credential"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rotated_lease_token_is_redacted_from_a_running_module() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            fixture_wall_clock_ms: Some(40_000),
            lease_ttl_secs: 6,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    let secret_file = d.root.path().join("rotated-token.txt");
    let exec = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &exec,
                json!({"behavior":"leak_file","path":secret_file}),
            ),
        )
        .await,
    );
    rpc.module_event(&exec, "invocation.started").await;

    // The host renews the lease with a token the dispatcher has never seen.
    let renewal = rpc.event("lease.renew").await;
    let rotated = workspace_token_with_expiry(d.vm, now_ms() / 1000 + 900);
    assert_ne!(rotated, d.workspace_token);
    rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"result":{"seq":renewal["params"]["seq"],"token":rotated,"lease_ttl_secs":120}})).await;
    // The reply is processed in order before this request is answered.
    result(rpc.call("health", d.params(true, json!({}))).await);
    std::fs::write(&secret_file, &rotated).unwrap();

    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
        .await
        .expect("module settles");
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    let logs = result(
        rpc.call("modules.logs", d.params(false, json!({"exec_id":exec})))
            .await,
    );
    assert_eq!(logs["chunk"], "rotated=[REDACTED] end\n", "{logs}");
    assert!(!logs.to_string().contains(&rotated));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn limits_stop_a_module_with_a_definite_outcome() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;

    let output = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &output,
                json!({"behavior":"spew","stream":"stdout","bytes":FIXTURE_MAX_OUTPUT_BYTES*4}),
            ),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&output).await;
    assert_eq!(outcome["status"], "limit_exceeded", "{outcome}");
    assert_eq!(outcome["detail"]["limit"], "max_output_bytes");

    // A newline-free flood is bounded by bytes, not lines.
    let flood = d.task_exec_id(Uuid::new_v4(), 1);
    result(rpc.call("modules.invoke", fixture_invoke(&d, &flood, json!({"behavior":"spew","stream":"stdout","bytes":FIXTURE_MAX_OUTPUT_BYTES*4,"newlines":false}))).await);
    let outcome = rpc.wait_outcome(&flood).await;
    assert_eq!(outcome["status"], "limit_exceeded", "{outcome}");

    let logs = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &logs,
                json!({"behavior":"spew","stream":"stderr","bytes":FIXTURE_MAX_LOG_BYTES*4}),
            ),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&logs).await;
    assert_eq!(outcome["status"], "limit_exceeded", "{outcome}");
    assert_eq!(outcome["detail"]["limit"], "max_log_bytes");
    let retained = result(
        rpc.call("modules.logs", d.params(false, json!({"exec_id":logs})))
            .await,
    );
    assert!(retained["chunk"].as_str().unwrap().starts_with("xxxx"));
    assert!(retained["cursor"].as_u64().unwrap() > 0);

    let started = std::time::Instant::now();
    let clock = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &clock,
                json!({"behavior":"sleep","ms":FIXTURE_WALL_CLOCK_MS*10}),
            ),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&clock).await;
    assert_eq!(outcome["status"], "timed_out", "{outcome}");
    assert_eq!(outcome["detail"]["wall_clock_ms"], FIXTURE_WALL_CLOCK_MS);
    assert!(started.elapsed() < Duration::from_millis(FIXTURE_WALL_CLOCK_MS * 3));

    let failing = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &failing, json!({"behavior":"exit","code":3})),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&failing).await;
    assert_eq!(outcome["status"], "failed");
    assert_eq!(outcome["exit_code"], 3);
    assert_eq!(outcome["result"]["reason"], "requested_exit");

    let plain = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &plain, json!({"behavior":"plain_output"})),
        )
        .await,
    );
    let line = rpc.module_event(&plain, "module.output").await;
    assert_eq!(line["payload"]["line"], "hello plain world");
    assert_eq!(rpc.wait_outcome(&plain).await["status"], "succeeded");

    // A descendant holding the module's pipes open does not hang settlement
    // and does not outlive the module's process group.
    let marker = d.root.path().join("orphan-marker");
    let orphan = d.task_exec_id(Uuid::new_v4(), 1);
    let started = std::time::Instant::now();
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &orphan,
                json!({"behavior":"orphan","ms":1500,"marker":marker}),
            ),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&orphan).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "settlement waited for an orphaned pipe holder"
    );
    sleep(Duration::from_millis(2500)).await;
    assert!(
        !marker.exists(),
        "an orphaned descendant survived module settlement"
    );

    // A descendant that escaped the module's process group and holds only
    // stderr past the drain grace: settlement is definite
    // (`output_drain_timeout`), no reader is polled twice, and the dispatcher
    // keeps answering.
    let stderr_only = d.task_exec_id(Uuid::new_v4(), 1);
    let marker = d.root.path().join("orphan-stderr-marker");
    let started = std::time::Instant::now();
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &stderr_only,
                json!({"behavior":"orphan","ms":20000,"marker":marker,"hold":"stderr","escape":true}),
            ),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&stderr_only).await;
    assert_eq!(outcome["status"], "failed", "{outcome}");
    assert_eq!(outcome["detail"]["reason"], "output_drain_timeout");
    assert!(
        started.elapsed() < Duration::from_secs(12),
        "escaped stderr holder delayed settlement"
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.status",
                d.params(false, json!({"exec_id":stderr_only}))
            )
            .await
        )["status"],
        "finished"
    );

    let history = result(
        rpc.call(
            "modules.history",
            d.params(false, json!({"filter":{"module_id":FIXTURE_MODULE_ID}})),
        )
        .await,
    );
    assert_eq!(history["executions"].as_array().unwrap().len(), 8);
    assert!(history["executions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["status"]["finished"].is_object() && e["persistence"] == "journaled"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn control_overrides_and_undeclared_requests_are_refused_before_spawn() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;
    let exec = d.task_exec_id(Uuid::new_v4(), 1);

    for key in ["argv", "env", "cwd", "signal", "entrypoint", "uid"] {
        let mut params = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
        params[key] = json!("steer");
        let refused = error(
            rpc.call("modules.invoke", params).await,
            "MODULE_CONTROL_OVERRIDE",
        );
        assert_eq!(refused["data"]["key"], key);
        let mut nested = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
        nested["input"][key] = json!(["steer"]);
        error(
            rpc.call("modules.invoke", nested).await,
            "MODULE_CONTROL_OVERRIDE",
        );
    }
    error(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, "task.not-a-uuid.1.x", json!({})),
        )
        .await,
        "INVALID_EXEC_ID",
    );
    error(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &format!("task.{}.1.{}", Uuid::new_v4(), Uuid::new_v4()),
                json!({}),
            ),
        )
        .await,
        "INVALID_EXEC_ID",
    );
    let mut unknown = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    unknown["module_id"] = json!("shell.exec");
    error(
        rpc.call("modules.invoke", unknown).await,
        "MODULE_NOT_FOUND",
    );
    let mut operation = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    operation["operation"] = json!("exec");
    error(
        rpc.call("modules.invoke", operation).await,
        "OPERATION_NOT_FOUND",
    );
    let mut reply_channel = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    reply_channel["operation"] = json!("input.respond");
    error(
        rpc.call("modules.invoke", reply_channel).await,
        "OPERATION_NOT_FOUND",
    );
    let mut schema = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    schema["input_schema"] = json!("g5.other/v1");
    assert_eq!(
        error(
            rpc.call("modules.invoke", schema).await,
            "INPUT_SCHEMA_MISMATCH"
        )["data"]["expected"],
        FIXTURE_INPUT_SCHEMA
    );
    let mut narrowed = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    narrowed["grant"] = d.grant(&[]);
    assert_eq!(
        error(
            rpc.call("modules.invoke", narrowed).await,
            "CAPABILITY_DENIED"
        )["data"]["missing"],
        json!(["workspace.read"])
    );
    let mut preview = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    preview["grant"] = d.grant(&["workspace.read", "preview.service"]);
    error(
        rpc.call("modules.invoke", preview).await,
        "CAPABILITY_DENIED",
    );
    let mut supervisor = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    supervisor["supervisor_capability"] = json!({"secret":"ab".repeat(32),"binding":{}});
    error(
        rpc.call("modules.invoke", supervisor).await,
        "UNDECLARED_FIELD",
    );
    let mut malformed_grant = fixture_invoke(&d, &exec, json!({"behavior":"echo"}));
    malformed_grant["grant"] = json!({"grant_id":"not-a-uuid","capabilities":["workspace.read"]});
    error(
        rpc.call("modules.invoke", malformed_grant).await,
        "INVALID_GRANT",
    );

    // Built-in inputs are typed: a malformed identifier, an undeclared
    // field, an escaping path field and an unknown probe are refused before
    // anything is reserved.
    let builtin = d.operation_exec_id(Uuid::new_v4());
    let bootstrap = d.task_exec_id(Uuid::new_v4(), 1);
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &builtin,
                "workspace.changes",
                "get",
                json!({"repo_id":[]}),
            ),
        )
        .await,
        "INVALID_INPUT",
    );
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &builtin,
                "workspace.changes",
                "push",
                json!({"path":"../../etc"}),
            ),
        )
        .await,
        "INVALID_INPUT",
    );
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &builtin,
                "workspace.diagnostics",
                "probe",
                json!({"name":"disk-usage"}),
            ),
        )
        .await,
        "UNKNOWN_PROBE",
    );
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &bootstrap,
                "workspace.bootstrap",
                "run",
                json!({"profile":"prod"}),
            ),
        )
        .await,
        "UNKNOWN_PROFILE",
    );
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &bootstrap,
                "workspace.bootstrap",
                "run",
                json!({"graph":"not-an-object"}),
            ),
        )
        .await,
        "INVALID_INPUT",
    );

    // Zero-spawn observation: nothing above reached the runner.
    assert_eq!(
        result(
            rpc.call("modules.history", d.params(false, json!({})))
                .await
        )["executions"],
        json!([])
    );
    assert_eq!(
        result(
            rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
                .await
        )["status"],
        "unknown"
    );
    assert!(!d.state_dir().join("journal").join(&exec).exists());
    assert_eq!(
        result(
            rpc.call("runtime.inspect.summary", d.params(false, json!({})))
                .await
        )["modules"]["bounded_slots_in_use"],
        0
    );

    // Ordinary typed input that merely mentions the words is fine.
    let allowed = result(rpc.call("modules.invoke", fixture_invoke(&d, &exec, json!({"behavior":"echo","prompt":"please run argv with env in cwd and send a signal"}))).await);
    assert_eq!(allowed["disposition"], "started");
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(
        outcome["result"]["echo"]["prompt"],
        "please run argv with env in cwd and send a signal"
    );
    assert_eq!(
        result(
            rpc.call("modules.history", d.params(false, json!({})))
                .await
        )["executions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn secrets_are_redacted_across_chunks_and_never_enter_module_environment() {
    let d = fixture_dispatcher().await;
    let mut rpc = d.connect().await;
    let token = d.workspace_token.clone();

    let leak = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &leak, json!({"behavior":"leak","text":token})),
        )
        .await,
    );
    let progress = rpc.module_event(&leak, "module.progress").await;
    assert_eq!(progress["payload"]["leaked"], "[REDACTED]");
    let outcome = rpc.wait_outcome(&leak).await;
    assert_eq!(outcome["result"]["echoed"], "[REDACTED]");
    let logs = result(
        rpc.call("modules.logs", d.params(false, json!({"exec_id":leak})))
            .await,
    );
    assert_eq!(logs["chunk"], "token=[REDACTED] end\n");
    let replay = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":leak})))
            .await,
    );
    assert!(!replay.to_string().contains(&token));

    // A secret split across two consecutive structured text events is masked
    // in both frames, so no transcript reconstruction exposes it.
    let split = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &split, json!({"behavior":"split_events","text":token})),
        )
        .await,
    );
    assert_eq!(rpc.wait_outcome(&split).await["status"], "succeeded");
    let frames = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":split})))
            .await,
    );
    let texts: Vec<String> = frames["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "module.delta")
        .map(|e| e["payload"]["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(texts.len(), 2, "{frames}");
    assert!(!texts.concat().contains(&token), "{texts:?}");
    assert!(texts.iter().all(|t| t.contains("[REDACTED]")), "{texts:?}");

    // Three fragments, and two fragments separated by more than the hold:
    // an unresolved prefix is carried (and masked) across any number of
    // frames, so the concatenation never contains the secret.
    for (label, input) in [
        (
            "three frames",
            json!({"behavior":"split_events","text":token,"parts":3}),
        ),
        (
            "delayed frames",
            json!({"behavior":"split_events","text":token,"parts":2,"gap_ms":200}),
        ),
    ] {
        let exec = d.task_exec_id(Uuid::new_v4(), 1);
        result(
            rpc.call("modules.invoke", fixture_invoke(&d, &exec, input))
                .await,
        );
        assert_eq!(
            rpc.wait_outcome(&exec).await["status"],
            "succeeded",
            "{label}"
        );
        let frames = result(
            rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
                .await,
        );
        let texts: Vec<String> = frames["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "module.delta")
            .map(|e| e["payload"]["text"].as_str().unwrap().to_owned())
            .collect();
        assert!(!texts.concat().contains(&token), "{label}: {texts:?}");
        assert!(texts.concat().contains("[REDACTED]"), "{label}: {texts:?}");
    }

    // The second half of a secret arriving only in the outcome frame is
    // masked there too; the outcome is not journaled as an event but takes
    // part in the boundary resolution.
    for interleave in [false, true] {
        let into_outcome = d.task_exec_id(Uuid::new_v4(), 1);
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(
                    &d,
                    &into_outcome,
                    json!({"behavior":"split_into_outcome","text":token,"interleave":interleave}),
                ),
            )
            .await,
        );
        let outcome = rpc.wait_outcome(&into_outcome).await;
        assert_eq!(outcome["status"], "succeeded", "{outcome}");
        let delta = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":into_outcome})),
            )
            .await,
        )["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "module.delta")
            .unwrap()["payload"]["text"]
            .as_str()
            .unwrap()
            .to_owned();
        let echoed = outcome["result"]["echo"].as_str().unwrap().to_owned();
        assert!(
            !format!("{delta}{echoed}").contains(&token),
            "interleave {interleave}: {delta} + {echoed}"
        );
        assert!(
            echoed.contains("[REDACTED]"),
            "interleave {interleave}: {echoed}"
        );
    }

    let env = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &env, json!({"behavior":"env_dump"})),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&env).await;
    let vars = outcome["result"]["env"].as_object().unwrap();
    assert_eq!(vars["G5D_EXEC_ID"], env);
    assert_eq!(vars["G5D_MODULE_ID"], FIXTURE_MODULE_ID);
    assert_eq!(vars["G5D_MODULE_OPERATION"], "run");
    assert_eq!(
        vars["G5D_WORKSPACE_ROOT"],
        d.workspace_root().to_str().unwrap()
    );
    assert_eq!(
        std::fs::canonicalize(outcome["result"]["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(d.workspace_root()).unwrap()
    );
    for forbidden in [
        "G5D_WORKSPACE_TOKEN",
        "G5D_WORKSPACE_TOKEN_FILE",
        "G5D_DISPATCH_TOKEN",
        "G5D_MODULE_CATALOG_HASH",
        "G5D_API_URL",
    ] {
        assert!(
            !vars.contains_key(forbidden),
            "{forbidden} leaked into the module environment"
        );
    }
    assert!(vars.values().all(|v| !v.as_str().unwrap().contains(&token)));
    assert!(vars
        .values()
        .all(|v| !v.as_str().unwrap().contains(&hex::encode(d.listener_token))));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_module_loader_is_host_owned_without_dispatch_secret_injection() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            env: inherited_loader_env(),
            ..Default::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    // The production API does not send loader paths as dispatch secrets.
    // The re-exec boundary must supply its own trusted runtime environment.
    #[cfg(target_os = "linux")]
    let expected = [("LD_LIBRARY_PATH", "/opt/g5/bin".to_owned())];
    #[cfg(target_os = "macos")]
    let expected = [(
        "DYLD_LIBRARY_PATH",
        std::env::var("DYLD_LIBRARY_PATH").unwrap_or_default(),
    )];
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let expected: [(&str, String); 0] = [];
    for overwrite in [false, true] {
        let exec = d.task_exec_id(Uuid::new_v4(), 1);
        let mut params = fixture_invoke(&d, &exec, json!({"behavior":"env_dump"}));
        if overwrite {
            for (key, _) in &expected {
                params["secrets"][*key] = json!("/tmp/untrusted-dispatch-loader");
            }
        }
        result(rpc.call("modules.invoke", params).await);
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["status"], "succeeded", "{outcome}");
        let env = outcome["result"]["env"].as_object().unwrap();
        for (key, value) in &expected {
            assert_eq!(env.get(*key), Some(&json!(value)), "{outcome}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_short_configured_secret_split_into_single_byte_frames_is_masked() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            env: vec![("G5D_SECRET_TINY".into(), "k9q".into())],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    for (label, input) in [
        (
            "three single bytes",
            json!({"behavior":"split_events","text":"k9q","parts":3}),
        ),
        (
            "three single bytes, delayed",
            json!({"behavior":"split_events","text":"k9q","parts":3,"gap_ms":200}),
        ),
        // A frame without any text between the fragments neither continues
        // nor breaks the chain.
        (
            "three single bytes around frames without text",
            json!({"behavior":"split_events","text":"k9q","parts":3,"interleave":true}),
        ),
        (
            "three single bytes around frames without text, delayed",
            json!({"behavior":"split_events","text":"k9q","parts":3,"gap_ms":200,"interleave":true}),
        ),
    ] {
        let exec = d.task_exec_id(Uuid::new_v4(), 1);
        result(
            rpc.call("modules.invoke", fixture_invoke(&d, &exec, input))
                .await,
        );
        assert_eq!(
            rpc.wait_outcome(&exec).await["status"],
            "succeeded",
            "{label}"
        );
        let frames = result(
            rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
                .await,
        );
        let texts: Vec<String> = frames["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "module.delta")
            .map(|e| e["payload"]["text"].as_str().unwrap().to_owned())
            .collect();
        // The single leading byte stays as emitted; every continuation is
        // masked.
        assert_eq!(
            texts,
            ["part one k", "[REDACTED]", "[REDACTED] last part"],
            "{label}: {frames}"
        );
    }
}

/// A delta that merely opens like a long secret is ordinary text: "Hey"
/// ends in the two bytes every JWT starts with, and neither a hold expiry
/// nor a frame without text between it and the next delta may mask it
/// (QA, 2026-09-23: "H[REDACTED]"). A delta that opens a short secret, or
/// carries eight bytes of a long one, is still masked at the boundary.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delta_that_merely_opens_a_long_secret_is_not_masked_at_the_boundary() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnopqrstuvwxyz";
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            env: vec![
                ("G5D_SECRET_JWT".into(), jwt.into()),
                ("G5D_SECRET_TINY".into(), "k9q".into()),
            ],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    // The carried "ey" chain is completed only by the exact remainder of the
    // token; a fragment that diverges from it is not the secret and stays
    // in the clear, as it always did.
    let remainder = format!("{} done", &jwt[2..]);
    for (label, frames, expected) in [
        (
            "next delta after the hold expired",
            json!([{"payload":"Hey","gap_ms":80},{"payload":"! there"}]),
            vec![json!("Hey"), json!("! there")],
        ),
        (
            "frame without text between the deltas",
            json!([{"payload":"Hey"},{"payload":{"tokens":3}},{"payload":"! there"}]),
            vec![json!("Hey"), json!({"tokens":3}), json!("! there")],
        ),
        (
            "a real continuation is still masked",
            json!([{"payload":"Hey","gap_ms":80},{"payload":remainder}]),
            vec![json!("Hey"), json!("[REDACTED] done")],
        ),
        (
            "two bytes of a short secret stay masked",
            json!([{"payload":"token k9","gap_ms":80},{"payload":"x"}]),
            vec![json!("token [REDACTED]"), json!("x")],
        ),
    ] {
        let exec = d.task_exec_id(Uuid::new_v4(), 1);
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &exec, json!({"behavior":"scripted","frames":frames})),
            )
            .await,
        );
        assert_eq!(
            rpc.wait_outcome(&exec).await["status"],
            "succeeded",
            "{label}"
        );
        let deltas: Vec<Value> = result(
            rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
                .await,
        )["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "module.delta")
            .map(|e| e["payload"].clone())
            .collect();
        assert_eq!(deltas, expected, "{label}");
    }
}

/// Redaction decisions across frames are made on original text: a chain
/// completion never hides a boundary match in another leaf, the chain set
/// never evicts a viable chain, and literal mask text a module emits is
/// ordinary output.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn secret_fragments_across_multi_leaf_frames_and_literal_mask_text_are_masked() {
    async fn scripted_deltas(secrets: Vec<(&str, String)>, frames: Value) -> Vec<Value> {
        scripted(secrets, frames, None).await.0
    }

    async fn scripted(
        secrets: Vec<(&str, String)>,
        frames: Value,
        outcome: Option<Value>,
    ) -> (Vec<Value>, Value) {
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                fixture_module: true,
                env: secrets
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
                ..DispatcherOptions::default()
            },
        )
        .await;
        let mut rpc = d.connect().await;
        let exec = d.task_exec_id(Uuid::new_v4(), 1);
        let mut input = json!({"behavior":"scripted","frames":frames});
        if let Some(outcome) = outcome {
            input["outcome"] = outcome;
        }
        result(
            rpc.call("modules.invoke", fixture_invoke(&d, &exec, input))
                .await,
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["status"], "succeeded", "{outcome}");
        let deltas = result(
            rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
                .await,
        )["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "module.delta")
            .map(|e| e["payload"].clone())
            .collect();
        (deltas, outcome["result"].clone())
    }

    // A chain completed by the first leaf of a frame and a whole secret
    // spanning the previous frame's last leaf and that same first leaf:
    // both are masked, on both sides.
    let deltas = scripted_deltas(
        vec![
            ("G5D_SECRET_A", "k9cd".into()),
            ("G5D_SECRET_B", "abcd".into()),
        ],
        json!([
            {"payload":"k","gap_ms":60},
            {"payload":["9","ab"]},
            {"payload":"cd"}
        ]),
    )
    .await;
    assert_eq!(
        deltas,
        [
            json!("k"),
            json!(["[REDACTED]", "[REDACTED]"]),
            json!("[REDACTED]")
        ]
    );

    // Eleven chains alive at once: each two-leaf frame extends every chain
    // through its first leaf and starts another through its last; the
    // oldest still completes at the end.
    let target = format!("b{}TAIL", "a".repeat(10));
    let mut secrets = vec![("G5D_SECRET_TARGET", target)];
    let letters = ["c", "d", "e", "f", "g", "h", "i", "j", "k", "l"];
    let names = [
        "G5D_SECRET_C",
        "G5D_SECRET_D",
        "G5D_SECRET_E",
        "G5D_SECRET_F",
        "G5D_SECRET_G",
        "G5D_SECRET_H",
        "G5D_SECRET_I",
        "G5D_SECRET_J",
        "G5D_SECRET_K",
        "G5D_SECRET_L",
    ];
    let mut frames = vec![json!({"payload":"b","gap_ms":60})];
    for (index, letter) in letters.iter().enumerate() {
        secrets.push((
            names[index],
            format!("{letter}{}TAIL", "a".repeat(10 - index)),
        ));
        frames.push(json!({"payload":["a", letter]}));
    }
    frames.push(json!({"payload":"TAIL"}));
    let deltas = scripted_deltas(secrets, Value::Array(frames)).await;
    assert_eq!(deltas.len(), 12, "{deltas:?}");
    assert_eq!(deltas[0], json!("b"));
    for (index, letter) in letters.iter().enumerate() {
        assert_eq!(
            deltas[index + 1],
            json!(["[REDACTED]", letter]),
            "{deltas:?}"
        );
    }
    assert_eq!(deltas[11], json!("[REDACTED]"), "{deltas:?}");

    // A secret containing the literal mask text: the module's own
    // "[REDACTED]" is ordinary output, so the prefix is carried (and masked,
    // being longer than one byte) and the continuation masked — whether the
    // held frame leaves on expiry or is displaced by a frame without text.
    for gap in [Some(60), None] {
        let first = match gap {
            Some(gap) => json!({"payload":"key[REDACTED]","gap_ms":gap}),
            None => json!({"payload":"key[REDACTED]"}),
        };
        let deltas = scripted_deltas(
            vec![("G5D_SECRET_LITERAL", "key[REDACTED]tail".into())],
            json!([first, {"payload":{"tokens":1}}, {"payload":"tail"}]),
        )
        .await;
        assert_eq!(
            deltas,
            [
                json!("[REDACTED]"),
                json!({"tokens":1}),
                json!("[REDACTED]")
            ],
            "gap {gap:?}"
        );
    }

    // A pair that completes one secret and at the same time extends a longer
    // one: the completion masks both sides, the extension is still carried.
    let deltas = scripted_deltas(
        vec![
            ("G5D_SECRET_A", "k9".into()),
            ("G5D_SECRET_B", "k9TAIL".into()),
        ],
        json!([{"payload":"k"}, {"payload":"9"}, {"payload":"TAIL"}]),
    )
    .await;
    assert_eq!(
        deltas,
        [
            json!("[REDACTED]"),
            json!("[REDACTED]"),
            json!("[REDACTED]")
        ]
    );

    // Two secrets spanning the same pair with different heads: the union of
    // their spans is masked.
    let deltas = scripted_deltas(
        vec![
            ("G5D_SECRET_A", "abc".into()),
            ("G5D_SECRET_B", "bcTAIL".into()),
        ],
        json!([{"payload":"ab"}, {"payload":"cTAIL"}]),
    )
    .await;
    assert_eq!(deltas, [json!("[REDACTED]"), json!("[REDACTED]")]);

    // Every suffix that is a prefix of a secret is carried, not only the
    // longest: `ab` breaks on `TAIL`, `b` completes `bTAIL`.
    let deltas = scripted_deltas(
        vec![
            ("G5D_SECRET_A", "abX".into()),
            ("G5D_SECRET_B", "bTAIL".into()),
        ],
        json!([{"payload":"ab","gap_ms":60}, {"payload":"TAIL"}]),
    )
    .await;
    assert_eq!(deltas, [json!("[REDACTED]"), json!("[REDACTED]")]);

    // A leaf that completes one secret can begin another: the chain is
    // derived from its original text, whether the next frame follows at once
    // or after the hold.
    for gap in [Some(60), None] {
        let middle = match gap {
            Some(gap) => json!({"payload":"c","gap_ms":gap}),
            None => json!({"payload":"c"}),
        };
        let deltas = scripted_deltas(
            vec![
                ("G5D_SECRET_A", "abc".into()),
                ("G5D_SECRET_B", "cTAIL".into()),
            ],
            json!([{"payload":"ab","gap_ms":60}, middle, {"payload":"TAIL"}]),
        )
        .await;
        assert_eq!(
            deltas,
            [
                json!("[REDACTED]"),
                json!("[REDACTED]"),
                json!("[REDACTED]")
            ],
            "gap {gap:?}"
        );
    }

    // A boundary mask never hides a complete secret from the whole-value
    // scan: `a` + `bcTAIL` completes `abc` across the pair while `bcTAIL`
    // is itself a secret — the second leaf is masked whole, as a single
    // leaf, as the first of two leaves, and as the outcome.
    let overlapping = || {
        vec![
            ("G5D_SECRET_A", "abc".to_string()),
            ("G5D_SECRET_B", "bcTAIL".to_string()),
        ]
    };
    let deltas = scripted_deltas(
        overlapping(),
        json!([{"payload":"a"}, {"payload":"bcTAIL"}]),
    )
    .await;
    assert_eq!(deltas, [json!("[REDACTED]"), json!("[REDACTED]")]);
    let deltas = scripted_deltas(
        overlapping(),
        json!([{"payload":"a"}, {"payload":["bcTAIL", "x"]}]),
    )
    .await;
    assert_eq!(deltas, [json!("[REDACTED]"), json!(["[REDACTED]", "x"])]);
    let (deltas, outcome) = scripted(
        overlapping(),
        json!([{"payload":"a"}]),
        Some(json!("bcTAIL")),
    )
    .await;
    assert_eq!(deltas, [json!("[REDACTED]")]);
    assert_eq!(outcome, json!("[REDACTED]"));

    // Empty strings are preserved everywhere a leaf is kept.
    let (deltas, outcome) = scripted(
        overlapping(),
        json!([{"payload":""}, {"payload":{"a":"","b":""}}, {"payload":["", "x", ""]}]),
        Some(json!({"note":""})),
    )
    .await;
    assert_eq!(
        deltas,
        [json!(""), json!({"a":"","b":""}), json!(["", "x", ""])]
    );
    assert_eq!(outcome, json!({"note":""}));

    // Event kinds and object keys are module-supplied text too.
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            env: vec![("G5D_SECRET_NAME".into(), "hunter2hunter2".into())],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    let exec = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &exec,
                json!({"behavior":"scripted","frames":[
                    {"kind":"hunter2hunter2","payload":"named"},
                    {"payload":{"hunter2hunter2":1,"plain":"x","[REDACTED]":2}}
                ]}),
            ),
        )
        .await,
    );
    assert_eq!(rpc.wait_outcome(&exec).await["status"], "succeeded");
    let replay = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
            .await,
    );
    assert!(!replay.to_string().contains("hunter2hunter2"), "{replay}");
    let events = replay["events"].as_array().unwrap();
    assert_eq!(events[1]["kind"], "module.[REDACTED]", "{replay}");
    assert_eq!(events[1]["payload"], json!("named"));
    // Keys are visited in map order (sorted): the literal `[REDACTED]` key
    // keeps its name and the redacted one is suffixed.
    assert_eq!(
        events[2]["payload"],
        json!({"[REDACTED]":2,"[REDACTED]#2":1,"plain":"x"}),
        "{replay}"
    );
}

/// The outcome is redacted exactly once: the live terminal event, the
/// replayed event, `modules.status`, a retried invoke and its digest all
/// carry the same value — also for a secret that is a substring of the mask,
/// for which redaction is not idempotent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settlement_is_redacted_once_and_agrees_across_surfaces() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            env: vec![("G5D_SECRET_X".into(), "REDACTED".into())],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    let exec = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &exec,
                json!({"behavior":"scripted","frames":[],"outcome":{"note":"REDACTED"}}),
            ),
        )
        .await,
    );
    let live = rpc.wait_outcome(&exec).await;
    assert_eq!(live["status"], "succeeded", "{live}");
    assert!(
        live["result"]["note"]
            .as_str()
            .unwrap()
            .contains("[REDACTED]"),
        "{live}"
    );
    let status = result(
        rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
            .await,
    );
    assert_eq!(status["outcome"], live, "{status}");
    let replay = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
            .await,
    );
    let terminal = replay["events"].as_array().unwrap().last().unwrap();
    assert_eq!(terminal["terminal"], true);
    assert_eq!(terminal["payload"], live, "{replay}");
    let again = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &exec, json!({"behavior":"echo"})),
        )
        .await,
    );
    assert_eq!(again["disposition"], "already_finished");
    assert_eq!(again["outcome"], live, "{again}");
    assert_eq!(again["outcome_digest"], response_digest(&live), "{again}");
    // And after a restart the retained settlement is served as recorded.
    let mut d = d;
    d.restart().await;
    let mut rpc = d.connect().await;
    let status = result(
        rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
            .await,
    );
    assert_eq!(status["outcome"], live, "{status}");
    let again = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &exec, json!({"behavior":"echo"})),
        )
        .await,
    );
    assert_eq!(again["outcome"], live, "{again}");
    assert_eq!(again["outcome_digest"], response_digest(&live), "{again}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_slots_operation_identities_and_inspection_scope() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            bounded_execution_slots: Some(2),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    let a = d.task_exec_id(Uuid::new_v4(), 1);
    let b = d.operation_exec_id(Uuid::new_v4());
    let c = d.task_exec_id(Uuid::new_v4(), 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &a, json!({"behavior":"sleep"}))
            )
            .await
        )["disposition"],
        "started"
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &b, json!({"behavior":"sleep"}))
            )
            .await
        )["disposition"],
        "started"
    );
    let busy = error(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &c, json!({"behavior":"sleep"})),
        )
        .await,
        "EXECUTION_SLOTS_EXHAUSTED",
    );
    assert_eq!(busy["data"]["state"], "busy");
    let summary = result(
        rpc.call("runtime.inspect.summary", d.params(false, json!({})))
            .await,
    );
    assert_eq!(summary["modules"]["bounded_slots_in_use"], 2);
    assert_eq!(summary["modules"]["running"], 2);
    // Distinct concurrent identities keep distinct journals and dispositions.
    assert_eq!(
        result(
            rpc.call("modules.cancel", d.params(false, json!({"exec_id":a})))
                .await
        )["disposition"],
        "signalling"
    );
    assert_eq!(rpc.wait_outcome(&a).await["status"], "cancelled");
    assert_eq!(
        result(
            rpc.call("modules.status", d.params(false, json!({"exec_id":b})))
                .await
        )["status"],
        "running"
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &c, json!({"behavior":"echo"}))
            )
            .await
        )["disposition"],
        "started"
    );
    assert_eq!(rpc.wait_outcome(&c).await["status"], "succeeded");
    assert_eq!(
        result(
            rpc.call("modules.cancel", d.params(false, json!({"exec_id":b})))
                .await
        )["disposition"],
        "signalling"
    );
    assert_eq!(rpc.wait_outcome(&b).await["status"], "cancelled");
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &b, json!({"behavior":"echo"}))
            )
            .await
        )["disposition"],
        "cancelled"
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &c, json!({"behavior":"echo"}))
            )
            .await
        )["disposition"],
        "already_finished"
    );
    assert_eq!(
        result(
            rpc.call("runtime.inspect.summary", d.params(false, json!({})))
                .await
        )["modules"]["bounded_slots_in_use"],
        0
    );
    // Operation identities carry exactly one attempt.
    error(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &format!("operation.{}.2.{}", Uuid::new_v4(), d.vm),
                json!({}),
            ),
        )
        .await,
        "INVALID_EXEC_ID",
    );
    let history = result(
        rpc.call("modules.history", d.params(false, json!({})))
            .await,
    );
    let rows = history["executions"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .all(|row| row["workspace_id"] == d.workspace.to_string()
            && row["vm_id"] == d.vm.to_string()
            && row["grant_id"].is_string()
            && row["actor_user_id"] == d.owner.to_string()));

    // A support grant reads bounded diagnostics but cannot invoke, cancel,
    // respond or touch files.
    let mut inspection = json!({"exec_id":c});
    inspection["context"] = d.inspection_context();
    assert_eq!(
        result(rpc.call("modules.history", inspection.clone()).await)["executions"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(result(rpc.call("modules.logs", inspection.clone()).await)["cursor"].is_number());
    assert_eq!(
        result(rpc.call("modules.events", inspection.clone()).await)["events"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["terminal"],
        true
    );
    let mut probe = json!({"name":"runtime-health"});
    probe["context"] = d.inspection_context();
    assert_eq!(
        result(rpc.call("diagnostics.probe", probe).await)["ok"],
        true
    );
    let mut invoke = fixture_invoke(
        &d,
        &d.task_exec_id(Uuid::new_v4(), 1),
        json!({"behavior":"echo"}),
    );
    invoke["context"] = d.inspection_context();
    error(rpc.call("modules.invoke", invoke).await, "OWNER_REQUIRED");
    error(
        rpc.call("modules.cancel", inspection.clone()).await,
        "CANCEL_AUTHORITY_REQUIRED",
    );
    error(
        rpc.call("events.ack", inspection.clone()).await,
        "LIFECYCLE_AUTHORITY_REQUIRED",
    );
    let mut list = json!({});
    list["context"] = d.inspection_context();
    error(rpc.call("modules.list", list).await, "OWNER_REQUIRED");
    assert_eq!(
        result(
            rpc.call("modules.history", d.params(false, json!({})))
                .await
        )["executions"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_within_the_incarnation_settles_interrupted_runs_and_keeps_watermarks() {
    // This scenario needs the executions to remain live until the explicit
    // dispatcher restart below. The default fixture wall clock is only three
    // seconds, which lets a heavily parallel CI run settle the oldest sleeper
    // as `timed_out` before the restart and stops exercising recovery.
    let mut d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            fixture_wall_clock_ms: Some(60_000),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let running = d.task_exec_id(Uuid::new_v4(), 1);
    let finished = d.task_exec_id(Uuid::new_v4(), 1);
    let pruned = d.task_exec_id(Uuid::new_v4(), 1);
    let stopped = d.task_exec_id(Uuid::new_v4(), 1);
    {
        let mut rpc = d.connect().await;
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &running, json!({"behavior":"sleep","ms":20000})),
            )
            .await,
        );
        rpc.module_event(&running, "invocation.started").await;
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &finished, json!({"behavior":"echo"})),
            )
            .await,
        );
        let outcome = rpc.wait_outcome(&finished).await;
        assert_eq!(outcome["status"], "succeeded");
        result(
            rpc.call(
                "events.ack",
                d.params(true, json!({"exec_id":finished,"up_to_seq":99})),
            )
            .await,
        );
        // A running execution whose every event so far was acknowledged
        // away: the journal file is gone, the watermark must survive.
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &pruned, json!({"behavior":"sleep","ms":20000})),
            )
            .await,
        );
        rpc.module_event(&pruned, "module.progress").await;
        let acked = result(
            rpc.call(
                "events.ack",
                d.params(true, json!({"exec_id":pruned,"up_to_seq":99})),
            )
            .await,
        );
        assert_eq!(acked["acked_through"], 2);
        assert!(!d.state_dir().join("journal").join(&pruned).exists());
        // A cancelled run that settled and was fully acknowledged: the
        // decision stays `cancelled`, the exact outcome is still retained.
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &stopped, json!({"behavior":"sleep","ms":20000})),
            )
            .await,
        );
        rpc.module_event(&stopped, "invocation.started").await;
        // A reason carrying a configured secret — here one that crosses the
        // 256-character display bound — is redacted whole before it is
        // bounded, retained and echoed, on this reply and on every later
        // surface.
        let filler = "owner stop ".to_owned() + &"x".repeat(239);
        let stop = result(
            rpc.call(
                "modules.cancel",
                d.params(
                    false,
                    json!({"exec_id":stopped,"reason":format!("{filler}{}", d.workspace_token)}),
                ),
            )
            .await,
        );
        assert_eq!(stop["disposition"], "signalling");
        let echoed = stop["reason"].as_str().unwrap();
        assert_eq!(echoed.chars().count(), 256, "{echoed}");
        assert!(echoed.starts_with(&filler), "{echoed}");
        assert_eq!(&echoed[filler.len()..], "[REDAC", "{echoed}");
        let outcome = rpc.wait_outcome(&stopped).await;
        assert_eq!(outcome["status"], "cancelled");
        result(
            rpc.call(
                "events.ack",
                d.params(true, json!({"exec_id":stopped,"up_to_seq":99})),
            )
            .await,
        );
    }
    d.restart().await;
    let mut rpc = d.connect().await;
    assert_eq!(rpc.ready["pending_outcomes"], 2);
    for exec in [&running, &pruned] {
        let status = result(
            rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
                .await,
        );
        assert_eq!(status["status"], "finished", "{status}");
        assert_eq!(status["outcome"]["status"], "interrupted");
        let again = result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, exec, json!({"behavior":"echo"})),
            )
            .await,
        );
        assert_eq!(again["disposition"], "already_finished");
        assert_eq!(again["outcome"]["status"], "interrupted");
    }
    let replay = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":running})))
            .await,
    );
    let kinds: Vec<&str> = replay["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "invocation.started",
            "module.progress",
            "invocation.outcome"
        ]
    );
    assert_eq!(replay["events"][2]["terminal"], true);
    // The pruned execution's interrupted outcome continues the sequence
    // rather than reusing an acknowledged number.
    let replay = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":pruned})))
            .await,
    );
    assert_eq!(replay["events"].as_array().unwrap().len(), 1);
    assert_eq!(replay["events"][0]["seq"], 3);
    assert_eq!(replay["events"][0]["kind"], "invocation.outcome");
    // The fully acknowledged finished identity is still settled by its disposition.
    let again = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &finished, json!({"behavior":"echo"})),
        )
        .await,
    );
    assert_eq!(again["disposition"], "already_finished");
    assert_eq!(again["outcome"]["status"], "succeeded");
    let status = result(
        rpc.call(
            "modules.status",
            d.params(false, json!({"exec_id":stopped})),
        )
        .await,
    );
    assert_eq!(status["status"], "finished", "{status}");
    assert_eq!(status["outcome"]["status"], "cancelled");
    assert_eq!(
        status["cancelled"].as_str().unwrap().chars().count(),
        256,
        "{status}"
    );
    assert!(
        status["cancelled"].as_str().unwrap().ends_with("[REDAC"),
        "{status}"
    );
    assert!(
        !status.to_string().contains(&d.workspace_token[..8]),
        "{status}"
    );
    let again = result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &stopped, json!({"behavior":"echo"})),
        )
        .await,
    );
    assert_eq!(again["disposition"], "cancelled");
    assert_eq!(again["outcome"]["status"], "cancelled");

    // And new work runs normally after the restart.
    let fresh = d.task_exec_id(Uuid::new_v4(), 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &fresh, json!({"behavior":"echo"}))
            )
            .await
        )["disposition"],
        "started"
    );
    assert_eq!(rpc.wait_outcome(&fresh).await["status"], "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn journal_persistence_failures_unwind_admission_and_are_reported() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            fixture_module: true,
            fixture_wall_clock_ms: Some(40_000),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = d.connect().await;
    let journal_dir = d.state_dir().join("journal");
    let ack_dir = journal_dir.join(".ack");

    // A live native request whose reply will be delivered while the
    // journal is unavailable.
    let reply_task = Uuid::new_v4();
    let awaiting = d.task_exec_id(reply_task, 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &awaiting,
                json!({"behavior":"await_reply","request_id":"q-1"}),
            ),
        )
        .await,
    );
    rpc.module_event(&awaiting, "module.input.request").await;

    // A second live request remains blocked on its reply while the journal is
    // unavailable, so the journal can recover before delivery and settlement.
    let linger_task = Uuid::new_v4();
    let lingering = d.task_exec_id(linger_task, 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(
                &d,
                &lingering,
                json!({"behavior":"await_reply","request_id":"q-2","linger_ms":6000}),
            ),
        )
        .await,
    );
    rpc.module_event(&lingering, "module.input.request").await;

    // A run that will finish while the journal is unavailable.
    let doomed = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &doomed, json!({"behavior":"sleep","ms":800})),
        )
        .await,
    );
    rpc.module_event(&doomed, "invocation.started").await;

    // Break the journal: its directory becomes a plain file. Three modules
    // are live and may append while the directory is being emptied, which
    // makes `remove_dir_all` lose a race on the final rmdir; it is retried
    // (directories are only created at startup, so once it succeeds no
    // append can bring the directory back).
    let mut attempts = 0;
    loop {
        match std::fs::remove_dir_all(&journal_dir) {
            Ok(()) => break,
            Err(error) if attempts < 40 => {
                attempts += 1;
                let _ = error;
                sleep(Duration::from_millis(25)).await;
            }
            Err(error) => panic!("removing the journal directory: {error}"),
        }
    }
    std::fs::write(&journal_dir, b"not a directory").unwrap();

    // A new invoke cannot persist its start: nothing is launched, nothing
    // stays reserved, and the identity is retryable.
    let blocked = d.task_exec_id(Uuid::new_v4(), 1);
    error(
        rpc.call(
            "modules.invoke",
            fixture_invoke(&d, &blocked, json!({"behavior":"echo"})),
        )
        .await,
        "JOURNAL_ERROR",
    );
    assert_eq!(
        result(
            rpc.call(
                "modules.status",
                d.params(false, json!({"exec_id":blocked}))
            )
            .await
        )["status"],
        "unknown"
    );
    let summary = result(
        rpc.call("runtime.inspect.summary", d.params(false, json!({})))
            .await,
    );
    // The two reply-blocked runs are still live. The short doomed run may
    // already have settled while this test was replacing the journal; either
    // way, the refused invocation must not consume a slot or become live.
    // These counters are separate observations, so the doomed run can settle
    // between them. The exact remaining-slot check below also detects a leak.
    let bounded_slots = summary["modules"]["bounded_slots_in_use"].as_u64().unwrap();
    let running = summary["modules"]["running"].as_u64().unwrap();
    assert!((2..=3).contains(&bounded_slots), "{summary}");
    assert!((2..=3).contains(&running), "{summary}");

    // The reply reaches the module, but without journal evidence the RPC is
    // not acknowledged; retries persist the evidence without redelivering.
    let answer = json!({"decision":"approve"});
    error(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, reply_task, 1, "q-1", &answer),
        )
        .await,
        "JOURNAL_ERROR",
    );
    error(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, reply_task, 1, "q-1", &answer),
        )
        .await,
        "JOURNAL_ERROR",
    );

    // The doomed run settles in memory only. Depending on whether its held
    // progress event flushed before the journal was replaced, the runner can
    // retain either the module's success or a journal-append failure; the
    // authoritative invariant is that the terminal outcome is not durable.
    let status = timeout(Duration::from_secs(10), async {
        loop {
            let status = result(
                rpc.call("modules.status", d.params(false, json!({"exec_id":doomed})))
                    .await,
            );
            if status["status"] == "finished" {
                return status;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("doomed execution must still settle");
    assert_eq!(status["persistence"], "failed");
    let pending = result(
        rpc.call("outcomes.pending", d.params(true, json!({})))
            .await,
    );
    let entry = pending["pending"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["exec_id"] == doomed)
        .expect("unpersisted outcome is still pending");
    assert_eq!(entry["source"], "memory");
    // Delivery is not settlement: the answered run can still be finishing
    // after the doomed run settles, and slot release follows the terminal
    // state transition. Wait for both before asserting only the lingering
    // request retains a slot; do not impose a scheduler ordering on the runs.
    let summary = timeout(Duration::from_secs(10), async {
        loop {
            let answered = result(
                rpc.call(
                    "modules.status",
                    d.params(false, json!({"exec_id":awaiting})),
                )
                .await,
            );
            let summary = result(
                rpc.call("runtime.inspect.summary", d.params(false, json!({})))
                    .await,
            );
            if answered["status"] == "finished" && summary["modules"]["bounded_slots_in_use"] == 1 {
                return summary;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("answered and doomed executions must release their slots");
    assert!(
        summary["modules"]["unpersisted_outcomes"].as_u64().unwrap() >= 1,
        "{summary}"
    );
    // The lingering run still holds its slot; both completed runs released theirs.
    assert_eq!(summary["modules"]["bounded_slots_in_use"], 1);
    let history = result(
        rpc.call("modules.history", d.params(false, json!({})))
            .await,
    );
    let row = history["executions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["exec_id"] == doomed)
        .unwrap();
    assert_eq!(row["persistence"], "failed");

    // With the journal back, the refused identity launches normally.
    std::fs::remove_file(&journal_dir).unwrap();
    std::fs::create_dir_all(&ack_dir).unwrap();
    // Deliver the second reply only after recovery. Its evidence and terminal
    // outcome are both durable, while the first run remains available as an
    // unpersisted in-memory outcome.
    assert_eq!(
        result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, linger_task, 1, "q-2", &answer),
            )
            .await,
        )["disposition"],
        "delivered"
    );
    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&lingering))
        .await
        .expect("lingering module settles");
    assert_eq!(outcome["status"], "succeeded", "{outcome}");
    let status = result(
        rpc.call(
            "modules.status",
            d.params(false, json!({"exec_id":lingering})),
        )
        .await,
    );
    assert_eq!(status["persistence"], "journaled", "{status}");
    let kinds: Vec<String> = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":lingering})),
        )
        .await,
    )["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_owned())
        .collect();
    let respond_at = kinds
        .iter()
        .position(|k| k == "input.respond")
        .expect("delivery evidence journaled at settlement");
    assert_eq!(kinds.last().map(String::as_str), Some("invocation.outcome"));
    assert!(respond_at < kinds.len() - 1);
    assert_eq!(
        result(
            rpc.call(
                "modules.input.respond",
                respond_params(&d, linger_task, 1, "q-2", &answer)
            )
            .await
        )["disposition"],
        "already_delivered"
    );

    // The delivery evidence is persisted on retry — exactly once, without a
    // second delivery.
    let retry = result(
        rpc.call(
            "modules.input.respond",
            respond_params(&d, reply_task, 1, "q-1", &answer),
        )
        .await,
    );
    assert_eq!(retry["disposition"], "already_delivered", "{retry}");
    let evidence = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":awaiting})))
            .await,
    );
    let dispositions: Vec<&str> = evidence["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "input.respond")
        .map(|e| e["payload"]["disposition"].as_str().unwrap())
        .collect();
    assert_eq!(dispositions, ["delivered"]);
    let status = result(
        rpc.call(
            "modules.status",
            d.params(false, json!({"exec_id":awaiting})),
        )
        .await,
    );
    assert_eq!(status["status"], "finished", "{status}");
    assert_eq!(status["persistence"], "failed", "{status}");
    assert_eq!(
        result(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &blocked, json!({"behavior":"echo"}))
            )
            .await
        )["disposition"],
        "started"
    );
    assert_eq!(rpc.wait_outcome(&blocked).await["status"], "succeeded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_changes_without_parent_can_get_and_push_but_refuses_update() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            repos: vec![RepoSpec {
                upstream_branch: None,
                ..RepoSpec::default()
            }],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let origin = d.origin.as_ref().unwrap();
    stage_origin(&root, &d.repos[0].1, &origin.bare(d.repo));
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("{}/git/{}", origin.url, d.repo),
        ],
    );
    let mut rpc = d.connect().await;
    for operation in ["get", "update"] {
        let exec = d.operation_exec_id(Uuid::new_v4());
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &d,
                    &exec,
                    "workspace.changes",
                    operation,
                    json!({"repo_id":d.repo}),
                ),
            )
            .await,
        );
        let outcome = rpc.wait_outcome(&exec).await;
        let member = &outcome["result"]["members"][0];
        if operation == "get" {
            assert_eq!(outcome["status"], "succeeded", "{outcome}");
            assert!(member["upstream_branch"].is_null());
            assert_eq!(
                member["against_upstream"],
                json!({"ahead":null,"behind":null})
            );
            assert_eq!(member["errors"], json!([]));
        } else {
            assert_eq!(outcome["status"], "failed", "{outcome}");
            assert_eq!(member["errors"], json!(["PARENT_UNAVAILABLE"]));
        }
    }
    std::fs::write(root.join("owned.txt"), "new owner edit\n").unwrap();
    let exec = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &exec,
                "workspace.changes",
                "push",
                json!({"repo_id":d.repo}),
            ),
        )
        .await,
    );
    let pushed = rpc.wait_outcome(&exec).await;
    assert_eq!(pushed["status"], "succeeded", "{pushed}");
    assert_eq!(pushed["result"]["members"][0]["pushed"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_commit_uses_indexed_attributes_and_rejects_changed_raw_bytes() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    git(&root, &["config", "core.autocrlf", "false"]);
    std::fs::write(root.join(".gitattributes"), "selected.txt text eol=lf\n").unwrap();
    std::fs::write(root.join("selected.txt"), "before\n").unwrap();
    std::fs::write(root.join("unrelated.txt"), "unrelated baseline\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "filtered baseline"]);
    let before = git(&root, &["rev-parse", "HEAD"]);

    // `git add` falls back to indexed attributes when this worktree file is
    // absent. `hash-object --path` does not, even with GIT_INDEX_FILE set.
    std::fs::remove_file(root.join(".gitattributes")).unwrap();
    std::fs::write(root.join("selected.txt"), b"saved\r\n").unwrap();
    std::fs::write(root.join("unrelated.txt"), "unrelated staged\n").unwrap();
    git(&root, &["add", "unrelated.txt"]);
    let expected = git(
        &root,
        &["hash-object", "--no-filters", "--", "selected.txt"],
    );
    let input = json!({
        "repo_id": d.repo,
        "paths": ["selected.txt"],
        "expected_blobs": {"selected.txt": expected},
    });
    let mut rpc = d.connect().await;
    let exec = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &exec, "workspace.changes", "commit", input.clone()),
        )
        .await,
    );
    let committed = rpc.wait_outcome(&exec).await;
    assert_eq!(
        committed["status"],
        "succeeded",
        "{committed}\n{}",
        d.stderr()
    );
    assert_eq!(
        committed["result"]["members"][0]["committed"], true,
        "{committed}"
    );
    let head = git(&root, &["rev-parse", "HEAD"]);
    assert_ne!(head, before);
    assert_eq!(
        git(&root, &["diff", "--name-only", &before, &head]),
        "selected.txt"
    );
    let saved = std::process::Command::new("git")
        .args(["show", "HEAD:selected.txt"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(saved.status.success());
    assert_eq!(saved.stdout, b"saved\n");
    assert_eq!(
        std::fs::read(root.join("selected.txt")).unwrap(),
        b"saved\r\n"
    );
    assert!(!root.join(".gitattributes").exists());
    assert_eq!(git(&root, &["show", ":unrelated.txt"]), "unrelated staged");
    assert_eq!(
        git(&root, &["show", "HEAD:unrelated.txt"]),
        "unrelated baseline"
    );

    // Removing the extra filtered-hash comparison must retain the actual
    // Save version guard and preserve HEAD/index on rejection.
    std::fs::write(root.join("selected.txt"), b"changed since Save\r\n").unwrap();
    let retry = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &retry, "workspace.changes", "commit", input),
        )
        .await,
    );
    let rejected = rpc.wait_outcome(&retry).await;
    assert_eq!(
        rejected["result"]["members"][0]["errors"][0], "FILE_CHANGED_DURING_SAVE",
        "{rejected}"
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&root, &["show", ":unrelated.txt"]), "unrelated staged");
}

/// `g5.architecture.workspace_rebase`: the source rebase runs in the owner
/// checkout on exact captured inputs and publishes with the captured G5 head
/// as its only lease. Refusals leave the checkout and G5 unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_changes_rebase_replays_in_the_checkout_under_the_captured_lease() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let origin = d.origin.as_ref().unwrap();
    let bare = origin.bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("{}/git/{}", origin.url, d.repo),
        ],
    );
    let mut rpc = d.connect().await;
    let g5 = |bare: &Path| git(bare, &["rev-parse", "refs/heads/shared"]);
    let target = |bare: &Path| git(bare, &["rev-parse", "refs/heads/main"]);
    async fn rebase(
        d: &TestDispatcher,
        rpc: &mut g5d::dispatch_test_support::Rpc,
        expected: &str,
        onto: &str,
    ) -> Value {
        let exec = d.operation_exec_id(Uuid::new_v4());
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    d,
                    &exec,
                    "workspace.changes",
                    "rebase",
                    json!({"repo_id":d.repo,"expected_head":expected,"onto_branch":"main","onto_head":onto}),
                ),
            )
            .await,
        );
        rpc.wait_outcome(&exec).await
    }

    // Inputs are validated before admission.
    let bad = d.operation_exec_id(Uuid::new_v4());
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &bad,
                "workspace.changes",
                "rebase",
                json!({"repo_id":d.repo}),
            ),
        )
        .await,
        "INVALID_INPUT",
    );

    // A published commit, then the target advances: clean replay.
    std::fs::write(root.join("feature.txt"), "feature\n").unwrap();
    git(&root, &["add", "feature.txt"]);
    git(&root, &["commit", "-q", "-m", "feature"]);
    let feature = git(&root, &["rev-parse", "HEAD"]);
    git(
        &root,
        &[
            "push",
            "-q",
            bare.to_str().unwrap(),
            "HEAD:refs/heads/shared",
        ],
    );
    advance_base(&bare, "main", "base.txt");
    let onto = target(&bare);
    let replayed = rebase(&d, &mut rpc, &feature, &onto).await;
    assert_eq!(
        replayed["status"],
        "succeeded",
        "{replayed}\n{}",
        d.stderr()
    );
    let member = &replayed["result"]["members"][0];
    let head = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(member["head"], head);
    assert_eq!(member["published"], true);
    assert_eq!(member["previous_head"], feature);
    assert_eq!(g5(&bare), head);
    assert_eq!(git(&root, &["rev-parse", "HEAD^"]), onto);
    assert_eq!(
        git(&root, &["rev-parse", "refs/remotes/origin/shared"]),
        head
    );
    assert_eq!(git(&root, &["show", "HEAD:feature.txt"]), "feature");

    // Already on the target: a validated no-op publishes nothing.
    let noop = rebase(&d, &mut rpc, &head, &onto).await;
    assert_eq!(noop["result"]["members"][0]["published"], false, "{noop}");
    assert_eq!(g5(&bare), head);

    // Stale captured inputs are refused, never adopted.
    let stale = rebase(&d, &mut rpc, &feature, &onto).await;
    assert_eq!(
        stale["result"]["members"][0]["errors"],
        json!(["INTERNAL_BRANCH_MOVED"]),
        "{stale}"
    );
    advance_base(&bare, "main", "base2.txt");
    let moved = rebase(&d, &mut rpc, &head, &onto).await;
    assert_eq!(
        moved["result"]["members"][0]["errors"],
        json!(["TARGET_MOVED"]),
        "{moved}"
    );
    assert_eq!(g5(&bare), head);
    let onto = target(&bare);

    // Dirty files are never reset, stashed or replayed.
    std::fs::write(root.join("scratch.txt"), "uncommitted\n").unwrap();
    let dirty = rebase(&d, &mut rpc, &head, &onto).await;
    assert_eq!(
        dirty["result"]["members"][0]["errors"],
        json!(["DIRTY_WORKING_TREE"]),
        "{dirty}"
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read_to_string(root.join("scratch.txt")).unwrap(),
        "uncommitted\n"
    );
    std::fs::remove_file(root.join("scratch.txt")).unwrap();

    // The checkout lacks newer G5 work and holds none of its own: it takes
    // the G5 head before replaying, so nothing published is lost.
    advance_base(&bare, "shared", "from-g5.txt");
    let newer = g5(&bare);
    let caught_up = rebase(&d, &mut rpc, &newer, &onto).await;
    assert_eq!(caught_up["status"], "succeeded", "{caught_up}");
    let head = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(g5(&bare), head);
    assert_eq!(git(&root, &["show", "HEAD:from-g5.txt"]), "base moved");
    assert_eq!(
        git(&root, &["merge-base", "--is-ancestor", &onto, "HEAD"]),
        ""
    );

    // Unpublished local commits are part of the replay.
    std::fs::write(root.join("local.txt"), "local\n").unwrap();
    git(&root, &["add", "local.txt"]);
    git(&root, &["commit", "-q", "-m", "local"]);
    advance_base(&bare, "main", "base3.txt");
    let onto = target(&bare);
    let with_local = rebase(&d, &mut rpc, &head, &onto).await;
    assert_eq!(with_local["status"], "succeeded", "{with_local}");
    let head = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(g5(&bare), head);
    assert_eq!(git(&root, &["show", "HEAD:local.txt"]), "local");

    // Local and G5 histories both moved: the checkout's own commits are
    // replayed onto the G5 head, so neither side's work is lost.
    std::fs::write(root.join("mine.txt"), "mine\n").unwrap();
    git(&root, &["add", "mine.txt"]);
    git(&root, &["commit", "-q", "-m", "mine"]);
    advance_base(&bare, "shared", "theirs.txt");
    let theirs = g5(&bare);
    let reconciled = rebase(&d, &mut rpc, &theirs, &onto).await;
    assert_eq!(reconciled["status"], "succeeded", "{reconciled}");
    let head = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(g5(&bare), head);
    assert_eq!(git(&root, &["rev-parse", "HEAD^"]), theirs);
    assert_eq!(git(&root, &["show", "HEAD:mine.txt"]), "mine");

    // The split an earlier central rebase left: G5 holds rebased copies of
    // the checkout's commits and the checkout added one more. Only the new
    // commit is replayed; the copies G5 already holds are not duplicated.
    let old_local = head.clone();
    advance_base(&bare, "main", "moved-base.txt");
    let onto = target(&bare);
    let copy = tempfile::tempdir().unwrap();
    let central = copy.path().join("c");
    git(
        copy.path(),
        &[
            "clone",
            "-q",
            "-b",
            "shared",
            bare.to_str().unwrap(),
            central.to_str().unwrap(),
        ],
    );
    git(&central, &["config", "user.name", "Central"]);
    git(&central, &["config", "user.email", "central@g5.test"]);
    git(&central, &["rebase", "-q", "origin/main"]);
    git(
        &central,
        &["push", "-q", "-f", "origin", "HEAD:refs/heads/shared"],
    );
    let central_head = g5(&bare);
    assert_ne!(central_head, old_local);
    std::fs::write(root.join("after.txt"), "after\n").unwrap();
    git(&root, &["add", "after.txt"]);
    git(&root, &["commit", "-q", "-m", "after central rebase"]);
    let split = rebase(&d, &mut rpc, &central_head, &onto).await;
    assert_eq!(split["status"], "succeeded", "{split}");
    let head = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(g5(&bare), head);
    assert_eq!(git(&root, &["rev-parse", "HEAD^"]), central_head);
    assert_eq!(git(&root, &["show", "HEAD:after.txt"]), "after");

    // Conflicting local and G5 commits refuse and restore the checkout.
    std::fs::write(root.join("both.txt"), "mine\n").unwrap();
    git(&root, &["add", "both.txt"]);
    git(&root, &["commit", "-q", "-m", "mine"]);
    let mine = git(&root, &["rev-parse", "HEAD"]);
    advance_base(&bare, "shared", "both.txt");
    let theirs = g5(&bare);
    let diverged = rebase(&d, &mut rpc, &theirs, &onto).await;
    let member = &diverged["result"]["members"][0];
    assert_eq!(
        member["errors"],
        json!(["LOCAL_HISTORY_DIVERGED"]),
        "{diverged}"
    );
    assert_eq!(member["local_head"], mine);
    assert_eq!(member["conflicts"], json!(["both.txt"]));
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), mine);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    assert_eq!(g5(&bare), theirs);
    git(&root, &["reset", "-q", "--hard", &theirs]);

    // A replay conflict restores the checkout and leaves G5 unchanged.
    std::fs::write(root.join("conflict.txt"), "workspace side\n").unwrap();
    git(&root, &["add", "conflict.txt"]);
    git(&root, &["commit", "-q", "-m", "workspace side"]);
    let local = git(&root, &["rev-parse", "HEAD"]);
    git(
        &root,
        &[
            "push",
            "-q",
            "-f",
            bare.to_str().unwrap(),
            "HEAD:refs/heads/shared",
        ],
    );
    advance_base(&bare, "main", "conflict.txt");
    let onto = target(&bare);
    let conflicted = rebase(&d, &mut rpc, &local, &onto).await;
    let member = &conflicted["result"]["members"][0];
    assert_eq!(member["errors"], json!(["REBASE_CONFLICT"]), "{conflicted}");
    assert_eq!(member["conflicts"], json!(["conflict.txt"]));
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), local);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    assert!(!root.join(".git/rebase-merge").exists());
    assert_eq!(g5(&bare), local);
}

/// `g5.workspace.remote.adopt-provider-history`: adopting resets the owner
/// checkout to the reviewed provider tip, pinned as the divergence run's
/// remote input, and publishes it to G5 with the captured G5 head as its only
/// lease. Refusals leave the checkout, its local work and G5 unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_changes_adopt_resets_the_checkout_to_the_pinned_provider_tip() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let origin = d.origin.as_ref().unwrap();
    let bare = origin.bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("{}/git/{}", origin.url, d.repo),
        ],
    );
    let mut rpc = d.connect().await;
    let g5 = |bare: &Path| git(bare, &["rev-parse", "refs/heads/shared"]);
    let run = Uuid::new_v4();

    // The provider's unrelated history, pinned in G5 as the run's input.
    let provider = tempfile::tempdir().unwrap();
    let work = provider.path().join("p");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    git(&work, &["config", "user.name", "Provider"]);
    git(&work, &["config", "user.email", "provider@g5.test"]);
    std::fs::write(work.join("provider.txt"), "provider\n").unwrap();
    git(&work, &["add", "provider.txt"]);
    git(&work, &["commit", "-q", "-m", "provider history"]);
    let tip = git(&work, &["rev-parse", "HEAD"]);
    git(
        &work,
        &[
            "push",
            "-q",
            bare.to_str().unwrap(),
            &format!(
                "HEAD:{}",
                g5_contracts::workspace_git::resolution_ref(d.workspace, run, "remote")
            ),
        ],
    );
    let head = g5(&bare);
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);

    async fn adopt(
        d: &TestDispatcher,
        rpc: &mut g5d::dispatch_test_support::Rpc,
        run: Uuid,
        expected: &str,
        tip: &str,
    ) -> Value {
        let exec = d.operation_exec_id(Uuid::new_v4());
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    d,
                    &exec,
                    "workspace.changes",
                    "adopt",
                    json!({"repo_id":d.repo,"run_id":run,"expected_head":expected,"provider_tip":tip}),
                ),
            )
            .await,
        );
        rpc.wait_outcome(&exec).await
    }

    // Inputs are validated before admission.
    let bad = d.operation_exec_id(Uuid::new_v4());
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &bad,
                "workspace.changes",
                "adopt",
                json!({"repo_id":d.repo,"expected_head":head}),
            ),
        )
        .await,
        "INVALID_INPUT",
    );

    // Stale captured inputs are refused, never adopted.
    let moved = adopt(&d, &mut rpc, run, &tip, &tip).await;
    assert_eq!(
        moved["result"]["members"][0]["errors"],
        json!(["INTERNAL_BRANCH_MOVED"]),
        "{moved}\n{}",
        d.stderr()
    );
    let unpinned = adopt(&d, &mut rpc, run, &head, &head).await;
    assert_eq!(
        unpinned["result"]["members"][0]["errors"],
        json!(["PROVIDER_INPUT_MOVED"]),
        "{unpinned}"
    );

    // Uncommitted files are never discarded.
    std::fs::write(root.join("scratch.txt"), "uncommitted\n").unwrap();
    let dirty = adopt(&d, &mut rpc, run, &head, &tip).await;
    assert_eq!(
        dirty["result"]["members"][0]["errors"],
        json!(["DIRTY_WORKING_TREE"]),
        "{dirty}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("scratch.txt")).unwrap(),
        "uncommitted\n"
    );
    std::fs::remove_file(root.join("scratch.txt")).unwrap();

    // Nor are commits G5 lacks.
    std::fs::write(root.join("local.txt"), "local\n").unwrap();
    git(&root, &["add", "local.txt"]);
    git(&root, &["commit", "-q", "-m", "local"]);
    let local = git(&root, &["rev-parse", "HEAD"]);
    let unpushed = adopt(&d, &mut rpc, run, &head, &tip).await;
    let member = &unpushed["result"]["members"][0];
    assert_eq!(member["errors"], json!(["UNPUSHED_COMMITS"]), "{unpushed}");
    assert_eq!(member["local_head"], local);
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), local);
    assert_eq!(g5(&bare), head);
    git(&root, &["reset", "-q", "--hard", &head]);

    let adopted = adopt(&d, &mut rpc, run, &head, &tip).await;
    assert_eq!(adopted["status"], "succeeded", "{adopted}\n{}", d.stderr());
    let member = &adopted["result"]["members"][0];
    assert_eq!(member["head"], tip);
    assert_eq!(member["previous_head"], head);
    assert_eq!(member["published"], true);
    assert_eq!(g5(&bare), tip, "G5 holds the provider tip");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), tip);
    assert_eq!(git(&root, &["show", "HEAD:provider.txt"]), "provider");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&root, &["rev-parse", "refs/remotes/origin/shared"]),
        tip
    );

    // Already adopted: a validated no-op publishes nothing.
    let again = adopt(&d, &mut rpc, run, &tip, &tip).await;
    assert_eq!(again["status"], "succeeded", "{again}");
    assert_eq!(again["result"]["members"][0]["published"], false, "{again}");
    assert_eq!(g5(&bare), tip);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_changes_reports_association_dirty_ahead_behind_and_refuses_dirty_update() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let origin = d.origin.as_ref().unwrap();
    let bare = origin.bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    // The checkout's own `origin` is the internal HTTP URL, as the platform
    // materializes it; repository Git holds no credential for it.
    let internal_url = format!("{}/git/{}", origin.url, d.repo);
    git(&root, &["remote", "set-url", "origin", &internal_url]);
    let mut rpc = d.connect().await;
    let get = |d: &TestDispatcher| {
        builtin_invoke(
            d,
            &d.operation_exec_id(Uuid::new_v4()),
            "workspace.changes",
            "get",
            json!({}),
        )
    };

    let exec = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &exec,
                "workspace.changes",
                "get",
                json!({"repo_id":d.repo}),
            ),
        )
        .await,
    );
    let clean = rpc.wait_outcome(&exec).await;
    assert_eq!(clean["status"], "succeeded", "{clean}");
    let member = &clean["result"]["members"][0];
    assert_eq!(member["repo_id"], d.repo.to_string());
    assert_eq!(member["shared_branch"], "shared");
    assert_eq!(member["upstream_branch"], "main");
    assert_eq!(member["head"], "shared");
    assert_eq!(member["on_shared_branch"], true);
    assert_eq!(member["internal_ref_present"], true);
    assert_eq!(member["errors"], json!([]));
    assert_eq!(member["dirty"], json!([]));
    assert_eq!(member["against_internal"], json!({"ahead":0,"behind":0}));
    assert_eq!(member["against_upstream"], json!({"ahead":0,"behind":0}));
    assert_eq!(member["sequencer"], json!([]));
    let unknown = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &unknown,
                "workspace.changes",
                "get",
                json!({"repo_id":Uuid::new_v4()}),
            ),
        )
        .await,
    );
    assert_eq!(
        rpc.wait_outcome(&unknown).await["detail"]["reason"],
        "UNKNOWN_MEMBER"
    );
    // An operation the module does not offer is refused before admission.
    let squash = d.operation_exec_id(Uuid::new_v4());
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &squash, "workspace.changes", "squash", json!({})),
        )
        .await,
        "OPERATION_NOT_FOUND",
    );

    // One commit ahead plus a dirty file: the dirty set is reported and
    // `update` is refused without touching the tree.
    std::fs::write(root.join("feature.txt"), "feature\n").unwrap();
    git(&root, &["add", "feature.txt"]);
    git(&root, &["commit", "-q", "-m", "feature"]);
    std::fs::write(root.join("scratch.txt"), "uncommitted\n").unwrap();
    let head_before = git(&root, &["rev-parse", "HEAD"]);
    let exec = get(&d);
    let exec_id = exec["exec_id"].as_str().unwrap().to_owned();
    result(rpc.call("modules.invoke", exec).await);
    let dirty = rpc.wait_outcome(&exec_id).await;
    let member = &dirty["result"]["members"][0];
    assert_eq!(
        member["dirty"],
        json!([{"status":"??","path":"scratch.txt"}])
    );
    assert_eq!(member["against_internal"], json!({"ahead":1,"behind":0}));
    let update = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &update, "workspace.changes", "update", json!({})),
        )
        .await,
    );
    let refused = rpc.wait_outcome(&update).await;
    assert_eq!(refused["status"], "failed", "{refused}");
    assert_eq!(
        refused["result"]["members"][0]["errors"],
        json!(["DIRTY_WORKING_TREE"])
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head_before);
    assert!(root.join("scratch.txt").exists());

    // Repository Git itself cannot reach the internal origin: the checkout
    // holds no credential and the origin refuses it.
    let direct = std::process::Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "push",
            "-q",
            "origin",
            "shared",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        !direct.status.success(),
        "an unauthenticated repository push must be refused"
    );
    assert!(origin.unauthorized() >= 1);
    assert_eq!(origin.served(), 0);

    // Push never stages or commits: the dirty file stays dirty and only the
    // committed head reaches the existing internal branch — through the
    // dispatcher's token-isolated transport.
    let push = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &push, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let progress = rpc.module_event(&push, "module.progress").await;
    assert_eq!(progress["payload"]["step"], "push");
    let pushed = rpc.wait_outcome(&push).await;
    assert_eq!(pushed["status"], "succeeded", "{pushed}\n{}", d.stderr());
    assert_eq!(pushed["result"]["members"][0]["pushed"], true);
    assert_eq!(pushed["result"]["members"][0]["head"], head_before);
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/shared"]), head_before);
    assert_eq!(git(&root, &["status", "--porcelain"]), "?? scratch.txt");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head_before);
    assert!(origin.served() >= 2);
    assert_eq!(
        git(&root, &["rev-parse", "refs/remotes/origin/shared"]),
        head_before
    );

    // `commit` is the one operation that stages: everything in the working
    // tree, the untracked scratch file included, becomes one commit on the
    // shared branch under the module's identity, with the given message.
    // A clean tree commits nothing and reports the unchanged head. No hook
    // of the repository runs for it: this one would refuse every commit.
    let hooks = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("prepare-commit-msg"), "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(
        hooks.join("prepare-commit-msg"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let commit = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &commit,
                "workspace.changes",
                "commit",
                json!({"message": "Review of demo: round 1"}),
            ),
        )
        .await,
    );
    let progress = rpc.module_event(&commit, "module.progress").await;
    assert_eq!(progress["payload"]["step"], "commit");
    let committed = rpc.wait_outcome(&commit).await;
    assert_eq!(
        committed["status"],
        "succeeded",
        "{committed}\n{}",
        d.stderr()
    );
    let member = &committed["result"]["members"][0];
    assert_eq!(member["committed"], true, "{committed}");
    assert_eq!(member["files"], 1);
    let head_committed = git(&root, &["rev-parse", "HEAD"]);
    assert_ne!(head_committed, head_before);
    assert_eq!(member["head"], head_committed);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&root, &["log", "-1", "--format=%s"]),
        "Review of demo: round 1"
    );
    assert!(git(&root, &["show", "--stat", "--format=", "HEAD"]).contains("scratch.txt"));
    let again = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &again, "workspace.changes", "commit", json!({})),
        )
        .await,
    );
    let unchanged = rpc.wait_outcome(&again).await;
    assert_eq!(unchanged["status"], "succeeded", "{unchanged}");
    assert_eq!(
        unchanged["result"]["members"][0],
        json!({"repo_id":d.repo,"committed":false,"head":head_committed,"files":0})
    );
    // Unmerged index entries without any sequencer marker (what a
    // conflicting `stash pop` or `apply --3way` leaves): the save refuses
    // rather than staging the conflict markers as content.
    std::fs::write(root.join("scratch.txt"), "theirs\n").unwrap();
    git(&root, &["add", "scratch.txt"]);
    let theirs = git(&root, &["write-tree"]);
    std::fs::write(root.join("scratch.txt"), "ours\n").unwrap();
    git(&root, &["add", "scratch.txt"]);
    let ours = git(&root, &["write-tree"]);
    // A three-way index merge from the "ours" index: conflicting stages,
    // no MERGE_HEAD or sequencer state.
    git(&root, &["read-tree", "-m", "HEAD", &ours, &theirs]);
    assert_eq!(git(&root, &["status", "--porcelain"]), "UU scratch.txt");
    let conflicted = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &conflicted, "workspace.changes", "commit", json!({})),
        )
        .await,
    );
    let refused = rpc.wait_outcome(&conflicted).await;
    assert_eq!(refused["status"], "failed", "{refused}\n{}", d.stderr());
    assert_eq!(
        refused["result"]["members"][0]["errors"],
        json!(["UNMERGED_PATHS"])
    );
    assert_eq!(
        refused["result"]["members"][0]["paths"],
        json!(["scratch.txt"])
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head_committed);
    assert_eq!(git(&root, &["status", "--porcelain"]), "UU scratch.txt");
    git(&root, &["reset", "-q", "--hard"]);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    // Dirty only through the index (staged, then the file restored to
    // HEAD): staging everything leaves nothing to commit, and that is a
    // clean member, not a failure.
    std::fs::write(root.join("scratch.txt"), "changed\n").unwrap();
    git(&root, &["add", "scratch.txt"]);
    git(
        &root,
        &["restore", "--worktree", "--source=HEAD", "scratch.txt"],
    );
    assert_eq!(git(&root, &["status", "--porcelain"]), "MM scratch.txt");
    let staged_only = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &staged_only, "workspace.changes", "commit", json!({})),
        )
        .await,
    );
    let nothing = rpc.wait_outcome(&staged_only).await;
    assert_eq!(nothing["status"], "succeeded", "{nothing}\n{}", d.stderr());
    assert_eq!(
        nothing["result"]["members"][0],
        json!({"repo_id":d.repo,"committed":false,"head":head_committed,"files":0})
    );
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    // The commit is what the next push publishes.
    let push = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &push, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let pushed = rpc.wait_outcome(&push).await;
    assert_eq!(pushed["status"], "succeeded", "{pushed}\n{}", d.stderr());
    assert_eq!(
        git(&bare, &["rev-parse", "refs/heads/shared"]),
        head_committed
    );

    // The parent moved: merge it without renaming the destination branch.
    advance_base(&bare, "main", "base.txt");
    let update = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &update, "workspace.changes", "update", json!({})),
        )
        .await,
    );
    let updated = rpc.wait_outcome(&update).await;
    assert_eq!(updated["status"], "succeeded", "{updated}\n{}", d.stderr());
    assert_eq!(
        updated["result"]["members"][0],
        json!({"repo_id":d.repo,"updated":true,"upstream_branch":"main"})
    );
    assert!(root.join("base.txt").exists());
    assert!(root.join("feature.txt").exists());
    assert_eq!(git(&root, &["symbolic-ref", "--short", "HEAD"]), "shared");
    let exec = get(&d);
    let exec_id = exec["exec_id"].as_str().unwrap().to_owned();
    result(rpc.call("modules.invoke", exec).await);
    let after = rpc.wait_outcome(&exec_id).await;
    // The feature, scratch commit and merge commit are ahead of the parent.
    assert_eq!(
        after["result"]["members"][0]["against_upstream"],
        json!({"ahead":3,"behind":0})
    );

    // Parent update retains the published workspace history, so publication
    // remains a fast-forward guarded by an exact destination lease.
    let merged = git(&root, &["rev-parse", "HEAD"]);
    let push = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &push, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let pushed = rpc.wait_outcome(&push).await;
    assert_eq!(pushed["status"], "succeeded", "{pushed}\n{}", d.stderr());
    assert_eq!(git(&bare, &["rev-parse", "refs/heads/shared"]), merged);

    // A stale tracking ref must not let push recreate a deleted internal
    // branch: the lease pins the destination to the last observed value.
    git(&bare, &["update-ref", "-d", "refs/heads/shared"]);
    assert!(!ref_exists(&bare, "refs/heads/shared"));
    let push = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &push, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let refused = rpc.wait_outcome(&push).await;
    assert_eq!(refused["status"], "failed", "{refused}");
    assert_eq!(refused["result"]["members"][0]["pushed"], false);
    assert_eq!(
        refused["result"]["members"][0]["errors"],
        json!(["INTERNAL_BRANCH_MOVED"])
    );
    assert!(
        !ref_exists(&bare, "refs/heads/shared"),
        "push recreated a deleted internal branch"
    );
    git(
        &bare,
        &[
            "update-ref",
            "refs/heads/shared",
            &git(&root, &["rev-parse", "refs/remotes/origin/shared"]),
        ],
    );

    // Detached and interrupted-sequencer states are exposed, not hidden;
    // switch_back restores the existing association.
    git(&root, &["checkout", "-q", "--detach"]);
    let git_dir = git(&root, &["rev-parse", "--absolute-git-dir"]);
    std::fs::write(
        format!("{git_dir}/MERGE_HEAD"),
        "0000000000000000000000000000000000000000\n",
    )
    .unwrap();
    let exec = get(&d);
    let exec_id = exec["exec_id"].as_str().unwrap().to_owned();
    result(rpc.call("modules.invoke", exec).await);
    let detached = rpc.wait_outcome(&exec_id).await;
    let member = &detached["result"]["members"][0];
    assert_eq!(member["detached_head"], true);
    assert_eq!(member["head"], Value::Null);
    assert_eq!(member["sequencer"], json!(["merge"]));
    assert_eq!(
        member["errors"],
        json!(["DETACHED_HEAD", "INTERRUPTED_SEQUENCER"])
    );
    std::fs::remove_file(format!("{git_dir}/MERGE_HEAD")).unwrap();
    let back = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &back, "workspace.changes", "switch_back", json!({})),
        )
        .await,
    );
    let switched = rpc.wait_outcome(&back).await;
    assert_eq!(switched["status"], "succeeded", "{switched}");
    assert_eq!(
        switched["result"]["members"][0],
        json!({"repo_id":d.repo,"switched":true,"materialized_local":false})
    );
    assert_eq!(git(&root, &["symbolic-ref", "--short", "HEAD"]), "shared");

    // Cancelling a built-in mid-operation is definite and leaves no open
    // sequencer behind.
    let long = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &long, "workspace.changes", "get", json!({})),
        )
        .await,
    );
    let _ = rpc
        .call("modules.cancel", d.params(false, json!({"exec_id":long})))
        .await;
    let outcome = rpc.wait_outcome(&long).await;
    assert!(
        matches!(
            outcome["status"].as_str(),
            Some("cancelled") | Some("succeeded")
        ),
        "{outcome}"
    );
    assert!(!Path::new(&format!("{git_dir}/rebase-merge")).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changes_never_reports_an_unreadable_tree_as_clean() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            catalog_edit: Some(Arc::new(|catalog: &mut Value| {
                for module in catalog["modules"].as_array_mut().unwrap() {
                    if module["module_id"] == "workspace.changes" {
                        module["limits"]["max_output_bytes"] = json!(1024);
                    }
                }
            })),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    // Enough untracked files that the porcelain status exceeds the bound.
    for index in 0..40 {
        std::fs::write(
            root.join(format!("untracked-file-with-a-long-name-{index:03}.txt")),
            "x",
        )
        .unwrap();
    }
    let mut rpc = d.connect().await;
    let get = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &get, "workspace.changes", "get", json!({})),
        )
        .await,
    );
    let report = rpc.wait_outcome(&get).await;
    assert_eq!(report["status"], "succeeded", "{report}");
    let member = &report["result"]["members"][0];
    assert_eq!(member["dirty"], Value::Null);
    assert!(
        member["errors"]
            .as_array()
            .unwrap()
            .contains(&json!("STATUS_UNAVAILABLE")),
        "{member}"
    );
    let update = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &update, "workspace.changes", "update", json!({})),
        )
        .await,
    );
    let refused = rpc.wait_outcome(&update).await;
    assert_eq!(refused["status"], "failed", "{refused}");
    assert_eq!(
        refused["result"]["members"][0]["errors"],
        json!(["STATUS_UNAVAILABLE"])
    );
    assert_eq!(git(&root, &["symbolic-ref", "--short", "HEAD"]), "shared");
}

/// Repository steps run under the host-provisioned unprivileged identity
/// while the supervisor keeps the dispatcher's: a step cannot read the
/// dispatcher's private state. Needs root on Linux to switch identities.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bootstrap_steps_run_unprivileged_and_cannot_read_private_state() {
    // SAFETY: plain libc query.
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("skipped: identity separation needs root to switch to the module uid");
        return;
    }
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: vec![
                ("G5D_MODULE_UID".into(), "65534".into()),
                ("G5D_MODULE_GID".into(), "65534".into()),
            ],
            ..DispatcherOptions::default()
        },
    )
    .await;
    use std::os::unix::fs::PermissionsExt;
    for dir in [
        d.root.path().to_path_buf(),
        d.workspace_root(),
        d.repo_root(),
    ] {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
    }
    let root = d.repo_root();
    let journal = d.state_dir().join("journal");
    std::fs::write(
        root.join("g5.yaml"),
        format!("name: fixture\nsetup:\n  bootstrap:\n    - \"id -u > uid.txt; ls {} > journal.txt 2>&1; echo $? > journal-rc.txt\"\n", journal.display()),
    )
    .unwrap();
    let mut rpc = d.connect().await;
    let exec = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &exec, "workspace.bootstrap", "run", json!({})),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}\n{}", d.stderr());
    assert_eq!(
        std::fs::read_to_string(root.join("uid.txt"))
            .unwrap()
            .trim(),
        "65534"
    );
    assert_ne!(
        std::fs::read_to_string(root.join("journal-rc.txt"))
            .unwrap()
            .trim(),
        "0",
        "a step could list the dispatcher's private journal"
    );
}

/// Changes operates on the authoritative shared branch exactly as named:
/// every name Git accepts is accepted, not only a character whitelist.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changes_accepts_every_git_valid_shared_branch_name() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            repos: vec![RepoSpec {
                root_path: "code".into(),
                role: Some("code".into()),
                branch: "feature+test".into(),
                upstream_branch: Some("main".into()),
                follow_branch: false,
            }],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let origin = d.origin.as_ref().unwrap();
    let bare = origin.bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    let internal_url = format!("{}/git/{}", origin.url, d.repo);
    git(&root, &["remote", "set-url", "origin", &internal_url]);
    let mut rpc = d.connect().await;

    let get = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &get, "workspace.changes", "get", json!({})),
        )
        .await,
    );
    let got = rpc.wait_outcome(&get).await;
    assert_eq!(got["status"], "succeeded", "{got}");
    let member = &got["result"]["members"][0];
    assert_eq!(member["shared_branch"], "feature+test");
    assert_eq!(member["head"], "feature+test");
    assert_eq!(member["on_shared_branch"], true);
    assert_eq!(member["internal_ref_present"], true);
    assert_eq!(member["errors"], json!([]), "{got}");

    std::fs::write(root.join("work.txt"), "work\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "work"]);
    let push = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &push, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let pushed = rpc.wait_outcome(&push).await;
    assert_eq!(pushed["status"], "succeeded", "{pushed}\n{}", d.stderr());
    assert_eq!(pushed["result"]["members"][0]["pushed"], true, "{pushed}");
    assert_eq!(
        git(&bare, &["rev-parse", "refs/heads/feature+test"]),
        git(&root, &["rev-parse", "HEAD"])
    );

    let update = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &update, "workspace.changes", "update", json!({})),
        )
        .await,
    );
    let updated = rpc.wait_outcome(&update).await;
    assert_eq!(updated["status"], "succeeded", "{updated}\n{}", d.stderr());
    assert_eq!(
        updated["result"]["members"][0]["updated"], true,
        "{updated}"
    );
    assert_eq!(
        git(&root, &["symbolic-ref", "--short", "HEAD"]),
        "feature+test"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_repo_changes_preserve_names_and_update_uses_each_member_base() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            repos: vec![
                RepoSpec {
                    root_path: "alpha".into(),
                    role: Some("code".into()),
                    branch: "shared".into(),
                    upstream_branch: Some("main".into()),
                    follow_branch: false,
                },
                RepoSpec {
                    root_path: "beta".into(),
                    role: Some("code".into()),
                    branch: "shared".into(),
                    upstream_branch: Some("master".into()),
                    follow_branch: false,
                },
            ],
            ..DispatcherOptions::default()
        },
    )
    .await;
    let origin = d.origin.as_ref().unwrap();
    let (alpha_id, beta_id) = (d.repos[0].0, d.repos[1].0);
    let (alpha, beta) = (d.member_root(0), d.member_root(1));
    let alpha_bare = origin.bare(alpha_id);
    let beta_bare = origin.bare(beta_id);
    stage_origin(&alpha, &d.repos[0].1, &alpha_bare);
    // Beta has an origin whose internal `shared` ref was never published, and
    // a native checkout moved it onto another local branch.
    std::fs::write(beta.join("README.md"), "beta\n").unwrap();
    git(&beta, &["add", "-A"]);
    git(&beta, &["commit", "-q", "-m", "beta seed"]);
    git(
        &beta,
        &["remote", "add", "origin", beta_bare.to_str().unwrap()],
    );
    git(&beta, &["push", "-q", "origin", "shared:refs/heads/master"]);
    git(&beta, &["fetch", "-q", "origin"]);
    git(&beta, &["checkout", "-q", "-b", "experiment"]);
    let mut rpc = d.connect().await;

    let exec = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &exec, "workspace.changes", "get", json!({})),
        )
        .await,
    );
    let report = rpc.wait_outcome(&exec).await;
    let members = report["result"]["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["repo_id"], alpha_id.to_string());
    assert_eq!(members[0]["errors"], json!([]));
    assert_eq!(members[1]["repo_id"], beta_id.to_string());
    assert_eq!(members[1]["head"], "experiment");
    assert_eq!(members[1]["foreign_head"], true);
    assert_eq!(
        members[1]["errors"],
        json!(["BRANCH_ASSOCIATION_MISMATCH", "MISSING_INTERNAL_BRANCH"])
    );

    // Push preserves alpha's destination and refuses beta instead of letting
    // `git push` create a missing internal branch; switch_back likewise.
    let push = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &push, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let pushed = rpc.wait_outcome(&push).await;
    assert_eq!(pushed["status"], "failed", "{pushed}\n{}", d.stderr());
    assert_eq!(pushed["result"]["members"][0]["pushed"], true);
    assert_eq!(pushed["result"]["members"][1]["pushed"], false);
    assert_eq!(
        pushed["result"]["members"][1]["errors"],
        json!(["BRANCH_ASSOCIATION_MISMATCH", "MISSING_INTERNAL_BRANCH"])
    );
    assert!(!ref_exists(&beta_bare, "refs/heads/shared"));
    let back = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &back, "workspace.changes", "switch_back", json!({})),
        )
        .await,
    );
    let switched = rpc.wait_outcome(&back).await;
    assert_eq!(switched["result"]["members"][0]["switched"], true);
    assert_eq!(
        switched["result"]["members"][1],
        json!({"repo_id":beta_id,"switched":false,"errors":["MISSING_INTERNAL_BRANCH"]})
    );
    assert_eq!(
        git(&beta, &["symbolic-ref", "--short", "HEAD"]),
        "experiment"
    );
    assert!(!ref_exists(&beta_bare, "refs/heads/shared"));

    // Once the platform publishes beta's internal branch, switch_back may
    // materialize the local tracking branch from that existing ref only.
    git(
        &beta,
        &["push", "-q", "origin", "experiment:refs/heads/shared"],
    );
    git(&beta, &["fetch", "-q", "origin"]);
    git(&beta, &["branch", "-q", "-D", "shared"]);
    let back = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &back,
                "workspace.changes",
                "switch_back",
                json!({"repo_id":beta_id}),
            ),
        )
        .await,
    );
    let switched = rpc.wait_outcome(&back).await;
    assert_eq!(switched["status"], "succeeded", "{switched}");
    assert_eq!(
        switched["result"]["members"][0],
        json!({"repo_id":beta_id,"switched":true,"materialized_local":true})
    );
    assert_eq!(git(&beta, &["symbolic-ref", "--short", "HEAD"]), "shared");

    // An ordinary update must retain published local commits as ancestors.
    std::fs::write(alpha.join("local.txt"), "local work\n").unwrap();
    git(&alpha, &["add", "local.txt"]);
    git(&alpha, &["commit", "-q", "-m", "local work"]);
    let local_head = git(&alpha, &["rev-parse", "HEAD"]);

    // Update merges each member with its own parent (`main` vs `master`)
    // without renaming either destination.
    advance_base(&alpha_bare, "main", "alpha-base.txt");
    advance_base(&beta_bare, "master", "beta-base.txt");
    let update = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &update, "workspace.changes", "update", json!({})),
        )
        .await,
    );
    let updated = rpc.wait_outcome(&update).await;
    assert_eq!(updated["status"], "succeeded", "{updated}\n{}", d.stderr());
    assert_eq!(updated["result"]["members"][0]["upstream_branch"], "main");
    assert_eq!(updated["result"]["members"][1]["upstream_branch"], "master");
    assert!(alpha.join("alpha-base.txt").exists() && !alpha.join("beta-base.txt").exists());
    assert!(beta.join("beta-base.txt").exists() && !beta.join("alpha-base.txt").exists());
    assert_eq!(git(&alpha, &["symbolic-ref", "--short", "HEAD"]), "shared");
    assert_eq!(git(&beta, &["symbolic-ref", "--short", "HEAD"]), "shared");
    assert_eq!(
        git(&alpha, &["merge-base", &local_head, "HEAD"]),
        local_head
    );
    assert!(!ref_exists(&alpha_bare, "refs/heads/experiment"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_update_favors_parent_after_ordinary_update_aborts_conflict() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.member_root(0);
    let bare = d.origin.as_ref().unwrap().bare(d.repos[0].0);
    stage_origin(&root, &d.repos[0].1, &bare);
    std::fs::write(root.join("README.md"), "reviewer's suggestion\n").unwrap();
    std::fs::write(root.join("review.txt"), "nonconflicting review work\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "review changes"]);
    let original_head = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["push", "-q", "origin", "shared"]);
    git(&root, &["fetch", "-q", "origin"]);
    advance_base(&bare, "main", "README.md");
    let mut rpc = d.connect().await;

    for favor_base in [false, true] {
        let exec = d.operation_exec_id(Uuid::new_v4());
        result(
            rpc.call(
                "modules.invoke",
                builtin_invoke(
                    &d,
                    &exec,
                    "workspace.changes",
                    "update",
                    json!({"favor_base": favor_base}),
                ),
            )
            .await,
        );
        let outcome = rpc.wait_outcome(&exec).await;
        if favor_base {
            assert_eq!(outcome["status"], "succeeded", "{outcome}\n{}", d.stderr());
            assert_eq!(
                outcome["result"]["members"][0]["resolved"],
                json!(["README.md"])
            );
            assert_eq!(
                std::fs::read_to_string(root.join("README.md")).unwrap(),
                "base moved\n"
            );
        } else {
            assert_eq!(outcome["status"], "failed", "{outcome}");
            assert_eq!(git(&root, &["rev-parse", "HEAD"]), original_head);
            assert!(!ref_exists(&root, "MERGE_HEAD"));
        }
        assert_eq!(git(&root, &["status", "--porcelain"]), "");
        assert_eq!(
            std::fs::read_to_string(root.join("review.txt")).unwrap(),
            "nonconflicting review work\n"
        );
    }
    assert_eq!(
        git(&root, &["merge-base", &original_head, "HEAD"]),
        original_head
    );
    let exec = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &exec, "workspace.changes", "push", json!({})),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}\n{}", d.stderr());
    assert_eq!(
        git(&bare, &["rev-parse", "refs/heads/shared"]),
        git(&root, &["rev-parse", "HEAD"])
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_unavailable_context_fails_before_provider_handoff() {
    let files = tempfile::tempdir().unwrap();
    let missing_provider = files.path().join("missing-provider");
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let mut params = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "continuation",
        "Continue",
        policy,
        "G5_CLAUDE_PATH",
        &missing_provider,
    );
    params["input"]["recovery"] = json!({
        "included_messages":0,"omitted_messages":0,"truncated_messages":0,
        "attachment_references":0,"omitted_attachment_references":0,
        "unavailable_reason":"The original delivered objective is unavailable."
    });
    let mut rpc = d.connect().await;
    assert_eq!(
        result(rpc.call("modules.invoke", params).await)["disposition"],
        "started"
    );
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["result"]["status"], "failed", "{outcome}");
    assert_eq!(outcome["result"]["code"], "context_unavailable");
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    let events = replay["events"].as_array().unwrap();
    assert!(events.iter().any(|event| event["kind"] == "module.notice"
        && event["payload"]["code"] == "context_unavailable"));
    assert!(!events.iter().any(|event| event["kind"] == "input.accepted"
        || event["payload"]["code"] == "conversation_reconstructed"));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_expired_lease_at_bundle_begin_is_retryable() {
    for (provider, provider_path) in [
        ("claude_code", "G5_CLAUDE_PATH"),
        ("codex", "G5_CODEX_PATH"),
    ] {
        let files = tempfile::tempdir().unwrap();
        let missing_provider = files.path().join("missing-provider");
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        d.origin
            .as_ref()
            .unwrap()
            .set_session_file_begin_forbidden(true);
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let sandbox = if provider == "codex" {
            "read-only"
        } else {
            "none"
        };
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":provider,"questions":"disabled","tool_approval":"auto_deny","sandbox":sandbox,"allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let params = agent_turn_invoke(
            &d,
            task,
            1,
            provider,
            "human",
            "Continue",
            policy,
            provider_path,
            &missing_provider,
        );
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["result"]["status"], "failed", "{outcome}");
        assert_eq!(outcome["result"]["code"], "agent_turn_failed", "{outcome}");
        assert_eq!(outcome["result"]["retryable"], true, "{outcome}");
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        assert!(!replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "module.input.accepted"));
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_delivers_verified_image_through_real_module() {
    use base64::Engine as _;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let api = MockServer::start().await;
    // The dispatcher needs its authenticated Git origin while agent.turn
    // needs the attachment fixture at the same configured API base URL.
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    let git_origin = Arc::new(tokio::sync::Mutex::new(None::<String>));
    let target = Arc::clone(&git_origin);
    let attachment_api = api.uri();
    let proxy_task = tokio::spawn(async move {
        while let Ok((mut incoming, _)) = proxy.accept().await {
            let target = Arc::clone(&target);
            let attachment_api = attachment_api.clone();
            tokio::spawn(async move {
                let mut first = [0u8; 8192];
                let Ok(count) = incoming.read(&mut first).await else {
                    return;
                };
                let route = String::from_utf8_lossy(&first[..count]);
                let destination = if route.contains("/git/") {
                    target.lock().await.clone().unwrap()
                } else {
                    attachment_api
                };
                let address = destination
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap();
                let Ok(mut upstream) = tokio::net::TcpStream::connect(address).await else {
                    return;
                };
                if upstream.write_all(&first[..count]).await.is_ok() {
                    let _ = tokio::io::copy_bidirectional(&mut incoming, &mut upstream).await;
                }
            });
        }
    });
    let files = tempfile::tempdir().unwrap();
    let image = &base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
        .unwrap();
    let expected = files.path().join("expected-image");
    std::fs::write(&expected, image).unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode(image);
    let claude = executable_script(
        files.path(),
        "claude-image-fixture.sh",
        &format!(
            r#"#!/bin/sh
set -eu
IFS= read -r frame
case "$frame" in *'"text":"inspect this image"'*'"type":"image"'*|*'"type":"image"'*'"text":"inspect this image"'*) ;; *) exit 40 ;; esac
case "$frame" in *'"data":"{encoded}"'*) ;; *) exit 41 ;; esac
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"11111111-1111-4111-8111-111111111111","model":"fixture-model","mcp_servers":[]}}'
printf '%s\n' '{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"image verified"}}]}}}}'
printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"result":"image verified","usage":{{}}}}'
"#
        ),
    );
    let app_codex = executable_script(files.path(), "codex-app-server-image.py", &{
        let script = codex_app_server::peer_script("image");
        let anchor = r#"
fail_phase(request, "turn")
"#;
        assert_eq!(
            script.matches(anchor).count(),
            1,
            "image assertions must be injected exactly once at top level"
        );
        script.replace(anchor, r#"
fail_phase(request, "turn")
images = [part for part in request["params"]["input"] if part["type"] == "localImage"]
assert len(images) == 1
assert Path(images[0]["path"]).read_bytes() == Path(__file__).with_name("expected-image").read_bytes()
"#)
    });
    let mut env = inherited_loader_env();
    env.push(("G5D_API_URL".into(), proxy_url));
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env,
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    *git_origin.lock().await = Some(d.origin.as_ref().unwrap().url.clone());
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for (provider, script, key) in [
        ("claude_code", &claude, "G5_CLAUDE_PATH"),
        ("codex", &app_codex, "G5_CODEX_PATH"),
    ] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":provider,"questions":"disabled","tool_approval":"auto_deny","sandbox":if provider == "codex" { "read-only" } else { "none" },"allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            provider,
            "human",
            "inspect this image",
            policy,
            key,
            script,
        );
        if script == &app_codex {
            params["version"] = json!("1.0.8");
            params["input"]["provider"]["options"]["codex_transport"] =
                json!("app_server_stdio_v1");
        }
        let input_id = params["input"]["task"]["initial_input"]["input_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let session_id = params["input"]["task"]["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        Mock::given(method("POST"))
            .and(path(format!(
                "/api/internal/chat-sessions/{session_id}/session-files/begin"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&api)
            .await;
        let attachment_id = Uuid::new_v4().to_string();
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/internal/chat-attachments/{attachment_id}/content"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "attachmentId": attachment_id,
                "sessionId": session_id,
                "inputId": input_id,
                "fileName": "image.png",
                "contentType": "image/png",
                "sizeBytes": image.len(),
                "sha256": format!("{:x}", Sha256::digest(image)),
                "contentBase64": encoded,
            })))
            .expect(1)
            .mount(&api)
            .await;
        params["input"]["attachments"] = json!([{
            "kind":"image", "id":attachment_id, "input_id":input_id,
            "file_name":"image.png", "content_type":"image/png",
            "size_bytes":image.len(), "sha256":format!("{:x}", Sha256::digest(image)),
        }]);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(
            outcome["result"]["status"], "succeeded",
            "{provider}: {outcome}"
        );
    }
    // Refusals must survive the real dispatcher/module boundary without a
    // transport acceptance, even when the stored bytes match their digest.
    for case in ["file", "invalid_image", "missing", "invalid_manifest"] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "inspect this image",
            policy,
            "G5_CLAUDE_PATH",
            &claude,
        );
        let input_id = params["input"]["task"]["initial_input"]["input_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let session_id = params["input"]["task"]["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let attachment_id = Uuid::new_v4().to_string();
        let (kind, mime, bytes) = if case == "file" {
            ("file", "application/pdf", b"file".as_slice())
        } else {
            ("image", "image/png", b"not an image".as_slice())
        };
        let digest = format!("{:x}", Sha256::digest(bytes));
        params["input"]["attachments"] = json!([{
            "kind":kind, "id":attachment_id, "input_id":input_id,
            "file_name":"image.png", "content_type":mime,
            "size_bytes":bytes.len(), "sha256":digest,
        }]);
        if case == "invalid_manifest" {
            params["input"]["attachment_preflight_error"] =
                json!("reserved attachment descriptors are invalid");
        }
        if case == "invalid_image" {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/api/internal/chat-attachments/{attachment_id}/content"
                )))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "attachmentId":attachment_id, "sessionId":session_id, "inputId":input_id,
                    "fileName":"image.png", "contentType":mime,
                    "sizeBytes":bytes.len(), "sha256":digest,
                    "contentBase64":base64::engine::general_purpose::STANDARD.encode(bytes),
                })))
                .expect(1)
                .mount(&api)
                .await;
        }
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["result"]["status"], "failed", "{case}: {outcome}");
        let expected = match case {
            "file" => "unsupported_attachment_content",
            "invalid_image" => "attachment_invalid_image",
            "missing" => "attachment_unavailable",
            _ => "invalid_attachment_descriptor",
        };
        assert_eq!(outcome["result"]["code"], expected, "{case}: {outcome}");
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        assert!(
            !replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["kind"] == "input.accepted"),
            "{case}: {replay}"
        );
    }
    proxy_task.abort();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_startup_errors_are_durable_without_publishing_stderr_secrets() {
    for provider in ["codex", "claude_code"] {
        for missing_binary in [false, true] {
            let files = tempfile::tempdir().unwrap();
            let script = executable_script(
                files.path(),
                "startup-error.sh",
                r#"#!/bin/sh
IFS= read -r input
printf 'Error loading config.toml: invalid transport; token=%s\n' "$G5_TEST_SECRET" >&2
exit 1
"#,
            );
            let d = TestDispatcher::spawn(
                bin(),
                DispatcherOptions {
                    env: inherited_loader_env(),
                    git_origin: true,
                    ..Default::default()
                },
            )
            .await;
            stage_origin(
                &d.repo_root(),
                &d.repos[0].1,
                &d.origin.as_ref().unwrap().bare(d.repo),
            );
            let task = Uuid::new_v4();
            let exec = d.task_exec_id(task, 1);
            let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":provider,"questions":"disabled","tool_approval":"auto_deny","sandbox":if provider == "codex" { "read-only" } else { "none" },"allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
            let missing = files.path().join("missing-cli");
            let mut params = agent_turn_invoke(
                &d,
                task,
                1,
                provider,
                "human",
                "hello",
                policy,
                if provider == "codex" {
                    "G5_CODEX_PATH"
                } else {
                    "G5_CLAUDE_PATH"
                },
                if missing_binary { &missing } else { &script },
            );
            params["secrets"]["G5_TEST_SECRET"] = json!("private-token-must-not-appear");
            let mut rpc = d.connect().await;
            assert_eq!(
                result(rpc.call("modules.invoke", params).await)["disposition"],
                "started"
            );
            let outcome = rpc.wait_outcome(&exec).await;
            assert_eq!(outcome["result"]["status"], "failed", "{outcome}");
            assert_eq!(
                outcome["result"]["provider_started"], !missing_binary,
                "only a successful process spawn may keep a rejected-resume fence: {outcome}"
            );
            let replay = result(
                rpc.call(
                    "events.replay",
                    d.params(true, json!({"exec_id":exec,"from_seq":1})),
                )
                .await,
            );
            let notices: Vec<_> = replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| {
                    event["kind"] == "module.notice" && event["payload"]["level"] == "error"
                })
                .collect();
            assert_eq!(notices.len(), 1, "{replay}");
            let payload = &notices[0]["payload"];
            assert_eq!(payload["level"], "error");
            if missing_binary {
                assert_eq!(payload["code"], "agent_turn_failed");
            } else {
                assert_eq!(payload["code"], "provider_start_failed");
                assert!(payload["message"]
                    .as_str()
                    .unwrap()
                    .contains("could not load its configuration"));
            }
            assert!(!replay.to_string().contains("private-token-must-not-appear"));
            assert!(!replay.to_string().contains("token="));
        }
    }
}

/// Linux subreaper acceptance through the shipped dispatcher and agent adapter.
/// Tools deliberately leave the provider group/session and ignore interruption.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn agent_execution_tree_is_reaped_before_settlement() {
    for mode in [
        "owner_cancel",
        "owner_pause",
        "interrupt",
        "beyond_setup",
        "setup_timeout",
        "log",
        "supervisor_failure",
        "provider_error",
    ] {
        let providers = tempfile::tempdir().unwrap();
        let grandchild = executable_script(
            providers.path(),
            "orphaned-grandchild",
            r#"#!/bin/sh
trap '' INT TERM
echo $$ >> "$HOME/pids"
echo ready > "$HOME/grandchild-ready"
while :; do echo grandchild >> "$HOME/writes"; sleep 0.02; done
"#,
        );
        let tool = executable_script(
            providers.path(),
            "detached-tool",
            &format!(
                r#"#!/bin/sh
trap '' INT TERM
echo $$ >> "$HOME/pids"
# The intermediate shell exits, so the second session is adopted while the
# supervisor/provider/tool are still alive. Cleanup must own both lineages.
sh -c 'setsid "$1" >/dev/null 2>&1 &' sh '{}'
while [ ! -f "$HOME/grandchild-ready" ]; do sleep 0.01; done
echo ready > "$HOME/tool-ready"
while :; do echo tool >> "$HOME/writes"; sleep 0.02; done
"#,
                grandchild.display()
            ),
        );
        let provider = executable_script(
            providers.path(),
            "provider",
            &format!(
                r#"#!/bin/sh
trap 'echo interrupted > "$HOME/interrupted"' INT
trap '' TERM
echo $$ >> "$HOME/pids"
echo "$PPID" > "$HOME/supervisor"
echo "$PPID" >> "$HOME/pids"
setsid '{}' &
while [ ! -f "$HOME/tool-ready" ]; do sleep 0.01; done
echo ready > "$HOME/ready"
case "$HOME" in
  *setup_timeout-primary) ;; # Fill stdin to exercise setup failure cleanup.
  *) IFS= read -r initial ;;
esac
while :; do
  echo provider >> "$HOME/writes"
  # A provider that fails on its own leaves a failed turn: still reaped.
  if [ -f "$HOME/fail" ]; then exit 3; fi
  if [ -f "$HOME/flood" ]; then
    printf '%s\n' 'log limit payload payload payload payload payload payload payload payload' >&2
  else
    sleep 0.02
  fi
done
"#,
                tool.display()
            ),
        );
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                fixture_module: true,
                bounded_execution_slots: Some(2),
                catalog_edit: Some(Arc::new(move |catalog| {
                    let entries = catalog["modules"].as_array_mut().unwrap();
                    let entry = entries
                        .iter_mut()
                        .find(|entry| entry["module_id"] == "agent.turn")
                        .unwrap();
                    // A stale catalog bound must not become an agent lifetime limit.
                    entry["limits"]["wall_clock_ms"] = json!(10000);
                    // An agent turn declares no output cap; the dispatcher's
                    // own limit stop is exercised through its log cap.
                    entry["limits"]["max_log_bytes"] = json!(256 * 1024);
                })),
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let mut rpc = d.connect().await;
        let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":60000,"max_output_bytes":1048576}});
        let mut executions = Vec::new();
        // Complete sibling startup before launching the primary turn.
        for name in ["sibling", "primary"] {
            let home = d.repo_root().join(format!("containment-{mode}-{name}"));
            std::fs::create_dir(&home).unwrap();
            let task = Uuid::new_v4();
            let exec = d.task_exec_id(task, 1);
            let prompt = if mode == "setup_timeout" && name == "primary" {
                "x".repeat(256 * 1024)
            } else {
                "containment test".into()
            };
            let mut params = agent_turn_invoke(
                &d,
                task,
                1,
                "claude_code",
                "human",
                &prompt,
                policy.clone(),
                "G5_CLAUDE_PATH",
                &provider,
            );
            params["secrets"]["HOME"] = json!(home);
            if name == "sibling" {
                params["input"]["limits"]["setup_timeout_ms"] = json!(60000);
            } else if matches!(mode, "beyond_setup" | "setup_timeout") {
                params["input"]["limits"]["setup_timeout_ms"] = json!(5000);
            }
            assert_eq!(
                result(rpc.call("modules.invoke", params).await)["disposition"],
                "started"
            );
            timeout(Duration::from_secs(10), async {
                while !home.join("ready").exists() {
                    sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap_or_else(|_| panic!("{mode}/{name}: provider never ready: {}", d.stderr()));
            executions.push((exec, home));
        }
        executions.reverse(); // primary first for the assertions below
        let replacement = d.task_exec_id(Uuid::new_v4(), 1);
        error(
            rpc.call(
                "modules.invoke",
                fixture_invoke(&d, &replacement, json!({"behavior":"sleep","ms":10})),
            )
            .await,
            "EXECUTION_SLOTS_EXHAUSTED",
        );
        let (exec, home) = &executions[0];
        match mode {
            "setup_timeout" => {}
            "beyond_setup" => {
                // Startup completed above. The running process tree survives
                // both the input setup bound and a stale catalog clock bound.
                sleep(Duration::from_secs(11)).await;
                assert_eq!(
                    result(
                        rpc.call("modules.status", d.params(false, json!({"exec_id":exec})))
                            .await
                    )["status"],
                    "running",
                );
                result(
                    rpc.call(
                        "modules.cancel",
                        d.params(true, json!({"exec_id":exec,"reason":"owner_cancel"})),
                    )
                    .await,
                );
            }
            "log" => std::fs::write(home.join("flood"), "go").unwrap(),
            "provider_error" => std::fs::write(home.join("fail"), "go").unwrap(),
            "supervisor_failure" => {
                let pid: i32 = std::fs::read_to_string(home.join("supervisor"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            reason => {
                result(
                    rpc.call(
                        "modules.cancel",
                        d.params(true, json!({"exec_id":exec,"reason":reason})),
                    )
                    .await,
                );
                error(
                    rpc.call(
                        "modules.invoke",
                        fixture_invoke(&d, &replacement, json!({"behavior":"sleep","ms":10})),
                    )
                    .await,
                    "EXECUTION_SLOTS_EXHAUSTED",
                );
            }
        }
        let outcome = timeout(Duration::from_secs(15), rpc.wait_outcome(exec))
            .await
            .expect("bounded cleanup");
        let expected = match mode {
            "log" => "limit_exceeded",
            "supervisor_failure" => "failed",
            "setup_timeout" | "provider_error" => "succeeded",
            _ => "cancelled",
        };
        assert_eq!(outcome["status"], expected, "{mode}: {outcome}");
        if mode == "log" {
            assert_eq!(outcome["detail"]["limit"], "max_log_bytes", "{outcome}");
        }
        if mode == "setup_timeout" {
            assert_eq!(outcome["result"]["status"], "timed_out", "{outcome}");
        }
        if mode == "provider_error" {
            assert_eq!(outcome["result"]["status"], "failed", "{outcome}");
        }
        for pid in std::fs::read_to_string(home.join("pids")).unwrap().lines() {
            let pid: i32 = pid.parse().unwrap();
            assert_eq!(
                unsafe { libc::kill(pid, 0) },
                -1,
                "{mode}: process {pid} survived settlement"
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        if matches!(mode, "owner_cancel" | "owner_pause" | "interrupt") {
            assert!(
                home.join("interrupted").exists(),
                "provider did not receive cooperative interruption"
            );
        }
        assert_eq!(
            result(
                rpc.call(
                    "modules.invoke",
                    fixture_invoke(&d, &replacement, json!({"behavior":"sleep","ms":10}))
                )
                .await
            )["disposition"],
            "started",
            "reaped execution must release capacity"
        );
        rpc.wait_outcome(&replacement).await;
        let writes = std::fs::read(home.join("writes")).unwrap();
        let replay = result(
            rpc.call(
                "events.replay",
                d.params(true, json!({"exec_id":exec,"from_seq":1})),
            )
            .await,
        );
        let sibling_home = &executions[1].1;
        let sibling_writes = std::fs::metadata(sibling_home.join("writes"))
            .unwrap()
            .len();
        sleep(Duration::from_millis(150)).await;
        assert_eq!(
            std::fs::read(home.join("writes")).unwrap(),
            writes,
            "late workspace writes"
        );
        assert_eq!(
            result(
                rpc.call(
                    "events.replay",
                    d.params(true, json!({"exec_id":exec,"from_seq":1}))
                )
                .await
            ),
            replay,
            "late events"
        );
        assert!(
            std::fs::metadata(sibling_home.join("writes"))
                .unwrap()
                .len()
                > sibling_writes,
            "sibling was interrupted"
        );
        result(
            rpc.call(
                "modules.cancel",
                d.params(true, json!({"exec_id":executions[1].0})),
            )
            .await,
        );
        assert_eq!(
            rpc.wait_outcome(&executions[1].0).await["status"],
            "cancelled"
        );
    }
}

/// Process state from `/proc/<pid>/stat`; `None` once the process is gone.
#[cfg(target_os = "linux")]
fn proc_state(pid: i32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.trim_start().chars().next()
}

/// A `/proc/<pid>/status` field of a live process, split on whitespace.
#[cfg(target_os = "linux")]
fn proc_status_field(pid: i32, field: &str) -> Vec<String> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    status
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .unwrap()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The real uid of a live process.
#[cfg(target_os = "linux")]
fn proc_uid(pid: i32) -> u32 {
    proc_status_field(pid, "Uid:")[0].parse().unwrap()
}

#[cfg(target_os = "linux")]
fn proc_parent(pid: i32) -> i32 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    status
        .lines()
        .find_map(|line| line.strip_prefix("PPid:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[cfg(target_os = "linux")]
async fn read_pid(path: &Path) -> i32 {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(pid) = text.trim().parse() {
                    return pid;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} was never written", path.display()))
}

/// A provider script that detaches a long-lived process (through `launcher`,
/// which may switch identity first), records the supervisor, and then ends its
/// turn with a successful result, reaches its turn budget, keeps working until
/// stopped, or leaves a permission request unanswered. Its session-title call
/// stalls.
#[cfg(target_os = "linux")]
fn leftover_provider(dir: &Path, launcher: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let daemon = executable_script(
        dir,
        "leftover-daemon",
        r#"#!/bin/sh
trap '' INT TERM
echo $$ > "$1.tmp" && mv "$1.tmp" "$1"
while :; do sleep 0.05; done
"#,
    );
    let provider = executable_script(
        dir,
        "leftover-provider",
        &format!(
            r#"#!/bin/sh
set -eu
case " $* " in *" --no-session-persistence "*)
  echo started > "$HOME/title-started"
  sleep 600
  exit 0 ;;
esac
echo "$PPID" > "$HOME/supervisor"
{launcher} setsid '{}' "$HOME/daemon.pid" </dev/null >/dev/null 2>&1 &
while [ ! -f "$HOME/daemon.pid" ]; do sleep 0.01; done
IFS= read -r initial
if [ -f "$HOME/keep-working" ]; then
  echo ready > "$HOME/ready"
  while :; do sleep 0.05; done
fi
printf '%s\n' '{{"type":"system","subtype":"init","session_id":"77777777-7777-4777-8777-777777777777","model":"fixture-model","mcp_servers":[]}}'
if [ -f "$HOME/ask-permission" ]; then
  printf '%s\n' '{{"type":"control_request","request_id":"provider-q-1","request":{{"subtype":"can_use_tool","tool_name":"Write","input":{{"file_path":"/tmp/leftover","content":"x"}}}}}}'
  cat >/dev/null
  exit 0
fi
if [ -f "$HOME/max-turns" ]; then
  printf '%s\n' '{{"type":"result","subtype":"error_max_turns","is_error":true,"result":"Reached the turn budget","usage":{{}}}}'
  exit 0
fi
printf '%s\n' '{{"type":"result","subtype":"success","is_error":false,"result":"database started","usage":{{}}}}'
"#,
            daemon.display()
        ),
    );
    (provider, daemon)
}

#[cfg(target_os = "linux")]
fn leftover_policy() -> Value {
    json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":[],"limits":{"setup_timeout_ms":60000,"max_output_bytes":1048576}})
}

/// A turn the provider ends itself leaves what the agent started running for
/// the agent to manage: the execution owner releases it instead of reaping it,
/// the turn settles at once and its execution capacity is free. An outcome the
/// supervisor forces (an unanswered permission request), a reached turn
/// budget (task or policy limit) and an interruption of the supervisor while
/// it finalizes a successful turn still reap it.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_normal_finish_releases_processes_the_agent_left_running() {
    let providers = tempfile::tempdir().unwrap();
    let (provider, _) = leftover_provider(providers.path(), "");
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            fixture_module: true,
            bounded_execution_slots: Some(1),
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    for mode in [
        "finish",
        "permission_timeout",
        "max_turns_task",
        "max_turns_policy",
        "interrupted_finalization",
    ] {
        let home = d.repo_root().join(format!("leftover-{mode}"));
        std::fs::create_dir(&home).unwrap();
        let mut policy = leftover_policy();
        match mode {
            "permission_timeout" => {
                std::fs::write(home.join("ask-permission"), "1").unwrap();
                policy = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
                policy["limits"]["native_response_timeout_ms"] = json!(200);
            }
            "max_turns_task" | "max_turns_policy" => {
                // A task limit must fit within the policy limit.
                std::fs::write(home.join("max-turns"), "1").unwrap();
                policy["limits"]["max_turns"] = json!(2);
            }
            _ => {}
        }
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "start a database",
            policy,
            "G5_CLAUDE_PATH",
            &provider,
        );
        params["secrets"]["HOME"] = json!(home);
        if mode == "max_turns_task" {
            params["input"]["limits"]["max_turns"] = json!(2);
        }
        if mode == "interrupted_finalization" {
            // Title generation runs only for a real (non-fake) provider.
            use_native_provider_fixture(&mut params);
            params["input"]["provider"]["options"]["session_title"] = json!(true);
            params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
        }
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started",
            "{mode}"
        );
        let daemon = read_pid(&home.join("daemon.pid")).await;
        if mode == "interrupted_finalization" {
            // The provider succeeded; the supervisor is generating a title
            // when it is interrupted directly, without a dispatcher stop.
            let supervisor = read_pid(&home.join("supervisor")).await;
            timeout(Duration::from_secs(10), async {
                while !home.join("title-started").exists() {
                    sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("title generation never started");
            unsafe {
                libc::kill(supervisor, libc::SIGTERM);
            }
        }
        let outcome = timeout(Duration::from_secs(25), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("{mode}: the turn did not settle"));
        assert_eq!(outcome["status"], "succeeded", "{mode}: {outcome}");
        sleep(Duration::from_millis(300)).await;
        if mode == "finish" {
            assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
            let state = proc_state(daemon);
            assert!(
                state.is_some_and(|state| state != 'Z'),
                "the process the agent left running was stopped: {state:?}"
            );
        } else {
            let (status, code) = match mode {
                "permission_timeout" => ("suspended", "native_response_timeout"),
                "interrupted_finalization" => ("suspended", "interrupted"),
                _ => ("succeeded", "max_turns_reached"),
            };
            assert_eq!(outcome["result"]["status"], status, "{mode}: {outcome}");
            assert_eq!(outcome["result"]["code"], code, "{mode}: {outcome}");
            assert_eq!(
                proc_state(daemon),
                None,
                "{mode}: the turn left the process running"
            );
        }
        let replacement = d.task_exec_id(Uuid::new_v4(), 1);
        assert_eq!(
            result(
                rpc.call(
                    "modules.invoke",
                    fixture_invoke(&d, &replacement, json!({"behavior":"sleep","ms":10})),
                )
                .await
            )["disposition"],
            "started",
            "{mode}: the turn holds no execution capacity once settled"
        );
        rpc.wait_outcome(&replacement).await;
        if mode == "finish" {
            unsafe {
                libc::kill(daemon, libc::SIGKILL);
            }
        }
    }
}

/// The execution owner keeps the dispatcher identity while the supervisor
/// runs as the module identity, so it reaps descendants that switched
/// identity (as through the guest's sudo) on a stop, and releases them after
/// a normal finish. Needs root on Linux to switch identities.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_owner_reaps_or_releases_descendants_of_another_identity() {
    // SAFETY: plain libc query.
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("skipped: identity separation needs root to switch to the module uid");
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let providers = tempfile::tempdir().unwrap();
    std::fs::set_permissions(providers.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    // A setuid-root launcher stands in for the guest's passwordless sudo.
    let source = providers.path().join("become-root.c");
    std::fs::write(
        &source,
        "#define _GNU_SOURCE\n#include <grp.h>\n#include <unistd.h>\nint main(int argc, char **argv) {\n  if (argc < 2 || setgroups(0, 0) || setresgid(0, 0, 0) || setresuid(0, 0, 0)) return 125;\n  execvp(argv[1], argv + 1);\n  return 126;\n}\n",
    )
    .unwrap();
    let become_root = providers.path().join("become-root");
    let compiled = std::process::Command::new("cc")
        .arg("-o")
        .arg(&become_root)
        .arg(&source)
        .status()
        .expect("cc is required to build the sudo stand-in");
    assert!(compiled.success());
    std::fs::set_permissions(&become_root, std::fs::Permissions::from_mode(0o4755)).unwrap();
    let (provider, _) =
        leftover_provider(providers.path(), &format!("'{}'", become_root.display()));
    for finish in [false, true] {
        let d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: [
                    inherited_loader_env(),
                    vec![
                        ("G5D_MODULE_UID".into(), "65534".into()),
                        ("G5D_MODULE_GID".into(), "65534".into()),
                    ],
                ]
                .concat(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        for dir in [
            d.root.path().to_path_buf(),
            d.workspace_root(),
            d.repo_root(),
        ] {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        }
        // The dispatcher handed the workspace to the module identity at start;
        // stage the origin as the harness, then hand the tree back.
        let chown = |owner: &str| {
            let status = std::process::Command::new("chown")
                .args(["-R", owner])
                .arg(d.workspace_root())
                .status()
                .unwrap();
            assert!(status.success());
        };
        chown("0:0");
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        chown("65534:65534");
        let mut rpc = d.connect().await;
        let home = d.repo_root().join(format!("identity-home-{finish}"));
        std::fs::create_dir(&home).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o777)).unwrap();
        if !finish {
            std::fs::write(home.join("keep-working"), "1").unwrap();
        }
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            "start a database as another user",
            leftover_policy(),
            "G5_CLAUDE_PATH",
            &provider,
        );
        params["secrets"]["HOME"] = json!(home);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        let pid_file = home.join("daemon.pid");
        let started = tokio::select! {
            pid = read_pid(&pid_file) => Ok(pid),
            outcome = rpc.wait_outcome(&exec) => Err(outcome),
        };
        let daemon = match started {
            Ok(pid) => pid,
            Err(outcome) => {
                let events = replay_events(&mut rpc, &d, &exec).await;
                panic!("finish={finish}: the turn ended before its daemon started: {outcome} {events:?}");
            }
        };
        assert_eq!(proc_uid(daemon), 0, "the stand-in did not switch identity");
        if !finish {
            let supervisor = read_pid(&home.join("supervisor")).await;
            assert_eq!(proc_uid(supervisor), 65534, "supervisor identity");
            assert_eq!(
                proc_status_field(supervisor, "Gid:")[0],
                "65534",
                "supervisor group"
            );
            assert!(
                proc_status_field(supervisor, "Groups:").is_empty(),
                "the supervisor keeps no supplementary groups"
            );
            assert_eq!(
                proc_uid(proc_parent(supervisor)),
                0,
                "the execution owner keeps the dispatcher identity"
            );
            timeout(Duration::from_secs(10), async {
                while !home.join("ready").exists() {
                    sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("provider never ready");
            result(
                rpc.call(
                    "modules.cancel",
                    d.params(true, json!({"exec_id":exec,"reason":"owner_cancel"})),
                )
                .await,
            );
        }
        let outcome = timeout(Duration::from_secs(15), rpc.wait_outcome(&exec))
            .await
            .unwrap_or_else(|_| panic!("finish={finish}: cleanup did not settle the turn"));
        if finish {
            assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
            assert!(
                proc_state(daemon).is_some_and(|state| state != 'Z'),
                "a normal finish stopped the other identity's process"
            );
            unsafe {
                libc::kill(daemon, libc::SIGKILL);
            }
        } else {
            assert_eq!(outcome["status"], "cancelled", "{outcome}");
            assert_eq!(
                proc_state(daemon),
                None,
                "a stop left the other identity's process running"
            );
        }
    }
}

/// Real module admission, provider process, native-settings overlay, Git
/// transport and flush in a personal workspace, whose own repository holds the
/// owner's memory (`g5.personal.memory`). The provider here scripts file
/// writes; live model behavior has a separate opt-in probe.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn personal_memory_binds_the_personal_member_and_flush_guards_it() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "memory-provider.py",
        r#"#!/usr/bin/env python3
import json, os, pathlib, sys
args=sys.argv[1:]
settings=json.loads(args[args.index('--settings')+1])
request=json.loads(sys.stdin.readline())
prompt=request['message']['content']
if not isinstance(prompt,str): prompt=prompt[0]['text']
assert args[args.index('--permission-mode')+1]=='dontAsk'
assert args[args.index('--setting-sources')+1]=='user,project'
assert '--allowedTools' not in args
assert 'permissions' not in settings
assert json.loads(pathlib.Path('.claude/settings.json').read_text())['permissions']['deny']==['WebFetch']
assert 'autoMemoryDirectory' in settings
assert 'G5D_AGENT_PERSONAL_ROOT' not in os.environ
memory=pathlib.Path(settings['autoMemoryDirectory'])
assert memory.is_absolute() and memory.parts[-3:]==('personal','memory','claude')
if 'save' in prompt:
    (memory/'MEMORY.md').write_text('first lesson')
    (memory/'obsolete.md').write_text('obsolete')
else:
    assert (memory/'MEMORY.md').read_text()=='human edit'
    (memory/'MEMORY.md').write_text('updated lesson')
    (memory/'topic.md').write_text('new topic')
    (memory/'obsolete.md').unlink()
private=pathlib.Path(os.environ['HOME'])/'.claude'
private.mkdir(exist_ok=True)
(private/'private-session.json').write_text('private transcript')
print(json.dumps({'type':'system','subtype':'init','session_id':'11111111-1111-4111-8111-111111111111','model':'fixture-model','mcp_servers':[]}))
print(json.dumps({'type':'result','subtype':'success','is_error':False,'session_id':'11111111-1111-4111-8111-111111111111','result':'memory written','usage':{}}))
"#,
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            repos: vec![RepoSpec {
                root_path: "personal".into(),
                branch: "users/me".into(),
                upstream_branch: None,
                follow_branch: true,
                ..RepoSpec::default()
            }],
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let repo = d.repo_root();
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    let settings = r#"{"permissions":{"deny":["WebFetch"]},"autoMemoryDirectory":"/unrelated"}"#;
    std::fs::write(repo.join(".claude/settings.json"), settings).unwrap();
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":["Read","Write","Edit"],"limits":{"setup_timeout_ms":20000,"max_output_bytes":1048576}});
    for prompt in ["save", "update"] {
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut invocation = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            policy.clone(),
            "G5_CLAUDE_PATH",
            &claude,
        );
        invocation["input"]["workspace_target"] =
            json!({"relative_path":"personal","repo_id":d.repo,"personal":true});
        // A task secret cannot replace the dispatcher's repository context.
        invocation["secrets"]["G5D_AGENT_PERSONAL_ROOT"] = json!("elsewhere");
        result(rpc.call("modules.invoke", invocation).await);
        let outcome = rpc.wait_outcome(&exec).await;
        assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
        if prompt == "save" {
            git(&repo, &["add", "."]);
            git(&repo, &["commit", "-qm", "first memory"]);
            std::fs::write(repo.join("memory/claude/MEMORY.md"), "human edit").unwrap();
            // This future topic is absent at launch; capture must inspect the
            // resulting tree, not just the paths seen during setup.
            std::fs::write(
                repo.join(".gitignore"),
                "memory/claude/MEMORY.md\nmemory/claude/topic.md\n",
            )
            .unwrap();
        }
    }
    assert!(!d.workspace_root().join("elsewhere").exists());
    assert_eq!(
        std::fs::read_to_string(repo.join(".claude/settings.json")).unwrap(),
        settings
    );
    std::fs::create_dir_all(repo.join(".g5/state")).unwrap();
    std::fs::write(repo.join(".g5/state/private.json"), "private state").unwrap();
    std::fs::write(repo.join("staged.txt"), "staged content").unwrap();
    git(&repo, &["add", "staged.txt"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    result(rpc.call("admission.close", d.params(true, json!({}))).await);
    let refused = rpc
        .call(
            "workspace.flush",
            d.params(true, json!({"reason":"shutdown"})),
        )
        .await;
    assert!(
        refused.to_string().contains("FLUSH_MEMORY_IGNORED"),
        "{refused}"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(repo.join("memory/claude/topic.md")).unwrap(),
        "new topic"
    );
    // Ordinary staging before a later ignore rule must not hide this topic
    // from capture's separate HEAD-based index.
    std::fs::remove_file(repo.join(".gitignore")).unwrap();
    git(&repo, &["add", "memory/claude/topic.md"]);
    std::fs::write(repo.join(".gitignore"), "memory/claude/topic.md\n").unwrap();
    let index = std::fs::read(repo.join(".git/index")).unwrap();
    let refused = rpc
        .call(
            "workspace.flush",
            d.params(true, json!({"reason":"shutdown"})),
        )
        .await;
    assert!(
        refused.to_string().contains("FLUSH_MEMORY_IGNORED"),
        "{refused}"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    // Already committed memory is still captured even if a later ignore rule
    // matches it, because Git's capture index already tracks that path.
    std::fs::write(repo.join(".gitignore"), "memory/claude/MEMORY.md\n").unwrap();
    let receipt = result(
        rpc.call(
            "workspace.flush",
            d.params(true, json!({"reason":"shutdown"})),
        )
        .await,
    );
    let snapshot = receipt["repos"][0]["snapshot_oid"].as_str().unwrap();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    assert_eq!(
        git(
            &bare,
            &["show", &format!("{snapshot}:memory/claude/MEMORY.md")]
        ),
        "updated lesson"
    );
    assert_eq!(
        git(
            &bare,
            &["show", &format!("{snapshot}:memory/claude/topic.md")]
        ),
        "new topic"
    );
    let tree = git(&bare, &["ls-tree", "-r", "--name-only", snapshot]);
    assert!(!tree.contains("obsolete.md"));
    assert!(!tree.contains("private"));
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
}

/// Without the owner's personal repository, memory is off and no workspace
/// repository receives it; an unsafe memory path fails before the provider runs.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn personal_memory_is_off_without_the_personal_repository_and_refuses_unsafe_paths() {
    let providers = tempfile::tempdir().unwrap();
    let claude = executable_script(
        providers.path(),
        "memory-skip.py",
        r#"#!/usr/bin/env python3
import json, os, pathlib, sys
args=sys.argv[1:]
settings=json.loads(args[args.index('--settings')+1])
assert settings['autoMemoryEnabled'] is False
assert 'autoMemoryDirectory' not in settings
assert os.environ['CLAUDE_CODE_DISABLE_AUTO_MEMORY']=='1'
pathlib.Path(os.environ['HOME'],'spawned').write_text('yes')
sys.stdin.readline()
print(json.dumps({'type':'result','subtype':'success','is_error':False,'result':'skipped','usage':{}}))
"#,
    );
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":["Read"],"limits":{"setup_timeout_ms":20000,"max_output_bytes":1048576}});
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let task = Uuid::new_v4();
    let mut invocation = agent_turn_invoke(
        &d,
        task,
        1,
        "claude_code",
        "human",
        "skip",
        policy.clone(),
        "G5_CLAUDE_PATH",
        &claude,
    );
    // A task secret cannot name a personal repository Start left out.
    invocation["secrets"]["G5D_AGENT_PERSONAL_ROOT"] = json!("spoofed-root");
    result(rpc.call("modules.invoke", invocation).await);
    let exec = d.task_exec_id(task, 1);
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    assert!(
        replay["events"].as_array().unwrap().iter().any(|event| {
            event["kind"] == "module.notice"
                && event["payload"]["code"] == "shared_memory_skipped"
                && event["payload"]["message"]
                    == "The owner's personal repository is not in this workspace, so memory is off."
        }),
        "{replay}"
    );
    assert!(!d.repo_root().join(".g5/memory").exists());
    assert!(!d.repo_root().join("memory").exists());
    assert!(!d.workspace_root().join("spoofed-root").exists());

    let mounted = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            references: vec![personal_mount_spec()],
            ..DispatcherOptions::default()
        },
    )
    .await;
    stage_origin(
        &mounted.repo_root(),
        &mounted.repos[0].1,
        &mounted.origin.as_ref().unwrap().bare(mounted.repo),
    );
    let outside = providers.path().join("private");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, mounted.workspace_root().join("personal/memory")).unwrap();
    let mut rpc = mounted.connect().await;
    let task = Uuid::new_v4();
    let invocation = agent_turn_invoke(
        &mounted,
        task,
        1,
        "claude_code",
        "human",
        "invalid",
        policy,
        "G5_CLAUDE_PATH",
        &claude,
    );
    result(rpc.call("modules.invoke", invocation).await);
    let outcome = rpc.wait_outcome(&mounted.task_exec_id(task, 1)).await;
    assert_eq!(outcome["result"]["status"], "failed", "{outcome}");
    assert_eq!(outcome["result"]["code"], "shared_memory_setup_failed");
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

/// The owner's personal repository mounted beside the members (`g5.personal.mount`).
fn personal_mount_spec() -> RepoSpec {
    RepoSpec {
        root_path: "personal".into(),
        role: None,
        branch: "users/me".into(),
        upstream_branch: None,
        follow_branch: false,
    }
}

/// Commit `file` in the code member and push it through the relay, as an
/// agent's own `git push` does.
async fn push_code(d: &TestDispatcher, file: &str) {
    let code = d.repo_root();
    std::fs::write(code.join(file), format!("{file}\n")).unwrap();
    git(&code, &["add", file]);
    git(&code, &["commit", "-qm", file]);
    let output = native_git_command(d, &["push", "-q", "origin", "HEAD"]).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Wait for `check` to hold, failing after `limit`.
async fn eventually(limit: Duration, what: &str, mut check: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + limit;
    while !check() {
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        sleep(Duration::from_millis(100)).await;
    }
}

fn try_git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Memory follows code pushes (`g5.personal.memory-sync`): a native push of a
/// workspace repository commits and publishes only the mounted personal
/// repository's memory, keeps the owner's other work in place, skips rather
/// than replaying over it, and merges concurrent index lines as a union.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn personal_memory_follows_code_pushes_into_the_mount() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            references: vec![personal_mount_spec()],
            ..Default::default()
        },
    )
    .await;
    let origin = d.origin.as_ref().unwrap();
    let code = d.repo_root();
    stage_origin(&code, &d.repos[0].1, &origin.bare(d.repo));
    let layout: Value =
        serde_json::from_slice(&std::fs::read(d.workspace_root().join(".g5/layout.json")).unwrap())
            .unwrap();
    git(
        &code,
        &[
            "remote",
            "set-url",
            "origin",
            layout["repos"][0]["git_url"].as_str().unwrap(),
        ],
    );
    let (personal_id, spec) = d.references[0].clone();
    let personal = d.workspace_root().join(&spec.root_path);
    let bare = origin.bare(personal_id);
    let branch = spec.branch.clone();
    std::fs::write(personal.join("notes.md"), "notes\n").unwrap();
    git(&personal, &["add", "-A"]);
    git(&personal, &["commit", "-qm", "pinned"]);
    git(&personal, &["push", "-q", bare.to_str().unwrap(), &branch]);
    let pinned = git(&personal, &["rev-parse", "HEAD"]);
    let tip = || git(&bare, &["rev-parse", &branch]);

    // The agent saves memory while the owner has other work in the checkout.
    let memory = personal.join("memory/claude");
    std::fs::create_dir_all(&memory).unwrap();
    std::fs::write(memory.join("MEMORY.md"), "- [First](first.md) — one\n").unwrap();
    std::fs::write(memory.join("first.md"), "first lesson\n").unwrap();
    let codex_memory = personal.join("memory/codex");
    std::fs::create_dir_all(&codex_memory).unwrap();
    std::fs::write(codex_memory.join("memory_summary.md"), "Codex lesson\n").unwrap();
    std::fs::write(personal.join("notes.md"), "draft notes\n").unwrap();
    std::fs::write(personal.join("staged.md"), "staged\n").unwrap();
    git(&personal, &["add", "staged.md"]);
    push_code(&d, "one.txt").await;
    eventually(Duration::from_secs(30), "memory published", || {
        tip() != pinned
    })
    .await;
    let first = tip();
    assert_eq!(git(&bare, &["rev-parse", &format!("{first}^")]), pinned);
    assert_eq!(
        git(&bare, &["diff", "--name-only", &pinned, &first]),
        "memory/claude/MEMORY.md\nmemory/claude/first.md\nmemory/codex/memory_summary.md"
    );
    assert_eq!(
        git(
            &bare,
            &["show", &format!("{first}:memory/codex/memory_summary.md")]
        ),
        "Codex lesson"
    );
    assert_eq!(
        git(&bare, &["log", "-1", "--format=%s", &first]),
        "Save agent memory"
    );
    assert_eq!(git(&personal, &["rev-parse", "HEAD"]), first);
    assert_eq!(
        std::fs::read_to_string(personal.join("notes.md")).unwrap(),
        "draft notes\n"
    );
    assert_eq!(
        git(&personal, &["diff", "--cached", "--name-only"]),
        "staged.md"
    );
    assert_eq!(origin.unauthorized(), 0, "the relay sent the VM credential");

    // Another workspace publishes memory meanwhile; this checkout's own work
    // blocks a replay, so its new memory stays committed locally.
    let other = tempfile::tempdir().unwrap();
    let clone = other.path().join("w");
    git(
        other.path(),
        &[
            "clone",
            "-q",
            "-b",
            &branch,
            bare.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    let index = clone.join("memory/claude/MEMORY.md");
    let mut lines = std::fs::read_to_string(&index).unwrap();
    lines.push_str("- [Other](other.md) — two\n");
    std::fs::write(&index, lines).unwrap();
    std::fs::write(clone.join("memory/claude/other.md"), "other lesson\n").unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-qm", "other workspace memory"]);
    git(&clone, &["push", "-q", "origin", &branch]);
    let theirs = tip();
    let mut lines = std::fs::read_to_string(memory.join("MEMORY.md")).unwrap();
    lines.push_str("- [Third](third.md) — three\n");
    std::fs::write(memory.join("MEMORY.md"), lines).unwrap();
    std::fs::write(memory.join("third.md"), "third lesson\n").unwrap();
    push_code(&d, "two.txt").await;
    eventually(Duration::from_secs(30), "memory committed locally", || {
        git(&personal, &["rev-parse", "HEAD"]) != first
    })
    .await;
    sleep(Duration::from_secs(2)).await;
    assert_eq!(
        tip(),
        theirs,
        "a replay over the owner's work was attempted"
    );
    assert_eq!(
        std::fs::read_to_string(personal.join("notes.md")).unwrap(),
        "draft notes\n"
    );
    assert_eq!(
        git(&personal, &["diff", "--cached", "--name-only"]),
        "staged.md"
    );

    // Once the owner's work is gone, the next code push replays and publishes,
    // keeping both workspaces' index lines.
    git(&personal, &["restore", "--staged", "staged.md"]);
    std::fs::remove_file(personal.join("staged.md")).unwrap();
    git(&personal, &["checkout", "--", "notes.md"]);
    push_code(&d, "three.txt").await;
    eventually(Duration::from_secs(30), "replayed memory published", || {
        tip() != theirs
    })
    .await;
    let merged = tip();
    assert_eq!(git(&bare, &["rev-parse", &format!("{merged}^")]), theirs);
    assert_eq!(
        git(
            &bare,
            &["show", &format!("{merged}:memory/claude/MEMORY.md")]
        ),
        "- [First](first.md) — one\n- [Other](other.md) — two\n- [Third](third.md) — three"
    );
    for topic in ["first.md", "other.md", "third.md"] {
        assert!(
            try_git(
                &bare,
                &["cat-file", "-e", &format!("{merged}:memory/claude/{topic}")]
            )
            .is_some(),
            "{topic}"
        );
    }
    assert_eq!(git(&personal, &["rev-parse", "HEAD"]), merged);
    assert!(git(&personal, &["status", "--porcelain"]).is_empty());

    // Unpublished commits outside memory are the owner's to publish.
    std::fs::write(personal.join("plan.md"), "plan\n").unwrap();
    git(&personal, &["add", "plan.md"]);
    git(&personal, &["commit", "-qm", "owner plan"]);
    std::fs::write(memory.join("fourth.md"), "fourth lesson\n").unwrap();
    let planned = git(&personal, &["rev-parse", "HEAD"]);
    push_code(&d, "four.txt").await;
    eventually(Duration::from_secs(30), "memory committed locally", || {
        git(&personal, &["rev-parse", "HEAD"]) != planned
    })
    .await;
    sleep(Duration::from_secs(2)).await;
    assert_eq!(tip(), merged, "the owner's unpublished commit was pushed");
}

/// Without a personal repository, Codex runs with memory off and explains why.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn personal_memory_codex_turn_reports_a_missing_personal_repository() {
    let providers = tempfile::tempdir().unwrap();
    let codex = codex_app_server_fixture(
        providers.path(),
        "codex-silent.sh",
        "",
        json!({"items":[{"id":"message-1","type":"agentMessage","text":"done"}]}),
    );
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            git_origin: true,
            ..Default::default()
        },
    )
    .await;
    stage_origin(
        &d.repo_root(),
        &d.repos[0].1,
        &d.origin.as_ref().unwrap().bare(d.repo),
    );
    let mut rpc = d.connect().await;
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"codex","questions":"disabled","tool_approval":"auto_deny","sandbox":"read-only","allowed_tools":[],"limits":{"setup_timeout_ms":20_000,"max_output_bytes":1048576}});
    let task = Uuid::new_v4();
    let exec = d.task_exec_id(task, 1);
    let params = agent_turn_invoke(
        &d,
        task,
        1,
        "codex",
        "human",
        "hello",
        policy,
        "G5_CODEX_PATH",
        &codex,
    );
    result(rpc.call("modules.invoke", params).await);
    let outcome = timeout(Duration::from_secs(30), rpc.wait_outcome(&exec))
        .await
        .expect("the turn settles");
    assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
    let replay = result(
        rpc.call(
            "events.replay",
            d.params(true, json!({"exec_id":exec,"from_seq":1})),
        )
        .await,
    );
    let events = replay["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["kind"] == "module.message.final"),
        "{replay}"
    );
    assert!(
        events
            .iter()
            .any(|event| event["payload"]["code"] == "shared_memory_skipped"),
        "{replay}"
    );
}

/// Opt-in native CLI E2E: modules.invoke -> Claude Write into the mounted
/// personal repository (outside the launch directory) -> a code push -> memory
/// sync -> a fresh dispatcher's personal checkout -> native memory recall.
/// Only the model and control-plane Git service are fixtures; native tools,
/// adapter admission, transport, sync and new-home discovery execute normally.
/// Point G5_NATIVE_CLAUDE_TEST_PATH to the CLI executable whose version matches
/// scripts/claude-code-cli-version.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the pinned Claude CLI; set G5_NATIVE_CLAUDE_TEST_PATH and TMPDIR=/tmp"]
async fn personal_memory_native_save_sync_and_fresh_session_recall() {
    let claude = std::path::PathBuf::from(
        std::env::var("G5_NATIVE_CLAUDE_TEST_PATH").expect("set pinned Claude CLI path"),
    );
    let version = std::process::Command::new(&claude)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    let pin = include_str!("../../../scripts/claude-code-cli-version").trim();
    assert!(String::from_utf8_lossy(&version.stdout).starts_with(&format!("{pin} ")));
    let options = || DispatcherOptions {
        git_origin: true,
        env: inherited_loader_env(),
        lease_ttl_secs: 300,
        references: vec![personal_mount_spec()],
        ..Default::default()
    };
    // Native launches resolve the staged runtime binary; test-path overrides
    // are intentionally unavailable outside fake-provider mode.
    let stage_cli = |d: &TestDispatcher| {
        let runtime = d
            .catalog_path()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("bin");
        std::fs::create_dir_all(&runtime).unwrap();
        std::os::unix::fs::symlink(&claude, runtime.join("claude")).unwrap();
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
    };
    let source = TestDispatcher::spawn(bin(), options()).await;
    stage_cli(&source);
    let layout: Value = serde_json::from_slice(
        &std::fs::read(source.workspace_root().join(".g5/layout.json")).unwrap(),
    )
    .unwrap();
    git(
        &source.repo_root(),
        &[
            "remote",
            "set-url",
            "origin",
            layout["repos"][0]["git_url"].as_str().unwrap(),
        ],
    );
    let (personal_id, spec) = source.references[0].clone();
    let personal = source.workspace_root().join(&spec.root_path);
    let bare = source.origin.as_ref().unwrap().bare(personal_id);
    std::fs::write(personal.join("notes.md"), "notes\n").unwrap();
    git(&personal, &["add", "-A"]);
    git(&personal, &["commit", "-qm", "pinned"]);
    git(
        &personal,
        &["push", "-q", bare.to_str().unwrap(), &spec.branch],
    );
    let pinned = git(&personal, &["rev-parse", "HEAD"]);
    let memory = personal.canonicalize().unwrap().join("memory/claude");
    let label = "Native-dispatch-memory-4d02";
    let model = MockServer::start().await;
    mount_native_memory_model(&model, memory.clone(), label, false).await;
    let mut rpc = source.connect().await;
    let saved = invoke_native_memory_turn(&source, &mut rpc, &claude, &model.uri(), false).await;
    assert_eq!(saved["result"]["status"], "succeeded", "{saved}");
    assert_eq!(
        std::fs::read_to_string(memory.join("MEMORY.md")).unwrap(),
        label
    );
    assert_eq!(
        model
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/v1/messages")
            .count(),
        2
    );
    assert!(!source.repo_root().join(".g5/memory").exists());
    push_code(&source, "after-save.txt").await;
    let tip = || git(&bare, &["rev-parse", &spec.branch]);
    eventually(Duration::from_secs(60), "memory published", || {
        tip() != pinned
    })
    .await;
    let published = tip();
    assert_eq!(
        git(
            &bare,
            &["show", &format!("{published}:memory/claude/MEMORY.md")]
        ),
        label
    );
    assert!(!git(&bare, &["ls-tree", "-r", "--name-only", &published]).contains(".claude/"));

    // A later Start checks the personal branch out at its published head.
    let fresh = TestDispatcher::spawn(bin(), options()).await;
    stage_cli(&fresh);
    let restored = fresh.workspace_root().join("personal");
    git(
        &restored,
        &["fetch", "-q", bare.to_str().unwrap(), &spec.branch],
    );
    git(&restored, &["reset", "-q", "--hard", "FETCH_HEAD"]);
    let fresh_memory = restored.canonicalize().unwrap().join("memory/claude");
    assert!(!fresh.root.path().join("home/agent/.claude").exists());
    model.reset().await;
    mount_native_memory_model(&model, fresh_memory.clone(), label, true).await;
    let mut fresh_rpc = fresh.connect().await;
    let recalled =
        invoke_native_memory_turn(&fresh, &mut fresh_rpc, &claude, &model.uri(), true).await;
    assert_eq!(recalled["result"]["status"], "succeeded", "{recalled}");
    let requests = model.received_requests().await.unwrap();
    let calls: Vec<_> = requests
        .iter()
        .filter(|r| r.url.path() == "/v1/messages")
        .collect();
    assert_eq!(calls.len(), 1);
    let body: Value = calls[0].body_json().unwrap();
    assert!(
        body.to_string().contains(label),
        "native CLI failed to inject the restored memory"
    );
    assert_eq!(
        std::fs::read_to_string(fresh_memory.join("MEMORY.md")).unwrap(),
        label
    );
}

#[cfg(unix)]
async fn invoke_native_memory_turn(
    d: &TestDispatcher,
    rpc: &mut g5d::dispatch_test_support::Rpc,
    claude: &Path,
    endpoint: &str,
    recall: bool,
) -> Value {
    let policy = json!({"schema_version":"g5.agent.permission-policy/v1","provider":"claude_code","questions":"disabled","tool_approval":"auto_deny","sandbox":"none","allowed_tools":if recall { json!(["Read"]) } else { json!(["Read","Write"]) },"limits":{"setup_timeout_ms":60000,"max_output_bytes":1048576}});
    let task = Uuid::new_v4();
    let mut params = agent_turn_invoke(
        d,
        task,
        1,
        "claude_code",
        "human",
        if recall {
            "Recall the saved project label."
        } else {
            "Save the supplied project label using native memory."
        },
        policy,
        "G5_CLAUDE_PATH",
        claude,
    );
    params["input"]["provider"]["options"] = json!({});
    params["input"]["provider"]["auth_mode"] = json!("anthropic_api_key");
    params["input"]["provider"]["base_url"] = json!(endpoint);
    params["input"]["provider"]["model_id"] = json!("claude-sonnet-4-6");
    params["input"]["limits"]["setup_timeout_ms"] = json!(60000);
    params["input"]["workspace_target"] = json!({"repo_id":d.repo,"relative_path":"code"});
    params["secrets"]["G5_HARNESS_DIR"] = json!(harness_sources());
    params["secrets"]["ANTHROPIC_BASE_URL"] = json!(endpoint);
    params["secrets"]["ANTHROPIC_API_KEY"] = json!("native-memory-fixture");
    params["secrets"]["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"] = json!("1");
    result(rpc.call("modules.invoke", params).await);
    let outcome = rpc.wait_outcome(&d.task_exec_id(task, 1)).await;
    if outcome["result"]["status"] != "succeeded" {
        let logs = result(
            rpc.call(
                "modules.logs",
                d.params(false, json!({"exec_id":d.task_exec_id(task, 1)})),
            )
            .await,
        );
        panic!("native memory turn failed: {outcome}; logs: {logs}");
    }
    outcome
}

#[cfg(unix)]
async fn mount_native_memory_model(
    server: &MockServer,
    memory: std::path::PathBuf,
    label: &'static str,
    recall: bool,
) {
    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"input_tokens":100})))
        .mount(server)
        .await;
    Mock::given(method("POST")).and(path("/v1/messages"))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = request.body_json().unwrap();
            assert!(body["system"].to_string().contains(memory.to_str().unwrap()));
            let result_seen = body["messages"].as_array().unwrap().iter().any(|message| {
                message["content"].as_array().is_some_and(|parts| parts.iter().any(|part| part["type"] == "tool_result"))
            });
            let block = if recall || result_seen {
                if recall { assert!(body.to_string().contains(label)); }
                else { assert_eq!(std::fs::read_to_string(memory.join("MEMORY.md")).unwrap(), label); }
                json!({"type":"text","text":label})
            } else {
                json!({"type":"tool_use","id":"toolu_memory","name":"Write","input":{"file_path":memory.join("MEMORY.md"),"content":label}})
            };
            let tool = block["type"] == "tool_use";
            let message = json!({"id":"msg_memory","type":"message","role":"assistant","model":body["model"],"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":100,"output_tokens":20}});
            let start = if tool { json!({"type":"tool_use","id":"toolu_memory","name":"Write","input":{}}) } else { json!({"type":"text","text":""}) };
            let delta = if tool { json!({"type":"input_json_delta","partial_json":block["input"].to_string()}) } else { json!({"type":"text_delta","text":label}) };
            let events = [
                json!({"type":"message_start","message":message}),
                json!({"type":"content_block_start","index":0,"content_block":start}),
                json!({"type":"content_block_delta","index":0,"delta":delta}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":if tool {"tool_use"} else {"end_turn"},"stop_sequence":null},"usage":{"output_tokens":20}}),
                json!({"type":"message_stop"}),
            ];
            let stream = events.iter().map(|event| format!("event: {}\ndata: {event}\n\n", event["type"].as_str().unwrap())).collect::<String>();
            ResponseTemplate::new(200).set_body_raw(stream, "text/event-stream")
        }).mount(server).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changes_missing_head_refuses_update_and_releases_admission() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            git_origin: true,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    let bare = d.origin.as_ref().unwrap().bare(d.repo);
    stage_origin(&root, &d.repos[0].1, &bare);
    git(&root, &["rm", "-rf", "."]);
    git(&root, &["commit", "-qm", "empty tree"]);
    let head = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["update-ref", "refs/remotes/origin/shared", &head]);
    git(&root, &["update-ref", "-d", "refs/heads/shared"]);
    assert!(git(&root, &["status", "--porcelain"]).is_empty());
    let mut rpc = d.connect().await;
    let update = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &update, "workspace.changes", "update", json!({})),
        )
        .await,
    );
    let refused = rpc.wait_outcome(&update).await;
    assert_eq!(refused["status"], "failed", "{refused}");
    assert_eq!(
        refused["result"]["members"][0]["errors"],
        json!(["HEAD_UNAVAILABLE"])
    );
    // The failed invocation must settle its mutation admission and Git gate.
    git(&root, &["update-ref", "refs/heads/shared", &head]);
    std::fs::write(root.join("after.txt"), "after refusal\n").unwrap();
    let commit = d.operation_exec_id(Uuid::new_v4());
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &commit,
                "workspace.changes",
                "commit",
                json!({"message":"after refusal"}),
            ),
        )
        .await,
    );
    let saved = rpc.wait_outcome(&commit).await;
    assert_eq!(saved["status"], "succeeded", "{saved}");
    assert_eq!(git(&root, &["show", "HEAD:after.txt"]), "after refusal");
    result(rpc.call("admission.close", d.params(true, json!({}))).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bootstrap_runs_setup_without_exposing_credentials_or_accepting_execution_graphs() {
    let d = TestDispatcher::spawn(
        bin(),
        DispatcherOptions {
            env: inherited_loader_env(),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let root = d.repo_root();
    std::fs::write(root.join("g5.yaml"), "name: fixture\nsetup:\n  bootstrap:\n    - \"printf step-ran > step.txt\"\n    - \"env > env.txt\"\n  env:\n    STEP_MARKER: marker-value\n").unwrap();
    let mut rpc = d.connect().await;
    let exec = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &exec, "workspace.bootstrap", "run", json!({})),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&exec).await;
    assert_eq!(outcome["status"], "succeeded", "{outcome}\n{}", d.stderr());
    assert_eq!(
        outcome["result"]["members"][0]["steps"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        std::fs::read_to_string(root.join("step.txt")).unwrap(),
        "step-ran"
    );
    let env = std::fs::read_to_string(root.join("env.txt")).unwrap();
    assert!(env.contains("STEP_MARKER=marker-value"));
    assert!(!env.contains(&d.workspace_token));
    let journal = result(
        rpc.call("events.replay", d.params(true, json!({"exec_id":exec})))
            .await,
    );
    assert!(!journal.to_string().contains(&d.workspace_token));
    assert!(!journal.to_string().contains("module.bootstrap.graph"));
    std::fs::write(root.join("g5.yaml"),"name: fixture\nsetup:\n  bootstrap:\n    - \"exit 3\"\n    - \"printf never > never.txt\"\n").unwrap();
    let failing = d.task_exec_id(Uuid::new_v4(), 1);
    result(
        rpc.call(
            "modules.invoke",
            builtin_invoke(&d, &failing, "workspace.bootstrap", "run", json!({})),
        )
        .await,
    );
    let outcome = rpc.wait_outcome(&failing).await;
    assert_eq!(outcome["status"], "failed", "{outcome}");
    assert_eq!(outcome["result"]["reason"], "STEP_FAILED");
    assert!(!root.join("never.txt").exists());
    let retired = d.task_exec_id(Uuid::new_v4(), 1);
    error(
        rpc.call(
            "modules.invoke",
            builtin_invoke(
                &d,
                &retired,
                "workspace.bootstrap",
                "run",
                json!({"graph":{"tasks":[]}}),
            ),
        )
        .await,
        "INVALID_INPUT",
    );
}
