//! Bounded termination of a child-owned process group or terminal session.
//!
//! The caller owns the child waiter: a missing process group (ESRCH) is not
//! evidence that its leader has been reaped. A waiter may also check that all
//! processes in a terminal session have exited.

/// Per-execution orphan adoption and reap receipts for descendants that leave
/// their original process group/session. Complements the PTY session target.
#[cfg(target_os = "linux")]
pub mod execution_owner;

/// Set in an agent supervisor's environment by a Linux execution owner that
/// honours a release. Without it the supervisor always exits 0.
pub const EXECUTION_RELEASE_ENV: &str = "G5D_EXECUTION_OWNER_RELEASE";
/// Supervisor exit status asking its execution owner to leave the remaining
/// descendants running instead of reaping them, because the provider ended
/// the turn itself. The owner reports it to the dispatcher as success.
pub const EXECUTION_RELEASE_EXIT_STATUS: i32 = 117;

use std::{io, time::Duration};

use async_trait::async_trait;
use thiserror::Error;

#[cfg(unix)]
pub const KILL_SIGNAL: i32 = libc::SIGKILL;
// Group signalling is a no-op on non-Unix; the child waiter still kills the leader.
#[cfg(not(unix))]
pub const KILL_SIGNAL: i32 = 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationTarget {
    ProcessGroup(u32),
    Session(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct TerminationPolicy {
    pub first_signal: i32,
    pub grace: Duration,
    pub kill_deadline: Duration,
    /// Some providers require SIGKILL for descendants even if the leader
    /// exits during the grace window.
    pub always_kill_after_grace: bool,
}

#[derive(Debug, Error)]
pub enum TerminationError {
    #[error("process group or session ID must be between 1 and i32::MAX")]
    InvalidTarget,
    #[error("failed to signal {target:?} with signal {signal}: {source}")]
    Signal {
        target: TerminationTarget,
        signal: i32,
        #[source]
        source: io::Error,
    },
    #[error("failed to confirm child reap: {0}")]
    Wait(#[source] io::Error),
    #[error("processes in {target:?} were not confirmed reaped within {deadline:?} after SIGKILL")]
    Deadline {
        target: Option<TerminationTarget>,
        deadline: Duration,
    },
}

#[async_trait]
pub trait ReapWaiter: Send {
    type Receipt: Send;

    /// Return a receipt only after the leader has actually been reaped (and,
    /// for a session, its other groups are gone). `None` means time expired.
    async fn wait_for_reap(&mut self, bound: Duration) -> io::Result<Option<Self::Receipt>>;

    /// Optional child-handle backstop after group SIGKILL. This must not be
    /// used as a substitute for `wait_for_reap`.
    fn after_kill(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Signal, wait for graceful exit, escalate and require positive reap proof.
/// A missing target falls back to `after_kill` and the kill deadline.
pub async fn terminate<W: ReapWaiter>(
    target: Option<TerminationTarget>,
    policy: TerminationPolicy,
    waiter: &mut W,
) -> Result<W::Receipt, TerminationError> {
    terminate_with(target, policy, waiter, signal).await
}

async fn terminate_with<W: ReapWaiter>(
    target: Option<TerminationTarget>,
    policy: TerminationPolicy,
    waiter: &mut W,
    mut signal_target: impl FnMut(TerminationTarget, i32) -> Result<(), TerminationError>,
) -> Result<W::Receipt, TerminationError> {
    if let Some(target) = target {
        if target.id() == 0 || target.id() > i32::MAX as u32 {
            return Err(TerminationError::InvalidTarget);
        }
        if policy.grace.is_zero() || policy.first_signal == KILL_SIGNAL {
            if let Err(error) = signal_target(target, KILL_SIGNAL) {
                return Err(finish_failed_signal(error, policy.kill_deadline, waiter).await);
            }
        } else {
            if let Err(error) = signal_target(target, policy.first_signal) {
                if let Err(kill_error) = signal_target(target, KILL_SIGNAL) {
                    tracing::warn!(%kill_error, "SIGKILL after initial signal failure also failed");
                }
                return Err(finish_failed_signal(error, policy.kill_deadline, waiter).await);
            }
            let grace_started = tokio::time::Instant::now();
            let receipt = match waiter.wait_for_reap(policy.grace).await {
                Ok(receipt) => receipt,
                // An agent provider always gets a group SIGKILL after its
                // fixed grace, even if the leader's first wait failed.
                Err(error) if policy.always_kill_after_grace => {
                    tracing::warn!(%error, "graceful child reap failed; continuing to SIGKILL");
                    None
                }
                Err(error) => {
                    // A failed grace wait cannot justify leaving a group that
                    // may ignore the first signal alive.
                    if let Err(signal_error) = signal_target(target, KILL_SIGNAL) {
                        return Err(finish_failed_signal(
                            signal_error,
                            policy.kill_deadline,
                            waiter,
                        )
                        .await);
                    }
                    let backstop_result = waiter.after_kill();
                    if let Err(backstop_error) = backstop_result {
                        tracing::warn!(%backstop_error, "direct child kill backstop failed");
                    }
                    return match waiter.wait_for_reap(policy.kill_deadline).await {
                        Ok(Some(receipt)) => Ok(receipt),
                        Ok(None) => Err(TerminationError::Wait(error)),
                        Err(reap_error) => {
                            tracing::warn!(%reap_error, "forced child reap failed after graceful wait error");
                            Err(TerminationError::Wait(error))
                        }
                    };
                }
            };
            if !policy.always_kill_after_grace {
                if let Some(receipt) = receipt {
                    return Ok(receipt);
                }
            } else {
                tokio::time::sleep(policy.grace.saturating_sub(grace_started.elapsed())).await;
            }
            if let Err(error) = signal_target(target, KILL_SIGNAL) {
                return Err(finish_failed_signal(error, policy.kill_deadline, waiter).await);
            }
            if let Some(receipt) = receipt {
                waiter.after_kill().map_err(TerminationError::Wait)?;
                return Ok(receipt);
            }
        }
    }
    waiter.after_kill().map_err(TerminationError::Wait)?;
    waiter
        .wait_for_reap(policy.kill_deadline)
        .await
        .map_err(TerminationError::Wait)?
        .ok_or(TerminationError::Deadline {
            target,
            deadline: policy.kill_deadline,
        })
}

async fn finish_failed_signal<W: ReapWaiter>(
    error: TerminationError,
    kill_deadline: Duration,
    waiter: &mut W,
) -> TerminationError {
    if let Err(backstop_error) = waiter.after_kill() {
        tracing::warn!(%backstop_error, "direct child kill backstop failed");
    }
    match waiter.wait_for_reap(kill_deadline).await {
        Ok(Some(_)) => {}
        Ok(None) => tracing::warn!("reap not confirmed after a signal error"),
        Err(reap_error) => tracing::warn!(%reap_error, "reap check failed after a signal error"),
    }
    error
}

impl TerminationTarget {
    fn id(self) -> u32 {
        match self {
            Self::ProcessGroup(id) | Self::Session(id) => id,
        }
    }
}

/// Signal a child-owned group. ESRCH is harmless, but only a reap receipt
/// proves that the child exited.
pub fn signal(target: TerminationTarget, signal: i32) -> Result<(), TerminationError> {
    if target.id() == 0 || target.id() > i32::MAX as u32 {
        return Err(TerminationError::InvalidTarget);
    }
    signal_inner(target, signal).map_err(|source| TerminationError::Signal {
        target,
        signal,
        source,
    })
}

#[cfg(target_os = "linux")]
fn signal_inner(target: TerminationTarget, signal: i32) -> io::Result<()> {
    match target {
        TerminationTarget::ProcessGroup(id) => signal_group(id, signal),
        TerminationTarget::Session(id) => {
            signal_session_groups(session_process_groups(id)?, signal, signal_group)
        }
    }
}

#[cfg(target_os = "linux")]
fn signal_session_groups(
    groups: impl IntoIterator<Item = u32>,
    signal: i32,
    mut signal_group: impl FnMut(u32, i32) -> io::Result<()>,
) -> io::Result<()> {
    let mut first_error = None;
    for group in groups {
        if let Err(error) = signal_group(group, signal) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn signal_inner(target: TerminationTarget, signal: i32) -> io::Result<()> {
    match target {
        TerminationTarget::ProcessGroup(id) => signal_group(id, signal),
        TerminationTarget::Session(id) => {
            let mut groups = session_process_groups(id)?;
            // The leader's own group is signalled even when enumeration saw
            // nothing: a session that only has its leader left is still live.
            groups.insert(id);
            let mut first_error = None;
            for group in groups {
                if let Err(error) = signal_group(group, signal) {
                    first_error.get_or_insert(error);
                }
            }
            first_error.map_or(Ok(()), Err)
        }
    }
}

/// Without `/proc`, the live process groups of a session are found by asking
/// the kernel for every visible process's session (`getsid`), listing the
/// processes with `ps`. Used by the local process-mode host on macOS, whose
/// dispatcher children put previews and terminals in groups of their own.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn session_process_groups(session_id: u32) -> io::Result<std::collections::HashSet<u32>> {
    if session_id == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "session 0"));
    }
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,pgid=,stat="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "ps failed while listing session {session_id}: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let mut groups = std::collections::HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(group), Some(state)) = (
            fields.next().and_then(|pid| pid.parse::<i32>().ok()),
            fields.next().and_then(|group| group.parse::<u32>().ok()),
            fields.next(),
        ) else {
            continue;
        };
        if state.starts_with('Z') {
            continue;
        }
        // SAFETY: getsid only reads kernel process state for `pid`.
        let session = unsafe { libc::getsid(pid) };
        if session > 0 && session as u32 == session_id && group != 0 {
            groups.insert(group);
        }
    }
    Ok(groups)
}

/// What the host can see of the processes that may belong to one
/// incarnation of a process-mode workspace
/// (`g5.runtime.boot.backend.process-local`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IncarnationCensus {
    /// Live process groups of the incarnation's session.
    pub groups: std::collections::HashSet<u32>,
    /// Live processes whose environment carries the incarnation's marker,
    /// wherever their session is (terminals and detached applications).
    pub tagged: std::collections::HashSet<u32>,
    /// Live processes of this user, started no earlier than the incarnation,
    /// whose environment could not be read (non-dumpable on Linux, a failed or
    /// truncated argument read on macOS). They may or may not belong to the
    /// incarnation; while any exists, its exit cannot be proven.
    pub unknown: Vec<u32>,
}

impl IncarnationCensus {
    /// Nothing attributable is left and nothing is uncertain.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.groups.is_empty() && self.tagged.is_empty() && self.unknown.is_empty()
    }
}

/// Slack before `since` within which a process still counts as started
/// after the incarnation, covering clock granularity of the start-time
/// sources.
const START_SLACK: Duration = Duration::from_secs(2);

fn started_after(started: std::time::SystemTime, since: Option<std::time::SystemTime>) -> bool {
    since.is_none_or(|since| started + START_SLACK >= since)
}

/// The environment entries that mark a process as the incarnation's: the
/// marker itself, and the same value under the outer-marker key a nested
/// host stamps on its own dispatchers (`{key}_OUTER`), so an incarnation
/// that runs a whole previewed instance still owns that instance's work.
#[cfg(unix)]
fn marker_needles(key: &str, value: &str) -> [Vec<u8>; 2] {
    [
        format!("{key}={value}").into_bytes(),
        format!("{key}_OUTER={value}").into_bytes(),
    ]
}

#[cfg(unix)]
fn carries_marker(environment: &[u8], needles: &[Vec<u8>; 2]) -> bool {
    environment
        .split(|byte| *byte == 0)
        .any(|variable| needles.iter().any(|needle| variable == needle.as_slice()))
}

/// One live process as the host observed it, before attribution.
#[cfg(unix)]
struct Observed {
    pid: u32,
    parent: u32,
    group: u32,
    session: u32,
    same_user: bool,
    started: std::time::SystemTime,
    environment: EnvironmentRead,
}

#[cfg(unix)]
enum EnvironmentRead {
    Tagged,
    Untagged,
    /// The read failed or came back incomplete: this process may be ours.
    Unreadable,
    /// The kernel returned the arguments without an environment. macOS does
    /// this for every platform and hardened-runtime binary read by an
    /// unprivileged user (every shell, `sleep`, browsers), the incarnation's
    /// own children included, so it is no evidence either way. Linux's
    /// `/proc` never withholds, so only the macOS census constructs it.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Withheld,
}

/// Attribute observed processes to the incarnation. A process whose
/// environment could not be read is a **member** when its parent chain reaches
/// the incarnation's session leader (ancestry is ownership), **unknown** when
/// the chain reaches this host (an orphan of some incarnation of this host,
/// which Linux keeps attributable through the child-subreaper rule) or when no
/// host is given (a restarted host settling what a dead one left), and not
/// ours otherwise. Unknowns are further bounded by the incarnation's start. A
/// process whose environment the kernel withholds is a member by session or
/// ancestry alone and otherwise not ours: a hardened process that left the
/// session and lost its ancestry is the macOS form of the documented limit.
#[cfg(unix)]
fn classify(
    observed: Vec<Observed>,
    session: Option<u32>,
    host: Option<u32>,
    since: Option<std::time::SystemTime>,
) -> IncarnationCensus {
    let parents: std::collections::HashMap<u32, u32> = observed
        .iter()
        .map(|process| (process.pid, process.parent))
        .collect();
    let reaches = |mut pid: u32, target: u32| -> bool {
        for _ in 0..256 {
            if pid == target {
                return true;
            }
            match parents.get(&pid) {
                Some(parent) if *parent != pid && *parent > 1 => pid = *parent,
                _ => return false,
            }
        }
        false
    };
    let mut census = IncarnationCensus::default();
    for process in &observed {
        if session == Some(process.session) && process.group != 0 {
            census.groups.insert(process.group);
        }
        match process.environment {
            EnvironmentRead::Tagged => {
                census.tagged.insert(process.pid);
            }
            EnvironmentRead::Untagged => {}
            EnvironmentRead::Withheld => {
                if process.same_user && session.is_some_and(|leader| reaches(process.pid, leader)) {
                    census.tagged.insert(process.pid);
                }
            }
            EnvironmentRead::Unreadable if !process.same_user => {}
            EnvironmentRead::Unreadable => {
                if session.is_some_and(|leader| reaches(process.pid, leader)) {
                    census.tagged.insert(process.pid);
                } else if host.is_none_or(|host| reaches(process.pid, host))
                    && started_after(process.started, since)
                {
                    census.unknown.push(process.pid);
                }
            }
        }
    }
    census
}

/// Enumerate the incarnation's observable processes, other than the caller:
/// the groups of `session` (when given), every process carrying `key=value`,
/// and the unknowns described on [`IncarnationCensus`], attributed by
/// [`classify`]. `host` is the pid under which the incarnation's processes
/// are reparented when they are orphaned (this host, a child subreaper on
/// Linux); `None` for a host settling another host's incarnations. Processes
/// that scrubbed their environment are not found; that is the documented
/// limit of a host process without a cgroup or VM boundary. Processes whose
/// environment is merely unreadable are attributed, not assumed absent.
#[cfg(target_os = "linux")]
pub fn incarnation_census(
    session: Option<u32>,
    host: Option<u32>,
    key: &str,
    value: &str,
    since: Option<std::time::SystemTime>,
) -> io::Result<IncarnationCensus> {
    let needles = marker_needles(key, value);
    let own = std::process::id();
    // SAFETY: geteuid has no failure modes and touches no memory.
    let uid = unsafe { libc::geteuid() };
    let boot = linux_boot_time()?;
    // SAFETY: sysconf reads a constant.
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let mut observed = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == own {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((state, parent, group, sid, start_ticks)) = parse_proc_stat_full(&stat) else {
            continue;
        };
        // A thread group whose leader exited is reported as a zombie while its
        // other threads keep running (`pthread_exit` from `main`): the leader's
        // state and environment say nothing about them, so the group is alive
        // while any task is, and its environment is read from a live task.
        let mut environ_path = entry.path().join("environ");
        if state == 'Z' || state == 'X' {
            match live_task(&entry.path()) {
                Some(task) => environ_path = task.join("environ"),
                None => continue,
            }
        }
        let environment = match std::fs::read(&environ_path) {
            Ok(environ) => {
                if carries_marker(&environ, &needles) {
                    EnvironmentRead::Tagged
                } else {
                    EnvironmentRead::Untagged
                }
            }
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                EnvironmentRead::Unreadable
            }
            // Vanished between listing and read.
            Err(_) => continue,
        };
        observed.push(Observed {
            pid,
            parent,
            group,
            session: sid,
            same_user: proc_real_uid(&entry.path()) == Some(uid),
            started: boot + Duration::from_secs_f64(start_ticks as f64 / ticks_per_second),
            environment,
        });
    }
    Ok(classify(observed, session, host, since))
}

/// The `/proc/<pid>/task/<tid>` directory of a task of `proc_dir`'s thread
/// group that is not a zombie, when one exists.
#[cfg(target_os = "linux")]
fn live_task(proc_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(proc_dir.join("task"))
        .ok()?
        .filter_map(Result::ok)
        .map(|task| task.path())
        .find(|task| {
            std::fs::read_to_string(task.join("stat"))
                .ok()
                .and_then(|stat| parse_proc_stat_full(&stat))
                .is_some_and(|(state, ..)| state != 'Z' && state != 'X')
        })
}

/// Reap zombies that were reparented to this process because it is a child
/// subreaper: orphans of an incarnation whose own parent exited. Only
/// zombies that are not direct children of this process's own session are
/// touched (`keep_for_owner` names pids another owner will wait for), so the
/// exit statuses of children this process spawned itself stay with their
/// owners. Returns how many were reaped.
#[cfg(target_os = "linux")]
pub fn reap_reparented_zombies(keep_for_owner: impl Fn(u32) -> bool) -> io::Result<usize> {
    let me = std::process::id();
    // SAFETY: getsid(0) reads this process's own session id.
    let my_session = unsafe { libc::getsid(0) } as u32;
    let mut reaped = 0;
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((state, parent, _group, session, _start)) = parse_proc_stat_full(&stat) else {
            continue;
        };
        if state != 'Z' || parent != me || session == my_session || keep_for_owner(pid) {
            continue;
        }
        let mut status = 0;
        // SAFETY: waitpid on a zombie this process is now the parent of; a
        // pid that was already reaped returns ECHILD, which is harmless.
        if unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) } == pid as i32 {
            reaped += 1;
        }
    }
    Ok(reaped)
}

#[cfg(not(target_os = "linux"))]
pub fn reap_reparented_zombies(_keep_for_owner: impl Fn(u32) -> bool) -> io::Result<usize> {
    Ok(0)
}

#[cfg(target_os = "linux")]
fn linux_boot_time() -> io::Result<std::time::SystemTime> {
    let stat = std::fs::read_to_string("/proc/stat")?;
    let seconds = stat
        .lines()
        .find_map(|line| line.strip_prefix("btime "))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .ok_or_else(|| io::Error::other("/proc/stat has no btime"))?;
    Ok(std::time::UNIX_EPOCH + Duration::from_secs(seconds))
}

#[cfg(target_os = "linux")]
fn proc_real_uid(proc_dir: &std::path::Path) -> Option<u32> {
    let status = std::fs::read_to_string(proc_dir.join("status")).ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

/// `(state, parent, group, session, start_ticks)` from `/proc/<pid>/stat`.
#[cfg(target_os = "linux")]
fn parse_proc_stat_full(stat: &str) -> Option<(char, u32, u32, u32, u64)> {
    let command_end = stat.rfind(')')?;
    let fields: Vec<&str> = stat[command_end + 1..].split_whitespace().collect();
    // Fields after the command: state, ppid, pgrp, session, ..., starttime is
    // the twentieth (index 19).
    let state = fields.first()?.chars().next()?;
    Some((
        state,
        fields.get(1)?.parse().ok()?,
        fields.get(2)?.parse().ok()?,
        fields.get(3)?.parse().ok()?,
        fields.get(19)?.parse().ok()?,
    ))
}

/// See the Linux variant. Without `/proc`, `ps` lists this user's processes
/// with parent, group, state and elapsed time, `getsid` gives the session, and
/// `KERN_PROCARGS2` reads each environment; a failed or truncated read of a
/// live same-user process is attributed by ancestry or recorded as unknown
/// rather than assumed untagged, and an environment the kernel withheld
/// (platform and hardened binaries) is attributed by session and ancestry.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn incarnation_census(
    session: Option<u32>,
    host: Option<u32>,
    key: &str,
    value: &str,
    since: Option<std::time::SystemTime>,
) -> io::Result<IncarnationCensus> {
    let needles = marker_needles(key, value);
    let own = std::process::id();
    // SAFETY: geteuid has no failure modes and touches no memory.
    let uid = unsafe { libc::geteuid() };
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,uid=,pgid=,stat=,etime="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "ps failed while listing processes: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let now = std::time::SystemTime::now();
    let mut observed = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(parent), Some(process_uid), Some(group), Some(state), Some(etime)) = (
            fields.next().and_then(|pid| pid.parse::<u32>().ok()),
            fields.next().and_then(|ppid| ppid.parse::<u32>().ok()),
            fields.next().and_then(|uid| uid.parse::<u32>().ok()),
            fields.next().and_then(|group| group.parse::<u32>().ok()),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        if pid == own || state.starts_with('Z') {
            continue;
        }
        // SAFETY: getsid only reads kernel process state for `pid`.
        let sid = unsafe { libc::getsid(pid as i32) };
        let environment = match procargs_environment(pid) {
            Ok(ProcessArguments::Visible(environment)) => {
                if carries_marker(&environment, &needles) {
                    EnvironmentRead::Tagged
                } else {
                    EnvironmentRead::Untagged
                }
            }
            Ok(ProcessArguments::Withheld) => EnvironmentRead::Withheld,
            Ok(ProcessArguments::Gone) => continue,
            Err(_) => EnvironmentRead::Unreadable,
        };
        observed.push(Observed {
            pid,
            parent,
            group,
            session: if sid > 0 { sid as u32 } else { 0 },
            same_user: process_uid == uid,
            started: now - Duration::from_secs(parse_etime_seconds(etime)),
            environment,
        });
    }
    Ok(classify(observed, session, host, since))
}

/// `[[dd-]hh:]mm:ss` from `ps -o etime`, in seconds.
#[cfg(all(unix, not(target_os = "linux")))]
fn parse_etime_seconds(etime: &str) -> u64 {
    let (days, clock) = match etime.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().unwrap_or(0), clock),
        None => (0, etime),
    };
    let mut seconds = 0u64;
    for part in clock.split(':') {
        seconds = seconds * 60 + part.parse::<u64>().unwrap_or(0);
    }
    days * 86_400 + seconds
}

/// What `KERN_PROCARGS2` said about a live process.
#[cfg(all(unix, not(target_os = "linux")))]
enum ProcessArguments {
    /// The strings in which the environment is found.
    Visible(Vec<u8>),
    /// The kernel returned the arguments alone.
    Withheld,
    /// The process exited between listing and reading.
    Gone,
}

/// The environment block of `pid` through `KERN_PROCARGS2`; `Err` when the
/// process exists but its arguments cannot be read completely.
#[cfg(all(unix, not(target_os = "linux")))]
fn procargs_environment(pid: u32) -> io::Result<ProcessArguments> {
    let mut name = [libc::CTL_KERN, libc::KERN_ARGMAX, 0];
    let mut argmax: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    // SAFETY: valid name array and output buffer for an integer sysctl.
    if unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            2,
            &mut argmax as *mut libc::c_int as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0u8; argmax.max(4096) as usize];
    let mut size = buffer.len();
    let mut name = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    // SAFETY: valid name array and a buffer of `size` bytes.
    if unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            3,
            buffer.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        let error = io::Error::last_os_error();
        // SAFETY: signal zero only probes process existence.
        if unsafe { libc::kill(pid as i32, 0) } != 0 {
            return Ok(ProcessArguments::Gone);
        }
        return Err(error);
    }
    let truncated = size >= buffer.len();
    buffer.truncate(size);
    Ok(match parse_procargs2_environment(&buffer, truncated)? {
        Some(strings) => ProcessArguments::Visible(strings),
        None => ProcessArguments::Withheld,
    })
}

/// The strings of a `KERN_PROCARGS2` reply in which an environment variable
/// may be found: the reply is `argc`, the executable path, padding, `argc`
/// arguments, then the environment strings, and the arguments are returned
/// with the environment because the two cannot be told apart when an argument
/// is empty (review R21). A reply that filled the buffer may have lost the
/// end and is an error, so the census records the process as unknown. A reply
/// whose strings end with the `argc` arguments is `None`: XNU returns argv
/// alone, successfully, for every platform and hardened-runtime binary an
/// unprivileged caller reads (macOS 26.5: `/bin/sh`, `sleep`, `zsh`,
/// browsers), so the absence of an environment says nothing about the marker.
pub fn parse_procargs2_environment(buffer: &[u8], truncated: bool) -> io::Result<Option<Vec<u8>>> {
    if truncated {
        return Err(io::Error::other(
            "KERN_PROCARGS2 result filled the buffer; its environment may be incomplete",
        ));
    }
    if buffer.len() < 4 {
        return Err(io::Error::other("short KERN_PROCARGS2 result"));
    }
    let argc = u32::from_ne_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
    let mut rest = &buffer[4..];
    // Executable path, then padding NULs.
    let path_end = rest
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(rest.len());
    rest = &rest[path_end..];
    while rest.first() == Some(&0) {
        rest = &rest[1..];
    }
    // The arguments and the environment: `argc` NUL-terminated strings, then
    // the environment strings. An empty argument is a lone NUL, the same byte
    // as the padding, so the boundary between padding and arguments, and
    // hence between arguments and environment, is ambiguous (review R21).
    // The strings are therefore returned whole, arguments included: a marker
    // is found wherever the boundary falls, and a marker that is nowhere in
    // them is absent under every reading. An argument that happens to spell
    // the marker reads as a member, the conservative error.
    let strings = rest;
    // Every real process has an environment (at least its PATH), so a reply
    // whose strings end with the `argc` arguments is the kernel's omission.
    for _ in 0..argc {
        let end = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());
        rest = &rest[(end + 1).min(rest.len())..];
    }
    if rest.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    Ok(Some(strings.to_vec()))
}

#[cfg(not(unix))]
pub fn incarnation_census(
    _session: Option<u32>,
    _host: Option<u32>,
    _key: &str,
    _value: &str,
    _since: Option<std::time::SystemTime>,
) -> io::Result<IncarnationCensus> {
    Ok(IncarnationCensus::default())
}

#[cfg(not(unix))]
fn signal_inner(_target: TerminationTarget, _signal: i32) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn signal_group(id: u32, signal: i32) -> io::Result<()> {
    // SAFETY: callers pass the process group created for their child.
    let result = unsafe { libc::killpg(id as i32, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(error)
}

/// Linux job control can create several process groups in one PTY session.
#[cfg(target_os = "linux")]
pub fn session_process_groups(session_id: u32) -> io::Result<std::collections::HashSet<u32>> {
    if session_id == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "session 0"));
    }
    let mut groups = std::collections::HashSet::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((state, group, session)) = parse_proc_stat_process_group(&stat) else {
            continue;
        };
        if session == session_id && group != 0 && state != 'Z' && state != 'X' {
            groups.insert(group);
        }
    }
    Ok(groups)
}

/// Parse /proc/<pid>/stat after the final `)` in the command field.
pub fn parse_proc_stat_process_group(stat: &str) -> Option<(char, u32, u32)> {
    let (state, _, group, session) = parse_proc_stat(stat)?;
    Some((state, group, session))
}

// Shared by PTY session cleanup and the execution owner's direct-child scan.
fn parse_proc_stat(stat: &str) -> Option<(char, u32, u32, u32)> {
    let command_start = stat.find('(')?;
    let command_end = stat.rfind(')')?;
    if command_end <= command_start {
        return None;
    }
    let mut fields = stat[command_end + 1..].split_whitespace();
    let state = fields.next()?;
    if state.len() != 1 {
        return None;
    }
    let state = state.chars().next()?;
    let parent = fields.next()?.parse::<u32>().ok()?;
    let group = fields.next()?.parse::<u32>().ok()?;
    let session = fields.next()?.parse::<u32>().ok()?;
    Some((state, parent, group, session))
}

/// Wait for a Tokio child; used by short-lived module and agent subprocesses.
pub struct TokioChildWaiter<'a>(pub &'a mut tokio::process::Child);

#[async_trait]
impl ReapWaiter for TokioChildWaiter<'_> {
    type Receipt = std::process::ExitStatus;

    async fn wait_for_reap(&mut self, bound: Duration) -> io::Result<Option<Self::Receipt>> {
        match tokio::time::timeout(bound, self.0.wait()).await {
            Ok(status) => status.map(Some),
            Err(_) => Ok(None),
        }
    }

    fn after_kill(&mut self) -> io::Result<()> {
        let _ = self.0.start_kill();
        Ok(())
    }
}

/// Tokio child adapter with a direct-child backstop if termination fails.
/// The error still reports that descendants may remain alive.
pub async fn terminate_tokio_child(
    target: Option<TerminationTarget>,
    policy: TerminationPolicy,
    child: &mut tokio::process::Child,
) -> Result<std::process::ExitStatus, TerminationError> {
    terminate_with_direct_child_backstop(target, policy, &mut TokioChildWaiter(child), signal).await
}

async fn terminate_with_direct_child_backstop<W: ReapWaiter>(
    target: Option<TerminationTarget>,
    policy: TerminationPolicy,
    waiter: &mut W,
    mut signal_target: impl FnMut(TerminationTarget, i32) -> Result<(), TerminationError>,
) -> Result<W::Receipt, TerminationError> {
    // The target-less path enters the kill wait immediately. Other paths begin
    // their kill deadline when SIGKILL is first attempted.
    let mut kill_deadline = target
        .is_none()
        .then(|| tokio::time::Instant::now() + policy.kill_deadline);
    let result = terminate_with(target, policy, waiter, |target, sent_signal| {
        if sent_signal == KILL_SIGNAL {
            kill_deadline.get_or_insert_with(|| tokio::time::Instant::now() + policy.kill_deadline);
        }
        signal_target(target, sent_signal)
    })
    .await;
    if result.is_err() {
        let _ = waiter.after_kill();
        let deadline =
            kill_deadline.unwrap_or_else(|| tokio::time::Instant::now() + policy.kill_deadline);
        let _ = waiter
            .wait_for_reap(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await;
    }
    result
}

#[cfg(test)]
mod tests {
    /// A tagged thread group whose leader exited is still a member: the
    /// leader reads as a zombie with an unreadable environment while a worker
    /// thread runs on, and the census attributes it through a live task.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_zombie_leader_with_live_threads_is_still_a_tagged_member() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("SKIP: python3 is required for the zombie-leader fixture");
            return;
        }
        // SAFETY: geteuid has no failure modes and touches no memory.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("SKIP: root reads every environment; the hidden-environment fixture needs an unprivileged runner");
            return;
        }
        let value = uuid::Uuid::new_v4().to_string();
        // The interpreter's main thread (the thread-group leader) exits while
        // a worker thread keeps the process alive.
        let mut child = std::process::Command::new("python3")
            .args([
                "-c",
                "import ctypes, threading, time\n\
                 threading.Thread(target=lambda: time.sleep(600)).start()\n\
                 ctypes.CDLL(None).pthread_exit(None)",
            ])
            .env("G5D_TEST_CENSUS_TAG", &value)
            .spawn()
            .unwrap();
        let pid = child.id();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let leader_state = loop {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
            let (state, ..) = super::parse_proc_stat_full(&stat).unwrap();
            if state == 'Z' || std::time::Instant::now() > deadline {
                break state;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(leader_state, 'Z', "the fixture's main thread exited");
        assert!(
            std::fs::read(format!("/proc/{pid}/environ")).is_err(),
            "the zombie leader hides the environment"
        );
        let census =
            super::incarnation_census(None, None, "G5D_TEST_CENSUS_TAG", &value, None).unwrap();
        assert!(census.tagged.contains(&pid), "{census:?}");
        assert!(!census.is_settled());
        // SAFETY: the test owns the fixture process.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        child.wait().unwrap();
        let census =
            super::incarnation_census(None, None, "G5D_TEST_CENSUS_TAG", &value, None).unwrap();
        assert!(census.is_settled(), "{census:?}");
    }

    /// The orphan reaper waits only on zombies reparented to this process
    /// from another session, never on this process's own children. The check
    /// runs in a child process (`reaper_fixture`) because becoming a child
    /// subreaper is process-wide and would adopt other tests' orphans.
    #[cfg(target_os = "linux")]
    #[test]
    fn reaper_waits_only_on_reparented_zombies_of_other_sessions() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process_termination::tests::reaper_fixture",
                "--include-ignored",
            ])
            .env("G5D_TEST_REAPER_FIXTURE", "1")
            .status()
            .unwrap();
        assert!(status.success(), "reaper fixture failed: {status}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "fixture process for reaper_waits_only_on_reparented_zombies_of_other_sessions"]
    fn reaper_fixture() {
        if std::env::var_os("G5D_TEST_REAPER_FIXTURE").is_none() {
            return;
        }
        // SAFETY: prctl with PR_SET_CHILD_SUBREAPER only changes this
        // process's reparenting attribute.
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("orphan.pid");
        // A session leader that leaves a child behind and exits.
        let mut leader = std::process::Command::new("setsid");
        leader.args([
            "sh",
            "-c",
            &format!("sleep 600 & echo $! > '{}'", pid_file.display()),
        ]);
        let status = leader.status().unwrap();
        assert!(status.success());
        let orphan: i32 = (0..100)
            .find_map(|_| {
                std::thread::sleep(Duration::from_millis(20));
                std::fs::read_to_string(&pid_file)
                    .ok()
                    .and_then(|text| text.trim().parse().ok())
            })
            .expect("orphan pid published");
        // Reparented to this process (the subreaper), still running: nothing
        // to reap yet.
        assert_eq!(super::reap_reparented_zombies(|_| false).unwrap(), 0);
        // SAFETY: the test owns this fixture process.
        unsafe { libc::kill(orphan, libc::SIGKILL) };
        let reaped = (0..200)
            .map(|_| {
                std::thread::sleep(Duration::from_millis(20));
                super::reap_reparented_zombies(|_| false).unwrap()
            })
            .find(|reaped| *reaped > 0)
            .expect("the orphan's zombie is reaped");
        assert_eq!(reaped, 1);
        // SAFETY: signal zero only probes process existence.
        assert_ne!(
            unsafe { libc::kill(orphan, 0) },
            0,
            "zombie gone after reaping"
        );
        // A kept pid is left for its owner.
        let keep: std::process::Child = std::process::Command::new("sleep")
            .arg("600")
            .spawn()
            .unwrap();
        let kept = keep.id() as i32;
        unsafe { libc::kill(kept, libc::SIGKILL) };
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(super::reap_reparented_zombies(|_| true).unwrap(), 0);
        // Our own direct child shares our session and is never reaped here.
        assert_eq!(super::reap_reparented_zombies(|_| false).unwrap(), 0);
        let mut keep = keep;
        keep.wait().unwrap();
    }

    /// Reviews R7, R15, R16: an unreadable environment is attributed by
    /// ancestry only where adoption is established; without an adopter every
    /// eligible unreadable process stays uncertain, and a process outside an
    /// established adopter's tree is not the incarnation's.
    #[cfg(unix)]
    #[test]
    fn unreadable_processes_are_attributed_by_ancestry_or_kept_uncertain() {
        let since = Some(std::time::SystemTime::now() - Duration::from_secs(60));
        let observe = |pid: u32, parent: u32, session: u32| Observed {
            pid,
            parent,
            group: pid,
            session,
            same_user: true,
            started: std::time::SystemTime::now(),
            environment: EnvironmentRead::Unreadable,
        };
        // A hidden child of the leader (pid 8000) is a member.
        let child = classify(
            vec![observe(9001, 8000, 9001)],
            Some(8000),
            Some(7000),
            since,
        );
        assert!(child.tagged.contains(&9001) && !child.is_settled());
        // An orphan adopted by the host is uncertain.
        let adopted = classify(
            vec![observe(9001, 7000, 9001)],
            Some(8000),
            Some(7000),
            since,
        );
        assert_eq!(adopted.unknown, vec![9001]);
        // Outside an established adopter's tree: not ours.
        let foreign = classify(vec![observe(9001, 1, 9001)], Some(8000), Some(7000), since);
        assert!(foreign.is_settled());
        // No adopter (macOS, a failed prctl, or a restarted host): uncertain.
        let unadopted = classify(vec![observe(9001, 1, 9001)], Some(8000), None, since);
        assert_eq!(unadopted.unknown, vec![9001]);
        let restart = classify(vec![observe(9001, 1, 9001)], None, None, since);
        assert_eq!(restart.unknown, vec![9001]);
        // Started before the incarnation: never ours.
        let mut old = observe(9001, 1, 9001);
        old.started = std::time::SystemTime::now() - Duration::from_secs(3600);
        assert!(classify(vec![old], None, None, since).is_settled());
        // Another user's unreadable process is never ours.
        let mut other = observe(9001, 7000, 9001);
        other.same_user = false;
        assert!(classify(vec![other], Some(8000), Some(7000), since).is_settled());
    }

    /// A nested host stamps the outer incarnation's id under the outer key;
    /// the outer census reads either key as the marker.
    #[cfg(unix)]
    #[test]
    fn the_outer_marker_key_counts_as_the_marker() {
        let needles = marker_needles("G5D_TEST_CENSUS_TAG", "outer-1");
        assert!(carries_marker(
            b"PATH=/usr/bin\x00G5D_TEST_CENSUS_TAG=outer-1\x00",
            &needles
        ));
        assert!(carries_marker(
            b"G5D_TEST_CENSUS_TAG=inner-2\x00G5D_TEST_CENSUS_TAG_OUTER=outer-1\x00",
            &needles
        ));
        assert!(!carries_marker(
            b"G5D_TEST_CENSUS_TAG=inner-2\x00G5D_TEST_CENSUS_TAG_OUTER=outer-9\x00",
            &needles
        ));
        assert!(!carries_marker(
            b"G5D_TEST_CENSUS_TAGX=outer-1\x00",
            &needles
        ));
    }

    /// An environment the kernel withholds (every platform and hardened binary
    /// on macOS, the incarnation's own shells included) is no evidence: the
    /// process is a member by ancestry or session and otherwise not ours, even
    /// for a restarted host, so a developer's stray `sleep` or browser never
    /// blocks exit proof.
    #[cfg(unix)]
    #[test]
    fn withheld_environments_are_attributed_by_ancestry_or_session_only() {
        let since = Some(std::time::SystemTime::now() - Duration::from_secs(60));
        let observe = |pid: u32, parent: u32, session: u32| Observed {
            pid,
            parent,
            group: pid,
            session,
            same_user: true,
            started: std::time::SystemTime::now(),
            environment: EnvironmentRead::Withheld,
        };
        let child = classify(vec![observe(9001, 8000, 9001)], Some(8000), None, since);
        assert!(child.tagged.contains(&9001) && !child.is_settled());
        let in_session = classify(vec![observe(9001, 1, 8000)], Some(8000), None, since);
        assert!(in_session.groups.contains(&9001) && !in_session.is_settled());
        let stray = classify(vec![observe(9001, 1, 9001)], Some(8000), None, since);
        assert!(stray.is_settled(), "{stray:?}");
        let restart = classify(vec![observe(9001, 1, 9001)], None, None, since);
        assert!(restart.is_settled(), "{restart:?}");
    }

    /// Argument-only replies are the kernel withholding the environment
    /// (`None`); a full buffer is uncertainty; a visible environment is read.
    #[test]
    fn procargs2_environment_is_evidence_only_when_present_and_complete() {
        let mut argv_only = 2u32.to_ne_bytes().to_vec();
        argv_only.extend_from_slice(b"/bin/sleep\x00\x00sleep\x00600\x00");
        assert!(super::parse_procargs2_environment(&argv_only, false)
            .unwrap()
            .is_none());
        let mut padded = argv_only.clone();
        padded.extend_from_slice(b"\x00\x00\x00");
        assert!(super::parse_procargs2_environment(&padded, false)
            .unwrap()
            .is_none());

        let mut visible = argv_only.clone();
        visible.extend_from_slice(b"PATH=/usr/bin\x00G5D_PROCESS_INCARNATION=review\x00");
        let environment = super::parse_procargs2_environment(&visible, false)
            .unwrap()
            .unwrap();
        assert!(environment
            .split(|byte| *byte == 0)
            .any(|variable| variable == b"G5D_PROCESS_INCARNATION=review"));
        let mut untagged = argv_only.clone();
        untagged.extend_from_slice(b"PATH=/usr/bin\x00HOME=/Users/dev\x00");
        let environment = super::parse_procargs2_environment(&untagged, false)
            .unwrap()
            .unwrap();
        assert!(!environment
            .split(|byte| *byte == 0)
            .any(|variable| variable == b"G5D_PROCESS_INCARNATION=review"));

        assert!(super::parse_procargs2_environment(&visible, true).is_err());
        assert!(super::parse_procargs2_environment(b"\x01", false).is_err());

        // Review R21: an empty first argument is one NUL, indistinguishable
        // from the padding after the path; the marker that follows as the
        // first environment entry must still be found, and a marker that is
        // nowhere stays absent.
        let marker = |strings: &[u8]| {
            strings
                .split(|byte| *byte == 0)
                .any(|variable| variable == b"G5D_PROCESS_INCARNATION=review")
        };
        let mut empty_first = 1u32.to_ne_bytes().to_vec();
        empty_first.extend_from_slice(
            b"/bin/sleep\x00\x00\x00\x00\x00\x00G5D_PROCESS_INCARNATION=review\x00PATH=/usr/bin\x00",
        );
        assert!(marker(
            &super::parse_procargs2_environment(&empty_first, false)
                .unwrap()
                .unwrap()
        ));
        let mut empty_first_untagged = 1u32.to_ne_bytes().to_vec();
        empty_first_untagged.extend_from_slice(
            b"/bin/sleep\x00\x00\x00\x00\x00\x00HOME=/Users/dev\x00PATH=/usr/bin\x00",
        );
        assert!(!marker(
            &super::parse_procargs2_environment(&empty_first_untagged, false)
                .unwrap()
                .unwrap()
        ));
        // Only the marker and an empty argument: under the padding reading the
        // marker is the argument and nothing follows, so the reply carries no
        // environment and the process is attributed by ancestry instead.
        let mut only_marker = 1u32.to_ne_bytes().to_vec();
        only_marker.extend_from_slice(b"/bin/sleep\x00\x00\x00G5D_PROCESS_INCARNATION=review\x00");
        assert!(super::parse_procargs2_environment(&only_marker, false)
            .unwrap()
            .is_none());
    }

    use super::*;

    struct NeverReaped;

    struct ImmediatelyReaped;

    struct GraceWaitFailsOnce {
        fail: bool,
        backstop_calls: usize,
    }

    struct GraceWaitFailsThenTimesOut(bool);

    struct NeverReapedWithBounds {
        bounds: Vec<Duration>,
        backstop_calls: usize,
    }

    #[async_trait]
    impl ReapWaiter for ImmediatelyReaped {
        type Receipt = ();

        async fn wait_for_reap(&mut self, _bound: Duration) -> io::Result<Option<()>> {
            Ok(Some(()))
        }
    }

    #[async_trait]
    impl ReapWaiter for NeverReaped {
        type Receipt = ();

        async fn wait_for_reap(&mut self, _bound: Duration) -> io::Result<Option<()>> {
            Ok(None)
        }
    }

    #[async_trait]
    impl ReapWaiter for GraceWaitFailsOnce {
        type Receipt = ();

        async fn wait_for_reap(&mut self, _bound: Duration) -> io::Result<Option<()>> {
            if std::mem::replace(&mut self.fail, false) {
                Err(io::Error::other("grace wait failed"))
            } else {
                Ok(Some(()))
            }
        }

        fn after_kill(&mut self) -> io::Result<()> {
            self.backstop_calls += 1;
            Ok(())
        }
    }

    #[async_trait]
    impl ReapWaiter for GraceWaitFailsThenTimesOut {
        type Receipt = ();

        async fn wait_for_reap(&mut self, _bound: Duration) -> io::Result<Option<()>> {
            if std::mem::replace(&mut self.0, false) {
                Err(io::Error::other("grace wait failed"))
            } else {
                Ok(None)
            }
        }
    }

    #[async_trait]
    impl ReapWaiter for NeverReapedWithBounds {
        type Receipt = ();

        async fn wait_for_reap(&mut self, bound: Duration) -> io::Result<Option<()>> {
            self.bounds.push(bound);
            tokio::time::sleep(bound).await;
            Ok(None)
        }

        fn after_kill(&mut self) -> io::Result<()> {
            self.backstop_calls += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn missing_group_is_not_a_reap_receipt() {
        let error = terminate(
            None,
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::ZERO,
                kill_deadline: Duration::ZERO,
                always_kill_after_grace: false,
            },
            &mut NeverReaped,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, TerminationError::Deadline { .. }));
    }

    #[tokio::test]
    async fn rejects_a_process_group_that_would_wrap_to_a_negative_pid() {
        let error = terminate(
            Some(TerminationTarget::ProcessGroup(u32::MAX)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::ZERO,
                kill_deadline: Duration::ZERO,
                always_kill_after_grace: false,
            },
            &mut NeverReaped,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, TerminationError::InvalidTarget));
    }

    #[tokio::test]
    async fn invalid_group_still_kills_and_reaps_direct_child() {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "exec sleep 30"])
            .spawn()
            .unwrap();
        let error = terminate_tokio_child(
            Some(TerminationTarget::ProcessGroup(0)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::ZERO,
                kill_deadline: Duration::from_secs(2),
                always_kill_after_grace: false,
            },
            &mut child,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, TerminationError::InvalidTarget));
        assert!(child.try_wait().unwrap().is_some());
    }

    #[tokio::test]
    async fn fixed_grace_still_kills_descendants_after_leader_reap() {
        let mut signals = Vec::new();
        let start = tokio::time::Instant::now();
        terminate_with(
            Some(TerminationTarget::ProcessGroup(42)),
            TerminationPolicy {
                first_signal: libc::SIGINT,
                grace: Duration::from_millis(10),
                kill_deadline: Duration::from_secs(1),
                always_kill_after_grace: true,
            },
            &mut ImmediatelyReaped,
            |_, signal| {
                signals.push(signal);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(signals, [libc::SIGINT, KILL_SIGNAL]);
        assert!(start.elapsed() >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn fixed_grace_escalates_after_a_reap_wait_error() {
        let mut signals = Vec::new();
        let mut waiter = GraceWaitFailsOnce {
            fail: true,
            backstop_calls: 0,
        };
        terminate_with(
            Some(TerminationTarget::ProcessGroup(42)),
            TerminationPolicy {
                first_signal: libc::SIGINT,
                grace: Duration::from_millis(1),
                kill_deadline: Duration::from_secs(1),
                always_kill_after_grace: true,
            },
            &mut waiter,
            |_, signal| {
                signals.push(signal);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(signals, [libc::SIGINT, KILL_SIGNAL]);
        assert_eq!(waiter.backstop_calls, 1);
    }

    #[tokio::test]
    async fn graceful_wait_error_still_kills_the_group() {
        let mut signals = Vec::new();
        let mut waiter = GraceWaitFailsOnce {
            fail: true,
            backstop_calls: 0,
        };
        terminate_with(
            Some(TerminationTarget::ProcessGroup(42)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::from_millis(1),
                kill_deadline: Duration::from_secs(1),
                always_kill_after_grace: false,
            },
            &mut waiter,
            |_, signal| {
                signals.push(signal);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(signals, [libc::SIGTERM, KILL_SIGNAL]);
        assert_eq!(waiter.backstop_calls, 1);
    }

    #[tokio::test]
    async fn graceful_wait_error_without_reap_keeps_the_original_error() {
        let mut signals = Vec::new();
        let error = terminate_with(
            Some(TerminationTarget::ProcessGroup(42)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::from_millis(1),
                kill_deadline: Duration::from_secs(1),
                always_kill_after_grace: false,
            },
            &mut GraceWaitFailsThenTimesOut(true),
            |_, signal| {
                signals.push(signal);
                Ok(())
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(&error, TerminationError::Wait(_)));
        assert!(error.to_string().contains("grace wait failed"));
        assert_eq!(signals, [libc::SIGTERM, KILL_SIGNAL]);
    }

    #[tokio::test]
    async fn graceful_wait_error_runs_direct_child_backstop_when_group_kill_fails() {
        let mut waiter = GraceWaitFailsOnce {
            fail: true,
            backstop_calls: 0,
        };
        let error = terminate_with(
            Some(TerminationTarget::ProcessGroup(42)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::from_millis(1),
                kill_deadline: Duration::from_secs(1),
                always_kill_after_grace: false,
            },
            &mut waiter,
            |target, signal| {
                if signal == KILL_SIGNAL {
                    Err(TerminationError::Signal {
                        target,
                        signal,
                        source: io::Error::other("group kill failed"),
                    })
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, TerminationError::Signal { .. }));
        assert_eq!(waiter.backstop_calls, 1);
    }

    #[tokio::test]
    async fn first_signal_error_still_escalates_and_runs_direct_child_backstop() {
        let mut signals = Vec::new();
        let mut waiter = GraceWaitFailsOnce {
            fail: false,
            backstop_calls: 0,
        };
        let error = terminate_with(
            Some(TerminationTarget::Session(42)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::from_secs(1),
                kill_deadline: Duration::from_secs(1),
                always_kill_after_grace: false,
            },
            &mut waiter,
            |target, signal| {
                signals.push(signal);
                if signal == libc::SIGTERM {
                    Err(TerminationError::Signal {
                        target,
                        signal,
                        source: io::Error::from_raw_os_error(libc::EPERM),
                    })
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            TerminationError::Signal {
                signal: libc::SIGTERM,
                ..
            }
        ));
        assert_eq!(signals, [libc::SIGTERM, KILL_SIGNAL]);
        assert_eq!(waiter.backstop_calls, 1);
    }

    #[tokio::test]
    async fn signal_failure_uses_one_kill_deadline_across_both_reap_attempts() {
        let mut waiter = NeverReapedWithBounds {
            bounds: Vec::new(),
            backstop_calls: 0,
        };
        let error = terminate_with_direct_child_backstop(
            Some(TerminationTarget::ProcessGroup(42)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::from_secs(1),
                kill_deadline: Duration::from_millis(10),
                always_kill_after_grace: false,
            },
            &mut waiter,
            |target, signal| {
                if signal == libc::SIGTERM {
                    Err(TerminationError::Signal {
                        target,
                        signal,
                        source: io::Error::other("first signal failed"),
                    })
                } else {
                    Ok(())
                }
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, TerminationError::Signal { .. }));
        assert_eq!(waiter.bounds.len(), 2);
        assert_eq!(waiter.bounds[0], Duration::from_millis(10));
        assert_eq!(waiter.bounds[1], Duration::ZERO);
        assert_eq!(waiter.backstop_calls, 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn session_signal_attempts_every_group_after_error() {
        let mut attempted = Vec::new();
        let error = signal_session_groups([11, 22, 33], libc::SIGTERM, |group, signal| {
            assert_eq!(signal, libc::SIGTERM);
            attempted.push(group);
            if group == 22 {
                Err(io::Error::from_raw_os_error(libc::EPERM))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(attempted, [11, 22, 33]);
        assert_eq!(error.raw_os_error(), Some(libc::EPERM));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn escalates_an_unresponsive_group_and_reaps_its_leader() {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};

        let mut command = tokio::process::Command::new("sh");
        command
            .args(["-c", "trap '' TERM; sleep 30 & echo $!; wait"])
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0);
        let mut child = command.spawn().unwrap();
        let pid = child.id().unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .await
            .unwrap();
        let descendant: i32 = ready.trim().parse().unwrap();

        let status = terminate(
            Some(TerminationTarget::ProcessGroup(pid)),
            TerminationPolicy {
                first_signal: libc::SIGTERM,
                grace: Duration::from_millis(50),
                kill_deadline: Duration::from_secs(2),
                always_kill_after_grace: false,
            },
            &mut TokioChildWaiter(&mut child),
        )
        .await
        .unwrap();
        assert!(!status.success());
        assert!(child.try_wait().unwrap().is_some());
        let descendant_gone = tokio::time::timeout(Duration::from_secs(2), async {
            while process_is_running(descendant) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();
        if !descendant_gone {
            // SAFETY: this test owns the child process group.
            unsafe { libc::killpg(pid as i32, KILL_SIGNAL) };
        }
        assert!(
            descendant_gone,
            "descendant {descendant} survived group SIGKILL"
        );
    }

    #[cfg(unix)]
    fn process_is_running(pid: i32) -> bool {
        #[cfg(target_os = "linux")]
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            return !stat
                .rfind(')')
                .and_then(|end| stat[end + 1..].split_whitespace().next())
                .is_some_and(|state| state == "Z" || state == "X");
        }
        // SAFETY: signal 0 only inspects the test's descendant process.
        unsafe { libc::kill(pid, 0) == 0 }
    }
}
