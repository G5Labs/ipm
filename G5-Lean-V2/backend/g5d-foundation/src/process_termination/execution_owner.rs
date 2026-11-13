//! Linux lifecycle ownership for one execution, independent of process groups.
//!
//! The single-threaded owner is a child subreaper, so double-forked and setsid
//! descendants are adopted here, never by a sibling execution. It keeps the
//! dispatcher's identity and applies the module identity to the supervisor
//! alone, so it can reap descendants that switch identity (guest sudo). The
//! private receipt follows ECHILD, or a release the supervisor requests after
//! its provider ended the turn itself. This is not a hostile-code sandbox.

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};

use super::{EXECUTION_RELEASE_ENV, EXECUTION_RELEASE_EXIT_STATUS};

pub const ENTRYPOINT: &str = "__g5-execution-owner";
const GRACE: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(10);
const REAP_RECEIPT: u8 = 0xa7;
const NO_IDENTITY: &str = "-";

/// The module identity the owner applies to the supervisor alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SupervisorIdentity {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

/// Dispatcher side. Dropping the socket requests cleanup even if its runner
/// future is lost. The child endpoint is dropped immediately after spawn.
pub struct ExecutionOwner {
    socket: UnixStream,
    child_socket: Option<UnixStream>,
}

impl ExecutionOwner {
    /// The owner itself must keep the caller's identity: callers do not apply
    /// `identity` to the returned command.
    pub fn command(
        program: &std::path::Path,
        args: &[String],
        identity: SupervisorIdentity,
    ) -> io::Result<(tokio::process::Command, Self)> {
        let (socket, child_socket) = UnixStream::pair()?;
        socket.set_nonblocking(true)?;
        let fd = child_socket.as_raw_fd();
        let id =
            |value: Option<u32>| value.map_or_else(|| NO_IDENTITY.to_string(), |v| v.to_string());
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .arg(ENTRYPOINT)
            .arg(fd.to_string())
            .arg(id(identity.uid))
            .arg(id(identity.gid))
            .arg(program)
            .args(args);
        // SAFETY: fcntl is async-signal-safe; only this owned descriptor is
        // inherited. The owner restores CLOEXEC before spawning any children.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok((
            command,
            Self {
                socket,
                child_socket: Some(child_socket),
            },
        ))
    }

    pub fn spawned(&mut self) {
        self.child_socket.take();
    }

    /// Never release admission on a missing receipt. The first wait is bounded
    /// for diagnostics; later proof can still recover the quarantined slot.
    pub async fn stop_and_reap(&mut self, child: &mut tokio::process::Child) {
        // A half-close cannot get stuck behind control-channel backpressure;
        // it requests cleanup while preserving the receipt direction.
        let _ = self.socket.shutdown(std::net::Shutdown::Write);
        let started = Instant::now();
        let mut warned = false;
        loop {
            let mut receipt = [0];
            if matches!(self.socket.read(&mut receipt), Ok(1))
                && receipt[0] == REAP_RECEIPT
                && child.wait().await.is_ok()
            {
                return;
            }
            if !warned && started.elapsed() >= GRACE + Duration::from_secs(5) {
                tracing::error!(
                    "agent execution cleanup unconfirmed; retaining execution capacity"
                );
                warned = true;
            }
            tokio::time::sleep(POLL).await;
        }
    }
}

/// Called before constructing Tokio, so waitpid has one authoritative owner.
/// The private socket is never inherited by the supervisor/provider/tools.
pub fn run(mut args: impl Iterator<Item = std::ffi::OsString>) -> io::Result<i32> {
    let fd: i32 = args
        .next()
        .and_then(|s| s.to_str()?.parse().ok())
        .filter(|fd| *fd > 2)
        .ok_or_else(|| io::Error::other("missing owner socket"))?;
    let uid = parse_identity(args.next())?;
    let gid = parse_identity(args.next())?;
    let program = args
        .next()
        .ok_or_else(|| io::Error::other("missing supervised program"))?;
    // Reject a malformed direct CLI invocation before taking fd ownership.
    // SAFETY: F_GETFD only checks whether this descriptor exists.
    if unsafe { libc::fcntl(fd, libc::F_GETFD) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the dispatcher transfers this socket to this private entrypoint.
    let mut socket = unsafe { UnixStream::from_raw_fd(fd) };
    // SAFETY: prctl configures only this process; fcntl protects the owned fd.
    unsafe {
        if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) == -1
            || libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) == -1
        {
            // Setup failed before any child exists: release the reservation.
            let error = io::Error::last_os_error();
            let _ = socket.write_all(&[REAP_RECEIPT]);
            return Err(error);
        }
    }
    socket.set_nonblocking(true)?;
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .process_group(0)
        .env(EXECUTION_RELEASE_ENV, "1");
    // std applies the group, clears supplementary groups, then the user.
    if let Some(gid) = gid {
        command.gid(gid);
    }
    if let Some(uid) = uid {
        command.uid(uid);
    }
    let supervisor = match command.spawn() {
        Ok(child) => child.id() as i32,
        Err(error) => {
            // No child was created, so there is no execution to clean up.
            socket.write_all(&[REAP_RECEIPT])?;
            return Err(error);
        }
    };
    let mut result = 1;
    let mut stopping = None;
    loop {
        let mut request = [0];
        if stopping.is_none() {
            match socket.read(&mut request) {
                Ok(_) => {
                    stopping = Some(Instant::now());
                    // SAFETY: unreaped direct child identity cannot be reused.
                    unsafe {
                        libc::kill(supervisor, libc::SIGTERM);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => {
                    stopping = Some(Instant::now());
                }
            }
        }
        loop {
            let mut status = 0;
            // SAFETY: this single-threaded process owns every child waiter.
            let reaped = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if reaped == supervisor {
                let released = libc::WIFEXITED(status)
                    && libc::WEXITSTATUS(status) == EXECUTION_RELEASE_EXIT_STATUS;
                result = if released {
                    0
                } else if libc::WIFEXITED(status) {
                    libc::WEXITSTATUS(status)
                } else {
                    1
                };
                // A provider that ended its own turn leaves what remains to the
                // agent; the VM's init adopts it once this owner exits. A stop
                // request, a forced or failed turn and supervisor failure never
                // leave adopted tools alive.
                if released && stopping.is_none() && !stop_requested(&mut socket) {
                    let _ = socket.write_all(&[REAP_RECEIPT]);
                    return Ok(result);
                }
                stopping.get_or_insert(Instant::now() - GRACE);
            }
            if reaped > 0 {
                continue;
            }
            if reaped == -1 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ECHILD) {
                    let _ = socket.write_all(&[REAP_RECEIPT]);
                    return Ok(result);
                }
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                // Retain orphan-adoption authority on unexpected wait errors.
                stopping.get_or_insert(Instant::now() - GRACE);
            }
            break;
        }
        if stopping.is_some_and(|start| start.elapsed() >= GRACE) {
            // Kill direct children; their descendants are then adopted here.
            // No wait/reap occurs between enumeration and signal, preventing
            // PID reuse. Repeat until ECHILD, including concurrent forks.
            let children = match direct_children() {
                Ok(children) => children,
                Err(_) => {
                    // Losing /proc visibility cannot authorize owner exit.
                    // Keep adoption/capacity and retry; VM teardown is the
                    // independent recovery boundary for persistent failure.
                    std::thread::sleep(POLL);
                    continue;
                }
            };
            for pid in children {
                // SAFETY: each PID is an unreaped direct child of this owner.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Any bytes or EOF from the dispatcher are a stop request.
fn stop_requested(socket: &mut UnixStream) -> bool {
    let mut request = [0];
    !matches!(socket.read(&mut request), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
}

fn parse_identity(arg: Option<std::ffi::OsString>) -> io::Result<Option<u32>> {
    let arg = arg.ok_or_else(|| io::Error::other("missing supervisor identity"))?;
    match arg.to_str() {
        Some(NO_IDENTITY) => Ok(None),
        Some(id) => id
            .parse()
            .map(Some)
            .map_err(|_| io::Error::other("invalid supervisor identity")),
        None => Err(io::Error::other("invalid supervisor identity")),
    }
}

/// Use ordinary proc stat rather than /proc/.../children, which depends on
/// optional guest kernel configuration. The PTY path uses the same parser.
fn direct_children() -> io::Result<Vec<i32>> {
    let owner = std::process::id();
    let mut children = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        if super::parse_proc_stat(&stat).is_some_and(|(_, parent, _, _)| parent == owner) {
            children.push(pid);
        }
    }
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_identity_travels_in_the_private_argv() {
        let args = |identity| {
            let (command, _owner) = ExecutionOwner::command(
                std::path::Path::new("g5d"),
                &["agent-turn".into()],
                identity,
            )
            .unwrap();
            command
                .as_std()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        let none = args(SupervisorIdentity::default());
        assert_eq!(none[0], ENTRYPOINT);
        assert_eq!(&none[2..], ["-", "-", "g5d", "agent-turn"]);
        let set = args(SupervisorIdentity {
            uid: Some(1000),
            gid: Some(1001),
        });
        assert_eq!(&set[2..], ["1000", "1001", "g5d", "agent-turn"]);
        assert_eq!(parse_identity(Some("-".into())).unwrap(), None);
        assert_eq!(parse_identity(Some("65534".into())).unwrap(), Some(65534));
        assert!(parse_identity(Some("root".into())).is_err());
        assert!(parse_identity(None).is_err());
    }

    #[tokio::test]
    async fn owner_exit_and_socket_eof_do_not_prove_descendant_cleanup() {
        let (socket, peer) = UnixStream::pair().unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut owner = ExecutionOwner {
            socket,
            child_socket: None,
        };
        let mut child = tokio::process::Command::new("true").spawn().unwrap();
        child.wait().await.unwrap();
        drop(peer);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), owner.stop_and_reap(&mut child))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn descendant_receipt_still_requires_owner_reap() {
        let (socket, mut peer) = UnixStream::pair().unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut owner = ExecutionOwner {
            socket,
            child_socket: None,
        };
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        peer.write_all(&[REAP_RECEIPT]).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), owner.stop_and_reap(&mut child))
                .await
                .is_err()
        );
        child.kill().await.unwrap();
    }
}
