//! PTY session manager — top-level orchestrator for all terminal sessions within a worker.
//!
//! The `TerminalManager` manages session creation, lookup, reuse, and cleanup.
//! It provides the public API consumed by the `tools/terminal.rs` tool implementation
//! and the `connection/streams.rs` terminal stream.
//!
//! Enforces session limits, tracks the "last active" session for default reuse,
//! and coordinates graceful shutdown of all sessions when the task completes or fails.

pub mod hosted;
pub mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub use hosted::{HostedCursor, HostedPty, HostedReadEnd, HostedSession};
pub use session::{CommandOutput, ReplaySnapshot, SessionConfig, TerminalSession};

/// Closed set of reasons for removing a browser terminal session.
#[derive(Clone, Copy, Debug)]
pub enum TerminalCloseReason {
    WorkspaceStop,
    ForkInterrupt,
    ConnectionEnd,
    StreamFailure,
    ClientClose,
    ProcessExit,
    DetachGraceExpired,
    DrainTimeout,
    ReattachWriteInFlight,
    RetiringRetry,
    /// A hosted session's writer closed and its retained output was
    /// delivered; the viewer's stream ends while the session stays attachable.
    SessionEnded,
    /// A hosted session's viewer fell further behind than the ring retains;
    /// its stream ends so it can reattach with its offset and reset.
    ViewerLagged,
    /// A hosted session was discarded because the same name was hosted again.
    HostReplaced,
}

impl TerminalCloseReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceStop => "workspace_stop",
            Self::ForkInterrupt => "fork_interrupt",
            Self::ConnectionEnd => "connection_end",
            Self::StreamFailure => "stream_failure",
            Self::ClientClose => "client_close",
            Self::ProcessExit => "process_exit",
            Self::DetachGraceExpired => "detach_grace_expired",
            Self::DrainTimeout => "drain_timeout",
            Self::ReattachWriteInFlight => "reattach_write_in_flight",
            Self::RetiringRetry => "retiring_retry",
            Self::SessionEnded => "session_ended",
            Self::ViewerLagged => "viewer_lagged",
            Self::HostReplaced => "host_replaced",
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalConfig
// ---------------------------------------------------------------------------

/// Configuration for the terminal manager.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// Maximum number of concurrent terminal sessions.
    pub max_sessions: usize,
    /// Default shell to use (None = $SHELL or /bin/bash).
    pub default_shell: Option<String>,
    /// Default command timeout in seconds.
    pub default_timeout_secs: u64,
    /// Default working directory for new sessions.
    pub workspace_path: PathBuf,
    /// Maximum output size per command in bytes.
    pub max_output_size: usize,
    /// Scrollback buffer size per session in bytes (for reconnection replay).
    pub scrollback_size: usize,
    /// Full scrollback buffer size per session in bytes (for get_output polling).
    pub full_scrollback_size: usize,
    /// Environment variables to inject into all sessions.
    pub env_vars: HashMap<String, String>,
    /// Unprivileged uid used for terminal shells inside a workspace VM.
    pub uid: Option<u32>,
    /// Unprivileged gid used for terminal shells inside a workspace VM.
    pub gid: Option<u32>,
    /// Default PTY columns.
    pub default_cols: u16,
    /// Default PTY rows.
    pub default_rows: u16,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            max_sessions: 10,
            default_shell: None,
            default_timeout_secs: 300,
            workspace_path: PathBuf::from("."),
            max_output_size: 61440, // 60KB
            scrollback_size: 4096,
            full_scrollback_size: 65536, // 64KB
            env_vars: HashMap::new(),
            uid: None,
            gid: None,
            default_cols: 80,
            default_rows: 24,
        }
    }
}

// ---------------------------------------------------------------------------
// SessionState
// ---------------------------------------------------------------------------

/// Terminal session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session is executing a command.
    Running,
    /// Session is idle, waiting for next command.
    Idle,
    /// Shell process has exited normally.
    Completed,
    /// Shell process has exited with an error or was killed.
    Failed,
}

// ---------------------------------------------------------------------------
// RunInTerminalArgs
// ---------------------------------------------------------------------------

/// Arguments for run_in_terminal (matches AGENTS.md §6.3 interface).
#[derive(Debug, Clone, Deserialize)]
pub struct RunInTerminalArgs {
    /// Shell command to execute.
    pub command: String,
    /// Working directory (defaults to workspace root).
    pub cwd: Option<String>,
    /// Max seconds before timeout (default: 300).
    pub timeout: Option<u64>,
    /// Reuse a specific terminal session.
    pub terminal_id: Option<String>,
    /// Force a fresh terminal session.
    pub new_terminal: Option<bool>,
    /// Run without blocking the agent loop.
    pub is_background: Option<bool>,
}

// ---------------------------------------------------------------------------
// TerminalResult
// ---------------------------------------------------------------------------

/// Result from run_in_terminal (matches AGENTS.md §6.3 interface).
#[derive(Debug, Clone, Serialize)]
pub struct TerminalResult {
    /// Session ID for reuse in subsequent calls.
    pub terminal_id: String,
    /// Exit code (None for background processes or timed-out commands).
    pub exit_code: Option<i32>,
    /// Command output (stdout + stderr merged via PTY, truncated to 60KB).
    pub stdout: String,
    /// Always empty for PTY sessions (stdout/stderr are merged).
    pub stderr: String,
}

// ---------------------------------------------------------------------------
// SessionInfo
// ---------------------------------------------------------------------------

/// Summary information about a terminal session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub terminal_id: String,
    /// Current session state.
    pub state: SessionState,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 timestamp of last command execution.
    pub last_command_at: Option<String>,
    /// PID of the shell process.
    pub pid: u32,
    /// Whether this session was started as a background session.
    pub is_background: bool,
}

// ---------------------------------------------------------------------------
// TerminalManager
// ---------------------------------------------------------------------------

/// Internal per-session metadata tracked by the manager.
struct ManagedSession {
    session: Arc<TerminalSession>,
    is_background: bool,
    interactive: bool,
    attachment_generation: u64,
    attached: bool,
    retiring: bool,
    attachment_cancel: Option<CancellationToken>,
}

/// One generation of browser ownership over an interactive terminal. A newer
/// attachment cancels the previous generation, fencing delayed input and output.
pub struct InteractiveAttachment {
    pub session: Arc<TerminalSession>,
    pub generation: u64,
    pub cancel: CancellationToken,
    pub reattached: bool,
}

/// A hosted session the manager retains, with its current viewer generation.
/// Hosted sessions live apart from shells: they never count toward
/// `max_sessions`, never become `last_active_id` and are not enumerated.
struct HostedManaged {
    session: Arc<HostedSession>,
    attachment_generation: u64,
    attached: bool,
    attachment_cancel: Option<CancellationToken>,
}

/// One viewer generation over a hosted session. A newer attachment cancels
/// the previous one exactly as for a shell; the replay snapshot and the
/// cursor were taken atomically at the ring boundary.
pub struct HostedAttachment {
    pub session: Arc<HostedSession>,
    pub generation: u64,
    pub cancel: CancellationToken,
    pub reattached: bool,
    pub replay: ReplaySnapshot,
    pub cursor: HostedCursor,
}

/// Manages all terminal sessions for a worker.
///
/// Thread-safe via internal `RwLock`. All public methods take `&self`.
pub struct TerminalManager {
    /// Module configuration.
    config: TerminalConfig,
    /// Active sessions keyed by terminal_id.
    sessions: RwLock<HashMap<String, ManagedSession>>,
    /// ID of the last actively used session (for default reuse).
    last_active_id: RwLock<Option<String>>,
    /// Hosted (read-only, dispatcher-owned) sessions keyed by name.
    hosted: RwLock<HashMap<String, HostedManaged>>,
}

impl TerminalManager {
    /// Create a new terminal manager.
    #[must_use]
    pub fn new(config: TerminalConfig) -> Self {
        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            last_active_id: RwLock::new(None),
            hosted: RwLock::new(HashMap::new()),
        }
    }

    // -- Hosted sessions (`g5.preview.output.live-tail`) --

    /// Host a PTY under `name` for a module child: open the pair, keep and
    /// drain the master, return the slave to pass to the child. A session
    /// already hosted under that name (the previous preview) is discarded
    /// first, its viewer superseded. The session's retained output is bounded
    /// by `capacity`.
    pub async fn host_session(
        &self,
        name: &str,
        capacity: usize,
        cols: u16,
        rows: u16,
    ) -> Result<HostedPty, G5Error> {
        if name.is_empty() || name.len() > g5_contracts::terminal::MAX_TERMINAL_ID_BYTES {
            return Err(G5Error::Internal {
                message: "hosted terminal name must be 1 to 128 bytes".to_string(),
            });
        }
        if capacity == 0 {
            return Err(G5Error::Internal {
                message: "hosted terminal capacity must be nonzero".to_string(),
            });
        }
        let hosted = HostedSession::open(name, capacity, cols.max(1), rows.max(1))?;
        let previous = {
            let mut sessions = self.hosted.write().await;
            sessions.insert(
                name.to_string(),
                HostedManaged {
                    session: Arc::clone(&hosted.session),
                    attachment_generation: 0,
                    attached: false,
                    attachment_cancel: None,
                },
            )
        };
        if let Some(previous) = previous {
            tracing::info!(terminal_id = %name, close_reason = TerminalCloseReason::HostReplaced.as_str(), "discarding hosted terminal");
            if let Some(cancel) = previous.attachment_cancel {
                cancel.cancel();
            }
            previous.session.end().await;
        }
        tracing::info!(terminal_id = %name, capacity, "hosted terminal session");
        Ok(hosted)
    }

    /// Attach a viewer to a hosted session by name, superseding the previous
    /// viewer. Only an existing session can be attached: this never creates
    /// one. An ended session attaches too, for replay of its retained tail.
    pub async fn attach_hosted(
        &self,
        name: &str,
        cols: u16,
        rows: u16,
        offset: Option<u64>,
    ) -> Result<HostedAttachment, G5Error> {
        if cols == 0 || rows == 0 {
            return Err(G5Error::Internal {
                message: "terminal dimensions must be nonzero".to_string(),
            });
        }
        let mut sessions = self.hosted.write().await;
        let managed = sessions.get_mut(name).ok_or_else(|| G5Error::NotFound {
            resource: format!("hosted terminal session: {name}"),
        })?;
        // Resize before the snapshot so the child's redraw, if any, lands in
        // the new viewer's stream rather than the old one's.
        managed.session.resize(cols, rows).await?;
        if let Some(previous) = managed.attachment_cancel.take() {
            previous.cancel();
        }
        let reattached = managed.attachment_generation > 0;
        managed.attachment_generation = managed.attachment_generation.saturating_add(1);
        managed.attached = true;
        let cancel = CancellationToken::new();
        managed.attachment_cancel = Some(cancel.clone());
        let (replay, cursor) = managed.session.replay_from(offset);
        Ok(HostedAttachment {
            session: Arc::clone(&managed.session),
            generation: managed.attachment_generation,
            cancel,
            reattached,
            replay,
            cursor,
        })
    }

    /// Mark a hosted viewer generation gone. The session is unaffected: a
    /// hosted session ends only with its writer or the manager.
    pub async fn detach_hosted(&self, name: &str, generation: u64) -> bool {
        let mut sessions = self.hosted.write().await;
        let Some(managed) = sessions.get_mut(name) else {
            return false;
        };
        if managed.attachment_generation != generation {
            return false;
        }
        managed.attached = false;
        if let Some(cancel) = managed.attachment_cancel.take() {
            cancel.cancel();
        }
        true
    }

    /// Resize a hosted session only for its current viewer.
    pub async fn resize_hosted(
        &self,
        name: &str,
        generation: u64,
        cols: u16,
        rows: u16,
    ) -> Result<(), G5Error> {
        let sessions = self.hosted.read().await;
        let managed = sessions
            .get(name)
            .filter(|managed| managed.attachment_generation == generation && managed.attached)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("hosted terminal session: {name}"),
            })?;
        managed.session.resize(cols, rows).await
    }

    /// The hosted session under `name`, if the manager retains one.
    pub async fn hosted_session(&self, name: &str) -> Option<Arc<HostedSession>> {
        self.hosted
            .read()
            .await
            .get(name)
            .map(|managed| Arc::clone(&managed.session))
    }

    /// Whether `name` has a current hosted viewer.
    pub async fn hosted_attached(&self, name: &str) -> bool {
        self.hosted
            .read()
            .await
            .get(name)
            .is_some_and(|managed| managed.attached)
    }

    /// Discard every hosted session: supersede viewers, close masters, drop
    /// the retained output. Part of workspace stop and ownership transfer.
    async fn discard_hosted(&self, reason: TerminalCloseReason) {
        self.discard_hosted_where(reason, |_| true).await;
    }

    /// Discard the hosted sessions whose names satisfy `select`. A preview
    /// start uses this to retain at most one session in its namespace: the
    /// previous preview's output goes when the next one is hosted.
    pub async fn discard_hosted_where(
        &self,
        reason: TerminalCloseReason,
        select: impl Fn(&str) -> bool,
    ) {
        let discarded = {
            let mut sessions = self.hosted.write().await;
            let names = sessions
                .keys()
                .filter(|name| select(name))
                .cloned()
                .collect::<Vec<_>>();
            names
                .into_iter()
                .filter_map(|name| sessions.remove(&name).map(|managed| (name, managed)))
                .collect::<Vec<_>>()
        };
        for (name, managed) in discarded {
            tracing::info!(terminal_id = %name, close_reason = reason.as_str(), "discarding hosted terminal");
            if let Some(cancel) = managed.attachment_cancel {
                cancel.cancel();
            }
            managed.session.end().await;
        }
    }

    /// Execute a command in a terminal session.
    ///
    /// This is the primary API used by the `run_in_terminal` tool.
    ///
    /// Session resolution algorithm:
    /// 1. `new_terminal: true` → create new session
    /// 2. `terminal_id: Some(id)` → lookup specific session
    /// 3. Default → reuse last active session, or create if none exists
    ///
    /// # Errors
    /// - `G5Error::NotFound` if a specific `terminal_id` is requested but doesn't exist
    /// - `G5Error::Internal` if session limit is reached or PTY spawn fails
    pub async fn execute(&self, args: RunInTerminalArgs) -> Result<TerminalResult, G5Error> {
        let new_terminal = args.new_terminal.unwrap_or(false);
        let is_background = args.is_background.unwrap_or(false);
        let timeout_secs = args.timeout.unwrap_or(self.config.default_timeout_secs);
        let timeout = std::time::Duration::from_secs(timeout_secs);

        // Resolve session
        let session = self
            .resolve_session(args.terminal_id.as_deref(), new_terminal, is_background)
            .await?;

        let terminal_id = session.id().to_string();

        // Update last active
        {
            let mut last = self.last_active_id.write().await;
            *last = Some(terminal_id.clone());
        }

        // When no cwd is specified, default to the workspace root so that
        // reused sessions don't silently inherit a stale working directory
        // from a previous command.
        let effective_cwd = args
            .cwd
            .or_else(|| self.config.workspace_path.to_str().map(String::from));

        if is_background {
            // Background execution: write command and return immediately
            session
                .execute_background(&args.command, effective_cwd.as_deref())
                .await?;

            Ok(TerminalResult {
                terminal_id,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            })
        } else {
            // Foreground execution: wait for completion
            let output = session
                .execute_command(&args.command, effective_cwd.as_deref(), timeout)
                .await?;

            Ok(TerminalResult {
                terminal_id,
                exit_code: output.exit_code,
                stdout: output.output,
                stderr: String::new(), // PTY merges stdout/stderr
            })
        }
    }

    /// Get the output of a background terminal session.
    ///
    /// Returns output accumulated since the last call to `get_output` for this session.
    ///
    /// # Errors
    /// - `G5Error::NotFound` if the terminal_id doesn't exist
    pub async fn get_output(&self, terminal_id: &str) -> Result<String, G5Error> {
        let sessions = self.sessions.read().await;
        let managed = sessions.get(terminal_id).ok_or_else(|| G5Error::NotFound {
            resource: format!("terminal session: {}", terminal_id),
        })?;

        managed.session.drain_output().await
    }

    /// List all active terminal sessions.
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        // Use try_read to avoid blocking; return empty if lock is held
        let sessions = match self.sessions.try_read() {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "Cannot acquire sessions read lock — returning empty list");
                return Vec::new();
            }
        };

        sessions
            .values()
            .map(|managed| {
                let s = &managed.session;
                SessionInfo {
                    terminal_id: s.id().to_string(),
                    state: s.state(),
                    created_at: s.created_at().to_rfc3339(),
                    last_command_at: s.last_command_at().map(|t| t.to_rfc3339()),
                    pid: s.pid(),
                    is_background: managed.is_background,
                }
            })
            .collect()
    }

    /// Get information about a specific session.
    pub fn get_session_info(&self, terminal_id: &str) -> Option<SessionInfo> {
        let sessions = self.sessions.try_read().ok()?;
        let managed = sessions.get(terminal_id)?;
        let s = &managed.session;
        Some(SessionInfo {
            terminal_id: s.id().to_string(),
            state: s.state(),
            created_at: s.created_at().to_rfc3339(),
            last_command_at: s.last_command_at().map(|t| t.to_rfc3339()),
            pid: s.pid(),
            is_background: managed.is_background,
        })
    }

    /// Resize a terminal session's PTY dimensions.
    ///
    /// # Errors
    /// - `G5Error::NotFound` if the terminal_id doesn't exist
    pub async fn resize(&self, terminal_id: &str, cols: u16, rows: u16) -> Result<(), G5Error> {
        let sessions = self.sessions.read().await;
        let managed = sessions.get(terminal_id).ok_or_else(|| G5Error::NotFound {
            resource: format!("terminal session: {}", terminal_id),
        })?;

        managed.session.resize(cols, rows).await
    }

    /// Open a dedicated browser-facing interactive PTY. Unlike the command
    /// helper, this never reuses another caller's session.
    pub async fn open_interactive(
        &self,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<TerminalSession>, G5Error> {
        self.open_interactive_with_env(cols, rows, HashMap::new())
            .await
    }

    /// Open a browser-facing PTY with the current owner's dispatch-time
    /// secrets added to the fixed workspace Git environment. The environment
    /// belongs only to this fresh session and is never written into the
    /// manager-wide configuration or reused by another owner.
    pub async fn open_interactive_with_env(
        &self,
        cols: u16,
        rows: u16,
        env: HashMap<String, String>,
    ) -> Result<Arc<TerminalSession>, G5Error> {
        Ok(self
            .attach_interactive(None, cols, rows, env)
            .await?
            .session)
    }

    /// Create or reattach a browser-facing PTY. Reattachment is permitted only
    /// for a live interactive session in this manager; the API authenticates
    /// current workspace ownership before a request reaches this boundary.
    pub async fn attach_interactive(
        &self,
        terminal_id: Option<&str>,
        cols: u16,
        rows: u16,
        env: HashMap<String, String>,
    ) -> Result<InteractiveAttachment, G5Error> {
        if cols == 0 || rows == 0 {
            return Err(G5Error::Internal {
                message: "terminal dimensions must be nonzero".to_string(),
            });
        }

        if let Some(terminal_id) = terminal_id {
            let (attachment, retire_generation) = {
                let mut sessions = self.sessions.write().await;
                let managed = sessions
                    .get_mut(terminal_id)
                    .ok_or_else(|| G5Error::NotFound {
                        resource: format!("interactive terminal session: {terminal_id}"),
                    })?;
                if !managed.interactive || !managed.session.is_alive() {
                    return Err(G5Error::NotFound {
                        resource: format!("interactive terminal session: {terminal_id}"),
                    });
                }
                // A blocking PTY write cannot be aborted safely mid-syscall.
                // Retire this identity before letting a new attachment send
                // input, so no old-generation bytes can reach its shell.
                if managed.retiring || managed.session.has_interactive_write_in_flight() {
                    managed.retiring = true;
                    managed.attached = false;
                    if let Some(previous) = managed.attachment_cancel.take() {
                        previous.cancel();
                    }
                    (None, Some(managed.attachment_generation))
                } else {
                    // Resize while the previous attachment still owns the
                    // session. If this future is aborted while waiting for
                    // the PTY master lock, no new generation is left attached
                    // without a connection cleanup guard.
                    managed.session.resize(cols, rows).await?;
                    if let Some(previous) = managed.attachment_cancel.take() {
                        previous.cancel();
                    }
                    managed.attachment_generation = managed.attachment_generation.saturating_add(1);
                    managed.attached = true;
                    let cancel = CancellationToken::new();
                    managed.attachment_cancel = Some(cancel.clone());
                    (
                        Some((
                            Arc::clone(&managed.session),
                            managed.attachment_generation,
                            cancel,
                        )),
                        None,
                    )
                }
            };
            if let Some(generation) = retire_generation {
                tracing::info!(%terminal_id, generation, close_reason = TerminalCloseReason::ReattachWriteInFlight.as_str(), "closing terminal");
                self.close_interactive(terminal_id, generation).await?;
                return Err(G5Error::NotFound {
                    resource: format!("interactive terminal session: {terminal_id}"),
                });
            }
            let (session, generation, cancel) = attachment.expect("reattachment or retirement");
            return Ok(InteractiveAttachment {
                session,
                generation,
                cancel,
                reattached: true,
            });
        }

        let cancel = CancellationToken::new();
        let session = self
            .create_new_session_with_size_and_env(
                false,
                true,
                cols,
                rows,
                env,
                Some(cancel.clone()),
            )
            .await?;
        Ok(InteractiveAttachment {
            session,
            generation: 1,
            cancel,
            reattached: false,
        })
    }

    /// Mark an attachment generation disconnected without destroying its PTY.
    /// Returns true only for the current generation.
    pub async fn detach_interactive(&self, terminal_id: &str, generation: u64) -> bool {
        let mut sessions = self.sessions.write().await;
        let Some(managed) = sessions.get_mut(terminal_id) else {
            return false;
        };
        if !managed.interactive || managed.attachment_generation != generation {
            return false;
        }
        managed.attached = false;
        true
    }

    /// Write an input packet only while the attachment generation still owns
    /// the interactive session. Registry access is short: a full PTY input
    /// queue must not block reattachment or workspace shutdown.
    pub async fn write_interactive(
        &self,
        terminal_id: &str,
        generation: u64,
        data: &[u8],
    ) -> Result<(), G5Error> {
        let (session, cancel, permit) = {
            let sessions = self.sessions.read().await;
            let managed = sessions
                .get(terminal_id)
                .filter(|managed| {
                    managed.interactive
                        && managed.attachment_generation == generation
                        && managed.attached
                        && !managed.retiring
                })
                .ok_or_else(|| G5Error::NotFound {
                    resource: format!("interactive terminal session: {terminal_id}"),
                })?;
            let cancel = managed
                .attachment_cancel
                .clone()
                .expect("attached interactive session has cancellation token");
            (
                Arc::clone(&managed.session),
                cancel,
                managed.session.reserve_interactive_write(),
            )
        };
        session.write_interactive_raw(data, cancel, permit).await
    }

    /// Resize an interactive session only for its current attachment owner.
    pub async fn resize_interactive(
        &self,
        terminal_id: &str,
        generation: u64,
        cols: u16,
        rows: u16,
    ) -> Result<(), G5Error> {
        let sessions = self.sessions.read().await;
        let managed = sessions
            .get(terminal_id)
            .filter(|managed| {
                managed.interactive
                    && managed.attachment_generation == generation
                    && managed.attached
            })
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("interactive terminal session: {terminal_id}"),
            })?;
        managed.session.resize(cols, rows).await
    }

    /// Close a detached terminal only if no newer attachment claimed it during
    /// the reconnect grace period.
    pub async fn close_if_detached(
        &self,
        terminal_id: &str,
        generation: u64,
    ) -> Result<bool, G5Error> {
        let should_close = {
            let sessions = self.sessions.read().await;
            sessions.get(terminal_id).is_some_and(|managed| {
                managed.interactive
                    && managed.attachment_generation == generation
                    && !managed.attached
            })
        };
        if !should_close {
            return Ok(false);
        }
        self.close_generation(terminal_id, Some(generation)).await?;
        Ok(true)
    }

    /// Close one interactive session and remove it from the manager before
    /// returning, so lifecycle quiescence can rely on the session count.
    pub async fn close(&self, terminal_id: &str) -> Result<(), G5Error> {
        self.close_generation(terminal_id, None).await
    }

    /// Close an interactive attachment only when it still owns the current
    /// generation. A delayed transport teardown must never kill a newer
    /// owner's reattached PTY.
    pub async fn close_interactive(
        &self,
        terminal_id: &str,
        generation: u64,
    ) -> Result<(), G5Error> {
        self.close_generation(terminal_id, Some(generation)).await
    }

    async fn close_generation(
        &self,
        terminal_id: &str,
        generation: Option<u64>,
    ) -> Result<(), G5Error> {
        let session = {
            let mut sessions = self.sessions.write().await;
            if generation.is_some_and(|expected| {
                sessions
                    .get(terminal_id)
                    .is_none_or(|managed| managed.attachment_generation != expected)
            }) {
                return Ok(());
            }
            sessions.get_mut(terminal_id).map(|managed| {
                // Keep the retiring entry visible until shutdown completes.
                // Connection drain may abort this caller while shutdown is
                // between SIGTERM and SIGKILL; a later close can then retry.
                managed.retiring = true;
                managed.attached = false;
                if let Some(cancel) = managed.attachment_cancel.take() {
                    cancel.cancel();
                }
                Arc::clone(&managed.session)
            })
        };
        if let Some(session) = session {
            session.shutdown().await?;
            let mut sessions = self.sessions.write().await;
            if sessions
                .get(terminal_id)
                .is_some_and(|managed| Arc::ptr_eq(&managed.session, &session))
            {
                sessions.remove(terminal_id);
            }
        }
        let mut last = self.last_active_id.write().await;
        if last.as_deref() == Some(terminal_id) {
            *last = None;
        }
        Ok(())
    }

    /// Shut down all terminal sessions gracefully.
    ///
    /// Shutdown sequence:
    /// 1. Send SIGTERM to all running child processes in all sessions
    /// 2. Wait up to 5 seconds for graceful exit
    /// 3. Send SIGKILL to any remaining processes
    /// 4. Close all PTY file descriptors
    /// 5. Drain and discard any remaining output buffers
    /// 6. Deregister the terminal StreamSource from the connection module
    pub async fn shutdown(&self) -> Result<(), G5Error> {
        self.shutdown_with_reason(TerminalCloseReason::WorkspaceStop)
            .await
    }

    /// Shut down every registered terminal for a specific lifecycle trigger.
    /// Hosted sessions are discarded with their retained output.
    pub async fn shutdown_with_reason(&self, reason: TerminalCloseReason) -> Result<(), G5Error> {
        tracing::info!(
            close_reason = reason.as_str(),
            "Terminal manager shutting down all sessions"
        );
        self.discard_hosted(reason).await;

        let sessions = {
            let mut registered = self.sessions.write().await;
            registered
                .iter_mut()
                .map(|(id, managed)| {
                    managed.retiring = true;
                    managed.attached = false;
                    if let Some(cancel) = managed.attachment_cancel.take() {
                        cancel.cancel();
                    }
                    (
                        id.clone(),
                        managed.attachment_generation,
                        Arc::clone(&managed.session),
                    )
                })
                .collect::<Vec<_>>()
        };
        let session_count = sessions.len();
        let results = futures_util::future::join_all(sessions.into_iter().map(
            |(id, generation, session)| async move {
                tracing::info!(terminal_id = %id, generation, close_reason = reason.as_str(), "closing terminal during manager shutdown");
                let result = session.shutdown().await;
                (id, session, result)
            }))
            .await;
        let mut first_error = None;
        {
            let mut registered = self.sessions.write().await;
            for (id, session, result) in results {
                match result {
                    Ok(()) => {
                        if registered
                            .get(&id)
                            .is_some_and(|managed| Arc::ptr_eq(&managed.session, &session))
                        {
                            registered.remove(&id);
                        }
                    }
                    Err(error) if first_error.is_none() => first_error = Some(error),
                    Err(_) => {}
                }
            }
        }

        let mut last = self.last_active_id.write().await;
        *last = None;

        tracing::info!(session_count, "Terminal manager shutdown complete");
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    // -- Internal helpers --

    /// Resolve a session based on the provided arguments.
    ///
    /// Session resolution algorithm:
    /// 1. `new_terminal` → create new session
    /// 2. `terminal_id` → lookup specific session (error if not found)
    /// 3. Default → reuse last active session if alive, or create new
    async fn resolve_session(
        &self,
        terminal_id: Option<&str>,
        new_terminal: bool,
        is_background: bool,
    ) -> Result<Arc<TerminalSession>, G5Error> {
        if new_terminal {
            return self.create_new_session(is_background).await;
        }

        if let Some(id) = terminal_id {
            let sessions = self.sessions.read().await;
            if let Some(managed) = sessions.get(id) {
                if managed.session.is_alive() && !managed.retiring {
                    return Ok(Arc::clone(&managed.session));
                }
            }
            return Err(G5Error::NotFound {
                resource: format!("Terminal session not found: {}", id),
            });
        }

        // Default: reuse last active session, or create if none exists
        {
            let last = self.last_active_id.read().await;
            if let Some(ref last_id) = *last {
                let sessions = self.sessions.read().await;
                if let Some(managed) = sessions.get(last_id) {
                    if managed.session.is_alive() && !managed.retiring {
                        return Ok(Arc::clone(&managed.session));
                    }
                }
            }
        }

        self.create_new_session(is_background).await
    }

    /// Create a new terminal session, enforcing the session limit.
    async fn create_new_session(
        &self,
        is_background: bool,
    ) -> Result<Arc<TerminalSession>, G5Error> {
        self.create_new_session_with_size(
            is_background,
            self.config.default_cols,
            self.config.default_rows,
        )
        .await
    }

    async fn create_new_session_with_size(
        &self,
        is_background: bool,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<TerminalSession>, G5Error> {
        self.create_new_session_with_size_and_env(
            is_background,
            false,
            cols,
            rows,
            HashMap::new(),
            None,
        )
        .await
    }

    async fn create_new_session_with_size_and_env(
        &self,
        is_background: bool,
        interactive: bool,
        cols: u16,
        rows: u16,
        extra_env: HashMap<String, String>,
        attachment_cancel: Option<CancellationToken>,
    ) -> Result<Arc<TerminalSession>, G5Error> {
        // A previous retirement may have failed while its PTY was still
        // draining. Retry cleanup before counting it against the new owner's
        // session limit; never reattach to a retiring identity.
        self.reap_retiring_sessions().await;
        // Determine shell
        let shell = self
            .config
            .default_shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| {
                if std::path::Path::new("/bin/bash").exists() {
                    "/bin/bash".to_string()
                } else {
                    "/bin/sh".to_string()
                }
            });

        let mut env = self.config.env_vars.clone();
        env.extend(extra_env);
        let session_config = SessionConfig {
            shell,
            cwd: self.config.workspace_path.clone(),
            env,
            uid: self.config.uid,
            gid: self.config.gid,
            interactive,
            cols,
            rows,
            max_output_size: self.config.max_output_size,
            scrollback_size: self.config.scrollback_size,
            full_scrollback_size: self.config.full_scrollback_size,
        };

        // Hold registration through the synchronous PTY spawn. There is no
        // cancellation point between creating the child and recording its
        // attachment, including when connection teardown aborts the caller.
        let mut sessions = self.sessions.write().await;
        let alive_count = sessions.values().filter(|m| m.session.is_alive()).count();
        if alive_count >= self.config.max_sessions {
            return Err(G5Error::Internal {
                message: format!(
                    "Maximum concurrent terminal sessions reached ({}). \
                     Close existing sessions before creating new ones.",
                    self.config.max_sessions
                ),
            });
        }
        let session = TerminalSession::spawn(session_config)?;
        let session = Arc::new(session);
        let terminal_id = session.id().to_string();

        tracing::info!(
            terminal_id = %terminal_id,
            pid = session.pid(),
            is_background = is_background,
            "Created new terminal session"
        );

        sessions.insert(
            terminal_id.clone(),
            ManagedSession {
                session: Arc::clone(&session),
                is_background,
                interactive,
                attachment_generation: u64::from(interactive),
                attached: interactive,
                retiring: false,
                attachment_cancel,
            },
        );
        drop(sessions);

        if !interactive {
            let mut last = self.last_active_id.write().await;
            *last = Some(terminal_id);
        }

        Ok(session)
    }

    async fn reap_retiring_sessions(&self) {
        let ids = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter(|(_, managed)| managed.retiring)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };
        for id in ids {
            tracing::info!(terminal_id = %id, close_reason = TerminalCloseReason::RetiringRetry.as_str(), "retrying terminal close");
            if let Err(error) = self.close(&id).await {
                tracing::warn!(%id, %error, "retiring terminal cleanup will be retried");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Resolve default shell helper (used in tests)
// ---------------------------------------------------------------------------

/// Resolve the default shell path.
///
/// Checks `$SHELL`, then `/bin/bash`, then falls back to `/bin/sh`.
#[must_use]
pub fn resolve_default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if std::path::Path::new("/bin/bash").exists() {
            "/bin/bash".to_string()
        } else {
            "/bin/sh".to_string()
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_config_defaults() {
        let config = TerminalConfig::default();
        assert_eq!(config.max_sessions, 10);
        assert_eq!(config.default_timeout_secs, 300);
        assert_eq!(config.max_output_size, 61440);
        assert_eq!(config.scrollback_size, 4096);
        assert_eq!(config.full_scrollback_size, 65536);
        assert_eq!(config.default_cols, 80);
        assert_eq!(config.default_rows, 24);
        assert!(config.default_shell.is_none());
        assert!(config.env_vars.is_empty());
    }

    #[test]
    fn test_session_state_serde() {
        let state = SessionState::Running;
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SessionState::Running);
    }

    #[test]
    fn test_session_state_variants() {
        assert_ne!(SessionState::Running, SessionState::Idle);
        assert_ne!(SessionState::Idle, SessionState::Completed);
        assert_ne!(SessionState::Completed, SessionState::Failed);
    }

    #[test]
    fn test_run_in_terminal_args_deserialization() {
        let json = r#"{
            "command": "echo hello",
            "cwd": "/tmp",
            "timeout": 60,
            "terminal_id": "sess-1",
            "new_terminal": true,
            "is_background": false
        }"#;
        let args: RunInTerminalArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.command, "echo hello");
        assert_eq!(args.cwd, Some("/tmp".to_string()));
        assert_eq!(args.timeout, Some(60));
        assert_eq!(args.terminal_id, Some("sess-1".to_string()));
        assert_eq!(args.new_terminal, Some(true));
        assert_eq!(args.is_background, Some(false));
    }

    #[test]
    fn test_run_in_terminal_args_minimal() {
        let json = r#"{"command": "ls"}"#;
        let args: RunInTerminalArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.command, "ls");
        assert!(args.cwd.is_none());
        assert!(args.timeout.is_none());
        assert!(args.terminal_id.is_none());
        assert!(args.new_terminal.is_none());
        assert!(args.is_background.is_none());
    }

    #[test]
    fn test_terminal_result_serialization() {
        let result = TerminalResult {
            terminal_id: "sess-1".to_string(),
            exit_code: Some(0),
            stdout: "hello world\n".to_string(),
            stderr: String::new(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["terminal_id"], "sess-1");
        assert_eq!(json["exit_code"], 0);
        assert_eq!(json["stdout"], "hello world\n");
        assert_eq!(json["stderr"], "");
    }

    #[test]
    fn test_terminal_result_background() {
        let result = TerminalResult {
            terminal_id: "sess-2".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json["exit_code"].is_null());
    }

    #[test]
    fn test_session_info_serialization() {
        let info = SessionInfo {
            terminal_id: "sess-1".to_string(),
            state: SessionState::Idle,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_command_at: Some("2026-01-01T00:01:00Z".to_string()),
            pid: 12345,
            is_background: false,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["terminal_id"], "sess-1");
        assert_eq!(json["pid"], 12345);
        assert_eq!(json["is_background"], false);
    }

    #[test]
    fn test_resolve_default_shell() {
        let shell = resolve_default_shell();
        assert!(
            shell == "/bin/bash" || shell == "/bin/sh" || !shell.is_empty(),
            "Shell should be a valid path: {}",
            shell
        );
    }

    #[tokio::test]
    async fn interactive_session_streams_input_output_and_resizes() {
        let temp = tempfile::tempdir().unwrap();
        let manager = TerminalManager::new(TerminalConfig {
            max_sessions: 1,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        });
        let session = manager
            .open_interactive_with_env(
                80,
                24,
                HashMap::from([("G5D_TEST_TERMINAL_OWNER".to_string(), "current".to_string())]),
            )
            .await
            .unwrap();
        session.resize(100, 40).await.unwrap();
        session
            .write_raw(b"printf 'workspace-terminal-proof:%s\\n' \"$G5D_TEST_TERMINAL_OWNER\"\n")
            .await
            .unwrap();
        let output = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut output = Vec::new();
            while !String::from_utf8_lossy(&output).contains("workspace-terminal-proof:current") {
                output.extend(session.read_raw().await.unwrap().unwrap());
            }
            output
        })
        .await
        .unwrap();
        assert!(String::from_utf8_lossy(&output).contains("workspace-terminal-proof:current"));
        let id = session.id().to_string();
        manager.close(&id).await.unwrap();
        assert!(manager.list_sessions().is_empty());
    }

    #[tokio::test]
    async fn interactive_attachment_fences_stale_owner_and_allows_reattach() {
        let temp = tempfile::tempdir().unwrap();
        let manager = TerminalManager::new(TerminalConfig {
            max_sessions: 2,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        });
        let first = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let id = first.session.id().to_string();
        assert!(!first.reattached);
        assert!(manager.detach_interactive(&id, first.generation).await);

        let second = manager
            .attach_interactive(Some(&id), 100, 30, HashMap::new())
            .await
            .unwrap();
        assert!(second.reattached);
        assert!(first.cancel.is_cancelled());
        assert!(manager
            .close_interactive(&id, first.generation)
            .await
            .is_ok());
        assert!(manager.get_session_info(&id).is_some());

        manager
            .close_interactive(&id, second.generation)
            .await
            .unwrap();
        assert!(manager.list_sessions().is_empty());
    }

    #[tokio::test]
    async fn cancelled_reattach_during_resize_keeps_previous_owner() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(TerminalManager::new(TerminalConfig {
            max_sessions: 2,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        }));
        let first = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let id = first.session.id().to_string();
        let resize_lock = first.session.hold_resize_lock_for_test().await;
        let attempts = first.session.resize_attempts_for_test();
        let reattach_manager = Arc::clone(&manager);
        let reattach_id = id.clone();
        let reattach = tokio::spawn(async move {
            reattach_manager
                .attach_interactive(Some(&reattach_id), 100, 30, HashMap::new())
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while first.session.resize_attempts_for_test() == attempts {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("reattach never reached resize");
        reattach.abort();
        assert!(matches!(reattach.await, Err(error) if error.is_cancelled()));
        drop(resize_lock);

        let sessions = manager.sessions.read().await;
        let current = sessions.get(&id).unwrap();
        assert_eq!(current.attachment_generation, first.generation);
        assert!(current.attached);
        assert!(!first.cancel.is_cancelled());
        drop(sessions);

        let second = manager
            .attach_interactive(Some(&id), 100, 30, HashMap::new())
            .await
            .unwrap();
        assert_eq!(second.generation, first.generation + 1);
        manager
            .close_interactive(&id, second.generation)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fresh_terminal_retries_cleanup_of_a_retiring_session() {
        let temp = tempfile::tempdir().unwrap();
        let manager = TerminalManager::new(TerminalConfig {
            max_sessions: 1,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        });
        let old = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let old_id = old.session.id().to_string();
        {
            let mut sessions = manager.sessions.write().await;
            let old = sessions.get_mut(&old_id).unwrap();
            old.retiring = true;
            old.attached = false;
            old.attachment_cancel.take().unwrap().cancel();
        }
        let fresh = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .expect("retiring session must be reaped before capacity check");
        assert_ne!(fresh.session.id(), old_id);
        assert!(manager.get_session_info(&old_id).is_none());
        manager.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn failed_close_stays_fenced_until_cleanup_retry_succeeds() {
        let temp = tempfile::tempdir().unwrap();
        let manager = TerminalManager::new(TerminalConfig {
            max_sessions: 1,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        });
        let old = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let old_id = old.session.id().to_string();
        old.session.fail_next_shutdown_for_test();
        assert!(manager
            .close_interactive(&old_id, old.generation)
            .await
            .is_err());
        {
            let sessions = manager.sessions.read().await;
            let retained = sessions.get(&old_id).expect("failed cleanup is retained");
            assert!(retained.retiring);
            assert!(!retained.attached);
        }
        assert!(
            manager
                .attach_interactive(Some(&old_id), 80, 24, HashMap::new())
                .await
                .is_err(),
            "failed-close terminal must never be reattached"
        );
        assert!(manager.get_session_info(&old_id).is_none());
        manager.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn aborted_close_retains_session_for_retry() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(TerminalManager::new(TerminalConfig {
            max_sessions: 1,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        }));
        let attachment = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let id = attachment.session.id().to_string();
        let generation = attachment.generation;
        let shutdown_lock = attachment.session.hold_shutdown_lock_for_test().await;
        let closer_manager = Arc::clone(&manager);
        let closer_id = id.clone();
        let closer = tokio::spawn(async move {
            closer_manager
                .close_interactive(&closer_id, generation)
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if manager
                    .sessions
                    .read()
                    .await
                    .get(&id)
                    .is_some_and(|managed| managed.retiring)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("close did not retain a retiring session");
        closer.abort();
        assert!(matches!(closer.await, Err(error) if error.is_cancelled()));
        drop(shutdown_lock);

        assert!(manager.sessions.read().await.contains_key(&id));
        manager.close(&id).await.unwrap();
        assert!(!manager.sessions.read().await.contains_key(&id));
    }

    #[tokio::test]
    async fn aborted_manager_shutdown_retains_sessions_for_retry() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(TerminalManager::new(TerminalConfig {
            max_sessions: 1,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        }));
        let attachment = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let id = attachment.session.id().to_string();
        let shutdown_lock = attachment.session.hold_shutdown_lock_for_test().await;
        let shutdown_manager = Arc::clone(&manager);
        let shutdown = tokio::spawn(async move { shutdown_manager.shutdown().await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if manager
                    .sessions
                    .read()
                    .await
                    .get(&id)
                    .is_some_and(|managed| managed.retiring)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("manager shutdown did not retain a retiring session");
        shutdown.abort();
        assert!(matches!(shutdown.await, Err(error) if error.is_cancelled()));
        drop(shutdown_lock);

        assert!(manager.sessions.read().await.contains_key(&id));
        manager.shutdown().await.unwrap();
        assert!(!manager.sessions.read().await.contains_key(&id));
    }

    #[tokio::test]
    async fn failed_manager_shutdown_retains_only_a_retiring_identity() {
        let temp = tempfile::tempdir().unwrap();
        let manager = TerminalManager::new(TerminalConfig {
            max_sessions: 1,
            workspace_path: temp.path().to_path_buf(),
            ..TerminalConfig::default()
        });
        let old = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .unwrap();
        let old_id = old.session.id().to_string();
        old.session.fail_next_shutdown_for_test();
        assert!(manager.shutdown().await.is_err());
        {
            let sessions = manager.sessions.read().await;
            let retained = sessions.get(&old_id).expect("failed cleanup is retained");
            assert!(retained.retiring);
            assert!(!retained.attached);
        }
        assert!(
            manager
                .execute(RunInTerminalArgs {
                    command: "printf should-not-run".to_string(),
                    cwd: None,
                    timeout: None,
                    terminal_id: Some(old_id.clone()),
                    new_terminal: None,
                    is_background: None,
                })
                .await
                .is_err(),
            "failed-close identity must reject explicit command reuse"
        );
        let fresh = manager
            .attach_interactive(None, 80, 24, HashMap::new())
            .await
            .expect("fresh open retries failed lifecycle cleanup");
        assert_ne!(fresh.session.id(), old_id);
        manager.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_terminal_manager_creation() {
        let config = TerminalConfig::default();
        let manager = TerminalManager::new(config);
        let sessions = manager.list_sessions();
        assert!(sessions.is_empty());
    }

    // -- Hosted sessions (`g5.preview.output.live-tail`) --

    #[cfg(unix)]
    mod hosted_sessions {
        use super::*;
        use std::io::Write as _;
        use std::time::Duration;

        /// The manager and the workspace directory its shells start in; the
        /// directory must outlive any shell the test opens.
        fn manager(max_sessions: usize) -> (TerminalManager, tempfile::TempDir) {
            let temp = tempfile::tempdir().unwrap();
            let manager = TerminalManager::new(TerminalConfig {
                max_sessions,
                workspace_path: temp.path().to_path_buf(),
                ..TerminalConfig::default()
            });
            (manager, temp)
        }

        /// Write to the slave as the module child would. The PTY's line
        /// discipline is left at its defaults, so tests write no newlines
        /// and compare exact bytes.
        fn writer(pty: &HostedPty) -> std::fs::File {
            std::fs::File::from(pty.slave.try_clone().unwrap())
        }

        async fn drain(cursor: &mut HostedCursor, cancel: &CancellationToken) -> Vec<u8> {
            let mut bytes = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_secs(5), cursor.next(cancel)).await {
                    Ok(Ok(chunk)) => bytes.extend(chunk),
                    Ok(Err(HostedReadEnd::Ended)) => return bytes,
                    Ok(Err(end)) => panic!("unexpected end {end:?} after {bytes:?}"),
                    Err(_) => panic!("hosted cursor stalled after {bytes:?}"),
                }
            }
        }

        /// Read from the cursor until `bytes` (which may already hold the
        /// replay) contains `expected`.
        async fn read_until(
            cursor: &mut HostedCursor,
            cancel: &CancellationToken,
            mut bytes: Vec<u8>,
            expected: &[u8],
        ) -> Vec<u8> {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !bytes.windows(expected.len()).any(|w| w == expected) {
                    bytes.extend(cursor.next(cancel).await.expect("more output"));
                }
                bytes
            })
            .await
            .expect("hosted output timeout")
        }

        /// A hosted session replays from an offset, resets to the retained
        /// tail when the offset is unavailable, and delivers bytes written
        /// after the viewer attached, like a shell's `replay_from`.
        #[tokio::test]
        async fn hosted_session_replays_and_resets_offsets_like_a_shell() {
            let (manager, _workspace) = manager(2);
            let pty = manager
                .host_session("preview:task-1", 16, 80, 24)
                .await
                .unwrap();
            let mut slave = writer(&pty);
            slave.write_all(b"0123456789").unwrap();
            let first = manager
                .attach_hosted("preview:task-1", 80, 24, None)
                .await
                .unwrap();
            assert!(!first.reattached);
            assert_eq!(first.generation, 1);
            let mut first_cursor = first.cursor;
            assert_eq!(first.replay.offset, 0);
            // A first attach without an offset is a reset to the retained
            // tail, exactly as a shell reports it.
            assert!(first.replay.reset);
            let seen = read_until(
                &mut first_cursor,
                &first.cancel,
                first.replay.data.clone(),
                b"9",
            )
            .await;
            assert_eq!(seen, b"0123456789");

            slave.write_all(b"abcdef").unwrap();
            let live = read_until(&mut first_cursor, &first.cancel, Vec::new(), b"f").await;
            assert_eq!(
                live, b"abcdef",
                "bytes written after the attach arrive live"
            );

            // Resume exactly at the viewer's offset: nothing missed or doubled.
            let resumed = manager
                .attach_hosted("preview:task-1", 80, 24, Some(first_cursor.offset()))
                .await
                .unwrap();
            assert!(resumed.reattached);
            assert_eq!(resumed.generation, 2);
            assert!(
                first.cancel.is_cancelled(),
                "the second viewer supersedes the first"
            );
            assert_eq!(resumed.replay.offset, 16);
            assert!(!resumed.replay.reset);
            assert!(resumed.replay.data.is_empty());
            assert_eq!(
                first_cursor.next(&first.cancel).await,
                Err(HostedReadEnd::Superseded)
            );

            // A stale offset older than the 16-byte ring gets the tail and a reset.
            slave.write_all(b"GHIJ").unwrap();
            let mut resumed_cursor = resumed.cursor;
            read_until(&mut resumed_cursor, &resumed.cancel, Vec::new(), b"J").await;
            let stale = manager
                .attach_hosted("preview:task-1", 80, 24, Some(2))
                .await
                .unwrap();
            assert!(stale.replay.reset);
            assert_eq!(stale.replay.offset, 20 - 16);
            assert_eq!(stale.replay.data, b"456789abcdefGHIJ");
            manager.shutdown().await.unwrap();
        }

        /// The manager offers no input path to a hosted session: the
        /// interactive write and resize-by-generation calls do not know it,
        /// and detaching a viewer leaves the session alive and attachable.
        #[tokio::test]
        async fn hosted_session_drops_input_and_detach_does_not_end_it() {
            let (manager, _workspace) = manager(2);
            let pty = manager
                .host_session("preview:task-2", 1024, 80, 24)
                .await
                .unwrap();
            let attachment = manager
                .attach_hosted("preview:task-2", 100, 30, None)
                .await
                .unwrap();
            assert!(manager
                .write_interactive("preview:task-2", attachment.generation, b"rm -rf /\n")
                .await
                .is_err());
            assert!(manager
                .resize_interactive("preview:task-2", attachment.generation, 120, 40)
                .await
                .is_err());
            manager
                .resize_hosted("preview:task-2", attachment.generation, 120, 40)
                .await
                .expect("the current viewer may resize");
            assert!(
                manager
                    .detach_hosted("preview:task-2", attachment.generation)
                    .await
            );
            assert!(!manager.hosted_attached("preview:task-2").await);
            assert!(manager
                .resize_hosted("preview:task-2", attachment.generation, 120, 40)
                .await
                .is_err());
            assert!(
                pty.session.is_alive(),
                "detaching never ends a hosted session"
            );
            let mut slave = writer(&pty);
            slave.write_all(b"after-detach").unwrap();
            let again = manager
                .attach_hosted("preview:task-2", 80, 24, None)
                .await
                .unwrap();
            let mut cursor = again.cursor;
            let seen = read_until(
                &mut cursor,
                &again.cancel,
                again.replay.data.clone(),
                b"detach",
            )
            .await;
            assert_eq!(seen, b"after-detach");
            manager.shutdown().await.unwrap();
        }

        /// Once the last slave holder closes, the session is ended but
        /// retained: a late attach gets the whole tail and the cursor ends.
        #[tokio::test]
        async fn hosted_session_is_attachable_after_the_slave_closes_and_then_ends() {
            let (manager, _workspace) = manager(2);
            let pty = manager
                .host_session("preview:task-3", 1024, 80, 24)
                .await
                .unwrap();
            let session = Arc::clone(&pty.session);
            {
                let mut slave = writer(&pty);
                slave.write_all(b"build done").unwrap();
            }
            drop(pty);
            tokio::time::timeout(Duration::from_secs(5), async {
                while session.is_alive() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the master must read EOF once every slave holder closed");
            let late = manager
                .attach_hosted("preview:task-3", 80, 24, None)
                .await
                .expect("an ended session stays attachable for replay");
            assert_eq!(late.replay.data, b"build done");
            let mut cursor = late.cursor;
            assert_eq!(drain(&mut cursor, &late.cancel).await, b"");
            // A viewer that resumes mid-tail drains the rest, then ends.
            let partial = manager
                .attach_hosted("preview:task-3", 80, 24, Some(6))
                .await
                .unwrap();
            assert_eq!(partial.replay.data, b"done");
            assert!(!partial.replay.reset);
            manager.shutdown().await.unwrap();
            assert!(manager.hosted_session("preview:task-3").await.is_none());
        }

        /// `end` ends the session even for bytes the reader had read but not
        /// retained yet: checked under the ring lock, nothing read after
        /// `end` is retained or re-publishes a live state, and the master is
        /// released although the writer never paused. The read is confirmed
        /// before `end`, so the race under test is the one exercised.
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn hosted_session_end_drops_bytes_read_after_it_and_releases_the_master() {
            let (manager, _workspace) = manager(2);
            let pty = manager
                .host_session("preview:task-6", 1024, 80, 24)
                .await
                .unwrap();
            let session = Arc::clone(&pty.session);
            let mut slave = writer(&pty);
            slave.write_all(b"before").unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while session.next_offset() < 6 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("the reader retains the first write");
            let reads_before = session.reads();
            // A holder pins the ring: the reader reads the next write but
            // cannot retain it until `end` has run.
            let (locked_tx, locked_rx) = std::sync::mpsc::channel::<()>();
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            let holder = {
                let session = Arc::clone(&session);
                let mut slave = writer(&pty);
                std::thread::spawn(move || {
                    let held = session.hold_ring();
                    slave.write_all(b"after").unwrap();
                    locked_tx.send(()).unwrap();
                    let _ = release_rx.recv();
                    drop(held);
                })
            };
            locked_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("the holder locked the ring");
            // `end` must come after the read, or the reader would leave
            // through the wake path without ever holding the bytes.
            tokio::time::timeout(Duration::from_secs(5), async {
                while session.reads() == reads_before {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("the reader read the second write");
            let ending = tokio::spawn({
                let session = Arc::clone(&session);
                async move { session.end().await }
            });
            tokio::time::timeout(Duration::from_secs(5), async {
                while session.is_alive() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("end marks the session ended before it needs the ring");
            release_tx.send(()).unwrap();
            holder.join().unwrap();
            ending.await.unwrap();
            // The reader finishes and releases the master: the child's writes
            // fail with EIO even though it never paused.
            let error = tokio::task::spawn_blocking(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                loop {
                    match slave.write_all(b"x") {
                        Err(error) => return Some(error),
                        Ok(()) if std::time::Instant::now() < deadline => continue,
                        Ok(()) => return None,
                    }
                }
            })
            .await
            .unwrap()
            .expect("writes must fail once the master is released");
            assert_eq!(error.raw_os_error(), Some(libc::EIO), "{error}");
            // Only what was retained before `end` replays; the ended state
            // was never overwritten by the late read.
            let late = manager
                .attach_hosted("preview:task-6", 80, 24, None)
                .await
                .unwrap();
            assert_eq!(late.replay.data, b"before");
            let mut cursor = late.cursor;
            assert_eq!(drain(&mut cursor, &late.cancel).await, b"");
            manager.shutdown().await.unwrap();
        }

        /// Hosting the same name again discards the previous session and
        /// supersedes its viewer; the new session starts empty.
        #[tokio::test]
        async fn hosted_session_is_replaced_by_the_next_host_of_the_same_name() {
            let (manager, _workspace) = manager(2);
            let old = manager
                .host_session("preview:task-4", 1024, 80, 24)
                .await
                .unwrap();
            let mut old_slave = writer(&old);
            old_slave.write_all(b"old run").unwrap();
            let viewer = manager
                .attach_hosted("preview:task-4", 80, 24, None)
                .await
                .unwrap();
            let fresh = manager
                .host_session("preview:task-4", 1024, 80, 24)
                .await
                .unwrap();
            assert!(viewer.cancel.is_cancelled());
            assert!(!old.session.is_alive(), "the replaced session is ended");
            assert!(fresh.session.is_alive());
            let attached = manager
                .attach_hosted("preview:task-4", 80, 24, None)
                .await
                .unwrap();
            assert!(Arc::ptr_eq(&attached.session, &fresh.session));
            assert!(attached.replay.data.is_empty());
            assert!(!attached.reattached, "the new session's first viewer");
            manager.shutdown().await.unwrap();
        }

        /// Hosted sessions do not count toward `max_sessions`, never become
        /// `last_active_id`, and are absent from the enumeration the agent's
        /// terminal tool reads.
        #[tokio::test]
        async fn hosted_sessions_are_outside_the_shell_limits_and_listing() {
            let (manager, _workspace) = manager(1);
            let _pty = manager
                .host_session("preview:task-5", 1024, 80, 24)
                .await
                .unwrap();
            assert!(manager.list_sessions().is_empty());
            assert!(manager.get_session_info("preview:task-5").is_none());
            assert!(manager.last_active_id.read().await.is_none());
            let shell = manager
                .attach_interactive(None, 80, 24, HashMap::new())
                .await
                .expect("the hosted session leaves the single shell slot free");
            assert!(manager
                .attach_interactive(Some("preview:task-5"), 80, 24, HashMap::new())
                .await
                .is_err());
            assert!(manager
                .execute(RunInTerminalArgs {
                    command: "true".to_string(),
                    cwd: None,
                    timeout: Some(5),
                    terminal_id: Some("preview:task-5".to_string()),
                    new_terminal: None,
                    is_background: None,
                })
                .await
                .is_err());
            manager
                .close_interactive(shell.session.id(), shell.generation)
                .await
                .unwrap();
            manager.shutdown().await.unwrap();
        }

        /// Each hosted session keeps its own capacity; a viewer that falls
        /// further behind than the ring retains is told so.
        #[tokio::test]
        async fn hosted_session_capacity_is_per_session() {
            let (manager, _workspace) = manager(2);
            let small = manager
                .host_session("preview:small", 8, 80, 24)
                .await
                .unwrap();
            assert_eq!(small.session.capacity(), 8);
            let viewer = manager
                .attach_hosted("preview:small", 80, 24, None)
                .await
                .unwrap();
            let mut slave = writer(&small);
            slave.write_all(b"0123456789abcdef").unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                while small.session.next_offset() < 16 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let mut cursor = viewer.cursor;
            assert_eq!(
                cursor.next(&viewer.cancel).await,
                Err(HostedReadEnd::Lagged)
            );
            let reset = manager
                .attach_hosted("preview:small", 80, 24, Some(0))
                .await
                .unwrap();
            assert!(reset.replay.reset);
            assert_eq!(reset.replay.data, b"89abcdef");
            assert!(manager
                .host_session("preview:none", 0, 80, 24)
                .await
                .is_err());
            manager.shutdown().await.unwrap();
        }
    }
}
