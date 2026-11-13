//! `g5d preview-serve` handler — the in-VM entrypoint for the `preview` task type
//! (`g5/g5d/cli` D34, `g5/g5d/host` D9).
//!
//! This runs inside the attached workspace VM. It reads the authoritative
//! app config from the selected repository's `.g5/workspace.yaml`
//! `preview:` block (`command`, `port`, `working_dir`, `env`,
//! `cleanup_command`), launches the customer's app on the declared guest
//! port, and stays up until common task cancellation or workspace stop. It is
//! deliberately self-contained: there is NO carrier, sidecar,
//! LLM-proxy, or live workspace file machinery, and it injects NO g5-api credential — the
//! preview process receives no provider credential.

#[cfg(unix)]
use std::os::fd::{FromRawFd as _, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use g5_contracts::error::G5Error;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::cli::config::{PreviewWorkspaceConfig, WorkspaceConfig};

use super::{GlobalFlags, PreviewServeArgs};

/// Boot-injected override for the in-VM workspace mount point. Defaults to the
/// private `/work` directory the supervisor stamps into every preview spec.
const PREVIEW_WORK_DIR_ENV: &str = "G5D_PREVIEW_WORK_DIR";

/// Default in-VM workspace mount (matches `host::assignment::GUEST_WORKSPACE_DIR`).
const DEFAULT_PREVIEW_WORK_DIR: &str = "/work";

/// Boot-injected selector for the named preview profile to serve
/// (`g5/g5-common/types` D18, `g5/g5d/cli` D34). When set, the app config is
/// read from `.g5/preview_profiles/<name>.yaml`; when absent, from the
/// `.g5/workspace.yaml` `preview:` block (back-compat). Matches the env var the
/// host's `build_preview_env` stamps.
const PREVIEW_PROFILE_ENV: &str = "G5D_PREVIEW_PROFILE";

/// The descriptor the dispatcher passes for the application's terminal
/// (`g5.preview.output.live-tail`): the slave of a PTY whose master the
/// dispatcher hosts as the read-only terminal session `preview:<task id>`.
/// When set, the application runs on it as its stdin, stdout and controlling
/// terminal, its stderr is teed onto it, and the profile's cleanup command
/// runs on it too. When absent (an older dispatcher, a unit fixture), the
/// application's stdout is discarded and its stderr read only for the tail.
const APP_TTY_FD_ENV: &str = "G5_APP_TTY_FD";

/// npm cache location env. In a preview guest this is set either by the
/// customer image's OCI env (in-rootfs fallback cache) or — overriding it —
/// by the host's shared-volume stamp (`host::assignment::NPM_CONFIG_CACHE_ENV`,
/// `preview_tunnel` D22).
const NPM_CONFIG_CACHE_ENV: &str = "npm_config_cache";

/// Image-declared pre-warmed npm cache to seed an empty shared cache from
/// (`doctor_and_sidecar` D51). Declared by customer images that bake a warm
/// cache (e.g. `g5d-pairsoft-preview`); absent everywhere else.
const NPM_CACHE_SEED_ENV: &str = "G5D_NPM_CACHE_SEED";

/// npm's content-addressed cache subdirectory. Its presence is the "this
/// cache has been populated" marker, and it is the unit the seeding copy
/// stages + atomically renames.
const CACACHE_DIR: &str = "_cacache";

fn resolve_preview_cwd(work_dir: &Path, working_dir: Option<&str>) -> std::io::Result<PathBuf> {
    let Some(dir) = working_dir.map(str::trim).filter(|dir| !dir.is_empty()) else {
        return Ok(work_dir.to_path_buf());
    };
    let path = PathBuf::from(dir);
    if path.is_absolute() {
        return Ok(path);
    }

    let canonical_root = std::fs::canonicalize(work_dir)?;
    let canonical_cwd = std::fs::canonicalize(work_dir.join(path))?;
    if !canonical_cwd.starts_with(&canonical_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "relative preview working_dir '{}' resolves outside selected repository '{}'",
                dir,
                work_dir.display()
            ),
        ));
    }
    Ok(canonical_cwd)
}

fn validate_preview_env_entry(key: &str, value: &str) -> Result<(), G5Error> {
    if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0') {
        return Err(G5Error::Internal {
            message: format!(
                "preview-serve: invalid preview.env entry (key must be non-empty and free of '=' and NUL; value free of NUL): key={key:?}"
            ),
        });
    }
    Ok(())
}

fn resolve_nonempty_env_var(
    name: &str,
    value: Result<String, std::env::VarError>,
) -> Result<Option<String>, G5Error> {
    match value {
        Ok(value) if value.is_empty() => Err(G5Error::Internal {
            message: format!("preview-serve: {name} is present but empty"),
        }),
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(G5Error::Internal {
            message: format!("preview-serve: {name} is not valid Unicode"),
        }),
    }
}

/// Scrub G5-only environment from the customer app's command BEFORE spawning it.
///
/// The customer app must inherit neither host credentials/bindings nor the G5
/// runtime library path:
/// - `G5D_PREVIEW_ATTACH*` / `G5D_AGENT_*` — the per-VM attach token (the only
///   remotely usable credential), the attach port, and sidecar runtime bindings
///   (custody separation, human review 2026-08-20). The attach listener is
///   spawned BEFORE this scrub and keeps its own copy.
/// - `ANTHROPIC_BASE_URL` — the repointed provider base URL.
/// - `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` — platform loader paths must not
///   make the customer's dynamic loader select platform libraries. A customer
///   can explicitly set its own library path in `preview.env`, applied after
///   this scrub. The platform runtime itself remains read-only.
fn scrub_g5_runtime_env_from_customer_command(command: &mut tokio::process::Command) {
    for (key, _) in std::env::vars_os() {
        let Some(key_str) = key.to_str() else {
            continue;
        };
        if key_str.starts_with("G5D_PREVIEW_ATTACH") || key_str.starts_with("G5D_AGENT_") {
            command.env_remove(&key);
        }
    }
    command.env_remove("ANTHROPIC_BASE_URL");
    g5d_foundation::runtime_environment::remove_native_loader_paths(command);
}

/// The host mounts the G5 runtime bundle (which carries `node`, see the
/// worker-host startup script's runtime bin) read-only at this guest path.
#[cfg(test)]
const RUNTIME_BIN_DIR: &str = g5d_foundation::runtime_environment::GUEST_RUNTIME_BIN_DIR;

/// The bundle directory this preview appends: the dispatcher names it for a
/// process-mode host (`G5D_RUNTIME_BIN_DIR`); a guest keeps the mounted one.
fn runtime_bin_dir() -> String {
    g5d_foundation::runtime_environment::runtime_bin_dir()
        .to_string_lossy()
        .into_owned()
}

/// Make the runtime bundle's tools reachable from the customer's command as a
/// last-resort fallback. The guest PATH comes from the customer's base image,
/// which need not carry `node`; the bundle does, so a `preview.command` such as
/// `node server.js` exited with "command not found" on an image without node.
/// The bundle dir is appended, never prepended, so an image's own tools keep
/// winning, and a customer `PATH` in `preview.env` (applied after this)
/// replaces it entirely. The dispatcher now appends the same directory to every
/// module's command path, so this is normally already satisfied; the shared
/// helper is idempotent and keeps the guarantee for any caller that is not.
fn append_runtime_bin_to_customer_path(command: &mut tokio::process::Command) {
    let inherited = std::env::var("PATH").unwrap_or_default();
    command.env(
        "PATH",
        customer_path_with_bin(&runtime_bin_dir(), &inherited),
    );
}

/// Pure PATH transformation behind [`append_runtime_bin_to_customer_path`]
/// for the guest bundle directory.
#[cfg(test)]
fn customer_path_with_runtime_bin(inherited: &str) -> String {
    customer_path_with_bin(RUNTIME_BIN_DIR, inherited)
}

fn customer_path_with_bin(bin: &str, inherited: &str) -> String {
    g5d_foundation::runtime_environment::append_path(bin, inherited)
}

/// The application's terminal, when the dispatcher hosts one. The descriptor
/// stays open until `preview-serve` exits so Close (the application's
/// shutdown output, then the cleanup command) streams into the same session.
#[cfg(unix)]
#[derive(Clone)]
struct AppTty {
    fd: Arc<OwnedFd>,
    /// Serializes this process's own writes (stage markers, the stderr tee)
    /// so lines never interleave.
    writer: Arc<tokio::sync::Mutex<tokio::fs::File>>,
}

#[cfg(unix)]
impl AppTty {
    /// The terminal named by `G5_APP_TTY_FD`, if any. A value that is not an
    /// open terminal is an error: the dispatcher that set it owes a PTY.
    fn from_env() -> Result<Option<Self>, G5Error> {
        let Some(value) = resolve_nonempty_env_var(APP_TTY_FD_ENV, std::env::var(APP_TTY_FD_ENV))?
        else {
            return Ok(None);
        };
        let fd: i32 = value.parse().map_err(|_| G5Error::Internal {
            message: format!(
                "preview-serve: {APP_TTY_FD_ENV} is not a descriptor number: {value:?}"
            ),
        })?;
        Self::adopt(fd).map(Some)
    }

    /// Take ownership of an inherited terminal descriptor. It is made
    /// close-on-exec here: the application receives it only as its standard
    /// streams, never under its original number.
    fn adopt(fd: i32) -> Result<Self, G5Error> {
        // SAFETY: fcntl and isatty only inspect the descriptor.
        if fd < 0 || unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
            return Err(G5Error::Internal {
                message: format!("preview-serve: {APP_TTY_FD_ENV}={fd} is not an open descriptor"),
            });
        }
        if unsafe { libc::isatty(fd) } != 1 {
            return Err(G5Error::Internal {
                message: format!("preview-serve: {APP_TTY_FD_ENV}={fd} is not a terminal"),
            });
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(G5Error::Internal {
                message: format!(
                    "preview-serve: cannot mark {APP_TTY_FD_ENV} close-on-exec: {}",
                    std::io::Error::last_os_error()
                ),
            });
        }
        // SAFETY: the descriptor is open and this process now owns it.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let writer = std::fs::File::from(fd.try_clone().map_err(|error| G5Error::Internal {
            message: format!("preview-serve: cannot duplicate the application terminal: {error}"),
        })?);
        Ok(Self {
            fd: Arc::new(fd),
            writer: Arc::new(tokio::sync::Mutex::new(tokio::fs::File::from_std(writer))),
        })
    }

    /// A standard-stream handle for a child: the same terminal.
    fn stdio(&self) -> Result<std::process::Stdio, G5Error> {
        Ok(std::process::Stdio::from(self.fd.try_clone().map_err(
            |error| G5Error::Internal {
                message: format!(
                    "preview-serve: cannot duplicate the application terminal: {error}"
                ),
            },
        )?))
    }

    /// Write raw bytes to the terminal (the stderr tee).
    async fn write(&self, bytes: &[u8]) {
        let mut writer = self.writer.lock().await;
        let _ = writer.write_all(bytes).await;
        let _ = writer.flush().await;
    }

    /// A dim stage marker on its own line, for the owner reading the log.
    /// Markers go only to the terminal: never to the protocol channel or the
    /// journal.
    async fn marker(&self, text: &str) {
        self.write(format!("\x1b[2m── {text} ──\x1b[0m\n").as_bytes())
            .await;
    }
}

/// How long the application group gets after SIGTERM before SIGKILL when the
/// preview closes. Cleanup starts only once the group is gone.
const APP_STOP_GRACE: Duration = Duration::from_secs(10);

/// Bound on the profile's `cleanup_command`. The dispatcher's own close grace
/// for `workspace.preview` is longer, so an overrun is reported here as a
/// failed cleanup instead of a silent SIGKILL of this process.
const CLEANUP_BOUND: Duration = Duration::from_secs(60);

/// How much of the application's stderr the failure outcome keeps
/// (`g5.preview.service.exit-diagnostics`): the last 4 KiB, enough for a
/// build error and its cause, far under the dispatcher's event bound.
const STDERR_TAIL_BYTES: usize = 4096;

/// After the app exits, how long the outcome waits for its output pipes to
/// reach end of file. A customer command can leave a background process
/// holding them (the full-stack profile's dockerd does not: it logs to a
/// file), so this is a bound, not a requirement: such a process holds only
/// this wrapper's pipes, never the dispatcher's protocol channel.
const APP_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(250);

/// The tail survives a panicked reader: its bytes are plain data.
fn tail_snapshot(tail: &Mutex<StderrTail>) -> std::sync::MutexGuard<'_, StderrTail> {
    tail.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The last `cap` bytes of the application's stderr, kept beside the
/// forwarded stream so a failure outcome can carry why the app exited.
#[derive(Debug)]
struct StderrTail {
    bytes: Vec<u8>,
    truncated: bool,
    cap: usize,
}

impl StderrTail {
    fn new(cap: usize) -> Self {
        Self {
            bytes: Vec::new(),
            truncated: false,
            cap,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        let before = self.bytes.len() + chunk.len();
        if chunk.len() >= self.cap {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&chunk[chunk.len() - self.cap..]);
        } else {
            self.bytes.extend_from_slice(chunk);
            if self.bytes.len() > self.cap {
                let excess = self.bytes.len() - self.cap;
                self.bytes.drain(..excess);
            }
        }
        if self.bytes.len() < before {
            self.truncated = true;
        }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// Read the app's stderr for the tail that explains a failure
/// (`g5.preview.service.exit-diagnostics`) and, when the dispatcher hosts a
/// terminal for the application, tee it onto that terminal so the owner's
/// live view shows both streams in order. Nothing becomes an event: no
/// per-line event is emitted (each would be a durable task event the API's
/// shared reconciler applies one at a time, and one verbose build delayed
/// every workspace's queued work), and the bytes are not written to this
/// process's stderr (the guest log showed them to nobody the owner can
/// reach). Without a terminal the app's stdout is discarded at spawn.
fn forward_stderr(
    stderr: tokio::process::ChildStderr,
    tail: Arc<Mutex<StderrTail>>,
    #[cfg(unix)] tty: Option<AppTty>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut reader = stderr;
        let mut buffer = vec![0u8; 8192];
        loop {
            let read = match reader.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            tail_snapshot(&tail).push(&buffer[..read]);
            #[cfg(unix)]
            if let Some(tty) = &tty {
                tty.write(&buffer[..read]).await;
            }
        }
    })
}

/// Wait up to the drain grace for the forwarders to reach end of file; a
/// background process that still holds a pipe costs only the bound. A
/// forwarder that finished is never polled again.
async fn drain_forwarders(tasks: &mut [Option<tokio::task::JoinHandle<()>>]) {
    let drain = async {
        for task in tasks.iter_mut() {
            if let Some(handle) = task.as_mut() {
                let _ = handle.await;
                *task = None;
            }
        }
    };
    let _ = tokio::time::timeout(APP_OUTPUT_DRAIN_GRACE, drain).await;
}

/// Stop the forwarders still running (a background process holds their
/// pipe) and wait for each to end, so nothing they would forward can follow
/// the frames emitted after this returns. Dropping a `JoinHandle` only
/// detaches its task.
async fn stop_forwarders(tasks: &mut [Option<tokio::task::JoinHandle<()>>]) {
    for task in tasks.iter_mut().filter_map(Option::take) {
        task.abort();
        let _ = task.await;
    }
}

/// The module `outcome` frame for an app that exited with a failure: its
/// status, the stderr tail and the guest's own error. The dispatcher stores
/// the payload as the attempt outcome's `result`, which the API persists.
fn app_failure_outcome(
    status: std::process::ExitStatus,
    tail: &StderrTail,
    message: &str,
) -> serde_json::Value {
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&status);
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    serde_json::json!({
        "status": "app_exited",
        "app_exit_code": status.code(),
        "app_signal": signal,
        "stderr_tail": tail.text(),
        "stderr_tail_truncated": tail.truncated,
        "message": message,
    })
}

/// Write one protocol frame as one line under the stdout lock, so frames
/// from the output forwarder and this task never interleave.
fn emit_frame(kind: &str, payload: serde_json::Value) {
    use std::io::Write as _;
    let frame = serde_json::json!({
        "kind": kind,
        "payload": payload
    });
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{frame}");
    let _ = out.flush();
}

/// Close bounds, a parameter so tests do not wait the production values.
#[derive(Debug, Clone, Copy)]
struct CloseBounds {
    app_stop: Duration,
    cleanup: Duration,
}

impl Default for CloseBounds {
    fn default() -> Self {
        Self {
            app_stop: APP_STOP_GRACE,
            cleanup: CLEANUP_BOUND,
        }
    }
}

/// Run the customer's preview app until owner Close (SIGTERM), workspace
/// lifecycle stop (SIGINT), or the app's own exit. Close and self-exit run the
/// profile cleanup; lifecycle stop only tears down the app group.
pub async fn execute(_args: &PreviewServeArgs, _global: &GlobalFlags) -> Result<(), G5Error> {
    let work_dir =
        resolve_nonempty_env_var(PREVIEW_WORK_DIR_ENV, std::env::var(PREVIEW_WORK_DIR_ENV))?
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_PREVIEW_WORK_DIR));

    // Authoritative in-VM read of the app config (g5-api only presence-gated the
    // shape and discarded the values). The config source is resolved by the
    // boot-injected profile selector: a named profile reads
    // `.g5/preview_profiles/<name>.yaml`; its absence reads the
    // `.g5/workspace.yaml` `preview:` block (back-compat). Fail closed either way.
    let profile =
        resolve_nonempty_env_var(PREVIEW_PROFILE_ENV, std::env::var(PREVIEW_PROFILE_ENV))?;
    let preview = match profile.as_deref() {
        Some(name) => WorkspaceConfig::load_preview_profile(&work_dir, name),
        None => WorkspaceConfig::load_preview(&work_dir),
    }
    .map_err(|e| G5Error::Internal {
        message: format!("preview-serve: {e}"),
    })?;

    // Relative paths are under the selected repository root; absolute paths
    // remain verbatim (this is the customer's own VM — not a security boundary;
    // the `command` could run anything regardless).
    let cwd = resolve_preview_cwd(&work_dir, preview.working_dir.as_deref()).map_err(|e| {
        G5Error::Internal {
            message: format!("preview-serve: invalid working_dir: {e}"),
        }
    })?;

    tracing::info!(
        port = preview.port,
        command = %preview.command,
        cleanup_command = preview.cleanup_command.as_deref().unwrap_or(""),
        cwd = %cwd.display(),
        "preview-serve launching customer app"
    );

    // Seed an empty shared npm cache from the image-baked seed before the app
    // starts (`doctor_and_sidecar` D51). Strictly best-effort: every failure
    // logs and continues — seeding is an optimization, never a boot gate, and
    // npm treats a partial/absent cache as integrity-verified misses.
    seed_shared_npm_cache_from_env().await;

    // Owner Close arrives as SIGTERM. Workspace/VM lifecycle stop is a
    // distinct SIGINT so the separate app group is still torn down but the
    // customer's cleanup command is not run.
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|e| G5Error::Internal {
        message: format!("preview-serve: SIGTERM handler failed: {e}"),
    })?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|e| G5Error::Internal {
        message: format!("preview-serve: SIGINT handler failed: {e}"),
    })?;
    let stop = async move {
        tokio::select! {
            _ = terminate.recv() => PreviewStop::Close,
            _ = interrupt.recv() => PreviewStop::Lifecycle,
        }
    };
    #[cfg(unix)]
    let tty = AppTty::from_env()?;
    run_preview(
        &preview,
        &cwd,
        stop,
        CloseBounds::default(),
        #[cfg(unix)]
        tty,
    )
    .await
}

/// Why the dispatcher asked the preview wrapper to stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreviewStop {
    /// Owner Close: stop the app, then run profile cleanup.
    Close,
    /// Workspace/VM lifecycle stop: stop the app without customer cleanup.
    Lifecycle,
}

/// How the application phase ended.
enum AppEnd {
    /// The app exited on its own.
    Exited(std::process::ExitStatus),
    /// A stop was requested while the app was running.
    Stopping(PreviewStop),
}

/// The customer's own shell command, with the launch command's cwd, env
/// scrub, PATH fallback, `preview.env` and authoritative `PORT`. The command
/// leads its own process group so the whole tree can be stopped on a bound.
/// On a hosted terminal the application receives the PTY as its standard
/// streams only; it is deliberately *not* made the controlling terminal: a
/// session leader's exit would then SIGHUP the whole application tree, so a
/// Close that SIGTERMs the group would kill a server before its own shutdown
/// handler ran, changing Close semantics the owner relies on.
fn customer_command(
    preview: &PreviewWorkspaceConfig,
    cwd: &Path,
    script: &str,
) -> Result<tokio::process::Command, G5Error> {
    let mut command = tokio::process::Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command.current_dir(cwd);
    scrub_g5_runtime_env_from_customer_command(&mut command);
    append_runtime_bin_to_customer_path(&mut command);
    for (key, value) in &preview.env {
        // `Command::env` PANICS on a key containing `=` or NUL, or a value
        // containing NUL. `preview.env` is user-controlled workspace config, so
        // validate and fail closed with an error instead of crashing the in-VM
        // `preview-serve` process.
        validate_preview_env_entry(key, value)?;
        // `PORT` is owned by the supervisor's host->guest port mapping (it mapped
        // the published host port to `preview.port` from .g5/workspace.yaml). A
        // user `PORT` here would make the app bind a different port than the one
        // published, so the preview URL could not connect — skip it; the
        // authoritative `PORT` is applied AFTER this loop so it always wins.
        if key.eq_ignore_ascii_case("PORT") {
            continue;
        }
        command.env(key, value);
    }
    // Authoritative serving port — set LAST so it overrides any user `PORT`. The
    // app must listen on exactly the port the supervisor published.
    command.env("PORT", preview.port.to_string());
    command.process_group(0);
    Ok(command)
}

/// Launch the app, report readiness, wait for stop or the app's exit, and stop
/// the app group. Owner Close and self-exit then run cleanup; lifecycle stop
/// deliberately skips it.
///
/// Exit status: success when Close completed with a successful (or absent)
/// cleanup, or when the app exited cleanly and cleanup succeeded; failure
/// otherwise, naming the app's status and any cleanup error.
async fn run_preview(
    preview: &PreviewWorkspaceConfig,
    cwd: &Path,
    stop: impl std::future::Future<Output = PreviewStop>,
    bounds: CloseBounds,
    #[cfg(unix)] tty: Option<AppTty>,
) -> Result<(), G5Error> {
    // The app never shares this process's stdout, the dispatcher's protocol
    // channel. With a hosted terminal its stdin and stdout are that terminal
    // and its stderr is teed onto it; without one its stdout is discarded.
    // Either way stderr is a pipe read for the failure tail (no credential
    // is in this env to scrub).
    #[cfg(unix)]
    let mut command = match &tty {
        Some(tty) => {
            let mut command = customer_command(preview, cwd, &preview.command)?;
            command.stdin(tty.stdio()?).stdout(tty.stdio()?);
            command
        }
        None => {
            let mut command = customer_command(preview, cwd, &preview.command)?;
            command.stdout(std::process::Stdio::null());
            command
        }
    };
    #[cfg(not(unix))]
    let mut command = {
        let mut command = customer_command(preview, cwd, &preview.command)?;
        command.stdout(std::process::Stdio::null());
        command
    };
    let mut child = command
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| G5Error::Internal {
            message: format!("preview-serve: failed to spawn app command: {e}"),
        })?;
    let pid = child.id();
    let tail = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
    let mut forwarders = [child.stderr.take().map(|stderr| {
        forward_stderr(
            stderr,
            tail.clone(),
            #[cfg(unix)]
            tty.clone(),
        )
    })];
    #[cfg(unix)]
    if let Some(tty) = &tty {
        tty.marker(&format!("waiting for port {}", preview.port))
            .await;
    }

    // A service task is running as soon as the process exists, but browser
    // admission remains closed until the configured listener is observable.
    // Emit the configured port only after a successful loopback connect; the
    // API journals this fact and returns PREVIEW_NOT_READY before it arrives.
    tokio::pin!(stop);
    let mut ready = false;
    let end = loop {
        tokio::select! {
            biased;
            reason = &mut stop => break AppEnd::Stopping(reason),
            status = child.wait() => {
                break AppEnd::Exited(status.map_err(|e| G5Error::Internal {
                    message: format!("preview-serve: waiting on app process failed: {e}"),
                })?);
            }
            connected = probe_port(preview.port), if !ready => {
                if connected {
                    emit_frame("preview.ready", serde_json::json!({"port": preview.port}));
                    ready = true;
                    #[cfg(unix)]
                    if let Some(tty) = &tty {
                        tty.marker(&format!("port {} listening", preview.port)).await;
                    }
                }
            }
        }
    };

    let app_status = match end {
        AppEnd::Exited(status) => {
            tracing::warn!(code = ?status.code(), "preview app process exited before close");
            // The shell leader can exit after backgrounding descendants. Stop
            // the residual application group before cleanup so a successful
            // self-exit cannot leave a daemon running in the shared VM.
            stop_app_group(pid, &mut child, Duration::ZERO).await;
            // Let the last stderr land in the tail (and on the terminal).
            drain_forwarders(&mut forwarders).await;
            #[cfg(unix)]
            if let Some(tty) = &tty {
                tty.marker(&format!(
                    "application exited with status {}",
                    exit_status_text(status)
                ))
                .await;
            }
            Some(status)
        }
        AppEnd::Stopping(PreviewStop::Close) => {
            tracing::info!("preview close requested; stopping the app group");
            #[cfg(unix)]
            if let Some(tty) = &tty {
                tty.marker("close requested, stopping application").await;
            }
            stop_app_group(pid, &mut child, bounds.app_stop).await;
            // The application's shutdown output belongs to the same session;
            // give the tee the same bounded chance the self-exit path gets.
            drain_forwarders(&mut forwarders).await;
            None
        }
        AppEnd::Stopping(PreviewStop::Lifecycle) => {
            tracing::info!(
                "preview lifecycle stop requested; stopping the app group without cleanup"
            );
            stop_app_group(pid, &mut child, Duration::ZERO).await;
            return Ok(());
        }
    };

    let cleanup = run_cleanup(
        preview,
        cwd,
        bounds.cleanup,
        #[cfg(unix)]
        tty.as_ref(),
    )
    .await;
    #[cfg(unix)]
    if let Some(tty) = &tty {
        tty.marker("closed").await;
    }

    let failure = match (app_status, cleanup) {
        (None, Ok(())) => None,
        (Some(status), Ok(())) if status.success() => None,
        (Some(status), Ok(())) => Some(format!(
            "preview app exited with status {:?}",
            status.code()
        )),
        (None, Err(error)) => Some(format!("preview cleanup failed: {error}")),
        (Some(status), Err(error)) if status.success() => {
            Some(format!("preview cleanup failed: {error}"))
        }
        (Some(status), Err(error)) => Some(format!(
            "preview app exited with status {:?}; cleanup failed: {error}",
            status.code()
        )),
    };
    // An app that failed on its own explains itself in the outcome
    // (`g5.preview.service.exit-diagnostics`); the frame is the module
    // protocol's last, so it follows cleanup's own output and the app's
    // forwarders are stopped first: a process the app left holding its
    // pipes cannot add an output frame after it.
    stop_forwarders(&mut forwarders).await;
    if let (Some(status), Some(message)) = (app_status, &failure) {
        if !status.success() {
            let tail = tail_snapshot(&tail);
            emit_frame("outcome", app_failure_outcome(status, &tail, message));
        }
    }
    failure.map_or(Ok(()), |message| Err(G5Error::Internal { message }))
}

/// `N` for an exit code, `signal N` for a signal, for the stage marker.
fn exit_status_text(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return code.to_string();
    }
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return format!("signal {signal}");
    }
    "unknown".to_string()
}

/// One loopback connect attempt with a short bound, paced at 250 ms.
async fn probe_port(port: u16) -> bool {
    let connected = tokio::time::timeout(
        Duration::from_millis(250),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    if !connected {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    connected
}

/// SIGTERM the app's process group, give the whole group up to `grace`, then
/// SIGKILL any survivors so no descendant outlives the close.
async fn stop_app_group(pid: Option<u32>, child: &mut tokio::process::Child, grace: Duration) {
    use g5d_foundation::process_termination::{
        signal, terminate_tokio_child, TerminationPolicy, TerminationTarget,
    };
    let target = pid.map(TerminationTarget::ProcessGroup);
    #[cfg(unix)]
    let deadline = tokio::time::Instant::now() + grace;
    if let Err(error) = terminate_tokio_child(
        target,
        TerminationPolicy {
            first_signal: libc::SIGTERM,
            grace,
            kill_deadline: Duration::from_secs(5),
            always_kill_after_grace: false,
        },
        child,
    )
    .await
    {
        tracing::warn!(?target, %error, "preview child reap unconfirmed");
    }
    // Reaping the shell does not mean its application has finished handling
    // SIGTERM. Preserve the remainder of the original group grace, returning
    // promptly once the group is empty. A zero-grace lifecycle stop never waits.
    #[cfg(unix)]
    if let Some(group) = pid
        .and_then(|id| i32::try_from(id).ok())
        .filter(|id| *id > 0)
    {
        while tokio::time::Instant::now() < deadline {
            if !app_group_has_live_processes(group as u32, deadline).await {
                break;
            }
            tokio::time::sleep_until(
                deadline.min(tokio::time::Instant::now() + Duration::from_millis(20)),
            )
            .await;
        }
    }
    // Descendants that ignored SIGTERM are killed within the original bound.
    // An already-empty group is not an error.
    if let Some(target) = target {
        let _ = signal(target, libc::SIGKILL);
    }
}

/// Check for a running application group without waiting for orphan reaping;
/// uncertain observations retain the original, bounded shutdown grace.
#[cfg(unix)]
async fn app_group_has_live_processes(pid: u32, deadline: tokio::time::Instant) -> bool {
    // SAFETY: this is our child's group; signal zero only probes existence.
    if unsafe { libc::killpg(pid as i32, 0) } != 0 {
        return std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
    }
    #[cfg(target_os = "linux")]
    {
        // Linux keeps unreaped zombies in their group. Inspect them off the
        // async executor, under the same deadline; errors retain the grace.
        let scan = tokio::task::spawn_blocking(move || {
            linux_app_group_has_live_processes(Path::new("/proc"), pid, deadline)
        });
        !matches!(
            tokio::time::timeout_at(deadline, scan).await,
            Ok(Ok(Ok(false)))
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = deadline;
        true
    }
}

/// A Linux group is finished only when every observed member is a single-thread
/// zombie/dead process. A zombie leader with workers or incomplete data is live.
#[cfg(any(target_os = "linux", test))]
fn linux_app_group_has_live_processes(
    proc_root: &Path,
    pid: u32,
    deadline: tokio::time::Instant,
) -> std::io::Result<bool> {
    use g5d_foundation::process_termination::parse_proc_stat_process_group;
    let mut saw_finished_member = false;
    for entry in std::fs::read_dir(proc_root)? {
        if tokio::time::Instant::now() >= deadline {
            return Ok(true);
        }
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        let (state, group, _) = parse_proc_stat_process_group(&stat)
            .ok_or_else(|| std::io::Error::other("unreadable process group state"))?;
        if group != pid {
            continue;
        }
        // A zombie leader can still have live worker threads. Field 20 is
        // num_threads: only a Z/X process with exactly one thread is finished.
        let threads = stat
            .rsplit_once(')')
            .and_then(|(_, fields)| fields.split_whitespace().nth(17))
            .and_then(|threads| threads.parse::<u32>().ok());
        if !matches!(state, 'Z' | 'X') || threads != Some(1) {
            return Ok(true);
        }
        saw_finished_member = true;
    }
    // A partial/hidden proc view cannot prove absence after killpg succeeded.
    Ok(!saw_finished_member)
}

/// Run the profile's `cleanup_command`, if any, under `bound`. An overrun
/// kills the cleanup's group and counts as a failure.
async fn run_cleanup(
    preview: &PreviewWorkspaceConfig,
    cwd: &Path,
    bound: Duration,
    #[cfg(unix)] tty: Option<&AppTty>,
) -> Result<(), String> {
    let Some(script) = preview.cleanup_command.as_deref() else {
        return Ok(());
    };
    tracing::info!(command = script, "preview cleanup starting");
    // On a hosted terminal the cleanup runs on the same TTY as the
    // application, so the owner sees it under the application's shutdown
    // output. Otherwise both streams are discarded: nothing of it is shown
    // anywhere, and with no pipes a process cleanup leaves behind holds
    // nothing of this wrapper's, let alone the dispatcher's channel.
    let mut command = customer_command(preview, cwd, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    match tty {
        Some(tty) => {
            tty.marker(&format!("running cleanup: {script}")).await;
            command
                .stdin(tty.stdio().map_err(|e| e.to_string())?)
                .stdout(tty.stdio().map_err(|e| e.to_string())?)
                .stderr(tty.stdio().map_err(|e| e.to_string())?);
        }
        None => {
            command
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
    }
    #[cfg(not(unix))]
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to spawn cleanup command: {e}"))?;
    let pid = child.id();
    let waited = tokio::time::timeout(bound, child.wait()).await;
    match waited {
        Ok(status) => {
            // Completion is the whole cleanup group, not only its shell
            // leader: a backgrounded descendant must not turn an incomplete
            // cleanup into a reported clean close.
            stop_app_group(pid, &mut child, Duration::ZERO).await;
            match status {
                Ok(status) if status.success() => {
                    tracing::info!("preview cleanup finished");
                    Ok(())
                }
                Ok(status) => Err(format!(
                    "cleanup command exited with status {:?}",
                    status.code()
                )),
                Err(error) => Err(format!("waiting on cleanup command failed: {error}")),
            }
        }
        Err(_) => {
            stop_app_group(pid, &mut child, Duration::ZERO).await;
            Err(format!(
                "cleanup command did not finish within {}s",
                bound.as_secs()
            ))
        }
    }
}

/// Env-driven wrapper around [`seed_npm_cache`]: reads the cache/seed paths
/// from `npm_config_cache` / `G5D_NPM_CACHE_SEED` and runs the copy on the
/// blocking pool (the cacache tree is hundreds of MB of small files).
async fn seed_shared_npm_cache_from_env() {
    let cache_dir = match std::env::var(NPM_CONFIG_CACHE_ENV) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => return,
    };
    let seed_dir = match std::env::var(NPM_CACHE_SEED_ENV) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v),
        _ => return,
    };
    if cache_dir == seed_dir {
        // No host-mounted shared cache override — the app is already using the
        // image-baked cache directly; nothing to seed.
        return;
    }
    let started = std::time::Instant::now();
    let cache_for_log = cache_dir.clone();
    let seed_for_log = seed_dir.clone();
    let result = tokio::task::spawn_blocking(move || seed_npm_cache(&cache_dir, &seed_dir)).await;
    match result {
        Ok(Ok(true)) => tracing::info!(
            cache = %cache_for_log.display(),
            seed = %seed_for_log.display(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "seeded shared npm cache from image-baked seed"
        ),
        Ok(Ok(false)) => {}
        Ok(Err(e)) => tracing::warn!(
            cache = %cache_for_log.display(),
            seed = %seed_for_log.display(),
            error = %e,
            "npm cache seeding failed; continuing without seed"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "npm cache seeding task panicked; continuing without seed"
        ),
    }
}

/// Seed `<cache_dir>/_cacache` from `<seed_dir>/_cacache` (`doctor_and_sidecar`
/// D51). Returns `Ok(true)` when this call performed the seed, `Ok(false)`
/// when there was nothing to do (already seeded, no seed present, or a
/// concurrent boot won the rename race).
///
/// Concurrency-safe across preview VMs sharing the same workspace cache: the
/// copy lands in a per-process staging dir and is installed with one atomic
/// `rename`; the rename loser removes its staging dir and uses the winner's
/// cache.
fn seed_npm_cache(cache_dir: &Path, seed_dir: &Path) -> std::io::Result<bool> {
    let target = cache_dir.join(CACACHE_DIR);
    if target.exists() {
        return Ok(false);
    }
    let seed_cacache = seed_dir.join(CACACHE_DIR);
    if !seed_cacache.is_dir() {
        return Ok(false);
    }
    let staging = cache_dir.join(format!(".g5-seed-tmp-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    let copied = copy_dir_recursive(&seed_cacache, &staging);
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    match std::fs::rename(&staging, &target) {
        Ok(()) => Ok(true),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            if target.exists() {
                // A concurrent preview boot of the same workspace seeded first.
                Ok(false)
            } else {
                Err(e)
            }
        }
    }
}

/// Minimal recursive dir copy for the cacache tree (directories + regular
/// files only — cacache contains nothing else; anything else is skipped).
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `run_preview` without a hosted terminal: today's streams.
    async fn run_without_tty(
        preview: &PreviewWorkspaceConfig,
        cwd: &Path,
        stop: impl std::future::Future<Output = PreviewStop>,
        bounds: CloseBounds,
    ) -> Result<(), G5Error> {
        run_preview(preview, cwd, stop, bounds, None).await
    }

    fn write_file(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn preview_cwd_defaults_to_selected_repo_and_resolves_relative_or_absolute_values() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path().join("code");
        let client = repo_root.join("client");
        std::fs::create_dir_all(&client).unwrap();
        assert_eq!(
            resolve_preview_cwd(&repo_root, None).expect("default cwd"),
            repo_root
        );
        assert_eq!(
            resolve_preview_cwd(&repo_root, Some("  ")).expect("blank cwd"),
            repo_root
        );
        assert_eq!(
            resolve_preview_cwd(&repo_root, Some("client")).expect("relative cwd"),
            client.canonicalize().unwrap()
        );
        assert_eq!(
            resolve_preview_cwd(&repo_root, Some("/srv/app")).expect("absolute cwd"),
            PathBuf::from("/srv/app")
        );
    }

    #[test]
    fn preview_cwd_rejects_relative_traversal_outside_selected_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path().join("code");
        let sibling = tmp.path().join("docs");
        std::fs::create_dir_all(&repo_root).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();

        let error = resolve_preview_cwd(&repo_root, Some("../docs"))
            .expect_err("relative traversal must stay inside the selected repo");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[cfg(unix)]
    #[test]
    fn preview_cwd_rejects_symlink_escape_outside_selected_repo() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let repo_root = tmp.path().join("code");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&repo_root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        symlink(&outside, repo_root.join("linked-app")).unwrap();

        let error = resolve_preview_cwd(&repo_root, Some("linked-app"))
            .expect_err("relative symlink escape must stay inside the selected repo");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn preview_env_entries_reject_unsafe_process_values() {
        for (key, value) in [
            ("", "value"),
            ("KEY=OTHER", "value"),
            ("KEY\0OTHER", "value"),
            ("KEY", "value\0other"),
        ] {
            assert!(
                validate_preview_env_entry(key, value).is_err(),
                "unsafe preview env entry must fail: key={key:?} value={value:?}"
            );
        }
        validate_preview_env_entry("NODE_ENV", "production")
            .expect("ordinary preview env entry is valid");
    }

    #[tokio::test]
    async fn customer_command_does_not_inherit_g5_runtime_or_credential_env() {
        // The preview boot environment may carry platform loader paths and
        // attach/provider bindings; the customer app
        // must inherit NONE of them (custody separation + runtime isolation). An
        // unrelated benign var still passes through.
        std::env::set_var("LD_LIBRARY_PATH", "/opt/g5/bin");
        std::env::set_var("DYLD_LIBRARY_PATH", "/opt/g5/bin");
        std::env::set_var("G5D_AGENT_LLM_UNIX_SOCKET", "/run/g5-attach/llm.sock");
        std::env::set_var("ANTHROPIC_BASE_URL", "http://127.0.0.1:17002");
        std::env::set_var("PREVIEW_SERVE_TEST_KEEP", "keep-me");

        let mut command = tokio::process::Command::new("/bin/sh");
        command.arg("-c").arg(
            "printf 'LD=%s|DYLD=%s|AGENT=%s|ANTHROPIC=%s|KEEP=%s' \
             \"${LD_LIBRARY_PATH:-}\" \"${DYLD_LIBRARY_PATH:-}\" \
             \"${G5D_AGENT_LLM_UNIX_SOCKET:-}\" \"${ANTHROPIC_BASE_URL:-}\" \
             \"${PREVIEW_SERVE_TEST_KEEP:-}\"",
        );
        scrub_g5_runtime_env_from_customer_command(&mut command);
        let out = command.output().await.expect("spawn shell");

        for key in [
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "G5D_AGENT_LLM_UNIX_SOCKET",
            "ANTHROPIC_BASE_URL",
            "PREVIEW_SERVE_TEST_KEEP",
        ] {
            std::env::remove_var(key);
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout, "LD=|DYLD=|AGENT=|ANTHROPIC=|KEEP=keep-me",
            "customer app must not inherit loader paths / G5D_AGENT_* / ANTHROPIC_BASE_URL, \
             but an unrelated var passes through; got: {stdout}"
        );
    }

    #[test]
    fn customer_command_reaches_runtime_bin_as_a_path_fallback() {
        // A base image without node must still run `node server.js` through the
        // bundle mounted at /opt/g5/bin, after the image's own directories.
        let image_path = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
        assert_eq!(
            customer_path_with_runtime_bin(image_path),
            format!("{image_path}:{RUNTIME_BIN_DIR}")
        );
        // Already present anywhere: not duplicated, order untouched.
        assert_eq!(
            customer_path_with_runtime_bin("/opt/g5/bin:/usr/bin"),
            "/opt/g5/bin:/usr/bin"
        );
        assert_eq!(
            customer_path_with_runtime_bin("/usr/bin:/opt/g5/bin"),
            "/usr/bin:/opt/g5/bin"
        );
        // No inherited PATH at all: the bundle alone.
        assert_eq!(customer_path_with_runtime_bin(""), RUNTIME_BIN_DIR);
        // A lookalike prefix is not the bundle dir.
        assert_eq!(
            customer_path_with_runtime_bin("/opt/g5/bin-extra"),
            format!("/opt/g5/bin-extra:{RUNTIME_BIN_DIR}")
        );
    }

    #[tokio::test]
    async fn customer_path_override_from_preview_env_still_wins() {
        // preview.env is applied after the fallback, so an explicit customer
        // PATH replaces it entirely, exactly as the spawn path orders them.
        let mut command = tokio::process::Command::new("/bin/sh");
        command.arg("-c").arg("printf '%s' \"$PATH\"");
        append_runtime_bin_to_customer_path(&mut command);
        command.env("PATH", "/customer/bin");
        let out = command.output().await.expect("spawn shell");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "/customer/bin");
    }

    #[test]
    fn preview_routing_env_falls_back_only_when_absent() {
        for name in [PREVIEW_WORK_DIR_ENV, PREVIEW_PROFILE_ENV] {
            assert_eq!(
                resolve_nonempty_env_var(name, Err(std::env::VarError::NotPresent)).unwrap(),
                None,
                "an absent {name} must preserve its documented fallback"
            );
            assert_eq!(
                resolve_nonempty_env_var(name, Ok("selected".to_string())).unwrap(),
                Some("selected".to_string()),
                "a non-empty {name} must remain authoritative"
            );
        }
    }

    #[test]
    fn preview_routing_env_rejects_present_empty_values() {
        for name in [PREVIEW_WORK_DIR_ENV, PREVIEW_PROFILE_ENV] {
            let error = resolve_nonempty_env_var(name, Ok(String::new()))
                .expect_err("a present-but-empty routing variable must fail closed");
            assert!(
                error.to_string().contains(name),
                "routing error must identify {name}: {error}"
            );
        }
    }

    #[test]
    fn seed_npm_cache_copies_cacache_into_empty_shared_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = tmp.path().join("seed");
        let cache = tmp.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        write_file(
            &seed.join(CACACHE_DIR).join("content-v2/sha512/ab/cd"),
            "tarball-bytes",
        );
        write_file(
            &seed.join(CACACHE_DIR).join("index-v5/12/34"),
            "index-entry",
        );

        let seeded = seed_npm_cache(&cache, &seed).expect("seed");
        assert!(seeded, "empty cache must be seeded");
        assert_eq!(
            std::fs::read_to_string(cache.join(CACACHE_DIR).join("content-v2/sha512/ab/cd"))
                .unwrap(),
            "tarball-bytes"
        );
        assert_eq!(
            std::fs::read_to_string(cache.join(CACACHE_DIR).join("index-v5/12/34")).unwrap(),
            "index-entry"
        );
        // No staging leftovers.
        let leftovers: Vec<_> = std::fs::read_dir(&cache)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".g5-seed-tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "staging dir must not survive");
    }

    #[test]
    fn seed_npm_cache_is_noop_when_target_already_populated() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = tmp.path().join("seed");
        let cache = tmp.path().join("cache");
        write_file(&seed.join(CACACHE_DIR).join("content-v2/x"), "new");
        write_file(&cache.join(CACACHE_DIR).join("content-v2/y"), "existing");

        let seeded = seed_npm_cache(&cache, &seed).expect("seed");
        assert!(!seeded, "populated cache must be left alone");
        assert!(
            !cache.join(CACACHE_DIR).join("content-v2/x").exists(),
            "winner's cache must not be overwritten"
        );
        assert!(cache.join(CACACHE_DIR).join("content-v2/y").exists());
    }

    #[test]
    fn seed_npm_cache_is_noop_without_a_seed_cacache() {
        let tmp = tempfile::tempdir().unwrap();
        let seed = tmp.path().join("seed"); // no _cacache inside
        let cache = tmp.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&seed).unwrap();

        let seeded = seed_npm_cache(&cache, &seed).expect("seed");
        assert!(!seeded);
        assert!(!cache.join(CACACHE_DIR).exists());
    }

    /// `g5.preview.service.exit-diagnostics`: the tail keeps exactly the last
    /// `cap` bytes across chunk boundaries and says when it dropped any.
    #[test]
    fn stderr_tail_keeps_only_the_last_bytes() {
        let mut tail = StderrTail::new(8);
        tail.push(b"abc");
        assert_eq!(tail.text(), "abc");
        assert!(!tail.truncated);
        tail.push(b"defgh");
        assert_eq!(tail.text(), "abcdefgh");
        assert!(!tail.truncated, "exactly the bound drops nothing");
        tail.push(b"ij");
        assert_eq!(tail.text(), "cdefghij");
        assert!(tail.truncated);
        let mut big = StderrTail::new(4);
        big.push(b"0123456789");
        assert_eq!(big.text(), "6789", "one chunk over the bound keeps its end");
        assert!(big.truncated);
        let mut lossy = StderrTail::new(3);
        lossy.push("héllo".as_bytes());
        assert_eq!(lossy.text(), "llo");
    }

    /// The failure outcome names the app's status, the bounded tail and the
    /// guest's error; a clean exit emits no outcome frame.
    #[cfg(unix)]
    #[tokio::test]
    async fn app_failure_outcome_carries_the_bounded_stderr_tail() {
        use std::os::unix::process::ExitStatusExt;
        let mut tail = StderrTail::new(STDERR_TAIL_BYTES);
        tail.push(b"error: failed to open registry\nCaused by: Permission denied\n");
        let payload = app_failure_outcome(
            std::process::ExitStatus::from_raw(1 << 8),
            &tail,
            "preview app exited with status Some(1)",
        );
        assert_eq!(payload["status"], "app_exited");
        assert_eq!(payload["app_exit_code"], 1);
        assert!(payload["app_signal"].is_null());
        assert_eq!(
            payload["stderr_tail"],
            "error: failed to open registry\nCaused by: Permission denied\n"
        );
        assert_eq!(payload["stderr_tail_truncated"], false);
        assert_eq!(payload["message"], "preview app exited with status Some(1)");

        let killed = app_failure_outcome(std::process::ExitStatus::from_raw(9), &tail, "killed");
        assert!(killed["app_exit_code"].is_null());
        assert_eq!(killed["app_signal"], 9);

        // The real forwarding reader feeds the same tail from a child's pipe.
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(1, "printf 'line one\\nline two\\n' >&2; exit 3", None);
        let shared = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let mut child = customer_command(&preview, tmp.path(), &preview.command)
            .unwrap()
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let reader = forward_stderr(child.stderr.take().unwrap(), shared.clone(), None);
        let status = child.wait().await.unwrap();
        reader.await.unwrap();
        assert_eq!(status.code(), Some(3));
        assert_eq!(tail_snapshot(&shared).text(), "line one\nline two\n");
    }

    /// A forwarder whose pipe a background process still holds is stopped
    /// within the drain grace and joined, so the outcome frame that follows
    /// cannot be trailed by output.
    #[tokio::test]
    async fn stop_forwarders_ends_readers_whose_pipes_are_still_held() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(1, "sleep 30; echo late; echo late >&2", None);
        let mut child = customer_command(&preview, tmp.path(), &preview.command)
            .unwrap()
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let tail = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let mut forwarders = [child
            .stderr
            .take()
            .map(|stderr| forward_stderr(stderr, tail.clone(), None))];
        let started = std::time::Instant::now();
        drain_forwarders(&mut forwarders).await;
        assert!(
            forwarders.iter().all(Option::is_some),
            "the held pipe keeps the forwarder past the grace"
        );
        stop_forwarders(&mut forwarders).await;
        assert!(forwarders.iter().all(Option::is_none));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "stopping does not wait for the holder: {:?}",
            started.elapsed()
        );
        child.kill().await.unwrap();
        let _ = child.wait().await;
    }

    /// A preview config for close tests: `command` serves `port`, `cleanup`
    /// is the optional cleanup command.
    fn close_config(port: u16, command: &str, cleanup: Option<&str>) -> PreviewWorkspaceConfig {
        PreviewWorkspaceConfig {
            command: command.to_string(),
            port,
            working_dir: None,
            env: std::collections::BTreeMap::new(),
            cleanup_command: cleanup.map(str::to_string),
        }
    }

    /// Whether `pid` still exists.
    fn pid_alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks for the process's existence.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// The pid a test app wrote to `path`, waiting up to two seconds for it.
    async fn read_pid(path: &Path) -> i32 {
        for _ in 0..40 {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    return pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("app never wrote its pid to {}", path.display());
    }

    /// A hosted terminal for the application, read back through the same
    /// ring the dispatcher serves viewers from.
    #[cfg(unix)]
    struct CapturedTty {
        session: Arc<g5d_foundation::terminal::HostedSession>,
        tty: Option<AppTty>,
    }

    #[cfg(unix)]
    impl CapturedTty {
        fn open() -> Self {
            use std::os::fd::IntoRawFd as _;
            let hosted =
                g5d_foundation::terminal::HostedSession::open("preview:test", 1 << 20, 80, 24)
                    .unwrap();
            let tty = AppTty::adopt(hosted.slave.into_raw_fd()).unwrap();
            Self {
                session: hosted.session,
                tty: Some(tty),
            }
        }

        fn tty(&self) -> AppTty {
            self.tty.clone().expect("the terminal is still held")
        }

        /// Everything written to the terminal once every holder closed it;
        /// this drops the test's own holder first.
        async fn output(&mut self) -> String {
            self.tty.take();
            tokio::time::timeout(Duration::from_secs(5), async {
                while self.session.is_alive() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("the terminal's last holder never closed");
            let (replay, _) = self.session.replay_from(None);
            String::from_utf8_lossy(&replay.data).into_owned()
        }
    }

    fn position(text: &str, needle: &str) -> usize {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing from terminal output:\n{text}"))
    }

    /// `g5.preview.output.live-tail`: with `G5_APP_TTY_FD` the application's
    /// stdin and stdout are the terminal (not its controlling terminal, so a
    /// leader's exit never SIGHUPs the tree), stderr is teed onto it and
    /// still fills the tail, the cleanup command runs on the same terminal,
    /// and the stage markers appear in order.
    #[cfg(unix)]
    #[tokio::test]
    async fn tty_mode_runs_the_app_and_cleanup_on_the_hosted_terminal_with_stage_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let preview = close_config(
            port,
            "printf 'APP_STDOUT\\n'; printf 'APP_STDERR\\n' >&2; \
             if [ -t 0 ] && [ -t 1 ]; then printf 'APP_ON_TTY\\n'; fi; \
             if [ ! -t 2 ]; then printf 'STDERR_IS_PIPE\\n'; fi; \
             if (exec 3</dev/tty) 2>/dev/null; then printf 'CONTROLLING_TTY\\n'; else printf 'NO_CONTROLLING_TTY\\n'; fi; \
             trap 'printf \"APP_SHUTDOWN\\n\"; exit 0' TERM; \
             while :; do sleep 0.1; done",
            Some("printf 'CLEANUP_OUT\\n'; printf 'CLEANUP_ERR\\n' >&2; [ -t 1 ] && printf 'CLEANUP_ON_TTY\\n'"),
        );
        let mut captured = CapturedTty::open();
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let tty = captured.tty();
        let run = tokio::spawn(async move {
            run_preview(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                bounds,
                Some(tty),
            )
            .await
        });
        // Let the app print and the probe see the listener before Close.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (replay, _) = captured.session.replay_from(None);
                let text = String::from_utf8_lossy(&replay.data).into_owned();
                if text.contains("NO_CONTROLLING_TTY") && text.contains("listening") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the application never reached the terminal");
        close_tx.send(()).unwrap();
        run.await
            .unwrap()
            .expect("close with cleanup exits cleanly");
        let text = captured.output().await;
        drop(listener);

        for needle in [
            "APP_STDOUT",
            "APP_STDERR",
            "APP_ON_TTY",
            "STDERR_IS_PIPE",
            "NO_CONTROLLING_TTY",
            "APP_SHUTDOWN",
            "CLEANUP_OUT",
            "CLEANUP_ERR",
            "CLEANUP_ON_TTY",
        ] {
            position(&text, needle);
        }
        let waiting = position(&text, &format!("── waiting for port {port} ──"));
        let listening = position(&text, &format!("── port {port} listening ──"));
        let closing = position(&text, "── close requested, stopping application ──");
        let shutdown = position(&text, "APP_SHUTDOWN");
        let cleanup = position(&text, "── running cleanup: printf 'CLEANUP_OUT");
        let cleanup_out = position(&text, "CLEANUP_OUT\r\n");
        let closed = position(&text, "── closed ──");
        assert!(waiting < listening, "{text}");
        assert!(listening < closing, "{text}");
        assert!(closing < shutdown, "{text}");
        assert!(shutdown < cleanup, "{text}");
        assert!(cleanup < cleanup_out, "{text}");
        assert!(cleanup_out < closed, "{text}");
        assert!(
            text.contains("\x1b[2m── closed ──\x1b[0m"),
            "markers are dim: {text:?}"
        );
        assert!(
            !text.contains("\"kind\""),
            "no protocol frame reaches the terminal: {text}"
        );
    }

    /// A launcher may exit on SIGTERM before its child finishes shutting down.
    /// The child's shutdown output still precedes cleanup on the hosted terminal.
    #[cfg(unix)]
    #[tokio::test]
    async fn close_preserves_descendant_shutdown_after_the_launch_shell_exits() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(
            &tmp.path().join("app.sh"),
            "trap 'sleep 0.15; printf \"CHILD_SHUTDOWN\\n\"; exit 0' TERM\n\
             printf 'CHILD_READY\\n'\n\
             while :; do sleep 1; done\n",
        );
        let preview = close_config(
            1,
            "sh app.sh & wait",
            Some("printf 'CLEANUP_AFTER_CHILD\\n'"),
        );
        let mut captured = CapturedTty::open();
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let tty = captured.tty();
        let run = tokio::spawn(async move {
            run_preview(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                CloseBounds {
                    app_stop: Duration::from_secs(10),
                    cleanup: Duration::from_secs(2),
                },
                Some(tty),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (replay, _) = captured.session.replay_from(None);
                if String::from_utf8_lossy(&replay.data).contains("CHILD_READY") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the child never installed its shutdown handler");
        close_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("close finishes without waiting out the unused grace")
            .unwrap()
            .expect("close and cleanup succeed");
        let text = captured.output().await;
        assert!(
            position(&text, "CHILD_SHUTDOWN") < position(&text, "── running cleanup:"),
            "the child's graceful shutdown must finish before cleanup: {text}"
        );
        position(&text, "CLEANUP_AFTER_CHILD");
    }

    /// An application that fails on its own in TTY mode still explains
    /// itself in the outcome: the tail is read from the teed pipe exactly as
    /// without a terminal, and the exit marker names the status.
    #[cfg(unix)]
    #[tokio::test]
    async fn tty_mode_keeps_the_failure_tail_and_marks_the_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let mut captured = CapturedTty::open();
        let preview = close_config(
            1,
            "printf 'building\\n'; printf 'error: boom\\n' >&2; exit 7",
            Some("printf 'cleaned\\n'"),
        );
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let error = run_preview(
            &preview,
            tmp.path(),
            std::future::pending::<PreviewStop>(),
            bounds,
            Some(captured.tty()),
        )
        .await
        .expect_err("a failed app fails the preview");
        assert!(
            error
                .to_string()
                .contains("preview app exited with status Some(7)"),
            "{error}"
        );
        let text = captured.output().await;
        let building = position(&text, "building");
        let boom = position(&text, "error: boom");
        let exited = position(&text, "── application exited with status 7 ──");
        let cleanup = position(&text, "── running cleanup: printf 'cleaned");
        let cleaned = position(&text, "cleaned\r\n");
        let closed = position(&text, "── closed ──");
        assert!(building < exited && boom < exited, "{text}");
        assert!(
            exited < cleanup && cleanup < cleaned && cleaned < closed,
            "{text}"
        );

        // The tee leaves the tail exactly as the pipe-only reader fills it.
        let tail = Arc::new(Mutex::new(StderrTail::new(STDERR_TAIL_BYTES)));
        let mut teed = CapturedTty::open();
        let mut child = customer_command(&preview, tmp.path(), "printf 'one\\ntwo\\n' >&2")
            .unwrap()
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let reader = forward_stderr(child.stderr.take().unwrap(), tail.clone(), Some(teed.tty()));
        child.wait().await.unwrap();
        reader.await.unwrap();
        assert_eq!(tail_snapshot(&tail).text(), "one\ntwo\n");
        assert_eq!(teed.output().await, "one\r\ntwo\r\n");
    }

    /// Without `G5_APP_TTY_FD` nothing changes: the application's stdout is
    /// not a terminal (it is discarded) and no marker is written anywhere.
    #[cfg(unix)]
    #[tokio::test]
    async fn without_the_tty_env_the_app_keeps_todays_streams() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(
            1,
            "if [ -t 1 ]; then touch stdout-is-tty; else touch stdout-is-not-tty; fi; \
             if [ -t 0 ]; then touch stdin-is-tty; fi; exit 0",
            None,
        );
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        run_without_tty(
            &preview,
            tmp.path(),
            std::future::pending::<PreviewStop>(),
            bounds,
        )
        .await
        .expect("a clean exit succeeds");
        assert!(tmp.path().join("stdout-is-not-tty").exists());
        assert!(!tmp.path().join("stdout-is-tty").exists());
        assert!(!tmp.path().join("stdin-is-tty").exists());
        assert!(
            AppTty::from_env().unwrap().is_none(),
            "no env var, no terminal"
        );
        assert!(
            AppTty::adopt(-1).is_err() && AppTty::adopt(i32::MAX).is_err(),
            "a closed descriptor is refused"
        );
        let not_a_tty = std::fs::File::open("/dev/null").unwrap();
        assert!(
            AppTty::adopt(std::os::fd::AsRawFd::as_raw_fd(&not_a_tty)).is_err(),
            "a non-terminal descriptor is refused"
        );
        assert_eq!(
            exit_status_text(std::os::unix::process::ExitStatusExt::from_raw(3 << 8)),
            "3"
        );
        assert_eq!(
            exit_status_text(std::os::unix::process::ExitStatusExt::from_raw(9)),
            "signal 9"
        );
    }

    /// Zombies cannot consume the grace, but live members and worker threads can.
    #[test]
    fn linux_group_scan_ignores_only_finished_single_thread_members() {
        let tmp = tempfile::tempdir().unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        for (state, threads, live) in [
            ('Z', "1", false),
            ('X', "1", false),
            ('S', "1", true),
            ('Z', "2", true),
            ('Z', "0", true),
            ('Z', "invalid", true),
            ('Z', "", true),
        ] {
            // Fields 3 through 20; the command deliberately contains ')'.
            write_file(
                &tmp.path().join("12/stat"),
                &format!(
                    "12 (a ) process) {state} 1 42 7 {}{threads}",
                    "0 ".repeat(13)
                ),
            );
            assert_eq!(
                linux_app_group_has_live_processes(tmp.path(), 42, deadline).unwrap(),
                live,
                "state={state}, threads={threads}"
            );
        }
        // A live process in another group does not keep this group alive.
        write_file(
            &tmp.path().join("12/stat"),
            "12 (zombie) Z 1 42 7 0 0 0 0 0 0 0 0 0 0 0 0 0 1",
        );
        write_file(&tmp.path().join("13/stat"), "13 (other) S 1 43 7");
        assert!(!linux_app_group_has_live_processes(tmp.path(), 42, deadline).unwrap());
        write_file(&tmp.path().join("13/stat"), "13 (app worker) S 1 42 7");
        assert!(linux_app_group_has_live_processes(tmp.path(), 42, deadline).unwrap());
    }

    /// Missing, malformed, or late process evidence cannot shorten the grace.
    #[test]
    fn linux_group_scan_keeps_grace_when_visibility_or_time_is_uncertain() {
        let tmp = tempfile::tempdir().unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        // killpg succeeded, so an empty or disappearing proc view is uncertain.
        assert!(linux_app_group_has_live_processes(tmp.path(), 42, deadline).unwrap());
        std::fs::create_dir(tmp.path().join("12")).unwrap();
        assert!(linux_app_group_has_live_processes(tmp.path(), 42, deadline).unwrap());
        write_file(&tmp.path().join("12/stat"), "malformed");
        assert!(linux_app_group_has_live_processes(tmp.path(), 42, deadline).is_err());
        assert!(
            linux_app_group_has_live_processes(&tmp.path().join("unavailable"), 42, deadline)
                .is_err()
        );
        assert!(
            linux_app_group_has_live_processes(tmp.path(), 42, tokio::time::Instant::now())
                .unwrap()
        );
    }

    /// Isolate the subreaper setting from the test runner and its other children.
    #[cfg(target_os = "linux")]
    #[test]
    fn close_does_not_wait_for_an_orphan_zombie_to_be_reaped() {
        let tmp = tempfile::tempdir().unwrap();
        let completed = tmp.path().join("completed");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cli::preview_serve::tests::orphan_zombie_close_fixture",
                "--include-ignored",
            ])
            .env("G5D_TEST_PREVIEW_ZOMBIE_FIXTURE", &completed)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "orphan zombie close fixture failed: {status}"
        );
        assert_eq!(std::fs::read_to_string(completed).unwrap(), "ok");
    }

    /// Retain an adopted zombie so close cannot depend on its parent's reaping.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    #[ignore = "isolated process for close_does_not_wait_for_an_orphan_zombie_to_be_reaped"]
    async fn orphan_zombie_close_fixture() {
        let Some(completed) = std::env::var_os("G5D_TEST_PREVIEW_ZOMBIE_FIXTURE") else {
            return;
        };
        // SAFETY: only this isolated fixture process adopts its own orphans.
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
        let tmp = tempfile::tempdir().unwrap();
        write_file(
            &tmp.path().join("app.sh"),
            // Exit only after the original shell is reaped, so that shell
            // cannot reap this child before the fixture adopts it.
            "leader=$1\n\
             trap 'while kill -0 \"$leader\" 2>/dev/null; do :; done; exit 0' TERM\n\
             echo $$ > descendant.pid\nwhile :; do :; done\n",
        );
        let preview = close_config(1, "trap 'exit 0' TERM; sh app.sh $$ & wait", None);
        let mut child = customer_command(&preview, tmp.path(), &preview.command)
            .unwrap()
            .spawn()
            .unwrap();
        let group = child.id().unwrap();
        let descendant = read_pid(&tmp.path().join("descendant.pid")).await;
        let started = tokio::time::Instant::now();
        stop_app_group(Some(group), &mut child, Duration::from_secs(5)).await;
        let elapsed = started.elapsed();
        let stat = std::fs::read_to_string(format!("/proc/{descendant}/stat")).unwrap();
        // SAFETY: observe our group, then reap only the child adopted above.
        let group_exists = unsafe { libc::killpg(group as i32, 0) } == 0;
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(descendant, &mut status, libc::WNOHANG) };
        assert_eq!(
            reaped, descendant,
            "the fixture must leave no orphan zombie"
        );
        assert_eq!(
            g5d_foundation::process_termination::parse_proc_stat_process_group(&stat)
                .map(|(state, group, _)| (state, group)),
            Some(('Z', group)),
        );
        assert!(
            group_exists,
            "the unreaped zombie must keep killpg successful"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "zombie consumed grace: {elapsed:?}"
        );
        std::fs::write(completed, "ok").unwrap();
    }

    /// A shell may exit on SIGTERM before its application's handler runs.
    /// The whole group must retain the grace, including its terminal output.
    #[cfg(unix)]
    #[tokio::test]
    async fn close_preserves_descendant_shutdown_output_after_shell_exit() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("app.sh"),
            "trap 'sleep 0.2; printf \"APP_SHUTDOWN\\n\"; printf \"APP_SHUTDOWN_ERR\\n\" >&2; exit 0' TERM\n\
             echo $$ > descendant.pid\n\
             while :; do sleep 30; done\n",
        )
        .unwrap();
        let preview = close_config(
            1,
            "trap 'exit 0' TERM; sh app.sh & wait",
            Some("printf 'CLEANUP_OUT\\n'"),
        );
        let mut captured = CapturedTty::open();
        let tty = captured.tty();
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let run = tokio::spawn(async move {
            run_preview(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                CloseBounds {
                    app_stop: Duration::from_secs(1),
                    cleanup: Duration::from_secs(1),
                },
                Some(tty),
            )
            .await
        });
        read_pid(&tmp.path().join("descendant.pid")).await;
        close_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("close remains bounded")
            .unwrap()
            .expect("the application and cleanup close cleanly");
        // The application keeps the PTY open until exit, so EOF also proves it
        // stopped without depending on when PID 1 reaps an orphaned zombie.
        let text = captured.output().await;
        let closing = position(&text, "close requested, stopping application");
        let shutdown = position(&text, "APP_SHUTDOWN\r\n");
        let shutdown_err = position(&text, "APP_SHUTDOWN_ERR\r\n");
        let cleanup = position(&text, "running cleanup:");
        let closed = position(&text, "── closed ──");
        assert!(closing < shutdown && shutdown < cleanup, "{text}");
        assert!(shutdown_err < cleanup && cleanup < closed, "{text}");
    }

    /// Reaping a slow shell must leave only the remainder of the original
    /// grace for a surviving application, not start another full grace.
    #[cfg(unix)]
    #[tokio::test]
    async fn close_uses_remaining_group_grace_after_reaping_slow_shell() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("app.sh"),
            "trap '' TERM\necho $$ > descendant.pid\nwhile :; do sleep 30; done\n",
        )
        .unwrap();
        let preview = close_config(1, "trap 'sleep 2; exit 0' TERM; sh app.sh & wait", None);
        let mut captured = CapturedTty::open();
        let mut child = customer_command(&preview, tmp.path(), &preview.command)
            .unwrap()
            .stdout(captured.tty().stdio().unwrap())
            .stderr(captured.tty().stdio().unwrap())
            .spawn()
            .unwrap();
        read_pid(&tmp.path().join("descendant.pid")).await;
        let grace = Duration::from_secs(3);
        let started = tokio::time::Instant::now();
        tokio::time::timeout(
            Duration::from_secs(6),
            stop_app_group(child.id(), &mut child, grace),
        )
        .await
        .expect("a surviving descendant must not extend the group grace");
        assert!(started.elapsed() >= grace);
        // Allow scheduling slack, but reject the 5 seconds that granting a
        // fresh grace after the shell's 2-second handler would require.
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the shell's exit must not reset the group deadline"
        );
        // The TERM-ignoring application keeps its PTY open until killed. EOF
        // proves it stopped without depending on PID 1's zombie reap timing.
        captured.output().await;
    }

    #[tokio::test]
    async fn close_stops_the_app_group_then_runs_cleanup_with_the_launch_env() {
        let tmp = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let preview = close_config(
            port,
            "echo $$ > app.pid; exec sleep 30",
            Some("test -f app.pid && printf '%s' \"$PORT\" > cleanup.txt"),
        );
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let run = tokio::spawn(async move {
            run_without_tty(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                bounds,
            )
            .await
        });
        let pid = read_pid(&tmp.path().join("app.pid")).await;
        assert!(pid_alive(pid), "app runs until close");
        assert!(
            !tmp.path().join("cleanup.txt").exists(),
            "cleanup must not run while the app serves"
        );
        let close_started = tokio::time::Instant::now();
        close_tx.send(()).unwrap();
        run.await
            .unwrap()
            .expect("close with a successful cleanup exits cleanly");
        assert!(
            close_started.elapsed() < bounds.app_stop,
            "an empty group must not wait out the remaining grace"
        );
        assert!(!pid_alive(pid), "the app group is gone before cleanup");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("cleanup.txt")).unwrap(),
            port.to_string(),
            "cleanup runs in the launch cwd with the authoritative PORT"
        );
        drop(listener);
    }

    #[tokio::test]
    async fn close_kills_an_app_that_ignores_sigterm_within_the_bound() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(1, "trap '' TERM; echo $$ > app.pid; sleep 30", None);
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_millis(300),
            cleanup: Duration::from_secs(5),
        };
        let run = tokio::spawn(async move {
            run_without_tty(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                bounds,
            )
            .await
        });
        let pid = read_pid(&tmp.path().join("app.pid")).await;
        close_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(10), run)
            .await
            .expect("close is bounded")
            .unwrap()
            .expect("a killed app still closes cleanly without cleanup");
        assert!(!pid_alive(pid), "SIGKILL reaches the app after the grace");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn close_kills_a_surviving_descendant_at_the_grace_deadline() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(
            1,
            "sh -c 'trap \"\" TERM; echo $$ > app.pid; exec sleep 30' & wait",
            None,
        );
        let mut child = customer_command(&preview, tmp.path(), &preview.command)
            .unwrap()
            .spawn()
            .unwrap();
        let group = child.id();
        let app_pid = read_pid(&tmp.path().join("app.pid")).await;
        tokio::time::timeout(
            Duration::from_secs(2),
            stop_app_group(group, &mut child, Duration::from_millis(300)),
        )
        .await
        .expect("a surviving descendant must not extend the grace");
        tokio::time::timeout(Duration::from_secs(2), async {
            while pid_alive(app_pid) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("SIGKILL must reach descendants after their launch shell exits");
    }

    /// The G5 `full-stack` profile pattern: a daemon the command starts under
    /// `setsid` leaves the app's process group, survives the app stop, and
    /// is still there for the cleanup command to use (dockerd for
    /// `just dev-down`).
    #[tokio::test]
    async fn a_daemon_started_under_setsid_outlives_the_app_stop_for_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(
            1,
            "setsid sh -c 'echo $$ > daemon.pid; exec sleep 30' & echo $$ > app.pid; exec sleep 30",
            Some("kill -0 \"$(cat daemon.pid)\" && touch cleanup-saw-daemon; kill \"$(cat daemon.pid)\""),
        );
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let run = tokio::spawn(async move {
            run_without_tty(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                bounds,
            )
            .await
        });
        let app_pid = read_pid(&tmp.path().join("app.pid")).await;
        let daemon_pid = read_pid(&tmp.path().join("daemon.pid")).await;
        close_tx.send(()).unwrap();
        run.await
            .unwrap()
            .expect("close with a cleanup that reaches the daemon exits cleanly");
        assert!(!pid_alive(app_pid), "the app group is stopped");
        assert!(
            tmp.path().join("cleanup-saw-daemon").exists(),
            "the setsid daemon must survive the app stop so cleanup can use it"
        );
        for _ in 0..20 {
            if !pid_alive(daemon_pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!pid_alive(daemon_pid), "cleanup stopped the daemon");
    }

    #[tokio::test]
    async fn lifecycle_stop_kills_the_app_group_without_running_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(
            1,
            "echo $$ > app.pid; exec sleep 30",
            Some("touch cleanup-must-not-run"),
        );
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let run = tokio::spawn(async move {
            run_without_tty(
                &preview,
                &cwd,
                async move {
                    let _ = stop_rx.await;
                    PreviewStop::Lifecycle
                },
                bounds,
            )
            .await
        });
        let pid = read_pid(&tmp.path().join("app.pid")).await;

        let stop_started = tokio::time::Instant::now();
        stop_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(10), run)
            .await
            .expect("lifecycle stop is bounded")
            .unwrap()
            .expect("lifecycle stop exits cleanly");
        assert!(
            stop_started.elapsed() < bounds.app_stop,
            "a lifecycle stop must not wait for owner-close grace"
        );
        assert!(!pid_alive(pid), "lifecycle stop left the app running");
        assert!(
            !tmp.path().join("cleanup-must-not-run").exists(),
            "workspace lifecycle stop ran profile cleanup"
        );
    }

    #[tokio::test]
    async fn close_reports_a_failed_or_overrunning_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(1, "sleep 30", Some("exit 3"));
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let run = tokio::spawn(async move {
            run_without_tty(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                bounds,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        close_tx.send(()).unwrap();
        let error = run
            .await
            .unwrap()
            .expect_err("a non-zero cleanup fails the close");
        assert!(
            error
                .to_string()
                .contains("cleanup command exited with status Some(3)"),
            "{error}"
        );

        let preview = close_config(1, "sleep 30", Some("echo $$ > cleanup.pid; exec sleep 30"));
        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let cwd = tmp.path().to_path_buf();
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_millis(500),
        };
        let run = tokio::spawn(async move {
            run_without_tty(
                &preview,
                &cwd,
                async move {
                    let _ = close_rx.await;
                    PreviewStop::Close
                },
                bounds,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(300)).await;
        close_tx.send(()).unwrap();
        let error = run
            .await
            .unwrap()
            .expect_err("an overrunning cleanup fails the close");
        assert!(
            error.to_string().contains("did not finish within"),
            "{error}"
        );
        let cleanup_pid = read_pid(&tmp.path().join("cleanup.pid")).await;
        assert!(!pid_alive(cleanup_pid), "an overrunning cleanup is killed");
    }

    #[tokio::test]
    async fn an_app_that_exits_on_its_own_still_runs_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(1, "exit 2", Some("touch cleanup.txt"));
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };
        let error = run_without_tty(
            &preview,
            tmp.path(),
            std::future::pending::<PreviewStop>(),
            bounds,
        )
        .await
        .expect_err("a failed app fails the preview");
        assert!(
            error
                .to_string()
                .contains("preview app exited with status Some(2)"),
            "{error}"
        );
        assert!(
            tmp.path().join("cleanup.txt").exists(),
            "cleanup ran after the exit"
        );

        let preview = close_config(1, "exit 0", Some("touch clean-exit.txt"));
        run_without_tty(
            &preview,
            tmp.path(),
            std::future::pending::<PreviewStop>(),
            bounds,
        )
        .await
        .expect("a clean exit with a successful cleanup succeeds");
        assert!(tmp.path().join("clean-exit.txt").exists());
    }

    #[tokio::test]
    async fn self_exit_stops_background_app_children_before_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(
            1,
            "(sleep 0.3; touch app-child-leaked) & exit 0",
            Some("sleep 0.6; test ! -e app-child-leaked; touch cleanup.txt"),
        );
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };

        run_without_tty(
            &preview,
            tmp.path(),
            std::future::pending::<PreviewStop>(),
            bounds,
        )
        .await
        .expect("cleanup must run only after residual app children are stopped");
        assert!(tmp.path().join("cleanup.txt").exists());
        assert!(
            !tmp.path().join("app-child-leaked").exists(),
            "a background app child survived the self-exit"
        );
    }

    #[tokio::test]
    async fn successful_cleanup_does_not_leave_background_children() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = close_config(
            1,
            "exit 0",
            Some("(sleep 0.3; touch cleanup-child-leaked) & exit 0"),
        );
        let bounds = CloseBounds {
            app_stop: Duration::from_secs(5),
            cleanup: Duration::from_secs(5),
        };

        run_without_tty(
            &preview,
            tmp.path(),
            std::future::pending::<PreviewStop>(),
            bounds,
        )
        .await
        .expect("a successful cleanup stops its whole process group");
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            !tmp.path().join("cleanup-child-leaked").exists(),
            "a background cleanup child survived a reported clean close"
        );
    }
}
