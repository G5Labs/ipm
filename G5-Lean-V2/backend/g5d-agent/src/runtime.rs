use base64::Engine as _;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::runner::claude::ClaudeFrameMapper;
use crate::runner::codex_app_server::{self, AppServer};
use crate::runner::events::{usage_f64, usage_u64, BackgroundTask, TaskPhase};
use crate::runner::session_files::{
    harvest_complete, restore_verified, RestoreOutcome, SessionFileLayout, SkipReason,
};
use crate::runner::{Attachment, TurnEvent, TurnUsage};
use g5_contracts::error::G5Error;
use g5_contracts::utils::replace_nul_in_json;
use g5_internal_api::InternalApiClient;
use g5d_foundation::process_termination::{EXECUTION_RELEASE_ENV, EXECUTION_RELEASE_EXIT_STATUS};
use g5d_foundation::runtime_environment::prepend_path;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

mod steering;

use crate::policy::{resolve_policy, ResolvedPolicy};
use crate::protocol::{
    control_error_response, display_projection, interrupt_request, native_response_envelope,
    parse_claude_control, question_hook_response, question_initialize, ClaudeControl,
    NativeRequest, NativeRequestKind, QUESTION_INIT_ID,
};
use crate::types::{
    AgentTurnInputV1, AgentTurnOutcomeV1, AgentTurnStatus, ProviderId, SessionFilesState,
    OUTCOME_SCHEMA,
};

const PROVIDER_EXIT_GRACE: Duration = Duration::from_secs(15);
const ATTEMPT_SETTLEMENT_GRACE: Duration = Duration::from_secs(8);
/// After Claude's background set empties between turns, input stays open
/// this long for the report turn Claude starts next (probed: about 100 ms).
const BACKGROUND_SETTLE: Duration = Duration::from_secs(3);
/// How long the owner's End lets Claude's current turn finish before
/// interrupting it, so End takes effect within a bounded time.
const END_TURN_GRACE: Duration = Duration::from_secs(10);
/// How long the owner's End waits for Claude to answer its interrupt before
/// closing input anyway.
const END_INTERRUPT_REPLY: Duration = Duration::from_secs(10);
/// Once End is due, how long its interrupt may take to start being written
/// before End drops Claude's input instead.
const END_INTERRUPT_WRITE: Duration = Duration::from_secs(1);
/// Claude 2.1.285 exits with this code at input EOF after an in-band
/// interrupt aborted a turn (probed on Linux, 2026-09-30).
const CLAUDE_INTERRUPTED_TURN_EXIT_CODE: i32 = 1;
/// How long writing to Claude's input may make no progress before the input
/// counts as broken. Claude reads its input continuously.
const PROVIDER_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Harvest after a normal exit. It no longer shares an attempt deadline, so
/// it carries its own bound.
const NORMAL_HARVEST_GRACE: Duration = Duration::from_secs(300);
/// A resumed session that still awaits a deferred question reissues it right
/// after acknowledging initialization (about 20 ms on the pinned CLI, Linux,
/// 2026-10-30). One that has advanced past the question never does, so the
/// handshake is bounded separately from the provider setup budget
/// (`g5.chat.native-question-recovery`).
const NATIVE_QUESTION_RESTORE_GRACE: Duration = Duration::from_secs(30);
const FAKE_DEFER_STOP_SAVE: &str = "G5_TOOL_ONLY_FACT_E2E:defer-stop-save";
/// After the provider exits, how long its output pipes may stay open. A
/// descendant that inherited them must not hold the attempt open.
const PROVIDER_DRAIN_GRACE: Duration = Duration::from_secs(5);
/// Bounds on the background set published to the conversation.
const MAX_BACKGROUND_TASKS_LISTED: usize = 20;
const MAX_BACKGROUND_TEXT_BYTES: usize = 300;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Longest provider output line assembled in memory. The pinned providers
/// cap one tool result near 1 MiB before JSON escaping, so real lines reach
/// about 1.5 MB (QA 2026-10-12: 1.04 to 1.44 MB Codex `item.completed`
/// lines, a 1.07 MB Claude image read). A line past this ceiling is
/// discarded to its newline and noted, not a failed turn; memory stays a
/// few lines per turn (see the output channel in `run`).
const MAX_PROVIDER_LINE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROVIDER_STDERR_BYTES: u64 = 8 * 1024 * 1024;
/// Most bytes of native control frames (the handshake, hook answers and
/// control-request rejections) that may wait for the provider's input at
/// once. Each answers one provider frame, and a rejection echoes the
/// provider's own request id, which can be as long as a provider line, so
/// only a provider that sends control requests faster than it reads the
/// replies reaches this. The reply that does not fit is dropped and the
/// attempt ends as a provider error (`g5.agents.output-bounds`); the queue
/// never holds more. The owner's input (steering, native replies, End) is
/// not counted: the dispatcher's control channel paces it.
const MAX_QUEUED_CONTROL_BYTES: usize = 1024 * 1024;
/// Upper bound on the one-shot summary-title call that follows a successful
/// first turn. It is best effort: on timeout or failure the session keeps the
/// provisional first-message title.
const SESSION_TITLE_TIMEOUT: Duration = Duration::from_secs(60);
/// Below this much remaining turn budget the summary call is skipped: the
/// title is optional and has its own bounded request timeout.
const SESSION_TITLE_MIN_BUDGET: Duration = Duration::from_secs(5);
/// Longest slice of the request and of the reply the title prompt quotes.
const SESSION_TITLE_CONTEXT_BYTES: usize = 2000;
/// The API caps stored titles at 80 bytes; anything longer is trimmed there.
const SESSION_TITLE_MAX_BYTES: usize = 120;
/// Most stdout the summary call may print before it is stopped. Its one
/// result object (the title, usage and cost) is a few kilobytes.
const SESSION_TITLE_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
struct InvocationRecord {
    input: AgentTurnInputV1,
}

#[derive(Debug, Deserialize)]
struct DeliveredResponse {
    kind: String,
    request_id: String,
    provider_request_id: String,
    response_digest: String,
    validated_response: Value,
}

struct PendingRequest {
    request: NativeRequest,
    deadline: Option<Instant>,
    /// The owner's reply is queued for Claude's input. The request stays
    /// pending, with its deadline, until the reply is completely written.
    reply_queued: bool,
}

/// Claude's input after the prompt. Frames are queued in order and written by
/// the main loop without blocking it, so provider output, control lines and
/// every clock keep running while Claude is slow to read.
#[derive(Default)]
struct ProviderInput {
    stdin: Option<ChildStdin>,
    queue: VecDeque<QueuedFrame>,
    /// When writing last made progress, or the first queued frame arrived.
    progress_at: Option<Instant>,
    /// Bytes of queued `NativeControl` frames, written or not.
    control_bytes: usize,
    /// A control reply did not fit under `MAX_QUEUED_CONTROL_BYTES` and was
    /// dropped; the main loop ends the attempt as a provider error.
    control_overflow: bool,
}

struct QueuedFrame {
    bytes: Vec<u8>,
    written: usize,
    purpose: FramePurpose,
}

enum FramePurpose {
    /// An immutable user message; acknowledge only after Claude's flush.
    Steer { request_id: String },
    /// The owner's native reply; delivered once completely written.
    Reply {
        provider_request_id: String,
        request_id: String,
        response_digest: String,
        disposition: &'static str,
        source_attempt: Option<u32>,
    },
    /// Native SDK handshake or hook response.
    NativeControl,
    /// Initial App Server turn request; bytes may reach the native provider.
    InitialTurn,
    /// The owner's End.
    Interrupt,
}

impl FramePurpose {
    fn starts_initial_input(&self) -> bool {
        matches!(
            self,
            Self::InitialTurn
                | Self::Reply {
                    source_attempt: Some(_),
                    ..
                }
        )
    }
}

impl ProviderInput {
    fn open(&self) -> bool {
        self.stdin.is_some()
    }

    fn writing(&self) -> bool {
        self.stdin.is_some() && !self.queue.is_empty()
    }

    fn enqueue(
        &mut self,
        frame: &Value,
        purpose: FramePurpose,
        now: Instant,
    ) -> Result<(), G5Error> {
        let mut bytes = serde_json::to_vec(frame).map_err(|error| internal(error.to_string()))?;
        bytes.push(b'\n');
        if matches!(purpose, FramePurpose::NativeControl) {
            // Once a reply is dropped the provider's control stream is
            // broken, so nothing later is queued behind the gap.
            let queued = self.control_bytes.saturating_add(bytes.len());
            if self.control_overflow || queued > MAX_QUEUED_CONTROL_BYTES {
                self.control_overflow = true;
                return Ok(());
            }
            self.control_bytes = queued;
        }
        if self.queue.is_empty() {
            self.progress_at = Some(now);
        }
        self.queue.push_back(QueuedFrame {
            bytes,
            written: 0,
            purpose,
        });
        Ok(())
    }

    /// Drop replies not yet started whose request ended meanwhile, for
    /// example because Claude cancelled it.
    fn drop_stale_replies(&mut self, pending: &HashMap<String, PendingRequest>) {
        self.queue.retain(|frame| {
            frame.written > 0
                || match &frame.purpose {
                    FramePurpose::Reply {
                        provider_request_id,
                        ..
                    } => pending.contains_key(provider_request_id),
                    FramePurpose::Interrupt
                    | FramePurpose::NativeControl
                    | FramePurpose::InitialTurn
                    | FramePurpose::Steer { .. } => true,
                }
        });
    }

    /// Write part of the front frame. Cancel safe: when another branch of
    /// the loop wins, nothing was written.
    async fn write_some(&mut self) -> std::io::Result<usize> {
        let (Some(stdin), Some(frame)) = (self.stdin.as_mut(), self.queue.front()) else {
            return std::future::pending().await;
        };
        stdin.write(&frame.bytes[frame.written..]).await
    }

    /// Record `written` bytes; return the front frame once it is complete.
    fn advance(&mut self, written: usize, now: Instant) -> Option<QueuedFrame> {
        self.progress_at = Some(now);
        let frame = self.queue.front_mut()?;
        frame.written += written;
        if frame.written < frame.bytes.len() {
            return None;
        }
        let frame = self.queue.pop_front()?;
        if matches!(frame.purpose, FramePurpose::NativeControl) {
            self.control_bytes = self.control_bytes.saturating_sub(frame.bytes.len());
        }
        Some(frame)
    }

    /// The front frame is the End interrupt and nothing of it is written.
    fn interrupt_starts_next(&self) -> bool {
        self.queue.front().is_some_and(|frame| {
            frame.written == 0 && matches!(frame.purpose, FramePurpose::Interrupt)
        })
    }

    /// Withdraw the End interrupt if none of it is written yet.
    fn withdraw_unstarted_interrupt(&mut self) -> bool {
        let before = self.queue.len();
        self.queue
            .retain(|frame| frame.written > 0 || !matches!(frame.purpose, FramePurpose::Interrupt));
        self.queue.len() != before
    }

    fn stalled(&self, now: Instant) -> bool {
        self.writing()
            && self
                .progress_at
                .is_some_and(|at| now >= at + PROVIDER_WRITE_TIMEOUT)
    }

    /// Close the input, returning the frames that were not completely
    /// written. A partial line may have reached Claude, so nothing more is.
    fn close(&mut self) -> Vec<QueuedFrame> {
        self.stdin = None;
        self.progress_at = None;
        self.control_bytes = 0;
        self.queue.drain(..).collect()
    }
}

enum ProviderOutput {
    Stdout(String),
    Stderr(String),
    /// A line crossed `MAX_PROVIDER_LINE_BYTES`: it is dropped through its
    /// newline, and this is sent once, as the line crosses the ceiling.
    LineSkipped {
        stderr: bool,
    },
    /// Stderr passed its log cap (`MAX_PROVIDER_STDERR_BYTES`).
    LogLimitExceeded,
    ReadFailed,
    Closed,
}

/// The frozen wire transport owns its adapter and lifecycle differences. The
/// supervisor, process custody and settlement remain shared by all transports.
enum Transport {
    ClaudeStream(ClaudeFrameMapper),
    CodexAppServer(AppServer),
}

impl Transport {
    fn is_claude(&self) -> bool {
        matches!(self, Self::ClaudeStream(_))
    }

    fn accepts_initial_write(&self, input: &AgentTurnInputV1) -> bool {
        !matches!(self, Self::CodexAppServer(_)) && input.native_question_reply.is_none()
    }

    async fn advance_input(
        &self,
        state: &mut TurnState,
        pending: &mut HashMap<String, PendingRequest>,
        provider_input: &mut ProviderInput,
        idle_since: &mut Option<Instant>,
        now: Instant,
    ) -> Result<(), G5Error> {
        match self {
            Self::ClaudeStream(_) => {
                advance_claude_input(state, pending, provider_input, idle_since, now).await?;
            }
            Self::CodexAppServer(adapter) if adapter.ended() && !provider_input.writing() => {
                provider_input.close();
                state.input_closed = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn exit_grace_running(&self, state: &TurnState) -> bool {
        match self {
            Self::ClaudeStream(_) => {
                state.input_closed
                    && (state.request_after_close
                        || state.end == EndWait::Closed
                        || state.reply_undeliverable
                        || (!state.in_turn && state.background.is_empty()))
            }
            Self::CodexAppServer(_) => {
                state.result_seen || (state.input_closed && state.provider_error.is_some())
            }
        }
    }

    fn was_interrupted(&self) -> bool {
        matches!(self, Self::CodexAppServer(adapter) if adapter.was_interrupted())
    }
}

#[derive(Default)]
struct TurnState {
    subscription_usage: Option<crate::subscription_usage::Reporter>,
    steering: steering::Steering,
    provider_session_id: Option<String>,
    usage: Option<TurnUsage>,
    final_text: Option<String>,
    completed_units: Vec<(String, String)>,
    result_seen: bool,
    result_error: bool,
    max_turns_reached: bool,
    provider_error: Option<String>,
    config_error: bool,
    events: Arc<AtomicU64>,
    /// Inside a provider turn: from the input write, or a later Claude
    /// `system/init`, to that turn's `result`.
    in_turn: bool,
    /// Provider turns started in this attempt.
    turn: u32,
    /// Where the current turn's units start in `completed_units`.
    turn_units_from: usize,
    /// Claude's live background set as last reported, each with the time
    /// (unix ms) it was first reported.
    background: Vec<(BackgroundTask, u64)>,
    /// Claude reported background work at some point in this attempt.
    background_seen: bool,
    /// When the current background wait began (unix ms): the set last went
    /// from empty to non-empty.
    wait_started: Option<u64>,
    /// The owner's request to end Claude's background wait.
    end: EndWait,
    /// The interrupt control request issued for the End; Claude's reply to
    /// it is accepted even after input closed without waiting for it.
    issued_interrupt: Option<String>,
    /// The End's interrupt aborted a Claude turn.
    end_aborted_turn: bool,
    /// Claude refused the End's interrupt, so no turn is its effect.
    interrupt_refused: bool,
    /// A native reply could not be written and Claude's input was dropped.
    /// The attempt settles from what Claude does next; a provider still
    /// running when the exit grace ends is stopped as suspended.
    reply_undeliverable: bool,
    /// Claude's input is closed; no later request can be answered.
    input_closed: bool,
    /// Claude opened a native request after its input closed.
    request_after_close: bool,
    /// Every provider request identity this attempt has used — live, settled,
    /// cancelled or restored. The persistence contract is one use per attempt
    /// (`g5.chat.native-request-boundary`), so a settled id must not reopen.
    /// One id of at most 512 bytes per recorded open; nothing else bounds it.
    used_request_ids: HashSet<String>,
    /// The question Claude deferred (`tool_deferred`), a candidate for
    /// resumption from the saved session until exit and capture confirm that
    /// the session still awaits it (`g5.chat.native-question-recovery`).
    deferred_question: Option<String>,
    /// A deferring hook reply was queued: no further steering may be admitted
    /// to this process, whatever Claude does next.
    steering_closed: bool,
    /// Claude ran another native turn after deferring the question, so the
    /// saved session no longer offers it.
    advanced_after_deferral: bool,
    restoring_question: bool,
    /// While restoring a frozen answer: the resumed session must reissue the
    /// question by this instant (armed by Claude's initialize reply).
    restore_deadline: Option<Instant>,
}

/// Progress of the owner's End action on a Claude background wait.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
enum EndWait {
    #[default]
    None,
    /// Requested at `since`; applied once Claude is between turns, or during
    /// a turn after `END_TURN_GRACE`.
    Requested { since: Instant },
    /// The interrupt is queued behind input still being written. It is
    /// withdrawn if the background work ends before its first byte is
    /// written; only then does End take effect. End is `due` at its receipt
    /// plus `END_TURN_GRACE`; if the interrupt has not started
    /// `END_INTERRUPT_WRITE` after that, End drops the input and the bounded
    /// stop follows.
    InterruptQueued { request_id: String, due: Instant },
    /// The interrupt control was written; input closes on Claude's reply or
    /// after `END_INTERRUPT_REPLY`.
    Interrupting {
        request_id: String,
        sent_at: Instant,
        replied: bool,
    },
    /// Input closed to end the wait. A provider that does not then exit
    /// within the exit grace is stopped and the attempt is interrupted.
    Closed,
}

/// Provider setup/input delivery is bounded separately from device login's
/// owner-response wait. Running provider work has no lifetime deadline.
#[derive(Debug, Clone, Copy)]
struct Deadlines {
    setup: Instant,
    review_execution: bool,
}

impl Deadlines {
    fn setup_at(&self, progress: &AttemptProgress) -> Instant {
        self.setup
            + Duration::from_millis(progress.authorization_elapsed_ms.load(Ordering::Acquire))
    }

    async fn reached(self, progress: &AttemptProgress) {
        loop {
            // Device authorization has its own bounded owner-response clock.
            // It must not consume the provider setup/input-delivery budget.
            let resumed = progress.setup_resumed.notified();
            if progress.authorizing.load(Ordering::Acquire) {
                resumed.await;
                continue;
            }
            if progress.input_handed.load(Ordering::Relaxed) {
                if self.review_execution {
                    let accepted = *progress
                        .review_input_at
                        .lock()
                        .expect("review acceptance clock");
                    if let Some(accepted) = accepted {
                        tokio::time::sleep_until((accepted + Duration::from_secs(45 * 60)).into())
                            .await;
                        return;
                    }
                }
                std::future::pending::<()>().await;
            }
            let at = self.setup_at(progress);
            if Instant::now() >= at {
                return;
            }
            tokio::select! { _ = tokio::time::sleep_until(at.into()) => {}, _ = progress.setup_resumed.notified() => {} }
        }
    }
}

#[derive(Default)]
struct AttemptProgress {
    authorizing: AtomicBool,
    authorization_elapsed_ms: AtomicU64,
    setup_resumed: tokio::sync::Notify,
    provider_started: AtomicBool,
    input_write_started: AtomicBool,
    /// The provider took the input (native App Server acceptance, or legacy
    /// write and flush with exec EOF). The
    /// default deadline lifts here, before the acceptance is journaled.
    input_handed: AtomicBool,
    review_input_at: std::sync::Mutex<Option<Instant>>,
    input_accepted: AtomicBool,
    input_rejected: AtomicBool,
    event_seq: Arc<AtomicU64>,
    /// The provider itself ended the turn (a successful result, or a deferred
    /// native question with a clean exit): no outcome was forced. What it left
    /// running then belongs to the agent, not to execution cleanup.
    provider_ended_turn: AtomicBool,
}

fn interruption() -> std::io::Result<impl std::future::Future<Output = ()>> {
    #[cfg(unix)]
    let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    Ok(async move {
        #[cfg(unix)]
        {
            signal.recv().await;
        }
        #[cfg(not(unix))]
        std::future::pending::<()>().await;
    })
}

pub async fn execute() -> Result<(), G5Error> {
    execute_with_status().await.map(|_| ())
}

/// Run one turn and return the process exit status. Under a Linux execution
/// owner a turn the provider ended itself asks the owner to release, not reap,
/// the processes the agent left running.
pub async fn execute_with_status() -> Result<i32, G5Error> {
    // Install before setup/launch so external interruption is queued even
    // while restore or authorization is pending. The execution owner enforces
    // the absolute cleanup deadline if any of that work stalls.
    let attempt_started = Instant::now();
    let interrupt = interruption().map_err(io_error)?;
    let setup_interrupt = interruption().map_err(io_error)?;
    let mut control = BufReader::new(tokio::io::stdin()).lines();
    let Some(first) = control.next_line().await.map_err(io_error)? else {
        return Err(internal(
            "agent.turn invocation channel closed before input",
        ));
    };
    let record: InvocationRecord = match serde_json::from_str(&first) {
        Ok(record) => record,
        Err(error) => {
            emit_outcome(failed_outcome(
                "invalid_invocation",
                false,
                SessionFilesState::Absent,
            ))
            .await?;
            return Err(internal(format!("invalid agent.turn invocation: {error}")));
        }
    };
    let input = record.input;
    if let Err(code) = input.validate() {
        emit_outcome(failed_outcome(
            &code.to_ascii_lowercase(),
            false,
            input.session_binding.session_files_state,
        ))
        .await?;
        return Ok(0);
    }
    let policy = match resolve_policy(&input.provider.policy_snapshot) {
        Ok(policy) => policy,
        Err(code) => {
            emit_outcome(failed_outcome(
                &code.to_ascii_lowercase(),
                false,
                input.session_binding.session_files_state,
            ))
            .await?;
            return Ok(0);
        }
    };
    let own_deadline = attempt_started + Duration::from_millis(input.limits.setup_timeout_ms);
    let unix_deadline = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|millis| UNIX_EPOCH.checked_add(Duration::from_millis(millis)))
            .and_then(|deadline| {
                let remaining = deadline
                    .duration_since(SystemTime::now())
                    .unwrap_or_default();
                Instant::now().checked_add(remaining)
            })
    };
    let deadlines = Deadlines {
        review_execution: input.provider.options.contains_key("formal_review"),
        setup: unix_deadline("G5D_SETUP_DEADLINE_UNIX_MS")
            .map_or(own_deadline, |deadline| deadline.min(own_deadline)),
    };
    let session_files_state = input.session_binding.session_files_state;
    let progress = AttemptProgress::default();
    let run_future = run(input, policy, &mut control, interrupt, deadlines, &progress);
    tokio::pin!(run_future);
    // An interruption or deadline seen here never releases what the turn left
    // running, even when the turn then completes inside the settlement grace.
    let (result, stopped_outside_turn) = tokio::select! {
        result = &mut run_future => (result, false),
        _ = setup_interrupt => (
            if progress.provider_started.load(Ordering::Relaxed) {
                tokio::time::timeout(ATTEMPT_SETTLEMENT_GRACE, &mut run_future)
                    .await
                    .unwrap_or_else(|_| Ok(failed_outcome_with_status(
                        AgentTurnStatus::Suspended, "interrupted", false,
                        SessionFilesState::PersistFailed,
                    )))
            } else {
                Ok(failed_outcome_with_status(
                    AgentTurnStatus::Suspended, "interrupted", false, session_files_state,
                ))
            },
            true,
        ),
        _ = deadlines.reached(&progress) => (
            if progress.provider_started.load(Ordering::Relaxed) {
                tokio::time::timeout(ATTEMPT_SETTLEMENT_GRACE, &mut run_future)
                    .await
                    .unwrap_or_else(|_| Ok(failed_outcome_with_status(
                        AgentTurnStatus::TimedOut, "wall_clock_exceeded", true,
                        SessionFilesState::PersistFailed,
                    )))
            } else {
                Ok(failed_outcome_with_status(
                    AgentTurnStatus::TimedOut, "wall_clock_exceeded", true, session_files_state,
                ))
            },
            true,
        ),
    };
    // Vetoing the release leaves what remains to the Linux execution owner,
    // which reaps only this turn's descendants; this late in the turn the
    // provider's group id is no proof of ownership any more.
    let release = !stopped_outside_turn
        && result.is_ok()
        && progress.provider_ended_turn.load(Ordering::Relaxed)
        && std::env::var_os(EXECUTION_RELEASE_ENV).is_some();
    let mut outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            // Spawn, harness installation and broken-pipe failures can precede
            // the provider event loop. Preserve a durable error in those paths.
            eprintln!("agent turn failed: {error}");
            let restore_unavailable = matches!(error, G5Error::SessionFileStoreUnavailable { .. });
            emit("notice", json!({
                "code": if restore_unavailable { "session_files_restore_unavailable" } else { "agent_turn_failed" },
                "level":"error",
                "message": if restore_unavailable {
                    "The saved provider session could not be checked. Retry this turn; its saved state has been preserved."
                } else {
                    "The agent could not start or finish its turn. Please retry; if the problem persists, check the worker logs."
                },
                "retryable":true
            })).await?;
            failed_outcome(
                if restore_unavailable {
                    "session_files_restore_unavailable"
                } else {
                    "agent_turn_failed"
                },
                true,
                interrupted_harvest_state(session_files_state, &progress.provider_started),
            )
        }
    };
    outcome.input_delivery_unknown = progress.input_write_started.load(Ordering::Relaxed)
        && !progress.input_accepted.load(Ordering::Relaxed)
        && !progress.input_rejected.load(Ordering::Relaxed);
    outcome.provider_started = progress.provider_started.load(Ordering::Relaxed);
    outcome.last_event_seq = progress.event_seq.load(Ordering::Relaxed);
    emit_outcome(outcome).await?;
    Ok(if release {
        EXECUTION_RELEASE_EXIT_STATUS
    } else {
        0
    })
}

fn interrupted_harvest_state(
    initial: SessionFilesState,
    provider_started: &AtomicBool,
) -> SessionFilesState {
    if provider_started.load(Ordering::Relaxed) {
        SessionFilesState::PersistFailed
    } else {
        initial
    }
}

async fn run(
    input: AgentTurnInputV1,
    policy: ResolvedPolicy,
    control: &mut tokio::io::Lines<BufReader<tokio::io::Stdin>>,
    interrupt: impl std::future::Future<Output = ()>,
    deadlines: Deadlines,
    progress: &AttemptProgress,
) -> Result<AgentTurnOutcomeV1, G5Error> {
    tokio::pin!(interrupt);
    let mut workspace_root = env_path("G5D_WORKSPACE_ROOT", "/work");
    let assignment = crate::formal_review::invocation(&input)?;
    if assignment.is_some() {
        workspace_root = workspace_root
            .canonicalize()
            .map_err(|e| internal(e.to_string()))?;
    }
    let original_home = env_path("HOME", "/home/agent");
    let review_environment = assignment
        .as_ref()
        .map(|assignment| {
            crate::formal_review::Environment::prepare(
                &input,
                assignment,
                &workspace_root,
                &original_home,
            )
        })
        .transpose()?;
    let cwd = match &review_environment {
        Some(review) => review.cwd.clone(),
        None => target_cwd(&workspace_root, &input)?,
    };
    let home = review_environment
        .as_ref()
        .map_or(original_home, |review| review.home.clone());
    let memory = if assignment.is_none() {
        let personal_root = std::env::var("G5D_AGENT_PERSONAL_ROOT").ok();
        let writable = match input.provider.provider_id {
            ProviderId::ClaudeCode => {
                policy.allowed_tools.is_empty()
                    || policy.allowed_tools.iter().any(|tool| {
                        matches!(tool.as_str(), "Write" | "Edit" | "Bash" | "NotebookEdit")
                    })
            }
            ProviderId::Codex => policy.sandbox != Some("read-only"),
        };
        match crate::memory::prepare(
            &workspace_root,
            personal_root.as_deref(),
            input.provider.provider_id,
            writable,
        )
        .and_then(|binding| {
            if binding.is_bound() && input.provider.provider_id == ProviderId::ClaudeCode {
                crate::memory::validate_claude_settings(&cwd, &home)?;
            } else if input.provider.provider_id == ProviderId::Codex {
                crate::memory::bind_codex(&home, &binding)?;
            }
            Ok(binding)
        }) {
            Ok(binding) => binding,
            Err(error) => {
                emit("notice", json!({"code":"shared_memory_setup_failed","level":"error","message":error.to_string(),"retryable":false})).await?;
                return Ok(failed_outcome(
                    "shared_memory_setup_failed",
                    false,
                    input.session_binding.session_files_state,
                ));
            }
        }
    } else {
        crate::memory::Binding::Disabled
    };
    // Only a failed binding is news; disabled-by-design memory stays silent.
    if let crate::memory::Binding::Skipped(reason) = &memory {
        emit("notice", json!({"code":"shared_memory_skipped","level":"info","message":reason,"retryable":false})).await?;
    }
    let memory_settings = if input.provider.provider_id == ProviderId::ClaudeCode {
        Some(crate::memory::claude_settings(&home, &memory))
    } else {
        None
    };
    let api_client = incarnation_client()?;
    let (attachment_dir, attachments) = match materialize_attachments(
        &input,
        api_client.as_ref(),
        &home,
    )
    .await
    {
        Ok(value) => value,
        Err(code) => {
            emit("notice", json!({"code":code,"level":"error","message":"A reserved chat attachment could not be delivered. Check that it still exists, belongs to this input and matches its recorded hash and type.","retryable":false})).await?;
            return Ok(failed_outcome(
                code,
                false,
                input.session_binding.session_files_state,
            ));
        }
    };
    // Formal native state is private and never restored/published. Its launch
    // root is outside the reviewed tree; ordinary sessions keep containment.
    let layout_root = review_environment
        .as_ref()
        .map_or(workspace_root.as_path(), |review| review.cwd.as_path());
    let layout = SessionFileLayout::for_provider(
        match input.provider.provider_id {
            ProviderId::ClaudeCode => {
                g5_internal_api::session_files::SessionFileProvider::ClaudeCode
            }
            ProviderId::Codex => g5_internal_api::session_files::SessionFileProvider::Codex,
        },
        &home,
        layout_root,
        cwd.strip_prefix(layout_root)
            .map_err(|_| internal("working directory escaped workspace root"))?,
    );
    let mut resume_id = None;
    let mut unusable_reason = None;
    if let (Some(client), Some(id), Some(_)) = (
        api_client.as_ref(),
        input.session_binding.provider_session_id.as_deref(),
        input
            .resume_prompt
            .as_ref()
            .map(|_| ())
            .or_else(|| input.native_question_reply.as_ref().map(|_| ())),
    ) {
        if let Some(workspace_id) = std::env::var("G5D_WORKSPACE_ID")
            .ok()
            .and_then(|value| uuid::Uuid::parse_str(&value).ok())
        {
            let session_id = uuid::Uuid::parse_str(&input.task.session_id)
                .map_err(|_| internal("invalid chat session id for bundle recovery"))?;
            let pending_bundles = client
                .pending_session_file_bundles(workspace_id, Some(session_id))
                .await
                .map_err(|error| G5Error::SessionFileStoreUnavailable {
                    message: error.to_string(),
                })?;
            for pending in pending_bundles {
                if pending.provider_session_id != id || pending.provider != layout.provider() {
                    continue;
                }
                let pending_layout = SessionFileLayout::for_provider(
                    pending.provider,
                    &home,
                    &workspace_root,
                    Path::new(&pending.cwd_relative_path),
                );
                match harvest_complete(
                    client,
                    &input.task.session_id,
                    &pending_layout,
                    id,
                    pending.generation,
                )
                .await
                {
                    Ok(report) if report.found > 0 && report.skipped.is_empty() => {}
                    Ok(report) => {
                        eprintln!("pending native session cannot be completed: {report:?}")
                    }
                    Err(error) if pending_completion_is_definitive(&error) => {
                        eprintln!("pending native session cannot be completed: {error}");
                    }
                    Err(error) => {
                        return Err(G5Error::SessionFileStoreUnavailable {
                            message: error.to_string(),
                        })
                    }
                }
            }
        }
    }
    if let (Some(client), Some(id), Some(_)) = (
        api_client.as_ref(),
        input.session_binding.provider_session_id.as_deref(),
        input
            .resume_prompt
            .as_ref()
            .map(|_| ())
            .or_else(|| input.native_question_reply.as_ref().map(|_| ())),
    ) {
        match restore_verified(client, &input.task.session_id, &layout, id).await {
            Ok(RestoreOutcome::Present | RestoreOutcome::Restored { .. }) => {
                resume_id = Some(id.to_string())
            }
            Ok(RestoreOutcome::NothingStored) => {}
            Ok(RestoreOutcome::Unusable { reason }) => {
                eprintln!("native session {id} not resumable: {reason}");
                unusable_reason = Some(reason);
            }
            // Lookup/download failure is indeterminate. Keep the committed
            // bundle available for this reservation's next attempt instead
            // of fencing it with a fresh provider session.
            Err(error) => return Err(error),
        }
    }
    if input.native_question_reply.is_some() && resume_id.is_none() {
        notice_question_restore_failed().await?;
        return Ok(failed_outcome(
            NATIVE_QUESTION_RESTORE_FAILED,
            false,
            input.session_binding.session_files_state,
        ));
    }
    if resume_id.is_none() {
        let unavailable = input
            .recovery
            .as_ref()
            .and_then(|recovery| recovery.unavailable_reason.as_deref())
            .or_else(|| {
                (input.session_binding.provider_session_id.is_some() && input.recovery.is_none())
                    .then_some(
                    "The earlier native session has no frozen delivered-history reconstruction.",
                )
            });
        if let Some(reason) = unavailable {
            emit("notice", json!({"code":"context_unavailable","level":"error","message":reason,"retryable":false})).await?;
            return Ok(failed_outcome(
                "context_unavailable",
                false,
                input.session_binding.session_files_state,
            ));
        }
    }

    let exec_id = std::env::var("G5D_EXEC_ID").unwrap_or_else(|_| input.task.task_id.clone());
    let mut provider_env: BTreeMap<String, String> = std::env::vars().collect();
    // Loader paths are for the platform g5d binary, not provider CLIs.
    let loader = g5d_foundation::runtime_environment::take_native_loader_paths(&mut provider_env);
    // The agent runs `g5d` commands (issues, platform reports) from its shell:
    // a shim on its PATH applies the loader path to g5d alone.
    match std::env::current_exe().and_then(|g5d| install_g5d_shim(&home, &g5d, &loader)) {
        Ok(bin) => {
            let path = provider_env.get("PATH").cloned().unwrap_or_default();
            provider_env.insert("PATH".into(), prepend_path(&bin, &path));
        }
        Err(error) => eprintln!("g5d shim unavailable to the agent: {error}"),
    }
    provider_env.insert("HOME".into(), home.to_string_lossy().into_owned());
    provider_env.insert("G5D_TASK_ID".into(), input.task.task_id.clone());
    provider_env.insert("G5D_SESSION_ID".into(), input.task.session_id.clone());
    provider_env.insert("G5D_EXEC_ID".into(), exec_id.clone());
    provider_env.remove("G5D_AGENT_REPOSITORIES");
    provider_env.remove("G5D_AGENT_PERSONAL_ROOT");
    provider_env.remove(EXECUTION_RELEASE_ENV);
    if input.provider.provider_id == ProviderId::ClaudeCode {
        provider_env.insert(
            "CLAUDE_CONFIG_DIR".into(),
            home.join(".claude").to_string_lossy().into_owned(),
        );
        // These native overrides precede --settings and could redirect the
        // collection away from the repository selected by the incarnation.
        for key in [
            "CLAUDE_CODE_REMOTE_MEMORY_DIR",
            "CLAUDE_COWORK_MEMORY_PATH_OVERRIDE",
            "CLAUDE_MEMORY_STORES",
            "CLAUDE_COWORK_MEMORY_INDEX_CONTENT",
        ] {
            provider_env.remove(key);
        }
        if !memory.is_bound() {
            provider_env.insert("CLAUDE_CODE_DISABLE_AUTO_MEMORY".into(), "1".into());
        }
    }
    let fake_provider = fake_provider_enabled(&input);
    // Native subscription login owns this VM-local home. Retired credential
    // injection is never honored, including on routes that bypass login.
    provider_env.remove("CODEX_AUTH_JSON");
    if input.provider.provider_id == ProviderId::Codex {
        // The CLI requires an explicitly configured CODEX_HOME to exist even
        // for API-key exec, which never passes through the login gate.
        let codex_home = home.join(".codex");
        std::fs::create_dir_all(&codex_home).map_err(io_error)?;
        provider_env.insert(
            "CODEX_HOME".into(),
            codex_home.to_string_lossy().into_owned(),
        );
    }
    // Supply the harness before native login or execution reads the provider
    // home. Fake fixtures remain independent of a mounted production bundle.
    let harness = if fake_provider || assignment.is_some() {
        Ok(())
    } else {
        match input.provider.provider_id {
            ProviderId::ClaudeCode => {
                crate::harness::prepare_claude(&home, input.workspace_target.personal)
            }
            ProviderId::Codex => {
                crate::harness::prepare_codex(&home, input.workspace_target.personal)
            }
        }
    };
    match harness {
        Ok(()) => {}
        // A failed write in the home is an ordinary, retryable turn failure.
        Err(crate::harness::HarnessError::Install(error)) => return Err(io_error(error)),
        Err(crate::harness::HarnessError::Unavailable(error)) => {
            eprintln!("Harness preparation failed: {error}");
            emit("notice", json!({"code":"harness_unavailable","level":"error","message":"The agent could not load the G5 harness. The worker's runtime bundle needs repair.","retryable":false})).await?;
            return Ok(failed_outcome(
                "harness_unavailable",
                false,
                input.session_binding.session_files_state,
            ));
        }
    };
    if assignment.is_some() {
        crate::formal_review::sanitize_environment(&mut provider_env);
    }
    if assignment.is_none()
        && input.provider.auth_mode
            == Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription)
    {
        for key in [
            "OPENAI_API_KEY",
            "CODEX_API_KEY",
            "OPENROUTER_API_KEY",
            "CODEX_ACCESS_TOKEN",
            "OPENAI_BASE_URL",
        ] {
            provider_env.remove(key);
        }
        // Do not let entering login rescue setup that already expired.
        if Instant::now() >= deadlines.setup_at(progress) {
            return Ok(failed_outcome_with_status(
                AgentTurnStatus::TimedOut,
                "wall_clock_exceeded",
                false,
                input.session_binding.session_files_state,
            ));
        }
        let authorization_started = Instant::now();
        progress.authorizing.store(true, Ordering::Release);
        let result = if let Some(client) = &api_client {
            let program = provider_binary("G5_CODEX_PATH", "codex", fake_provider);
            crate::codex_auth::authorize(&input, client, &program, &cwd, &provider_env).await
        } else {
            Err("codex_auth_failed")
        };
        progress.authorization_elapsed_ms.store(
            authorization_started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            Ordering::Release,
        );
        progress.authorizing.store(false, Ordering::Release);
        progress.setup_resumed.notify_one();
        if let Err(code) = result {
            emit("notice", json!({"code":code,"level":"error","message":g5_internal_api::codex_auth::auth_notice(code),"retryable":false})).await?;
            return Ok(failed_outcome(
                code,
                false,
                input.session_binding.session_files_state,
            ));
        }
    }
    let transport_args = if fake_provider {
        Vec::new()
    } else {
        crate::provider_transport::configure(
            input.provider.provider_id,
            input.provider.auth_mode,
            input.provider.base_url.as_deref(),
            &mut provider_env,
        )?
    };

    let provider_session_seed = uuid::Uuid::new_v4().to_string();
    let (program, mut args, initial_bytes, mut transport) = launch_plan(
        &input,
        &policy,
        LaunchFiles {
            cwd: &cwd,
            home: &home,
            attachments: &attachments,
            memory: &memory,
        },
        resume_id.as_deref(),
        &provider_session_seed,
        fake_provider,
        memory_settings.as_deref(),
    )?;
    args.extend(transport_args);
    let _attachment_dir = attachment_dir;
    if Instant::now() >= deadlines.setup_at(progress) {
        return Ok(failed_outcome_with_status(
            AgentTurnStatus::TimedOut,
            "wall_clock_exceeded",
            true,
            input.session_binding.session_files_state,
        ));
    }
    // The turn's ontology start (`g5.chat.ontology-card`), recorded before the
    // provider can change a file. Workspace chats only: the personal chat and
    // formal reviews have no cards.
    let ontology = match (api_client.as_ref(), std::env::var("G5D_WORKSPACE_ID")) {
        (Some(client), Ok(workspace_id))
            if assignment.is_none() && !input.workspace_target.personal =>
        {
            let members = std::env::var("G5D_AGENT_REPOSITORIES")
                .map(|json| crate::ontology_turn::code_members(&workspace_root, &json))
                .unwrap_or_default();
            crate::ontology_turn::TurnOntology::begin(
                client,
                &workspace_id,
                &input.task.task_id,
                input.task.attempt,
                members,
            )
            .await
        }
        _ => None,
    };
    // Fence the old generation only after all fallible launch preparation,
    // and before the process can change the provider's native state.
    let mut bundle_begin_error = None;
    let bundle_generation = if let Some(client) =
        api_client.as_ref().filter(|_| assignment.is_none())
    {
        let generation = uuid::Uuid::new_v4();
        let begun = client
            .begin_session_file_bundle(
                &input.task.session_id,
                &g5_internal_api::dtos::SessionFileBundleBegin {
                    generation,
                    task_id: uuid::Uuid::parse_str(&input.task.task_id)
                        .map_err(|_| internal("invalid task id for bundle publication"))?,
                    attempt: i32::try_from(input.task.attempt)
                        .map_err(|_| internal("invalid attempt for bundle publication"))?,
                    reservation_id: uuid::Uuid::parse_str(&input.task.initial_input.reservation_id)
                        .map_err(|_| internal("invalid reservation id for bundle publication"))?,
                    cwd_relative_path: cwd
                        .strip_prefix(&workspace_root)
                        .map_err(|_| internal("working directory escaped workspace root"))?
                        .to_string_lossy()
                        .into_owned(),
                },
            )
            .await;
        match begun {
            Ok(()) => Some(generation),
            Err(G5Error::Conflict { .. }) => {
                return Ok(failed_outcome(
                    "turn_superseded",
                    false,
                    input.session_binding.session_files_state,
                ));
            }
            Err(
                error @ (G5Error::NotFound { .. }
                | G5Error::Forbidden { .. }
                | G5Error::Unauthorized),
            ) => {
                return Err(error);
            }
            Err(error) if resume_id.is_none() => {
                // A fresh provider has no prior native state to advance. Run
                // the turn, but leave its files unpublished and report that
                // later turns will need reconstruction.
                bundle_begin_error = Some(error.to_string());
                None
            }
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let mut initial_notice = None;
    if resume_id.is_none() {
        if let Some(recovery) = &input.recovery {
            let incomplete = recovery.omitted_messages > 0
                || recovery.truncated_messages > 0
                || recovery.attachment_references > 0
                || recovery.omitted_attachment_references > 0;
            let prior_native_session = input.session_binding.provider_session_id.is_some();
            initial_notice = Some(json!({
                "code":"conversation_reconstructed",
                "level": if incomplete || prior_native_session { "warning" } else { "info" },
                "message":format!(
                    "Started a fresh provider session from {} delivered history messages ({} messages omitted; {} message bodies truncated). {} attachment references are listed ({} omitted); their bytes were not replayed.{}",
                    recovery.included_messages, recovery.omitted_messages, recovery.truncated_messages,
                    recovery.attachment_references, recovery.omitted_attachment_references,
                    if prior_native_session { " The prior native session could not resume, so earlier tool-only context may be missing." } else { "" }
                ),
                "reason": unusable_reason,
                "retryable":false
            }));
        }
    }

    let mut command = Command::new(&program);
    command
        .args(args)
        .current_dir(&cwd)
        .env_clear()
        .envs(&provider_env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|error| {
        internal(format!(
            "failed to start {} provider: {error}",
            input.provider.provider_id.as_str()
        ))
    })?;
    progress.provider_started.store(true, Ordering::Relaxed);
    let pid = child.id();
    let mut provider_stdin = child
        .stdin
        .take()
        .ok_or_else(|| internal("provider stdin was not piped"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("provider stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| internal("provider stderr was not piped"))?;
    progress
        .input_write_started
        .store(transport.accepts_initial_write(&input), Ordering::Relaxed);
    let initial_write = tokio::select! {
        result = async {
            provider_stdin.write_all(&initial_bytes).await?;
            provider_stdin.flush().await
        } => result.map_err(io_error),
        _ = &mut interrupt => {
            stop_provider_group(pid, &mut child).await;
            return Ok(failed_outcome_with_status(
                AgentTurnStatus::Suspended, "interrupted", false,
                SessionFilesState::PersistFailed,
            ));
        },
        _ = tokio::time::sleep_until(deadlines.setup_at(progress).into()) => {
            stop_provider_group(pid, &mut child).await;
            return Ok(failed_outcome_with_status(
                AgentTurnStatus::TimedOut, "wall_clock_exceeded", true,
                SessionFilesState::PersistFailed,
            ));
        },
    };
    initial_write?;
    let mut provider_input = ProviderInput {
        stdin: Some(provider_stdin),
        ..ProviderInput::default()
    };
    if transport.accepts_initial_write(&input) {
        confirm_initial_input(&input, progress, &mut initial_notice).await?;
    }

    // One queued message: with lines up to `MAX_PROVIDER_LINE_BYTES`, a deeper
    // queue would hold many of them before the loop projects and bounds them.
    // A turn holds at most the line each reader assembles, the queued one
    // and the one being projected.
    let (output_tx, mut output_rx) = mpsc::channel(1);
    // Provider output has no cumulative limit (`g5.agents.output-bounds`):
    // each line and each recorded event is bounded, and only stderr keeps a
    // total, its log cap.
    spawn_output_reader(stdout, None, output_tx.clone());
    spawn_output_reader(stderr, Some(MAX_PROVIDER_STDERR_BYTES), output_tx);
    let mut closed_streams = 0u8;
    let mut state = TurnState {
        subscription_usage: crate::subscription_usage::Reporter::start(&input, api_client.clone()),
        steering: steering::Steering::new(&input),
        events: progress.event_seq.clone(),
        in_turn: true,
        turn: 1,
        input_closed: !provider_input.open(),
        restoring_question: input.native_question_reply.is_some(),
        ..TurnState::default()
    };
    let mut pending: HashMap<String, PendingRequest> = HashMap::new();
    let mut exit_status = None;
    let mut control_open = true;
    let mut idle_since: Option<Instant> = None;
    let mut exit_grace: Option<Instant> = None;
    let mut exit_grace_spent = false;
    let mut drain_deadline: Option<Instant> = None;
    let mut forced_outcome = None;
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        // Checked before the exit test so a provider that overran the reply
        // queue with its last lines and exited is still a provider error.
        if forced_outcome.is_none() && provider_input.control_overflow {
            record(
                &mut state,
                "notice",
                json!({"code":"provider_control_backlog","level":"error","message":"The provider sent control requests faster than it read the replies, so the turn was stopped.","retryable":false}),
            )
            .await?;
            // The notice above is the turn's explanation; without a provider
            // error on record, settlement would add its start-failure notice.
            state
                .provider_error
                .get_or_insert_with(|| "provider control replies overran their bound".into());
            forced_outcome = Some((AgentTurnStatus::Failed, "provider_error", true));
            invalidate_all(&mut pending, "execution_ended").await?;
            stop_provider_group(pid, &mut child).await;
            exit_status = child.try_wait().ok().flatten();
            break;
        }
        if exit_status.is_some() {
            if closed_streams >= 2 {
                break;
            }
            let now = Instant::now();
            if now >= *drain_deadline.get_or_insert(now + PROVIDER_DRAIN_GRACE) {
                // The provider exited but something it started still holds
                // its output open. Stop waiting for it; whether it keeps
                // running is decided with the outcome below.
                eprintln!("provider output still open after exit; settling");
                break;
            }
        }
        provider_input.drop_stale_replies(&pending);
        // The earliest clock is armed ahead of provider output, control lines
        // and writes, so after a delayed wakeup or slow event delivery a late
        // reply or a buffered cancellation cannot erase an expired request.
        // Restoring a frozen answer is setup until its native callback accepts
        // it. After acceptance only permission requests have a deadline.
        let setup_clock = (!transport.accepts_initial_write(&input)
            && !progress.input_accepted.load(Ordering::Relaxed))
        .then(|| deadlines.setup_at(progress));
        let restore_clock = (state.restoring_question, state.restore_deadline);
        let next_clock = next_clock(setup_clock, restore_clock.1, &pending);
        tokio::select! {
            biased;
            _ = &mut interrupt => {
                forced_outcome = Some((AgentTurnStatus::Suspended, "interrupted", false));
                stop_provider_group(pid, &mut child).await;
                exit_status = child.try_wait().ok().flatten();
                // Descendant pipe EOF is not needed for interruption. The
                // external owner handles all remaining processes before settle.
                break;
            }
            _ = sleep_until_clock(next_clock), if next_clock.is_some() && forced_outcome.is_none() => {
                if let Some(expired) = expired_clock(Instant::now(), setup_clock, restore_clock, &pending) {
                    forced_outcome = Some(expired);
                    invalidate_all(&mut pending, "execution_ended").await?;
                    stop_provider_group(pid, &mut child).await;
                    exit_status = child.try_wait().ok().flatten();
                    break;
                }
            }
            output = output_rx.recv(), if closed_streams < 2 => {
                match output {
                    Some(ProviderOutput::Stdout(line)) => {
                        handle_provider_line(
                            &input,
                            &policy,
                            &mut transport,
                            &mut state,
                            &mut pending,
                            &mut provider_input,
                            &line,
                            progress,
                            &mut initial_notice,
                        ).await?;
                    }
                    Some(ProviderOutput::Stderr(line)) => {
                        if !line.trim().is_empty() {
                            state.config_error |= is_provider_config_error(&line);
                            eprintln!("{}", line);
                        }
                    }
                    Some(ProviderOutput::LineSkipped { stderr }) => {
                        record(
                            &mut state,
                            "notice",
                            json!({"code":"provider_line_skipped","level":"warning","message":provider_line_skipped_message(stderr),"retryable":false}),
                        )
                        .await?;
                    }
                    Some(ProviderOutput::LogLimitExceeded) => {
                        forced_outcome = Some((AgentTurnStatus::LimitExceeded, "max_log_bytes", false));
                        invalidate_all(&mut pending, "execution_ended").await?;
                        stop_provider_group(pid, &mut child).await;
                        exit_status = child.try_wait().ok().flatten();
                        break;
                    }
                    Some(ProviderOutput::ReadFailed) => {
                        forced_outcome = Some((AgentTurnStatus::Failed, "provider_output_read_failed", true));
                        invalidate_all(&mut pending, "execution_ended").await?;
                        stop_provider_group(pid, &mut child).await;
                        exit_status = child.try_wait().ok().flatten();
                        break;
                    }
                    Some(ProviderOutput::Closed) => {
                        closed_streams = closed_streams.saturating_add(1);
                    }
                    None => closed_streams = closed_streams.saturating_add(1),
                }
            }
            incoming = control.next_line(), if control_open && forced_outcome.is_none() => {
                match incoming.map_err(io_error)? {
                    Some(line) => {
                        handle_control_line(
                            transport.is_claude(),
                            &mut state,
                            &mut provider_input,
                            &mut pending,
                            &line,
                        ).await?;
                    }
                    // The dispatcher closed the control channel: no reply or
                    // End can arrive any more. Claude's input stays under the
                    // background window; an unanswered request still expires.
                    None => control_open = false,
                }
            }
            written = provider_input.write_some(), if provider_input.writing() => {
                match written {
                    Ok(written) if written > 0 => {
                        let now = Instant::now();
                        if provider_input.interrupt_starts_next() {
                            interrupt_started(&mut state, now).await?;
                        }
                        if provider_input.queue.front().is_some_and(|frame| frame.purpose.starts_initial_input()) {
                            progress.input_write_started.store(true, Ordering::Relaxed);
                        }
                        if let Some(frame) = provider_input.advance(written, now) {
                            // ChildStdin is unbuffered, but the shipped contract
                            // explicitly includes a completed flush.
                            if let Err(error) = provider_input.stdin.as_mut().unwrap().flush().await {
                                eprintln!("provider input flush failed: {error}");
                                provider_input_failed(&transport, &mut state, &mut provider_input, &mut pending).await?;
                                continue;
                            }
                            if let FramePurpose::Steer { request_id } = &frame.purpose {
                                if let Some(payload) = state.steering.shipped(request_id) {
                                    record(&mut state, "input.shipped", payload).await?;
                                }
                            }
                            let native_answer = matches!(frame.purpose, FramePurpose::Reply { source_attempt: Some(_), .. });
                            let delivered = frame_written(frame, &mut pending, &input).await?;
                            if native_answer && delivered {
                                progress.input_handed.store(true, Ordering::Relaxed);
                                progress.input_accepted.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    result => {
                        eprintln!("provider input write failed: {result:?}");
                        provider_input_failed(&transport, &mut state, &mut provider_input, &mut pending).await?;
                    }
                }
            }
            _ = tick.tick() => {
                if exit_status.is_none() {
                    exit_status = child.try_wait().map_err(io_error)?;
                }
                let now = Instant::now();
                if provider_input.stalled(now) {
                    eprintln!("provider input made no progress for {PROVIDER_WRITE_TIMEOUT:?}");
                    provider_input_failed(&transport, &mut state, &mut provider_input, &mut pending).await?;
                }
            }
        }
        // A handler may have waited long, for example on slow event
        // delivery. Stop and the clocks decide first, before End or the
        // input window act on what changed meanwhile.
        if forced_outcome.is_none() {
            let stopped = tokio::select! {
                biased;
                _ = &mut interrupt => true,
                _ = std::future::ready(()) => false,
            };
            if stopped {
                forced_outcome = Some((AgentTurnStatus::Suspended, "interrupted", false));
                stop_provider_group(pid, &mut child).await;
                exit_status = child.try_wait().ok().flatten();
                break;
            }
            if let Some(expired) = expired_clock(
                Instant::now(),
                setup_clock,
                (state.restoring_question, state.restore_deadline),
                &pending,
            ) {
                forced_outcome = Some(expired);
                invalidate_all(&mut pending, "execution_ended").await?;
                stop_provider_group(pid, &mut child).await;
                exit_status = child.try_wait().ok().flatten();
                break;
            }
        }
        let now = Instant::now();
        transport
            .advance_input(
                &mut state,
                &mut pending,
                &mut provider_input,
                &mut idle_since,
                now,
            )
            .await?;
        let grace_running = transport.exit_grace_running(&state);
        if !grace_running || exit_grace_spent {
            exit_grace = None;
        } else if exit_status.is_none()
            && now >= *exit_grace.get_or_insert(now + PROVIDER_EXIT_GRACE)
        {
            exit_grace_spent = true;
            if state.end == EndWait::Closed {
                forced_outcome =
                    Some((AgentTurnStatus::Interrupted, "background_wait_ended", false));
                stop_provider_group(pid, &mut child).await;
                exit_status = child.try_wait().ok().flatten();
                break;
            }
            if state.reply_undeliverable {
                // Like an unanswered native request, the interaction could
                // not go on: the task is held for Continue.
                forced_outcome = Some((
                    AgentTurnStatus::Suspended,
                    "native_response_undeliverable",
                    false,
                ));
                stop_provider_group(pid, &mut child).await;
                exit_status = child.try_wait().ok().flatten();
                break;
            }
            state.provider_error = Some("provider did not exit after its input closed".into());
            stop_provider_group(pid, &mut child).await;
            exit_status = child.try_wait().ok().flatten();
        }
    }
    if exit_status.is_none() {
        exit_status = child.wait().await.ok();
    }
    if !pending.is_empty() {
        invalidate_all(&mut pending, "process_exited").await?;
    }
    // The provider has exited: classify its turn now, so what it left running
    // is released or stopped before the bounded harvest and finalization.
    // A follow-up turn started but the provider exited before its result.
    let turn_incomplete = forced_outcome.is_none() && state.in_turn && state.result_seen;
    if turn_incomplete {
        state
            .provider_error
            .get_or_insert_with(|| "provider exited during a turn".into());
    }
    let forced = forced_outcome.is_some();
    let (mut status, mut code, mut retryable) = if let Some(forced) = forced_outcome {
        forced
    } else if transport.was_interrupted()
        && state.provider_error.is_none()
        && exit_status.as_ref().is_some_and(|status| status.success())
    {
        (AgentTurnStatus::Interrupted, "provider_interrupted", false)
    } else if state.deferred_question.is_some()
        && state.provider_error.is_none()
        && exit_status.as_ref().is_some_and(|status| status.success())
    {
        // The deferral is confirmed only if the session stopped at the
        // question. A process that went on to another turn saved past it:
        // the question is lost to the saved session and the turn holds for
        // the owner instead (`g5.chat.native-question-recovery`).
        if state.advanced_after_deferral {
            (AgentTurnStatus::Suspended, NATIVE_QUESTION_LOST, false)
        } else {
            (
                AgentTurnStatus::Suspended,
                "native_question_deferred",
                false,
            )
        }
    } else if state.restoring_question
        && state.provider_error.is_none()
        && exit_status.as_ref().is_some_and(|status| status.success())
    {
        // The resumed session ended on its own without ever reissuing the
        // question: a definite restoration failure, whatever it printed
        // (`g5.chat.native-question-recovery`).
        (
            AgentTurnStatus::Failed,
            NATIVE_QUESTION_RESTORE_FAILED,
            false,
        )
    } else if state.result_seen
        && !state.result_error
        && state.provider_error.is_none()
        // A refusal that arrives after an aborted result still means the
        // abort was not the End's effect.
        && !(state.end_aborted_turn && state.interrupt_refused)
        && exit_status.as_ref().is_some_and(|status| {
            // Claude's exit status after a turn the End aborted is the End's
            // effect too, like that turn's result.
            status.success()
                || (state.end_aborted_turn
                    && status.code() == Some(CLAUDE_INTERRUPTED_TURN_EXIT_CODE))
        })
    {
        (
            AgentTurnStatus::Succeeded,
            if state.max_turns_reached {
                "max_turns_reached"
            } else {
                "exit_code"
            },
            false,
        )
    } else {
        (AgentTurnStatus::Failed, "provider_error", true)
    };
    // A provider that ended its own turn keeps what it left running; a turn
    // budget it reached is a limit like any other, and an owner End that took
    // effect stops the background work rather than finishing the turn.
    let provider_ended_turn = assignment.is_none()
        && !forced
        && !state.max_turns_reached
        && state.issued_interrupt.is_none()
        && state.end != EndWait::Closed
        && matches!(
            status,
            AgentTurnStatus::Succeeded | AgentTurnStatus::Suspended
        );
    progress
        .provider_ended_turn
        .store(provider_ended_turn, Ordering::Relaxed);
    if !provider_ended_turn {
        // Otherwise stop what remains of the provider's process group before
        // the bounded harvest; the execution owner reaps anything that
        // escaped it.
        stop_provider_group(pid, &mut child).await;
    }

    let provider_session_id = state.provider_session_id.clone();
    let mut session_files_state = initial_session_files_state(
        provider_session_id.as_deref(),
        bundle_generation,
        resume_id.as_deref(),
    );
    emit("terminal.progress", json!({"phase":"harvesting"})).await?;
    if let Some(error) = bundle_begin_error {
        eprintln!("session bundle publication unavailable: {error}");
        notice_session_not_saved("fence_unavailable").await?;
    }
    if resume_id.is_some() && state.provider_session_id.is_none() {
        notice_session_not_saved("resume_not_reported").await?;
    }
    if let (Some(client), Some(provider_session_id), Some(generation)) = (
        &api_client,
        provider_session_id.as_deref(),
        bundle_generation,
    ) {
        let harvesting = harvest_complete(
            client,
            &input.task.session_id,
            &layout,
            provider_session_id,
            generation,
        );
        let bound = if forced_outcome.is_some() {
            Duration::from_secs(1)
        } else {
            NORMAL_HARVEST_GRACE
        };
        // The deterministic fake can leave this generation pending without
        // depending on an arbitrary one-second timing race. Real providers
        // always attempt the bounded harvest.
        let (harvested, harvest_timed_out) =
            if fake_defer_stop_save(&input, fake_provider, forced_outcome.is_some()) {
                (
                    Err(G5Error::ServiceUnavailable {
                        message: "fake provider deferred its forced native save".into(),
                    }),
                    true,
                )
            } else {
                match tokio::time::timeout(bound, harvesting).await {
                    Ok(result) => (result, false),
                    Err(_) => (
                        Err(G5Error::ServiceUnavailable {
                            message: "session harvest deadline exceeded".into(),
                        }),
                        true,
                    ),
                }
            };
        match harvested {
            Ok(report) if report.skipped.is_empty() && report.found > 0 => {}
            Ok(report) => {
                session_files_state = SessionFilesState::PersistFailed;
                let has_symlink = report
                    .skipped
                    .iter()
                    .any(|file| file.reason == SkipReason::Symlink);
                let has_oversized = report
                    .skipped
                    .iter()
                    .any(|file| file.reason == SkipReason::TooLarge);
                let reason = match (has_symlink, has_oversized) {
                    (true, _) => "symlink",
                    (false, true) => "too_large",
                    (false, false) => "no_resume_file",
                };
                notice_session_not_saved(reason).await?;
            }
            Err(error) => {
                session_files_state = SessionFilesState::PersistFailed;
                let reason =
                    harvest_failure_reason(&error, forced_outcome.is_some(), harvest_timed_out);
                eprintln!("session bundle publication failed: {error}");
                notice_session_not_saved(reason).await?;
            }
        }
    }
    emit("terminal.progress", json!({"phase":"finalizing"})).await?;

    // A deferred question is resumable only from a saved session that
    // stopped at it. One the process advanced past is invalidated whatever
    // ended the attempt (a provider error or an owner Stop after that turn
    // started leaves the same advanced transcript); one whose clean exit did
    // not capture a session is invalidated too. Either way no answerable
    // request outlives the outcome (`g5.chat.native-question-recovery`).
    // A forced outcome with an uncaptured but unadvanced session keeps its
    // question: the pending bundle may still complete on the next attempt.
    let lost_question = match &state.deferred_question {
        Some(_) if state.advanced_after_deferral => Some("native_session_advanced"),
        Some(_)
            if code == "native_question_deferred"
                && session_files_state != SessionFilesState::Present =>
        {
            Some("session_not_saved")
        }
        _ => None,
    };
    if lost_question.is_some() && code == "native_question_deferred" {
        (status, code, retryable) = (AgentTurnStatus::Suspended, NATIVE_QUESTION_LOST, false);
    }
    if let (Some(reason), Some(id)) = (lost_question, state.deferred_question.as_deref()) {
        emit(
            "provider_request.invalidated",
            json!({"provider_request_id":id,"reason":reason}),
        )
        .await?;
        state.events.fetch_add(1, Ordering::Relaxed);
        notice_question_lost(reason).await?;
        state.events.fetch_add(1, Ordering::Relaxed);
    }
    if code == NATIVE_QUESTION_RESTORE_FAILED {
        notice_question_restore_failed().await?;
        state.events.fetch_add(1, Ordering::Relaxed);
    }
    if turn_incomplete {
        emit("notice", json!({"code":"provider_turn_incomplete","level":"error","message":"The agent stopped before finishing its last turn.","retryable":true})).await?;
        state.events.fetch_add(1, Ordering::Relaxed);
    }
    if matches!(status, AgentTurnStatus::Failed)
        && !state.result_seen
        && state.provider_error.is_none()
        && code != NATIVE_QUESTION_RESTORE_FAILED
    {
        // Publish only fixed diagnostic text, never arbitrary provider stderr:
        // stderr can contain login tokens or secrets absent from our env list.
        emit("notice", json!({"code":"provider_start_failed","level":"error","message":provider_failure_message(input.provider.provider_id, state.config_error),"retryable":true})).await?;
        state.events.fetch_add(1, Ordering::Relaxed);
    }
    // A successful attempt ends the turn's reply with its ontology changes
    // card, bounded by the card's own fixed size.
    // A turn that reached its turn budget exceeded a limit, which counts as
    // failed for the card.
    if matches!(status, AgentTurnStatus::Succeeded) && !state.max_turns_reached {
        if let Some(ontology) = &ontology {
            if let Some(card) = ontology.card().await {
                emit("ontology.card", card).await?;
                state.events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    // Summary title (see `session_title_requested`): one short provider call
    // after a successful turn, while the platform still names the session by
    // its first words. Emitted before the outcome so the API applies it inside
    // the same turn and the session list refresh on `chat.done` shows it.
    if matches!(status, AgentTurnStatus::Succeeded)
        && session_title_requested(&input)
        && !fake_provider
    {
        if let Some(reply) = state.final_text.clone() {
            match generate_session_title(
                &input,
                &provider_env,
                &home,
                &reply,
                Instant::now() + SESSION_TITLE_TIMEOUT,
            )
            .await
            {
                Ok(generated) => {
                    // The extra call is real provider usage: fold it into the
                    // outcome so task accounting includes it.
                    if let Some(extra) = generated.usage {
                        state.usage = Some(match state.usage.take() {
                            Some(mut usage) => {
                                usage.input_tokens += extra.input_tokens;
                                usage.output_tokens += extra.output_tokens;
                                usage.cache_creation_input_tokens +=
                                    extra.cache_creation_input_tokens;
                                usage.cache_read_input_tokens += extra.cache_read_input_tokens;
                                usage.total_cost_usd =
                                    match (usage.total_cost_usd, extra.total_cost_usd) {
                                        (Some(a), Some(b)) => Some(a + b),
                                        (a, b) => a.or(b),
                                    };
                                usage
                            }
                            None => extra,
                        });
                    }
                    match generated.title {
                        Some(title) => {
                            emit(
                                "session.title",
                                json!({"title": title, "source": "provider_summary"}),
                            )
                            .await?;
                        }
                        None => eprintln!("session title: provider returned no usable title"),
                    }
                }
                Err(error) => eprintln!("session title: skipped ({error})"),
            }
        }
    }
    let approval_artifact = match &review_environment {
        Some(review) if status == AgentTurnStatus::Succeeded => match review.harvest() {
            Ok(artifact) => Some(artifact),
            Err(error) => {
                emit("notice",json!({"code":"approval_output_invalid","level":"error","message":error.to_string(),"retryable":false})).await?;
                return Ok(failed_outcome(
                    "approval_output_invalid",
                    false,
                    SessionFilesState::Absent,
                ));
            }
        },
        _ => None,
    };
    let final_message_ref = state.final_text.as_deref().map(content_ref);
    Ok(AgentTurnOutcomeV1 {
        schema_version: OUTCOME_SCHEMA.into(),
        status,
        code: code.into(),
        retryable,
        input_delivery_unknown: false,
        provider_started: true,
        final_message_ref,
        approval_artifact,
        provider_session_id,
        session_manifest_generation: input.session_binding.expected_manifest_generation,
        session_files_state,
        usage: state.usage,
        last_event_seq: state.events.load(Ordering::Relaxed),
    })
}

/// Codex runs only over the frozen App Server transport; this guest has no
/// exec launcher and never converts an exec-contract attempt
/// (`g5.chat.codex-exec-retirement`). Claude carries no Codex transport.
fn validate_codex_transport(input: &AgentTurnInputV1) -> Result<(), G5Error> {
    if input.provider.options.contains_key("codex_transport")
        && [
            "experimentalApi",
            "experimental_api",
            "capabilities",
            "features",
            "config",
            "listen",
            "argv",
        ]
        .iter()
        .any(|key| input.provider.options.contains_key(*key))
    {
        return Err(internal(
            "Codex configuration overrides are prohibited by the frozen transport contract",
        ));
    }
    match (
        input.provider.provider_id,
        input.provider.options.get("codex_transport"),
    ) {
        (ProviderId::ClaudeCode, None) => Ok(()),
        (ProviderId::Codex, Some(value)) if value.as_str() == Some(codex_app_server::TRANSPORT) => {
            Ok(())
        }
        (ProviderId::Codex, None) => Err(internal(
            "Codex requires the frozen App Server transport; this guest has no exec launcher",
        )),
        _ => Err(internal("unsupported frozen Codex transport")),
    }
}

fn app_server_adapter(
    input: &AgentTurnInputV1,
    policy: &ResolvedPolicy,
    cwd: &Path,
    attachments: &[Attachment],
    resume_id: Option<&str>,
) -> Result<AppServer, G5Error> {
    let prompt = if resume_id.is_some() {
        &input
            .resume_prompt
            .as_ref()
            .ok_or_else(|| internal("native resume requires a frozen current input"))?
            .text
    } else {
        &input.prompt.text
    };
    let mut parts = vec![json!({"type":"text","text":prompt})];
    for attachment in attachments {
        parts.push(json!({"type":"localImage","path":attachment.path}));
    }
    let thread = json!({"model":input.provider.model_id,"cwd":cwd,"sandbox":policy.sandbox.unwrap_or("read-only"),"approvalPolicy":"never"});
    let mut turn = json!({"input":parts,"model":input.provider.model_id});
    if let Some(effort) = option_string(input, "effort") {
        turn["effort"] = json!(effort);
    }
    Ok(AppServer::new(thread, turn, resume_id.map(str::to_owned)))
}

fn initial_input_acceptance(input: &AgentTurnInputV1) -> Value {
    let initial = &input.task.initial_input;
    json!({
        "reservation_id":initial.reservation_id,
        "input_id":initial.input_id,
        "revision":initial.revision,
        "delivery_generation":initial.delivery_generation,
        "system_notice_ids":initial.system_notice_ids,
    })
}

async fn confirm_initial_input(
    input: &AgentTurnInputV1,
    progress: &AttemptProgress,
    initial_notice: &mut Option<Value>,
) -> Result<(), G5Error> {
    *progress.review_input_at.lock().expect("acceptance clock") = Some(Instant::now());
    progress.input_handed.store(true, Ordering::Relaxed);
    progress.setup_resumed.notify_one();
    emit("input.accepted", initial_input_acceptance(input)).await?;
    progress.input_accepted.store(true, Ordering::Relaxed);
    if let Some(notice) = initial_notice.take() {
        emit("notice", notice).await?;
    }
    Ok(())
}

async fn handle_app_server_line(
    input: &AgentTurnInputV1,
    progress: &AttemptProgress,
    adapter: &mut AppServer,
    state: &mut TurnState,
    provider_input: &mut ProviderInput,
    line: &str,
    initial_notice: &mut Option<Value>,
) -> Result<(), G5Error> {
    let frame = serde_json::from_str(line).unwrap_or(Value::Null);
    let effects = adapter.receive(frame);
    if effects.rejected {
        progress.input_rejected.store(true, Ordering::Relaxed);
    }
    if effects.accepted && !progress.input_accepted.load(Ordering::Relaxed) {
        confirm_initial_input(input, progress, initial_notice).await?;
    }
    for write in effects.writes {
        let purpose = if write["method"] == "turn/start" {
            FramePurpose::InitialTurn
        } else {
            FramePurpose::NativeControl
        };
        provider_input.enqueue(&write, purpose, Instant::now())?;
    }
    for event in effects.events {
        project_event(input, state, event).await?;
    }
    if let Some(reason) = effects.failure {
        state.provider_error = Some(reason.into());
        let message = match reason {
            "unsupported_codex_native_request" => {
                "Codex requested an interaction that G5 does not support."
            }
            "codex_effective_configuration_mismatch" => {
                "Codex did not confirm the selected model and permissions."
            }
            "codex_binary_pin_mismatch" => "The workspace's Codex runtime needs to be updated.",
            _ => "Codex could not start or finish this turn.",
        };
        record(
            state,
            "notice",
            json!({"code":reason,"level":"error","message":message,"retryable":false}),
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_provider_line(
    input: &AgentTurnInputV1,
    policy: &ResolvedPolicy,
    transport: &mut Transport,
    state: &mut TurnState,
    pending: &mut HashMap<String, PendingRequest>,
    provider_input: &mut ProviderInput,
    line: &str,
    progress: &AttemptProgress,
    initial_notice: &mut Option<Value>,
) -> Result<(), G5Error> {
    let events = match transport {
        Transport::CodexAppServer(adapter) => {
            return handle_app_server_line(
                input,
                progress,
                adapter,
                state,
                provider_input,
                line,
                initial_notice,
            )
            .await;
        }
        Transport::ClaudeStream(mapper) => {
            let Some(frame) = parse_provider_frame(state, line).await? else {
                return Ok(());
            };
            if handle_claude_control_frame(input, policy, state, pending, provider_input, &frame)
                .await?
            {
                return Ok(());
            }
            mapper.map_frame(frame)
        }
    };
    for event in events {
        project_event(input, state, event).await?;
    }
    Ok(())
}

async fn parse_provider_frame(state: &mut TurnState, line: &str) -> Result<Option<Value>, G5Error> {
    match serde_json::from_str(line) {
        Ok(frame) => Ok(Some(frame)),
        Err(_) => {
            record(
                state,
                "notice",
                json!({"code":"provider_non_json_frame","level":"warning","message":"The provider emitted an unsupported non-JSON frame.","retryable":false}),
            ).await?;
            Ok(None)
        }
    }
}

async fn handle_claude_control_frame(
    input: &AgentTurnInputV1,
    policy: &ResolvedPolicy,
    state: &mut TurnState,
    pending: &mut HashMap<String, PendingRequest>,
    provider_input: &mut ProviderInput,
    frame: &Value,
) -> Result<bool, G5Error> {
    if frame["type"] == "result" && frame["stop_reason"] == "tool_deferred" {
        let tool = &frame["deferred_tool_use"];
        let id = tool["id"]
            .as_str()
            .filter(|id| native_request_identity_error(id).is_none());
        let session_id = frame["session_id"]
            .as_str()
            .filter(|id| native_request_identity_error(id).is_none());
        if tool["name"] != "AskUserQuestion"
            || id.is_none()
            || session_id.is_none()
            || id.is_some_and(|id| state.used_request_ids.contains(id))
            || state.restoring_question
            || state.deferred_question.is_some()
            || !state.steering_closed
            || !state.background.is_empty()
        {
            return Err(internal("unexpected native question deferral"));
        }
        state.used_request_ids.insert(id.unwrap().to_owned());
        let request = NativeRequest {
            provider_request_id: id.unwrap().to_owned(),
            tool_use_id: id.map(str::to_owned),
            kind: NativeRequestKind::Question,
            tool_name: "AskUserQuestion".into(),
            input: tool["input"].clone(),
        };
        let mut payload = request_opened_payload(&request, None, module_secrets());
        payload["resume_session_id"] = json!(session_id);
        record(state, "provider_request.opened", payload).await?;
        state.deferred_question = Some(request.provider_request_id);
    }
    if let Some(control) = parse_claude_control(frame) {
        match control {
            ClaudeControl::QuestionHook { request_id } => {
                // Defer to the saved session only while this process holds
                // nothing beyond its current turn. Steering already queued
                // for or flushed to Claude's input, a pending native request
                // or background work would advance the session past the
                // deferred question (pinned CLI, Linux, 2026-10-30), so the
                // question stays a live request of this attempt instead
                // (`g5.chat.native-question-recovery`). The decision is taken
                // before the reply is queued and closes steering admission for
                // the rest of the process: the window between the reply and
                // the `tool_deferred` result is unsafe too.
                let defer = !state.restoring_question
                    && !state.steering_closed
                    && state.deferred_question.is_none()
                    && state.background.is_empty()
                    && pending.is_empty()
                    && !state.steering.admitted_any();
                if defer {
                    state.steering_closed = true;
                }
                provider_input.enqueue(
                    &question_hook_response(&request_id, defer),
                    FramePurpose::NativeControl,
                    Instant::now(),
                )?;
            }
            ClaudeControl::Response {
                request_id,
                success,
            } if request_id == QUESTION_INIT_ID && policy.questions_enabled => {
                if !success {
                    return Err(internal("native question initialization failed"));
                }
                if state.restoring_question {
                    state.restore_deadline = Some(Instant::now() + NATIVE_QUESTION_RESTORE_GRACE);
                }
            }
            ClaudeControl::Opened(request) if state.restoring_question => {
                let answer = input.native_question_reply.as_ref().unwrap();
                if request.kind != NativeRequestKind::Question
                    || request.tool_use_id.as_deref() != Some(answer.provider_request_id.as_str())
                    || state.input_closed
                    || !pending.is_empty()
                {
                    return Err(internal("native_question_restore_mismatch"));
                }
                state
                    .used_request_ids
                    .insert(answer.provider_request_id.clone());
                state
                    .used_request_ids
                    .insert(request.provider_request_id.clone());
                pending.insert(
                    answer.provider_request_id.clone(),
                    PendingRequest {
                        request,
                        deadline: None,
                        reply_queued: false,
                    },
                );
                let line = serde_json::to_string(&json!({
                    "kind":"input.respond", "provider_request_id":answer.provider_request_id,
                    "request_id":answer.request_id, "response_digest":answer.response_digest,
                    "validated_response":answer.validated_response,
                }))
                .map_err(|error| internal(error.to_string()))?;
                handle_delivered_response(
                    provider_input,
                    pending,
                    &line,
                    Some(answer.source_attempt),
                )
                .await?;
                state.restoring_question = false;
                state.restore_deadline = None;
            }
            ClaudeControl::Opened(_) if state.input_closed => {
                // Claude's input is closed, so no reply can reach it. The
                // exit grace then runs instead of waiting on this turn.
                state.request_after_close = true;
                emit(
                    "notice",
                    json!({"code":"native_request_after_input_closed","level":"warning","message":"Claude asked for input after its input closed; the request cannot be answered.","retryable":false}),
                )
                .await?;
                state.events.fetch_add(1, Ordering::Relaxed);
            }
            ClaudeControl::Opened(request) => {
                // One use per identity per attempt: a settled or cancelled
                // id stays used, matching the persistence contract
                // (`g5.chat.native-request-boundary`). Reject through
                // Claude's own error response so it is not left blocked on
                // a control request nothing will answer.
                let rejection = if pending.contains_key(&request.provider_request_id)
                    || state
                        .used_request_ids
                        .contains(&request.provider_request_id)
                {
                    Some((
                        "provider_request_id_reused",
                        "The provider reused a native request id in one execution.",
                    ))
                } else {
                    native_request_identity_error(&request.provider_request_id)
                        .map(|reason| ("provider_request_id_invalid", reason))
                };
                if let Some((code, message)) = rejection {
                    state.provider_error = Some(format!("native request rejected: {code}"));
                    enqueue_control_rejection(
                        provider_input,
                        &request.provider_request_id,
                        message,
                    )?;
                    emit(
                        "notice",
                        json!({"code":code,"level":"error","message":message,"retryable":false}),
                    )
                    .await?;
                    state.events.fetch_add(1, Ordering::Relaxed);
                } else {
                    let timeout = (request.kind == NativeRequestKind::Permission)
                        .then(|| Duration::from_millis(policy.native_response_timeout_ms));
                    let deadline_unix_ms = timeout
                        .map(|timeout| now_unix_ms().saturating_add(timeout.as_millis() as u64));
                    record(
                        state,
                        "provider_request.opened",
                        request_opened_payload(&request, deadline_unix_ms, module_secrets()),
                    )
                    .await?;
                    state
                        .used_request_ids
                        .insert(request.provider_request_id.clone());
                    pending.insert(
                        request.provider_request_id.clone(),
                        PendingRequest {
                            request,
                            deadline: timeout.map(|timeout| Instant::now() + timeout),
                            reply_queued: false,
                        },
                    );
                }
            }
            ClaudeControl::Cancelled {
                provider_request_id,
            } => {
                let provider_request_id = pending
                    .iter()
                    .find(|(_, request)| request.request.provider_request_id == provider_request_id)
                    .map(|(id, _)| id.clone())
                    .unwrap_or(provider_request_id);
                if pending.remove(&provider_request_id).is_some() {
                    emit(
                        "provider_request.invalidated",
                        json!({"provider_request_id":provider_request_id,"reason":"provider_cancelled",
                            "source_attempt":input.native_question_reply.as_ref()
                                .filter(|answer| answer.provider_request_id == provider_request_id)
                                .map(|answer| answer.source_attempt)}),
                    )
                    .await?;
                    state.events.fetch_add(1, Ordering::Relaxed);
                }
            }
            ClaudeControl::Response {
                request_id,
                success,
            } if state.issued_interrupt.as_deref() == Some(request_id.as_str()) => {
                if !success {
                    eprintln!("Claude refused the background-wait interrupt; closing input");
                    state.interrupt_refused = true;
                }
                if let EndWait::Interrupting { replied, .. } = &mut state.end {
                    *replied = true;
                }
            }
            ClaudeControl::Unsupported(frame) => {
                state.provider_error = Some("unsupported provider control frame".into());
                // An unsupported control request would otherwise leave
                // Claude blocked awaiting a response nothing sends.
                // Reject it through the provider's own mechanism so the
                // turn settles promptly (`g5.chat.native-request-boundary`).
                if frame["type"] == "control_request" && !state.input_closed {
                    // Echo the raw id exactly; a normalized copy would
                    // not match the provider's pending control request.
                    if let Some(request_id) =
                        frame["request_id"].as_str().filter(|id| !id.is_empty())
                    {
                        enqueue_control_rejection(
                            provider_input,
                            request_id,
                            "This control request is unsupported by the G5 runtime.",
                        )?;
                    }
                }
                emit(
                    "notice",
                    json!({"code":"unsupported_provider_control_frame","level":"error","message":"The pinned provider emitted an unsupported control frame.","retryable":false}),
                )
                .await?;
                state.events.fetch_add(1, Ordering::Relaxed);
            }
            ClaudeControl::Response { .. } => {
                state.provider_error = Some("unsupported provider control frame".into());
                emit(
                    "notice",
                    json!({"code":"unsupported_provider_control_frame","level":"error","message":"The pinned provider emitted an unsupported control frame.","retryable":false}),
                )
                .await?;
                state.events.fetch_add(1, Ordering::Relaxed);
            }
        }
        return Ok(true);
    }
    Ok(false)
}

async fn project_event(
    input: &AgentTurnInputV1,
    state: &mut TurnState,
    event: TurnEvent,
) -> Result<(), G5Error> {
    if let TurnEvent::SubscriptionUsage { observation } = event {
        if let Some(reporter) = state.subscription_usage.as_mut() {
            reporter.observe(observation);
        }
        return Ok(());
    }
    let message_id = format!("{}:{}", input.task.task_id, input.task.attempt);
    // A final message's unit and text join the turn's state only once its
    // event is recorded, so the outcome never names unrecorded text. The
    // flag marks a terminal result, which also closes the turn's units.
    let mut final_unit: Option<(String, String, bool)> = None;
    let projected = match event {
        TurnEvent::SessionStarted {
            provider_session_id,
            model,
            ..
        } => {
            let phase = if input.session_binding.provider_session_id.as_deref()
                == Some(provider_session_id.as_str())
            {
                "resumed"
            } else {
                "started"
            };
            state.provider_session_id = Some(provider_session_id.clone());
            Some((
                "provider.session",
                json!({"provider_id":input.provider.provider_id.as_str(),"provider_session_id":provider_session_id,"phase":phase,"model_id":model}),
            ))
        }
        TurnEvent::TextDelta { text, unit_id } => Some((
            "message.delta",
            text_delta_payload(&message_id, unit_id.as_deref(), &text, module_secrets()),
        )),
        TurnEvent::AssistantText { text, unit_id } => {
            if state.completed_units.iter().any(|(id, _)| id == &unit_id) {
                return Ok(());
            }
            let payload = message_final_payload(
                &message_id,
                &unit_id,
                &text,
                &input.task.initial_input.input_id,
                module_secrets(),
            );
            final_unit = Some((unit_id, text, false));
            Some(("message.final", payload))
        }
        TurnEvent::Thinking { text } => Some((
            "thought.final",
            thought_final_payload(&message_id, &text, module_secrets()),
        )),
        TurnEvent::ToolStarted {
            tool_call_id,
            tool_name,
            input,
        } => Some((
            "tool.started",
            tool_started_payload(&tool_call_id, &tool_name, &input, module_secrets()),
        )),
        TurnEvent::ToolCompleted {
            tool_call_id,
            tool_name,
            output,
            is_error,
            ..
        } => Some((
            "tool.completed",
            tool_completed_payload(
                &tool_call_id,
                &tool_name,
                &output,
                is_error,
                module_secrets(),
            ),
        )),
        TurnEvent::PermissionDenied { tool_name, .. } => Some((
            "notice",
            json!({"code":"provider_permission_denied","level":"info","message":format!("Provider denied {tool_name}"),"retryable":false}),
        )),
        TurnEvent::Usage { usage, .. } => {
            // Claude reports tokens per turn but its cost as a session total;
            // Codex reports one turn per process.
            let usage = accumulate_usage(
                state.usage.take(),
                usage,
                input.provider.provider_id == ProviderId::ClaudeCode,
            );
            state.usage = Some(usage.clone());
            Some(("usage.updated", json!({"usage":usage})))
        }
        TurnEvent::TurnStarted {
            provider_session_id,
        } => {
            if state.provider_session_id.as_deref() != Some(provider_session_id.as_str()) {
                eprintln!("Claude started a follow-up turn under another session id");
            }
            // A turn after a deferral consumes input buffered before it and
            // saves past the deferred question; the question is lost to the
            // saved session (`g5.chat.native-question-recovery`).
            state.advanced_after_deferral |= state.deferred_question.is_some();
            state.in_turn = true;
            state.turn = state.turn.saturating_add(1);
            state.turn_units_from = state.completed_units.len();
            None
        }
        TurnEvent::BackgroundTasks { tasks } => {
            let now = now_unix_ms();
            let first_seen: HashMap<String, u64> = state
                .background
                .iter()
                .map(|(task, at)| (task.task_id.clone(), *at))
                .collect();
            state.background = tasks
                .into_iter()
                .map(|task| {
                    let at = first_seen.get(&task.task_id).copied().unwrap_or(now);
                    (task, at)
                })
                .collect();
            state.background_seen |= !state.background.is_empty();
            state.wait_started = if state.background.is_empty() {
                None
            } else {
                Some(state.wait_started.unwrap_or(now))
            };
            Some((
                "background_tasks.replaced",
                background_tasks_payload(&state.background, state.wait_started, module_secrets()),
            ))
        }
        TurnEvent::TaskLifecycle {
            task_id,
            phase,
            task_type,
            description,
            status,
            summary,
        } => Some(subtask_event(
            &task_id,
            phase,
            task_type.as_deref(),
            description.as_deref(),
            status.as_deref(),
            summary.as_deref(),
            module_secrets(),
        )),
        TurnEvent::Result {
            text,
            is_error,
            subtype,
            raw,
        } => {
            // Each Claude turn ends with its own result; every one must
            // succeed for the attempt to succeed.
            state.result_seen = true;
            state.in_turn = false;
            // Any result after the deferral other than the deferral itself
            // means the session went on without the question.
            state.advanced_after_deferral |=
                state.deferred_question.is_some() && raw["stop_reason"] != "tool_deferred";
            let max_turns = input.provider.provider_id == ProviderId::ClaudeCode
                && input
                    .limits
                    .max_turns
                    .or(input.provider.policy_snapshot.limits.max_turns)
                    .is_some()
                && subtype.as_deref() == Some("error_max_turns");
            state.max_turns_reached |= max_turns;
            // A turn the End's own interrupt aborted is the End's effect.
            // Claude marks such a result with an `aborted_*` terminal reason
            // (recorded at 2.1.285); any other error, or one after Claude
            // refused the interrupt, is a real failure.
            let end_aborted = is_error
                && state.issued_interrupt.is_some()
                && !state.interrupt_refused
                && subtype.as_deref() == Some("error_during_execution")
                && raw
                    .get("terminal_reason")
                    .and_then(Value::as_str)
                    .is_some_and(|reason| reason.starts_with("aborted_"));
            let turn_error = is_error && !max_turns && !end_aborted;
            state.result_error |= turn_error;
            state.end_aborted_turn |= end_aborted;
            let terminal_text = text.filter(|text| !text.trim().is_empty());
            if turn_error && state.provider_error.is_none() {
                state.provider_error = subtype;
            }
            let turn_units_from = state.turn_units_from.min(state.completed_units.len());
            state.turn_units_from = state.completed_units.len();
            terminal_text.and_then(|text| {
                let units = state.completed_units[turn_units_from..]
                    .iter()
                    .map(|(_, unit)| unit.trim())
                    .collect::<Vec<_>>();
                let repeated = units.iter().any(|unit| *unit == text.trim())
                    || ["", "\n", "\n\n", " "]
                        .iter()
                        .any(|separator| units.join(separator) == text.trim());
                if repeated || turn_error || end_aborted {
                    return None;
                }
                // The conversation keeps the first final per block, so each
                // follow-up turn's result-only answer gets its own block.
                let unit_id = if state.turn <= 1 {
                    "g5:terminal-result".to_string()
                } else {
                    format!("g5:terminal-result:{}", state.turn)
                };
                let payload = message_final_payload(
                    &message_id,
                    &unit_id,
                    &text,
                    &input.task.initial_input.input_id,
                    module_secrets(),
                );
                final_unit = Some((unit_id, text, true));
                Some(("message.final", payload))
            })
        }
        TurnEvent::ProviderError { message, .. } => {
            state.provider_error = Some(message.clone());
            Some((
                "notice",
                json!({"code":"provider_error","level":"error","message":bound_text(&redact_str(&message, module_secrets()), MAX_INLINE_TOOL_BYTES),"retryable":true}),
            ))
        }
        TurnEvent::ProviderWarning { message, .. } => Some((
            "notice",
            json!({"code":"provider_warning","level":"warning","message":bound_text(&redact_str(&message, module_secrets()), MAX_INLINE_TOOL_BYTES),"retryable":false}),
        )),
        TurnEvent::Stderr { line } => {
            eprintln!("{line}");
            None
        }
        TurnEvent::SubscriptionUsage { .. } => unreachable!("live observations bypass the journal"),
        TurnEvent::Unknown { frame } => {
            // Contract §3: frames this mapper does not model (`system`
            // unmodeled system subtypes and future types)
            // are ignored, not surfaced. They used to become user-visible
            // warning notices on every turn (QA, 2026-09-23); the log keeps
            // the type for diagnosis.
            eprintln!(
                "ignored provider frame type: {}",
                frame
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            );
            None
        }
    };
    let Some((kind, payload)) = projected else {
        return Ok(());
    };
    record(state, kind, payload).await?;
    if let Some((unit_id, text, closes_turn_units)) = final_unit {
        state.completed_units.push((unit_id, text.clone()));
        if closes_turn_units {
            state.turn_units_from = state.completed_units.len();
        }
        state.final_text = Some(text);
    }
    Ok(())
}

/// One line from the dispatcher's control channel: a native reply, or the
/// owner's End for a Claude background wait. Unknown lines are ignored.
async fn handle_control_line(
    claude: bool,
    state: &mut TurnState,
    provider_input: &mut ProviderInput,
    pending: &mut HashMap<String, PendingRequest>,
    line: &str,
) -> Result<(), G5Error> {
    let kind = serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| value.get("kind")?.as_str().map(str::to_owned));
    match kind.as_deref() {
        Some("input.steer") => {
            let eligible = claude
                && provider_input.open()
                && !state.input_closed
                && !state.steering_closed
                && state.deferred_question.is_none()
                && !state.restoring_question
                && state.end == EndWait::None
                && pending.is_empty()
                && (state.in_turn || state.background.is_empty());
            match state.steering.admit(line, eligible)? {
                steering::Admission::Write { request_id, frame } => {
                    provider_input.enqueue(
                        &frame,
                        FramePurpose::Steer { request_id },
                        Instant::now(),
                    )?;
                }
                steering::Admission::Reply { kind, payload } => {
                    record(state, kind, payload).await?;
                }
            }
            Ok(())
        }
        Some("input.respond") => {
            handle_delivered_response(provider_input, pending, line, None).await
        }
        Some(BACKGROUND_END_KIND) => {
            // Applied by `advance_claude_input`; repeats change nothing.
            if claude && state.end == EndWait::None {
                state.end = EndWait::Requested {
                    since: Instant::now(),
                };
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The earliest frozen-answer setup, question-restoration or pending
/// permission deadline.
fn next_clock(
    setup_deadline: Option<Instant>,
    restore_deadline: Option<Instant>,
    pending: &HashMap<String, PendingRequest>,
) -> Option<Instant> {
    pending
        .values()
        .filter_map(|request| request.deadline)
        .chain(setup_deadline)
        .chain(restore_deadline)
        .min()
}

async fn sleep_until_clock(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// Enforce frozen-answer setup and permission deadlines; accepted work is unbounded.
fn expired_clock(
    now: Instant,
    setup_deadline: Option<Instant>,
    restore: (bool, Option<Instant>),
    pending: &HashMap<String, PendingRequest>,
) -> Option<(AgentTurnStatus, &'static str, bool)> {
    let (restoring, restore_deadline) = restore;
    // A frozen answer whose question the resumed session does not reissue is
    // a definite restoration failure, not a slow setup: it is reported as
    // such and is not retried (`g5.chat.native-question-recovery`).
    if restoring
        && (restore_deadline.is_some_and(|at| now >= at)
            || setup_deadline.is_some_and(|at| now >= at))
    {
        return Some((
            AgentTurnStatus::Failed,
            NATIVE_QUESTION_RESTORE_FAILED,
            false,
        ));
    }
    if setup_deadline.is_some_and(|at| now >= at) {
        return Some((AgentTurnStatus::TimedOut, "wall_clock_exceeded", true));
    }
    pending
        .values()
        .any(|request| request.deadline.is_some_and(|deadline| now >= deadline))
        .then_some((AgentTurnStatus::Suspended, "native_response_timeout", false))
}

/// The dispatcher's control line for the owner's End action.
const BACKGROUND_END_KIND: &str = "background.end";

/// Advance Claude's input window: close it once Claude is between turns
/// with no background work and no pending native request, or apply the
/// owner's End by interrupting Claude's background work and then closing it.
/// End lets Claude's current turn finish for up to `END_TURN_GRACE`, then
/// interrupts it; the turn it aborts is the End's effect.
async fn advance_claude_input(
    state: &mut TurnState,
    pending: &mut HashMap<String, PendingRequest>,
    provider_input: &mut ProviderInput,
    idle_since: &mut Option<Instant>,
    now: Instant,
) -> Result<(), G5Error> {
    if state.input_closed || !provider_input.open() {
        match state.end {
            EndWait::Requested { .. } | EndWait::InterruptQueued { .. } => {
                // Input already closed before End reached Claude: background
                // work Claude still runs is ended by the exit grace and the
                // bounded stop. Without any, End has nothing to stop and a
                // turn in progress finishes normally.
                if state.background.is_empty() {
                    state.end = EndWait::None;
                } else {
                    emit_end_notice(state, false).await?;
                    state.end = EndWait::Closed;
                }
            }
            // Input was dropped before Claude answered the interrupt: End now
            // relies on the exit grace and the bounded stop.
            EndWait::Interrupting { .. } => state.end = EndWait::Closed,
            EndWait::None | EndWait::Closed => {}
        }
        return Ok(());
    }
    // End waits only, and only briefly, for Claude's own turn. A request a
    // background sub-agent left pending does not block it; Claude stops that
    // sub-agent and the request is invalidated when input closes.
    let idle = !state.in_turn;
    match &state.end {
        EndWait::Requested { .. } if state.background.is_empty() => {
            // The background work already ended: nothing is left to stop, and
            // the reply finishes normally, including Claude's report turn.
            state.end = EndWait::None;
        }
        EndWait::InterruptQueued { .. }
            if state.background.is_empty() && provider_input.withdraw_unstarted_interrupt() =>
        {
            // The work ended while the interrupt waited behind other input.
            state.end = EndWait::None;
        }
        EndWait::Requested { since } if idle || now >= *since + END_TURN_GRACE => {
            let due = *since + END_TURN_GRACE;
            let request_id = format!("g5-end-{}", uuid::Uuid::new_v4());
            // Queued behind any reply still being written. End takes effect
            // when its first byte is written (`interrupt_started`); a failed
            // write ends the wait through `provider_input_failed`.
            provider_input.enqueue(
                &interrupt_request(&request_id),
                FramePurpose::Interrupt,
                now,
            )?;
            state.end = EndWait::InterruptQueued { request_id, due };
            return Ok(());
        }
        EndWait::InterruptQueued { due, .. } if now >= *due + END_INTERRUPT_WRITE => {
            // End is due but its interrupt still waits behind input Claude
            // reads too slowly. End takes effect now. The requests end at
            // once, before anything is awaited (`invalidate_all` takes them
            // before publishing their invalidation), so a slow journal cannot
            // carry this decision past a request's deadline; the input closes
            // and the End notice follows.
            invalidate_all(pending, "background_wait_ended").await?;
            close_claude_input(state, provider_input);
            state.end = EndWait::Closed;
            emit_end_notice(state, state.in_turn).await?;
            return Ok(());
        }
        EndWait::Interrupting {
            sent_at, replied, ..
        } => {
            if *replied || now >= *sent_at + END_INTERRUPT_REPLY {
                invalidate_all(pending, "background_wait_ended").await?;
                close_claude_input(state, provider_input);
                state.end = EndWait::Closed;
            }
            return Ok(());
        }
        _ => {}
    }
    // A reply to a request Claude cancelled may still be finishing its line.
    let between_turns = idle && pending.is_empty() && !provider_input.writing();
    if !(between_turns && state.background.is_empty()) {
        *idle_since = None;
        return Ok(());
    }
    // A finished background task makes Claude start a report turn right
    // after the set empties; wait briefly for it before closing.
    let settle = if state.background_seen {
        BACKGROUND_SETTLE
    } else {
        Duration::ZERO
    };
    if now >= *idle_since.get_or_insert(now) + settle {
        close_claude_input(state, provider_input);
    }
    Ok(())
}

/// The End interrupt's first byte is being written: End takes effect now. Its
/// reply wait starts, and a turn aborted from here on is End's effect.
async fn interrupt_started(state: &mut TurnState, now: Instant) -> Result<(), G5Error> {
    let EndWait::InterruptQueued { request_id, .. } = &state.end else {
        return Ok(());
    };
    let request_id = request_id.clone();
    state.issued_interrupt = Some(request_id.clone());
    emit_end_notice(state, state.in_turn).await?;
    state.end = EndWait::Interrupting {
        request_id,
        sent_at: now,
        replied: false,
    };
    Ok(())
}

/// `turn_stopped`: End also interrupts the turn Claude is running.
async fn emit_end_notice(state: &TurnState, turn_stopped: bool) -> Result<(), G5Error> {
    let message = if turn_stopped {
        "The owner ended the wait. Claude's current turn and its background work were stopped."
    } else {
        "The owner ended the wait. Claude's background work was stopped."
    };
    emit(
        "notice",
        json!({"code":"background_wait_ended","level":"info","message":message,"retryable":false}),
    )
    .await?;
    state.events.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn close_claude_input(state: &mut TurnState, provider_input: &mut ProviderInput) {
    // Dropping the handle closes our end; a provider that already exited
    // closed its own.
    provider_input.close();
    state.input_closed = true;
}

/// A frame reached Claude's input in full. A reply counts as delivered only
/// now, and only while its request is still pending.
async fn frame_written(
    frame: QueuedFrame,
    pending: &mut HashMap<String, PendingRequest>,
    input: &AgentTurnInputV1,
) -> Result<bool, G5Error> {
    let FramePurpose::Reply {
        provider_request_id,
        request_id,
        response_digest,
        disposition,
        source_attempt,
    } = frame.purpose
    else {
        return Ok(false);
    };
    if pending.remove(&provider_request_id).is_none() {
        // Claude cancelled the request while its reply was being written.
        return Ok(false);
    }
    emit(
        "provider_request.replied",
        json!({
            "provider_request_id":provider_request_id,
            "request_id":request_id,
            "response_digest":response_digest,
            "disposition":disposition,
            "evidence":"transport_write",
            "source_attempt":source_attempt,
            "input_acceptance": source_attempt.map(|_| initial_input_acceptance(input)),
        }),
    )
    .await?;
    Ok(true)
}

/// Writing to Claude's input failed or made no progress for
/// `PROVIDER_WRITE_TIMEOUT`. A partial line may have been written, so the
/// input is dropped with whatever was still queued.
async fn provider_input_failed(
    transport: &Transport,
    state: &mut TurnState,
    provider_input: &mut ProviderInput,
    pending: &mut HashMap<String, PendingRequest>,
) -> Result<(), G5Error> {
    let unwritten = provider_input.close();
    state.input_closed = true;
    if matches!(transport, Transport::CodexAppServer(_)) {
        state.provider_error = Some("codex_input_write_failed".into());
        return Ok(());
    }
    let replies: Vec<&str> = unwritten
        .iter()
        .filter_map(|frame| match &frame.purpose {
            FramePurpose::Reply {
                provider_request_id,
                ..
            } => Some(provider_request_id.as_str()),
            FramePurpose::Interrupt
            | FramePurpose::NativeControl
            | FramePurpose::InitialTurn
            | FramePurpose::Steer { .. } => None,
        })
        .collect();
    if replies.is_empty() {
        if unwritten.iter().any(|frame| {
            matches!(
                frame.purpose,
                FramePurpose::NativeControl
                    | FramePurpose::InitialTurn
                    | FramePurpose::Steer { .. }
            )
        }) {
            // A lost SDK hook response can leave Claude blocked in a turn.
            // Keep the same bounded close/stop path used for a lost answer.
            state.reply_undeliverable = true;
            return invalidate_all(pending, "provider_input_failed").await;
        }
        // Only the End interrupt was left: End's bounded stop follows.
        return invalidate_all(pending, "background_wait_ended").await;
    }
    // An owner's reply did not reach Claude. Claude's own output still
    // decides the outcome: a clean finish succeeds, and a provider still
    // running when the exit grace ends is stopped. The requests end before
    // anything is published.
    state.reply_undeliverable = true;
    let lost_reply = replies.iter().any(|id| pending.contains_key(*id));
    invalidate_all(pending, "provider_input_failed").await?;
    if lost_reply {
        emit(
            "notice",
            json!({"code":"provider_input_failed","level":"error","message":"The agent stopped taking input, so the reply was not delivered.","retryable":true}),
        )
        .await?;
        state.events.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

/// Queue the owner's native reply for Claude's input. It is delivered once
/// completely written; until then the request keeps its deadline and Claude
/// can still cancel it.
async fn handle_delivered_response(
    provider_input: &mut ProviderInput,
    pending: &mut HashMap<String, PendingRequest>,
    line: &str,
    source_attempt: Option<u32>,
) -> Result<(), G5Error> {
    let delivered: DeliveredResponse = match serde_json::from_str(line) {
        Ok(delivered) => delivered,
        Err(_) => return Ok(()),
    };
    if delivered.kind != "input.respond" {
        return Ok(());
    }
    let Some(request) = pending.get_mut(&delivered.provider_request_id) else {
        emit(
            "notice",
            json!({"code":"stale_native_response","level":"warning","message":"A native response arrived after its provider request was no longer live.","retryable":false}),
        )
        .await?;
        return Ok(());
    };
    if request
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
        || request.reply_queued
        || !provider_input.open()
    {
        return Ok(());
    }
    let (envelope, disposition) =
        native_response_envelope(&request.request, &delivered.validated_response)
            .map_err(internal)?;
    provider_input.enqueue(
        &envelope,
        FramePurpose::Reply {
            provider_request_id: delivered.provider_request_id,
            request_id: delivered.request_id,
            response_digest: delivered.response_digest,
            disposition,
            source_attempt,
        },
        Instant::now(),
    )?;
    request.reply_queued = true;
    Ok(())
}

async fn invalidate_all(
    pending: &mut HashMap<String, PendingRequest>,
    reason: &str,
) -> Result<(), G5Error> {
    let requests = std::mem::take(pending);
    for (provider_request_id, _) in requests {
        emit(
            "provider_request.invalidated",
            json!({"provider_request_id":provider_request_id,"reason":reason}),
        )
        .await?;
    }
    Ok(())
}

async fn materialize_attachments(
    input: &AgentTurnInputV1,
    client: Option<&InternalApiClient>,
    home: &Path,
) -> Result<(Option<tempfile::TempDir>, Vec<Attachment>), &'static str> {
    if input.attachments.is_empty() {
        return Ok((None, vec![]));
    }
    let client = client.ok_or("attachment_api_unavailable")?;
    let base = home.join(".g5");
    std::fs::create_dir_all(&base).map_err(|_| "attachment_materialization_failed")?;
    let dir = tempfile::tempdir_in(base).map_err(|_| "attachment_materialization_failed")?;
    let mut materialized = Vec::with_capacity(input.attachments.len());
    for (index, descriptor) in input.attachments.iter().enumerate() {
        let response = client
            .get_chat_attachment_content(&descriptor.id)
            .await
            .map_err(|_| "attachment_unavailable")?;
        if response.attachment_id != descriptor.id
            || response.session_id != input.task.session_id
            || response.input_id.as_deref() != Some(descriptor.input_id.as_str())
            || response.file_name != descriptor.file_name
            || response.content_type != descriptor.content_type
            || response.size_bytes != descriptor.size_bytes
            || !response.sha256.eq_ignore_ascii_case(&descriptor.sha256)
        {
            return Err("attachment_identity_mismatch");
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(response.content_base64.as_bytes())
            .map_err(|_| "attachment_corrupt")?;
        if bytes.len() as i64 != descriptor.size_bytes
            || format!("{:x}", Sha256::digest(&bytes)) != descriptor.sha256.to_ascii_lowercase()
        {
            return Err("attachment_corrupt");
        }
        let format = match descriptor
            .content_type
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("image/png") => image::ImageFormat::Png,
            Some("image/jpeg") => image::ImageFormat::Jpeg,
            Some("image/gif") => image::ImageFormat::Gif,
            Some("image/webp") => image::ImageFormat::WebP,
            _ => return Err("attachment_unsupported_image"),
        };
        if image::guess_format(&bytes).ok() != Some(format) {
            return Err("attachment_invalid_image");
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        if reader.decode().is_err() {
            return Err("attachment_invalid_image");
        }
        let path = dir.path().join(format!("{index}-{}", descriptor.id));
        std::fs::write(&path, bytes).map_err(|_| "attachment_materialization_failed")?;
        materialized.push(Attachment {
            path,
            media_type: descriptor
                .content_type
                .as_deref()
                .map(str::to_ascii_lowercase),
        });
    }
    Ok((Some(dir), materialized))
}

struct LaunchFiles<'a> {
    cwd: &'a Path,
    home: &'a Path,
    attachments: &'a [Attachment],
    memory: &'a crate::memory::Binding,
}

#[allow(clippy::too_many_arguments)]
fn launch_plan(
    input: &AgentTurnInputV1,
    policy: &ResolvedPolicy,
    files: LaunchFiles<'_>,
    resume_id: Option<&str>,
    session_seed: &str,
    fake_provider: bool,
    memory_settings: Option<&str>,
) -> Result<(PathBuf, Vec<String>, Vec<u8>, Transport), G5Error> {
    let LaunchFiles {
        cwd,
        home,
        attachments,
        memory,
    } = files;
    validate_codex_transport(input)?;
    let transport = match input.provider.provider_id {
        ProviderId::ClaudeCode => Transport::ClaudeStream(ClaudeFrameMapper::with_auth(
            input.provider.auth_mode,
            input.provider.base_url.as_deref(),
        )),
        ProviderId::Codex if input.provider.options.contains_key("formal_review") => {
            Transport::CodexAppServer(crate::formal_review::codex_app_server_adapter(input, cwd))
        }
        ProviderId::Codex => Transport::CodexAppServer(app_server_adapter(
            input,
            policy,
            cwd,
            attachments,
            resume_id,
        )?),
    };
    if input.provider.options.contains_key("formal_review") {
        let (program, args, bytes) = crate::formal_review::launch(input, home, fake_provider)?;
        return Ok((program, args, bytes, transport));
    }
    match input.provider.provider_id {
        ProviderId::ClaudeCode => {
            let program = provider_binary("G5_CLAUDE_PATH", "claude", fake_provider);
            let mut args = vec![
                "-p".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--input-format".into(),
                "stream-json".into(),
                "--verbose".into(),
                "--include-partial-messages".into(),
            ];
            match resume_id {
                Some(id) => args.extend(["--resume".into(), id.into()]),
                None => args.extend(["--session-id".into(), session_seed.into()]),
            }
            args.extend([
                "--permission-mode".into(),
                policy.permission_mode.into(),
                "--permission-prompts".into(),
                policy.permission_prompts.into(),
            ]);
            if let Some(tool) = policy.permission_prompt_tool {
                args.extend(["--permission-prompt-tool".into(), tool.into()]);
            }
            if !policy.questions_enabled {
                args.extend(["--disallowedTools".into(), "AskUserQuestion".into()]);
            }
            if !policy.allowed_tools.is_empty() {
                args.push("--tools".into());
                args.extend(policy.allowed_tools.clone());
            }
            let mcp = cwd.join(".mcp.json");
            // A built-in-only selection also excludes MCP tools. With no
            // selection, retain the existing workspace MCP configuration.
            if policy.allowed_tools.is_empty() && mcp.is_file() {
                args.extend([
                    "--mcp-config".into(),
                    mcp.to_string_lossy().into_owned(),
                    "--strict-mcp-config".into(),
                ]);
            } else {
                args.push("--strict-mcp-config".into());
            }
            let settings = cwd.join(".claude/settings.json");
            if g5_contracts::claude_models::claude_native_model_overrides(&input.provider.model_id)
                .is_some()
            {
                // The host owns the frozen route. Keep memory/workspace settings,
                // but make native capability detection recognize every known
                // OpenRouter slug without changing its outbound model id.
                let mut merged: Value = if let Some(settings) = memory_settings {
                    serde_json::from_str(settings).map_err(|error| internal(error.to_string()))?
                } else if settings.is_file() {
                    serde_json::from_slice(
                        &std::fs::read(&settings).map_err(|error| internal(error.to_string()))?,
                    )
                    .map_err(|error| internal(error.to_string()))?
                } else {
                    json!({})
                };
                crate::runner::claude::apply_model_overrides(&input.provider.model_id, &mut merged)
                    .map_err(internal)?;
                args.extend(["--settings".into(), merged.to_string()]);
            } else if let Some(settings) = memory_settings {
                args.extend(["--settings".into(), settings.into()]);
            } else if settings.is_file() {
                args.extend(["--settings".into(), settings.to_string_lossy().into_owned()]);
            }
            // User scope loads the supplied ~/.claude/CLAUDE.md and native skills.
            args.extend(["--setting-sources".into(), "user,project".into()]);
            args.extend(crate::runner::claude::model_arguments(
                &input.provider.model_id,
                option_string(input, "effort").as_deref(),
            ));
            if let Some(max_turns) = input.limits.max_turns.or(input
                .provider
                .policy_snapshot
                .limits
                .max_turns)
            {
                args.extend(["--max-turns".into(), max_turns.to_string()]);
            }
            let prompt = if resume_id.is_some() {
                &input
                    .resume_prompt
                    .as_ref()
                    .ok_or_else(|| internal("native resume requires a frozen current input"))?
                    .text
            } else {
                &input.prompt.text
            };
            let mut bytes = Vec::new();
            if policy.questions_enabled {
                bytes = serde_json::to_vec(&question_initialize())
                    .map_err(|error| internal(error.to_string()))?;
                bytes.push(b'\n');
            }
            if input.native_question_reply.is_none() {
                let user = crate::runner::claude::build_stdin_checked(prompt, attachments)
                    .map_err(io_error)?;
                bytes.extend(user);
            }
            Ok((program, args, bytes, transport))
        }
        ProviderId::Codex => {
            if !policy.allowed_tools.is_empty() {
                return Err(internal(
                    "UNSUPPORTED_PERMISSION_POLICY: Codex allowed_tools",
                ));
            }
            let program = provider_binary("G5_CODEX_PATH", "codex", fake_provider);
            let mut args = codex_app_server::configuration();
            args.extend(crate::memory::codex_configuration(
                memory,
                policy.sandbox != Some("read-only"),
            ));
            for setting in [
                "approval_policy=\"never\"".to_string(),
                format!("sandbox_mode=\"{}\"", policy.sandbox.unwrap_or("read-only")),
            ] {
                args.extend(["-c".into(), setting]);
            }
            if input.provider.auth_mode
                == Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription)
            {
                args.extend(["-c".into(), "cli_auth_credentials_store=\"file\"".into()]);
            }
            let mut bytes = serde_json::to_vec(&codex_app_server::initialize())
                .map_err(|e| internal(e.to_string()))?;
            bytes.push(b'\n');
            Ok((program, args, bytes, transport))
        }
    }
}

/// Read one provider stream into lines on `sender`. Stdout (`log_limit` is
/// `None`) is read without a total; stderr reads at most its log cap before
/// `LogLimitExceeded`. A line is assembled only up to
/// `MAX_PROVIDER_LINE_BYTES`; the rest of a longer one is dropped to its
/// newline and reported once as `LineSkipped`, and the next line is read as
/// usual.
fn spawn_output_reader<T>(
    mut reader: T,
    log_limit: Option<u64>,
    sender: mpsc::Sender<ProviderOutput>,
) where
    T: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let stderr = log_limit.is_some();
    let max_bytes = log_limit.unwrap_or(u64::MAX);
    tokio::spawn(async move {
        let mut buffer = [0u8; 8192];
        let mut frame = Vec::new();
        let mut seen = 0u64;
        // The rest of a line that crossed `MAX_PROVIDER_LINE_BYTES` is
        // dropped up to its newline; reading goes on with the next line.
        let mut skipping = false;
        loop {
            let read = match reader.read(&mut buffer).await {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => {
                    let _ = sender.send(ProviderOutput::ReadFailed).await;
                    return;
                }
            };
            let allowed = max_bytes.saturating_sub(seen).min(read as u64) as usize;
            seen = seen.saturating_add(allowed as u64);
            let mut offset = 0;
            while offset < allowed {
                let end = buffer[offset..allowed]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map(|index| offset + index);
                let segment_end = end.unwrap_or(allowed);
                let segment = &buffer[offset..segment_end];
                if skipping {
                    // The rest of a dropped line.
                } else if frame.len().saturating_add(segment.len()) > MAX_PROVIDER_LINE_BYTES {
                    // Release the assembled part with the line.
                    frame = Vec::new();
                    skipping = true;
                    if sender
                        .send(ProviderOutput::LineSkipped { stderr })
                        .await
                        .is_err()
                    {
                        return;
                    }
                } else {
                    frame.extend_from_slice(segment);
                }
                if end.is_some() {
                    if skipping {
                        skipping = false;
                    } else if !send_provider_frame(&mut frame, stderr, &sender).await {
                        return;
                    }
                }
                offset = segment_end + usize::from(end.is_some());
            }
            if allowed < read {
                let _ = sender.send(ProviderOutput::LogLimitExceeded).await;
                return;
            }
        }
        if !skipping && !frame.is_empty() && !send_provider_frame(&mut frame, stderr, &sender).await
        {
            return;
        }
        let _ = sender.send(ProviderOutput::Closed).await;
    });
}

async fn send_provider_frame(
    frame: &mut Vec<u8>,
    stderr: bool,
    sender: &mpsc::Sender<ProviderOutput>,
) -> bool {
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    let bytes = std::mem::take(frame);
    if stderr {
        return sender
            .send(ProviderOutput::Stderr(
                String::from_utf8_lossy(&bytes).into_owned(),
            ))
            .await
            .is_ok();
    }
    let line = match String::from_utf8(bytes) {
        Ok(line) => line,
        Err(_) => {
            let _ = sender.send(ProviderOutput::ReadFailed).await;
            return false;
        }
    };
    sender.send(ProviderOutput::Stdout(line)).await.is_ok()
}

async fn stop_provider_group(pid: Option<u32>, child: &mut Child) {
    use g5d_foundation::process_termination::{
        terminate_tokio_child, TerminationPolicy, TerminationTarget,
    };
    let target = pid.map(TerminationTarget::ProcessGroup);
    if let Err(error) = terminate_tokio_child(
        target,
        TerminationPolicy {
            first_signal: libc::SIGINT,
            grace: Duration::from_millis(200),
            kill_deadline: Duration::from_secs(5),
            always_kill_after_grace: true,
        },
        child,
    )
    .await
    {
        tracing::warn!(?target, %error, "provider process reap unconfirmed");
    }
}

fn incarnation_client() -> Result<Option<InternalApiClient>, G5Error> {
    let Some(api_url) = std::env::var("G5D_API_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    Ok(InternalApiClient::from_workspace_env(&api_url, |key| {
        std::env::var(key).ok()
    }))
}

fn target_cwd(root: &Path, input: &AgentTurnInputV1) -> Result<PathBuf, G5Error> {
    let cwd = input
        .workspace_target
        .relative_path
        .as_deref()
        .map(|path| root.join(path))
        .unwrap_or_else(|| root.to_path_buf());
    if !cwd.is_dir() {
        return Err(internal(format!(
            "workspace target does not exist: {}",
            cwd.display()
        )));
    }
    Ok(cwd)
}

/// Standalone turns use the same launcher as dispatcher-owned terminals.
fn install_g5d_shim(
    home: &Path,
    g5d: &Path,
    loader: &[(String, String)],
) -> std::io::Result<PathBuf> {
    g5d_foundation::runtime_environment::install_g5d_launcher(&home.join(".g5/bin"), g5d, loader)
}

fn provider_binary(variable: &str, provider: &str, fake: bool) -> PathBuf {
    if let Ok(path) = std::env::var(variable) {
        return PathBuf::from(path);
    }
    let name = if fake {
        format!("fake-{provider}")
    } else {
        provider.to_string()
    };
    g5d_foundation::runtime_environment::runtime_bin_dir().join(name)
}

fn fake_provider_enabled(input: &AgentTurnInputV1) -> bool {
    fake_provider_enabled_with(input, |key| std::env::var(key).ok())
}

fn fake_defer_stop_save(input: &AgentTurnInputV1, fake_provider: bool, forced: bool) -> bool {
    fake_provider
        && forced
        && input
            .prompt
            .text
            .rsplit_once("Current input follows:\n")
            .map_or(input.prompt.text.as_str(), |(_, current)| current)
            .split_whitespace()
            .any(|word| word == FAKE_DEFER_STOP_SAVE)
}

fn pending_completion_is_definitive(error: &G5Error) -> bool {
    matches!(
        error,
        G5Error::SessionFileIntegrity { .. }
            | G5Error::Conflict { .. }
            | G5Error::NotFound { .. }
            | G5Error::Gone { .. }
            | G5Error::ValidationError { .. }
    )
}

fn fake_provider_enabled_with(
    input: &AgentTurnInputV1,
    get: impl Fn(&str) -> Option<String>,
) -> bool {
    if input
        .provider
        .options
        .get("fake_provider")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let key = match input.provider.provider_id {
        ProviderId::ClaudeCode => "G5_CLAUDE_CODE_FAKE",
        ProviderId::Codex => "G5_CODEX_FAKE",
    };
    get(key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

/// The API sets `provider.options.session_title = true` while the session is
/// still named by the platform (no title, or the provisional first-message
/// title). Absent or false means the user named it, or a summary already
/// landed, so the turn spends nothing on a title.
fn session_title_requested(input: &AgentTurnInputV1) -> bool {
    input
        .provider
        .options
        .get("session_title")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// `truncate_text` always appends its marker; this only cuts when needed.
fn bound_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        text.to_string()
    } else {
        truncate_text(text, max_bytes)
    }
}

fn session_title_prompt(request: &str, reply: &str) -> String {
    let request = bound_text(request.trim(), SESSION_TITLE_CONTEXT_BYTES);
    let reply = bound_text(reply.trim(), SESSION_TITLE_CONTEXT_BYTES);
    format!(
        "Write a title for a chat session, at most 8 words, in the same language as the request. \
         State the purpose of the session, not the outcome. Reply with the title only: \
         no quotes, no trailing period, no explanation.\n\n\
         <request>\n{request}\n</request>\n\n<reply>\n{reply}\n</reply>"
    )
}

/// First non-empty line of the provider's answer, stripped of quotes and
/// markdown decoration, bounded in length. `None` when nothing usable came
/// back (the API applies the same normalization again before storing).
fn sanitize_session_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line
        .trim_start_matches(['"', '\'', '`', '#', '*', '-'])
        .trim_end_matches(['"', '\'', '`', '*', '.'])
        .trim();
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() || collapsed.len() > SESSION_TITLE_MAX_BYTES * 4 {
        return None;
    }
    Some(bound_text(&collapsed, SESSION_TITLE_MAX_BYTES))
}

struct GeneratedTitle {
    title: Option<String>,
    usage: Option<TurnUsage>,
}

/// One-shot provider call in the turn's own credential environment (same
/// endpoint and model) but otherwise isolated: no tools, no MCP servers, no
/// hooks or project settings (it runs from a scratch directory, not the
/// workspace), no session persistence, and it never resumes the
/// conversation, so the summary cannot act on the workspace or become part of
/// the session's provider history. Claude Code only: Codex cannot be run
/// tool-less. Bounded independently by `SESSION_TITLE_TIMEOUT`; on timeout the whole process group is stopped
/// like the main provider.
async fn generate_session_title(
    input: &AgentTurnInputV1,
    provider_env: &BTreeMap<String, String>,
    home: &Path,
    reply: &str,
    wall_deadline: Instant,
) -> Result<GeneratedTitle, G5Error> {
    // Title generation has its own bounded budget after the turn completes.
    let budget = wall_deadline
        .saturating_duration_since(Instant::now())
        .min(SESSION_TITLE_TIMEOUT);
    if budget < SESSION_TITLE_MIN_BUDGET {
        return Err(internal("no title generation time left"));
    }
    if input.provider.provider_id == ProviderId::Codex {
        // `codex exec` has no switch that removes its shell/file tools; the
        // read-only sandbox only blocks writes. A summary call that could
        // still run commands on the workspace outside the turn's event
        // projection is not the tool-less call this feature promises, so
        // Codex sessions keep the first-message title.
        return Err(internal(
            "summary titles are not generated for Codex sessions",
        ));
    }
    let prompt = session_title_prompt(&input.prompt.text, reply);
    let model = input.provider.model_id.clone();
    let scratch = home.join(".g5").join("session-title");
    std::fs::create_dir_all(&scratch).map_err(io_error)?;
    let (program, args) = match input.provider.provider_id {
        ProviderId::Codex => unreachable!("excluded above"),
        ProviderId::ClaudeCode => (
            provider_binary("G5_CLAUDE_PATH", "claude", false),
            vec![
                "-p".to_string(),
                prompt,
                "--output-format".into(),
                "json".into(),
                "--model".into(),
                model,
                "--max-turns".into(),
                "1".into(),
                // Not `--bare`: it pins auth to ANTHROPIC_API_KEY and never
                // reads OAuth, which is how subscription accounts authenticate
                // here. Hooks and CLAUDE.md come from settings/project
                // discovery, which the scratch cwd plus `project`-only sources
                // leave empty.
                "--tools".into(),
                String::new(),
                "--no-session-persistence".into(),
                "--strict-mcp-config".into(),
                "--setting-sources".into(),
                "project".into(),
                "--permission-mode".into(),
                "plan".into(),
            ],
        ),
    };
    let mut command = Command::new(&program);
    command
        .args(args)
        .current_dir(&scratch)
        .env_clear()
        .envs(provider_env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| internal(format!("failed to start title provider: {error}")))?;
    let pid = child.id();
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("title provider stdout was not piped"))?;
    let mut reader = tokio::spawn(async move {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stdout.read(&mut chunk).await.map_err(io_error)?;
            if read == 0 {
                return Ok::<_, G5Error>(buffer);
            }
            if buffer.len().saturating_add(read) > SESSION_TITLE_MAX_OUTPUT_BYTES {
                return Err(internal("title provider output exceeded its limit"));
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
    });
    let result = tokio::time::timeout(budget, async {
        let stdout = (&mut reader)
            .await
            .map_err(|error| internal(format!("title reader failed: {error}")))??;
        let status = child.wait().await.map_err(io_error)?;
        Ok::<_, G5Error>((stdout, status))
    })
    .await;
    let (stdout, status) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            stop_provider_group(pid, &mut child).await;
            reader.abort();
            return Err(error);
        }
        Err(_) => {
            stop_provider_group(pid, &mut child).await;
            reader.abort();
            return Err(internal("title provider timed out"));
        }
    };
    if !status.success() {
        return Err(internal(format!(
            "title provider exited with {:?}",
            status.code()
        )));
    }
    let stdout = String::from_utf8_lossy(&stdout);
    let (text, usage) = parse_claude_title_output(&stdout);
    Ok(GeneratedTitle {
        title: text.as_deref().and_then(sanitize_session_title),
        usage,
    })
}

/// `claude -p --output-format json` prints one result object: the reply in
/// `result`, usage under `usage`, cost in `total_cost_usd`.
fn parse_claude_title_output(stdout: &str) -> (Option<String>, Option<TurnUsage>) {
    let Ok(frame) = serde_json::from_str::<Value>(stdout.trim()) else {
        return (Some(stdout.to_string()), None);
    };
    let text = frame["result"].as_str().map(str::to_string);
    let usage = frame.get("usage").map(|usage| TurnUsage {
        input_tokens: usage_u64(usage, &["input_tokens"]),
        output_tokens: usage_u64(usage, &["output_tokens"]),
        cache_creation_input_tokens: usage_u64(usage, &["cache_creation_input_tokens"]),
        cache_read_input_tokens: usage_u64(usage, &["cache_read_input_tokens"]),
        total_cost_usd: usage_f64(&frame, &["total_cost_usd"]),
        duration_ms: frame["duration_ms"].as_u64(),
        model: None,
        finish_reason: None,
    });
    (text, usage)
}

fn option_string(input: &AgentTurnInputV1, key: &str) -> Option<String> {
    input
        .provider
        .options
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Sum Claude's per-turn token counts across the attempt. Claude's cost is
/// a session total, so the latest value replaces the earlier one.
fn accumulate_usage(
    previous: Option<TurnUsage>,
    next: TurnUsage,
    cumulative_cost: bool,
) -> TurnUsage {
    let Some(mut usage) = previous else {
        return next;
    };
    usage.input_tokens += next.input_tokens;
    usage.output_tokens += next.output_tokens;
    usage.cache_creation_input_tokens += next.cache_creation_input_tokens;
    usage.cache_read_input_tokens += next.cache_read_input_tokens;
    usage.total_cost_usd = if cumulative_cost {
        next.total_cost_usd.or(usage.total_cost_usd)
    } else {
        match (usage.total_cost_usd, next.total_cost_usd) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        }
    };
    usage.duration_ms = match (usage.duration_ms, next.duration_ms) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    usage.model = next.model.or(usage.model);
    usage.finish_reason = next.finish_reason.or(usage.finish_reason);
    usage
}

/// The conversation's kind for a Claude background task type.
fn background_kind(task_type: Option<&str>) -> &'static str {
    match task_type {
        Some("local_bash") => "shell",
        Some("local_agent") => "agent",
        _ => "other",
    }
}

fn bounded_background_text(text: &str, secrets: &[String]) -> String {
    let text = redact_str(text.trim(), secrets);
    if text.len() <= MAX_BACKGROUND_TEXT_BYTES {
        text
    } else {
        truncate_text(&text, MAX_BACKGROUND_TEXT_BYTES)
    }
}

/// `background_tasks.replaced`: Claude's whole live set, bounded and
/// redacted, with a digest of the full set.
fn background_tasks_payload(
    tasks: &[(BackgroundTask, u64)],
    wait_started: Option<u64>,
    secrets: &[String],
) -> Value {
    let listed = tasks
        .iter()
        .take(MAX_BACKGROUND_TASKS_LISTED)
        .map(|(task, started)| {
            json!({
                "task_id": bounded_background_text(&task.task_id, secrets),
                "kind": background_kind(Some(task.task_type.as_str())),
                "description": bounded_background_text(&task.description, secrets),
                "started_at_unix_ms": started,
            })
        })
        .collect::<Vec<_>>();
    let all = tasks
        .iter()
        .map(|(task, _)| json!([task.task_id, task.task_type, task.description]))
        .collect::<Vec<_>>();
    let content = match tasks.len() {
        0 => "No background work remains".to_string(),
        1 => "Waiting on 1 background task".to_string(),
        count => format!("Waiting on {count} background tasks"),
    };
    json!({
        "tasks_ref": json_ref(&Value::Array(all)),
        "tasks": listed,
        "count": tasks.len(),
        "truncated": tasks.len() > MAX_BACKGROUND_TASKS_LISTED,
        "wait_started_at_unix_ms": wait_started,
        "content": content,
    })
}

/// `subtask.*` for one step of a Claude background task or sub-agent.
fn subtask_event(
    task_id: &str,
    phase: TaskPhase,
    task_type: Option<&str>,
    description: Option<&str>,
    status: Option<&str>,
    summary: Option<&str>,
    secrets: &[String],
) -> (&'static str, Value) {
    let task_id = bounded_background_text(task_id, secrets);
    let text = |value: Option<&str>| value.map(|text| bounded_background_text(text, secrets));
    match phase {
        TaskPhase::Started => {
            let kind = background_kind(task_type);
            let label = match kind {
                "shell" => "Started background command",
                "agent" => "Started background agent",
                _ => "Started background task",
            };
            let description = text(description);
            let content = match &description {
                Some(description) => format!("{label}: {description}"),
                None => label.to_string(),
            };
            (
                "subtask.started",
                json!({"task_id":task_id,"kind":kind,"description":description,"content":content}),
            )
        }
        TaskPhase::Progress => {
            let description = text(description);
            (
                "subtask.progress",
                json!({
                    "task_id":task_id,
                    "progress_ref":content_ref(description.as_deref().unwrap_or_default()),
                    "description":description,
                    "content":description.clone().unwrap_or_else(|| "Background task in progress".into()),
                }),
            )
        }
        TaskPhase::Finished => {
            let status = match status {
                Some("completed") => "ok",
                Some("stopped" | "killed") => "stopped",
                _ => "error",
            };
            let label = match status {
                "ok" => "Background task finished",
                "stopped" => "Background task stopped",
                _ => "Background task failed",
            };
            let content = match text(summary) {
                Some(summary) => format!("{label}: {summary}"),
                None => label.to_string(),
            };
            let mut payload = json!({"task_id":task_id,"status":status,"summary":text(summary),"content":content});
            if let Some(summary) = summary {
                payload["result_ref"] = json!(content_ref(summary));
            }
            ("subtask.completed", payload)
        }
    }
}

fn content_ref(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}

/// Upper bound of the recorded copy of assistant text, a thought or a text
/// delta. Far above any model reply, so a real answer is never cut, and far
/// below the 1 MiB journal event bound. `content_ref` keeps naming the full
/// text (the outcome's `final_message_ref` is the same digest).
const MAX_INLINE_TEXT_BYTES: usize = 256 * 1024;

/// `text` with the module secrets masked, then cut at a character boundary
/// until it is at most `MAX_INLINE_TEXT_BYTES` as serialized, with whether it
/// was cut. Masking comes first so a secret straddling the cut cannot leave
/// a prefix the dispatcher's exact-match redaction no longer recognises (as
/// for tool events). The bound is on the serialized form because escaping
/// control characters can grow text sixfold.
fn inline_long_text(text: &str, secrets: &[String]) -> (String, bool) {
    let text = redact_str(text, secrets);
    let serialized_len = |text: &str| {
        serde_json::to_string(text)
            .map(|json| json.len())
            .unwrap_or(usize::MAX)
    };
    if serialized_len(&text) <= MAX_INLINE_TEXT_BYTES {
        return (text, false);
    }
    let mut limit = MAX_INLINE_TEXT_BYTES.min(text.len());
    loop {
        let cut = truncate_text(&text, limit);
        let size = serialized_len(&cut);
        if size <= MAX_INLINE_TEXT_BYTES || limit <= INLINE_TRUNCATION_MARKER.len() {
            return (cut, true);
        }
        // Shrink in proportion to the overshoot: a few bytes for plain text,
        // up to sixfold for control characters.
        let scaled = (limit as u128 * MAX_INLINE_TEXT_BYTES as u128 / size as u128) as usize;
        limit = scaled.min(limit.saturating_sub(64));
    }
}

/// The `message.final` payload: content bounded by `inline_long_text`, its
/// `content_ref` the digest of the full text.
fn message_final_payload(
    message_id: &str,
    block_id: &str,
    text: &str,
    input_id: &str,
    secrets: &[String],
) -> Value {
    let (content, truncated) = inline_long_text(text, secrets);
    json!({
        "message_id": message_id,
        "block_id": block_id,
        "content_ref": content_ref(text),
        "content": content,
        "content_truncated": truncated,
        "in_reply_to_input_ids": [input_id],
    })
}

/// The `thought.final` payload, bounded like `message_final_payload`.
fn thought_final_payload(message_id: &str, text: &str, secrets: &[String]) -> Value {
    let (content, truncated) = inline_long_text(text, secrets);
    json!({
        "thought_id": format!("{message_id}:thought"),
        "content_ref": content_ref(text),
        "content": content,
        "content_truncated": truncated,
    })
}

/// Deltas are many and small; the flag is carried only when a delta was cut.
fn text_delta_payload(
    message_id: &str,
    block_id: Option<&str>,
    text: &str,
    secrets: &[String],
) -> Value {
    let (text_delta, truncated) = inline_long_text(text, secrets);
    let mut payload =
        json!({"message_id": message_id, "block_id": block_id, "text_delta": text_delta});
    if truncated {
        payload["text_truncated"] = json!(true);
    }
    payload
}

/// Queue a control-request rejection for Claude. The envelope echoes the
/// provider's own request id, up to a full provider line, and is held like
/// every native control frame under `MAX_QUEUED_CONTROL_BYTES`: a stream
/// flooding malformed or reused control requests ends as a provider error
/// instead of accumulating queued envelopes without bound.
fn enqueue_control_rejection(
    provider_input: &mut ProviderInput,
    request_id: &str,
    error: &str,
) -> Result<(), G5Error> {
    provider_input.enqueue(
        &control_error_response(request_id, error),
        FramePurpose::NativeControl,
        Instant::now(),
    )
}

/// The persistence contract for one native request or session identity
/// (`provider_requests` DDL): non-empty and trimmed, at most 512 bytes, and
/// no U+0000, which storage sanitization would re-map onto another id —
/// distinct identities are never merged (`g5.chat.native-request-boundary`).
fn native_request_identity_error(id: &str) -> Option<&'static str> {
    if id.is_empty() {
        Some("The provider's native request identity is empty.")
    } else if id.len() > 512 {
        Some("The provider's native request identity exceeds 512 bytes.")
    } else if id.contains('\0') {
        Some("The provider's native request identity carries U+0000.")
    } else if id.trim() != id {
        Some("The provider's native request identity is untrimmed.")
    } else {
        None
    }
}

/// Bound of a journaled native request's `payload` and `display`: exactly
/// the API's own per-field limit (`task_lifecycle.rs`
/// `MAX_NATIVE_REQUEST_BYTES`), so every request the API accepts is
/// journaled uncut (masked as the dispatcher masks it anyway) and only one
/// it would refuse is cut.
const MAX_INLINE_REQUEST_BYTES: usize = 64 * 1024;

/// The journaled native request: its input and display projection masked
/// and bounded like a tool input, beside the digest of the full input. The
/// request the supervisor keeps in memory stays complete, so the envelope
/// written back to the provider is unchanged; a cut copy only changes what
/// the owner sees.
fn request_opened_payload(
    request: &NativeRequest,
    deadline_unix_ms: Option<u64>,
    secrets: &[String],
) -> Value {
    let (payload, truncated) = inline_record_within(
        &redact_json(&request.input, secrets),
        MAX_INLINE_REQUEST_BYTES,
    );
    let (display, _) = inline_record_within(
        &redact_json(&display_projection(request), secrets),
        MAX_INLINE_REQUEST_BYTES,
    );
    json!({
        "provider_request_id": request.provider_request_id,
        "request_kind": request.kind.as_str(),
        "tool_name": request.tool_name,
        "payload_ref": json_ref(&request.input),
        "payload": payload,
        "payload_truncated": truncated,
        "display": display,
        "deadline_unix_ms": deadline_unix_ms,
    })
}

fn json_ref(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Upper bound of the inline tool input/output projection carried next to
/// its `*_ref` digest. The digest stays the identity of the full body; the
/// inline copy is what the timeline card shows (like `message.final` carries
/// `content` beside `content_ref`). Well under the 1 MiB journal event bound
/// and the API's 64 KiB timeline content bound.
const MAX_INLINE_TOOL_BYTES: usize = 32 * 1024;
const INLINE_TRUNCATION_MARKER: &str = "\n… [truncated]";
/// Same mask the dispatcher's redactor writes (g5d-dispatch modules/redact.rs).
const REDACTION_MASK: &str = "[REDACTED]";
/// Comma-separated names of the environment variables the dispatcher handed
/// this module as secrets (runner.rs `MODULE_ENV_SECRET_NAMES`).
const MODULE_ENV_SECRET_NAMES: &str = "G5D_SECRET_ENV_NAMES";

/// The secret values this module must never journal: the dispatch secrets
/// the runner named, plus the incarnation token. Longest first, so an
/// enclosing secret masks before a substring of it.
///
/// The dispatcher redacts every emitted line as well, but only by exact
/// match on decoded string leaves and only on what it receives: an inline
/// copy that was flattened (JSON-escaping quotes and newlines) or cut at the
/// bound could carry a secret the dispatcher no longer recognises. Masking
/// here, on the structured values, before either step, closes that.
fn module_secrets() -> &'static [String] {
    static SECRETS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    SECRETS.get_or_init(|| {
        let mut names: Vec<String> = std::env::var(MODULE_ENV_SECRET_NAMES)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect();
        names.push("G5D_WORKSPACE_TOKEN".to_string());
        sorted_secrets(names.iter().filter_map(|name| std::env::var(name).ok()))
    })
}

fn sorted_secrets(values: impl IntoIterator<Item = String>) -> Vec<String> {
    // Provider authentication trims surrounding whitespace. Its normalized
    // value can now appear in native-client output as well as the raw value.
    let mut values: Vec<String> = values
        .into_iter()
        .flat_map(|value| {
            let trimmed = (value.trim() != value).then(|| value.trim().to_string());
            std::iter::once(value).chain(trimmed)
        })
        .filter(|v| !v.is_empty())
        .collect();
    values.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    values.dedup();
    values
}

/// Mask every occurrence of a secret in the string leaves and keys of `value`.
fn redact_json(value: &Value, secrets: &[String]) -> Value {
    match value {
        Value::String(text) => Value::String(redact_str(text, secrets)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| redact_json(v, secrets)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| (redact_str(key, secrets), redact_json(v, secrets)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn redact_str(text: &str, secrets: &[String]) -> String {
    let text = secrets.iter().fold(text.to_string(), |acc, secret| {
        acc.replace(secret.as_str(), REDACTION_MASK)
    });
    redact_workspace_tokens(&text)
}

/// A workspace credential may rotate after a tool reads it but before its
/// output arrives. Identify that credential class before truncation so even a
/// delayed old token cannot leak a prefix. Other secrets use exact matching.
fn redact_workspace_tokens(text: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let mut redacted = text.to_string();
    for word in
        text.split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')))
    {
        let pieces = word.split('.').collect::<Vec<_>>();
        for token in pieces.windows(3) {
            if token[0].is_empty() || token[1].len() > 16_384 || token[2].is_empty() {
                continue;
            }
            let claims = URL_SAFE_NO_PAD
                .decode(token[1])
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            if claims.is_some_and(|claims| claims["token_use"] == "workspace_incarnation") {
                redacted = redacted.replace(&token.join("."), REDACTION_MASK);
            }
        }
    }
    redacted
}

fn tool_started_payload(call_id: &str, name: &str, input: &Value, secrets: &[String]) -> Value {
    let (inline, truncated) = inline_json(&redact_json(input, secrets));
    json!({
        "call_id": call_id,
        "name": name,
        "input_ref": json_ref(input),
        "input": inline,
        "input_truncated": truncated,
        "content": format!("Used {name}"),
    })
}

fn tool_completed_payload(
    call_id: &str,
    name: &str,
    output: &Value,
    is_error: bool,
    secrets: &[String],
) -> Value {
    let (inline, truncated) = inline_text(&tool_output_text(&redact_json(output, secrets)));
    json!({
        "call_id": call_id,
        "name": name,
        "status": if is_error { "error" } else { "ok" },
        "output_ref": json_ref(output),
        "output": inline,
        "output_truncated": truncated,
        "content": inline,
    })
}

/// The inline copy of a tool input: the JSON itself while it fits; else the
/// same structure with its long string leaves cut (a Bash `command` next to
/// an oversized `description` keeps the command renderable); else, for a
/// value with too many leaves, a truncated pretty rendering.
fn inline_json(value: &Value) -> (Value, bool) {
    inline_json_within(value, MAX_INLINE_TOOL_BYTES)
}

/// `inline_json` under an explicit bound of serialized bytes.
fn inline_json_within(value: &Value, max_bytes: usize) -> (Value, bool) {
    let serialized_len = |value: &Value| {
        serde_json::to_vec(value)
            .map(|b| b.len())
            .unwrap_or(usize::MAX)
    };
    if serialized_len(value) <= max_bytes {
        return (value.clone(), false);
    }
    // Share the budget that is left after the structure itself among the
    // string leaves, so a single long leaf keeps almost the whole bound.
    // JSON escaping can grow a leaf past its byte length, so shrink the cap
    // and retry before giving up on the structure.
    let (leaves, leaf_bytes) = string_leaves(value);
    let structure = serialized_len(value).saturating_sub(leaf_bytes);
    let mut cap = max_bytes.saturating_sub(structure) / leaves.max(1);
    while cap >= 256 {
        let bounded = bound_string_leaves(value, cap);
        if serialized_len(&bounded) <= max_bytes {
            return (bounded, true);
        }
        cap = cap.saturating_sub((cap / 16).max(64));
    }
    // Escaping grows the rendering (quotes, newlines), so it is measured as
    // serialized and cut further until it fits.
    let rendered = serde_json::to_string_pretty(value).unwrap_or_default();
    let mut limit = max_bytes;
    loop {
        let (text, _) = inline_text_within(&rendered, limit);
        let text = Value::String(text);
        if serialized_len(&text) <= max_bytes || limit <= INLINE_TRUNCATION_MARKER.len() {
            return (text, true);
        }
        limit = limit.saturating_sub((limit / 8).max(64));
    }
}

/// Key of the record that holds a cut rendering when a request projection
/// has too many leaves to keep its structure.
const TRUNCATED_RENDERING_KEY: &str = "g5_truncated_rendering";

/// `inline_json_within` for a value its consumers read as a record (a
/// native request's payload and display): where the bound cannot keep the
/// structure, the cut rendering is wrapped in a one-key record instead of
/// becoming a bare string.
fn inline_record_within(value: &Value, max_bytes: usize) -> (Value, bool) {
    match inline_json_within(value, max_bytes) {
        (Value::String(_), _) if !value.is_string() => {
            let overhead = TRUNCATED_RENDERING_KEY.len() + 8;
            let rendering = match inline_json_within(value, max_bytes.saturating_sub(overhead)).0 {
                Value::String(text) => text,
                other => other.to_string(),
            };
            (json!({ TRUNCATED_RENDERING_KEY: rendering }), true)
        }
        bounded => bounded,
    }
}

/// `(count, total bytes)` of the string leaves of `value`.
fn string_leaves(value: &Value) -> (usize, usize) {
    match value {
        Value::String(text) => (1, text.len()),
        Value::Array(items) => items
            .iter()
            .map(string_leaves)
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)),
        Value::Object(map) => map
            .values()
            .map(string_leaves)
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)),
        _ => (0, 0),
    }
}

fn bound_string_leaves(value: &Value, cap: usize) -> Value {
    match value {
        Value::String(text) if text.len() > cap => Value::String(truncate_text(text, cap)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| bound_string_leaves(v, cap)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| (key.clone(), bound_string_leaves(v, cap)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The inline copy of a tool output, bounded at a character boundary.
fn inline_text(text: &str) -> (String, bool) {
    inline_text_within(text, MAX_INLINE_TOOL_BYTES)
}

/// `inline_text` under an explicit byte bound.
fn inline_text_within(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    (truncate_text(text, max_bytes), true)
}

/// `text` cut to at most `max_bytes` including the truncation marker, at a
/// character boundary.
fn truncate_text(text: &str, max_bytes: usize) -> String {
    let mut end = max_bytes.saturating_sub(INLINE_TRUNCATION_MARKER.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{INLINE_TRUNCATION_MARKER}", &text[..end])
}

/// Flatten a provider tool result to text: a string as is, content blocks
/// joined by their text (images and other block kinds named in brackets),
/// anything else as JSON.
fn tool_output_text(output: &Value) -> String {
    match output {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|block| match block {
                Value::String(text) => text.clone(),
                Value::Object(map) => match map.get("text").and_then(Value::as_str) {
                    Some(text) => text.to_string(),
                    None => format!(
                        "[{}]",
                        map.get("type").and_then(Value::as_str).unwrap_or("block")
                    ),
                },
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => match map.get("text").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => serde_json::to_string_pretty(output).unwrap_or_default(),
        },
        other => other.to_string(),
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn env_path(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

fn harvest_failure_reason(error: &G5Error, forced: bool, timed_out: bool) -> &'static str {
    if timed_out {
        return if forced {
            "stopped_before_save"
        } else {
            "harvest_timeout"
        };
    }
    if matches!(error, G5Error::SessionFileIntegrity { .. }) {
        "no_resume_file"
    } else {
        "upload_failed"
    }
}

/// Outcome code of an attempt whose deferred question cannot be resumed from
/// the saved session; the turn holds for the owner, not for an answer.
const NATIVE_QUESTION_LOST: &str = "native_question_lost";
/// Outcome code of an answer-restoring attempt whose resumed session did not
/// reissue the question; the accepted answer stays with the request.
const NATIVE_QUESTION_RESTORE_FAILED: &str = "native_question_restore_failed";

async fn notice_question_lost(reason: &str) -> Result<(), G5Error> {
    // The reason stays in the payload for diagnosis; the owner-facing text
    // says only what happened and what to do.
    emit(
        "notice",
        json!({
            "code": NATIVE_QUESTION_LOST,
            "level": "warning",
            "reason": reason,
            "message": "The agent's question could not be kept. Continue the turn or send a new message.",
            "retryable": false,
        }),
    )
    .await
}

async fn notice_question_restore_failed() -> Result<(), G5Error> {
    emit(
        "notice",
        json!({
            "code": NATIVE_QUESTION_RESTORE_FAILED,
            "level": "error",
            "message": "Your answer could not be delivered. Send it as a new message to continue.",
            "retryable": false,
        }),
    )
    .await
}

async fn notice_session_not_saved(reason: &str) -> Result<(), G5Error> {
    emit(
        "notice",
        json!({
            "code": "session_files_persist_failed",
            "level": "warning",
            "reason": reason,
            "message": save_failure_message(reason),
            "retryable": false,
        }),
    )
    .await
}

fn save_failure_message(reason: &str) -> &'static str {
    match reason {
        "stopped_before_save" | "harvest_timeout" | "upload_failed" => {
            "This turn's provider session wasn't saved yet. If it is still incomplete at the next turn, G5 will rebuild context from the conversation history."
        }
        _ => "This turn's provider session couldn't be saved. The next turn will rebuild context from the conversation history.",
    }
}

fn initial_session_files_state(
    reported_session: Option<&str>,
    generation: Option<uuid::Uuid>,
    attempted_resume: Option<&str>,
) -> SessionFilesState {
    match (reported_session, generation, attempted_resume) {
        (Some(_), Some(_), _) => SessionFilesState::Present,
        (Some(_), None, _) | (None, _, Some(_)) => SessionFilesState::PersistFailed,
        (None, _, None) => SessionFilesState::Absent,
    }
}

/// Write one event line to the dispatcher, which journals it for the turn.
async fn emit(kind: &str, payload: Value) -> Result<(), G5Error> {
    let line = event_line(kind, payload)?;
    let mut stdout = tokio::io::stdout();
    stdout.write_all(&line).await.map_err(io_error)?;
    stdout.flush().await.map_err(io_error)
}

/// Record one provider-derived event and count it for the outcome. Every
/// event is written: a turn has no cumulative output limit
/// (`g5.agents.output-bounds`).
async fn record(state: &mut TurnState, kind: &str, payload: Value) -> Result<(), G5Error> {
    emit(kind, payload).await?;
    state.events.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Timeline text for a provider line dropped past `MAX_PROVIDER_LINE_BYTES`.
fn provider_line_skipped_message(stderr: bool) -> String {
    format!(
        "The provider emitted one {} line over {} MiB; it was not recorded.",
        if stderr { "log" } else { "output" },
        MAX_PROVIDER_LINE_BYTES / (1024 * 1024)
    )
}

/// One journal line. Provider and tool output can carry U+0000 (a command
/// that printed a binary file), which the control plane cannot store, so the
/// reported copy replaces it. Only the report changes: the turn keeps acting
/// on the provider's own values, and `*_ref` digests still name them.
fn event_line(kind: &str, mut payload: Value) -> Result<Vec<u8>, G5Error> {
    replace_nul_in_json(&mut payload);
    let mut bytes = serde_json::to_vec(&json!({"kind":kind,"payload":payload}))
        .map_err(|error| internal(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

async fn emit_outcome(outcome: AgentTurnOutcomeV1) -> Result<(), G5Error> {
    emit(
        "outcome",
        serde_json::to_value(outcome).map_err(|error| internal(error.to_string()))?,
    )
    .await
}

fn failed_outcome(
    code: &str,
    retryable: bool,
    session_files_state: SessionFilesState,
) -> AgentTurnOutcomeV1 {
    failed_outcome_with_status(
        AgentTurnStatus::Failed,
        code,
        retryable,
        session_files_state,
    )
}

fn failed_outcome_with_status(
    status: AgentTurnStatus,
    code: &str,
    retryable: bool,
    session_files_state: SessionFilesState,
) -> AgentTurnOutcomeV1 {
    AgentTurnOutcomeV1 {
        schema_version: OUTCOME_SCHEMA.into(),
        status,
        code: code.into(),
        retryable,
        input_delivery_unknown: false,
        provider_started: false,
        final_message_ref: None,
        approval_artifact: None,
        provider_session_id: None,
        session_manifest_generation: None,
        session_files_state,
        usage: None,
        last_event_seq: 0,
    }
}

fn io_error(error: impl std::fmt::Display) -> G5Error {
    internal(error.to_string())
}

fn is_provider_config_error(line: &str) -> bool {
    line.contains("Error loading config.toml") || line.contains("Error loading configuration")
}

fn provider_failure_message(provider: ProviderId, config_error: bool) -> String {
    let name = match provider {
        ProviderId::Codex => "Codex",
        ProviderId::ClaudeCode => "Claude Code",
    };
    if config_error {
        format!("{name} could not load its configuration. Check the workspace's provider configuration and MCP server settings.")
    } else {
        format!("{name} exited before completing its response. Please retry; if the problem persists, check the worker logs.")
    }
}

fn internal(message: impl Into<String>) -> G5Error {
    G5Error::Internal {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_credential_normalization_does_not_escape_redaction() {
        let secrets = sorted_secrets(["  fixture-provider-key\n".into()]);
        assert_eq!(
            redact_str("key=fixture-provider-key", &secrets),
            "key=[REDACTED]"
        );
        assert_eq!(
            redact_str("raw=  fixture-provider-key\n", &secrets),
            "raw=[REDACTED]"
        );
    }

    #[tokio::test]
    async fn workspace_client_reads_rotated_credential_for_existing_clients_and_retries() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let api = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("token");
        std::fs::write(&token_file, "initial-token").unwrap();
        let client = InternalApiClient::from_workspace_env(&api.uri(), |key| match key {
            "G5D_WORKSPACE_TOKEN_FILE" => Some(token_file.to_string_lossy().into_owned()),
            "G5D_WORKSPACE_TOKEN" => Some("stale-fallback".into()),
            _ => None,
        })
        .unwrap();
        let cloned = client.clone();
        let file = token_file.clone();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = calls.clone();
        Mock::given(method("GET"))
            .and(path("/api/internal/workspaces/ws/questions"))
            .respond_with(move |request: &wiremock::Request| {
                let call = count.fetch_add(1, Ordering::SeqCst);
                let expected = if call == 0 {
                    "Bearer initial-token"
                } else {
                    "Bearer rotated-token"
                };
                assert_eq!(request.headers.get("authorization").unwrap(), expected);
                if call == 0 {
                    let staged = file.with_extension("next");
                    std::fs::write(&staged, "rotated-token").unwrap();
                    std::fs::rename(staged, &file).unwrap();
                    ResponseTemplate::new(503)
                } else {
                    ResponseTemplate::new(200).set_body_json(json!({"data":{"sets":[]}}))
                }
            })
            .mount(&api)
            .await;
        client.list_question_sets("ws", None).await.unwrap();
        cloned.list_question_sets("ws", None).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        std::fs::remove_file(&token_file).unwrap();
        assert!(cloned.list_question_sets("ws", None).await.is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "missing credential must fail before HTTP"
        );
    }

    #[test]
    fn rotated_workspace_tokens_are_redacted_before_inline_truncation() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let token = format!(
            "e30.{}.signature",
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&json!({"token_use":"workspace_incarnation","exp":1})).unwrap()
            )
        );
        let text = format!(
            "{}token={token}. trailing",
            "x".repeat(MAX_INLINE_TOOL_BYTES - 10)
        );
        let payload = tool_completed_payload("call", "Bash", &json!({"output":text}), false, &[]);
        assert!(!payload.to_string().contains("e30."), "{payload}");
        assert_eq!(
            redact_str(&format!("old: {token}."), &[]),
            "old: [REDACTED]."
        );
        assert_eq!(redact_str("a.b.c", &[]), "a.b.c");
        let enclosing_secret = format!("private prefix {token} private suffix");
        assert_eq!(
            redact_str(&enclosing_secret, std::slice::from_ref(&enclosing_secret)),
            REDACTION_MASK
        );
    }

    #[test]
    fn harvest_timeout_notice_distinguishes_stop_from_normal_upload() {
        let timeout = G5Error::ServiceUnavailable {
            message: "session harvest deadline exceeded".into(),
        };
        assert_eq!(
            harvest_failure_reason(&timeout, true, true),
            "stopped_before_save"
        );
        assert_eq!(
            harvest_failure_reason(&timeout, false, true),
            "harvest_timeout"
        );
        assert_eq!(
            harvest_failure_reason(&timeout, false, false),
            "upload_failed"
        );
    }

    fn turn_usage(input: u64, output: u64, cost: Option<f64>) -> TurnUsage {
        TurnUsage {
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: 10,
            cache_read_input_tokens: 100,
            total_cost_usd: cost,
            duration_ms: Some(1000),
            model: Some("claude-haiku-4-5-20251001".into()),
            finish_reason: Some("end_turn".into()),
        }
    }

    #[test]
    fn claude_usage_sums_turn_tokens_and_keeps_the_latest_session_cost() {
        // 2.1.285 report turn: tokens per turn, cost a session total.
        let first = turn_usage(18, 293, Some(0.0197));
        let report = turn_usage(10, 131, Some(0.0232));
        let total = accumulate_usage(Some(first.clone()), report.clone(), true);
        assert_eq!(total.input_tokens, 28);
        assert_eq!(total.output_tokens, 424);
        assert_eq!(total.cache_creation_input_tokens, 20);
        assert_eq!(total.cache_read_input_tokens, 200);
        assert_eq!(total.total_cost_usd, Some(0.0232));
        assert_eq!(total.duration_ms, Some(2000));
        // Per-call costs (the title call) add up instead.
        assert_eq!(
            accumulate_usage(Some(first), report, false).total_cost_usd,
            Some(0.0197 + 0.0232)
        );
        assert_eq!(
            accumulate_usage(None, turn_usage(1, 2, None), true).input_tokens,
            1
        );
    }

    #[test]
    fn background_set_is_bounded_redacted_and_digested() {
        let secrets = vec!["sk-secret-token".to_string()];
        let tasks = (0..25)
            .map(|index| {
                (
                    BackgroundTask {
                        task_id: format!("task-{index}"),
                        task_type: if index == 0 {
                            "local_agent"
                        } else {
                            "local_bash"
                        }
                        .into(),
                        description: format!(
                            "curl -H 'Bearer sk-secret-token' {}",
                            "x".repeat(400)
                        ),
                    },
                    1_000 + index as u64,
                )
            })
            .collect::<Vec<_>>();
        let payload = background_tasks_payload(&tasks, Some(1_000), &secrets);
        assert_eq!(payload["count"], 25);
        assert_eq!(payload["truncated"], true);
        let listed = payload["tasks"].as_array().unwrap();
        assert_eq!(listed.len(), MAX_BACKGROUND_TASKS_LISTED);
        assert_eq!(listed[0]["kind"], "agent");
        assert_eq!(listed[1]["kind"], "shell");
        assert_eq!(listed[0]["started_at_unix_ms"], 1_000);
        let description = listed[0]["description"].as_str().unwrap();
        assert!(!description.contains("sk-secret-token"));
        assert!(description.len() <= MAX_BACKGROUND_TEXT_BYTES);
        assert!(payload["tasks_ref"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
        assert_eq!(payload["wait_started_at_unix_ms"], 1_000);
        let empty = background_tasks_payload(&[], None, &secrets);
        assert_eq!(empty["count"], 0);
        assert_eq!(empty["tasks"], json!([]));
    }

    #[test]
    fn expired_recovery_setup_wins_over_a_permission_deadline() {
        let start = Instant::now();
        let request = |deadline| PendingRequest {
            request: NativeRequest {
                provider_request_id: "q".into(),
                tool_use_id: None,
                kind: crate::protocol::NativeRequestKind::Permission,
                tool_name: "Bash".into(),
                input: json!({}),
            },
            deadline: Some(deadline),
            reply_queued: true,
        };
        let pending = HashMap::from([("q".to_string(), request(start + Duration::from_secs(2)))]);
        let setup = Some(start + Duration::from_secs(1));
        assert!(expired_clock(start, setup, (false, None), &pending).is_none());
        // A delayed wakeup after both: the earlier setup limit decides.
        let both = start + Duration::from_secs(3);
        assert!(matches!(
            expired_clock(both, setup, (false, None), &pending),
            Some((AgentTurnStatus::TimedOut, "wall_clock_exceeded", true))
        ));
        assert!(matches!(
            expired_clock(both, None, (false, None), &pending),
            Some((AgentTurnStatus::Suspended, "native_response_timeout", false))
        ));
        // While restoring a frozen answer, an expired restore grace or setup
        // budget is a definite restoration failure, not a wall-clock timeout
        // (`g5.chat.native-question-recovery`).
        let restore_due = start + Duration::from_secs(1);
        assert!(matches!(
            expired_clock(both, None, (true, Some(restore_due)), &HashMap::new()),
            Some((
                AgentTurnStatus::Failed,
                "native_question_restore_failed",
                false
            ))
        ));
        assert!(matches!(
            expired_clock(both, setup, (true, None), &HashMap::new()),
            Some((
                AgentTurnStatus::Failed,
                "native_question_restore_failed",
                false
            ))
        ));
        // Before either expires, restoring imposes no outcome.
        assert!(expired_clock(
            start,
            Some(start + Duration::from_secs(5)),
            (true, Some(start + Duration::from_secs(5))),
            &HashMap::new()
        )
        .is_none());
    }

    #[test]
    fn interrupted_turn_exit_code_matches_the_pinned_recording() {
        let recording: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/agent-turn-native/claude-2.1.321/recording.json"
        ))
        .unwrap();
        assert_eq!(
            recording["package"],
            format!(
                "@anthropic-ai/claude-code@{}",
                g5_contracts::claude_models::CLAUDE_CODE_CLI_VERSION.trim()
            )
        );
        let probes = &recording["probes"];
        // An interrupt that aborts a turn makes Claude exit with this code at
        // input EOF; an interrupt between turns does not.
        assert_eq!(
            probes["background-agent-interrupt-turn"]["exit_code"],
            CLAUDE_INTERRUPTED_TURN_EXIT_CODE
        );
        assert_eq!(probes["background-agent-interrupt"]["exit_code"], 0);
    }

    #[test]
    fn subtask_events_map_claude_task_lifecycle() {
        let secrets = vec!["hunter2".to_string()];
        let (kind, payload) = subtask_event(
            "a8bce50d36d7e4d1c",
            TaskPhase::Started,
            Some("local_agent"),
            Some("Background sleep task"),
            None,
            None,
            &secrets,
        );
        assert_eq!(kind, "subtask.started");
        assert_eq!(payload["kind"], "agent");
        assert_eq!(payload["description"], "Background sleep task");
        let (kind, payload) = subtask_event(
            "a8bce50d36d7e4d1c",
            TaskPhase::Progress,
            None,
            Some("Running with hunter2"),
            None,
            None,
            &secrets,
        );
        assert_eq!(kind, "subtask.progress");
        assert!(payload["progress_ref"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
        assert!(!payload["description"].as_str().unwrap().contains("hunter2"));
        for (status, expected) in [
            ("completed", "ok"),
            ("stopped", "stopped"),
            ("killed", "stopped"),
            ("failed", "error"),
        ] {
            let (kind, payload) = subtask_event(
                "b1nohmzbo",
                TaskPhase::Finished,
                None,
                None,
                Some(status),
                Some("Background command completed (exit code 0)"),
                &secrets,
            );
            assert_eq!(kind, "subtask.completed");
            assert_eq!(payload["status"], expected, "{status}");
            assert!(payload["result_ref"].as_str().is_some());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn accepted_input_has_no_deadline_after_more_than_an_hour() {
        let progress = AttemptProgress::default();
        progress.input_handed.store(true, Ordering::Relaxed);
        let deadlines = Deadlines {
            review_execution: false,
            setup: Instant::now(),
        };
        assert!(
            tokio::time::timeout(Duration::from_secs(7_201), deadlines.reached(&progress))
                .await
                .is_err()
        );
        assert!(expired_clock(
            Instant::now() + Duration::from_secs(7_201),
            None,
            (false, None),
            &HashMap::new()
        )
        .is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn formal_review_execution_deadline_starts_at_input_acceptance() {
        let progress = AttemptProgress::default();
        let accepted = Instant::now();
        *progress.review_input_at.lock().unwrap() = Some(accepted);
        progress.input_handed.store(true, Ordering::Relaxed);
        let deadlines = Deadlines {
            review_execution: true,
            setup: accepted - Duration::from_secs(1),
        };
        let wait = deadlines.reached(&progress);
        tokio::pin!(wait);
        assert!(
            tokio::time::timeout(Duration::from_secs(45 * 60 - 1), &mut wait)
                .await
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(2), &mut wait)
            .await
            .expect("formal review must end after its accepted-input budget");
    }

    #[tokio::test]
    async fn deadline_wait_ends_only_while_a_deadline_applies() {
        let progress = AttemptProgress::default();
        let past = Deadlines {
            review_execution: false,
            setup: Instant::now(),
        };
        tokio::time::timeout(Duration::from_secs(1), past.reached(&progress))
            .await
            .expect("an expired setup deadline is reached");
        progress.input_handed.store(true, Ordering::Relaxed);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), past.reached(&progress))
                .await
                .is_err(),
            "accepted input lifts the default deadline"
        );
    }

    /// Stderr keeps its log cap, counted before framing; stdout has none.
    #[tokio::test]
    async fn output_reader_counts_a_newline_free_log_flood_before_framing() {
        let (mut writer, reader) = tokio::io::duplex(8192);
        let (sender, mut receiver) = mpsc::channel(2);
        spawn_output_reader(reader, Some(128), sender);
        writer.write_all(&vec![b'x'; 3000]).await.unwrap();
        drop(writer);
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderOutput::LogLimitExceeded)
        ));
    }

    /// Provider stdout has no cumulative limit (`g5.agents.output-bounds`):
    /// the reader delivers every line of a stream far past the retired
    /// 64 MiB budget and holds one line at a time while doing so.
    #[tokio::test]
    async fn output_reader_reads_stdout_without_a_total() {
        const LINE: usize = 1024 * 1024;
        const LINES: usize = 80;
        let (mut writer, reader) = tokio::io::duplex(64 * 1024);
        let (sender, mut receiver) = mpsc::channel(1);
        spawn_output_reader(reader, None, sender);
        let writing = tokio::spawn(async move {
            let mut line = vec![b'x'; LINE];
            line.push(b'\n');
            for _ in 0..LINES {
                writer.write_all(&line).await.unwrap();
            }
        });
        let mut bytes = 0usize;
        loop {
            match receiver.recv().await {
                Some(ProviderOutput::Stdout(line)) => bytes += line.len() + 1,
                Some(ProviderOutput::Closed) => break,
                _ => panic!("stdout is read to its end without a limit"),
            }
        }
        assert_eq!(bytes, LINES * (LINE + 1));
        assert!(bytes > 64 * 1024 * 1024);
        writing.await.unwrap();
    }

    #[tokio::test]
    async fn output_reader_discards_a_line_over_the_ceiling_and_reads_on() {
        let (mut writer, reader) = tokio::io::duplex(64 * 1024);
        let (sender, mut receiver) = mpsc::channel(4);
        spawn_output_reader(reader, None, sender);
        let over = MAX_PROVIDER_LINE_BYTES + 3000;
        let writing = tokio::spawn(async move {
            writer.write_all(&vec![b'x'; over]).await.unwrap();
            writer.write_all(b"\nnext\n").await.unwrap();
            drop(writer);
        });
        let mut skips = 0;
        loop {
            match receiver.recv().await {
                Some(ProviderOutput::LineSkipped { stderr: false }) => skips += 1,
                Some(ProviderOutput::Stdout(line)) => {
                    assert_eq!(line, "next", "the line after the dropped one is delivered");
                    break;
                }
                _ => panic!("unexpected reader output"),
            }
        }
        assert_eq!(skips, 1, "the dropped line is reported once");
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderOutput::Closed)
        ));
        writing.await.unwrap();
    }

    #[test]
    fn long_text_events_are_bounded_beside_the_digest_of_the_full_text() {
        let text = "é".repeat(MAX_INLINE_TEXT_BYTES);
        let payload = message_final_payload("task:1", "msg:0", &text, "input-1", &[]);
        assert_eq!(payload["content_truncated"], true);
        assert_eq!(payload["content_ref"], content_ref(&text));
        let content = payload["content"].as_str().unwrap();
        assert!(content.len() <= MAX_INLINE_TEXT_BYTES);
        assert!(content.ends_with(INLINE_TRUNCATION_MARKER));
        assert_eq!(payload["in_reply_to_input_ids"], json!(["input-1"]));

        let short = message_final_payload("task:1", "msg:0", "hello", "input-1", &[]);
        assert_eq!(short["content"], "hello");
        assert_eq!(short["content_truncated"], false);

        let thought = thought_final_payload("task:1", &text, &[]);
        assert_eq!(thought["content_truncated"], true);
        assert_eq!(thought["content_ref"], content_ref(&text));
        assert!(thought["content"].as_str().unwrap().len() <= MAX_INLINE_TEXT_BYTES);

        // Control characters escape sixfold; the bound holds as serialized.
        let control = "\u{1}".repeat(MAX_INLINE_TEXT_BYTES);
        let escaped = message_final_payload("task:1", "msg:0", &control, "input-1", &[]);
        assert_eq!(escaped["content_truncated"], true);
        assert!(serde_json::to_string(&escaped["content"]).unwrap().len() <= MAX_INLINE_TEXT_BYTES);

        let delta = text_delta_payload("task:1", Some("msg:0"), "partial", &[]);
        assert_eq!(delta["text_delta"], "partial");
        assert!(delta.get("text_truncated").is_none());
        let cut = text_delta_payload("task:1", None, &text, &[]);
        assert_eq!(cut["text_truncated"], true);
        assert!(cut["text_delta"].as_str().unwrap().len() <= MAX_INLINE_TEXT_BYTES);
    }

    #[test]
    fn a_secret_straddling_a_bound_is_masked_before_the_cut() {
        let secret = format!("sk-live-{}", "Z".repeat(192));
        let secrets = vec![secret.clone()];
        let leaked = |text: &str| text.contains(&secret[..12]);
        // The 200-byte secret spans the bound from 150 bytes before it, so
        // it straddles wherever the cut lands; bounding first would keep a
        // prefix of it.
        let text = format!(
            "{}{secret}{}",
            "a".repeat(MAX_INLINE_TEXT_BYTES - 150),
            "b".repeat(400)
        );
        for payload in [
            message_final_payload("task:1", "msg:0", &text, "input-1", &secrets),
            thought_final_payload("task:1", &text, &secrets),
        ] {
            let content = payload["content"].as_str().unwrap();
            assert!(!leaked(content), "no secret prefix survives the cut");
            assert!(content.contains(REDACTION_MASK));
            assert_eq!(payload["content_truncated"], true);
            assert_eq!(
                payload["content_ref"],
                content_ref(&text),
                "digest of the original"
            );
        }
        let delta = text_delta_payload("task:1", None, &text, &secrets);
        assert!(!leaked(delta["text_delta"].as_str().unwrap()));

        let input = json!({
            "file_path": "/work/big.txt",
            "content": format!("{}{secret}{}", "a".repeat(MAX_INLINE_REQUEST_BYTES), "b".repeat(100)),
        });
        let request = NativeRequest {
            provider_request_id: "req-1".into(),
            tool_use_id: Some("toolu_1".into()),
            kind: NativeRequestKind::Permission,
            tool_name: "Write".into(),
            input: input.clone(),
        };
        let payload = request_opened_payload(&request, None, &secrets);
        assert!(!leaked(&payload["payload"].to_string()));
        assert!(!leaked(&payload["display"].to_string()));
        assert_eq!(
            payload["payload_ref"],
            json_ref(&input),
            "digest of the original"
        );
    }

    #[test]
    fn opened_request_events_are_bounded_beside_the_digest_of_the_full_input() {
        let input = json!({
            "file_path": "/work/big.txt",
            "content": "w".repeat(MAX_INLINE_REQUEST_BYTES * 4),
        });
        let request = NativeRequest {
            provider_request_id: "req-1".into(),
            tool_use_id: Some("toolu_1".into()),
            kind: NativeRequestKind::Permission,
            tool_name: "Write".into(),
            input: input.clone(),
        };
        let payload = request_opened_payload(&request, Some(1234), &[]);
        assert_eq!(payload["request_kind"], "permission");
        assert_eq!(payload["tool_name"], "Write");
        assert_eq!(payload["deadline_unix_ms"], 1234);
        assert_eq!(payload["payload_truncated"], true);
        assert_eq!(payload["payload_ref"], json_ref(&input));
        assert_eq!(payload["payload"]["file_path"], "/work/big.txt");
        let field_len = |value: &Value| serde_json::to_vec(value).unwrap().len();
        assert!(field_len(&payload["payload"]) <= MAX_INLINE_REQUEST_BYTES);
        assert!(field_len(&payload["display"]) <= MAX_INLINE_REQUEST_BYTES);
        assert_eq!(payload["display"]["tool_name"], "Write");
        assert_eq!(payload["display"]["input"]["file_path"], "/work/big.txt");
        // The whole event stays far below the dispatcher's 1 MiB event bound.
        let line = event_line("provider_request.opened", payload).unwrap();
        assert!(line.len() < 160 * 1024, "{}", line.len());

        // Anything the API accepts today is journaled uncut: a 40 KiB
        // input is above the tool bound but within the request bound.
        let medium = NativeRequest {
            input: json!({"file_path": "/work/m.txt", "content": "m".repeat(40 * 1024)}),
            ..request.clone()
        };
        let payload = request_opened_payload(&medium, None, &[]);
        assert_eq!(payload["payload"], medium.input);
        assert_eq!(payload["payload_truncated"], false);
        assert_eq!(payload["display"]["input"], medium.input);

        // Too many leaves to keep the structure: the rendering is cut as
        // serialized, escapes included, and still fits the API's bound.
        let leaves: Vec<Value> = (0..20_000).map(|i| json!(format!("\"q{i}\""))).collect();
        let wide_request = NativeRequest {
            input: json!({ "items": leaves.clone() }),
            ..request.clone()
        };
        let payload = request_opened_payload(&wide_request, None, &[]);
        for field in ["payload", "display"] {
            assert!(payload[field].is_object(), "{field} stays a record");
            assert!(
                serde_json::to_vec(&payload[field]).unwrap().len() <= MAX_INLINE_REQUEST_BYTES,
                "{field} within the bound"
            );
        }
        assert_eq!(payload["payload_truncated"], true);
        assert!(payload["payload"][TRUNCATED_RENDERING_KEY].is_string());
        let (wide, truncated) =
            inline_json_within(&json!({"items": leaves}), MAX_INLINE_REQUEST_BYTES);
        assert!(truncated && wide.is_string());
        assert!(serde_json::to_vec(&wide).unwrap().len() <= MAX_INLINE_REQUEST_BYTES);

        let small = NativeRequest {
            input: json!({"command": "ls"}),
            ..request
        };
        let payload = request_opened_payload(&small, None, &[]);
        assert_eq!(payload["payload"], json!({"command": "ls"}));
        assert_eq!(payload["payload_truncated"], false);
        assert_eq!(payload["display"]["input"], json!({"command": "ls"}));
        assert!(payload["deadline_unix_ms"].is_null());
    }

    #[tokio::test]
    async fn output_reader_keeps_a_complete_log_line_before_the_byte_cap() {
        let (mut writer, reader) = tokio::io::duplex(8192);
        let (sender, mut receiver) = mpsc::channel(2);
        spawn_output_reader(reader, Some(6), sender);
        writer.write_all(b"ok\nxxxx").await.unwrap();
        drop(writer);
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderOutput::Stderr(line)) if line == "ok"
        ));
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderOutput::LogLimitExceeded)
        ));
    }

    #[tokio::test]
    async fn stderr_decoding_does_not_fail_the_protocol_channel() {
        let (mut writer, reader) = tokio::io::duplex(8192);
        let (sender, mut receiver) = mpsc::channel(2);
        spawn_output_reader(reader, Some(100), sender);
        writer.write_all(&[0xff, b'\n']).await.unwrap();
        drop(writer);
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderOutput::Stderr(_))
        ));
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderOutput::Closed)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn the_agent_gets_a_g5d_shim_carrying_only_g5d_s_loader_path() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("print-env");
        std::fs::write(
            &target,
            "#!/bin/sh\nprintf '%s|%s' \"$LD_LIBRARY_PATH\" \"$*\"\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let loader = vec![(
            "LD_LIBRARY_PATH".to_string(),
            "/opt/g5/it's bin".to_string(),
        )];
        let bin = install_g5d_shim(home.path(), &target, &loader).unwrap();
        assert_eq!(bin, home.path().join(".g5/bin"));
        let output = std::process::Command::new(bin.join("g5d"))
            .args(["issue", "show", "acme/web#12"])
            .env_remove("LD_LIBRARY_PATH")
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "/opt/g5/it's bin|issue show acme/web#12"
        );
        // Reinstalling for the next turn replaces the shim in place.
        install_g5d_shim(home.path(), &target, &[]).unwrap();
        assert_eq!(
            prepend_path(&bin, "/usr/bin:/bin"),
            format!("{}:/usr/bin:/bin", bin.display())
        );
        assert_eq!(prepend_path(&bin, ""), bin.display().to_string());
    }

    #[test]
    fn session_title_is_requested_only_by_an_explicit_true_option() {
        let mut input = input(ProviderId::ClaudeCode, policy(ProviderId::ClaudeCode));
        assert!(!session_title_requested(&input));
        input
            .provider
            .options
            .insert("session_title".into(), json!(true));
        assert!(session_title_requested(&input));
        input
            .provider
            .options
            .insert("session_title".into(), json!("yes"));
        assert!(!session_title_requested(&input));
    }

    #[test]
    fn native_state_requires_a_reported_session_and_a_successful_fence() {
        let generation = uuid::Uuid::new_v4();
        assert_eq!(
            initial_session_files_state(Some("sid"), Some(generation), None),
            SessionFilesState::Present
        );
        assert_eq!(
            initial_session_files_state(Some("sid"), None, None),
            SessionFilesState::PersistFailed
        );
        assert_eq!(
            initial_session_files_state(None, Some(generation), Some("prior")),
            SessionFilesState::PersistFailed
        );
        assert_eq!(
            initial_session_files_state(None, Some(generation), None),
            SessionFilesState::Absent
        );
    }

    #[test]
    fn session_title_sanitizer_keeps_one_clean_line() {
        assert_eq!(
            sanitize_session_title("\n\"Fix preview readiness.\"\nignored"),
            Some("Fix preview readiness".to_string())
        );
        assert_eq!(
            sanitize_session_title("**Diagnose   blank   preview tab**"),
            Some("Diagnose blank preview tab".to_string())
        );
        assert_eq!(sanitize_session_title("  \n \"\" \n"), None);
    }

    #[test]
    fn claude_title_output_yields_text_and_usage() {
        let (text, usage) = parse_claude_title_output(
            r#"{"type":"result","subtype":"success","result":"Diagnose blank preview tab","total_cost_usd":0.002,"duration_ms":900,"usage":{"input_tokens":120,"output_tokens":8,"cache_creation_input_tokens":0,"cache_read_input_tokens":50}}"#,
        );
        assert_eq!(text.as_deref(), Some("Diagnose blank preview tab"));
        let usage = usage.expect("usage");
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_input_tokens
            ),
            (120, 8, 50)
        );
        assert_eq!(usage.total_cost_usd, Some(0.002));
        // Non-JSON output is passed through as text with no usage.
        let (text, usage) = parse_claude_title_output("Plain title\n");
        assert_eq!(text.as_deref(), Some("Plain title\n"));
        assert!(usage.is_none());
    }

    #[test]
    fn session_title_prompt_quotes_bounded_request_and_reply() {
        let prompt = session_title_prompt(&"x".repeat(5000), "done");
        assert!(prompt.contains("<request>") && prompt.contains("<reply>\ndone\n</reply>"));
        assert!(prompt.len() < 5000);
    }

    #[test]
    fn failed_save_notice_only_conditions_rebuild_for_pending_saves() {
        for reason in ["stopped_before_save", "harvest_timeout", "upload_failed"] {
            assert!(save_failure_message(reason).contains("If it is still incomplete"));
        }
        for reason in [
            "symlink",
            "too_large",
            "no_resume_file",
            "resume_not_reported",
            "fence_unavailable",
        ] {
            assert!(save_failure_message(reason).contains("The next turn will rebuild"));
        }
    }
    use crate::types::*;

    #[tokio::test]
    async fn subscription_usage_is_nonblocking_unjournaled_and_subscription_only() {
        use crate::subscription_usage::{Observation, Reporter};
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let api = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).set_delay(Duration::from_millis(300)))
            .expect(1)
            .mount(&api)
            .await;
        let client = InternalApiClient::new(&api.uri());
        let mut turn = input(ProviderId::Codex, policy(ProviderId::Codex));
        turn.task.task_id = uuid::Uuid::new_v4().to_string();
        turn.provider.auth_mode = Some(g5_contracts::types::AgentAuthModeV1::OpenAiApiKey);
        assert!(Reporter::start(&turn, Some(client.clone())).is_none());
        turn.provider.auth_mode = Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription);
        let mut state = TurnState {
            subscription_usage: Reporter::start(&turn, Some(client)),
            ..TurnState::default()
        };
        let observation = Observation::codex(&json!({"params":{"rateLimits":{"primary":{"usedPercent":23,"windowDurationMins":300}}}})).unwrap();
        let event = TurnEvent::SubscriptionUsage { observation };
        assert!(crate::runner::events::ProjectionState::default()
            .project(
                crate::runner::events::ProviderLabel::CODEX,
                "message",
                &event
            )
            .is_none());
        tokio::time::timeout(
            Duration::from_millis(100),
            project_event(&turn, &mut state, event),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(state.events.load(Ordering::Relaxed), 0);
        assert!(state.provider_error.is_none());
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !api.received_requests().await.unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(
            api.received_requests().await.unwrap().len(),
            1,
            "failed meter delivery is not retried"
        );
    }

    fn input(provider: ProviderId, policy: AgentPermissionPolicyV1) -> AgentTurnInputV1 {
        let text = "Do the work".to_string();
        AgentTurnInputV1 {
            recovery: None,
            schema_version: crate::types::INPUT_SCHEMA.into(),
            task: AgentTurnTaskV1 {
                task_id: "task".into(),
                attempt: 2,
                session_id: "session".into(),
                input_origin: InputOrigin::Workflow,
                initial_input: AgentSessionInputReservationV1 {
                    reservation_id: "reservation".into(),
                    input_id: "input".into(),
                    revision: 1,
                    delivery_generation: 1,
                    input_origin: InputOrigin::Workflow,
                    system_notice_ids: vec![],
                },
            },
            provider: AgentTurnProviderV1 {
                auth_mode: None,
                base_url: None,
                provider_id: provider,
                model_id: "model".into(),
                policy_ref: "policy".into(),
                policy_snapshot: policy,
                // Codex attempts always freeze App Server.
                options: if provider == ProviderId::Codex {
                    BTreeMap::from([(
                        "codex_transport".to_string(),
                        json!(codex_app_server::TRANSPORT),
                    )])
                } else {
                    BTreeMap::new()
                },
            },
            prompt: FrozenPromptV1 {
                sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
                text,
            },
            resume_prompt: None,
            native_question_reply: None,
            attachments: vec![],
            attachment_preflight_error: None,
            secret_preflight_error: None,
            workspace_target: WorkspaceTargetV1::default(),
            session_binding: SessionBindingV1 {
                session_files_state: SessionFilesState::Absent,
                provider_session_id: None,
                expected_manifest_generation: None,
                restore_ref: None,
            },
            continuation: None,
            limits: AgentTurnLimitsV1 {
                setup_timeout_ms: 60_000,
                max_turns: (provider == ProviderId::ClaudeCode).then_some(10),
                max_output_bytes: Some(1024),
            },
        }
    }

    /// Drive a Codex launch's adapter through initialization and thread
    /// creation; returns the thread request and the initial turn request.
    fn codex_initial_requests(transport: Transport, model: &str) -> (Value, Value) {
        let Transport::CodexAppServer(mut adapter) = transport else {
            panic!("Codex launches own an App Server adapter");
        };
        let next = adapter.receive(json!({"id":"g5-initialize","result":{"userAgent":format!("g5_agent/{} (Linux)",codex_app_server::VERSION.trim())}}));
        let thread = next.writes[1].clone();
        let id = thread["params"]["threadId"]
            .as_str()
            .unwrap_or("thread-new");
        // Also satisfies a formal review's confined-thread evidence.
        let turn = adapter.receive(json!({"id":"g5-thread","result":{"thread":{"id":id,"ephemeral":true},"model":model,"approvalPolicy":"never","sandbox":{"type":"readOnly"},"instructionSources":[]}}));
        (thread, turn.writes[0].clone())
    }

    fn policy(provider: ProviderId) -> AgentPermissionPolicyV1 {
        AgentPermissionPolicyV1 {
            schema_version: crate::types::POLICY_SCHEMA.into(),
            provider,
            questions: crate::types::QuestionPolicy::Disabled,
            tool_approval: crate::types::ToolApproval::AutoDeny,
            sandbox: if provider == ProviderId::ClaudeCode {
                crate::types::SandboxPolicy::None
            } else {
                crate::types::SandboxPolicy::ReadOnly
            },
            allowed_tools: vec![],
            limits: PermissionLimitsV1 {
                setup_timeout_ms: 60_000,
                max_turns: (provider == ProviderId::ClaudeCode).then_some(10),
                max_output_bytes: Some(1024),
                native_response_timeout_ms: Some(1000),
            },
        }
    }

    #[tokio::test]
    async fn failed_native_control_write_keeps_claude_reply_and_codex_failure_distinct() {
        for (provider, app_server) in [(ProviderId::ClaudeCode, false), (ProviderId::Codex, true)] {
            let mut input = input(provider, policy(provider));
            if app_server {
                input
                    .provider
                    .options
                    .insert("codex_transport".into(), json!(codex_app_server::TRANSPORT));
            }
            let temp = tempfile::tempdir().unwrap();
            let resolved = resolve_policy(&input.provider.policy_snapshot).unwrap();
            let (_, _, _, transport) = launch_plan(
                &input,
                &resolved,
                LaunchFiles {
                    cwd: temp.path(),
                    home: temp.path(),
                    attachments: &[],
                    memory: &crate::memory::Binding::Disabled,
                },
                None,
                "session",
                true,
                None,
            )
            .unwrap();
            let mut state = TurnState {
                in_turn: true,
                ..TurnState::default()
            };
            let mut provider_input = ProviderInput::default();
            provider_input
                .enqueue(
                    &json!({"type":"control_response"}),
                    FramePurpose::NativeControl,
                    Instant::now(),
                )
                .unwrap();
            provider_input_failed(
                &transport,
                &mut state,
                &mut provider_input,
                &mut HashMap::new(),
            )
            .await
            .unwrap();
            assert!(state.input_closed);
            assert!(provider_input.queue.is_empty());
            assert!(
                transport.exit_grace_running(&state),
                "failed input must bound shutdown"
            );
            if app_server {
                assert_eq!(
                    state.provider_error.as_deref(),
                    Some("codex_input_write_failed")
                );
                assert!(
                    !state.reply_undeliverable,
                    "Codex failure cannot hold a Claude reply for Continue"
                );
            } else {
                assert!(state.provider_error.is_none());
                assert!(
                    state.reply_undeliverable,
                    "lost Claude SDK response retains Continue semantics"
                );
            }
        }
    }

    #[test]
    fn native_request_identities_follow_the_persistence_contract() {
        assert!(native_request_identity_error("toolu-1").is_none());
        assert!(native_request_identity_error(&"x".repeat(512)).is_none());
        assert!(native_request_identity_error("").is_some());
        assert!(native_request_identity_error(&"x".repeat(513)).is_some());
        assert!(native_request_identity_error("a\0b").is_some());
        assert!(native_request_identity_error(" padded ").is_some());
    }

    fn can_use_tool(request_id: &str) -> String {
        json!({
            "type":"control_request","request_id":request_id,
            "request":{"subtype":"can_use_tool","tool_name":"Bash",
                "tool_use_id":request_id,"input":{}}
        })
        .to_string()
    }

    fn queued_control_response(provider_input: &ProviderInput) -> Value {
        let frame = provider_input
            .queue
            .back()
            .expect("a rejection must be queued for Claude");
        assert!(matches!(frame.purpose, FramePurpose::NativeControl));
        serde_json::from_slice(&frame.bytes).expect("queued frame is one JSON line")
    }

    /// A settled id must not reopen, and an unsupported control request must
    /// not leave Claude blocked: both are answered with the provider's error
    /// response and fail the turn (`g5.chat.native-request-boundary`).
    #[tokio::test]
    async fn reused_and_unsupported_control_requests_are_rejected_to_claude() {
        let mut policy = policy(ProviderId::ClaudeCode);
        policy.questions = crate::types::QuestionPolicy::Enabled;
        policy.tool_approval = crate::types::ToolApproval::Prompt;
        let input = input(ProviderId::ClaudeCode, policy.clone());
        let resolved = resolve_policy(&policy).unwrap();
        let mut mapper = Transport::ClaudeStream(ClaudeFrameMapper::new());
        let mut state = TurnState {
            in_turn: true,
            ..TurnState::default()
        };
        let mut pending = HashMap::new();
        let mut provider_input = ProviderInput::default();

        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &can_use_tool("req-1"),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(pending.contains_key("req-1"));
        assert!(state.provider_error.is_none());

        // The request settles and leaves the live map; its identity stays used.
        pending.remove("req-1");
        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &can_use_tool("req-1"),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(
            !pending.contains_key("req-1"),
            "a reused id must not reopen"
        );
        assert!(state.provider_error.is_some());
        let rejection = queued_control_response(&provider_input);
        assert_eq!(rejection["type"], "control_response");
        assert_eq!(rejection["response"]["subtype"], "error");
        assert_eq!(rejection["response"]["request_id"], "req-1");

        let mut state = TurnState {
            in_turn: true,
            ..TurnState::default()
        };
        let mut provider_input = ProviderInput::default();
        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &json!({"type":"control_request","request_id":"ctl-9",
                "request":{"subtype":"set_permission_mode"}})
            .to_string(),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(state.provider_error.is_some());
        let rejection = queued_control_response(&provider_input);
        assert_eq!(rejection["response"]["subtype"], "error");
        assert_eq!(rejection["response"]["request_id"], "ctl-9");

        let mut state = TurnState {
            in_turn: true,
            ..TurnState::default()
        };
        let mut provider_input = ProviderInput::default();
        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            // Storage sanitization would re-map U+0000 onto another identity.
            &can_use_tool("a\u{0}b"),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(state.provider_error.is_some());
        assert!(
            pending.is_empty(),
            "an invalid identity must never be opened"
        );
    }

    /// The control-reply queue has its own fixed bound
    /// (`g5.agents.output-bounds`). Native control frames fill it; the one
    /// that does not fit is dropped and flagged, and nothing is queued behind
    /// the gap. The owner's input is not counted against it.
    #[test]
    fn queued_control_replies_have_a_fixed_bound_the_owners_input_does_not_share() {
        let now = Instant::now();
        let frame = json!({"type":"control_response","pad":"x".repeat(100 * 1024)});
        let size = serde_json::to_vec(&frame).unwrap().len() + 1;
        let fits = MAX_QUEUED_CONTROL_BYTES / size;
        assert!(fits >= 2);
        let mut input = ProviderInput::default();
        for _ in 0..fits {
            input
                .enqueue(&frame, FramePurpose::NativeControl, now)
                .unwrap();
        }
        assert!(!input.control_overflow);
        assert_eq!(input.queue.len(), fits);
        assert_eq!(input.control_bytes, fits * size);

        input
            .enqueue(
                &frame,
                FramePurpose::Steer {
                    request_id: "steer-1".into(),
                },
                now,
            )
            .unwrap();
        input.enqueue(&frame, FramePurpose::Interrupt, now).unwrap();
        assert!(!input.control_overflow, "the owner's input is not counted");
        assert_eq!(input.queue.len(), fits + 2);

        input
            .enqueue(&frame, FramePurpose::NativeControl, now)
            .unwrap();
        assert!(input.control_overflow);
        input
            .enqueue(&json!({}), FramePurpose::NativeControl, now)
            .unwrap();
        assert_eq!(
            input.queue.len(),
            fits + 2,
            "the reply that did not fit, and every later one, is dropped"
        );
        assert!(input.control_bytes <= MAX_QUEUED_CONTROL_BYTES);
    }

    /// A provider that reads its replies frees their room, so control frames
    /// far past the bound in total never overflow it.
    #[test]
    fn written_control_replies_free_their_room() {
        let now = Instant::now();
        let frame = json!({"type":"control_response","pad":"x".repeat(100 * 1024)});
        let size = serde_json::to_vec(&frame).unwrap().len() + 1;
        let mut input = ProviderInput::default();
        for _ in 0..(4 * MAX_QUEUED_CONTROL_BYTES / size) {
            input
                .enqueue(&frame, FramePurpose::NativeControl, now)
                .unwrap();
            assert!(input.advance(size / 2, now).is_none());
            assert!(input.advance(size - size / 2, now).is_some());
        }
        assert!(!input.control_overflow);
        assert_eq!(input.control_bytes, 0);
        assert!(input.queue.is_empty());
    }

    /// A provider flooding rejected control requests without reading the
    /// replies never makes the supervisor hold more than the bound: with
    /// long request ids, which every rejection echoes, with one id longer
    /// than the bound itself, and with question hooks, which are answered
    /// rather than rejected (`g5.agents.output-bounds`).
    #[tokio::test]
    async fn a_flood_of_unread_control_replies_is_held_within_the_fixed_bound() {
        let mut policy = policy(ProviderId::ClaudeCode);
        policy.questions = crate::types::QuestionPolicy::Enabled;
        policy.tool_approval = crate::types::ToolApproval::Prompt;
        let input = input(ProviderId::ClaudeCode, policy.clone());
        let resolved = resolve_policy(&policy).unwrap();
        let unsupported = |id: &str| {
            json!({"type":"control_request","request_id":id,
                "request":{"subtype":"set_permission_mode"}})
            .to_string()
        };
        let hook = |id: &str| {
            json!({"type":"control_request","request_id":id,
                "request":{"subtype":"hook_callback","callback_id":"g5-native-question",
                    "input":{"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion"}}})
            .to_string()
        };
        let long_id = "i".repeat(100 * 1024);
        let over_bound_id = "i".repeat(MAX_QUEUED_CONTROL_BYTES + 1);
        for (name, line, sent_before_overflow) in [
            ("long ids", unsupported(&long_id), 11usize),
            ("one id over the bound", unsupported(&over_bound_id), 1),
            ("question hooks", hook(&long_id), 11),
        ] {
            let mut mapper = Transport::ClaudeStream(ClaudeFrameMapper::new());
            let mut state = TurnState {
                in_turn: true,
                ..TurnState::default()
            };
            let mut pending = HashMap::new();
            let mut provider_input = ProviderInput::default();
            let mut sent = 0usize;
            while !provider_input.control_overflow {
                sent += 1;
                assert!(sent <= 64, "{name}: the flood reaches the bound");
                handle_provider_line(
                    &input,
                    &resolved,
                    &mut mapper,
                    &mut state,
                    &mut pending,
                    &mut provider_input,
                    &line,
                    &AttemptProgress::default(),
                    &mut None,
                )
                .await
                .unwrap();
            }
            assert_eq!(sent, sent_before_overflow, "{name}");
            // The same flood goes on: nothing more is held.
            for _ in 0..8 {
                handle_provider_line(
                    &input,
                    &resolved,
                    &mut mapper,
                    &mut state,
                    &mut pending,
                    &mut provider_input,
                    &line,
                    &AttemptProgress::default(),
                    &mut None,
                )
                .await
                .unwrap();
            }
            let held: usize = provider_input
                .queue
                .iter()
                .map(|frame| frame.bytes.len())
                .sum();
            assert!(
                held <= MAX_QUEUED_CONTROL_BYTES,
                "{name}: {held} bytes held"
            );
            assert_eq!(held, provider_input.control_bytes, "{name}");
            assert_eq!(provider_input.queue.len(), sent - 1, "{name}");
        }
    }

    /// Identities reach the validator raw: a padded id is rejected (never
    /// silently trimmed open), the rejection echoes the exact raw id so the
    /// provider's pending control request matches, and a whitespace twin of a
    /// live id never merges with it.
    #[tokio::test]
    async fn raw_identities_are_validated_and_echoed_without_normalization() {
        let mut policy = policy(ProviderId::ClaudeCode);
        policy.questions = crate::types::QuestionPolicy::Enabled;
        policy.tool_approval = crate::types::ToolApproval::Prompt;
        let input = input(ProviderId::ClaudeCode, policy.clone());
        let resolved = resolve_policy(&policy).unwrap();
        let mut mapper = Transport::ClaudeStream(ClaudeFrameMapper::new());
        let mut state = TurnState {
            in_turn: true,
            ..TurnState::default()
        };
        let mut pending = HashMap::new();
        let mut provider_input = ProviderInput::default();

        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &can_use_tool(" padded "),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(pending.is_empty(), "a padded id must not open trimmed");
        assert!(state.provider_error.is_some());
        let rejection = queued_control_response(&provider_input);
        assert_eq!(
            rejection["response"]["request_id"], " padded ",
            "the rejection must echo the exact raw id"
        );

        let mut state = TurnState {
            in_turn: true,
            ..TurnState::default()
        };
        let mut provider_input = ProviderInput::default();
        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &can_use_tool("req-7"),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(pending.contains_key("req-7"));
        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &can_use_tool(" req-7 "),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert_eq!(
            pending.len(),
            1,
            "a whitespace twin must not merge with the live id"
        );
        assert!(pending.contains_key("req-7"));
        let rejection = queued_control_response(&provider_input);
        assert_eq!(rejection["response"]["request_id"], " req-7 ");
        handle_provider_line(
            &input,
            &resolved,
            &mut mapper,
            &mut state,
            &mut pending,
            &mut provider_input,
            &can_use_tool("req-8"),
            &AttemptProgress::default(),
            &mut None,
        )
        .await
        .unwrap();
        assert!(
            pending.contains_key("req-8"),
            "a valid request after a rejection keeps its guarantees"
        );
    }

    #[test]
    fn tool_events_carry_a_bounded_inline_copy_beside_their_references() {
        let input = json!({"command": "just dev-up", "run_in_background": true});
        let started = tool_started_payload("toolu_1", "Bash", &input, &[]);
        assert_eq!(started["name"], "Bash");
        assert_eq!(started["input"], input);
        assert_eq!(started["input_truncated"], false);
        assert_eq!(started["input_ref"], json_ref(&input));
        assert_eq!(started["content"], "Used Bash");

        let output = json!([
            {"type": "text", "text": "Command running in background"},
            {"type": "image", "source": {"data": "..."}}
        ]);
        let completed = tool_completed_payload("toolu_1", "Bash", &output, false, &[]);
        assert_eq!(completed["name"], "Bash");
        assert_eq!(completed["status"], "ok");
        assert_eq!(
            completed["output"],
            "Command running in background\n[image]"
        );
        assert_eq!(completed["content"], completed["output"]);
        assert_eq!(completed["output_truncated"], false);
        assert_eq!(completed["output_ref"], json_ref(&output));

        let failed = tool_completed_payload("toolu_2", "Bash", &json!("boom"), true, &[]);
        assert_eq!(failed["status"], "error");
        assert_eq!(failed["output"], "boom");
    }

    #[test]
    fn event_lines_replace_nul_that_tool_output_carries() {
        let output = json!("\u{7f}ELF\u{2}\u{1}\u{1}\0\0\0");
        let line = event_line(
            "tool.completed",
            tool_completed_payload("toolu_1", "Bash", &output, false, &[]),
        )
        .unwrap();
        assert!(!line[..line.len() - 1].contains(&b'\n'));
        let event: Value = serde_json::from_slice(&line).unwrap();
        assert_eq!(event["kind"], "tool.completed");
        assert_eq!(
            event["payload"]["content"],
            "\u{7f}ELF\u{2}\u{1}\u{1}\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(event["payload"]["output"], event["payload"]["content"]);
        assert_eq!(event["payload"]["output_ref"], json_ref(&output));
    }

    #[test]
    fn inline_tool_copies_are_bounded_while_references_cover_the_full_body() {
        let big = "é".repeat(MAX_INLINE_TOOL_BYTES);
        let (text, truncated) = inline_text(&big);
        assert!(truncated);
        assert!(text.len() <= MAX_INLINE_TOOL_BYTES);
        assert!(text.ends_with(INLINE_TRUNCATION_MARKER));

        let input = json!({"content": big});
        let started = tool_started_payload("toolu_3", "Write", &input, &[]);
        assert_eq!(started["input_truncated"], true);
        let content = started["input"]["content"].as_str().unwrap();
        assert!(content.ends_with(INLINE_TRUNCATION_MARKER));
        assert!(serde_json::to_vec(&started["input"]).unwrap().len() <= MAX_INLINE_TOOL_BYTES);
        assert_eq!(started["input_ref"], json_ref(&input));

        let small = json!({"path": "a.txt"});
        assert_eq!(inline_json(&small), (small.clone(), false));
    }

    #[test]
    fn oversized_shell_input_keeps_its_command_renderable() {
        let input = json!({
            "command": "just dev-up",
            "description": "d".repeat(MAX_INLINE_TOOL_BYTES * 2),
            "run_in_background": true
        });
        let started = tool_started_payload("toolu_4", "Bash", &input, &[]);
        assert_eq!(started["input_truncated"], true);
        assert_eq!(started["input"]["command"], "just dev-up");
        assert_eq!(started["input"]["run_in_background"], true);
        assert!(started["input"]["description"]
            .as_str()
            .unwrap()
            .ends_with(INLINE_TRUNCATION_MARKER));
        assert!(serde_json::to_vec(&started["input"]).unwrap().len() <= MAX_INLINE_TOOL_BYTES);
    }

    #[test]
    fn inline_copies_are_redacted_before_flattening_and_bounding() {
        let secret = "sk-ant-\"quoted\"\\slash\nline".to_string();
        let secrets = sorted_secrets([secret.clone(), "short".to_string()]);
        // A secret with quotes, a backslash and a newline is masked as a
        // string leaf, before the output is flattened to text; the digest
        // still covers the original body.
        let output = json!([{"type": "text", "text": format!("token {secret} short tail")}]);
        let completed = tool_completed_payload("toolu_5", "Read", &output, false, &secrets);
        assert_eq!(completed["output"], "token [REDACTED] [REDACTED] tail");
        assert_eq!(completed["content"], completed["output"]);
        assert_eq!(completed["output_ref"], json_ref(&output));
        // A secret straddling the bound is masked whole before the cut, so
        // no prefix of it survives in the truncated copy, wherever the cut
        // lands relative to the mask.
        for lead in [8usize, 4096, 8000] {
            let filler = "a".repeat(MAX_INLINE_TOOL_BYTES - INLINE_TRUNCATION_MARKER.len() - lead);
            let input = json!({"content": format!("{filler}{secret}{}", "z".repeat(8192))});
            let started = tool_started_payload("toolu_6", "Write", &input, &secrets);
            assert_eq!(started["input_truncated"], true, "lead {lead}");
            let content = started["input"]["content"].as_str().unwrap();
            assert!(content.ends_with(INLINE_TRUNCATION_MARKER), "lead {lead}");
            assert!(!content.contains("sk-ant"), "lead {lead}");
            assert!(!content.contains("quoted"), "lead {lead}");
            assert_eq!(started["input_ref"], json_ref(&input));
            if lead >= 4096 {
                assert!(content.contains(REDACTION_MASK), "lead {lead}");
            }
        }
        // Keys are module-supplied text too.
        let keyed = json!({secret.clone(): "v"});
        assert_eq!(redact_json(&keyed, &secrets), json!({"[REDACTED]": "v"}));
    }

    #[test]
    fn host_fake_selection_reaches_only_the_matching_provider() {
        let claude = input(ProviderId::ClaudeCode, policy(ProviderId::ClaudeCode));
        let codex = input(ProviderId::Codex, policy(ProviderId::Codex));
        let claude_env = |key: &str| (key == "G5_CLAUDE_CODE_FAKE").then(|| "1".to_string());
        assert!(fake_provider_enabled_with(&claude, claude_env));
        assert!(!fake_provider_enabled_with(&codex, claude_env));

        let mut explicit = codex;
        explicit
            .provider
            .options
            .insert("fake_provider".into(), Value::Bool(true));
        assert!(fake_provider_enabled_with(&explicit, |_| None));
    }

    #[test]
    fn deferred_stop_save_only_applies_to_the_current_fake_forced_turn() {
        let mut turn = input(ProviderId::Codex, policy(ProviderId::Codex));
        turn.prompt.text = format!("save later {FAKE_DEFER_STOP_SAVE}");
        assert!(fake_defer_stop_save(&turn, true, true));
        assert!(!fake_defer_stop_save(&turn, false, true));
        assert!(!fake_defer_stop_save(&turn, true, false));
        turn.prompt.text =
            format!("Earlier: {FAKE_DEFER_STOP_SAVE}\nCurrent input follows:\ncontinue normally");
        assert!(!fake_defer_stop_save(&turn, true, true));
    }

    #[test]
    fn pending_completion_rebuilds_only_on_definitive_failures() {
        assert!(pending_completion_is_definitive(
            &G5Error::SessionFileIntegrity {
                message: "unsafe local path".into(),
            }
        ));
        assert!(pending_completion_is_definitive(&G5Error::Conflict {
            reason: "stale generation".into(),
        }));
        assert!(!pending_completion_is_definitive(
            &G5Error::ServiceUnavailable {
                message: "temporarily offline".into(),
            }
        ));
        assert!(!pending_completion_is_definitive(&G5Error::Internal {
            message: "API error 500: temporarily offline".into(),
        }));
        assert!(!pending_completion_is_definitive(&G5Error::Unauthorized));
    }

    #[test]
    fn reconstructed_fact_reaches_both_fresh_provider_transports() {
        for provider in [ProviderId::ClaudeCode, ProviderId::Codex] {
            let mut turn = input(provider, policy(provider));
            turn.prompt.text = "Prior delivered conversation:\n{\"role\":\"user\",\"content\":\"The code is RAVEN-42\"}\nCurrent input follows:\nWhat is the code?".into();
            turn.prompt.sha256 = format!("{:x}", Sha256::digest(turn.prompt.text.as_bytes()));
            turn.recovery = Some(crate::types::RecoveryContextV1 {
                included_messages: 1,
                omitted_messages: 0,
                truncated_messages: 0,
                attachment_references: 0,
                omitted_attachment_references: 0,
                unavailable_reason: None,
            });
            let resolved = resolve_policy(&turn.provider.policy_snapshot).unwrap();
            let temp = tempfile::tempdir().unwrap();
            let (_, argv, bytes, transport) = launch_plan(
                &turn,
                &resolved,
                LaunchFiles {
                    cwd: temp.path(),
                    home: temp.path(),
                    attachments: &[],
                    memory: &crate::memory::Binding::Disabled,
                },
                None,
                "fresh-session",
                true,
                None,
            )
            .unwrap();
            if provider == ProviderId::Codex {
                let (thread, turn) = codex_initial_requests(transport, "model");
                assert_eq!(thread["method"], "thread/start");
                assert!(turn["params"]["input"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("RAVEN-42"));
            } else {
                assert!(String::from_utf8(bytes).unwrap().contains("RAVEN-42"));
                assert!(!argv.iter().any(|arg| arg == "--resume"));
            }
        }
    }

    #[test]
    fn verified_resume_sends_only_current_input_to_both_native_transports() {
        for provider in [ProviderId::ClaudeCode, ProviderId::Codex] {
            let mut turn = input(provider, policy(provider));
            turn.prompt.text = "Prior delivered conversation: historical-only secret ORCHID-73\nCurrent input follows: continue".into();
            turn.prompt.sha256 = format!("{:x}", Sha256::digest(turn.prompt.text.as_bytes()));
            let current = "Continue using the earlier tool result".to_string();
            turn.resume_prompt = Some(FrozenPromptV1 {
                sha256: format!("{:x}", Sha256::digest(current.as_bytes())),
                text: current.clone(),
            });
            let resolved = resolve_policy(&turn.provider.policy_snapshot).unwrap();
            let temp = tempfile::tempdir().unwrap();
            let (_, argv, bytes, transport) = launch_plan(
                &turn,
                &resolved,
                LaunchFiles {
                    cwd: temp.path(),
                    home: temp.path(),
                    attachments: &[],
                    memory: &crate::memory::Binding::Disabled,
                },
                Some("native-session"),
                "fresh-session",
                true,
                None,
            )
            .unwrap();
            let sent = match provider {
                ProviderId::ClaudeCode => {
                    assert!(argv
                        .windows(2)
                        .any(|pair| pair == ["--resume", "native-session"]));
                    String::from_utf8(bytes).unwrap()
                }
                ProviderId::Codex => {
                    let (thread, turn) = codex_initial_requests(transport, "model");
                    assert_eq!(thread["method"], "thread/resume");
                    assert_eq!(thread["params"]["threadId"], "native-session");
                    turn.to_string()
                }
            };
            assert!(sent.contains(&current));
            assert!(!sent.contains("ORCHID-73"));
        }
    }

    #[test]
    fn claude_native_policy_keeps_stdin_protocol_and_removes_custom_prompt_tool() {
        let mut policy = policy(ProviderId::ClaudeCode);
        policy.questions = crate::types::QuestionPolicy::Enabled;
        policy.tool_approval = crate::types::ToolApproval::Prompt;
        let input = input(ProviderId::ClaudeCode, policy.clone());
        let resolved = resolve_policy(&policy).unwrap();
        let (_program, argv, bytes, _) = launch_plan(
            &input,
            &resolved,
            LaunchFiles {
                cwd: Path::new("/work"),
                home: Path::new("/home/agent"),
                attachments: &[],
                memory: &crate::memory::Binding::Disabled,
            },
            None,
            "session-id",
            false,
            Some(r#"{"autoMemoryEnabled":false}"#),
        )
        .unwrap();
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--permission-prompt-tool", "stdio"]));
        assert!(argv.windows(2).any(|pair| pair == ["--max-turns", "10"]));
        assert!(!argv.iter().any(|arg| arg == "AskUserQuestion"));
        assert!(bytes.ends_with(b"\n"));
        // The harness is read from the native home, without an added directory
        // or a platform Claude MCP registration.
        assert!(!argv.iter().any(|arg| arg == "--add-dir"));
        assert!(!argv.iter().any(|arg| arg == "--mcp-config"));
        assert!(argv.iter().any(|arg| arg == "--strict-mcp-config"));
    }

    #[test]
    fn claude_without_effort_control_omits_the_native_effort_option() {
        let root = tempfile::tempdir().unwrap();
        let policy = policy(ProviderId::ClaudeCode);
        for model in ["claude-haiku-4-5", "anthropic/claude-haiku-4.5"] {
            let mut turn = input(ProviderId::ClaudeCode, policy.clone());
            turn.provider.model_id = model.into();
            turn.provider.options.insert("effort".into(), json!("none"));
            let (_, argv, _, _) = launch_plan(
                &turn,
                &resolve_policy(&policy).unwrap(),
                LaunchFiles {
                    cwd: root.path(),
                    home: root.path(),
                    attachments: &[],
                    memory: &crate::memory::Binding::Disabled,
                },
                None,
                "session-id",
                false,
                None,
            )
            .unwrap();
            assert!(!argv.iter().any(|arg| arg == "--effort"));
            assert!(argv.windows(2).any(|pair| pair == ["--model", model]));
        }
    }

    #[test]
    fn claude_router_capabilities_preserve_frozen_model_effort_and_caller_settings() {
        for from_memory in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let settings = r#"{"autoMemoryEnabled":false,"env":{"G5_TEST":"preserved"},"modelOverrides":{"claude-opus-4-8":"stale-model","workspace-alias":"custom/retained-model"}}"#;
            if !from_memory {
                std::fs::create_dir(root.path().join(".claude")).unwrap();
                std::fs::write(root.path().join(".claude/settings.json"), settings).unwrap();
            }
            let policy = policy(ProviderId::ClaudeCode);
            let mut turn = input(ProviderId::ClaudeCode, policy.clone());
            turn.provider.model_id = "anthropic/claude-opus-4.8".into();
            turn.provider.options.insert("effort".into(), json!("low"));
            let (_, argv, _, _) = launch_plan(
                &turn,
                &resolve_policy(&policy).unwrap(),
                LaunchFiles {
                    cwd: root.path(),
                    home: root.path(),
                    attachments: &[],
                    memory: &crate::memory::Binding::Disabled,
                },
                None,
                "session-id",
                false,
                from_memory.then_some(settings),
            )
            .unwrap();
            let settings: Value = serde_json::from_str(
                &argv
                    .windows(2)
                    .find(|pair| pair[0] == "--settings")
                    .unwrap()[1],
            )
            .unwrap();
            assert_eq!(settings["autoMemoryEnabled"], false);
            assert_eq!(settings["env"]["G5_TEST"], "preserved");
            assert_eq!(
                settings["modelOverrides"]["claude-opus-4-8"],
                turn.provider.model_id
            );
            assert_eq!(
                settings["modelOverrides"]["workspace-alias"],
                "custom/retained-model"
            );
            assert!(argv
                .windows(2)
                .any(|pair| pair == ["--model", "anthropic/claude-opus-4.8"]));
            assert!(argv.windows(2).any(|pair| pair == ["--effort", "low"]));
        }
        assert!(
            g5_contracts::claude_models::claude_native_model_overrides("custom/model").is_none()
        );
        assert!(
            g5_contracts::claude_models::claude_native_model_overrides("claude-opus-4-8[1m]")
                .is_none()
        );
    }

    #[test]
    fn claude_capability_merge_rejects_malformed_settings_without_changing_custom_pins() {
        for mut settings in [Value::Null, json!([]), json!({"modelOverrides": "invalid"})] {
            assert!(crate::runner::claude::apply_model_overrides(
                "anthropic/claude-opus-4.8",
                &mut settings
            )
            .is_err());
        }
        let mut custom = json!({"modelOverrides":{"workspace-alias":"custom/retained-model"}});
        let original = custom.clone();
        crate::runner::claude::apply_model_overrides("custom/unknown", &mut custom).unwrap();
        assert_eq!(custom, original);
    }

    #[test]
    fn claude_bypass_launch_preserves_frozen_question_policy() {
        for questions in [
            crate::types::QuestionPolicy::Enabled,
            crate::types::QuestionPolicy::Disabled,
        ] {
            let mut policy = policy(ProviderId::ClaudeCode);
            policy.questions = questions;
            policy.tool_approval = crate::types::ToolApproval::AutoAllow;
            let enabled = questions == crate::types::QuestionPolicy::Enabled;
            for resume in [None, Some("native-session")] {
                let mut turn = input(ProviderId::ClaudeCode, policy.clone());
                turn.resume_prompt = Some(turn.prompt.clone());
                let (_, argv, _, _) = launch_plan(
                    &turn,
                    &resolve_policy(&policy).unwrap(),
                    LaunchFiles {
                        cwd: Path::new("/work"),
                        home: Path::new("/home/agent"),
                        attachments: &[],
                        memory: &crate::memory::Binding::Disabled,
                    },
                    resume,
                    "session-id",
                    false,
                    None,
                )
                .unwrap();
                assert!(argv
                    .windows(2)
                    .any(|args| args == ["--permission-mode", "bypassPermissions"]));
                assert!(argv.windows(2).any(|args| args
                    == [
                        "--permission-prompts",
                        if enabled { "host" } else { "none" }
                    ]));
                assert_eq!(
                    argv.windows(2)
                        .any(|args| args == ["--permission-prompt-tool", "stdio"]),
                    enabled
                );
                assert_eq!(
                    argv.windows(2)
                        .any(|args| args == ["--disallowedTools", "AskUserQuestion"]),
                    !enabled
                );
            }
        }
    }

    #[test]
    fn claude_loads_user_instructions_without_an_added_directory() {
        let policy = policy(ProviderId::ClaudeCode);
        let input = input(ProviderId::ClaudeCode, policy.clone());
        let resolved = resolve_policy(&policy).unwrap();
        let (_program, argv, _bytes, _) = launch_plan(
            &input,
            &resolved,
            LaunchFiles {
                cwd: Path::new("/work"),
                home: Path::new("/home/agent"),
                attachments: &[],
                memory: &crate::memory::Binding::Disabled,
            },
            None,
            "session-id",
            false,
            None,
        )
        .unwrap();
        assert!(!argv.iter().any(|arg| arg == "--add-dir"));
        assert!(argv
            .windows(2)
            .any(|args| args == ["--setting-sources", "user,project"]));
    }

    #[test]
    fn explicit_claude_tools_preserve_approval_and_exclude_mcp() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(".mcp.json"), "{}").unwrap();
        for approval in [
            crate::types::ToolApproval::Prompt,
            crate::types::ToolApproval::AutoDeny,
            crate::types::ToolApproval::AutoAllow,
        ] {
            let mut policy = policy(ProviderId::ClaudeCode);
            policy.tool_approval = approval;
            policy.allowed_tools = vec!["Read".into()];
            let input = input(ProviderId::ClaudeCode, policy.clone());
            let resolved = resolve_policy(&policy).unwrap();
            let (_, args, _, _) = launch_plan(
                &input,
                &resolved,
                LaunchFiles {
                    cwd: temp.path(),
                    home: temp.path(),
                    attachments: &[],
                    memory: &crate::memory::Binding::Disabled,
                },
                None,
                "session",
                false,
                None,
            )
            .unwrap();
            assert!(args.windows(2).any(|pair| pair == ["--tools", "Read"]));
            assert!(args
                .windows(2)
                .any(|pair| pair == ["--permission-mode", resolved.permission_mode]));
            assert!(!args
                .iter()
                .any(|arg| arg == "--allowedTools" || arg == "--mcp-config"));
            assert!(args.iter().any(|arg| arg == "--strict-mcp-config"));
        }
        let policy = policy(ProviderId::ClaudeCode);
        let input = input(ProviderId::ClaudeCode, policy.clone());
        let (_, args, _, _) = launch_plan(
            &input,
            &resolve_policy(&policy).unwrap(),
            LaunchFiles {
                cwd: temp.path(),
                home: temp.path(),
                attachments: &[],
                memory: &crate::memory::Binding::Disabled,
            },
            None,
            "session",
            false,
            None,
        )
        .unwrap();
        assert!(args.iter().any(|arg| arg == "--mcp-config"));
    }

    #[test]
    fn formal_launch_preserves_pin_opt_outs_model_mapping_and_absent_effort() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join(".g5")).unwrap();
        std::fs::write(temp.path().join(".g5/review-system.txt"), "review only").unwrap();
        for provider in [ProviderId::ClaudeCode, ProviderId::Codex] {
            let mut turn = input(provider, policy(provider));
            turn.provider.model_id = if provider == ProviderId::ClaudeCode {
                "anthropic/claude-opus-4.8"
            } else {
                "gpt-5.6-sol"
            }
            .into();
            for effort in if provider == ProviderId::ClaudeCode {
                vec!["none", "high"]
            } else {
                vec!["high"]
            } {
                turn.provider.options.insert("effort".into(), json!(effort));
                let (_, args, bytes) =
                    crate::formal_review::launch(&turn, temp.path(), false).unwrap();
                if provider == ProviderId::Codex {
                    assert_eq!(
                        &args[..4],
                        ["app-server", "--listen", "stdio://", "--strict-config"]
                    );
                    assert!(!args.iter().any(|arg| arg == "exec" || arg == "--json"));
                    // Effort travels on the turn, not as a launch override.
                    assert!(!args
                        .iter()
                        .any(|arg| arg.starts_with("model_reasoning_effort")));
                    assert_eq!(
                        serde_json::from_slice::<Value>(&bytes).unwrap(),
                        codex_app_server::initialize()
                    );
                    let adapter = crate::formal_review::codex_app_server_adapter(
                        &turn,
                        Path::new("/tmp/launch"),
                    );
                    let (thread, request) = codex_initial_requests(
                        Transport::CodexAppServer(adapter),
                        &turn.provider.model_id,
                    );
                    assert_eq!(thread["method"], "thread/start");
                    assert_eq!(thread["params"]["ephemeral"], true);
                    assert!(thread["params"].get("sandbox").is_none());
                    assert_eq!(request["method"], "turn/start");
                    assert_eq!(request["params"]["input"][0]["text"], turn.prompt.text);
                    assert_eq!(request["params"]["effort"], effort);
                    for pair in codex_app_server::feature_configuration().chunks_exact(2) {
                        assert!(args.windows(2).any(|actual| actual == pair), "{pair:?}");
                    }
                    for setting in [
                        "features.memories=false",
                        "memories.use_memories=false",
                        "memories.generate_memories=false",
                    ] {
                        assert!(args.iter().any(|arg| arg == setting));
                    }
                } else {
                    assert_eq!(args.iter().any(|arg| arg == "--effort"), effort != "none");
                    if effort != "none" {
                        assert!(args.windows(2).any(|pair| pair == ["--effort", effort]));
                    }
                    let settings: Value = serde_json::from_str(
                        &args[args.iter().position(|arg| arg == "--settings").unwrap() + 1],
                    )
                    .unwrap();
                    assert_eq!(settings["disableAllHooks"], true);
                    assert_eq!(
                        settings["modelOverrides"],
                        g5_contracts::claude_models::claude_native_model_overrides(
                            &turn.provider.model_id
                        )
                        .unwrap()
                    );
                }
            }
        }
    }

    #[test]
    fn app_server_launch_uses_only_stable_handshake_and_frozen_native_resume_input() {
        let policy = policy(ProviderId::Codex);
        let mut input = input(ProviderId::Codex, policy.clone());
        input.provider.auth_mode = Some(g5_contracts::types::AgentAuthModeV1::ChatGptSubscription);
        input
            .provider
            .options
            .insert("codex_transport".into(), json!(codex_app_server::TRANSPORT));
        input
            .provider
            .options
            .insert("effort".into(), json!("high"));
        input.resume_prompt = Some(crate::types::FrozenPromptV1 {
            text: "only current input".into(),
            sha256: "frozen".into(),
        });
        let resolved = resolve_policy(&policy).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let (_, args, bytes, transport) = launch_plan(
            &input,
            &resolved,
            LaunchFiles {
                cwd: Path::new("/work"),
                home: temp.path(),
                attachments: &[],
                memory: &crate::memory::Binding::Disabled,
            },
            Some("thread-id"),
            "session-id",
            false,
            None,
        )
        .unwrap();
        assert_eq!(&args[..3], ["app-server", "--listen", "stdio://"]);
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-c", "cli_auth_credentials_store=\"file\""]));
        assert!(!args.iter().any(|arg| arg == "exec" || arg == "--json"));
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            codex_app_server::initialize()
        );
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("only current input"));
        let Transport::CodexAppServer(mut adapter) = transport else {
            panic!("App Server launch must own its native adapter");
        };
        let next = adapter.receive(json!({"id":"g5-initialize","result":{"userAgent":format!("g5_agent/{} (Linux)",codex_app_server::VERSION.trim())}}));
        assert_eq!(next.writes[1]["method"], "thread/resume");
        assert_eq!(next.writes[1]["params"]["threadId"], "thread-id");
        let turn =
            adapter.receive(json!({"id":"g5-thread","result":{"thread":{"id":"thread-id"},"model":input.provider.model_id,"approvalPolicy":"never","sandbox":{"type":"readOnly"}}}));
        assert_eq!(
            turn.writes[0]["params"]["input"][0]["text"],
            "only current input"
        );
        assert_eq!(turn.writes[0]["params"]["effort"], "high");
        assert_eq!(turn.writes[0]["params"]["model"], input.provider.model_id);
    }

    #[test]
    fn app_server_refuses_configuration_feature_and_experimental_api_overrides() {
        let mut input = input(ProviderId::Codex, policy(ProviderId::Codex));
        validate_codex_transport(&input).unwrap();
        // A historical exec-contract attempt is refused, never converted.
        input.provider.options.remove("codex_transport");
        assert!(validate_codex_transport(&input).is_err());
        let mut claude = self::input(ProviderId::ClaudeCode, policy(ProviderId::ClaudeCode));
        validate_codex_transport(&claude).unwrap();
        claude
            .provider
            .options
            .insert("codex_transport".into(), json!(codex_app_server::TRANSPORT));
        assert!(validate_codex_transport(&claude).is_err());
        input
            .provider
            .options
            .insert("codex_transport".into(), json!(codex_app_server::TRANSPORT));
        for key in [
            "experimentalApi",
            "experimental_api",
            "capabilities",
            "features",
            "config",
            "listen",
            "argv",
        ] {
            input.provider.options.insert(key.into(), json!(true));
            assert!(validate_codex_transport(&input).is_err(), "{key}");
            input.provider.options.remove(key);
        }
        input
            .provider
            .options
            .insert("codex_transport".into(), json!("future-transport"));
        assert!(validate_codex_transport(&input).is_err());
    }

    #[test]
    fn codex_launch_is_app_server_with_never_approval_and_pin_opt_outs() {
        let policy = policy(ProviderId::Codex);
        let mut input = input(ProviderId::Codex, policy.clone());
        input.resume_prompt = Some(input.prompt.clone());
        let resolved = resolve_policy(&policy).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let (_program, argv, bytes, _) = launch_plan(
            &input,
            &resolved,
            LaunchFiles {
                cwd: Path::new("/work"),
                home: temp.path(),
                attachments: &[],
                memory: &crate::memory::Binding::Disabled,
            },
            Some("thread-id"),
            "session-id",
            false,
            None,
        )
        .unwrap();
        assert_eq!(&argv[..3], ["app-server", "--listen", "stdio://"]);
        assert!(!argv.iter().any(|arg| arg == "exec" || arg == "resume"));
        assert!(argv.iter().any(|arg| arg == "approval_policy=\"never\""));
        assert!(argv.iter().any(|arg| arg == "features.memories=false"));
        assert!(argv
            .iter()
            .any(|arg| arg == "memories.generate_memories=false"));
        assert!(argv.iter().any(|arg| arg == "memories.use_memories=false"));
        assert!(argv
            .iter()
            .any(|arg| arg == "features.instant_interrupt=false"));
        assert!(argv
            .iter()
            .any(|arg| arg == "features.daemon_auto_start=false"));
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            codex_app_server::initialize()
        );
    }

    #[test]
    fn codex_new_and_resumed_launches_use_bound_memory() {
        let temp = tempfile::tempdir().unwrap();
        let binding = crate::memory::Binding::Bound(temp.path().join("personal/memory/codex"));
        {
            for resume in [None, Some("thread-id")] {
                for sandbox in ["read-only", "workspace-write", "danger-full-access"] {
                    let mut policy = resolve_policy(&policy(ProviderId::Codex)).unwrap();
                    policy.sandbox = Some(sandbox);
                    let mut input = input(ProviderId::Codex, self::policy(ProviderId::Codex));
                    input.resume_prompt = Some(input.prompt.clone());
                    let (_, args, _, _) = launch_plan(
                        &input,
                        &policy,
                        LaunchFiles {
                            cwd: temp.path(),
                            home: temp.path(),
                            attachments: &[],
                            memory: &binding,
                        },
                        resume,
                        "session",
                        false,
                        None,
                    )
                    .unwrap();
                    assert_eq!(args[0], "app-server");
                    assert!(args.iter().any(|arg| arg == "features.memories=true"));
                    assert!(args.iter().any(|arg| arg == "memories.use_memories=true"));
                    assert!(args.iter().any(|arg| arg
                        == &format!("memories.generate_memories={}", sandbox != "read-only")));
                    assert!(!args.iter().any(|arg| arg == "features.memories=false"));
                    let roots = args
                        .iter()
                        .find(|arg| arg.starts_with("sandbox_workspace_write.writable_roots="));
                    assert_eq!(roots.is_some(), sandbox != "read-only");
                    if let Some(roots) = roots {
                        assert!(roots.contains("/personal/memory/codex"));
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn active_launch_materializes_exact_image_for_both_providers() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let api = MockServer::start().await;
        let id = uuid::Uuid::new_v4().to_string();
        let session_id = uuid::Uuid::new_v4().to_string();
        let input_id = uuid::Uuid::new_v4().to_string();
        let image = &base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
            .unwrap();
        let digest = format!("{:x}", Sha256::digest(image));
        Mock::given(method("GET"))
            .and(path(format!("/api/internal/chat-attachments/{id}/content")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "attachmentId": id,
                "sessionId": session_id,
                "inputId": input_id,
                "fileName": "image.png",
                "contentType": "IMAGE/PNG",
                "sizeBytes": image.len(),
                "sha256": digest,
                "contentBase64": base64::engine::general_purpose::STANDARD.encode(image),
            })))
            .expect(2)
            .mount(&api)
            .await;
        let client = InternalApiClient::new(&api.uri());
        for provider in [ProviderId::ClaudeCode, ProviderId::Codex] {
            let mut turn = input(provider, policy(provider));
            turn.task.session_id = session_id.clone();
            turn.task.initial_input.input_id = input_id.clone();
            turn.attachments = vec![crate::types::TurnAttachmentV1 {
                kind: crate::types::TurnAttachmentKindV1::Image,
                id: id.clone(),
                input_id: input_id.clone(),
                file_name: "image.png".into(),
                content_type: Some("IMAGE/PNG".into()),
                size_bytes: image.len() as i64,
                sha256: digest.clone(),
            }];
            turn.validate().unwrap();
            let home = tempfile::tempdir().unwrap();
            let (_dir, attachments) = materialize_attachments(&turn, Some(&client), home.path())
                .await
                .unwrap();
            assert_eq!(
                std::fs::read(&attachments[0].path).unwrap(),
                image.as_slice()
            );
            let resolved = resolve_policy(&turn.provider.policy_snapshot).unwrap();
            let (_, _argv, stdin, transport) = launch_plan(
                &turn,
                &resolved,
                LaunchFiles {
                    cwd: home.path(),
                    home: home.path(),
                    attachments: &attachments,
                    memory: &crate::memory::Binding::Disabled,
                },
                None,
                "fresh",
                true,
                None,
            )
            .unwrap();
            match provider {
                ProviderId::ClaudeCode => {
                    let frame: Value = serde_json::from_slice(&stdin).unwrap();
                    assert_eq!(
                        frame["message"]["content"][0]["source"]["data"],
                        base64::engine::general_purpose::STANDARD.encode(image)
                    );
                    assert_eq!(
                        frame["message"]["content"][0]["source"]["media_type"],
                        "image/png"
                    );
                    assert_eq!(frame["message"]["content"][1]["text"], turn.prompt.text);
                }
                ProviderId::Codex => {
                    let (_, request) = codex_initial_requests(transport, "model");
                    assert_eq!(
                        request["params"]["input"],
                        json!([
                            {"type":"text","text":"Do the work"},
                            {"type":"localImage","path":attachments[0].path},
                        ])
                    );
                    assert_eq!(
                        serde_json::from_slice::<Value>(&stdin).unwrap(),
                        codex_app_server::initialize()
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn changed_attachment_bytes_fail_before_provider_launch() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let api = MockServer::start().await;
        let mut turn = input(ProviderId::ClaudeCode, policy(ProviderId::ClaudeCode));
        let id = uuid::Uuid::new_v4().to_string();
        let expected = b"original";
        let digest = format!("{:x}", Sha256::digest(expected));
        turn.attachments = vec![crate::types::TurnAttachmentV1 {
            kind: crate::types::TurnAttachmentKindV1::Image,
            id: id.clone(),
            input_id: turn.task.initial_input.input_id.clone(),
            file_name: "image.png".into(),
            content_type: Some("image/png".into()),
            size_bytes: expected.len() as i64,
            sha256: digest.clone(),
        }];
        Mock::given(method("GET"))
            .and(path(format!("/api/internal/chat-attachments/{id}/content")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "attachmentId": id,
                "sessionId": turn.task.session_id,
                "inputId": turn.task.initial_input.input_id,
                "fileName": "image.png",
                "contentType": "image/png",
                "sizeBytes": expected.len(),
                "sha256": digest,
                "contentBase64": base64::engine::general_purpose::STANDARD.encode(b"tampered"),
            })))
            .mount(&api)
            .await;
        let client = InternalApiClient::new(&api.uri());
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            materialize_attachments(&turn, Some(&client), home.path())
                .await
                .err(),
            Some("attachment_corrupt")
        );
    }

    #[tokio::test]
    async fn invalid_or_mislabeled_image_fails_before_provider_launch() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let valid_png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
            .unwrap();
        for (bytes, mime) in [
            (b"not an image".to_vec(), "image/png"),
            (valid_png, "image/jpeg"),
        ] {
            let api = MockServer::start().await;
            let mut turn = input(ProviderId::ClaudeCode, policy(ProviderId::ClaudeCode));
            let id = uuid::Uuid::new_v4().to_string();
            let digest = format!("{:x}", Sha256::digest(&bytes));
            turn.attachments = vec![crate::types::TurnAttachmentV1 {
                kind: crate::types::TurnAttachmentKindV1::Image,
                id: id.clone(),
                input_id: turn.task.initial_input.input_id.clone(),
                file_name: "image.png".into(),
                content_type: Some(mime.into()),
                size_bytes: bytes.len() as i64,
                sha256: digest.clone(),
            }];
            Mock::given(method("GET"))
                .and(path(format!("/api/internal/chat-attachments/{id}/content")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "attachmentId": id,
                    "sessionId": turn.task.session_id,
                    "inputId": turn.task.initial_input.input_id,
                    "fileName": "image.png",
                    "contentType": mime,
                    "sizeBytes": bytes.len(),
                    "sha256": digest,
                    "contentBase64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                })))
                .mount(&api)
                .await;
            let client = InternalApiClient::new(&api.uri());
            let home = tempfile::tempdir().unwrap();
            assert_eq!(
                materialize_attachments(&turn, Some(&client), home.path())
                    .await
                    .err(),
                Some("attachment_invalid_image")
            );
        }
    }

    #[tokio::test]
    async fn missing_and_cross_session_attachments_fail_before_provider_launch() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        for cross_session in [false, true] {
            let api = MockServer::start().await;
            let mut turn = input(ProviderId::ClaudeCode, policy(ProviderId::ClaudeCode));
            let id = uuid::Uuid::new_v4().to_string();
            let bytes = b"image";
            let digest = format!("{:x}", Sha256::digest(bytes));
            turn.attachments = vec![crate::types::TurnAttachmentV1 {
                kind: crate::types::TurnAttachmentKindV1::Image,
                id: id.clone(),
                input_id: turn.task.initial_input.input_id.clone(),
                file_name: "image.png".into(),
                content_type: Some("image/png".into()),
                size_bytes: bytes.len() as i64,
                sha256: digest.clone(),
            }];
            let response = if cross_session {
                ResponseTemplate::new(200).set_body_json(json!({
                    "attachmentId": id,
                    "sessionId": "another-session",
                    "inputId": turn.task.initial_input.input_id,
                    "fileName": "image.png",
                    "contentType": "image/png",
                    "sizeBytes": bytes.len(),
                    "sha256": digest,
                    "contentBase64": base64::engine::general_purpose::STANDARD.encode(bytes),
                }))
            } else {
                ResponseTemplate::new(404)
            };
            Mock::given(method("GET"))
                .and(path(format!("/api/internal/chat-attachments/{id}/content")))
                .respond_with(response)
                .mount(&api)
                .await;
            let client = InternalApiClient::new(&api.uri());
            let home = tempfile::tempdir().unwrap();
            let expected = if cross_session {
                "attachment_identity_mismatch"
            } else {
                "attachment_unavailable"
            };
            assert_eq!(
                materialize_attachments(&turn, Some(&client), home.path())
                    .await
                    .err(),
                Some(expected)
            );
        }
    }
}
