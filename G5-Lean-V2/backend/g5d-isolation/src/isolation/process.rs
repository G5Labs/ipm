//! Process-level isolation backend using git worktrees.
//!
//! This is the explicit host-process backend. It does not provide a VM or
//! container boundary, so it applies defensive guardrails instead of claiming
//! full isolation.
//!
//! # Behavior
//!
//! - Spawns a child process in the workspace (worktree) directory
//! - Clears inherited environment variables before applying a minimal allowlist
//! - Sets process group for clean kill (`setpgid`)
//! - Captures stdout/stderr for `RunResult`
//! - Logs warnings when resource limits are configured but not enforceable
//! - Kill switch: SIGTERM → 5s wait → SIGKILL process group

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::detect::find_binary;
use super::{
    child_reap_channel, BackendType, ChildReapReceipt, IsolationLevel, MicrovmBackend,
    MicrovmRunSpec, RunHandle, RunHandleInner, RunResult, FORCED_CHILD_REAP_TIMEOUT,
    GRACEFUL_CHILD_REAP_TIMEOUT,
};

// ---------------------------------------------------------------------------
// ProcessBackend
// ---------------------------------------------------------------------------

/// Process-level isolation backend using git worktrees.
///
/// This is the explicit host-process backend. It is intended for operator-
/// selected local development scenarios, not as a silent substitute for a VM.
pub struct ProcessBackend {
    /// Whether git is available (cached probe result).
    available: bool,
}

impl Default for ProcessBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessBackend {
    /// Create a new ProcessBackend.
    /// Probes for git availability during construction.
    #[must_use]
    pub fn new() -> Self {
        let available = find_binary("git").is_some();
        ProcessBackend { available }
    }

    /// Configure process group for clean kill (Unix only).
    #[cfg(unix)]
    fn configure_process_group(cmd: &mut tokio::process::Command) {
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }
    }

    /// No-op on non-Unix platforms.
    #[cfg(not(unix))]
    fn configure_process_group(_cmd: &mut tokio::process::Command) {
        // No process group management on non-Unix
    }

    fn resolve_workspace_root(spec: &MicrovmRunSpec) -> anyhow::Result<PathBuf> {
        let workspace_root = std::fs::canonicalize(&spec.mount_host_dir).map_err(|e| {
            anyhow::anyhow!(
                "ProcessBackend workspace root is not accessible ({}): {}",
                spec.mount_host_dir.display(),
                e
            )
        })?;

        if !workspace_root.is_dir() {
            anyhow::bail!(
                "ProcessBackend workspace root must be a directory: {}",
                workspace_root.display()
            );
        }

        Ok(workspace_root)
    }

    fn resolve_command_path(workspace_root: &Path, command: &str) -> anyhow::Result<PathBuf> {
        let command_path = Path::new(command);
        if command_path.components().count() > 1 {
            let candidate = if command_path.is_absolute() {
                command_path.to_path_buf()
            } else {
                workspace_root.join(command_path)
            };
            return std::fs::canonicalize(&candidate).map_err(|e| {
                anyhow::anyhow!(
                    "ProcessBackend command is not accessible ({}): {}",
                    candidate.display(),
                    e
                )
            });
        }

        find_binary(command)
            .ok_or_else(|| anyhow::anyhow!("ProcessBackend command not found in PATH: {}", command))
    }

    fn apply_guarded_environment(
        cmd: &mut tokio::process::Command,
        spec: &MicrovmRunSpec,
        workspace_root: &Path,
    ) -> anyhow::Result<()> {
        if !spec.extra_mounts.is_empty() {
            anyhow::bail!(
                "ProcessBackend does not support extra mounts; refusing to run with unsupported host paths"
            );
        }

        // A caller that names the child's home and temporary directory (the
        // workspace host keeps them beside its own state) must not get scratch
        // directories created inside the workspace root, where they would
        // show up in the repository's status.
        let supplied = |key: &str| spec.env.iter().any(|(name, _)| name == key);
        let home_dir = (!supplied("HOME")).then(|| workspace_root.join(".g5-process-home"));
        let tmp_dir = (!supplied("TMPDIR")).then(|| workspace_root.join(".g5-process-tmp"));
        for dir in home_dir.iter().chain(tmp_dir.iter()) {
            std::fs::create_dir_all(dir)?;
        }

        cmd.env_clear();

        let default_path = std::env::var("PATH").unwrap_or_else(|_| {
            if cfg!(target_os = "macos") {
                "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin".to_string()
            } else {
                "/usr/bin:/bin:/usr/local/bin".to_string()
            }
        });
        cmd.env("PATH", default_path);

        for key in ["LANG", "LC_ALL", "LC_CTYPE", "TERM"] {
            if let Ok(value) = std::env::var(key) {
                cmd.env(key, value);
            }
        }

        if let Some(home_dir) = &home_dir {
            cmd.env("HOME", home_dir);
        }
        if let Some(tmp_dir) = &tmp_dir {
            cmd.env("TMPDIR", tmp_dir);
            cmd.env("TMP", tmp_dir);
            cmd.env("TEMP", tmp_dir);
        }
        cmd.env("PWD", workspace_root);
        cmd.env("G5_PROCESS_WORKSPACE_ROOT", workspace_root);

        for (key, value) in &spec.env {
            cmd.env(key, value);
        }

        Ok(())
    }
}

impl ProcessBackend {
    /// Start a caller-built command as its own process group and return the
    /// handle this backend's `kill`/[`Self::terminate`] accept. The caller
    /// owns the program, arguments, environment, working directory and
    /// standard streams (a workspace host writes a dispatcher's output to a
    /// per-VM log); this backend owns the group, the sole child waiter and
    /// the reap receipt that custody relies on. Any output pipe the caller
    /// left in place is drained so a verbose child cannot block on exit. The
    /// handle publishes the host loopback as its address: a process child's
    /// listeners live there.
    pub fn spawn_group(cmd: tokio::process::Command) -> anyhow::Result<RunHandle> {
        Self::spawn(cmd, Scope::Group)
    }

    /// Like [`Self::spawn_group`], but the child leads a new session and its
    /// handle stands for the whole incarnation: every process group of that
    /// session plus every process carrying `tag` in its environment (work
    /// that left the session through `setsid`, such as PTY terminals and
    /// detached applications). A stop tears all of them down, and the reap
    /// receipt that custody consumes is published only once the leader has
    /// exited *and* none of them is left, as a VM teardown leaves no guest
    /// process behind. The caller must put `tag` into the child's environment
    /// itself; children inherit it from there.
    pub fn spawn_session(
        cmd: tokio::process::Command,
        tag: Option<ProcessTag>,
    ) -> anyhow::Result<RunHandle> {
        Self::spawn(cmd, Scope::Session(tag))
    }

    /// A new session for the child; on Linux the kernel also kills the
    /// leader if this host dies without running its shutdown path, closing
    /// the race where the host dies between spawn and the prctl. Detached
    /// descendants are not covered by that backstop; a restarted host settles
    /// them from the incarnation record before it reclaims anything.
    #[cfg(unix)]
    fn configure_session(cmd: &mut tokio::process::Command) {
        #[cfg(target_os = "linux")]
        // SAFETY: getpid has no failure modes.
        let expected_parent = unsafe { libc::getpid() };
        unsafe {
            cmd.pre_exec(move || {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() != expected_parent {
                        return Err(std::io::Error::other(
                            "host exited before the dispatcher was armed",
                        ));
                    }
                }
                Ok(())
            });
        }
    }

    #[cfg(not(unix))]
    fn configure_session(_cmd: &mut tokio::process::Command) {}

    fn spawn(mut cmd: tokio::process::Command, scope: Scope) -> anyhow::Result<RunHandle> {
        let session_leader = matches!(scope, Scope::Session(_));
        if session_leader {
            adopt_orphans();
            Self::configure_session(&mut cmd);
        } else {
            Self::configure_process_group(&mut cmd);
        }
        let tag = match &scope {
            Scope::Session(Some(tag)) => Some((tag.key.clone(), tag.value.clone())),
            _ => None,
        };
        let started = session_leader.then(std::time::SystemTime::now);
        let mut child = cmd.spawn()?;
        let child_pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("started process child has no PID"))?;
        if session_leader {
            owned_children().lock().unwrap().insert(child_pid);
        }
        let (reap_publisher, reap_monitor) = child_reap_channel();

        // Drain the inherited output pipes independently so a verbose
        // long-running process cannot block before it reaches exit.
        if let Some(mut stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut stdout, &mut tokio::io::sink()).await;
            });
        }
        if let Some(mut stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
            });
        }

        // This is the sole child owner and therefore the sole authority that
        // may publish a reap receipt. For a session leader the receipt is exit
        // proof for the whole incarnation: a leader that died with work still
        // running (a dispatcher crash under a live preview) is followed by a
        // teardown of that work, and the receipt waits until exit is proven.
        // Uncertainty (a same-user process started after the incarnation
        // whose environment cannot be read) is never resolved by assumption:
        // the waiter keeps proving, custody keeps the root until it succeeds.
        let waiter_tag = tag.clone();
        tokio::spawn(async move {
            let waited = child.wait().await;
            if session_leader {
                owned_children().lock().unwrap().remove(&child_pid);
            }
            match waited {
                Ok(status) => {
                    if session_leader {
                        let tag = waiter_tag.as_ref().map(|(key, value)| ProcessTag {
                            key: key.clone(),
                            value: value.clone(),
                        });
                        loop {
                            match teardown_incarnation(
                                Some(child_pid),
                                tag.as_ref(),
                                started,
                                adopted_host(),
                                INCARNATION_TEARDOWN_BOUND,
                            )
                            .await
                            {
                                Ok(()) => break,
                                Err(error) => tracing::warn!(
                                    child_pid,
                                    error = %error,
                                    "session leader exited but its incarnation's exit is not yet proven; still trying"
                                ),
                            }
                        }
                    }
                    let receipt = reap_publisher.publish_reaped(status);
                    tracing::debug!(
                        child_pid,
                        exit_code = ?receipt.exit_code(),
                        "Process backend child exited and was reaped"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        child_pid,
                        error = %error,
                        "Process backend waiter failed to reap child"
                    );
                    reap_publisher.publish_wait_failed(error.to_string());
                }
            }
        });

        Ok(RunHandle {
            id: format!("process-{}", child_pid),
            backend_type: BackendType::Process,
            vmm_loopback_addr: Some(std::net::Ipv4Addr::LOCALHOST),
            inner: RunHandleInner::Process {
                child_pid,
                pgid: Some(child_pid), // pgid == pid under setpgid(0,0) and setsid
                session_leader,
                tag,
                started,
            },
            reap_monitor,
            console_tail: None,
            console_relay_handles: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// Stop a started group with the caller's exact graceful window, like
    /// `IsolationManager::terminate`: a zero grace sends SIGKILL at once,
    /// otherwise SIGTERM is followed by SIGKILL only after the bound expires.
    /// Returns only with the waiter's reap receipt. An incarnation (session
    /// leader with its tagged processes) is always torn down at once; it has
    /// no graceful shape, see [`Self::spawn_session`].
    pub async fn terminate(
        &self,
        handle: &RunHandle,
        grace: Duration,
    ) -> anyhow::Result<ChildReapReceipt> {
        let target = Self::termination_target(handle)?;
        if let Some(receipt) = handle.current_reap_receipt()? {
            return Ok(receipt);
        }
        #[cfg(unix)]
        {
            match target {
                Target::Incarnation {
                    session,
                    tag,
                    started,
                } => Self::kill_incarnation(handle, session, tag.as_ref(), started).await,
                Target::Group(target) => {
                    super::terminate_handle_target(handle, target, grace, Duration::from_secs(10))
                        .await
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (target, grace);
            self.kill(handle).await
        }
    }

    /// SIGKILL every group of the incarnation's session and every process
    /// carrying its tag, then wait for the waiter's receipt, which arrives
    /// only once the leader is reaped and the incarnation's exit is proven.
    /// A bounded wait that ends without the receipt is an error the caller
    /// reports; the waiter keeps proving and a later stop can succeed.
    #[cfg(unix)]
    async fn kill_incarnation(
        handle: &RunHandle,
        session: u32,
        tag: Option<&ProcessTag>,
        started: Option<std::time::SystemTime>,
    ) -> anyhow::Result<ChildReapReceipt> {
        let census = incarnation_census(Some(session), tag, started, adopted_host())?;
        signal_census(&census, libc::SIGKILL)?;
        signal_group(session, libc::SIGKILL)?;
        handle
            .wait_for_reap(INCARNATION_PROOF_BOUND)
            .await
            .map_err(|error| {
                let census = incarnation_census(Some(session), tag, started, adopted_host())
                    .map(|census| describe(&census))
                    .unwrap_or_else(|error| error.to_string());
                anyhow::anyhow!("incarnation exit not yet proven ({census}): {error}")
            })
    }

    /// What a stop signals: the child's process group, or the whole
    /// incarnation when it was started as a session leader.
    #[cfg(unix)]
    fn termination_target(handle: &RunHandle) -> anyhow::Result<Target> {
        use g5d_foundation::process_termination::TerminationTarget;
        match &handle.inner {
            RunHandleInner::Process {
                child_pid,
                pgid,
                session_leader,
                tag,
                started,
            } => Ok(if *session_leader {
                Target::Incarnation {
                    session: *child_pid,
                    tag: tag.as_ref().map(|(key, value)| ProcessTag {
                        key: key.clone(),
                        value: value.clone(),
                    }),
                    started: *started,
                }
            } else {
                Target::Group(TerminationTarget::ProcessGroup(pgid.unwrap_or(*child_pid)))
            }),
            _ => anyhow::bail!("Invalid handle for ProcessBackend"),
        }
    }

    #[cfg(not(unix))]
    fn termination_target(handle: &RunHandle) -> anyhow::Result<u32> {
        match &handle.inner {
            RunHandleInner::Process { child_pid, .. } => Ok(*child_pid),
            _ => anyhow::bail!("Invalid handle for ProcessBackend"),
        }
    }
}

/// `key=value` carried by every process of an incarnation started with
/// [`ProcessBackend::spawn_session`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTag {
    pub key: String,
    pub value: String,
}

enum Scope {
    Group,
    Session(Option<ProcessTag>),
}

#[cfg(unix)]
enum Target {
    Group(g5d_foundation::process_termination::TerminationTarget),
    Incarnation {
        session: u32,
        tag: Option<ProcessTag>,
        started: Option<std::time::SystemTime>,
    },
}

/// How long a teardown pass gets before it reports that exit is not yet
/// proven; the waiter then tries again.
const INCARNATION_TEARDOWN_BOUND: Duration = Duration::from_secs(10);
/// How long a stop waits for the receipt before reporting the unproven exit
/// to its caller.
const INCARNATION_PROOF_BOUND: Duration = Duration::from_secs(6);

/// Whether this process is established as the parent its incarnations'
/// orphans reparent to. Only then may ancestry exclude a hidden process from
/// an incarnation (review R16); otherwise uncertainty is bounded by time only.
static ADOPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The pid that adopted this process's orphans, when adoption is established.
fn adopted_host() -> Option<u32> {
    ADOPTED
        .load(std::sync::atomic::Ordering::SeqCst)
        .then(std::process::id)
}

/// Session leaders this process spawned and still waits for. The orphan
/// reaper never touches them: their exit status belongs to their waiter.
fn owned_children() -> &'static std::sync::Mutex<std::collections::HashSet<u32>> {
    static OWNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u32>>> =
        std::sync::OnceLock::new();
    OWNED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Make this process the parent every orphan of its incarnations reparents to
/// (Linux `PR_SET_CHILD_SUBREAPER`), so a descendant that hid its environment
/// stays attributable to this host by ancestry, and reap those orphans once
/// they exit so a long-running host does not accumulate zombies. Idempotent;
/// the reaper runs once per process on the current runtime.
fn adopt_orphans() {
    #[cfg(target_os = "linux")]
    {
        static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        STARTED.get_or_init(|| {
            // SAFETY: prctl with PR_SET_CHILD_SUBREAPER only changes this
            // process's reparenting attribute.
            if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) } != 0 {
                tracing::warn!(
                    error = %std::io::Error::last_os_error(),
                    "could not become a child subreaper; hidden orphans are attributed by time only"
                );
            } else {
                ADOPTED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            // A plain thread, not a runtime task: it must outlive any one
            // runtime and costs one `/proc` scan per second.
            std::thread::Builder::new()
                .name("g5d-orphan-reaper".into())
                .spawn(|| {
                    let trace = std::env::var_os("G5D_ORPHAN_REAPER_TRACE").is_some();
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                        let owned = owned_children().lock().unwrap().clone();
                        let result =
                            g5d_foundation::process_termination::reap_reparented_zombies(|pid| {
                                owned.contains(&pid)
                            });
                        if trace {
                            eprintln!("orphan reaper: pass {result:?} (owned {owned:?})");
                        }
                        if let Err(error) = result {
                            tracing::debug!(%error, "orphan reaper pass failed");
                        }
                    }
                })
                .expect("spawn the orphan reaper thread");
        });
    }
}

#[cfg(unix)]
fn incarnation_census(
    session: Option<u32>,
    tag: Option<&ProcessTag>,
    started: Option<std::time::SystemTime>,
    adopted_by: Option<u32>,
) -> anyhow::Result<g5d_foundation::process_termination::IncarnationCensus> {
    let (key, value) = match tag {
        Some(tag) => (tag.key.as_str(), tag.value.as_str()),
        // No tag: only the session is observable, nothing can be unknown.
        None => ("", ""),
    };
    let mut census = g5d_foundation::process_termination::incarnation_census(
        session, adopted_by, key, value, started,
    )?;
    if tag.is_none() {
        census.tagged.clear();
        census.unknown.clear();
    }
    Ok(census)
}

#[cfg(unix)]
fn describe(census: &g5d_foundation::process_termination::IncarnationCensus) -> String {
    format!(
        "{} live group(s), {} tagged process(es), {} same-user process(es) whose environment could not be inspected{}",
        census.groups.len(),
        census.tagged.len(),
        census.unknown.len(),
        if census.unknown.is_empty() {
            String::new()
        } else {
            format!(
                ": pids {}",
                census
                    .unknown
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    )
}

#[cfg(unix)]
fn signal_group(group: u32, signal: i32) -> anyhow::Result<()> {
    use g5d_foundation::process_termination::{signal as signal_target, TerminationTarget};
    signal_target(TerminationTarget::ProcessGroup(group), signal)?;
    Ok(())
}

#[cfg(unix)]
fn signal_census(
    census: &g5d_foundation::process_termination::IncarnationCensus,
    signal: i32,
) -> anyhow::Result<()> {
    for group in &census.groups {
        signal_group(*group, signal)?;
    }
    for pid in &census.tagged {
        // SAFETY: pid names a live process of this user found by its tag; a
        // vanished pid is harmless (ESRCH), a recycled one is this user's.
        unsafe {
            libc::kill(*pid as i32, signal);
        }
    }
    Ok(())
}

/// Tear down an incarnation and prove its exit: SIGKILL every live group of
/// `session` and every process carrying `tag`, repeating until none is left,
/// then succeed only when no same-user process started since `started` has
/// an unreadable environment. `adopted_by` is the pid that adopted the
/// incarnation's orphans when that is established (this host, a child
/// subreaper on Linux); `None` keeps every eligible unreadable process
/// uncertain, which is what a restarted host settling a dead host's
/// incarnation and a host without adoption must do (reviews R15, R16).
/// Within `bound`; an unproven state is an error naming what is left, never a
/// receipt. Used by the waiter after a leader exits and by a restarted host
/// for the incarnations a dead host left behind (`g5.runtime.custody`,
/// `g5.runtime.boot.backend.process-local`).
#[cfg(unix)]
pub async fn teardown_incarnation(
    session: Option<u32>,
    tag: Option<&ProcessTag>,
    started: Option<std::time::SystemTime>,
    adopted_by: Option<u32>,
    bound: Duration,
) -> anyhow::Result<()> {
    let began = Instant::now();
    loop {
        let census = incarnation_census(session, tag, started, adopted_by)?;
        if census.is_settled() {
            return Ok(());
        }
        if began.elapsed() > bound {
            anyhow::bail!("exit not proven after {bound:?}: {}", describe(&census));
        }
        if !census.groups.is_empty() || !census.tagged.is_empty() {
            signal_census(&census, libc::SIGKILL)?;
            if let Some(session) = session {
                signal_group(session, libc::SIGKILL)?;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(not(unix))]
pub async fn teardown_incarnation(
    _session: Option<u32>,
    _tag: Option<&ProcessTag>,
    _started: Option<std::time::SystemTime>,
    _adopted_by: Option<u32>,
    _bound: Duration,
) -> anyhow::Result<()> {
    Ok(())
}

impl std::fmt::Debug for ProcessBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessBackend")
            .field("available", &self.available)
            .finish()
    }
}

#[async_trait::async_trait]
impl MicrovmBackend for ProcessBackend {
    async fn run(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunResult> {
        if !self.available {
            anyhow::bail!("ProcessBackend is not available: git not found in PATH");
        }

        if spec.cmd.is_empty() {
            anyhow::bail!("No command specified in MicrovmRunSpec");
        }

        // 1. Log resource limit warnings if configured
        if spec.cpus > 0 || spec.mem_mib > 0 {
            tracing::warn!(
                cpus = spec.cpus,
                mem_mib = spec.mem_mib,
                "Process isolation mode does not enforce resource limits (cpus={}, mem={}MiB). Consider using microvm mode for resource enforcement.",
                spec.cpus,
                spec.mem_mib
            );
        }

        let workspace_root = Self::resolve_workspace_root(spec)?;
        let command_path = Self::resolve_command_path(&workspace_root, &spec.cmd[0])?;

        // 2. Spawn child process in the workspace directory
        let mut cmd = tokio::process::Command::new(&command_path);
        cmd.args(&spec.cmd[1..]);
        cmd.current_dir(&workspace_root);
        Self::apply_guarded_environment(&mut cmd, spec, &workspace_root)?;

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        // 3. Set process group for clean kill
        Self::configure_process_group(&mut cmd);

        let start_time = Instant::now();

        tracing::debug!(
            cmd = ?spec.cmd,
            cwd = %workspace_root.display(),
            "Spawning child process"
        );

        let child = cmd.spawn()?;

        // 4. Wait for completion
        let output = child.wait_with_output().await?;
        let duration = start_time.elapsed();

        // 5. Return result
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        tracing::info!(
            exit_code = exit_code,
            duration_ms = duration.as_millis() as u64,
            "Process run completed"
        );

        Ok(RunResult {
            exit_code,
            stdout,
            stderr,
            duration,
        })
    }

    async fn start(&self, spec: &MicrovmRunSpec) -> anyhow::Result<RunHandle> {
        if !self.available {
            anyhow::bail!("ProcessBackend is not available: git not found in PATH");
        }

        if spec.cmd.is_empty() {
            anyhow::bail!("No command specified in MicrovmRunSpec");
        }

        // Log resource limit warnings
        if spec.cpus > 0 || spec.mem_mib > 0 {
            tracing::warn!(
                cpus = spec.cpus,
                mem_mib = spec.mem_mib,
                "Process isolation mode does not enforce resource limits"
            );
        }

        let workspace_root = Self::resolve_workspace_root(spec)?;
        let command_path = Self::resolve_command_path(&workspace_root, &spec.cmd[0])?;

        let mut cmd = tokio::process::Command::new(&command_path);
        cmd.args(&spec.cmd[1..]);
        cmd.current_dir(&workspace_root);
        Self::apply_guarded_environment(&mut cmd, spec, &workspace_root)?;

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        tracing::debug!(
            cmd = ?spec.cmd,
            cwd = %workspace_root.display(),
            "Starting long-running process"
        );

        Self::spawn_group(cmd)
    }

    async fn kill(&self, handle: &RunHandle) -> anyhow::Result<ChildReapReceipt> {
        let child_pid = match &handle.inner {
            RunHandleInner::Process { child_pid, .. } => *child_pid,
            _ => anyhow::bail!("Invalid handle for ProcessBackend"),
        };

        if let Some(receipt) = handle.current_reap_receipt()? {
            return Ok(receipt);
        }

        tracing::info!(child_pid = child_pid, "Killing process");

        #[cfg(unix)]
        {
            let receipt = match Self::termination_target(handle)? {
                Target::Incarnation {
                    session,
                    tag,
                    started,
                } => Self::kill_incarnation(handle, session, tag.as_ref(), started).await?,
                Target::Group(target) => {
                    super::terminate_handle_target(
                        handle,
                        target,
                        GRACEFUL_CHILD_REAP_TIMEOUT,
                        FORCED_CHILD_REAP_TIMEOUT,
                    )
                    .await?
                }
            };
            tracing::info!(child_pid, "Process killed and reaped");
            return Ok(receipt);
        }

        #[cfg(not(unix))]
        {
            let pid = child_pid.to_string();
            let graceful = tokio::process::Command::new("taskkill")
                .args(["/T", "/PID", &pid])
                .output()
                .await?;
            if graceful.status.success() {
                if let Ok(receipt) = handle.wait_for_reap(GRACEFUL_CHILD_REAP_TIMEOUT).await {
                    return Ok(receipt);
                }
            }
            let forced = tokio::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid])
                .output()
                .await?;
            if !forced.status.success() {
                anyhow::bail!(
                    "taskkill failed for process {child_pid}: {}",
                    String::from_utf8_lossy(&forced.stderr)
                );
            }
        }

        #[cfg(not(unix))]
        let receipt = handle
            .wait_for_reap(FORCED_CHILD_REAP_TIMEOUT)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "process {child_pid} did not publish a reap receipt after forced termination: {error}"
                )
            })?;
        #[cfg(not(unix))]
        {
            tracing::info!(child_pid = child_pid, "Process killed and reaped");
            Ok(receipt)
        }
    }

    fn probe(&self) -> bool {
        self.available
    }

    fn name(&self) -> &str {
        "process"
    }

    fn isolation_level(&self) -> IsolationLevel {
        IsolationLevel::Process
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_process_backend_new() {
        let backend = ProcessBackend::new();
        // git should be available in most dev environments
        let debug = format!("{:?}", backend);
        assert!(debug.contains("ProcessBackend"));
    }

    #[test]
    fn test_process_backend_name() {
        let backend = ProcessBackend::new();
        assert_eq!(backend.name(), "process");
    }

    #[test]
    fn test_process_backend_isolation_level() {
        let backend = ProcessBackend::new();
        assert_eq!(backend.isolation_level(), IsolationLevel::Process);
    }

    #[test]
    fn test_process_backend_probe() {
        let backend = ProcessBackend::new();
        // Should be true if git is in PATH
        if find_binary("git").is_some() {
            assert!(backend.probe());
        }
    }

    #[tokio::test]
    async fn test_process_backend_run_echo() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            // Skip if git not available
            return;
        }
        let temp = TempDir::new().unwrap();

        let spec = MicrovmRunSpec {
            image: String::new(), // Not used by process backend
            cmd: vec!["echo".to_string(), "hello world".to_string()],
            env: vec![],
            mount_host_dir: temp.path().to_path_buf(),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello world"));
        assert!(result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_process_backend_run_with_env() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["env".to_string()],
            env: vec![("G5_TEST_VAR".to_string(), "test_value".to_string())],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("G5_TEST_VAR=test_value"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_process_backend_clears_parent_env() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        std::env::set_var("G5_PARENT_SECRET", "should-not-leak");
        let temp = TempDir::new().unwrap();
        let canonical = std::fs::canonicalize(temp.path()).unwrap();
        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "printf '%s\\n%s\\n%s' \"$G5_PARENT_SECRET\" \"$HOME\" \"$TMPDIR\"".to_string(),
            ],
            env: vec![],
            mount_host_dir: temp.path().to_path_buf(),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await.unwrap();
        std::env::remove_var("G5_PARENT_SECRET");

        assert_eq!(result.exit_code, 0);
        let lines: Vec<&str> = result.stdout.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "");
        assert!(lines[1].starts_with(canonical.to_string_lossy().as_ref()));
        assert!(lines[2].starts_with(canonical.to_string_lossy().as_ref()));
    }

    #[tokio::test]
    async fn test_process_backend_rejects_extra_mounts() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["echo".to_string(), "blocked".to_string()],
            env: vec![],
            mount_host_dir: temp.path().to_path_buf(),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![crate::isolation::MicrovmMount::read_write(
                temp.path().join("cache"),
                "/cache",
            )],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not support extra mounts"));
    }

    #[tokio::test]
    async fn test_process_backend_run_nonzero_exit() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["false".to_string()],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await.unwrap();
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_process_backend_run_empty_cmd() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec![],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_backend_run_stderr() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo error_msg >&2".to_string(),
            ],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let result = backend.run(&spec).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stderr.contains("error_msg"));
    }

    #[tokio::test]
    async fn test_process_backend_start_and_handle() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["sleep".to_string(), "0.1".to_string()],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let handle = backend.start(&spec).await.unwrap();
        assert_eq!(handle.backend_type, BackendType::Process);
        assert!(handle.id.starts_with("process-"));
    }

    #[tokio::test]
    async fn test_process_backend_start_and_kill() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["sleep".to_string(), "60".to_string()],
            env: vec![],
            mount_host_dir: PathBuf::from("/tmp"),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let handle = backend.start(&spec).await.unwrap();
        assert!(handle.id.starts_with("process-"));

        // Kill should succeed
        let kill_result = backend.kill(&handle).await;
        assert!(kill_result.is_ok());
    }

    /// backend_selection D15/D18: signal dispatch is not teardown success.
    /// The real child delays its TERM exit, so `kill` can return only after
    /// the detached waiter publishes the post-`wait()` reap receipt. Both
    /// receipt shapes are contract-valid: the trap's graceful `exit 0`, or a
    /// signal exit (`None`) when a loaded scheduler pushes the trap past the
    /// graceful bound and `kill` correctly escalates to SIGKILL.
    #[cfg(unix)]
    #[tokio::test]
    async fn process_backend_kill_cannot_succeed_before_waiter_reap() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let ready = temp.path().join("ready");
        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "trap 'sleep 0.25; exit 0' TERM; : > \"$READY_FILE\"; while :; do sleep 60; done"
                    .to_string(),
            ],
            env: vec![(
                "READY_FILE".to_string(),
                ready.to_string_lossy().into_owned(),
            )],
            mount_host_dir: temp.path().to_path_buf(),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let handle = backend.start(&spec).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("real child installs its TERM handler");

        let started = Instant::now();
        let receipt = backend
            .kill(&handle)
            .await
            .expect("kill waits for the real waiter receipt");
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(200),
            "kill returned before the child completed its delayed TERM exit"
        );
        // Requiring `Some(0)` unconditionally races the scheduler (issue
        // #2108): the trap's delayed exit may miss the graceful bound, in
        // which case escalation reaps a signal exit instead.
        match receipt.exit_code() {
            Some(0) => {}
            None => assert!(
                elapsed >= GRACEFUL_CHILD_REAP_TIMEOUT,
                "signal-exit receipt published before the graceful SIGTERM window expired"
            ),
            Some(code) => {
                panic!("child exited with unexpected code {code} instead of the trap's exit 0")
            }
        }
    }

    /// backend_selection D13/D18: an already-published waiter receipt is
    /// stable evidence. Repeated kill calls must return it immediately rather
    /// than signalling a recycled PID or waiting on a synthetic pending seam.
    #[cfg(unix)]
    #[tokio::test]
    async fn process_backend_kill_is_idempotent_after_waiter_reap() {
        let backend = ProcessBackend::new();
        if !backend.probe() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let spec = MicrovmRunSpec {
            image: String::new(),
            cmd: vec!["sh".to_string(), "-c".to_string(), "exit 7".to_string()],
            env: vec![],
            mount_host_dir: temp.path().to_path_buf(),
            mount_guest_dir: "/work".to_string(),
            cpus: 0,
            mem_mib: 0,
            extra_mounts: vec![],
            port_maps: vec![],

            scratch_disk: None,
            block_workload: false,
            nested_virtualization: false,
            guest_restore: None,
        };

        let handle = backend.start(&spec).await.unwrap();
        let published = handle
            .wait_for_reap(Duration::from_secs(2))
            .await
            .expect("sole waiter publishes the natural exit");
        let first = backend
            .kill(&handle)
            .await
            .expect("already-reaped kill returns the receipt");
        let second = backend
            .kill(&handle)
            .await
            .expect("repeated kill remains idempotent");
        assert_eq!(published.exit_code(), Some(7));
        assert_eq!(first, published);
        assert_eq!(second, published);
    }

    #[tokio::test]
    async fn test_process_backend_kill_wrong_handle_type() {
        let backend = ProcessBackend::new();
        let handle = RunHandle {
            id: "cloud-hypervisor-test".to_string(),
            backend_type: BackendType::CloudHypervisor,
            vmm_loopback_addr: None,
            inner: RunHandleInner::Microvm {
                vm_name: "test".to_string(),
                child_pid: None,
            },
            reap_monitor: crate::isolation::pending_child_reap_monitor(),
            console_tail: None,
            console_relay_handles: std::sync::Mutex::new(Vec::new()),
        };
        let result = backend.kill(&handle).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid handle"));
    }
}
