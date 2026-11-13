//! Persistent terminal session lifecycle — individual PTY session management.
//!
//! Each `TerminalSession` wraps a PTY (pseudo-terminal) with a long-lived shell
//! process. The session preserves environment variables, working directory, and
//! shell history across multiple `run_in_terminal` invocations.
//!
//! Key responsibilities:
//! - Spawning a shell process with the correct environment
//! - Reading/writing to the PTY file descriptor
//! - Command execution with the marker protocol for completion detection
//! - Output capture with configurable truncation (60KB default)
//! - Timeout handling with SIGINT → wait → SIGKILL cascade
//! - Background execution (non-blocking)
//! - Session state tracking (Running, Idle, Completed, Failed)
//! - Process-group cleanup with a bounded SIGTERM → SIGKILL cascade

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
#[cfg(unix)]
use std::{
    net::Shutdown,
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
    os::unix::net::UnixStream,
};

#[cfg(test)]
use crate::process_termination::parse_proc_stat_process_group;
use crate::process_termination::ReapWaiter;
use g5_contracts::error::G5Error;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use super::SessionState;

const DISABLE_HISTORY_EXPANSION_COMMAND: &str = "set +H 2>/dev/null || true";

#[cfg(unix)]
pub(super) fn wait_for_pty_ready(fd: RawFd, wake_fd: RawFd, events: i16) -> std::io::Result<bool> {
    loop {
        let mut fds = [
            libc::pollfd {
                fd,
                events,
                revents: 0,
            },
            libc::pollfd {
                fd: wake_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: both descriptors remain owned by the caller for this poll.
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if fds[1].revents != 0 {
            return Ok(false);
        }
        if fds[0].revents != 0 {
            return Ok(true);
        }
    }
}

#[cfg(unix)]
pub(super) fn duplicate_pty_fd(fd: RawFd) -> Result<OwnedFd, G5Error> {
    // A readiness copy must never cross exec into a module, preview server,
    // or agent child: it has the same read/write access as the PTY master.
    // SAFETY: fd is a live PTY master descriptor; fcntl returns a new owner.
    let duplicated = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicated < 0 {
        return Err(G5Error::Internal {
            message: format!(
                "Failed to duplicate PTY readiness fd: {}",
                std::io::Error::last_os_error()
            ),
        });
    }
    // SAFETY: a successful F_DUPFD_CLOEXEC returns a separately owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(duplicated) })
}

#[cfg(unix)]
const TERMINATE_SIGNAL: i32 = libc::SIGTERM;
#[cfg(not(unix))]
const TERMINATE_SIGNAL: i32 = 0;

// ---------------------------------------------------------------------------
// SessionConfig
// ---------------------------------------------------------------------------

/// Configuration for a single terminal session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Shell executable path.
    pub shell: String,
    /// Initial working directory.
    pub cwd: PathBuf,
    /// Environment variables for the shell process.
    pub env: HashMap<String, String>,
    /// Unprivileged uid for the shell process.
    pub uid: Option<u32>,
    /// Unprivileged gid for the shell process.
    pub gid: Option<u32>,
    /// Browser PTYs need cancellable, nonblocking input writes.
    pub interactive: bool,
    /// PTY columns.
    pub cols: u16,
    /// PTY rows.
    pub rows: u16,
    /// Maximum output size per command in bytes.
    pub max_output_size: usize,
    /// Scrollback buffer size for reconnection replay (default 4KB).
    pub scrollback_size: usize,
    /// Full scrollback buffer size for output polling (default 64KB).
    pub full_scrollback_size: usize,
}

// ---------------------------------------------------------------------------
// CommandOutput
// ---------------------------------------------------------------------------

/// Output from a single command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// The captured output (stdout + stderr merged via PTY).
    pub output: String,
    /// Exit code of the command (None if timed out or killed).
    pub exit_code: Option<i32>,
    /// Whether the command timed out.
    pub timed_out: bool,
}

// ---------------------------------------------------------------------------
// RingBuffer for scrollback
// ---------------------------------------------------------------------------

/// A byte-level ring buffer for PTY scrollback.
///
/// O(1) push, O(n) read. Used for both the small reconnection buffer (4KB)
/// and the full scrollback buffer (64KB), and as a hosted session's whole
/// retained output.
pub(super) struct ByteRingBuffer {
    data: Vec<u8>,
    capacity: usize,
    write_pos: usize,
    filled: usize,
    next_offset: u64,
}

impl ByteRingBuffer {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            data: vec![0u8; capacity],
            capacity,
            write_pos: 0,
            filled: 0,
            next_offset: 0,
        }
    }

    pub(super) fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.data[self.write_pos] = b;
            self.write_pos = (self.write_pos + 1) % self.capacity;
            if self.filled < self.capacity {
                self.filled += 1;
            }
        }
        self.next_offset = self.next_offset.saturating_add(bytes.len() as u64);
    }

    fn contents(&self) -> Vec<u8> {
        if self.filled < self.capacity {
            self.data[..self.filled].to_vec()
        } else {
            let mut result = Vec::with_capacity(self.capacity);
            result.extend_from_slice(&self.data[self.write_pos..]);
            result.extend_from_slice(&self.data[..self.write_pos]);
            result
        }
    }

    fn clear(&mut self) {
        self.write_pos = 0;
        self.filled = 0;
    }

    /// The offset the next pushed byte will have.
    pub(super) fn next_offset(&self) -> u64 {
        self.next_offset
    }

    /// Bytes from `offset` to the end, or `None` when `offset` is older than
    /// the retained window (the caller fell behind and must reset).
    pub(super) fn read_from(&self, offset: u64) -> Option<Vec<u8>> {
        let oldest_offset = self.next_offset - self.filled as u64;
        if offset < oldest_offset || offset > self.next_offset {
            return None;
        }
        // Copy only the requested suffix: a viewer reads this once per
        // delivered chunk, and the retained window can be megabytes.
        let skip = (offset - oldest_offset) as usize;
        let len = self.filled - skip;
        let start = if self.filled < self.capacity {
            skip
        } else {
            (self.write_pos + skip) % self.capacity
        };
        let first = len.min(self.capacity - start);
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(&self.data[start..start + first]);
        out.extend_from_slice(&self.data[..len - first]);
        Some(out)
    }

    pub(super) fn replay_from(&self, requested_offset: Option<u64>) -> ReplaySnapshot {
        let oldest_offset = self.next_offset - self.filled as u64;
        let can_resume = requested_offset
            .is_some_and(|offset| offset >= oldest_offset && offset <= self.next_offset);
        let offset = if can_resume {
            requested_offset.expect("checked above")
        } else {
            oldest_offset
        };
        let data = self.contents()[(offset - oldest_offset) as usize..].to_vec();
        ReplaySnapshot {
            offset,
            data,
            reset: !can_resume,
        }
    }
}

/// Bytes retained at an atomic attachment boundary. `offset` identifies the
/// first byte in `data`; `reset` means the requested history is no longer held.
pub struct ReplaySnapshot {
    pub offset: u64,
    pub data: Vec<u8>,
    pub reset: bool,
}

/// The polling window (`g5.terminal.output-retention`): the larger ring and
/// the polling reader's cursor into it. A reader receives what the ring still
/// holds past its cursor; output that left the ring before it was read is
/// reported as lost, never retained elsewhere.
struct PollingBuffer {
    ring: ByteRingBuffer,
    drained_offset: u64,
}

impl PollingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            ring: ByteRingBuffer::new(capacity),
            drained_offset: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.ring.push(bytes);
    }

    /// Everything retained since the last drain, and how many earlier bytes
    /// were not retained.
    fn drain(&mut self) -> (Vec<u8>, u64) {
        let snapshot = self.ring.replay_from(Some(self.drained_offset));
        let lost = if snapshot.reset {
            snapshot.offset.saturating_sub(self.drained_offset)
        } else {
            0
        };
        self.drained_offset = self.ring.next_offset;
        (snapshot.data, lost)
    }

    fn clear(&mut self) {
        self.ring.clear();
        self.drained_offset = self.ring.next_offset;
    }
}

// ---------------------------------------------------------------------------
// TerminalSession
// ---------------------------------------------------------------------------

/// A queued or executing browser input packet. Dropping the async caller does
/// not release this reservation while its blocking write can still reach the
/// PTY; the blocking task owns it through its actual return.
pub(super) struct InteractiveWritePermit(Arc<AtomicUsize>);

impl Drop for InteractiveWritePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn write_pty_input(
    writer: &mut dyn Write,
    mut data: &[u8],
    owner_cancel: Option<&CancellationToken>,
    shutdown: &CancellationToken,
    #[cfg(unix)] mut wait_ready: Option<&mut dyn FnMut() -> Result<bool, G5Error>>,
) -> Result<(), G5Error> {
    while !data.is_empty() {
        if shutdown.is_cancelled() {
            return Err(G5Error::NotFound {
                resource: "terminal session stopped".to_string(),
            });
        }
        if owner_cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(G5Error::NotFound {
                resource: "superseded terminal attachment".to_string(),
            });
        }
        match writer.write(&data[..data.len().min(16 * 1024)]) {
            Ok(0) => {
                return Err(G5Error::Internal {
                    message: "PTY writer accepted zero input bytes".to_string(),
                });
            }
            Ok(written) => data = &data[written..],
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                #[cfg(unix)]
                if let Some(wait_ready) = wait_ready.as_mut() {
                    if !wait_ready()? {
                        return Err(G5Error::NotFound {
                            resource: "terminal write cancelled".to_string(),
                        });
                    }
                    continue;
                }
                // Noninteractive writers normally block in write(). This
                // fallback also lets synthetic writers exercise cancellation.
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => {
                return Err(G5Error::Internal {
                    message: format!("Failed to write raw data to PTY: {error}"),
                });
            }
        }
    }
    writer.flush().map_err(|error| G5Error::Internal {
        message: format!("Failed to flush PTY writer: {error}"),
    })
}

/// A persistent terminal session backed by a PTY.
///
/// Each session wraps a PTY pair (master + slave) with a long-lived shell process.
/// The session lifecycle is: Spawn → Execute commands → Shutdown.
///
/// Thread-safe via internal synchronization primitives.
pub struct TerminalSession {
    /// Unique session identifier (UUID v7).
    id: String,
    /// Current session state.
    state: RwLock<SessionState>,
    /// Session creation timestamp.
    created_at: chrono::DateTime<chrono::Utc>,
    /// Last command execution timestamp.
    last_command_at: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
    /// PID of the shell process.
    pid: AtomicU32,
    /// Whether the shell process is still alive.
    alive: AtomicBool,
    /// PTY master writer (for sending input to the shell).
    pty_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Reservations include queued writes, not just tasks already inside the
    /// write loop. Reattachment retires a PTY with any such writer.
    interactive_writes: Arc<AtomicUsize>,
    /// Cancels queued or active PTY input before process-group teardown.
    write_shutdown: CancellationToken,
    /// Wakes the interactive reader's blocking poll during shutdown.
    #[cfg(unix)]
    reader_wake: Option<UnixStream>,
    /// Kept alive by blocking writes even if the session is dropped.
    #[cfg(unix)]
    write_ready_fd: Option<Arc<OwnedFd>>,
    #[cfg(any(test, feature = "test-helpers"))]
    shutdown_failure_once: AtomicBool,
    #[cfg(test)]
    resize_attempts: AtomicUsize,
    #[cfg(test)]
    reader_attempts: Arc<AtomicUsize>,
    /// PTY master reader channel.
    /// A dedicated std::thread reads from the PTY fd in a loop and sends
    /// chunks through this channel. This avoids blocking the async runtime.
    pty_rx: Mutex<tokio::sync::mpsc::Receiver<ReadChunkResult>>,
    /// The child process handle.
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// Serializes shutdown/reap so concurrent stream and lifecycle teardown
    /// cannot both signal or reap the same process group.
    shutdown: Mutex<()>,
    /// The PTY master handle (kept alive for resize operations).
    /// The slave side is dropped after spawn so the master gets EOF when the
    /// shell process exits.
    pty_master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    /// Scrollback buffer for reconnection replay (small, default 4KB).
    scrollback: StdMutex<ByteRingBuffer>,
    /// The polling window for `drain_output` (larger, default 64KB). With the
    /// replay buffer, the only retained output of the session.
    full_scrollback: StdMutex<PollingBuffer>,
    /// Broadcast channel for raw PTY output (for streaming to browser).
    output_tx: broadcast::Sender<Vec<u8>>,
    /// Maximum output size per command.
    max_output_size: usize,
    /// Current working directory (best-effort tracking).
    current_working_dir: RwLock<PathBuf>,
}

impl TerminalSession {
    /// Spawn a new terminal session with the given configuration.
    ///
    /// Creates a new PTY pair and spawns a shell process with:
    /// - Shell: configured shell path
    /// - Working directory: configured cwd
    /// - Environment: inherited + configured env vars + task-specific vars
    /// - PTY size: configured cols × rows
    /// - Interactive mode with job control enabled
    ///
    /// # Errors
    /// Returns `G5Error::Internal` if PTY creation or shell spawn fails.
    pub fn spawn(config: SessionConfig) -> Result<Self, G5Error> {
        let pty_system = NativePtySystem::default();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to create PTY pair: {}", e),
            })?;

        // Terminals execute repository commands, so use the same unprivileged
        // identity as module processes. `setpriv` is part of the Linux guest
        // image and performs the identity change before starting the shell.
        let mut cmd = match (config.uid, config.gid) {
            (Some(_uid), Some(_gid)) => {
                #[cfg(target_os = "linux")]
                {
                    let mut command = CommandBuilder::new("/usr/bin/setpriv");
                    command.args([
                        format!("--reuid={_uid}"),
                        format!("--regid={_gid}"),
                        "--clear-groups".to_string(),
                        "--".to_string(),
                        config.shell.clone(),
                    ]);
                    command
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return Err(G5Error::Internal {
                        message: "Terminal uid/gid isolation requires Linux".to_string(),
                    });
                }
            }
            (None, None) => CommandBuilder::new(&config.shell),
            _ => {
                return Err(G5Error::Internal {
                    message: "Terminal uid and gid must be configured together".to_string(),
                });
            }
        };

        // Set working directory
        cmd.cwd(&config.cwd);

        // Do not expose dispatcher credentials or control-plane configuration
        // through an interactive shell. Only the explicitly assembled terminal
        // environment crosses this boundary.
        cmd.env_clear();

        // Set environment variables
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // Generate session ID (UUID v7 for time-ordering)
        let session_id = uuid::Uuid::now_v7().to_string();

        // Add G5-specific environment variables
        cmd.env("G5_SESSION_ID", &session_id);
        cmd.env("TERM", "xterm-256color");

        // Disable interactive pagers so commands like `git log` don't block
        // the agent loop waiting for user input that never comes.
        cmd.env("GIT_PAGER", "cat");
        cmd.env("PAGER", "cat");

        // Spawn the child process on the slave side of the PTY
        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to spawn shell process: {}", e),
            })?;

        let pid = child.process_id().unwrap_or(0);

        // Drop the slave side of the PTY. The child process has inherited
        // the slave fd, so we don't need it. Closing it here ensures the PTY
        // master read returns EIO (→ EOF) when the last slave fd holder
        // (the shell process) exits.
        let master = pty_pair.master;
        drop(pty_pair.slave);

        // Get the master reader and writer
        let reader = master.try_clone_reader().map_err(|e| G5Error::Internal {
            message: format!("Failed to clone PTY reader: {}", e),
        })?;

        let writer = master.take_writer().map_err(|e| G5Error::Internal {
            message: format!("Failed to take PTY writer: {}", e),
        })?;

        #[cfg(unix)]
        if config.interactive {
            let fd = master.as_raw_fd().ok_or_else(|| G5Error::Internal {
                message: "Interactive PTY master has no file descriptor".to_string(),
            })?;
            // portable-pty clones share this file description. The reader
            // handles WouldBlock below; the writer can observe teardown
            // instead of blocking forever in a full PTY input queue.
            // SAFETY: fd is the live PTY master supplied by portable-pty;
            // fcntl reads and updates only its file status flags.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(G5Error::Internal {
                    message: format!(
                        "Failed to make interactive PTY nonblocking: {}",
                        std::io::Error::last_os_error()
                    ),
                });
            }
        }

        #[cfg(unix)]
        let (reader_poll_fd, reader_wake_rx, reader_wake, write_ready_fd) = if config.interactive {
            let fd = master
                .as_raw_fd()
                .expect("interactive master fd checked above");
            let (wake_rx, wake_tx) = UnixStream::pair().map_err(|error| G5Error::Internal {
                message: format!("Failed to create PTY reader wake pipe: {error}"),
            })?;
            // The reader and writer each own a descriptor through their
            // blocking waits, even if session teardown drops the master.
            let reader_fd = duplicate_pty_fd(fd)?;
            let writer_fd = duplicate_pty_fd(fd)?;
            (
                Some(reader_fd),
                Some(wake_rx),
                Some(wake_tx),
                Some(Arc::new(writer_fd)),
            )
        } else {
            (None, None, None, None)
        };

        let (output_tx, _) = broadcast::channel(256);

        // Spawn a dedicated reader thread that reads from the PTY fd and
        // sends chunks through an mpsc channel. This avoids blocking the
        // tokio async runtime on PTY reads.
        let (pty_tx, pty_rx) = tokio::sync::mpsc::channel::<ReadChunkResult>(256);
        #[cfg(test)]
        let reader_attempts = Arc::new(AtomicUsize::new(0));
        #[cfg(test)]
        let thread_reader_attempts = Arc::clone(&reader_attempts);
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", &session_id[..8]))
            .spawn(move || {
                let mut reader = reader;
                #[cfg(unix)]
                let wake_rx = reader_wake_rx;
                let mut buf = [0u8; 4096];
                loop {
                    #[cfg(test)]
                    thread_reader_attempts.fetch_add(1, Ordering::Relaxed);
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            let _ = pty_tx.blocking_send(ReadChunkResult::Eof);
                            break;
                        }
                        Ok(n) => {
                            if pty_tx
                                .blocking_send(ReadChunkResult::Data(buf[..n].to_vec()))
                                .is_err()
                            {
                                break; // Receiver dropped — session closed
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            #[cfg(unix)]
                            if let (Some(fd), Some(wake)) =
                                (reader_poll_fd.as_ref(), wake_rx.as_ref())
                            {
                                match wait_for_pty_ready(
                                    fd.as_raw_fd(),
                                    wake.as_raw_fd(),
                                    libc::POLLIN,
                                ) {
                                    Ok(true) => continue,
                                    Ok(false) => break,
                                    Err(error) => {
                                        let _ = pty_tx.blocking_send(ReadChunkResult::Error(
                                            error.to_string(),
                                        ));
                                        break;
                                    }
                                }
                            }
                            // Noninteractive readers normally block inside
                            // read(); this path is only a fallback for a
                            // platform without a readiness descriptor.
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            // On Linux, EIO means the slave PTY closed (shell
                            // exited). Treat it as EOF.
                            if e.raw_os_error() == Some(libc::EIO) {
                                let _ = pty_tx.blocking_send(ReadChunkResult::Eof);
                            } else {
                                let _ = pty_tx.blocking_send(ReadChunkResult::Error(e.to_string()));
                            }
                            break;
                        }
                    }
                }
            })
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to spawn PTY reader thread: {}", e),
            })?;

        let session = Self {
            id: session_id,
            state: RwLock::new(SessionState::Idle),
            created_at: chrono::Utc::now(),
            last_command_at: RwLock::new(None),
            pid: AtomicU32::new(pid),
            alive: AtomicBool::new(true),
            pty_writer: Arc::new(Mutex::new(writer)),
            interactive_writes: Arc::new(AtomicUsize::new(0)),
            write_shutdown: CancellationToken::new(),
            #[cfg(unix)]
            reader_wake,
            #[cfg(unix)]
            write_ready_fd,
            #[cfg(any(test, feature = "test-helpers"))]
            shutdown_failure_once: AtomicBool::new(false),
            #[cfg(test)]
            resize_attempts: AtomicUsize::new(0),
            #[cfg(test)]
            reader_attempts,
            pty_rx: Mutex::new(pty_rx),
            child: Mutex::new(child),
            shutdown: Mutex::new(()),
            pty_master: Mutex::new(master),
            scrollback: StdMutex::new(ByteRingBuffer::new(config.scrollback_size)),
            full_scrollback: StdMutex::new(PollingBuffer::new(config.full_scrollback_size)),
            output_tx,
            max_output_size: config.max_output_size,
            current_working_dir: RwLock::new(config.cwd),
        };

        Ok(session)
    }

    /// Execute a command and wait for completion.
    ///
    /// Uses the marker protocol to detect command completion and capture exit code:
    /// 1. Optionally change directory via `cd {dir} && `
    /// 2. Write the command with a unique end-marker
    /// 3. Read output until marker is seen or timeout
    /// 4. Parse exit code from marker line
    /// 5. Strip marker from output, truncate if needed
    ///
    /// # Errors
    /// Returns `G5Error::Internal` on PTY I/O failures.
    pub async fn execute_command(
        &self,
        command: &str,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> Result<CommandOutput, G5Error> {
        // Update state to Running
        {
            let mut state = self.state.write().await;
            *state = SessionState::Running;
        }
        {
            let mut last = self.last_command_at.write().await;
            *last = Some(chrono::Utc::now());
        }

        // Generate unique marker for command completion detection
        let marker = format!("__G5_END_{}__", uuid::Uuid::now_v7().simple());

        // Build the full command string with marker. The history-expansion
        // guard is a complete command before arbitrary user input so multiline
        // snippets containing `!` cannot prevent marker emission.
        let full_command = if let Some(dir) = cwd {
            // Update tracked working directory
            {
                let mut cwd_lock = self.current_working_dir.write().await;
                *cwd_lock = PathBuf::from(dir);
            }
            build_foreground_command(command, Some(dir), &marker)
        } else {
            build_foreground_command(command, None, &marker)
        };

        // Write command to PTY
        {
            let mut writer = self.pty_writer.lock().await;
            writer
                .write_all(full_command.as_bytes())
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to write command to PTY: {}", e),
                })?;
            writer.flush().map_err(|e| G5Error::Internal {
                message: format!("Failed to flush PTY writer: {}", e),
            })?;
        }

        // Read output until marker or timeout
        let result = self.read_until_marker(&marker, timeout).await;

        // Update state back to Idle
        {
            let mut state = self.state.write().await;
            *state = SessionState::Idle;
        }

        // Check if shell process has exited
        self.check_alive().await;

        result
    }

    /// Execute a command in the background (don't wait for completion).
    ///
    /// Writes the command to the PTY but does NOT wait for the marker.
    /// Returns immediately. Output accumulates in the session's scrollback
    /// buffer and can be polled via `drain_output()`.
    ///
    /// # Errors
    /// Returns `G5Error::Internal` on PTY write failures.
    pub async fn execute_background(
        &self,
        command: &str,
        cwd: Option<&str>,
    ) -> Result<(), G5Error> {
        {
            let mut last = self.last_command_at.write().await;
            *last = Some(chrono::Utc::now());
        }

        let full_command = if let Some(dir) = cwd {
            {
                let mut cwd_lock = self.current_working_dir.write().await;
                *cwd_lock = PathBuf::from(dir);
            }
            build_background_command(command, Some(dir))
        } else {
            build_background_command(command, None)
        };

        let mut writer = self.pty_writer.lock().await;
        writer
            .write_all(full_command.as_bytes())
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to write background command to PTY: {}", e),
            })?;
        writer.flush().map_err(|e| G5Error::Internal {
            message: format!("Failed to flush PTY writer: {}", e),
        })?;

        Ok(())
    }

    /// Get the output retained since the last call (for background commands).
    ///
    /// Returns what the polling window still holds past the previous drain.
    /// Output that left the window before it was drained is not retained
    /// anywhere; the result then starts with a notice naming the lost byte
    /// count (`g5.terminal.output-retention`).
    pub async fn drain_output(&self) -> Result<String, G5Error> {
        // First, try to read any available output from the PTY
        self.try_read_available().await;

        let (data, lost) = self.full_scrollback.lock().unwrap().drain();
        let mut output = String::new();
        if lost > 0 {
            output.push_str(&format!(
                "\n[{lost} bytes of earlier output were not retained]\n"
            ));
        }
        output.push_str(&String::from_utf8_lossy(&data));
        Ok(output)
    }

    /// Bytes currently retained across the session's output windows.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn retained_output_bytes(&self) -> usize {
        self.scrollback.lock().unwrap().filled + self.full_scrollback.lock().unwrap().ring.filled
    }

    /// Get the session's unique ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the current session state.
    ///
    /// Uses `try_read()` to avoid blocking. Panics if the lock is poisoned.
    pub fn state(&self) -> SessionState {
        // In a tokio context, try_read should succeed unless there's contention
        match self.state.try_read() {
            Ok(guard) => *guard,
            Err(e) => {
                // Lock is contended; return a reasonable default
                // This can happen in synchronous contexts
                tracing::warn!(error = %e, "session state RwLock contended — returning Idle");
                SessionState::Idle
            }
        }
    }

    /// Get the shell process PID.
    pub fn pid(&self) -> u32 {
        self.pid.load(Ordering::Relaxed)
    }

    /// Check if the shell process is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Resize the PTY dimensions.
    ///
    /// Calls `ioctl(TIOCSWINSZ)` on the PTY master fd to update the terminal
    /// dimensions. This causes the shell and any running programs to receive
    /// `SIGWINCH` and adjust their output accordingly.
    ///
    /// # Errors
    /// Returns `G5Error::Internal` on resize failure.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), G5Error> {
        #[cfg(test)]
        self.resize_attempts.fetch_add(1, Ordering::Relaxed);
        let pty_master = self.pty_master.lock().await;
        pty_master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to resize PTY: {}", e),
            })
    }

    #[cfg(test)]
    pub(super) async fn hold_resize_lock_for_test(
        &self,
    ) -> tokio::sync::MutexGuard<'_, Box<dyn portable_pty::MasterPty + Send>> {
        self.pty_master.lock().await
    }

    #[cfg(test)]
    pub(super) async fn hold_shutdown_lock_for_test(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.shutdown.lock().await
    }

    #[cfg(test)]
    pub(super) fn resize_attempts_for_test(&self) -> usize {
        self.resize_attempts.load(Ordering::Relaxed)
    }

    /// Write raw bytes to the PTY master (for browser keyboard input).
    ///
    /// # Errors
    /// Returns `G5Error::Internal` on write failure.
    pub async fn write_raw(&self, data: &[u8]) -> Result<(), G5Error> {
        self.write_raw_inner(data, None, None).await
    }

    pub(super) fn reserve_interactive_write(&self) -> InteractiveWritePermit {
        self.interactive_writes.fetch_add(1, Ordering::AcqRel);
        InteractiveWritePermit(Arc::clone(&self.interactive_writes))
    }

    pub(super) fn has_interactive_write_in_flight(&self) -> bool {
        self.interactive_writes.load(Ordering::Acquire) != 0
    }

    /// Offload the synchronous PTY master write so a full slave input queue
    /// cannot block a Tokio worker or the terminal registry during shutdown.
    pub(super) async fn write_interactive_raw(
        &self,
        data: &[u8],
        cancel: CancellationToken,
        permit: InteractiveWritePermit,
    ) -> Result<(), G5Error> {
        self.write_raw_inner(data, Some(cancel), Some(permit)).await
    }

    async fn write_raw_inner(
        &self,
        data: &[u8],
        cancel: Option<CancellationToken>,
        permit: Option<InteractiveWritePermit>,
    ) -> Result<(), G5Error> {
        let writer = Arc::clone(&self.pty_writer);
        let data = data.to_vec();
        let write_cancel = cancel.clone();
        let write_shutdown = self.write_shutdown.clone();
        #[cfg(unix)]
        let write_ready_fd = self.write_ready_fd.clone();
        #[cfg(unix)]
        let runtime_handle = tokio::runtime::Handle::current();
        let mut task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut writer = writer.blocking_lock();
            #[cfg(unix)]
            let mut wake_reader: Option<UnixStream> = None;
            #[cfg(unix)]
            let mut wake_task = None;
            #[cfg(unix)]
            let mut wait_ready = || -> Result<bool, G5Error> {
                if wake_reader.is_none() {
                    let (reader, wake_writer) =
                        UnixStream::pair().map_err(|error| G5Error::Internal {
                            message: format!("Failed to create PTY write wake pipe: {error}"),
                        })?;
                    let owner = write_cancel.clone();
                    let shutdown = write_shutdown.clone();
                    wake_task = Some(runtime_handle.spawn(async move {
                        if let Some(owner) = owner {
                            tokio::select! {
                                _ = owner.cancelled() => {},
                                _ = shutdown.cancelled() => {},
                            }
                        } else {
                            shutdown.cancelled().await;
                        }
                        let _ = wake_writer.shutdown(Shutdown::Write);
                    }));
                    wake_reader = Some(reader);
                }
                wait_for_pty_ready(
                    write_ready_fd
                        .as_ref()
                        .expect("readiness fd present")
                        .as_raw_fd(),
                    wake_reader.as_ref().expect("wake pipe present").as_raw_fd(),
                    libc::POLLOUT,
                )
                .map_err(|error| G5Error::Internal {
                    message: format!("Failed to wait for PTY write readiness: {error}"),
                })
            };
            let result = write_pty_input(
                &mut **writer,
                &data,
                write_cancel.as_ref(),
                &write_shutdown,
                #[cfg(unix)]
                write_ready_fd
                    .as_ref()
                    .map(|_| &mut wait_ready as &mut dyn FnMut() -> Result<bool, G5Error>),
            );
            #[cfg(unix)]
            if let Some(task) = wake_task {
                task.abort();
            }
            result
        });
        let result = if let Some(cancel) = cancel {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(G5Error::NotFound {
                    resource: "superseded terminal attachment".to_string(),
                }),
                result = &mut task => result.map_err(|error| G5Error::Internal {
                    message: format!("PTY write task failed: {error}"),
                })?,
            }
        } else {
            (&mut task).await.map_err(|error| G5Error::Internal {
                message: format!("PTY write task failed: {error}"),
            })?
        };
        result
    }

    /// Subscribe to raw PTY output (for streaming to browser).
    ///
    /// Returns a broadcast receiver that receives all output bytes as they
    /// are produced. Each subscriber gets its own receiver.
    pub fn subscribe_output(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    /// Read and account for the next raw PTY output chunk. Browser terminal
    /// bridges use this as the single consumer of the PTY reader channel.
    pub async fn read_raw(&self) -> Result<Option<Vec<u8>>, G5Error> {
        self.read_raw_inner(None).await
    }

    /// Read output for one browser attachment. Once its generation is
    /// superseded, it must not consume output after the new replay snapshot.
    pub async fn read_raw_for_attachment(
        &self,
        owner_cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>, G5Error> {
        self.read_raw_inner(Some(owner_cancel)).await
    }

    async fn read_raw_inner(
        &self,
        owner_cancel: Option<&CancellationToken>,
    ) -> Result<Option<Vec<u8>>, G5Error> {
        loop {
            if owner_cancel.is_some_and(CancellationToken::is_cancelled) {
                return Err(G5Error::NotFound {
                    resource: "superseded terminal attachment".to_string(),
                });
            }
            // Keep the receiver lock through all byte bookkeeping. There must
            // be no await after recv consumes a chunk: serve_pty can cancel
            // this read when another WebSocket message arrives. A new owner
            // also takes this lock before snapshotting replay.
            let mut rx = self.pty_rx.lock().await;
            if owner_cancel.is_some_and(CancellationToken::is_cancelled) {
                return Err(G5Error::NotFound {
                    resource: "superseded terminal attachment".to_string(),
                });
            }
            let chunk = if let Some(cancel) = owner_cancel {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(G5Error::NotFound {
                        resource: "superseded terminal attachment".to_string(),
                    }),
                    result = rx.recv() => result.unwrap_or(ReadChunkResult::Eof),
                }
            } else {
                match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => ReadChunkResult::Eof,
                    Err(_) => ReadChunkResult::WouldBlock,
                }
            };
            match chunk {
                ReadChunkResult::Data(bytes) => {
                    let _ = self.output_tx.send(bytes.clone());
                    self.scrollback.lock().unwrap().push(&bytes);
                    self.full_scrollback.lock().unwrap().push(&bytes);
                    drop(rx);
                    return Ok(Some(bytes));
                }
                ReadChunkResult::WouldBlock => continue,
                ReadChunkResult::Eof => {
                    self.alive.store(false, Ordering::Relaxed);
                    return Ok(None);
                }
                ReadChunkResult::Error(error) => {
                    self.alive.store(false, Ordering::Relaxed);
                    return Err(G5Error::Internal {
                        message: format!("PTY read error: {error}"),
                    });
                }
            }
        }
    }

    /// Copy replay at the same receiver boundary used by the old attachment.
    pub async fn replay_from(&self, requested_offset: Option<u64>) -> ReplaySnapshot {
        let _rx = self.pty_rx.lock().await;
        self.scrollback
            .lock()
            .unwrap()
            .replay_from(requested_offset)
    }

    /// Shut down the complete PTY process group and reap its leader.
    ///
    /// Cleanup sequence:
    /// 1. Send SIGTERM to the shell's process group
    /// 2. Wait a bounded grace period for the leader and descendants to exit
    /// 3. Send SIGKILL to the process group if anything remains
    /// 4. Close the PTY master fd (implicit on drop)
    /// 5. Reap the child process (waitpid)
    /// 6. Clear the scrollback buffer
    pub async fn shutdown(&self) -> Result<(), G5Error> {
        let _shutdown = self.shutdown.lock().await;
        self.write_shutdown.cancel();
        #[cfg(unix)]
        if let Some(wake) = &self.reader_wake {
            let _ = wake.shutdown(Shutdown::Write);
        }
        #[cfg(any(test, feature = "test-helpers"))]
        if self.shutdown_failure_once.swap(false, Ordering::AcqRel) {
            return Err(shutdown_error(
                &self.id,
                "injected one-shot failure".to_string(),
            ));
        }
        tracing::info!(
            terminal_id = %self.id,
            pid = self.pid(),
            "Shutting down terminal session"
        );

        let pid = self.pid();
        let mut child = self.child.lock().await;
        let already_reaped = child
            .try_wait()
            .map_err(|error| {
                shutdown_error(&self.id, format!("poll shell before shutdown: {error}"))
            })?
            .is_some();
        let live_before = session_has_live_processes(pid).map_err(|error| {
            shutdown_error(&self.id, format!("inspect terminal session: {error}"))
        })?;
        if !already_reaped || live_before {
            let mut waiter = TerminalReapWaiter {
                child: &mut **child,
                session_id: pid,
                leader_reaped: already_reaped,
            };
            if let Err(error) = crate::process_termination::terminate(
                Some(crate::process_termination::TerminationTarget::Session(pid)),
                crate::process_termination::TerminationPolicy {
                    first_signal: TERMINATE_SIGNAL,
                    grace: Duration::from_secs(2),
                    kill_deadline: Duration::from_secs(3),
                    always_kill_after_grace: false,
                },
                &mut waiter,
            )
            .await
            {
                self.alive.store(
                    session_has_live_processes(pid).unwrap_or(true),
                    Ordering::Relaxed,
                );
                let mut state = self.state.write().await;
                *state = SessionState::Failed;
                return Err(shutdown_error(&self.id, error.to_string()));
            }
        }
        drop(child);

        // Mark as dead
        self.alive.store(false, Ordering::Relaxed);

        // Update state
        {
            let mut state = self.state.write().await;
            *state = SessionState::Completed;
        }

        // Retire the reader queue at the same boundary as byte bookkeeping.
        // Otherwise a queued final chunk can refill the cleared buffers on a
        // later read, or an in-flight reader can append after the clear.
        let mut rx = self.pty_rx.lock().await;
        rx.close();
        // A sender that already reserved capacity can still deliver a chunk.
        while rx.recv().await.is_some() {}

        // Clear scrollback buffers while no reader can account for more bytes.
        {
            let mut sb = self.scrollback.lock().unwrap();
            sb.clear();
        }
        {
            let mut fsb = self.full_scrollback.lock().unwrap();
            fsb.clear();
        }

        tracing::info!(
            terminal_id = %self.id,
            "Terminal session shutdown complete"
        );
        Ok(())
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn fail_next_shutdown_for_test(&self) {
        self.shutdown_failure_once.store(true, Ordering::Release);
    }

    /// Get session creation timestamp.
    pub fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
    }

    /// Get last command execution timestamp.
    pub fn last_command_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.last_command_at
            .try_read()
            .ok()
            .and_then(|guard| *guard)
    }

    // -- Internal helpers --

    /// Read PTY output until the marker is found or timeout expires.
    ///
    /// This is the core of the marker protocol. It reads from the PTY in a loop,
    /// accumulating output and scanning for the marker pattern. The PTY read is
    /// blocking, so we hold the reader lock for each read chunk and yield between
    /// reads to allow other async tasks to progress.
    async fn read_until_marker(
        &self,
        marker: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, G5Error> {
        let mut accumulated = String::new();
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            // Check timeout
            if tokio::time::Instant::now() >= deadline {
                // Timeout: send SIGINT (Ctrl+C) to foreground process group
                self.send_interrupt().await;

                // Wait 2 seconds for the process to respond to SIGINT
                tokio::time::sleep(Duration::from_secs(2)).await;

                accumulated.push_str(&format!(
                    "\n[Command timed out after {}s]\n",
                    timeout.as_secs()
                ));

                return Ok(CommandOutput {
                    output: self.truncate_output(&accumulated),
                    exit_code: None,
                    timed_out: true,
                });
            }

            // Read a chunk from the PTY (blocking read with short timeout)
            let read_result = self.read_chunk().await;

            match read_result {
                ReadChunkResult::Data(bytes) => {
                    // Broadcast to subscribers (for browser streaming)
                    let _ = self.output_tx.send(bytes.clone());

                    // Add to accumulated output
                    accumulated.push_str(&String::from_utf8_lossy(&bytes));
                }
                ReadChunkResult::Eof => {
                    // Shell exited — PTY read returned EOF
                    self.alive.store(false, Ordering::Relaxed);
                    // Try to get exit code from the child process since the
                    // marker echo may never have run (e.g. `exit N`).
                    let exit_code = {
                        let mut child = self.child.lock().await;
                        child
                            .try_wait()
                            .ok()
                            .flatten()
                            .map(|status| status.exit_code() as i32)
                    };
                    {
                        let mut state = self.state.write().await;
                        *state = match exit_code {
                            Some(0) | None => SessionState::Completed,
                            Some(_) => SessionState::Failed,
                        };
                    }
                    return Ok(CommandOutput {
                        output: self.truncate_output(&accumulated),
                        exit_code,
                        timed_out: false,
                    });
                }
                ReadChunkResult::WouldBlock => {
                    // No data available right now — yield and retry
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                ReadChunkResult::Error(e) => {
                    self.alive.store(false, Ordering::Relaxed);
                    return Err(G5Error::Internal {
                        message: format!("PTY read error: {}", e),
                    });
                }
            }

            // Check if marker is in accumulated output
            // Look for the pattern: \n{marker}:{digits}\n
            if let Some(result) = self.try_extract_marker_result(&accumulated, marker) {
                return Ok(result);
            }
        }
    }

    /// Read a single chunk of data from the PTY reader channel.
    ///
    /// The dedicated reader thread sends chunks through an mpsc channel.
    /// We recv with a 100ms timeout so the caller can periodically check
    /// the command-level deadline.
    async fn read_chunk(&self) -> ReadChunkResult {
        let mut rx = self.pty_rx.lock().await;
        let chunk = match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(result)) => result,
            Ok(None) => ReadChunkResult::Eof, // Channel closed
            Err(_elapsed) => ReadChunkResult::WouldBlock, // No data within 100ms
        };
        if let ReadChunkResult::Data(bytes) = &chunk {
            self.scrollback.lock().unwrap().push(bytes);
            self.full_scrollback.lock().unwrap().push(bytes);
        }
        chunk
    }

    /// Try to extract the marker and exit code from accumulated output.
    ///
    /// Scans for the pattern `{marker}:{digits}` and returns a `CommandOutput`
    /// if found, with the marker line stripped from the output.
    fn try_extract_marker_result(&self, accumulated: &str, marker: &str) -> Option<CommandOutput> {
        let marker_pattern = format!("{}:", marker);

        // Search backwards through all occurrences of the marker pattern,
        // accepting only those where the colon is immediately followed by
        // ASCII digits (i.e. the real exit-code line, not the PTY echo of
        // the command which contains `$?`).
        let mut search_end = accumulated.len();
        while let Some(pos) = accumulated[..search_end].rfind(&marker_pattern) {
            let after_colon = &accumulated[pos + marker_pattern.len()..];
            let first_char = after_colon.chars().next();
            if first_char.is_some_and(|c| c.is_ascii_digit()) {
                // This is the real marker — parse exit code
                let after_marker = &accumulated[pos..];
                let exit_code = parse_exit_code_from_marker(after_marker, marker);

                // Find the start of the marker line
                let line_start = accumulated[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0);

                // Take everything before the marker line
                let clean_output = accumulated[..line_start].to_string();

                // Strip command echo lines (lines containing the marker setup)
                let clean_output = strip_command_echo(&clean_output, marker);

                return Some(CommandOutput {
                    output: self.truncate_output(&clean_output),
                    exit_code,
                    timed_out: false,
                });
            }
            // Not a valid marker (echo line with $?) — keep searching backwards
            if pos == 0 {
                break;
            }
            search_end = pos;
        }

        None
    }

    /// Truncate output to max_output_size.
    ///
    /// When output exceeds the limit, keep the first 10KB and last 50KB with
    /// a `\n... [output truncated, {total_bytes} bytes total] ...\n` separator.
    /// This preserves both the initial context (command echo, early output)
    /// and the final result (exit status, error messages).
    fn truncate_output(&self, output: &str) -> String {
        let bytes = output.as_bytes();
        if bytes.len() <= self.max_output_size {
            return output.to_string();
        }

        let total_bytes = bytes.len();
        let first_chunk_size = 10240.min(self.max_output_size / 3); // 10KB or 1/3 of max
        let separator_reserve = 100;
        let last_chunk_size = self
            .max_output_size
            .saturating_sub(first_chunk_size)
            .saturating_sub(separator_reserve);

        if last_chunk_size == 0 || first_chunk_size == 0 {
            // max_output_size is too small for the 3-part strategy; just truncate
            return String::from_utf8_lossy(&bytes[..self.max_output_size]).to_string();
        }

        let first_part = String::from_utf8_lossy(&bytes[..first_chunk_size]);
        let last_part = String::from_utf8_lossy(&bytes[total_bytes - last_chunk_size..]);

        format!(
            "{}\n... [output truncated, {} bytes total] ...\n{}",
            first_part, total_bytes, last_part
        )
    }

    /// Send SIGINT to the foreground process group (Ctrl+C equivalent).
    ///
    /// Writes ETX (0x03) to the PTY, which the terminal line discipline
    /// translates to SIGINT for the foreground process group.
    async fn send_interrupt(&self) {
        let mut writer = self.pty_writer.lock().await;
        let _ = writer.write_all(&[0x03]); // ETX = Ctrl+C
        let _ = writer.flush();
    }

    /// Check if the shell process is still alive and update state accordingly.
    async fn check_alive(&self) {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(Some(status)) => {
                self.alive.store(false, Ordering::Relaxed);
                let mut state = self.state.write().await;
                if status.success() {
                    *state = SessionState::Completed;
                } else {
                    *state = SessionState::Failed;
                }
            }
            Ok(None) => {
                // Still running — no state change
            }
            Err(e) => {
                tracing::debug!(error = %e, "Error checking process status — assuming still alive");
            }
        }
    }

    /// Try to read any available output from the PTY without blocking.
    ///
    /// Used by `drain_output()` to flush pending PTY data before returning.
    /// Drains all currently-buffered chunks from the reader channel.
    async fn try_read_available(&self) {
        let mut rx = self.pty_rx.lock().await;
        loop {
            match rx.try_recv() {
                Ok(ReadChunkResult::Data(bytes)) => {
                    // Broadcast to subscribers
                    let _ = self.output_tx.send(bytes.clone());

                    // Add to scrollback buffers
                    {
                        let mut sb = self.scrollback.lock().unwrap();
                        sb.push(&bytes);
                    }
                    {
                        let mut fsb = self.full_scrollback.lock().unwrap();
                        fsb.push(&bytes);
                    }
                }
                Ok(ReadChunkResult::Eof) => {
                    self.alive.store(false, Ordering::Relaxed);
                    break;
                }
                Ok(ReadChunkResult::Error(_)) => {
                    self.alive.store(false, Ordering::Relaxed);
                    break;
                }
                Ok(ReadChunkResult::WouldBlock) => {
                    // Shouldn't happen from the reader thread, but ignore
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.alive.store(false, Ordering::Relaxed);
                    break;
                }
            }
        }
    }
}

fn shutdown_error(terminal_id: &str, detail: String) -> G5Error {
    G5Error::Internal {
        message: format!("terminal {terminal_id} shutdown failed: {detail}"),
    }
}

#[cfg(target_os = "linux")]
fn session_has_live_processes(session_id: u32) -> std::io::Result<bool> {
    Ok(!crate::process_termination::session_process_groups(session_id)?.is_empty())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn session_has_live_processes(session_id: u32) -> std::io::Result<bool> {
    Ok(process_group_exists(session_id))
}

#[cfg(not(unix))]
fn session_has_live_processes(_session_id: u32) -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_group_exists(process_group_id: u32) -> bool {
    if process_group_id == 0 {
        return false;
    }
    // SAFETY: signal 0 performs an existence/permission check only.
    let result = unsafe { libc::killpg(process_group_id as i32, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

struct TerminalReapWaiter<'a> {
    child: &'a mut (dyn portable_pty::Child + Send + Sync),
    session_id: u32,
    leader_reaped: bool,
}

#[async_trait::async_trait]
impl ReapWaiter for TerminalReapWaiter<'_> {
    type Receipt = ();

    async fn wait_for_reap(&mut self, timeout: Duration) -> std::io::Result<Option<()>> {
        wait_for_process_group_exit(
            self.child,
            self.session_id,
            &mut self.leader_reaped,
            timeout,
        )
        .await
        .map(|done| done.then_some(()))
    }

    fn after_kill(&mut self) -> std::io::Result<()> {
        // portable-pty is the cross-platform backstop for the leader.
        let _ = self.child.kill();
        Ok(())
    }
}

async fn wait_for_process_group_exit(
    child: &mut (dyn portable_pty::Child + Send + Sync),
    process_group_id: u32,
    leader_reaped: &mut bool,
    timeout: Duration,
) -> std::io::Result<bool> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if !*leader_reaped {
            *leader_reaped = child.try_wait()?.is_some();
        }
        if *leader_reaped && !session_has_live_processes(process_group_id)? {
            return Ok(true);
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// Read chunk result
// ---------------------------------------------------------------------------

/// Result of a single PTY read operation.
enum ReadChunkResult {
    /// Successfully read N bytes.
    Data(Vec<u8>),
    /// PTY returned EOF (shell exited).
    Eof,
    /// No data available (would block).
    WouldBlock,
    /// Read error.
    Error(String),
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Parse exit code from a marker line.
///
/// Expected format: `{marker}:{exit_code}`
/// Extracts digits immediately following `{marker}:`.
fn parse_exit_code_from_marker(text: &str, marker: &str) -> Option<i32> {
    let marker_with_colon = format!("{}:", marker);
    if let Some(pos) = text.find(&marker_with_colon) {
        let after = &text[pos + marker_with_colon.len()..];
        let code_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        code_str.parse().ok()
    } else {
        None
    }
}

/// Strip ANSI escape sequences from text.
///
/// Handles common sequences: CSI (`ESC[...`), OSC (`ESC]...BEL/ST`),
/// single-char escapes, and backspace.
fn strip_ansi(s: &str) -> String {
    // Lazy static would be better, but using once_cell or just constructing
    // each time is fine for our usage (called once per line).
    let re = regex::Regex::new(
        r"(?x)
        \x1b \[ [0-9;?]* [a-zA-Z~]    # CSI sequences
      | \x1b \] [^\x07\x1b]* (?:\x07 | \x1b\\)  # OSC sequences
      | \x1b [()][0-9A-Za-z]           # Character set selection
      | \x1b [=>\x1b]                  # Keypad/other mode
      | \x08                            # Backspace
      ",
    )
    .unwrap();
    re.replace_all(s, "").to_string()
}

/// Strip command echo lines from output.
///
/// The PTY echoes the command we typed (including the marker setup).
/// We filter out any lines whose ANSI-stripped text contains the marker string.
/// We also filter out shell prompt lines (lines starting with common prompt
/// patterns) to leave only actual command output.
fn strip_command_echo(output: &str, marker: &str) -> String {
    let filtered: Vec<&str> = output
        .lines()
        .filter(|line| {
            let clean_line = strip_ansi(line);
            !clean_line.contains(marker)
        })
        .collect();
    let mut result = filtered.join("\n");
    // Preserve trailing newline if the original had one
    if output.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Build a foreground command for the persistent shell marker protocol.
fn build_foreground_command(command: &str, cwd: Option<&str>, marker: &str) -> String {
    let command_line = if let Some(dir) = cwd {
        format!("cd {} && {}", shell_escape(dir), command)
    } else {
        command.to_string()
    };
    format!(
        "{}\n{}; echo \"\\n{}:$?\\n\"\n",
        DISABLE_HISTORY_EXPANSION_COMMAND, command_line, marker
    )
}

/// Build a background command for the persistent shell.
fn build_background_command(command: &str, cwd: Option<&str>) -> String {
    let command_line = if let Some(dir) = cwd {
        format!("cd {} && {}", shell_escape(dir), command)
    } else {
        command.to_string()
    };
    format!(
        "{}\n{} &\n",
        DISABLE_HISTORY_EXPANSION_COMMAND, command_line
    )
}

/// Escape a string for safe use in shell commands.
///
/// Uses single-quote wrapping with escaped embedded single quotes.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn input_does_not_cancel_a_consumed_pty_chunk_during_bookkeeping() {
        let mut session = TerminalSession::spawn(SessionConfig {
            shell: "/bin/sh".to_string(),
            cwd: std::env::current_dir().unwrap(),
            env: HashMap::new(),
            uid: None,
            gid: None,
            interactive: true,
            cols: 80,
            rows: 24,
            max_output_size: 4096,
            scrollback_size: 4096,
            full_scrollback_size: 4096,
        })
        .unwrap();
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(1);
        *session.pty_rx.get_mut() = output_rx;
        let session = Arc::new(session);

        let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let held_session = session.clone();
        let holder = tokio::task::spawn_blocking(move || {
            let _guard = held_session.full_scrollback.lock().unwrap();
            let _ = locked_tx.send(());
            release_rx.recv().unwrap();
        });
        locked_rx.await.unwrap();

        let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<()>(1);
        let reader_session = session.clone();
        let reader = tokio::spawn(async move {
            let owner = CancellationToken::new();
            tokio::select! {
                biased;
                _ = input_rx.recv() => None,
                output = reader_session.read_raw_for_attachment(&owner) => output.unwrap(),
            }
        });
        let chunk = b"cancel-safe-pty-output".to_vec();
        output_tx
            .send(ReadChunkResult::Data(chunk.clone()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if session.scrollback.lock().unwrap().replay_from(None).data == chunk {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // The old reader has taken the chunk and holds the receiver lock
        // while finishing byte bookkeeping. Reattach must wait at that same
        // boundary, then include the chunk in its replay snapshot.
        let replay_session = Arc::clone(&session);
        let replay = tokio::spawn(async move { replay_session.replay_from(Some(0)).await });
        tokio::task::yield_now().await;
        assert!(!replay.is_finished(), "reattach passed an in-flight reader");
        input_tx.send(()).await.unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), reader)
                .await
                .unwrap()
                .unwrap(),
            Some(chunk)
        );
        let snapshot = replay.await.unwrap();
        assert_eq!(snapshot.offset, 0);
        assert_eq!(snapshot.data, b"cancel-safe-pty-output");
        holder.await.unwrap();
        session.shutdown().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn idle_interactive_reader_waits_for_readiness_without_polling() {
        let session = TerminalSession::spawn(SessionConfig {
            shell: "/bin/sh".to_string(),
            cwd: std::env::current_dir().unwrap(),
            env: HashMap::new(),
            uid: None,
            gid: None,
            interactive: true,
            cols: 80,
            rows: 24,
            max_output_size: 4096,
            scrollback_size: 4096,
            full_scrollback_size: 4096,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let before = session.reader_attempts.load(Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let after = session.reader_attempts.load(Ordering::Relaxed);
        assert!(
            after - before <= 2,
            "idle reader made {} read attempts",
            after - before
        );
        session.shutdown().await.unwrap();
    }

    #[test]
    fn proc_stat_parser_handles_embedded_parentheses_and_spaces() {
        assert_eq!(
            parse_proc_stat_process_group("123 (shell ) with spaces) S 1 456 789 0 0"),
            Some(('S', 456, 789))
        );
        assert_eq!(
            parse_proc_stat_process_group("124 (child) Z 123 456 789 0 0"),
            Some(('Z', 456, 789))
        );
        assert_eq!(
            parse_proc_stat_process_group("125 (broken) S 1 x 789"),
            None
        );
        assert_eq!(
            parse_proc_stat_process_group("126 missing delimiters S 1 2 3"),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn pty_readiness_copies_close_on_exec() {
        let (master, _peer) = UnixStream::pair().unwrap();
        for _ in 0..2 {
            let copy = duplicate_pty_fd(master.as_raw_fd()).unwrap();
            // SAFETY: copy owns a live descriptor for this query.
            let flags = unsafe { libc::fcntl(copy.as_raw_fd(), libc::F_GETFD) };
            assert!(flags >= 0);
            assert_ne!(flags & libc::FD_CLOEXEC, 0);
        }
    }

    #[test]
    fn nonblocking_pty_write_stops_on_owner_or_shutdown_cancellation() {
        struct FullInputQueue;
        impl Write for FullInputQueue {
            fn write(&mut self, _data: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::WouldBlock.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        for cancel_owner in [true, false] {
            let owner = CancellationToken::new();
            let shutdown = CancellationToken::new();
            let trigger = if cancel_owner {
                owner.clone()
            } else {
                shutdown.clone()
            };
            let trigger_thread = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                trigger.cancel();
            });
            let result = write_pty_input(
                &mut FullInputQueue,
                b"blocked",
                Some(&owner),
                &shutdown,
                #[cfg(unix)]
                None,
            );
            trigger_thread.join().unwrap();
            assert!(matches!(result, Err(G5Error::NotFound { .. })));
        }
    }

    #[cfg(unix)]
    #[test]
    fn closed_write_waker_ends_blocked_write() {
        struct BlockedOnce(bool);
        impl Write for BlockedOnce {
            fn write(&mut self, _data: &[u8]) -> std::io::Result<usize> {
                assert!(!self.0, "write retried after its wake pipe closed");
                self.0 = true;
                Err(std::io::ErrorKind::WouldBlock.into())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let (pty, _pty_peer) = UnixStream::pair().unwrap();
        let (wake_reader, wake_writer) = UnixStream::pair().unwrap();
        drop(wake_writer);
        let mut wait_ready = || {
            wait_for_pty_ready(pty.as_raw_fd(), wake_reader.as_raw_fd(), libc::POLLOUT).map_err(
                |error| G5Error::Internal {
                    message: error.to_string(),
                },
            )
        };
        let result = write_pty_input(
            &mut BlockedOnce(false),
            b"blocked",
            None,
            &CancellationToken::new(),
            Some(&mut wait_ready),
        );
        assert!(matches!(result, Err(G5Error::NotFound { .. })));
    }

    #[test]
    fn test_parse_exit_code_from_marker() {
        let marker = "__G5_END_abc123__";
        assert_eq!(
            parse_exit_code_from_marker(&format!("{}:0\n", marker), marker),
            Some(0)
        );
        assert_eq!(
            parse_exit_code_from_marker(&format!("{}:1\n", marker), marker),
            Some(1)
        );
        assert_eq!(
            parse_exit_code_from_marker(&format!("{}:127\n", marker), marker),
            Some(127)
        );
        assert_eq!(parse_exit_code_from_marker("no marker here", marker), None);
    }

    #[test]
    fn test_parse_exit_code_multiline() {
        let marker = "__G5_END_xyz789__";
        let text = format!("some output\n{}:42\nmore stuff", marker);
        assert_eq!(parse_exit_code_from_marker(&text, marker), Some(42));
    }

    #[test]
    fn test_strip_command_echo() {
        let marker = "__G5_END_abc123__";
        let output = format!("echo hello; echo \"\\n{}:$?\\n\"\nhello\n", marker);
        let stripped = strip_command_echo(&output, marker);
        assert_eq!(stripped, "hello\n");
    }

    #[test]
    fn test_strip_command_echo_no_match() {
        let output = "line1\nline2\nline3\n";
        let stripped = strip_command_echo(output, "nonexistent_marker");
        assert_eq!(stripped, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("/path/to/dir"), "'/path/to/dir'");
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn test_build_foreground_command_disables_history_expansion_first() {
        let command = "python -c \"\nprint('All tests passed!')\n\"";
        let marker = "__G5_END_test__";
        let full_command = build_foreground_command(command, Some("/workspace/repo"), marker);

        assert!(full_command
            .starts_with("set +H 2>/dev/null || true\ncd '/workspace/repo' && python -c \""));
        assert!(full_command.contains("print('All tests passed!')"));
        assert!(full_command.ends_with("; echo \"\\n__G5_END_test__:$?\\n\"\n"));
    }

    #[test]
    fn test_build_background_command_disables_history_expansion_first() {
        let full_command = build_background_command("echo 'watch!'", None);

        assert_eq!(
            full_command,
            "set +H 2>/dev/null || true\necho 'watch!' &\n"
        );
    }

    #[test]
    fn test_byte_ring_buffer_basic() {
        let mut buf = ByteRingBuffer::new(8);
        buf.push(b"hello");
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_byte_ring_buffer_overflow() {
        let mut buf = ByteRingBuffer::new(8);
        buf.push(b"hello"); // 5 bytes
        buf.push(b"world!"); // 6 bytes, total 11 > capacity 8

        let contents = buf.contents();
        assert_eq!(contents.len(), 8);
        // "helloworld!" is 11 chars. Last 8: l-o-w-o-r-l-d-!
        assert_eq!(&contents, b"loworld!");
    }

    #[test]
    fn test_byte_ring_buffer_exact_capacity() {
        let mut buf = ByteRingBuffer::new(5);
        buf.push(b"hello"); // exactly 5 bytes = capacity
        assert_eq!(buf.contents(), b"hello");
    }

    #[test]
    fn test_byte_ring_buffer_double_wrap() {
        let mut buf = ByteRingBuffer::new(4);
        buf.push(b"abcdefghij"); // 10 bytes, wraps twice around 4-byte buffer
        let contents = buf.contents();
        assert_eq!(contents.len(), 4);
        assert_eq!(&contents, b"ghij"); // last 4 bytes
    }

    #[test]
    fn test_byte_ring_buffer_clear() {
        let mut buf = ByteRingBuffer::new(8);
        buf.push(b"hello");
        assert_eq!(buf.contents().len(), 5);
        buf.clear();
        assert_eq!(buf.contents().len(), 0);
    }

    /// `read_from` returns exactly the suffix from an offset, before and
    /// after the ring wraps, and refuses an evicted or future offset.
    #[test]
    fn read_from_returns_only_the_requested_suffix_across_the_wrap() {
        let mut ring = ByteRingBuffer::new(8);
        ring.push(b"abcde");
        assert_eq!(ring.read_from(0).unwrap(), b"abcde");
        assert_eq!(ring.read_from(3).unwrap(), b"de");
        assert_eq!(ring.read_from(5).unwrap(), b"");
        assert!(ring.read_from(6).is_none());
        ring.push(b"fghij");
        // Retained: offsets 2..10 = "cdefghij", wrapped at write_pos 2.
        assert!(ring.read_from(1).is_none());
        assert_eq!(ring.read_from(2).unwrap(), b"cdefghij");
        assert_eq!(ring.read_from(5).unwrap(), b"fghij");
        assert_eq!(ring.read_from(8).unwrap(), b"ij");
        assert_eq!(ring.read_from(10).unwrap(), b"");
        assert!(ring.read_from(11).is_none());
        for offset in 2..=10u64 {
            assert_eq!(
                ring.read_from(offset).unwrap(),
                ring.replay_from(Some(offset)).data,
                "read_from agrees with replay_from at {offset}"
            );
        }
    }

    #[test]
    fn replay_uses_exact_offsets_and_reports_evicted_history() {
        let mut buffer = ByteRingBuffer::new(5);
        buffer.push(b"abc");
        let initial = buffer.replay_from(None);
        assert_eq!(initial.offset, 0);
        assert_eq!(initial.data, b"abc");
        assert!(initial.reset);

        buffer.push(b"def");
        let resumed = buffer.replay_from(Some(3));
        assert_eq!(resumed.offset, 3);
        assert_eq!(resumed.data, b"def");
        assert!(!resumed.reset);

        let evicted = buffer.replay_from(Some(0));
        assert_eq!(evicted.offset, 1);
        assert_eq!(evicted.data, b"bcdef");
        assert!(evicted.reset);
        assert!(buffer.replay_from(Some(7)).reset);
    }

    #[test]
    fn test_command_output_struct() {
        let output = CommandOutput {
            output: "hello\n".to_string(),
            exit_code: Some(0),
            timed_out: false,
        };
        assert_eq!(output.exit_code, Some(0));
        assert!(!output.timed_out);
    }

    #[test]
    fn test_command_output_timeout() {
        let output = CommandOutput {
            output: "partial output\n[Command timed out after 300s]\n".to_string(),
            exit_code: None,
            timed_out: true,
        };
        assert!(output.timed_out);
        assert!(output.exit_code.is_none());
    }

    #[test]
    fn test_session_config_struct() {
        let config = SessionConfig {
            shell: "/bin/bash".to_string(),
            cwd: PathBuf::from("/tmp"),
            env: HashMap::new(),
            uid: None,
            gid: None,
            interactive: false,
            cols: 80,
            rows: 24,
            max_output_size: 61440,
            scrollback_size: 4096,
            full_scrollback_size: 65536,
        };
        assert_eq!(config.shell, "/bin/bash");
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
        assert_eq!(config.max_output_size, 61440);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_discards_queued_output_and_closes_the_reader_channel() {
        let mut session = TerminalSession::spawn(SessionConfig {
            shell: "/bin/sh".to_string(),
            cwd: std::env::current_dir().unwrap(),
            env: HashMap::new(),
            uid: None,
            gid: None,
            interactive: true,
            cols: 80,
            rows: 24,
            max_output_size: 4096,
            scrollback_size: 4096,
            full_scrollback_size: 4096,
        })
        .unwrap();
        // Keep a producer alive with unread output, as when the shell's final
        // newline arrives after the last browser or polling read.
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(2);
        *session.pty_rx.get_mut() = output_rx;
        output_tx
            .send(ReadChunkResult::Data(
                b"queued before shutdown\r\n".to_vec(),
            ))
            .await
            .unwrap();
        session.scrollback.lock().unwrap().push(b"retained replay");
        session
            .full_scrollback
            .lock()
            .unwrap()
            .push(b"retained poll");

        // A producer can already own capacity when shutdown closes the queue.
        let in_flight = output_tx.clone().reserve_owned().await.unwrap();
        let session = Arc::new(session);
        let shutting_down = Arc::clone(&session);
        let shutdown = tokio::spawn(async move { shutting_down.shutdown().await });
        tokio::time::timeout(Duration::from_secs(10), async {
            while !output_tx.is_closed() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("shutdown closes the queue");
        assert!(
            !shutdown.is_finished(),
            "shutdown passed an in-flight sender"
        );
        in_flight.send(ReadChunkResult::Data(b"arrived during shutdown".to_vec()));
        tokio::time::timeout(Duration::from_secs(10), shutdown)
            .await
            .expect("shutdown consumes in-flight output")
            .unwrap()
            .unwrap();
        assert_eq!(session.drain_output().await.unwrap(), "");
        assert_eq!(session.retained_output_bytes(), 0);
        assert!(output_tx.is_closed());
        assert!(session.read_raw().await.unwrap().is_none());
        assert!(session.replay_from(None).await.data.is_empty());
        session.shutdown().await.unwrap();
    }

    /// `g5.terminal.output-retention`: sustained output far beyond both
    /// windows, consumed through the browser streaming path alone, leaves
    /// nothing retained beyond those windows; a later polling read receives
    /// the retained tail and is told what was lost, and reads nothing twice.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn streaming_output_is_retained_only_within_the_bounded_windows() {
        let scrollback_size = 4096;
        let full_scrollback_size = 16 * 1024;
        let session = Arc::new(
            TerminalSession::spawn(SessionConfig {
                shell: "/bin/sh".to_string(),
                cwd: std::env::current_dir().unwrap(),
                // An empty prompt: the shell otherwise prints one after each
                // command, racing the assertions that a drain reads nothing.
                env: HashMap::from([
                    ("PATH".to_string(), "/usr/bin:/bin".to_string()),
                    ("PS1".to_string(), String::new()),
                ]),
                uid: None,
                gid: None,
                interactive: true,
                cols: 80,
                rows: 24,
                max_output_size: 4096,
                scrollback_size,
                full_scrollback_size,
            })
            .unwrap(),
        );
        // Well beyond both windows together.
        let total = 1024 * 1024;
        session
            .write_raw(
                format!("head -c {total} /dev/zero | tr '\\0' x; printf '\\nSTREAM_%s\\n' DONE\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut streamed = Vec::new();
        tokio::time::timeout(Duration::from_secs(30), async {
            // Wait for the marker's line terminator too: the PTY may deliver
            // it in a later chunk, and a drain must then find nothing left.
            while !(streamed.windows(11).any(|w| w == b"STREAM_DONE") && streamed.ends_with(b"\n"))
            {
                match session.read_raw().await.unwrap() {
                    Some(bytes) => streamed.extend(bytes),
                    None => panic!("shell exited early: {}", String::from_utf8_lossy(&streamed)),
                }
            }
        })
        .await
        .expect("streamed output");
        assert!(streamed.len() >= total, "streamed {} bytes", streamed.len());
        assert!(
            session.retained_output_bytes() <= scrollback_size + full_scrollback_size,
            "retained {} bytes after streaming {}",
            session.retained_output_bytes(),
            streamed.len()
        );

        let polled = session.drain_output().await.unwrap();
        assert!(
            polled.starts_with("\n[")
                && polled.contains("bytes of earlier output were not retained]"),
            "{polled:?}"
        );
        assert!(polled.contains("STREAM_DONE"), "{polled:?}");
        assert!(
            polled.len() <= full_scrollback_size + 80,
            "{}",
            polled.len()
        );
        assert_eq!(session.drain_output().await.unwrap(), "");

        // Within the window, polling is lossless and reports no loss.
        session
            .write_raw(b"printf 'SMALL_%s\\n' OUTPUT\n")
            .await
            .unwrap();
        let mut small = String::new();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !(small.contains("SMALL_OUTPUT") && small.ends_with('\n')) {
                small.push_str(&session.drain_output().await.unwrap());
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("polled output");
        assert!(!small.contains("not retained"), "{small:?}");
        session.shutdown().await.unwrap();
        assert_eq!(session.drain_output().await.unwrap(), "");
    }
}
