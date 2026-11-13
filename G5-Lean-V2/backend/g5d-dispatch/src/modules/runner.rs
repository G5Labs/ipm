//! The private process runner. Reached only after catalog resolution, input
//! validation and grant intersection; never addressable by a caller.
//!
//! Platform built-ins that are pure projections (`workspace.changes`,
//! `workspace.diagnostics`) run in-process under the same wall clock and
//! cancellation. `workspace.bootstrap` and every non-builtin module run as a
//! child process of the pinned entrypoint in its own process group with a
//! scrubbed environment, bounded stdout/stderr, hard resource limits and —
//! for bounded runs — one wall-clock deadline that covers the invocation
//! record, the run and the final pipe drain. The child's stdin is the
//! execution's private control channel: the invocation record first, then
//! any delivered native replies. Repository descendants never inherit it, and
//! nothing the module left behind in its process group outlives settlement.
//! Linux agent turns additionally use a private child-subreaper and require its
//! receipt before settlement: every descendant reaped, including escaped
//! groups/sessions, or released after the provider ended the turn itself.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::redact::{Continuation, Redactor, Secrets, MASK};
use super::{
    builtin, capability, ControlMessage, ExecId, ModuleEntry, ModuleRuntime, DELIVERY_ACK_TIMEOUT,
};
use crate::config::DispatchConfig;
use crate::journal::Disposition;

/// The common environment every module process receives. Incarnation
/// credentials are added separately only for the shipped modules whose MVP
/// contracts require direct control-plane access.
pub const MODULE_ENV_EXEC_ID: &str = "G5D_EXEC_ID";
pub const MODULE_ENV_MODULE_ID: &str = "G5D_MODULE_ID";
pub const MODULE_ENV_MODULE_VERSION: &str = "G5D_MODULE_VERSION";
pub const MODULE_ENV_OPERATION: &str = "G5D_MODULE_OPERATION";
pub const MODULE_ENV_WORKSPACE_ROOT: &str = "G5D_WORKSPACE_ROOT";
/// agent.turn only: the names of the environment variables that carry
/// secrets (the dispatch secrets and the workspace token), so the module can
/// mask their values in the inline tool copies it journals before flattening
/// or bounding them (g5d-agent runtime.rs `module_secrets`). The
/// dispatcher still redacts every emitted line; this closes the cases exact
/// matching cannot see (escaped or cut values).
pub const MODULE_ENV_SECRET_NAMES: &str = "G5D_SECRET_ENV_NAMES";
/// Names the descriptor a preview service module receives for its
/// application's terminal (`g5.preview.output.live-tail`); `preview-serve`
/// gives the application that descriptor as its stdin, stdout and
/// controlling terminal.
pub const APP_TTY_FD_ENV: &str = "G5_APP_TTY_FD";
/// The fixed descriptor number the hosted PTY slave is passed on.
pub const APP_TTY_FD: i32 = 3;
/// Retained application output per preview session.
pub const PREVIEW_OUTPUT_RETAIN_BYTES_ENV: &str = "G5D_PREVIEW_OUTPUT_RETAIN_BYTES";
const DEFAULT_PREVIEW_OUTPUT_RETAIN_BYTES: usize = 4 * 1024 * 1024;

/// The hosted session capacity for preview output: 4 MiB unless the guest
/// environment sets another value between 64 KiB and 64 MiB.
fn preview_output_capacity() -> usize {
    std::env::var(PREVIEW_OUTPUT_RETAIN_BYTES_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|bytes| (64 * 1024..=64 * 1024 * 1024).contains(bytes))
        .unwrap_or(DEFAULT_PREVIEW_OUTPUT_RETAIN_BYTES)
}

const AGENT_TURN_FAKE_PROVIDER_ENV: &[&str] = &[
    "G5_CLAUDE_CODE_FAKE",
    "G5_CODEX_FAKE",
    "G5_FAKE_SCENARIO",
    "G5_FAKE_CLAUDE_SCENARIO",
    "G5_FAKE_CODEX_SCENARIO",
    "G5_FAKE_STRICT_RESUME",
    "G5_FAKE_BUDGET_RESUME_E2E",
];

/// After the module exited and its group was stopped, how long the runner
/// waits for the pipes to drain before settling with what it has.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Close grace for `workspace.preview`: SIGTERM first so `preview-serve` can
/// stop the app and run the profile's cleanup command, SIGKILL after. Longer
/// than the guest's own app-stop and cleanup bounds (10 s + 60 s) so an
/// overrun is reported by the guest rather than cut off here.
const PREVIEW_CLOSE_GRACE: Duration = Duration::from_secs(90);
/// Longest single stdout line a module may emit as one event.
const MAX_LINE_BYTES: usize = crate::journal::MAX_EVENT_BYTES;
/// How long one structured stdout event is held for boundary redaction when
/// no further event follows it.
const EVENT_HOLD: Duration = Duration::from_millis(25);
const AGENT_SETTLEMENT_GRACE: Duration = Duration::from_secs(12);

/// The environment `g5d preview-serve` reads: the selected repository's
/// directory, the named profile, and the isolated origin the browser reaches
/// the preview from (`g5.workspace.preview-origin-security`), which an API
/// without a preview host suffix does not send.
fn preview_env(
    config: &DispatchConfig,
    input: &Value,
) -> Result<Vec<(&'static str, std::ffi::OsString)>, &'static str> {
    let repo_id = input["repo_id"].as_str().ok_or("invalid_repo_id")?;
    let repo = config
        .layout
        .repos
        .iter()
        .find(|repo| repo.repo_id.to_string() == repo_id)
        .ok_or("repo_not_in_workspace")?;
    let mut env = vec![(
        "G5D_PREVIEW_WORK_DIR",
        config
            .paths
            .workspace_root
            .join(&repo.root_path)
            .into_os_string(),
    )];
    if let Some(profile) = input["profile"].as_str() {
        env.push(("G5D_PREVIEW_PROFILE", profile.into()));
    }
    if !input["origin"].is_null() {
        let origin = input["origin"]
            .as_str()
            .filter(|origin| builtin::is_preview_origin(origin))
            .ok_or("invalid_preview_origin")?;
        env.push(("G5D_PREVIEW_ORIGIN", origin.into()));
    }
    Ok(env)
}

pub fn outcome(status: &str, detail: Value) -> Value {
    json!({"status":status,"detail":detail})
}

#[derive(Clone, Copy)]
struct RunLimits {
    started: Instant,
    /// Bounds the dispatcher's own setup: Git refresh and the invocation
    /// record write.
    setup: Duration,
    /// The settlement deadline of the whole run. `Duration::MAX` for an
    /// agent turn: its startup limit
    /// covers only setup and input delivery, which the supervisor enforces.
    wall: Duration,
}

fn effective_run_limits(module: &ModuleEntry, input: &Value) -> Result<RunLimits, Value> {
    let mut wall = wall_clock(module);
    let mut attempt_wide = true;
    if module.module_id == "agent.turn" {
        let request = &input["limits"];
        let policy = &input["provider"]["policy_snapshot"]["limits"];
        let read_limit = |source: &Value, name: &str| {
            source[name]
                .as_u64()
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    outcome(
                        "failed",
                        json!({"reason":"invalid_agent_turn_limit","limit":name}),
                    )
                })
        };
        let request_wall = read_limit(request, "setup_timeout_ms")?;
        let policy_wall = read_limit(policy, "setup_timeout_ms")?;
        // `max_output_bytes` is retired (`g5.agents.output-bounds`): an API
        // that still sends it for workers that require it is accepted, and
        // the value is never read.
        if request_wall > policy_wall {
            return Err(outcome("failed", json!({"reason":"limit_exceeds_policy"})));
        }
        wall = wall
            .min(Duration::from_millis(request_wall))
            .min(Duration::from_millis(policy_wall));
        attempt_wide = false;
    }
    Ok(RunLimits {
        started: Instant::now(),
        setup: wall,
        wall: if attempt_wide { wall } else { Duration::MAX },
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    rt: Arc<ModuleRuntime>,
    exec_id: &ExecId,
    module: &ModuleEntry,
    operation: String,
    input: Value,
    capabilities: Vec<String>,
    dispatch_secrets: BTreeMap<String, String>,
    cancel: CancellationToken,
    control_rx: mpsc::Receiver<ControlMessage>,
) -> Value {
    let limits = match effective_run_limits(module, &input) {
        Ok(limits) => limits,
        Err(outcome) => return outcome,
    };
    if module.module_id == "agent.turn" && rt.transport().is_some() {
        let refresh_cap = Instant::now() + Duration::from_secs(120);
        let refresh_deadline = limits
            .started
            .checked_add(limits.setup)
            .map_or(refresh_cap, |deadline| deadline.min(refresh_cap));
        match builtin::refresh_provider_refs(&rt, None, &cancel, Some(refresh_deadline)).await {
            Ok(report) if report["status"] == "succeeded" => {}
            Ok(report) => {
                return outcome(
                    "failed",
                    json!({"reason":"git_inputs_unavailable","report":report}),
                )
            }
            Err(builtin::Stop::Cancelled) => return outcome("cancelled", json!({})),
            Err(builtin::Stop::TimedOut) => {
                return outcome("timed_out", json!({"reason":"git_inputs_unavailable"}))
            }
            Err(stop @ builtin::Stop::Native(_)) => return builtin::stopped(stop),
        }
    }
    if module.builtin && module.module_id != builtin::BOOTSTRAP {
        return builtin::run(rt, exec_id, module, &operation, input, cancel).await;
    }
    if limits
        .setup
        .saturating_sub(limits.started.elapsed())
        .is_zero()
    {
        return outcome(
            "timed_out",
            json!({"wall_clock_ms":limits.setup.as_millis()}),
        );
    }
    run_process(
        rt,
        exec_id,
        module,
        operation,
        input,
        capabilities,
        dispatch_secrets,
        cancel,
        control_rx,
        limits,
    )
    .await
}

/// The wall clock of a bounded run, or effectively unbounded for a service.
fn wall_clock(module: &ModuleEntry) -> Duration {
    module
        .limits
        .wall_clock_ms
        .map(Duration::from_millis)
        .unwrap_or(Duration::MAX)
}

#[allow(clippy::too_many_arguments)]
async fn run_process(
    rt: Arc<ModuleRuntime>,
    exec_id: &ExecId,
    module: &ModuleEntry,
    operation: String,
    input: Value,
    capabilities: Vec<String>,
    dispatch_secrets: BTreeMap<String, String>,
    cancel: CancellationToken,
    mut control_rx: mpsc::Receiver<ControlMessage>,
    limits: RunLimits,
) -> Value {
    let program = match resolve_entrypoint(&module.entrypoint[0], &rt.config.paths.runtime_root) {
        Ok(program) => program,
        Err(detail) => {
            return outcome(
                "failed",
                json!({"reason":"entrypoint_unresolvable","detail":detail}),
            )
        }
    };
    #[cfg(target_os = "linux")]
    let (mut command, mut execution_owner) = if module.module_id == "agent.turn" {
        // The owner keeps the dispatcher identity so it can reap descendants
        // that switch identity; it applies the module identity to the supervisor.
        let identity = if module.builtin {
            Default::default()
        } else {
            g5d_foundation::process_termination::execution_owner::SupervisorIdentity {
                uid: rt.config.module_uid,
                gid: rt.config.module_gid,
            }
        };
        match g5d_foundation::process_termination::execution_owner::ExecutionOwner::command(
            &program,
            &module.entrypoint[1..],
            identity,
        ) {
            Ok((command, owner)) => (command, Some(owner)),
            Err(error) => {
                return outcome(
                    "failed",
                    json!({"reason":"execution_owner_setup","error":error.to_string()}),
                )
            }
        }
    } else {
        let mut command = Command::new(&program);
        command.args(&module.entrypoint[1..]);
        (command, None)
    };
    #[cfg(not(target_os = "linux"))]
    let mut command = {
        let mut command = Command::new(&program);
        command.args(&module.entrypoint[1..]);
        command
    };
    command.env_clear();
    command.envs(rt.git().process_environment());
    // Where the bundle's executables and the provided harness are: the
    // mounted guest paths, or the host-native runtime directory a
    // process-mode host validated (`g5.runtime.boot.backend.process-local`).
    command.env(
        g5d_foundation::runtime_environment::RUNTIME_BIN_DIR_ENV,
        rt.config.paths.runtime_bin_dir(),
    );
    command.env("G5_HARNESS_DIR", rt.config.paths.harness_dir());
    if module.entrypoint.first().map(String::as_str) == Some("g5d") {
        // Platform-owned g5d subcommands resolve co-located runtime libraries
        // from the boot image. Customer preview commands are scrubbed again by
        // preview-serve before they are spawned.
        command.envs(g5d_foundation::runtime_environment::inherited_native_loader_paths());
    }
    command.envs(&dispatch_secrets);
    // Runtime linkage belongs to the host, not task configuration. Preserve it
    // across the scrub and apply it after task secrets so it cannot be replaced.
    // A host process links the host's own libraries; the guest bundle path
    // would be wrong there, so it is stamped only in a guest.
    if !rt.config.process_mode {
        command.envs(g5d_foundation::runtime_environment::native_loader_environment());
    }
    if module.module_id == "agent.turn" {
        // Share the startup/input-delivery clock; never set a task lifetime.
        let bound = limits.setup;
        let deadline = SystemTime::now()
            .checked_add(bound.saturating_sub(limits.started.elapsed()))
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|since_epoch| since_epoch.as_millis());
        if let Some(deadline) = deadline {
            command.env("G5D_SETUP_DEADLINE_UNIX_MS", deadline.to_string());
        }
    }
    command
        .env(MODULE_ENV_EXEC_ID, exec_id.as_str())
        .env(MODULE_ENV_MODULE_ID, &module.module_id)
        .env(MODULE_ENV_MODULE_VERSION, &module.module_version)
        .env(MODULE_ENV_OPERATION, &operation)
        .env(MODULE_ENV_WORKSPACE_ROOT, &rt.config.paths.workspace_root);

    // The MVP contracts give the agent-turn supervisor platform access for
    // native session-file restore/harvest.
    // Other catalog modules, including preview and repository-facing modules,
    // do not inherit either control-plane endpoint or credential.
    if module.module_id == "agent.turn" {
        // Only the incarnation's member vector supplies repository ownership.
        // Set this after task secrets so callers cannot replace that context.
        // `git_url` is the canonical origin the ontology changes card checks
        // before refetching a turn's start commit (`g5.chat.ontology-card`).
        let repositories: Vec<Value> = rt.config.layout.repos.iter().map(|repo| {
            json!({"repo_id": repo.repo_id, "root_path": repo.root_path, "role": repo.role, "git_url": repo.git_url})
        }).collect();
        command.env(
            "G5D_AGENT_REPOSITORIES",
            serde_json::to_string(&repositories).unwrap(),
        );
        // Agent memory lives only in the owner's personal repository
        // (`g5.personal.memory`); without it, memory is off for the launch.
        match rt.config.layout.personal_repository() {
            Some(personal) => command.env("G5D_AGENT_PERSONAL_ROOT", personal.root_path),
            None => command.env_remove("G5D_AGENT_PERSONAL_ROOT"),
        };
        let secret_names: Vec<&str> = dispatch_secrets.keys().map(String::as_str).collect();
        command
            .envs(
                g5d_foundation::runtime_environment::workspace_cli_environment(
                    rt.config.api_url.as_str(),
                    &rt.config.workspace_id.to_string(),
                    &rt.config.workspace_token_file().to_string_lossy(),
                ),
            )
            .env(MODULE_ENV_SECRET_NAMES, secret_names.join(","));
        for key in AGENT_TURN_FAKE_PROVIDER_ENV {
            if let Ok(value) = std::env::var(key) {
                command.env(key, value);
            }
        }
    }
    if module.module_id == builtin::PREVIEW {
        match preview_env(&rt.config, &input) {
            Ok(env) => command.envs(env),
            Err(reason) => return outcome("failed", json!({"reason": reason})),
        };
    }
    command.current_dir(&rt.config.paths.workspace_root);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // The owner must survive a dropped runner: socket EOF tells it to reap
    // the tree. kill_on_drop would destroy that authority before cleanup.
    #[cfg(target_os = "linux")]
    if execution_owner.is_some() {
        command.kill_on_drop(false);
    }
    #[cfg(unix)]
    {
        // Every descendant that does not escape the group is stopped with
        // the module. The host-provisioned unprivileged identity applies to
        // repository-facing (non-builtin) modules; the platform-shipped
        // bootstrap supervisor and an agent turn's execution owner keep the
        // dispatcher identity and apply that identity to what they launch.
        command.process_group(0);
        #[cfg(target_os = "linux")]
        let owner_applies_identity = execution_owner.is_some();
        #[cfg(not(target_os = "linux"))]
        let owner_applies_identity = false;
        if !module.builtin && !owner_applies_identity {
            if let Some(uid) = rt.config.module_uid {
                command.uid(uid);
            }
            if let Some(gid) = rt.config.module_gid {
                command.gid(gid);
            }
        }
    }
    if module.builtin {
        if let Some(uid) = rt.config.module_uid {
            command.env("G5D_MODULE_UID", uid.to_string());
        }
        if let Some(gid) = rt.config.module_gid {
            command.env("G5D_MODULE_GID", gid.to_string());
        }
    }
    apply_limits(&mut command, module);

    // A module that declares the preview service gets a hosted terminal for
    // its application's output (`g5.preview.output.live-tail`): the
    // dispatcher keeps the master, the module receives the slave as fd 3.
    // Without a terminal host (unit fixtures) or if the PTY cannot be opened,
    // the module spawns as before and its service keeps today's behavior.
    let hosted_tty = if module
        .requested_capabilities
        .iter()
        .any(|capability| capability == capability::PREVIEW_SERVICE)
    {
        match rt.terminal_host() {
            Some(terminals) => {
                // One retained preview session per VM: the previous preview's
                // output (another task's name) goes when this one is hosted.
                terminals
                    .discard_hosted_where(
                        g5d_foundation::terminal::TerminalCloseReason::HostReplaced,
                        |name| name.starts_with(g5_contracts::terminal::HOSTED_TERMINAL_PREFIX),
                    )
                    .await;
                match terminals
                    .host_session(
                        &g5_contracts::terminal::preview_terminal_id(exec_id.entity_id()),
                        preview_output_capacity(),
                        80,
                        24,
                    )
                    .await
                {
                    Ok(hosted) => Some(hosted),
                    Err(error) => {
                        tracing::warn!(exec_id = exec_id.as_str(), %error, "preview output terminal unavailable; the application runs without a TTY");
                        None
                    }
                }
            }
            None => None,
        }
    } else {
        None
    };
    #[cfg(unix)]
    if let Some(hosted) = hosted_tty.as_ref() {
        use std::os::fd::AsRawFd as _;
        let slave = hosted.slave.as_raw_fd();
        command.env(APP_TTY_FD_ENV, APP_TTY_FD.to_string());
        // SAFETY: the closure only duplicates one inherited descriptor onto a
        // fixed number and touches no allocator or lock.
        unsafe {
            command.pre_exec(move || {
                if slave == APP_TTY_FD {
                    // Keep it across exec: the dispatcher's copy is close-on-exec.
                    if libc::fcntl(APP_TTY_FD, libc::F_SETFD, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                } else if libc::dup2(slave, APP_TTY_FD) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let remaining = move || limits.setup.saturating_sub(limits.started.elapsed());
    let settlement_remaining = move || {
        let grace = if module.module_id == "agent.turn" {
            AGENT_SETTLEMENT_GRACE
        } else {
            Duration::ZERO
        };
        limits
            .wall
            .saturating_add(grace)
            .saturating_sub(limits.started.elapsed())
    };

    let spawned = command.spawn();
    // The module now holds the slave (or never will): the runner's copy must
    // go so the hosted session reads EOF once the module and its application
    // have closed theirs.
    drop(hosted_tty);
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            return outcome(
                "failed",
                json!({"reason":"spawn","error":error.to_string()}),
            );
        }
    };
    #[cfg(target_os = "linux")]
    if let Some(owner) = execution_owner.as_mut() {
        owner.spawned();
    }
    let pid = child.id();
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    // Invocation record: the module reads exactly this shape. The write is
    // already under the deadline: a module that never reads its stdin times
    // out. Bootstrap graph authority is the dispatcher's incarnation token,
    // which is revalidated by the API against the running command attempt.
    let record = json!({
        "exec_id": exec_id.as_str(),
        "module_id": module.module_id,
        "module_version": module.module_version,
        "operation": operation,
        "input": input,
        "capabilities": capabilities,
    });
    let (handed, write_started) =
        write_initial_record(&mut stdin, &record, remaining(), &cancel).await;
    if !handed {
        #[cfg(target_os = "linux")]
        if let Some(owner) = execution_owner.as_mut() {
            owner.stop_and_reap(&mut child).await;
        } else {
            stop_group(ModuleStopMode::Kill, pid, &mut child).await;
        }
        #[cfg(not(target_os = "linux"))]
        stop_group(ModuleStopMode::Kill, pid, &mut child).await;
        // A loader/entrypoint failure can close stdin before the normal output
        // readers start. Preserve its bounded, redacted stderr for diagnostics.
        let startup_log =
            read_startup_stderr(stderr, module.limits.max_log_bytes, rt.secrets()).await;
        rt.append_log(exec_id, &startup_log);
        let delivery_detail = json!({
            "input_delivery_unknown": module.module_id == "agent.turn" && write_started,
        });
        return if cancel.is_cancelled() {
            outcome("cancelled", delivery_detail)
        } else if remaining().is_zero() {
            outcome(
                "timed_out",
                json!({"wall_clock_ms":limits.setup.as_millis(),
                    "input_delivery_unknown":module.module_id == "agent.turn" && write_started}),
            )
        } else {
            outcome(
                "failed",
                json!({"reason":"control_channel_closed_at_start",
                    "input_delivery_unknown":module.module_id == "agent.turn" && write_started,
                    "exit_code":child.try_wait().ok().flatten().and_then(|status| status.code())}),
            )
        };
    }

    // The catalog bound counts the module's stdout bytes. `agent.turn`
    // declares none (`g5.agents.output-bounds`); each of its events is still
    // bounded like any module's.
    let max_output = module.limits.max_output_bytes;
    let max_log = module.limits.max_log_bytes;
    let (limit_tx, mut limit_rx) = mpsc::channel::<&'static str>(2);
    let (result_tx, mut result_rx) = mpsc::channel::<Value>(4);

    // stdout: JSON lines become journaled events; anything else is output.
    // Bytes are counted as they arrive, so a newline-free flood is bounded
    // exactly like a chatty one.
    let stdout_task = tokio::spawn(read_module_stdout(
        stdout,
        rt.clone(),
        exec_id.clone(),
        max_output,
        limit_tx.clone(),
        result_tx,
    ));

    // stderr: bounded, redacted logs.
    let stderr_task = tokio::spawn({
        let rt = rt.clone();
        let exec = exec_id.clone();
        async move {
            let mut reader = stderr;
            // Version first, then the snapshot: a rotation between the two
            // shows up as a change on the next chunk instead of being missed.
            let mut secrets_version = rt.secrets_version();
            let mut redactor = Redactor::new(rt.secrets());
            let mut buffer = vec![0u8; 8192];
            let mut seen: u64 = 0;
            loop {
                let read = match reader.read(&mut buffer).await {
                    Ok(0) => break,
                    // A read failure is not EOF: the held suffix may still be
                    // the beginning of a secret. Drop it instead of flushing.
                    Err(_) => return None,
                    Ok(read) => read,
                };
                seen += read as u64;
                if seen > max_log {
                    let _ = limit_tx.send("log").await;
                    return Some("log");
                }
                // A token rotated while the module runs is masked from the
                // next chunk on, including across the held boundary.
                let version = rt.secrets_version();
                if version != secrets_version {
                    redactor.set_secrets(rt.secrets());
                    secrets_version = version;
                }
                let safe = redactor.push(&buffer[..read]);
                rt.append_log(&exec, &safe);
            }
            rt.append_log(&exec, &redactor.finish());
            None
        }
    });

    // Control channel forwarder: a delivered reply is acknowledged only once
    // its bytes were written and flushed to the module's stdin.
    let control_task = tokio::spawn(async move {
        while let Some(message) = control_rx.recv().await {
            match message {
                ControlMessage::Steer { line, ack } => {
                    let written = matches!(
                        tokio::time::timeout(DELIVERY_ACK_TIMEOUT, write_line(&mut stdin, &line))
                            .await,
                        Ok(Ok(()))
                    );
                    let _ = ack.send(written);
                    if !written {
                        break;
                    }
                }
                ControlMessage::Respond {
                    request_id,
                    provider_request_id,
                    response_digest,
                    validated_response,
                    ack,
                } => {
                    let line = json!({
                        "kind":"input.respond",
                        "request_id":request_id,
                        "provider_request_id":provider_request_id,
                        "response_digest":response_digest,
                        "validated_response":validated_response,
                    });
                    // A write the module does not take within the delivery
                    // bound is abandoned and the channel closed, so a
                    // negative disposition is never followed by a late (or
                    // partial) frame reaching the module.
                    let written = matches!(
                        tokio::time::timeout(DELIVERY_ACK_TIMEOUT, write_line(&mut stdin, &line))
                            .await,
                        Ok(Ok(()))
                    );
                    let _ = ack.send(written);
                    if !written {
                        break;
                    }
                }
                ControlMessage::EndBackgroundWait { request_id, ack } => {
                    let line = json!({"kind":"background.end","request_id":request_id});
                    let written = matches!(
                        tokio::time::timeout(DELIVERY_ACK_TIMEOUT, write_line(&mut stdin, &line))
                            .await,
                        Ok(Ok(()))
                    );
                    let _ = ack.send(written);
                    if !written {
                        break;
                    }
                }
            }
        }
        // Dropping stdin closes the module's control channel.
        drop(stdin);
    });

    let mut module_result = None;
    let mut result_channel_open = true;
    let waited = loop {
        tokio::select! {
            status = child.wait() => break Ok(status),
            result = result_rx.recv(), if module_result.is_none() && result_channel_open => {
                if let Some(result) = result {
                    module_result = Some(result);
                    // An outcome is the module protocol's last frame. Closing
                    // stdin here prevents a pending control-channel read from
                    // keeping an otherwise finished module process alive.
                    control_task.abort();
                } else {
                    result_channel_open = false;
                }
            }
            _ = tokio::time::sleep(settlement_remaining()), if limits.wall != Duration::MAX => break Err("timed_out"),
            _ = cancel.cancelled() => break Err("cancelled"),
            Some(limit) = limit_rx.recv() => break Err(limit),
        }
    };
    // Agent turns require the external owner's tree-reap receipt. Other
    // modules retain group cleanup. Pipes are drained under a bound so a descriptor
    // cannot hang the outcome.
    // Only an owner's Preview Close runs the profile cleanup. Workspace stop,
    // transfer and other incarnation-lifecycle cancellations deliberately
    // use bounded no-cleanup teardown: their VM is leaving service and cleanup
    // is outside the preview-close contract.
    let cancel_reason = if matches!(waited, Err("cancelled")) {
        match rt.journal.disposition(exec_id) {
            Ok(Some(Disposition::Cancelled { reason, .. })) => Some(reason),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(exec_id = exec_id.as_str(), %error, "cancel reason unavailable; refusing graceful preview close");
                None
            }
        }
    } else {
        None
    };
    let stop_mode = module_stop_mode(
        &module.module_id,
        matches!(waited, Err("cancelled")),
        cancel_reason.as_deref(),
    );
    #[cfg(target_os = "linux")]
    let close = if let Some(owner) = execution_owner.as_mut() {
        owner.stop_and_reap(&mut child).await;
        None
    } else {
        stop_group(stop_mode, pid, &mut child).await
    };
    #[cfg(not(target_os = "linux"))]
    let close = stop_group(stop_mode, pid, &mut child).await;
    control_task.abort();
    // The reader handles stay owned here: a drain that overruns its grace is
    // aborted and joined, so no reader can append logs after settlement.
    let mut stdout_task = stdout_task;
    let mut stderr_task = stderr_task;
    let mut out_result: Option<Option<&'static str>> = None;
    let mut err_result: Option<Option<&'static str>> = None;
    let drained = tokio::time::timeout(DRAIN_GRACE, async {
        out_result = Some((&mut stdout_task).await.ok().flatten());
        err_result = Some((&mut stderr_task).await.ok().flatten());
    })
    .await;
    // Only a reader that did not finish is aborted and joined; a finished
    // handle is never polled again.
    if out_result.is_none() {
        stdout_task.abort();
        let _ = stdout_task.await;
    }
    if err_result.is_none() {
        stderr_task.abort();
        let _ = stderr_task.await;
    }
    let drained = drained.map(|_| out_result.flatten().or(err_result.flatten()));
    let limit = |which: &str| match which {
        "journal" => outcome("failed", json!({"reason":"journal_append_failed"})),
        "output" => outcome(
            "limit_exceeded",
            json!({"limit":"max_output_bytes","value":max_output}),
        ),
        // One stdout line longer than an event may be: its own bound, which
        // holds whether or not the module declares a total.
        "event" => outcome(
            "limit_exceeded",
            json!({"limit":"max_event_bytes","value":MAX_LINE_BYTES}),
        ),
        _ => outcome(
            "limit_exceeded",
            json!({"limit":"max_log_bytes","value":max_log}),
        ),
    };
    match waited {
        Ok(Ok(status)) => {
            // A module that crossed a limit and then exited on its own is
            // still over its declared bound; the exit status does not launder
            // the truncated output or logs.
            match drained {
                Ok(Some(which)) => return limit(which),
                Ok(None) => {}
                Err(_) => {
                    return outcome(
                        "failed",
                        json!({"reason":"output_drain_timeout","exit_code":status.code()}),
                    );
                }
            }
            let payload = module_result
                .or_else(|| result_rx.try_recv().ok())
                .unwrap_or(Value::Null);
            let code = status.code();
            if status.success() {
                json!({"status":"succeeded","exit_code":code,"result":payload})
            } else {
                json!({"status":"failed","exit_code":code,"result":payload})
            }
        }
        Ok(Err(error)) => outcome("failed", json!({"reason":"wait","error":error.to_string()})),
        Err("timed_out") => outcome(
            "timed_out",
            json!({
                "wall_clock_ms":limits.wall.as_millis(),
                "input_delivery_unknown":module.module_id == "agent.turn" && module_result.is_none(),
            }),
        ),
        Err("cancelled") => outcome(
            "cancelled",
            match close {
                Some(close) => json!({"close": close}),
                None => json!({}),
            },
        ),
        Err(which) => limit(which),
    }
}

/// A text leaf of a held event, kept as the module wrote it: every
/// decision — carried chains, the boundary with the next frame — is made on
/// the original bytes, and the leaf is rendered once when the event leaves,
/// with the union of every masked span: complete secrets in the original,
/// chain completions and extensions, and secrets spanning a boundary.
/// Provenance is explicit, never inferred from text, so literal mask text a
/// module emits is ordinary output.
struct LeafState {
    original: String,
    /// Masked byte spans of `original`, on character boundaries.
    spans: Vec<(usize, usize)>,
}

impl LeafState {
    fn new(original: String) -> Self {
        Self {
            original,
            spans: Vec::new(),
        }
    }

    fn mask(&mut self, start: usize, end: usize) {
        if start < end {
            self.spans.push((start, end));
        }
    }

    fn mask_head(&mut self, up_to: usize) {
        self.mask(0, up_to);
    }

    fn mask_tail_from(&mut self, from: usize) {
        self.mask(from, self.original.len());
    }

    fn render(&self, secrets: &Secrets) -> String {
        let mut spans = self.spans.clone();
        spans.extend(secrets.match_spans(&self.original));
        if spans.is_empty() {
            return self.original.clone();
        }
        spans.sort_unstable();
        let mut out = String::new();
        let mut position = 0;
        let mut current: Option<(usize, usize)> = None;
        for (start, end) in spans {
            match current.as_mut() {
                Some((_, current_end)) if start <= *current_end => {
                    *current_end = (*current_end).max(end);
                }
                _ => {
                    if let Some((start, end)) = current.take() {
                        out.push_str(&self.original[position..start]);
                        out.push_str(MASK);
                        position = end;
                    }
                    current = Some((start, end));
                }
            }
        }
        if let Some((start, end)) = current {
            out.push_str(&self.original[position..start]);
            out.push_str(MASK);
            position = end;
        }
        out.push_str(&self.original[position..]);
        out
    }
}

/// Drain structured stdout, retaining cross-frame redaction until real EOF.
async fn read_module_stdout<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    rt: Arc<ModuleRuntime>,
    exec: ExecId,
    max_output: Option<u64>,
    limit_tx: mpsc::Sender<&'static str>,
    result_tx: mpsc::Sender<Value>,
) -> Option<&'static str> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 8192];
    let mut seen: u64 = 0;
    // One structured event is held back briefly so a secret split
    // across two consecutive text fragments is masked in both before
    // either is journaled; the hold is flushed on the next event, on
    // a short idle, or at end of stream.
    let mut held: Option<HeldEvent> = None;
    let mut carry: Vec<Vec<u8>> = Vec::new();
    loop {
        let deadline = held
            .as_ref()
            .map(|event| event.deadline)
            .unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600));
        let read = tokio::select! {
            read = reader.read(&mut chunk) => read,
            _ = tokio::time::sleep_until(deadline), if held.is_some() => {
                // The hold is an absolute bound: partial reads of the
                // next line never postpone a held event.
                if let Some(event) = held.take() {
                    if !flush_held(&rt, &exec, event, &mut carry, false) {
                        let _ = limit_tx.send("journal").await;
                        return Some("journal");
                    }
                }
                continue;
            }
        };
        let read = match read {
            Ok(0) => break,
            // Broken pipes do not prove EOF. Neither the partial line nor the
            // held event may be flushed as complete: each can end in a secret.
            Err(_) => return None,
            Ok(read) => read,
        };
        seen = seen.saturating_add(read as u64);
        if max_output.is_some_and(|max| seen > max) {
            let _ = limit_tx.send("output").await;
            return Some("output");
        }
        buffer.extend_from_slice(&chunk[..read]);
        while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            if !handle_line(
                &rt,
                &exec,
                &result_tx,
                &line[..line.len() - 1],
                &mut held,
                &mut carry,
            ) {
                let _ = limit_tx.send("journal").await;
                return Some("journal");
            }
        }
        if buffer.len() > MAX_LINE_BYTES {
            let _ = limit_tx.send("event").await;
            return Some("event");
        }
    }
    if !buffer.is_empty() && !handle_line(&rt, &exec, &result_tx, &buffer, &mut held, &mut carry) {
        let _ = limit_tx.send("journal").await;
        return Some("journal");
    }
    if let Some(event) = held.take() {
        if !flush_held(&rt, &exec, event, &mut carry, true) {
            let _ = limit_tx.send("journal").await;
            return Some("journal");
        }
    }
    None
}

async fn read_startup_stderr<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    max_bytes: u64,
    secrets: Secrets,
) -> Vec<u8> {
    // Startup diagnostics need only a short prefix; an escaped pipe holder
    // must not keep the attempt alive or allocate unbounded memory.
    let mut limited = reader.take(max_bytes.min(16 * 1024));
    let mut bytes = Vec::new();
    let result = tokio::time::timeout(DRAIN_GRACE, limited.read_to_end(&mut bytes)).await;
    // Take returns Ok at its byte limit too; only a successful read with
    // budget remaining proves the underlying pipe reached EOF. On truncation,
    // timeout, or read error, discard the unresolved secret prefix instead.
    let reached_eof = matches!(result, Ok(Ok(_))) && limited.limit() > 0;
    let mut redactor = Redactor::new(secrets);
    let mut safe = redactor.push(&bytes);
    if reached_eof {
        safe.extend(redactor.finish());
    }
    safe
}

/// A structured stdout event held back for boundary redaction.
struct HeldEvent {
    kind: String,
    payload: Value,
    deadline: tokio::time::Instant,
    /// The first text leaf; the only one when the payload has a single leaf.
    first: Option<LeafState>,
    /// The last text leaf when it is distinct from the first.
    last: Option<LeafState>,
}

impl HeldEvent {
    /// The leaf that borders the next frame.
    fn tail_leaf(&mut self) -> Option<&mut LeafState> {
        self.last.as_mut().or(self.first.as_mut())
    }

    /// Render every kept leaf into the payload.
    fn render(&mut self, secrets: &Secrets) {
        if let Some(state) = &self.first {
            if let Some(leaf) = first_string_mut(boundary_payload(&self.kind, &mut self.payload)) {
                *leaf = state.render(secrets);
            }
        }
        if let Some(state) = &self.last {
            if let Some(leaf) = last_string_mut(boundary_payload(&self.kind, &mut self.payload)) {
                *leaf = state.render(secrets);
            }
        }
    }
}

/// Chat delta metadata (block/message ids) is not part of the displayed
/// text stream. Following JSON leaf order would match the previous delta
/// against an id instead of the next text fragment, leaking split secrets.
/// Whole-value redaction still covers every metadata field at emission.
fn boundary_payload<'a>(kind: &str, payload: &'a mut Value) -> &'a mut Value {
    if kind == "module.message.delta" && payload["text_delta"].is_string() {
        &mut payload["text_delta"]
    } else {
        payload
    }
}

fn emit_held(rt: &ModuleRuntime, exec: &ExecId, mut event: HeldEvent) -> bool {
    event.render(&rt.secrets());
    rt.emit(exec, &event.kind, event.payload, false).is_ok()
}

/// Emit a held event. Unless the stream ended, every unresolved secret
/// prefix at its tail is carried so later fragments can never complete it
/// in the clear.
fn flush_held(
    rt: &ModuleRuntime,
    exec: &ExecId,
    mut event: HeldEvent,
    chains: &mut Vec<Vec<u8>>,
    at_end: bool,
) -> bool {
    if !at_end {
        if let Some(state) = event.tail_leaf() {
            carry_unresolved_prefixes(&rt.secrets(), state, chains);
        }
    }
    emit_held(rt, exec, event)
}

/// Every suffix of the leaf's original text that is a proper prefix of a
/// secret is carried as its own chain (a longer suffix continuing one
/// secret says nothing about a shorter one continuing another), whatever
/// the leaf's masking already covers. Nothing here has seen a continuation:
/// the hold expired or the next frame carried no text. The longest suffix
/// that is a substantial part of a secret (`Secrets::prefix_is_substantial`:
/// four bytes, or half of a short secret) is masked; a shorter one stays as
/// emitted, which keeps typed payloads faithful and bounds what a fragment
/// boundary can expose to three bytes of a long secret, or one byte of a
/// short one. Masking every two-byte tail turned ordinary words that open
/// like a JWT ("Hey", "they", "key") into "H[REDACTED]" whenever the next
/// delta arrived after the hold (QA, 2026-09-23); a continuation that does
/// arrive is still masked in full by `resolve_chains`.
fn carry_unresolved_prefixes(secrets: &Secrets, state: &mut LeafState, chains: &mut Vec<Vec<u8>>) {
    let lens = secrets.unresolved_prefix_lens(&state.original);
    let mut mask_len = 0;
    for len in lens {
        let tail = &state.original.as_bytes()[state.original.len() - len..];
        if len >= 2 && mask_len == 0 && secrets.prefix_is_substantial(tail) {
            mask_len = len;
        }
        push_chain(chains, tail.to_vec());
    }
    if mask_len > 0 {
        state.mask_tail_from(state.original.len() - mask_len);
    }
}

/// Every chain is a distinct proper prefix of a configured secret, so the
/// set is bounded by the total length of the secrets; nothing is ever
/// evicted, because an evicted chain would let its continuation through.
fn push_chain(chains: &mut Vec<Vec<u8>>, chain: Vec<u8>) {
    if !chains.contains(&chain) {
        chains.push(chain);
    }
}

/// Resolve the carried chains against the original first text leaf of a new
/// frame: a chain the leaf completes reports the completing head, a chain
/// the leaf merely extends grows and reports the extension, a chain the leaf
/// breaks is dropped. Frames without text never reach here — they neither
/// continue nor break a secret.
fn resolve_chains(secrets: &Secrets, chains: &mut Vec<Vec<u8>>, original: &str) -> Continuation {
    let mut decided = Continuation::default();
    if chains.is_empty() {
        return decided;
    }
    let mut extended = Vec::new();
    for chain in chains.drain(..) {
        let continuation = secrets.continuations(&chain, original);
        decided.completed = decided.completed.max(continuation.completed);
        if continuation.extends {
            extended.push(chain);
        }
    }
    for chain in &mut extended {
        chain.extend_from_slice(original.as_bytes());
    }
    *chains = extended;
    decided.extends = !chains.is_empty();
    decided
}

fn handle_line(
    rt: &ModuleRuntime,
    exec: &ExecId,
    result_tx: &mpsc::Sender<Value>,
    line: &[u8],
    held: &mut Option<HeldEvent>,
    chains: &mut Vec<Vec<u8>>,
) -> bool {
    let secrets = rt.secrets();
    let text = String::from_utf8_lossy(line);
    let (kind, mut payload, is_outcome) = match serde_json::from_str::<Value>(&text) {
        Ok(value) if value.is_object() => {
            let kind = value["kind"].as_str().unwrap_or("module.event");
            let is_outcome = kind == "outcome";
            // The preview service's readiness admits its port to the HTTP
            // stream gate (see `ModuleRuntime::note_service_port`).
            if kind == "preview.ready" {
                if let Some(port) = value["payload"]["port"]
                    .as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                {
                    rt.note_service_port(exec, port);
                }
            }
            // The kind is module-supplied text as much as the payload is.
            (
                format!("module.{}", secrets.redact_str(kind)),
                value["payload"].clone(),
                is_outcome,
            )
        }
        _ => ("module.output".to_owned(), json!({"line":text}), false),
    };
    // Every decision — carried chains and the boundary with the held frame —
    // is made on original text; masking is recorded as spans and rendered
    // once when the frame leaves, so no match hides another.
    let boundary = boundary_payload(&kind, &mut payload);
    let original_first = first_string_mut(boundary).map(|first| first.clone());
    let leaves = string_leaf_count(boundary);
    let mut head_mask = 0usize;
    let mut mask_whole = false;
    if let Some(original) = original_first.as_deref() {
        let decided = resolve_chains(&secrets, chains, original);
        head_mask = decided.completed;
        mask_whole = decided.extends;
    }
    if let Some(mut previous) = held.take() {
        if let Some(state) = previous.tail_leaf() {
            match original_first.as_deref() {
                Some(original) => {
                    // Every secret spanning the pair is masked in both.
                    if let Some((prev_cut, next_cut)) =
                        secrets.find_boundary(&state.original, original)
                    {
                        state.mask_tail_from(prev_cut);
                        head_mask = head_mask.max(next_cut);
                    }
                    // Independently, every tail the next frame merely extends
                    // is carried (the longest masked when two bytes or more)
                    // and the extension masked now, so the chain can only
                    // ever resolve masked.
                    let mut longest_extended = 0;
                    for len in secrets.unresolved_prefix_lens(&state.original) {
                        let tail = &state.original.as_bytes()[state.original.len() - len..];
                        if secrets.continuations(tail, original).extends {
                            let mut chain = tail.to_vec();
                            chain.extend_from_slice(original.as_bytes());
                            push_chain(chains, chain);
                            longest_extended = longest_extended.max(len);
                        }
                    }
                    if longest_extended > 0 {
                        mask_whole = true;
                        if longest_extended >= 2 {
                            state.mask_tail_from(state.original.len() - longest_extended);
                        }
                    }
                }
                // The displacing frame has no text: the held frame leaves
                // exactly as if its hold had expired.
                None => carry_unresolved_prefixes(&secrets, state, chains),
            }
        }
        if !emit_held(rt, exec, previous) {
            return false;
        }
    }
    let first = original_first.map(|original| {
        let mut state = LeafState::new(original);
        if mask_whole {
            state.mask_head(state.original.len());
        } else if head_mask > 0 {
            state.mask_head(head_mask);
        }
        state
    });
    let last = if leaves >= 2 {
        last_string_mut(boundary_payload(&kind, &mut payload))
            .map(|last| LeafState::new(last.clone()))
    } else {
        None
    };
    let mut event = HeldEvent {
        kind,
        payload,
        deadline: tokio::time::Instant::now() + EVENT_HOLD,
        first,
        last,
    };
    if is_outcome {
        // The outcome is the last frame: it took part in the boundary
        // resolution above and is captured, never held or journaled here.
        event.render(&secrets);
        let _ = result_tx.try_send(event.payload);
        chains.clear();
        return true;
    }
    *held = Some(event);
    true
}

/// Number of string leaves in a JSON value.
fn string_leaf_count(value: &Value) -> usize {
    match value {
        Value::String(_) => 1,
        Value::Array(items) => items.iter().map(string_leaf_count).sum(),
        Value::Object(map) => map.values().map(string_leaf_count).sum(),
        _ => 0,
    }
}

/// The last string leaf of a JSON value in traversal order.
fn last_string_mut(value: &mut Value) -> Option<&mut String> {
    match value {
        Value::String(text) => Some(text),
        Value::Array(items) => items.iter_mut().rev().find_map(last_string_mut),
        Value::Object(map) => map.values_mut().rev().find_map(last_string_mut),
        _ => None,
    }
}

/// The first string leaf of a JSON value in traversal order.
fn first_string_mut(value: &mut Value) -> Option<&mut String> {
    match value {
        Value::String(text) => Some(text),
        Value::Array(items) => items.iter_mut().find_map(first_string_mut),
        Value::Object(map) => map.values_mut().find_map(first_string_mut),
        _ => None,
    }
}

/// Stop the module and every descendant it left in its process group, then
/// reap the child. Safe to call after the child already exited.
///
/// Owner Close sends SIGTERM so `preview-serve` stops the app and runs profile
/// cleanup, with SIGKILL after [`PREVIEW_CLOSE_GRACE`]. Preview lifecycle stop
/// sends bounded SIGINT so the wrapper tears down its separately grouped app
/// without cleanup. Every other stop keeps immediate group SIGKILL.
async fn stop_group(
    mode: ModuleStopMode,
    pid: Option<u32>,
    child: &mut tokio::process::Child,
) -> Option<&'static str> {
    use g5d_foundation::process_termination::{
        terminate_tokio_child, TerminationPolicy, TerminationTarget, KILL_SIGNAL,
    };
    let policy = match mode {
        ModuleStopMode::OwnerPreviewClose => TerminationPolicy {
            first_signal: libc::SIGTERM,
            grace: PREVIEW_CLOSE_GRACE,
            kill_deadline: DRAIN_GRACE,
            always_kill_after_grace: false,
        },
        ModuleStopMode::PreviewLifecycle => TerminationPolicy {
            first_signal: libc::SIGINT,
            grace: DRAIN_GRACE,
            kill_deadline: DRAIN_GRACE,
            always_kill_after_grace: false,
        },
        ModuleStopMode::Kill => TerminationPolicy {
            first_signal: KILL_SIGNAL,
            grace: Duration::ZERO,
            kill_deadline: DRAIN_GRACE,
            always_kill_after_grace: false,
        },
    };
    let target = pid.map(TerminationTarget::ProcessGroup);
    match terminate_tokio_child(target, policy, child).await {
        Ok(status) => (mode == ModuleStopMode::OwnerPreviewClose).then(|| classify_close(status)),
        Err(error) => {
            tracing::warn!(?target, %error, "module process reap unconfirmed");
            (mode == ModuleStopMode::OwnerPreviewClose).then_some("killed")
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModuleStopMode {
    Kill,
    OwnerPreviewClose,
    PreviewLifecycle,
}

/// Select the preview's signal from the durable cancellation reason. Missing
/// or unknown reasons fail closed to lifecycle teardown, never customer
/// cleanup; non-preview work keeps the ordinary immediate kill policy.
fn module_stop_mode(
    module_id: &str,
    cancelled: bool,
    cancel_reason: Option<&str>,
) -> ModuleStopMode {
    if module_id != builtin::PREVIEW || !cancelled {
        ModuleStopMode::Kill
    } else if cancel_reason == Some("owner_cancel") {
        ModuleStopMode::OwnerPreviewClose
    } else {
        ModuleStopMode::PreviewLifecycle
    }
}

/// The close classification of a preview's exit status under SIGTERM.
fn classify_close(status: std::process::ExitStatus) -> &'static str {
    match status.code() {
        Some(0) => "clean",
        Some(_) => "failed",
        None => "killed",
    }
}

/// Entry points are pinned by the catalog. A bare `g5d` resolves to the
/// running dispatcher binary so the bundle is self-contained; anything else
/// is canonicalized at launch and must remain below the canonical runtime
/// mount — a symlink inside the mount that points outside it is refused,
/// and the canonical path is what runs. The catalog names the guest mount;
/// a process-mode host's runtime directory stands in for it, so the pinned
/// catalog is the same file in both modes.
fn resolve_entrypoint(
    program: &str,
    runtime_root: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    if program == "g5d" {
        return std::env::current_exe().map_err(|error| format!("dispatcher binary: {error}"));
    }
    let program = std::path::Path::new(program);
    let relocated = program
        .strip_prefix(super::catalog::RUNTIME_MOUNT)
        .map(|below| runtime_root.join(below))
        .unwrap_or_else(|_| program.to_path_buf());
    canonical_entrypoint(&relocated, runtime_root)
}

/// The canonical form of `program`, required to stay below the canonical
/// form of `mount`.
fn canonical_entrypoint(
    program: &std::path::Path,
    mount: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let mount = std::fs::canonicalize(mount)
        .map_err(|error| format!("runtime mount {}: {error}", mount.display()))?;
    let canonical = std::fs::canonicalize(program)
        .map_err(|error| format!("entrypoint {}: {error}", program.display()))?;
    if !canonical.starts_with(&mount) || canonical == mount {
        return Err(format!(
            "entrypoint {} resolves outside the runtime mount",
            program.display()
        ));
    }
    Ok(canonical)
}

async fn write_initial_record<W: AsyncWrite + Unpin>(
    stdin: &mut W,
    record: &Value,
    remaining: Duration,
    cancel: &CancellationToken,
) -> (bool, bool) {
    let mut write_started = false;
    let handed = tokio::select! {
        written = async {
            write_started = true;
            write_line(stdin, record).await
        } => written.is_ok(),
        _ = tokio::time::sleep(remaining), if remaining != Duration::MAX => false,
        _ = cancel.cancelled() => false,
    };
    (handed, write_started)
}

async fn write_line<W: AsyncWrite + Unpin>(stdin: &mut W, value: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec(value).unwrap_or_default();
    bytes.push(b'\n');
    stdin.write_all(&bytes).await?;
    stdin.flush().await
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
type RlimitResource = libc::__rlimit_resource_t;
#[cfg(all(unix, not(all(target_os = "linux", target_env = "gnu"))))]
type RlimitResource = libc::c_int;

/// The bound a module receives for one resource: its catalog value, or the
/// dispatcher's own hard limit when that is lower. A hard limit can only be
/// raised with `CAP_SYS_RESOURCE`, and the pre-exec hook runs after the
/// child's identity switch (`Command::uid`), so no caller may assume it can
/// raise one. A dispatcher nested inside another module (the process-mode
/// host a `workspace.preview` starts, which lives under that module's 4 GiB
/// data and file-size caps) inherits caps below the `agent.turn` catalog
/// values; asking for more would fail every spawn with EPERM, so the request
/// is clamped to the inherited envelope instead. The child inherits the
/// dispatcher's limits, so the envelope is read here, in the parent, and a
/// clamp is logged with the effective value.
#[cfg(unix)]
fn within_inherited_envelope(
    module_id: &str,
    resource: RlimitResource,
    resource_name: &str,
    requested: u64,
) -> u64 {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit writes only the provided struct and reports failure
    // through its return value.
    if unsafe { libc::getrlimit(resource, &mut current) } != 0
        || current.rlim_max == libc::RLIM_INFINITY
    {
        return requested;
    }
    // rlim_t is u64 on Linux and macOS; the cast keeps other targets
    // compiling, so the lint about a same-type cast is expected.
    #[allow(clippy::unnecessary_cast)]
    let inherited = current.rlim_max as u64;
    if inherited < requested {
        tracing::info!(
            module_id,
            resource = resource_name,
            requested,
            effective = inherited,
            "module limit clamped to the dispatcher's inherited hard limit"
        );
    }
    requested.min(inherited)
}

#[cfg(unix)]
fn apply_limits(command: &mut Command, module: &ModuleEntry) {
    let memory_bytes = module.limits.memory_mib.saturating_mul(1024 * 1024);
    let disk_bytes = module
        .limits
        .disk_mib
        .map(|mib| mib.saturating_mul(1024 * 1024));
    let cpu_seconds = module
        .limits
        .wall_clock_ms
        .filter(|_| module.module_id != "agent.turn")
        .map(|ms| {
            // CPU time is bounded by wall clock times the declared core share,
            // plus a small allowance so a fully busy module is stopped by the
            // wall clock (a definite `timed_out`) rather than by SIGXCPU.
            (ms / 1000)
                .saturating_mul(u64::from(module.limits.cpu_cores))
                .saturating_add(5)
        });
    // Read in the parent, before the fork; only async-signal-safe calls
    // remain in the hook below.
    let id = module.module_id.as_str();
    #[cfg(target_os = "linux")]
    let memory_bytes = within_inherited_envelope(id, libc::RLIMIT_DATA, "data", memory_bytes);
    let disk_bytes =
        disk_bytes.map(|disk| within_inherited_envelope(id, libc::RLIMIT_FSIZE, "fsize", disk));
    let cpu_seconds =
        cpu_seconds.map(|cpu| within_inherited_envelope(id, libc::RLIMIT_CPU, "cpu", cpu));
    // SAFETY: only async-signal-safe setrlimit calls run in the child before exec.
    unsafe {
        command.pre_exec(move || {
            let set = |resource: RlimitResource, value: u64| {
                let limit = libc::rlimit {
                    rlim_cur: value as libc::rlim_t,
                    rlim_max: value as libc::rlim_t,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            };
            // RLIMIT_DATA covers heap and private writable mappings on
            // Linux ≥ 4.7; RLIMIT_AS would count glibc's reserved arenas and
            // thread stacks and stop healthy modules. Other kernels leave the
            // memory bound to the VM's own limits.
            #[cfg(target_os = "linux")]
            set(libc::RLIMIT_DATA, memory_bytes)?;
            #[cfg(not(target_os = "linux"))]
            let _ = memory_bytes;
            if let Some(disk) = disk_bytes {
                set(libc::RLIMIT_FSIZE, disk)?;
            }
            if let Some(cpu) = cpu_seconds {
                set(libc::RLIMIT_CPU, cpu)?;
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn apply_limits(_command: &mut Command, _module: &ModuleEntry) {}

#[cfg(test)]
mod tests {
    use std::process::Stdio;
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::{broadcast, mpsc};
    use tokio_util::sync::CancellationToken;
    use url::Url;
    use uuid::Uuid;

    use super::{
        canonical_entrypoint, carry_unresolved_prefixes, classify_close, handle_line,
        module_stop_mode, resolve_chains, stop_group, write_initial_record, LeafState,
        ModuleStopMode, PREVIEW_CLOSE_GRACE,
    };
    use crate::config::{
        DispatchConfig, OwnerGitIdentity, RepositoryLayout, RuntimePaths, WorkspaceLayout,
    };
    use crate::files::ObjectFormat;
    use crate::git::GitRuntime;
    use crate::journal::{Journal, DEFAULT_JOURNAL_BUDGET_BYTES, MAX_EVENT_BYTES};
    use crate::modules::{Catalog, ExecId, ModuleRuntime, Secrets};
    use crate::protocol::Admission;
    use std::time::Instant;
    use tokio::process::Command;

    #[tokio::test]
    async fn timed_out_partial_invocation_write_preserves_ambiguity() {
        let (mut writer, _unread) = tokio::io::duplex(1);
        let record = json!({"input":"x".repeat(1024)});
        let (handed, write_started) = write_initial_record(
            &mut writer,
            &record,
            Duration::from_millis(20),
            &CancellationToken::new(),
        )
        .await;
        assert!(!handed);
        assert!(write_started);
    }

    #[tokio::test]
    async fn early_loader_diagnostics_are_bounded_and_redacted() {
        let diagnostic = b"loader failed with secret-token-123 and trailing diagnostic";
        let safe = super::read_startup_stderr(
            diagnostic.as_slice(),
            45,
            Secrets::new(["secret-token-123".to_string()]),
        )
        .await;
        let text = String::from_utf8(safe).unwrap();
        assert!(text.starts_with("loader failed with [REDACTED]"));
        assert!(!text.contains("secret-token-123"));
        assert!(!text.contains("diagnostic"));
    }

    #[tokio::test]
    async fn early_loader_diagnostics_drop_secret_prefix_at_byte_limit() {
        let prefix = b"loader failed with ";
        let secret = "secret-token-123";
        let diagnostic = format!("loader failed with {secret} and trailing diagnostic");
        for retained in 1..secret.len() {
            let safe = super::read_startup_stderr(
                diagnostic.as_bytes(),
                (prefix.len() + retained) as u64,
                Secrets::new([secret.to_string()]),
            )
            .await;
            assert_eq!(safe, prefix, "leaked a {retained}-byte secret prefix");
        }
    }

    #[tokio::test]
    async fn early_loader_diagnostics_preserve_partial_match_at_real_eof() {
        let diagnostic = b"loader failed with secret-token";
        let safe = super::read_startup_stderr(
            diagnostic.as_slice(),
            1024,
            Secrets::new(["secret-token-123".to_string()]),
        )
        .await;
        assert_eq!(safe, diagnostic);
    }

    struct InterruptedDiagnostic {
        remaining: &'static [u8],
        stall: bool,
    }

    impl tokio::io::AsyncRead for InterruptedDiagnostic {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if !self.remaining.is_empty() {
                let count = self.remaining.len().min(buf.remaining());
                buf.put_slice(&self.remaining[..count]);
                self.remaining = &self.remaining[count..];
                return std::task::Poll::Ready(Ok(()));
            }
            if self.stall {
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(Err(std::io::Error::other("diagnostic pipe failed")))
            }
        }
    }

    #[tokio::test]
    async fn early_loader_diagnostics_drop_secret_prefix_on_read_error() {
        let safe = super::read_startup_stderr(
            InterruptedDiagnostic {
                remaining: b"loader failed with secret-token",
                stall: false,
            },
            1024,
            Secrets::new(["secret-token-123".to_string()]),
        )
        .await;
        assert_eq!(safe, b"loader failed with ");
    }

    #[tokio::test]
    async fn early_loader_diagnostics_drop_secret_prefix_on_timeout() {
        let safe = super::read_startup_stderr(
            InterruptedDiagnostic {
                remaining: b"loader failed with secret-token",
                stall: true,
            },
            1024,
            Secrets::new(["secret-token-123".to_string()]),
        )
        .await;
        assert_eq!(safe, b"loader failed with ");
    }

    #[tokio::test]
    async fn stdout_read_error_never_flushes_unresolved_secret_prefixes() {
        for remaining in [
            b"{\"kind\":\"progress\",\"payload\":{\"text\":\"secret-token\"}}\n".as_slice(),
            b"secret-token".as_slice(),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let runtime = test_runtime(temp.path());
            runtime.add_secret("secret-token-123");
            let exec = ExecId::parse(&format!(
                "task.{}.1.{}",
                Uuid::new_v4(),
                runtime.config.vm_id
            ))
            .unwrap();
            let (limit_tx, _limit_rx) = mpsc::channel(1);
            let (result_tx, _result_rx) = mpsc::channel(1);
            let result = super::read_module_stdout(
                InterruptedDiagnostic {
                    remaining,
                    stall: false,
                },
                runtime.clone(),
                exec.clone(),
                Some(1024),
                limit_tx,
                result_tx,
            )
            .await;
            assert_eq!(result, None);
            let events = runtime.journal.replay(&exec, 1).unwrap();
            assert!(
                !serde_json::to_string(&events)
                    .unwrap()
                    .contains("secret-token"),
                "read errors must discard incomplete lines and unresolved held events"
            );
        }
    }

    #[tokio::test]
    async fn stdout_real_eof_flushes_the_final_unterminated_line() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(temp.path());
        let exec = ExecId::parse(&format!(
            "task.{}.1.{}",
            Uuid::new_v4(),
            runtime.config.vm_id
        ))
        .unwrap();
        let (limit_tx, _limit_rx) = mpsc::channel(1);
        let (result_tx, _result_rx) = mpsc::channel(1);
        assert_eq!(
            super::read_module_stdout(
                b"normal output".as_slice(),
                runtime.clone(),
                exec.clone(),
                Some(1024),
                limit_tx,
                result_tx,
            )
            .await,
            None
        );
        let events = runtime.journal.replay(&exec, 1).unwrap();
        assert_eq!(events.len(), 1);
        assert!(serde_json::to_string(&events)
            .unwrap()
            .contains("normal output"));
    }

    /// A module with a declared cap is stopped at it; one without (an agent
    /// turn, `g5.agents.output-bounds`) is read to its end however much it
    /// writes, every event journaled.
    #[tokio::test]
    async fn stdout_is_capped_only_when_the_module_declares_a_cap() {
        let line = serde_json::to_vec(&json!({
            "kind": "progress",
            "payload": {"text": "x".repeat(900)},
        }))
        .unwrap();
        let mut stream = Vec::new();
        for _ in 0..64 {
            stream.extend_from_slice(&line);
            stream.push(b'\n');
        }
        for (cap, expected, journaled) in [
            (Some(16 * 1024), Some("output"), false),
            (Some(stream.len() as u64), None, true),
            (None, None, true),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let runtime = test_runtime(temp.path());
            let exec = ExecId::parse(&format!(
                "task.{}.1.{}",
                Uuid::new_v4(),
                runtime.config.vm_id
            ))
            .unwrap();
            let (limit_tx, mut limit_rx) = mpsc::channel(1);
            let (result_tx, _result_rx) = mpsc::channel(1);
            assert_eq!(
                super::read_module_stdout(
                    stream.as_slice(),
                    runtime.clone(),
                    exec.clone(),
                    cap,
                    limit_tx,
                    result_tx,
                )
                .await,
                expected,
                "{cap:?}"
            );
            assert_eq!(limit_rx.try_recv().ok(), expected, "{cap:?}");
            assert_eq!(
                runtime.journal.replay(&exec, 1).unwrap().len() == 64,
                journaled,
                "{cap:?}"
            );
        }
    }

    /// One stdout line longer than an event may be ends the run under its
    /// own code, with or without a declared total: it is not the output cap.
    #[tokio::test]
    async fn an_overlong_stdout_line_is_its_own_limit() {
        let stream = vec![b'x'; super::MAX_LINE_BYTES + 64 * 1024];
        for cap in [None, Some(u64::MAX), Some(64 * 1024 * 1024)] {
            let temp = tempfile::tempdir().unwrap();
            let runtime = test_runtime(temp.path());
            let exec = ExecId::parse(&format!(
                "task.{}.1.{}",
                Uuid::new_v4(),
                runtime.config.vm_id
            ))
            .unwrap();
            let (limit_tx, mut limit_rx) = mpsc::channel(1);
            let (result_tx, _result_rx) = mpsc::channel(1);
            assert_eq!(
                super::read_module_stdout(
                    stream.as_slice(),
                    runtime.clone(),
                    exec.clone(),
                    cap,
                    limit_tx,
                    result_tx,
                )
                .await,
                Some("event"),
                "{cap:?}"
            );
            assert_eq!(limit_rx.try_recv().ok(), Some("event"), "{cap:?}");
            assert!(runtime.journal.replay(&exec, 1).unwrap().is_empty());
        }
    }

    fn test_runtime(root: &std::path::Path) -> Arc<ModuleRuntime> {
        let paths = RuntimePaths::under_test_root(root);
        std::fs::create_dir_all(&paths.workspace_root).unwrap();
        std::fs::create_dir(paths.workspace_root.join("code")).unwrap();
        let repo_id = Uuid::new_v4();
        let config = Arc::new(DispatchConfig {
            workspace_id: Uuid::new_v4(),
            vm_id: Uuid::new_v4(),
            owner_user_id: Uuid::new_v4(),
            epoch: 1,
            workspace_token: "test-token".to_string(),
            listener_token: [7; 32],
            api_url: Url::parse("https://api.test").unwrap(),
            dispatch_port: 9100,
            lease_ttl_secs: 60,
            owner_git: OwnerGitIdentity {
                name: "Workspace Owner".to_string(),
                email: "owner@example.test".to_string(),
            },
            paths: paths.clone(),
            layout: WorkspaceLayout {
                repos: vec![RepositoryLayout {
                    repo_id,
                    root_path: "code".to_string(),
                    role: None,
                    git_url: format!("https://api.test/git/{repo_id}"),
                    branch: "shared".to_string(),
                    branch_oid: "a".repeat(40),
                    upstream_branch: Some("main".to_string()),
                    object_format: ObjectFormat::Sha1,
                    follow_branch: false,
                    door_url: None,
                }],
                restore: None,
                references: Vec::new(),
            },
            module_uid: None,
            module_gid: None,
            git_door_port: 0,
            pty_reattach_grace: std::time::Duration::from_secs(15),
            process_mode: false,
        });
        let git = Arc::new(GitRuntime::install(&config).unwrap());
        let journal =
            Arc::new(Journal::open(&paths.state_dir, DEFAULT_JOURNAL_BUDGET_BYTES).unwrap());
        let (events, _) = broadcast::channel(8);
        ModuleRuntime::new(
            config,
            Catalog {
                catalog_version: "test".to_string(),
                modules: Vec::new(),
                bounded_execution_slots: 1,
                content_hash: "test".to_string(),
            },
            root.join("catalog.json"),
            journal,
            Secrets::new(Vec::<String>::new()),
            Arc::new(Admission::default()),
            git,
            events,
        )
        .arc()
    }

    #[cfg(unix)]
    #[test]
    fn chat_delta_redaction_follows_text_across_metadata_and_expired_holds() {
        for expire in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let runtime = test_runtime(temp.path());
            runtime.add_secret("chat-stream-secret");
            let exec = ExecId::parse(&format!(
                "task.{}.1.{}",
                Uuid::new_v4(),
                runtime.config.vm_id
            ))
            .unwrap();
            let (result_tx, _result_rx) = mpsc::channel(1);
            let mut held = None;
            let mut chains = Vec::new();
            for fragment in ["chat-st", "ream-", "secret"] {
                let line = serde_json::to_vec(&json!({"kind":"message.delta","payload":{
                    "annotation":"chat-stream-secret", "block_id":"block", "message_id":"message",
                    "text_delta":fragment, "z_metadata":"trailing metadata"
                }}))
                .unwrap();
                assert!(handle_line(
                    &runtime,
                    &exec,
                    &result_tx,
                    &line,
                    &mut held,
                    &mut chains
                ));
                if expire {
                    assert!(super::flush_held(
                        &runtime,
                        &exec,
                        held.take().unwrap(),
                        &mut chains,
                        false
                    ));
                }
            }
            let end =
                serde_json::to_vec(&json!({"kind":"outcome","payload":{"status":"succeeded"}}))
                    .unwrap();
            assert!(handle_line(
                &runtime,
                &exec,
                &result_tx,
                &end,
                &mut held,
                &mut chains
            ));
            let events = runtime.journal.replay(&exec, 1).unwrap();
            assert_eq!(events.len(), 3);
            let text: String = events
                .iter()
                .map(|event| event.payload["text_delta"].as_str().unwrap())
                .collect();
            assert!(!text.contains("chat-stream-secret"), "{text}");
            assert!(text.contains("[REDACTED]"));
            for event in events {
                assert_eq!(event.payload["annotation"], "[REDACTED]");
                assert_eq!(event.payload["block_id"], "block");
                assert_eq!(event.payload["message_id"], "message");
                assert_eq!(event.payload["z_metadata"], "trailing metadata");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn expiry_masks_only_substantial_secret_prefixes() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnopqrstuvwxyz";
        let secrets = Secrets::new([jwt.to_string(), "k9q".into()]);

        // "ey" opens the token but is two bytes of seventy: emitted as is,
        // carried as a chain so a real continuation is still masked.
        let mut chains = Vec::new();
        let mut state = LeafState::new("Hey".into());
        carry_unresolved_prefixes(&secrets, &mut state, &mut chains);
        assert_eq!(state.render(&secrets), "Hey");
        assert_eq!(chains, vec![b"ey".to_vec()]);
        let decided = resolve_chains(&secrets, &mut chains, "Jhb");
        assert!(
            decided.extends,
            "a continuation of the carried chain still masks"
        );

        // Two bytes of a three-byte secret are substantial.
        let mut chains = Vec::new();
        let mut state = LeafState::new("token k9".into());
        carry_unresolved_prefixes(&secrets, &mut state, &mut chains);
        assert_eq!(state.render(&secrets), "token [REDACTED]");

        // Four bytes of a long secret are substantial whatever its length.
        let mut chains = Vec::new();
        let mut state = LeafState::new(format!("see {}", &jwt[..4]));
        carry_unresolved_prefixes(&secrets, &mut state, &mut chains);
        assert_eq!(state.render(&secrets), "see [REDACTED]");
    }

    /// A dispatcher nested inside another module (the process-mode host a
    /// `workspace.preview` starts) inherits that module's hard limits. The
    /// `agent.turn` catalog asks for 16 GiB of file size; under a lower
    /// inherited hard cap a pre-exec setrlimit fails with EPERM and the
    /// module never spawns (QA preview, 2026-10-13). The runner clamps to
    /// the envelope instead. The test re-executes itself under
    /// `ulimit -f 2048` (1 MiB in 512-byte blocks) so the lowered hard
    /// limit never touches the test harness process; the nested run
    /// prints a marker the outer run requires, so a filter that matched no
    /// test would fail rather than pass vacuously. Root lowers its own hard
    /// limit the same way and, without the clamp, would raise the child's
    /// back to the catalog value, so the test covers the root CI lane too.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_nested_dispatcher_clamps_module_limits_to_its_inherited_envelope() {
        const MARKER: &str = "G5D_TEST_NESTED_LIMIT_ENVELOPE";
        const EVIDENCE: &str = "nested-envelope-assertions-ran";
        if std::env::var_os(MARKER).is_none() {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg("ulimit -f 2048 && exec \"$0\" \"$1\" --exact --nocapture")
                .arg(std::env::current_exe().unwrap())
                // libtest names omit the crate, unlike `module_path!()`.
                .arg("modules::runner::tests::a_nested_dispatcher_clamps_module_limits_to_its_inherited_envelope")
                .env(MARKER, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains(EVIDENCE),
                "nested run failed or ran nothing:\n{stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let catalog = Catalog::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("modules/catalog.json"),
            None,
        )
        .unwrap();
        let module = catalog.find("agent.turn", None).unwrap();
        assert_eq!(
            module.limits.disk_mib,
            Some(16384),
            "the catalog asks for more than 1 MiB"
        );
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg("ulimit -H -f").stdout(Stdio::piped());
        super::apply_limits(&mut command, module);
        let output = command
            .spawn()
            .expect("the spawn succeeds inside a smaller envelope")
            .wait_with_output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "2048",
            "the module's file-size cap is the inherited envelope, not the catalog value"
        );
        println!("{EVIDENCE}");
    }

    #[test]
    fn agent_setup_limit_never_becomes_a_settlement_deadline() {
        let catalog = Catalog::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("modules/catalog.json"),
            None,
        )
        .unwrap();
        let module = catalog.find("agent.turn", None).unwrap();
        let input = json!({
            "limits": {"setup_timeout_ms": 300_000, "max_output_bytes": 1024},
            "provider": {"policy_snapshot": {"limits": {"setup_timeout_ms": 300_000, "max_output_bytes": 1024}}},
        });
        let limits = super::effective_run_limits(module, &input).unwrap();
        assert_eq!(limits.setup, Duration::from_secs(300));
        assert_eq!(limits.wall, Duration::MAX);
        assert!(module.limits.wall_clock_ms.is_none());
    }

    /// `max_output_bytes` is retired (`g5.agents.output-bounds`). An API
    /// that still sends it, one that stops sending it, and a value that
    /// would once have been refused (zero, or a request above its policy)
    /// are all admitted alike; the setup limit is still required and bounded
    /// by the policy.
    #[test]
    fn agent_turns_are_admitted_with_or_without_the_retired_output_limit() {
        let catalog = Catalog::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("modules/catalog.json"),
            None,
        )
        .unwrap();
        for version in [None, Some("1.0.8"), Some("1.0.14")] {
            let module = catalog.find("agent.turn", version).unwrap();
            assert!(module.limits.max_output_bytes.is_none());
            for (request, policy) in [
                (json!(67_108_864u64), json!(67_108_864u64)),
                (serde_json::Value::Null, serde_json::Value::Null),
                (json!(67_108_864u64), serde_json::Value::Null),
                (serde_json::Value::Null, json!(67_108_864u64)),
                (json!(0), json!(0)),
                (json!(2048), json!(1024)),
            ] {
                let mut input = json!({
                    "limits": {"setup_timeout_ms": 300_000},
                    "provider": {"policy_snapshot": {"limits": {"setup_timeout_ms": 300_000}}},
                });
                if !request.is_null() {
                    input["limits"]["max_output_bytes"] = request.clone();
                }
                if !policy.is_null() {
                    input["provider"]["policy_snapshot"]["limits"]["max_output_bytes"] =
                        policy.clone();
                }
                let limits = super::effective_run_limits(module, &input)
                    .unwrap_or_else(|refusal| panic!("{request} {policy}: {refusal}"));
                assert_eq!(limits.wall, Duration::MAX);
            }
        }
        let module = catalog.find("agent.turn", None).unwrap();
        let over_policy = json!({
            "limits": {"setup_timeout_ms": 300_001},
            "provider": {"policy_snapshot": {"limits": {"setup_timeout_ms": 300_000}}},
        });
        let refusal = |input: &serde_json::Value| {
            super::effective_run_limits(module, input)
                .err()
                .expect("the setup limit is still checked")
        };
        assert_eq!(
            refusal(&over_policy)["detail"]["reason"],
            "limit_exceeds_policy"
        );
        let missing_setup = json!({
            "limits": {"max_output_bytes": 1024},
            "provider": {"policy_snapshot": {"limits": {"setup_timeout_ms": 300_000}}},
        });
        assert_eq!(
            refusal(&missing_setup)["detail"]["limit"],
            "setup_timeout_ms"
        );
    }

    #[test]
    fn entrypoints_are_canonicalized_and_kept_below_the_mount() {
        let temp = tempfile::tempdir().unwrap();
        let mount = temp.path().join("opt/g5");
        std::fs::create_dir_all(mount.join("bin")).unwrap();
        std::fs::write(mount.join("bin/tool"), b"#!/bin/sh\n").unwrap();
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&outside, mount.join("bin/escape")).unwrap();
        std::os::unix::fs::symlink("tool", mount.join("bin/alias")).unwrap();
        let canonical_mount = std::fs::canonicalize(&mount).unwrap();
        assert_eq!(
            canonical_entrypoint(&mount.join("bin/tool"), &mount).unwrap(),
            canonical_mount.join("bin/tool")
        );
        assert_eq!(
            canonical_entrypoint(&mount.join("bin/alias"), &mount).unwrap(),
            canonical_mount.join("bin/tool"),
            "a symlink inside the mount runs as its canonical target"
        );
        assert!(canonical_entrypoint(&mount.join("bin/escape"), &mount).is_err());
        assert!(canonical_entrypoint(&mount.join("bin/missing"), &mount).is_err());
        assert!(canonical_entrypoint(&mount, &mount).is_err());
        assert!(canonical_entrypoint(&outside, &mount).is_err());
    }

    #[test]
    fn preview_ready_admits_the_port_for_the_preview_module_only() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(temp.path());
        let preview = ExecId::parse(&format!(
            "task.{}.1.{}",
            Uuid::new_v4(),
            runtime.config.vm_id
        ))
        .unwrap();
        let other = ExecId::parse(&format!(
            "task.{}.1.{}",
            Uuid::new_v4(),
            runtime.config.vm_id
        ))
        .unwrap();
        runtime.register_running_execution_for_test(&preview, crate::modules::builtin::PREVIEW);
        runtime.register_running_execution_for_test(&other, "workspace.terminal");
        let (result_tx, _result_rx) = mpsc::channel(1);
        let ready = serde_json::to_vec(&json!({
            "kind": "preview.ready",
            "payload": {"port": 4173},
        }))
        .unwrap();

        let mut held = None;
        let mut chains = Vec::new();
        assert!(handle_line(
            &runtime,
            &other,
            &result_tx,
            &ready,
            &mut held,
            &mut chains,
        ));
        assert!(
            !runtime.service_port_active(4173),
            "only the preview module may open the HTTP gate"
        );

        let mut held = None;
        let mut chains = Vec::new();
        assert!(handle_line(
            &runtime,
            &preview,
            &result_tx,
            &ready,
            &mut held,
            &mut chains,
        ));
        assert!(
            runtime.service_port_active(4173),
            "the preview module's ready event admits its port before the next scan"
        );
        assert!(!runtime.service_port_active(4174));
    }

    #[test]
    fn structured_output_journal_rejection_is_propagated() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(temp.path());
        let exec = ExecId::parse(&format!(
            "task.{}.1.{}",
            Uuid::new_v4(),
            runtime.config.vm_id
        ))
        .unwrap();
        let (result_tx, _result_rx) = mpsc::channel(1);
        let mut held = None;
        let mut chains = Vec::new();
        let oversized = serde_json::to_vec(&json!({
            "kind": "progress",
            "payload": {"text": "x".repeat(MAX_EVENT_BYTES)},
        }))
        .unwrap();
        assert!(handle_line(
            &runtime,
            &exec,
            &result_tx,
            &oversized,
            &mut held,
            &mut chains,
        ));
        let next = serde_json::to_vec(&json!({
            "kind": "progress",
            "payload": {"text": "next"},
        }))
        .unwrap();
        assert!(
            !handle_line(&runtime, &exec, &result_tx, &next, &mut held, &mut chains,),
            "the stdout reader must surface a journal append failure to the process runner"
        );
        assert!(runtime.journal.replay(&exec, 1).unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_closing_preview_gets_sigterm_and_its_exit_classifies_the_close() {
        /// A shell running `script` as the leader of its own process group.
        async fn spawn(script: &str) -> (Option<u32>, tokio::process::Child) {
            let mut command = Command::new("/bin/sh");
            command
                .arg("-c")
                .arg(script)
                .stdout(Stdio::piped())
                .process_group(0);
            let mut child = command.spawn().unwrap();
            let mut ready = String::new();
            tokio::time::timeout(
                Duration::from_secs(2),
                BufReader::new(child.stdout.take().unwrap()).read_line(&mut ready),
            )
            .await
            .expect("shell trap readiness timed out")
            .expect("read shell trap readiness");
            assert_eq!(ready.trim(), "ready");
            (child.id(), child)
        }
        let (pid, mut child) =
            spawn("trap 'exit 0' TERM; echo ready; while :; do sleep 0.1; done").await;
        let started = Instant::now();
        assert_eq!(
            stop_group(ModuleStopMode::OwnerPreviewClose, pid, &mut child).await,
            Some("clean")
        );
        assert!(
            started.elapsed() < PREVIEW_CLOSE_GRACE,
            "a clean exit returns before the grace"
        );

        let (pid, mut child) =
            spawn("trap 'exit 3' TERM; echo ready; while :; do sleep 0.1; done").await;
        assert_eq!(
            stop_group(ModuleStopMode::OwnerPreviewClose, pid, &mut child).await,
            Some("failed")
        );

        // Lifecycle stop gives preview-serve a bounded chance to tear down its
        // separately grouped app, but never produces an owner-close detail.
        let (pid, mut child) =
            spawn("trap 'exit 0' INT; echo ready; while :; do sleep 0.1; done").await;
        assert_eq!(
            stop_group(ModuleStopMode::PreviewLifecycle, pid, &mut child).await,
            None
        );
        assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(0));

        // Not closing: the group is killed at once and nothing is classified.
        let (pid, mut child) =
            spawn("trap 'exit 0' TERM; echo ready; while :; do sleep 0.1; done").await;
        assert_eq!(
            stop_group(ModuleStopMode::Kill, pid, &mut child).await,
            None
        );
        assert!(
            child
                .try_wait()
                .unwrap()
                .is_some_and(|status| status.code().is_none()),
            "a non-closing stop still SIGKILLs the group"
        );
    }

    #[test]
    fn preview_env_exports_the_isolated_origin_only_when_well_formed() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(temp.path());
        let repo_id = runtime.config.layout.repos[0].repo_id.to_string();
        let env = |input: serde_json::Value| {
            super::preview_env(&runtime.config, &input).map(|env| {
                env.into_iter()
                    .map(|(key, value)| (key, value.into_string().unwrap()))
                    .collect::<std::collections::BTreeMap<_, _>>()
            })
        };

        let origin = "https://0b9c2f0e-4ea9-401d-a054-8f782edffd5b.preview.g5.example";
        let full =
            env(json!({"repo_id": repo_id, "profile": "full-stack", "origin": origin})).unwrap();
        assert_eq!(full["G5D_PREVIEW_ORIGIN"], origin);
        assert_eq!(full["G5D_PREVIEW_PROFILE"], "full-stack");
        assert!(full["G5D_PREVIEW_WORK_DIR"].ends_with("/code"));

        // An API without a preview host suffix sends no origin.
        let bare = env(json!({"repo_id": repo_id, "profile": null})).unwrap();
        assert_eq!(
            bare.keys().copied().collect::<Vec<_>>(),
            ["G5D_PREVIEW_WORK_DIR"]
        );

        for bad in [
            json!("http://p.preview.g5.example"),
            json!("https://p.preview.g5.example/"),
            json!("https://p.preview.g5.example/app"),
            json!("https://p.preview.g5.example:8443"),
            json!("https://user@p.preview.g5.example"),
            json!("https://P.preview.g5.example"),
            json!("https://p.preview.g5.example\nX=1"),
            json!(""),
            json!(7),
        ] {
            assert_eq!(
                env(json!({"repo_id": repo_id, "origin": bad})),
                Err("invalid_preview_origin"),
                "{bad}"
            );
        }
        assert_eq!(
            env(json!({"repo_id": Uuid::new_v4().to_string()})),
            Err("repo_not_in_workspace")
        );
        assert_eq!(env(json!({})), Err("invalid_repo_id"));
    }

    #[test]
    fn a_signalled_preview_exit_is_a_killed_close() {
        use std::os::unix::process::ExitStatusExt as _;
        assert_eq!(
            classify_close(std::process::ExitStatus::from_raw(libc::SIGKILL)),
            "killed"
        );
        assert_eq!(
            classify_close(std::process::ExitStatus::from_raw(0)),
            "clean"
        );
        assert_eq!(
            classify_close(std::process::ExitStatus::from_raw(1 << 8)),
            "failed"
        );
    }

    #[test]
    fn preview_stop_mode_is_scoped_to_the_durable_cancel_reason() {
        assert_eq!(
            module_stop_mode(crate::modules::builtin::PREVIEW, true, Some("owner_cancel")),
            ModuleStopMode::OwnerPreviewClose
        );
        for reason in [Some("workspace_stop"), Some("fork_interrupt"), None] {
            assert_eq!(
                module_stop_mode(crate::modules::builtin::PREVIEW, true, reason),
                ModuleStopMode::PreviewLifecycle
            );
        }
        assert_eq!(
            module_stop_mode(
                crate::modules::builtin::PREVIEW,
                false,
                Some("owner_cancel")
            ),
            ModuleStopMode::Kill
        );
        assert_eq!(
            module_stop_mode("workspace.terminal", true, Some("owner_cancel")),
            ModuleStopMode::Kill
        );
    }

    #[test]
    fn cancelling_a_preview_drops_its_port_from_the_http_gate() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(temp.path());
        let preview = ExecId::parse(&format!(
            "task.{}.1.{}",
            Uuid::new_v4(),
            runtime.config.vm_id
        ))
        .unwrap();
        runtime.register_running_execution_for_test(&preview, crate::modules::builtin::PREVIEW);
        let (result_tx, _result_rx) = mpsc::channel(1);
        let ready = serde_json::to_vec(&json!({
            "kind": "preview.ready",
            "payload": {"port": 4173},
        }))
        .unwrap();
        let mut held = None;
        let mut chains = Vec::new();
        assert!(handle_line(
            &runtime,
            &preview,
            &result_tx,
            &ready,
            &mut held,
            &mut chains,
        ));
        assert!(runtime.service_port_active(4173));

        let reply = runtime
            .cancel(&json!({"exec_id": preview.as_str(), "reason": "owner_cancel"}))
            .expect("cancel a running preview");
        assert_eq!(reply["disposition"], "signalling");
        assert!(
            !runtime.service_port_active(4173),
            "a closing preview must not admit new streams while it is still running"
        );
    }
}
