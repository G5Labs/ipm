//! Durable logical-task state owned by the API.
//!
//! A module process reports facts through the dispatcher journal.  This file
//! is the only production path that projects those facts onto a task carrying
//! a workspace VM identity.  Dispatcher acknowledgements happen after the
//! caller commits the [`ScopedDb`] transaction containing [`TaskService::apply`].

use chrono::{DateTime, Utc};
use g5_api_foundation::contracts::preview_profiles::{
    PreviewApplicationDTO, PreviewApplicationSource,
};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::utils::{replace_nul, replace_nul_in_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::services::hosts::workspace_vm_release::{AgentWorkSettlement, ReleaseSettlement};
use crate::services::workspace::preview_config::{parse_preview_config, preview_application};
use crate::services::workspace::preview_run::{
    preview_close, preview_closing, preview_spec_port, preview_task_name, project_preview_run,
    PreviewEnding, PreviewRunEvents,
};
pub use crate::services::workspace::workspace_runtime::canonical_exec_id;
use crate::services::workspace::workspace_vm_service::{
    InterruptedWork, ReplayRecord, StoppedExecution, StoppedExecutionOutcome, WorkspaceAgentWork,
};

use crate::errors::AppError;
use crate::ws::events::TimelineEntryPayload;
use crate::ws::{Channel, WsEvent};

use crate::services::agents::attempt_snapshot::snapshot_launch_value;
use crate::services::workspace::workspace_runtime::RuntimeRequest;
use crate::services::TaskService;

const MAX_NATIVE_REQUEST_BYTES: usize = 64 * 1024;
const MAX_TIMELINE_CONTENT_BYTES: usize = 64 * 1024;

pub use g5_contracts::types::TaskKind;

#[derive(Debug, Clone, PartialEq)]
pub struct JournalTaskEvent {
    pub task_id: Uuid,
    pub attempt: i32,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub exec_id: String,
    pub seq: u64,
    pub kind: String,
    pub payload: Value,
    pub terminal: bool,
}

impl JournalTaskEvent {
    pub fn validate_identity(&self) -> Result<(), AppError> {
        if self.attempt <= 0 || self.seq == 0 {
            return Err(conflict("invalid task event sequence or attempt"));
        }
        let expected = canonical_exec_id(self.task_id, self.attempt, self.vm_id);
        if self.exec_id != expected {
            return Err(conflict(format!(
                "task event execution identity mismatch: expected {expected}"
            )));
        }
        if !self.payload.is_object() {
            return Err(conflict("task event payload must be an object"));
        }
        Ok(())
    }

    fn replace_nul(&mut self) {
        replace_nul(&mut self.kind);
        replace_nul_in_json(&mut self.payload);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskLifecycleEvent {
    Journal(JournalTaskEvent),
    StartDisposition {
        task_id: Uuid,
        attempt: i32,
        vm_id: Uuid,
        epoch: u64,
        disposition: Value,
    },
    VmLost {
        task_id: Uuid,
        attempt: i32,
        vm_id: Uuid,
        epoch: u64,
        evidence: Value,
    },
    RecoveredOutcome {
        task_id: Uuid,
        attempt: i32,
        vm_id: Uuid,
        epoch: u64,
        outcome: Value,
    },
}

impl TaskLifecycleEvent {
    /// Dispatcher facts carry module output, which may hold U+0000 (a tool
    /// that printed a binary file). PostgreSQL text and jsonb reject it, and a
    /// rejected journal event would be retried on every reconciler pass
    /// without ever reaching the outcome behind it, so every dispatcher value
    /// is stored with U+0000 replaced.
    fn replace_nul(&mut self) {
        match self {
            Self::Journal(event) => event.replace_nul(),
            Self::StartDisposition { disposition, .. } => replace_nul_in_json(disposition),
            Self::VmLost { evidence, .. } => replace_nul_in_json(evidence),
            Self::RecoveredOutcome { outcome, .. } => replace_nul_in_json(outcome),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResult {
    pub duplicate: bool,
    pub ack_through: Option<u64>,
    pub task_status: String,
    pub terminal: bool,
}

/// One journal event with the time this API received it. That time becomes
/// the stored row's `created_at`, so a batch committed at once keeps the
/// timeline's event order and cursors (`g5.chat.history-batches`).
#[derive(Debug, Clone)]
pub struct JournalBatchEvent {
    pub event: JournalTaskEvent,
    pub received_at: DateTime<Utc>,
    /// The assistant segment the live copy was published under, when a
    /// follower assigned one; the stored row keeps that identity rather than
    /// deriving it again from a map that may have moved on.
    pub message_id: Option<Uuid>,
}

/// What one history batch did.
#[derive(Debug, Clone)]
pub struct BatchApplyResult {
    /// Events stored by this batch.
    pub applied: u64,
    /// Events at or below the persisted prefix, ignored.
    pub duplicates: u64,
    /// The persisted contiguous prefix after the batch.
    pub ack_through: Option<u64>,
    pub task_status: String,
    pub terminal: bool,
    /// Updates to publish once the batch has committed: the stored rows of
    /// the batch's boundary kinds (a provider request becomes actionable
    /// only through its stored row), the task's status and the chat events
    /// those kinds produce.
    pub live: Vec<(Channel, WsEvent)>,
}

/// Why a history batch was not stored. The distinction is the caller's
/// recovery: an inconsistent batch is rebuilt from the journal and the
/// persisted prefix, a failed one is retried as it is.
#[derive(Debug)]
pub enum BatchApplyError {
    /// The batch disagrees with the persisted prefix or the attempt's
    /// identity: a sequence gap, a watermark that moved, a row already
    /// stored beyond the watermark, a settled or unknown attempt, mixed
    /// identities, or events after the terminal one.
    Inconsistent(String),
    /// Storage failed for another reason, usually transient.
    Failed(AppError),
}

impl std::fmt::Display for BatchApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inconsistent(reason) => write!(f, "journal batch is inconsistent: {reason}"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

impl From<AppError> for BatchApplyError {
    fn from(error: AppError) -> Self {
        Self::Failed(error)
    }
}

impl From<sqlx::Error> for BatchApplyError {
    fn from(error: sqlx::Error) -> Self {
        Self::Failed(error.into())
    }
}

impl From<BatchApplyError> for AppError {
    fn from(error: BatchApplyError) -> Self {
        match error {
            BatchApplyError::Inconsistent(reason) => conflict(reason),
            BatchApplyError::Failed(error) => error,
        }
    }
}

fn inconsistent(reason: impl Into<String>) -> BatchApplyError {
    BatchApplyError::Inconsistent(reason.into())
}

/// Routine transcript output waits for the batch timer; every other kind is
/// a durable control boundary that flushes the batch at once
/// (`g5.chat.history-batches`).
#[must_use]
pub fn is_routine_stream_event(kind: &str) -> bool {
    kind.starts_with("message.")
        || kind.starts_with("thought.")
        || kind.starts_with("tool.")
        || kind == "notice"
}

/// The stored row id of a journal event, derived from its execution identity
/// so the live copy published before persistence and the saved row are one
/// fact in the browser (`g5.chat.replay`).
#[must_use]
pub fn journal_event_row_id(task_id: Uuid, attempt: i32, vm_id: Uuid, seq: u64) -> Uuid {
    Uuid::new_v5(
        &task_id,
        format!("task-event:{attempt}:{vm_id}:{seq}").as_bytes(),
    )
}

/// The timeline entry a journal event projects to, for the live copy and the
/// stored row alike. `live` marks the copy published before persistence.
#[must_use]
pub fn journal_timeline_entry(
    event: &JournalTaskEvent,
    message_id: Uuid,
    session_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    live: bool,
) -> TimelineEntryPayload {
    let mut metadata = json!({
        "schema_version": "g5.task-event.v1",
        "exec_id": event.exec_id,
        "kind": event.kind,
        "payload": event.payload,
        "terminal": event.terminal,
        "taskId": event.task_id,
        "attempt": event.attempt,
        "seq": event.seq,
        "messageId": message_id,
        "sessionId": session_id,
        "projection_error": Value::Null,
    });
    if live {
        metadata["live"] = Value::Bool(true);
    }
    TimelineEntryPayload {
        id: journal_event_row_id(event.task_id, event.attempt, event.vm_id, event.seq),
        entry_type: timeline_type(&event.kind, &event.payload).to_string(),
        content: timeline_content(&event.kind, &event.payload),
        metadata,
        created_at,
    }
}

/// The post-commit live updates of one stored batch: the task's status and,
/// for a conversation, the chat events its boundary kinds produce. Built
/// without the database; the timeline rows themselves were published live
/// before the batch (`g5.chat.live-delivery`).
#[must_use]
pub fn journal_batch_live_events(
    org_id: Uuid,
    session_id: Option<Uuid>,
    task_id: Uuid,
    attempt: i32,
    events: &[(String, Value, Uuid)],
    task_status: &str,
) -> Vec<(Channel, WsEvent)> {
    let mut live = vec![(
        Channel::Task(task_id),
        WsEvent::TaskStatus {
            task_id,
            status: task_status.to_string(),
        },
    )];
    if let Some(session_id) = session_id {
        for (kind, payload, message_id) in events {
            let mut message = assistant_message(org_id, session_id, task_id, attempt);
            message.id = *message_id;
            live.extend(
                crate::services::agents::chat_service::agent_turn_live_events(
                    message,
                    kind,
                    payload,
                    task_status,
                ),
            );
        }
    }
    live
}

/// Bind one dispatcher replay record to the durable attempt identity selected
/// by the API. Dispatcher payloads cannot choose a different task, attempt, VM
/// or epoch during recovery.
pub(crate) fn journal_event_from_replay(
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    exec_id: &str,
    value: &Value,
) -> Result<JournalTaskEvent, AppError> {
    let seq = value["seq"]
        .as_u64()
        .ok_or_else(|| conflict("dispatcher event sequence is absent"))?;
    let kind = value["kind"]
        .as_str()
        .ok_or_else(|| conflict("dispatcher event kind is absent"))?;
    let payload = value
        .get("payload")
        .cloned()
        .ok_or_else(|| conflict("dispatcher event payload is absent"))?;
    let mut event = JournalTaskEvent {
        task_id,
        attempt,
        vm_id,
        epoch,
        exec_id: exec_id.to_string(),
        seq,
        kind: normalize_agent_turn_event_kind(kind).to_string(),
        payload,
        terminal: value["terminal"].as_bool().unwrap_or(false),
    };
    // `apply` replaces U+0000 again before storing; doing it here as well
    // keeps the live copy published from this event equal to the stored row.
    event.replace_nul();
    Ok(event)
}

pub(crate) fn normalize_agent_turn_event_kind(kind: &str) -> &str {
    let Some(candidate) = kind.strip_prefix("module.") else {
        return kind;
    };
    if matches!(
        candidate,
        "input.accepted"
            | "input.shipped"
            | "input.steer_rejected"
            | "input.steer_pending"
            | "provider.session"
            | "provider.capabilities"
            | "provider.status"
            | "provider.control_progress"
            | "provider_request.opened"
            | "provider_request.replied"
            | "provider_request.invalidated"
            | "message.delta"
            | "message.final"
            | "message.retracted"
            | "thought.delta"
            | "thought.final"
            | "thinking.progress"
            | "tool.started"
            | "tool.progress"
            | "tool.completed"
            | "plan.updated"
            | "subtask.started"
            | "subtask.progress"
            | "subtask.completed"
            | "background_tasks.replaced"
            | "workspace.change"
            | "context.compacted"
            | "context.usage"
            | "memory.recalled"
            | "artifact.persisted"
            | "commands.replaced"
            | "suggestion.available"
            | "usage.updated"
            | "conversation.reset"
            | "terminal.progress"
            | "notice"
            | "session.title"
            | "ontology.card"
    ) {
        candidate
    } else {
        kind
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReplyInput {
    pub task_id: Uuid,
    pub attempt: i32,
    pub provider_request_id: String,
    pub request_id: String,
    pub validated_response: Value,
    /// Start and answer (`g5.chat.start-and-send`): the Start consent this
    /// request recorded just before. A held question's first acceptance then
    /// needs that Start to be still under way instead of ready compute; the
    /// re-queued turn runs once the VM is up, as held work does after any
    /// Start (`g5.lifecycle.start.held-work`).
    pub pending_start: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeReplyOutcome {
    pub task_id: Uuid,
    pub attempt: i32,
    pub provider_request_id: String,
    pub request_id: String,
    pub response_digest: String,
    pub delivery_disposition: String,
}

#[derive(Debug, Clone)]
pub struct ClaimedNativeReply {
    local_disposition: Option<&'static str>,
    org_id: Uuid,
    workspace_id: Uuid,
    owner_user_id: Uuid,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    provider_request_id: String,
    request_id: String,
    response_digest: String,
    validated_response: Value,
}

#[derive(Debug, Clone)]
pub struct NativeRequestTimeout {
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    provider_request_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTaskCancelResult {
    pub task_id: Uuid,
    pub status: String,
    #[serde(skip)]
    execution: Option<WorkspaceTaskCancellation>,
}

/// What a chat's Stop did (`g5.agents.lifecycle.chat-stop`).
#[derive(Debug)]
pub enum ChatTurnStop {
    /// The named turn had already ended; nothing changed.
    AlreadyEnded,
    /// The named message was still waiting behind another turn and is
    /// withdrawn alone.
    Withdrawn,
    /// The turn ended, or its running attempt was asked to stop; a workflow turn
    /// is held for Continue instead. Deliver it after the commit.
    Stopped(WorkspaceTaskCancelResult),
}

/// What delivering a committed cancel or pause to the guest established
/// (`g5.chat.controls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelDelivery {
    /// The guest is signalling the running execution.
    Signalling,
    /// The execution had already finished; its outcome is being persisted.
    AlreadyFinished,
    /// The guest recorded the cancellation before any process ran.
    Cancelled,
    /// The transition needed no runtime delivery (queued or held work).
    NoExecution,
    /// The incarnation is gone; the reconciler settles the attempt.
    IncarnationGone,
    /// The runtime call failed; the follower redelivers the durable intent.
    Undelivered,
}

impl CancelDelivery {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Signalling => "signalling",
            Self::AlreadyFinished => "already_finished",
            Self::Cancelled => "cancelled",
            Self::NoExecution => "ended",
            Self::IncarnationGone => "ended",
            Self::Undelivered => "undelivered",
        }
    }
}

/// How a chat turn that is not running ends.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ChatTurnEnd {
    /// The owner's Stop named the turn: its own message is withdrawn if the
    /// agent never received it.
    Stopped,
    /// The owner sent a new message (`g5.agents.lifecycle.chat-held-turn`): a
    /// message of the turn the agent never received stays queued and runs
    /// first.
    NextMessage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePreviewView {
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub status: String,
    pub repo_id: Uuid,
    pub profile: Option<String>,
    pub port: u16,
    pub generation: u64,
    pub ready: bool,
    /// Recorded with a Start that has not brought the VM to ready yet: the
    /// task is `queued` and bound to that Start consent
    /// (`g5.preview.start-and-preview`).
    pub pending_start: bool,
    /// Close was requested and the guest is stopping the app and running
    /// the profile's cleanup; the task is still `running`.
    pub closing: bool,
    /// How a cancelled preview closed, from the guest's outcome: `clean`,
    /// `failed` (cleanup exited non-zero or overran), `killed` (the guest
    /// did not exit within the close grace) or `lost` (the VM was lost
    /// before cleanup finished). Absent until then.
    pub close: Option<String>,
    /// How the run ended, in the Activity row's structure
    /// (`g5.ui.activity.preview-row`); absent while it runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ending: Option<PreviewEnding>,
}

/// A running preview: its workspace, the incarnation its task runs on, and
/// the guest port it serves.
#[derive(Debug, Clone, Copy)]
pub struct WorkspacePreviewTarget {
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub port: u16,
}

/// The workspace's active preview, when one refuses a new start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePreview {
    /// Recorded with a Start that has not brought the VM up yet.
    WaitingForStart,
    /// Queued on, running on or closing on the VM.
    Running,
}

/// What `preview_start_preflight` reports.
#[derive(Debug, Clone, Default)]
pub struct PreviewStartPreflight {
    /// The caller owns the workspace; otherwise nothing else is read and
    /// admission refuses.
    pub owner: bool,
    pub replay: bool,
    pub vm_running: bool,
    /// The owner's own Start, one a preview may wait for.
    pub start_under_way: bool,
    /// Compute is starting or waiting for capacity under any Start, the
    /// owner's or not; a refusal then says the preview can be launched once
    /// the workspace runs, not that it should be started.
    pub starting: bool,
    pub active_preview: Option<ActivePreview>,
}

#[derive(Debug, Clone)]
pub struct StartWorkspacePreview {
    pub workspace_id: Uuid,
    pub actor_user_id: Uuid,
    pub repo_id: Uuid,
    pub profile: Option<String>,
    pub idempotency_key: String,
    /// The deployment's canonical preview host suffix. With one, the preview
    /// command learns its own isolated origin
    /// (`g5.workspace.preview-origin-security`).
    pub preview_host_suffix: Option<String>,
    /// The application as the branch commits it, resolved by the caller for
    /// a start while the owner's VM is not running
    /// (`g5.preview.start-and-preview`). Unused while the VM runs: the live
    /// checkout is the source then. `None` keeps the running-VM requirement.
    pub committed_application: Option<PreviewApplicationDTO>,
    /// The Start consent this request recorded (`start_and_send::begin`).
    /// A preview recorded while the VM is not running is bound to it, or,
    /// without one, to the Start already under way.
    pub start_intent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct WorkspaceTaskCancellation {
    workspace_id: Uuid,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttemptDecision {
    task_status: &'static str,
    attempt_reason: &'static str,
    continuation_reason: Option<&'static str>,
    hold_reason: Option<&'static str>,
    terminal: bool,
}

#[derive(Debug)]
struct LockedAttempt {
    org_id: Uuid,
    workspace_id: Uuid,

    task_kind: TaskKind,
    task_status: String,
    chat_session_id: Option<Uuid>,
    continuation_reason: Option<String>,
    task_spec: Value,
    policy_snapshot: Option<Value>,
    codex_auth_state: Option<Value>,
    acked_event_seq: u64,
    ended: bool,
}

impl TaskService {
    /// Discover configurations from the same live checkout that
    /// `start_workspace_preview` validates. Missing and malformed configs are
    /// omitted so the picker cannot advertise bytes from another authority.
    pub async fn list_live_preview_applications(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        repo_id: Uuid,
    ) -> Result<Vec<PreviewApplicationDTO>, AppError> {
        let handle = self
            .workspace_runtime
            .owner_handle(workspace_id, actor_user_id)
            .await
            .map_err(AppError::from_error)?;
        let runtime_request = |scope: &str| {
            RuntimeRequest::owner(
                &handle,
                actor_user_id,
                unix_time_ms().saturating_add(30_000),
                format!("preview-discovery:{scope}:{}", Uuid::new_v4()),
            )
        };
        let mut applications = Vec::new();

        match self
            .workspace_runtime
            .file_read(
                &handle,
                &runtime_request("workspace-default"),
                repo_id,
                ".g5/workspace.yaml",
            )
            .await
        {
            Ok(file) => {
                if let Some(config) = file["content"]
                    .as_str()
                    .and_then(|content| parse_preview_config(content, false).ok())
                {
                    applications.push(preview_application(
                        PreviewApplicationSource::WorkspaceDefault,
                        None,
                        ".g5/workspace.yaml".to_string(),
                        config,
                    ));
                }
            }
            Err(error) if error.message == "FILE_MISSING" => {}
            Err(error) => return Err(runtime_app_error(error)),
        }

        let listing = match self
            .workspace_runtime
            .file_list(
                &handle,
                &runtime_request("profile-list"),
                repo_id,
                ".g5/preview_profiles",
            )
            .await
        {
            Ok(listing) => Some(listing),
            Err(error) if error.message == "FILE_MISSING" => None,
            Err(error) => return Err(runtime_app_error(error)),
        };
        let mut profiles = listing
            .as_ref()
            .and_then(|listing| listing["entries"].as_array())
            .into_iter()
            .flatten()
            .filter(|entry| entry["type"].as_str() == Some("file"))
            .filter_map(|entry| entry["path"].as_str())
            .filter_map(|path| {
                path.strip_prefix(".g5/preview_profiles/")
                    .and_then(|name| name.strip_suffix(".yaml"))
                    .filter(|name| g5_contracts::types::validate_preview_profile_name(name).is_ok())
                    .map(|name| (name.to_string(), path.to_string()))
            })
            .collect::<Vec<_>>();
        profiles.sort();
        for (name, path) in profiles {
            let file = match self
                .workspace_runtime
                .file_read(
                    &handle,
                    &runtime_request(&format!("profile:{name}")),
                    repo_id,
                    &path,
                )
                .await
            {
                Ok(file) => file,
                Err(error) if error.message == "FILE_MISSING" => continue,
                Err(error) => return Err(runtime_app_error(error)),
            };
            let Some(config) = file["content"]
                .as_str()
                .and_then(|content| parse_preview_config(content, true).ok())
            else {
                continue;
            };
            applications.push(preview_application(
                PreviewApplicationSource::NamedProfile,
                Some(name),
                path,
                config,
            ));
        }
        Ok(applications)
    }

    pub async fn workspace_owner_terminal_secrets(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
        owner_user_id: Uuid,
    ) -> Result<std::collections::HashMap<String, String>, AppError> {
        let effective = self
            .dispatch_config_service()
            .resolve_effective(db, org_id, Some(workspace_id), Some(owner_user_id))
            .await?;
        // The terminal route admits this set under the shared policy and
        // answers the browser with the reason (`g5.terminal.secret-admission`).
        Ok(crate::services::agents::agent_dispatch::dispatch_secret_set(effective.secrets, false))
    }

    /// Create the owner's preview service task without starting or ensuring a
    /// workspace VM. The P4.1 `workspace_vms.preview_intent` row is the
    /// serialization authority. A workspace runs one preview at a time: while
    /// a preview is queued, running or closing, a new start is refused with a
    /// conflict and Close is the only offered action (the preview shares the
    /// owner's VM, so a silent replacement would leave the old preview's side
    /// effects beside the new one). The same idempotency key replays its task.
    pub async fn start_workspace_preview(
        &self,
        db: &mut ScopedDb,
        input: StartWorkspacePreview,
    ) -> Result<WorkspacePreviewView, AppError> {
        if input.idempotency_key.trim() != input.idempotency_key
            || input.idempotency_key.is_empty()
            || input.idempotency_key.len() > 256
        {
            return Err(conflict("preview idempotency key is invalid"));
        }
        if let Some(profile) = input.profile.as_deref() {
            g5_contracts::types::validate_preview_profile_name(profile).map_err(|error| {
                AppError::from_error(G5Error::ValidationError {
                    details: json!({"field":"profile","message":error.to_string()}),
                })
            })?;
        }

        // Resolve the profile through the live owner-only file surface while
        // the workspace runs: it validates the exact repo before any task or
        // preview intent is written. While the VM is not running, the caller
        // supplies the application as the branch commits it, and the task is
        // recorded bound to the Start that will bring the VM up
        // (`g5.preview.start-and-preview`).
        let handle = match self
            .workspace_runtime
            .owner_handle(input.workspace_id, input.actor_user_id)
            .await
        {
            Ok(handle) => Some(handle),
            Err(G5Error::WorkspaceNotRunning) => None,
            Err(error) => return Err(AppError::from_error(error)),
        };
        // Without a VM the port is known only from the committed
        // configuration; a replay needs none, so its absence is decided after
        // the replay check.
        let config_port: Option<u16> = match &handle {
            Some(handle) => {
                let config_path =
                    crate::services::preview_config::preview_config_path(input.profile.as_deref());
                let request = RuntimeRequest::owner(
                    handle,
                    input.actor_user_id,
                    unix_time_ms().saturating_add(30_000),
                    format!("preview-config:{}", input.idempotency_key),
                );
                let file = self
                    .workspace_runtime
                    .file_read(handle, &request, input.repo_id, &config_path)
                    .await
                    .map_err(runtime_app_error)?;
                let content = file["content"]
                    .as_str()
                    .ok_or_else(|| conflict("preview profile read omitted content"))?;
                Some(parse_preview_config(content, input.profile.is_some())?.port)
            }
            None => {
                match input.committed_application.as_ref() {
                    Some(committed) if committed.profile != input.profile => return Err(conflict(
                        "the committed preview application does not match the requested profile",
                    )),
                    committed => committed.map(|committed| committed.port),
                }
            }
        };

        // The digest names the request (repository and profile); the port is
        // derived from whichever configuration source applied, so a replay
        // after the VM came up matches the request recorded while it was down.
        let request_digest = preview_request_digest(input.repo_id, input.profile.as_deref());

        let authority = sqlx::query(
            "SELECT workspace.org_id, workspace.owner_user_id, workspace.is_base,
                    workspace.deleted_at, vm.vm_id, vm.epoch, vm.status,
                    vm.preview_generation, vm.preview_intent, vm.start_intent,
                    EXISTS(SELECT 1 FROM workspace_ownership_transfers transfer
                           WHERE transfer.workspace_id = workspace.id
                             AND transfer.status = 'pending') AS pending_transfer,
                    EXISTS(SELECT 1 FROM workspace_repos member
                           WHERE member.org_id = workspace.org_id
                             AND member.workspace_id = workspace.id
                             AND member.repo_id = $2) AS repo_member
             FROM workspaces AS workspace
             JOIN workspace_vms AS vm
               ON vm.org_id = workspace.org_id AND vm.workspace_id = workspace.id
             WHERE workspace.id = $1
             FOR UPDATE OF vm FOR SHARE OF workspace",
        )
        .bind(input.workspace_id)
        .bind(input.repo_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("workspace preview authority is absent"))?;
        let lease_valid = crate::services::workspace::workspace_runtime::lease_running(
            db.conn(),
            input.workspace_id,
        )
        .await?
            == Some(true);
        if authority.try_get::<Uuid, _>("owner_user_id")? != input.actor_user_id {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        if authority.try_get::<bool, _>("is_base")? {
            return Err(conflict("base workspaces must be forked before preview"));
        }
        if authority
            .try_get::<Option<DateTime<Utc>>, _>("deleted_at")?
            .is_some()
            || authority.try_get::<bool, _>("pending_transfer")?
        {
            return Err(conflict("workspace preview admission is closed"));
        }
        if !authority.try_get::<bool, _>("repo_member")? {
            return Err(conflict("preview repository is not a workspace member"));
        }
        // A replay answers from the recorded preview whatever the VM or its
        // Start are doing now, so a retry after a lost response sees the
        // preview it recorded, including one its Start then cancelled.
        let previous_intent = authority.try_get::<Option<Value>, _>("preview_intent")?;
        if let Some(intent) = previous_intent.as_ref() {
            if intent["idempotency_key"].as_str() == Some(input.idempotency_key.as_str()) {
                // Intents recorded before the port left the digest carry the
                // older shape; a replay of one of them matches it as well.
                let legacy_digest = intent["port"]
                    .as_u64()
                    .and_then(|port| u16::try_from(port).ok())
                    .map(|port| {
                        legacy_preview_request_digest(input.repo_id, input.profile.as_deref(), port)
                    });
                let stored = intent["request_digest"].as_str();
                if stored != Some(request_digest.as_str()) && stored != legacy_digest.as_deref() {
                    return Err(conflict(
                        "preview idempotency key was reused with another request",
                    ));
                }
                let task_id = intent["task_id"]
                    .as_str()
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .ok_or_else(|| conflict("preview intent has no valid task identity"))?;
                return self.preview_view(db, input.workspace_id, task_id).await;
            }
        }

        let config_port =
            config_port.ok_or_else(|| AppError::from_error(G5Error::WorkspaceNotRunning))?;
        let current_vm_id = authority
            .try_get::<Option<String>, _>("vm_id")?
            .and_then(|value| Uuid::parse_str(&value).ok());
        let current_epoch = authority.try_get::<i64, _>("epoch")?;
        let vm_status = authority.try_get::<String, _>("status")?;
        // The consent a pending preview is bound to. Under the VM row lock the
        // ready transition cannot interleave, so a consent seen live here is
        // settled later by the trigger, never missed (migration 152).
        // A request that recorded a Start and finds the VM running must find
        // the row still carrying that consent, neither ended nor replaced:
        // compute another Start brought up is not proof that this request's
        // Start succeeded (a cancelled Start must not launch later), as
        // `bind_input_to_start_consent` requires. A consent recorded against
        // compute already running is not consumed by a ready transition; it
        // is this request's as long as it is live on the row.
        let recorded_consent_consumed = || -> Result<(), AppError> {
            let Some(recorded) = input.start_intent_id else {
                return Ok(());
            };
            let intent = authority.try_get::<Option<Value>, _>("start_intent")?;
            let same = intent
                .as_ref()
                .and_then(|intent| intent["id"].as_str())
                .and_then(|id| Uuid::parse_str(id).ok())
                == Some(recorded);
            let ended = matches!(
                intent.as_ref().and_then(|intent| intent["status"].as_str()),
                Some("cancelled" | "expired" | "failed")
            );
            if same && !ended {
                Ok(())
            } else {
                Err(conflict(
                    "The Start ended before the preview was queued; nothing was started",
                ))
            }
        };
        let start_consent_id = match &handle {
            Some(handle) => {
                if vm_status != "running"
                    || !lease_valid
                    || current_vm_id != Some(handle.vm_id)
                    || current_epoch != handle.epoch as i64
                {
                    return Err(AppError::from_error(G5Error::WorkspaceNotRunning));
                }
                recorded_consent_consumed()?;
                None
            }
            // The VM came up between the handle read and this lock: the
            // Start succeeded, so the preview is recorded unbound and the
            // dispatcher admits it now, as a consent already consumed does
            // for a Start-and-send input.
            None if vm_status == "running" && lease_valid => {
                recorded_consent_consumed()?;
                None
            }
            None => {
                let intent = authority.try_get::<Option<Value>, _>("start_intent")?;
                let live_intent =
                    crate::services::workspace::workspace_vm_service::live_owner_start_intent(
                        intent.as_ref(),
                        Some(input.actor_user_id),
                        Utc::now(),
                    );
                // A request that recorded a Start binds only to that Start:
                // the owner's own later Start joins it rather than replacing
                // it, so a different live consent here means the recorded one
                // was cancelled and another action started the workspace,
                // and a cancelled Start must not launch the preview.
                match (live_intent, input.start_intent_id) {
                    (Some(live), Some(recorded)) if live == recorded => Some(live),
                    (Some(live), None) => Some(live),
                    (_, Some(_)) => {
                        return Err(conflict(
                            "The Start ended before the preview was queued; nothing was started",
                        ))
                    }
                    // Waiting for capacity is a consent status; the VM row
                    // itself still reads stopped or failed.
                    (None, None)
                        if vm_status == "starting"
                            || intent.as_ref().and_then(|intent| intent["status"].as_str())
                                == Some("queued_for_capacity") =>
                    {
                        return Err(conflict(
                            "the workspace is starting without a Start of yours to wait for; try again once it is running",
                        ))
                    }
                    (None, None) => return Err(AppError::from_error(G5Error::WorkspaceNotRunning)),
                }
            }
        };

        if self.preview_is_active(db, input.workspace_id).await? {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: "a preview is already running; close it before starting another"
                    .to_string(),
            }));
        }

        let generation = u64::try_from(authority.try_get::<i64, _>("preview_generation")?)
            .map_err(|_| conflict("preview generation is invalid"))?
            .saturating_add(1);
        let task_id = Uuid::new_v4();
        let mut module_input = json!({
            "repo_id": input.repo_id,
            "profile": input.profile,
        });
        // The preview host is `<task>.<suffix>` (routes/preview); the guest
        // exports it as G5D_PREVIEW_ORIGIN so the app can trust its own origin.
        if let Some(suffix) = input.preview_host_suffix.as_deref() {
            module_input["origin"] = json!(format!("https://{task_id}.{suffix}"));
        }
        let spec = json!({
            "command": {
                "schema_version": "g5.preview-task-spec/v1",
                "module_id": "workspace.preview",
                "module_version": "1.0.0",
                "operation": "start",
                "input_schema": "g5.workspace.preview.start/v1",
                "input": module_input,
            },
            "preview": {
                "generation": generation,
                "repo_id": input.repo_id,
                "profile": input.profile,
                "port": config_port,
                "idempotency_key": input.idempotency_key,
                "request_digest": request_digest,
            },
            "dependency_failure_policy": "fail_dependents",
        });
        sqlx::query(
            "INSERT INTO tasks(
                 id, name, status, retry_count,
                 depends_on, input_tokens, output_tokens, cost_usd, org_id,
                 workspace_id, kind, spec, attempt, start_confirmed,
                 created_by, created_at, updated_at, start_consent_id)
             VALUES ($1,$6,'queued',0,'{}'::uuid[],0,0,0,
                     $2,$3,'preview',$4,1,false,$5,now(),now(),$7)",
        )
        .bind(task_id)
        .bind(authority.try_get::<Uuid, _>("org_id")?)
        .bind(input.workspace_id)
        .bind(&spec)
        .bind(input.actor_user_id)
        .bind(preview_task_name(input.profile.as_deref()))
        .bind(start_consent_id)
        .execute(db.conn())
        .await?;
        sqlx::query(
            "UPDATE workspace_vms
             SET preview_generation = $2,
                 preview_intent = jsonb_build_object(
                     'generation',$2,'action',$3,'status','queued',
                     'requested_by',$4::text,'task_id',$5::text,
                     'idempotency_key',$6,'request_digest',$7,
                     'repo_id',$8::text,'profile',$9,'port',$10),
                 updated_at = now()
             WHERE workspace_id = $1",
        )
        .bind(input.workspace_id)
        .bind(i64::try_from(generation).map_err(|_| conflict("preview generation overflow"))?)
        .bind("start")
        .bind(input.actor_user_id)
        .bind(task_id)
        .bind(&input.idempotency_key)
        .bind(&request_digest)
        .bind(input.repo_id)
        .bind(input.profile.as_deref())
        .bind(i32::from(config_port))
        .execute(db.conn())
        .await?;

        Ok(WorkspacePreviewView {
            workspace_id: input.workspace_id,
            task_id,
            status: "queued".to_string(),
            repo_id: input.repo_id,
            profile: input.profile,
            port: config_port,
            generation,
            ready: false,
            pending_start: start_consent_id.is_some(),
            closing: false,
            close: None,
            ending: None,
        })
    }

    /// What a preview start needs to know before recording any consent
    /// (`g5.preview.start-and-preview`): whether the request replays the
    /// owner's recorded preview, whether the VM runs, whether the owner's
    /// Start is under way, and whether a preview is already active and, if
    /// so, still waiting for a Start. One read; admission decides again
    /// under the VM row lock.
    pub async fn preview_start_preflight(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        idempotency_key: &str,
    ) -> Result<PreviewStartPreflight, AppError> {
        // A workspace never started has no VM row yet; its owner still
        // records a Start, so the row is optional here.
        let row = sqlx::query(
            "SELECT vm.preview_intent ->> 'idempotency_key' = $3 AS replay,
                    vm.status = 'running' AND vm.lease_expires_at > clock_timestamp() AS vm_running,
                    vm.status = 'starting'
                        OR vm.start_intent ->> 'status' = 'queued_for_capacity' AS starting,
                    vm.start_intent,
                    (SELECT task.start_consent_id IS NOT NULL FROM tasks AS task
                     WHERE task.workspace_id = workspace.id AND task.kind = 'preview'
                       AND task.status IN ('queued','running','waiting')
                     ORDER BY task.created_at DESC LIMIT 1) AS active_pending
             FROM workspaces AS workspace
             LEFT JOIN workspace_vms AS vm
               ON vm.org_id = workspace.org_id AND vm.workspace_id = workspace.id
             WHERE workspace.id = $1 AND workspace.owner_user_id = $2
               AND workspace.deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(actor_user_id)
        .bind(idempotency_key)
        .fetch_optional(db.conn())
        .await?;
        let Some(row) = row else {
            return Ok(PreviewStartPreflight::default());
        };
        let intent: Option<Value> = row.try_get("start_intent")?;
        Ok(PreviewStartPreflight {
            owner: true,
            replay: row.try_get::<Option<bool>, _>("replay")?.unwrap_or(false),
            vm_running: row
                .try_get::<Option<bool>, _>("vm_running")?
                .unwrap_or(false),
            start_under_way:
                crate::services::workspace::workspace_vm_service::live_owner_start_intent(
                    intent.as_ref(),
                    Some(actor_user_id),
                    Utc::now(),
                )
                .is_some(),
            starting: row.try_get::<Option<bool>, _>("starting")?.unwrap_or(false),
            active_preview: row
                .try_get::<Option<bool>, _>("active_pending")?
                .map(|pending| {
                    if pending {
                        ActivePreview::WaitingForStart
                    } else {
                        ActivePreview::Running
                    }
                }),
        })
    }

    /// The workspace's current preview is still queued, running or closing,
    /// so a new start would be refused (`g5.vm.preview-exclusive`).
    pub async fn preview_is_active(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<bool, AppError> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tasks
                           WHERE workspace_id = $1 AND kind = 'preview'
                             AND status IN ('queued','running','waiting'))",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await?)
    }

    pub async fn current_workspace_preview(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Option<WorkspacePreviewView>, AppError> {
        let task_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT (vm.preview_intent ->> 'task_id')::uuid
             FROM workspace_vms AS vm
             JOIN workspaces AS workspace
               ON workspace.org_id = vm.org_id AND workspace.id = vm.workspace_id
             WHERE vm.workspace_id = $1 AND workspace.owner_user_id = $2
               AND vm.preview_intent ->> 'task_id' IS NOT NULL",
        )
        .bind(workspace_id)
        .bind(actor_user_id)
        .fetch_optional(db.conn())
        .await?;
        match task_id {
            Some(task_id) => self.preview_view(db, workspace_id, task_id).await.map(Some),
            None => Ok(None),
        }
    }

    /// Resolve the current running preview selected by a task id. The caller
    /// supplies a task-scoped database so tenant and owner visibility are
    /// established before this lifecycle state is inspected.
    pub async fn workspace_preview_target(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        require_ready: bool,
    ) -> Result<WorkspacePreviewTarget, AppError> {
        // clock_timestamp(): the caller's transaction may have waited for the
        // workspace row in its authorization, and now() is when it began.
        let row = sqlx::query(
            "SELECT task.workspace_id, task.status, task.spec, task.vm_id, task.epoch,
                    vm.preview_intent ->> 'task_id' = task.id::text AS current,
                    COALESCE(vm.status = 'running'
                             AND vm.lease_expires_at > clock_timestamp()
                             AND task.vm_id = vm.vm_id
                             AND task.epoch = vm.epoch, false) AS live_incarnation,
                    EXISTS(SELECT 1 FROM task_events event
                           WHERE event.task_id = task.id
                             AND event.metadata ->> 'kind' = 'module.preview.ready') AS ready
             FROM tasks AS task
             JOIN workspace_vms AS vm ON vm.workspace_id = task.workspace_id
             WHERE task.id = $1 AND task.kind = 'preview'",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("workspace preview task {task_id}"),
            })
        })?;
        let status: String = row.try_get("status")?;
        let ready: bool = row.try_get("ready")?;
        let spec: Value = row.try_get("spec")?;
        // A closing preview (`g5.preview.service.close`) admits no new
        // stream: its app is being stopped and cleaned up while the task is
        // still running.
        let closing = spec
            .pointer("/lifecycle/cancel_requested")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !row.try_get::<bool, _>("current")?
            || !row.try_get::<bool, _>("live_incarnation")?
            || status != "running"
            || closing
            || (require_ready && !ready)
        {
            return Err(AppError::from_error(G5Error::ServiceUnavailable {
                message: "PREVIEW_NOT_READY".to_string(),
            }));
        }
        let port = preview_spec_port(&spec).ok_or_else(|| {
            AppError::from_error(G5Error::Internal {
                message: "preview task has no valid configured port".to_string(),
            })
        })?;
        // A live incarnation matched the task's VM and epoch above.
        let not_ready = || {
            AppError::from_error(G5Error::ServiceUnavailable {
                message: "PREVIEW_NOT_READY".to_string(),
            })
        };
        let vm_id = row
            .try_get::<Option<String>, _>("vm_id")?
            .and_then(|vm_id| Uuid::parse_str(&vm_id).ok())
            .ok_or_else(not_ready)?;
        let epoch = row
            .try_get::<Option<i64>, _>("epoch")?
            .and_then(|epoch| u64::try_from(epoch).ok())
            .ok_or_else(not_ready)?;
        Ok(WorkspacePreviewTarget {
            workspace_id: row.try_get("workspace_id")?,
            vm_id,
            epoch,
            port,
        })
    }

    async fn preview_view(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        task_id: Uuid,
    ) -> Result<WorkspacePreviewView, AppError> {
        let row = sqlx::query(
            "SELECT task.status, task.spec, task.outcome, vm.preview_generation,
                    task.start_consent_id IS NOT NULL AND task.status = 'queued'
                        AS pending_start,
                    EXISTS(SELECT 1 FROM task_events event
                           WHERE event.task_id = task.id
                             AND (event.metadata ->> 'kind' = 'invocation.started'
                                  OR event.metadata ->> 'kind' LIKE 'module.%')) AS started,
                    EXISTS(SELECT 1 FROM task_events event
                           WHERE event.task_id = task.id
                             AND event.metadata ->> 'kind' = 'module.preview.ready') AS ready
             FROM tasks AS task
             JOIN workspace_vms AS vm ON vm.workspace_id = task.workspace_id
             WHERE task.id = $1 AND task.workspace_id = $2 AND task.kind = 'preview'",
        )
        .bind(task_id)
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("preview task is absent"))?;
        let spec: Value = row.try_get("spec")?;
        let repo_id = spec["preview"]["repo_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| conflict("preview task repository is invalid"))?;
        let port =
            preview_spec_port(&spec).ok_or_else(|| conflict("preview task port is invalid"))?;
        let status: String = row.try_get("status")?;
        let closing = preview_closing(&status, &spec);
        let outcome: Option<Value> = row.try_get("outcome")?;
        let close = outcome
            .as_ref()
            .and_then(|outcome| preview_close(&status, outcome));
        let started: bool = row.try_get("started")?;
        let ready: bool = row.try_get("ready")?;
        // The dialog words an ended run as the Activity row does
        // (`g5.ui.activity.preview-row`), from the same two event facts.
        let ending = project_preview_run(
            &status,
            &spec,
            outcome.as_ref(),
            PreviewRunEvents { started, ready },
        )
        .ending;
        Ok(WorkspacePreviewView {
            workspace_id,
            task_id,
            status,
            repo_id,
            profile: spec["preview"]["profile"].as_str().map(str::to_string),
            port,
            generation: spec["preview"]["generation"]
                .as_u64()
                .unwrap_or_else(|| row.try_get::<i64, _>("preview_generation").unwrap_or(0) as u64),
            ready,
            pending_start: row.try_get("pending_start")?,
            closing,
            close,
            ending,
        })
    }

    pub async fn is_workspace_runtime_task(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
    ) -> Result<bool, AppError> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT COALESCE(workspace_id IS NOT NULL
                    AND kind IN ('agent','command','preview'), false)
             FROM tasks WHERE id = $1",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .unwrap_or(false))
    }

    /// Persist owner cancellation before contacting the dispatcher. Queued
    /// and held work settles immediately; a running attempt is fenced by a
    /// durable cancel intent that the reconciler keeps redelivering.
    pub async fn request_workspace_task_cancel(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        task_id: Uuid,
    ) -> Result<WorkspaceTaskCancelResult, AppError> {
        share_task_workspace(db, task_id).await?;
        lock_preview_vm_row(db, task_id).await?;
        self.announce_chat_activity(db, task_id).await?;
        let row = sqlx::query(
            "SELECT task.org_id, task.workspace_id, task.kind, task.status,
                    task.attempt, task.vm_id, task.epoch, workspace.owner_user_id
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             WHERE task.id = $1 AND task.kind IN ('agent','command','preview')
             FOR UPDATE OF task",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("task is not a workspace runtime task"))?;
        if row.try_get::<Option<Uuid>, _>("owner_user_id")? != Some(actor_user_id) {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        let status: String = row.try_get("status")?;
        let is_preview = row.try_get::<String, _>("kind")? == "preview";
        if is_terminal_status(&status) {
            return Ok(WorkspaceTaskCancelResult {
                task_id,
                status,
                execution: None,
            });
        }
        if matches!(status.as_str(), "queued" | "waiting") {
            sqlx::query(
                "UPDATE tasks
                 SET status = 'cancelled',
                     outcome = jsonb_build_object('status','cancelled','reason','owner_cancel'),
                     hold_reason = NULL, continuation_reason = NULL,
                     start_consent_id = NULL,
                     ended_at = now(), updated_at = now()
                 WHERE id = $1 AND status IN ('queued','waiting')",
            )
            .bind(task_id)
            .execute(db.conn())
            .await?;
            sqlx::query(
                "UPDATE session_inputs
                 SET delivery = 'withdrawn', withdrawn_at = now(),
                     delivery_audit = delivery_audit || jsonb_build_array(
                         jsonb_build_object('event','task_cancelled'))
                 WHERE task_id = $1
                   AND delivery IN ('queued','reserved','delivery_unknown')",
            )
            .bind(task_id)
            .execute(db.conn())
            .await?;
            fail_task_dependents(
                db,
                row.try_get("org_id")?,
                row.try_get("workspace_id")?,
                task_id,
            )
            .await?;
            invalidate_attempt_requests(db, task_id, row.try_get("attempt")?, "owner_cancel")
                .await?;
            if is_preview {
                settle_preview_intent(db, row.try_get("workspace_id")?, task_id, "settled").await?;
            }
            if row.try_get::<String, _>("kind")? == "agent" {
                release_automatic_consent_without_work(db.conn(), row.try_get("workspace_id")?)
                    .await?;
            }
            return Ok(WorkspaceTaskCancelResult {
                task_id,
                status: "cancelled".to_string(),
                execution: None,
            });
        }
        if status != "running" {
            return Err(conflict(format!("task cannot be cancelled from {status}")));
        }
        let attempt: i32 = row.try_get("attempt")?;
        let vm_id = Uuid::parse_str(row.try_get::<String, _>("vm_id")?.as_str())
            .map_err(|_| conflict("workspace VM identity is invalid"))?;
        let epoch = u64::try_from(row.try_get::<i64, _>("epoch")?)
            .map_err(|_| conflict("task epoch is invalid"))?;
        // Repeated requests converge on the first intent: the requester and
        // time of the first Stop stand (`g5.chat.controls`).
        sqlx::query(
            "UPDATE tasks
             SET spec = jsonb_set(
                     spec, '{lifecycle}',
                     COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                         jsonb_build_object(
                             'cancel_requested',true,
                             'cancel_requested_by',
                                 COALESCE(spec #>> '{lifecycle,cancel_requested_by}', $2::text),
                             'cancel_requested_at',
                                 COALESCE(spec -> 'lifecycle' -> 'cancel_requested_at',
                                          to_jsonb(now()))),
                     true),
                 updated_at = now()
             WHERE id = $1 AND status = 'running'",
        )
        .bind(task_id)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        invalidate_attempt_requests(db, task_id, attempt, "owner_cancel").await?;
        if is_preview {
            settle_preview_intent(db, row.try_get("workspace_id")?, task_id, "cancelling").await?;
        }
        Ok(WorkspaceTaskCancelResult {
            task_id,
            status: "cancelling".to_string(),
            execution: Some(WorkspaceTaskCancellation {
                workspace_id: row.try_get("workspace_id")?,
                task_id,
                attempt,
                vm_id,
                epoch,
                reason: "owner_cancel",
            }),
        })
    }

    pub async fn request_workspace_task_pause(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        task_id: Uuid,
    ) -> Result<WorkspaceTaskCancelResult, AppError> {
        share_task_workspace(db, task_id).await?;
        self.announce_chat_activity(db, task_id).await?;
        let row = sqlx::query(
            "SELECT task.workspace_id, task.kind, task.status, task.attempt, task.vm_id,
                    task.epoch, workspace.owner_user_id
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             WHERE task.id = $1 AND task.kind IN ('agent','command','preview')
             FOR UPDATE OF task",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("task is not a workspace runtime task"))?;
        if row.try_get::<Option<Uuid>, _>("owner_user_id")? != Some(actor_user_id) {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        if row.try_get::<String, _>("kind")? != "agent" {
            return Err(conflict("only agent tasks can be paused"));
        }
        let status: String = row.try_get("status")?;
        if status == "waiting" {
            return Ok(WorkspaceTaskCancelResult {
                task_id,
                status,
                execution: None,
            });
        }
        if status == "queued" {
            sqlx::query(
                "UPDATE tasks
                 SET status = 'waiting', hold_reason = 'owner_stop',
                     updated_at = now()
                 WHERE id = $1 AND status = 'queued'",
            )
            .bind(task_id)
            .execute(db.conn())
            .await?;
            return Ok(WorkspaceTaskCancelResult {
                task_id,
                status: "waiting".to_string(),
                execution: None,
            });
        }
        if status != "running" {
            return Err(conflict(format!("task cannot be paused from {status}")));
        }
        let attempt: i32 = row.try_get("attempt")?;
        let vm_id = Uuid::parse_str(row.try_get::<String, _>("vm_id")?.as_str())
            .map_err(|_| conflict("workspace VM identity is invalid"))?;
        let epoch = u64::try_from(row.try_get::<i64, _>("epoch")?)
            .map_err(|_| conflict("task epoch is invalid"))?;
        sqlx::query(
            "UPDATE tasks
             SET spec = jsonb_set(
                     spec, '{lifecycle}',
                     COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                         jsonb_build_object(
                             'interrupt_reason','user',
                             'interrupt_requested_by',$2::text,
                             'interrupt_requested_at',now()),
                     true),
                 updated_at = now()
             WHERE id = $1 AND status = 'running'",
        )
        .bind(task_id)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        invalidate_attempt_requests(db, task_id, attempt, "owner_pause").await?;
        Ok(WorkspaceTaskCancelResult {
            task_id,
            status: "pausing".to_string(),
            execution: Some(WorkspaceTaskCancellation {
                workspace_id: row.try_get("workspace_id")?,
                task_id,
                attempt,
                vm_id,
                epoch,
                reason: "owner_pause",
            }),
        })
    }

    /// Stop in a chat (`g5.agents.lifecycle.chat-stop`). `turn_id` is the id
    /// the client holds for the turn: the session input a send returned, or
    /// the turn's reply. A running turn's attempt is cancelled and settles with
    /// its partial reply marked stopped; a queued or held turn ends now. Either
    /// way the messages queued behind it start the next turn. A message still
    /// waiting behind another turn is withdrawn alone, a workflow turn is held for
    /// Continue as before, and a turn that already ended is left alone, so a
    /// late Stop never reaches a later turn.
    pub async fn end_chat_turn(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<ChatTurnStop, AppError> {
        // Turn allocation first, as a send takes it, before any row lock.
        crate::services::agents::chat_service::lock_session_turn_allocation(db.conn(), session_id)
            .await?;
        self.announce_session_chat_activity(db, session_id).await?;
        let input = sqlx::query(
            "SELECT task_id, delivery FROM session_inputs
             WHERE id = $1 AND session_id = $2",
        )
        .bind(turn_id)
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?;
        let task_id = match &input {
            Some(input) => input.try_get::<Option<Uuid>, _>("task_id")?,
            None => sqlx::query_scalar::<_, Option<Uuid>>(
                "SELECT task_id FROM chat_messages WHERE id = $1 AND session_id = $2",
            )
            .bind(turn_id)
            .bind(session_id)
            .fetch_optional(db.conn())
            .await?
            .flatten(),
        };
        let Some(task_id) = task_id else {
            return Ok(ChatTurnStop::AlreadyEnded);
        };
        share_task_workspace(db, task_id).await?;
        let task = sqlx::query(
            "SELECT task.org_id, task.workspace_id, task.status,
                    workspace.owner_user_id
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             WHERE task.id = $1 AND task.kind = 'agent' AND task.chat_session_id = $2
             FOR UPDATE OF task",
        )
        .bind(task_id)
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("the named turn is not an agent turn of this session"))?;
        if task.try_get::<Option<Uuid>, _>("owner_user_id")? != Some(actor_user_id) {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        let status: String = task.try_get("status")?;
        if is_terminal_status(&status) {
            return Ok(ChatTurnStop::AlreadyEnded);
        }
        if let Some(input) = &input {
            let delivery: String = input.try_get("delivery")?;
            if delivery == "withdrawn" {
                return Ok(ChatTurnStop::AlreadyEnded);
            }
            // A queued or held turn's own message, its earliest, names the
            // turn, ended below. Any other queued message waits behind the turn
            // holding the session, in a workflow conversation too: it is withdrawn alone.
            if delivery == "queued"
                && !(matches!(status.as_str(), "queued" | "waiting")
                    && turn_message(db, task_id).await? == Some(turn_id))
            {
                self.withdraw_waiting_input_for_stop(db, actor_user_id, session_id, turn_id)
                    .await?;
                return Ok(ChatTurnStop::Withdrawn);
            }
        }
        if turn_kept_for_its_owner(db, task_id).await? {
            // A platform-owned workflow must retain its resolution task.
            return self
                .request_workspace_task_pause(db, actor_user_id, task_id)
                .await
                .map(ChatTurnStop::Stopped);
        }
        if status == "running" {
            return self
                .request_workspace_task_cancel(db, actor_user_id, task_id)
                .await
                .map(ChatTurnStop::Stopped);
        }
        end_chat_turn_now(db, task_id, ChatTurnEnd::Stopped).await?;
        ensure_next_queued_agent_task(
            db,
            task.try_get("org_id")?,
            task.try_get("workspace_id")?,
            Some(session_id),
            task_id,
        )
        .await?;
        Ok(ChatTurnStop::Stopped(WorkspaceTaskCancelResult {
            task_id,
            status: "cancelled".to_string(),
            execution: None,
        }))
    }

    pub async fn continue_workspace_task(
        &self,
        db: &mut ScopedDb,
        actor_user_id: Uuid,
        task_id: Uuid,
    ) -> Result<String, AppError> {
        share_task_workspace(db, task_id).await?;
        self.announce_chat_activity(db, task_id).await?;
        let row = sqlx::query(
            "SELECT task.status, task.kind, task.continuation_reason, task.hold_reason,
                    workspace.owner_user_id
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             WHERE task.id = $1 AND task.kind IN ('agent','command')
             FOR UPDATE OF task",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("task is not a continuable agent task"))?;
        if row.try_get::<Option<Uuid>, _>("owner_user_id")? != Some(actor_user_id) {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        let status: String = row.try_get("status")?;
        let hold_reason: Option<String> = row.try_get("hold_reason")?;
        if hold_reason.as_deref() == Some("delivery_unknown") {
            return Err(conflict(
                "agent task input delivery is unknown; authorize or withdraw that input before continuing",
            ));
        }
        // An accepted answer may need another attempt after a known-unwritten
        // delivery failure. The delivery_unknown guard above still requires an
        // explicit input decision before any uncertain response can be retried.
        let needs_answer: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM provider_requests WHERE task_id = $1
             AND resume_session_id IS NOT NULL AND status = 'pending')",
        )
        .bind(task_id)
        .fetch_one(db.conn())
        .await?;
        if needs_answer {
            return Err(conflict(
                "answer or cancel the pending question before continuing",
            ));
        }
        let continuation_reason: Option<String> = row.try_get("continuation_reason")?;
        let held_before_first_attempt = matches!(
            hold_reason.as_deref(),
            Some("owner_stop" | "idle_stop" | "transfer" | "start_required" | "vm_lost")
        ) && continuation_reason.is_none();
        if status != "waiting" || (continuation_reason.is_none() && !held_before_first_attempt) {
            return Err(conflict(format!(
                "agent task has no held continuation from {status}"
            )));
        }
        let updated = sqlx::query(
            "UPDATE tasks
             SET status = 'queued', hold_reason = NULL,
                 spec = spec - 'lifecycle', updated_at = now()
             WHERE id = $1 AND status = 'waiting' AND ended_at IS NULL",
        )
        .bind(task_id)
        .execute(db.conn())
        .await?;
        if updated.rows_affected() != 1 {
            return Err(conflict("task continuation lost its state race"));
        }
        Ok("queued".to_string())
    }

    /// Record the guest's confirmed acknowledgement of an attempt's persisted
    /// prefix (`g5.chat.journal`); never moves backwards.
    pub async fn record_journal_acknowledgement(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        attempt: i32,
        through: u64,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE task_attempts SET journal_acked_seq = GREATEST(journal_acked_seq, $3)
             WHERE task_id = $1 AND attempt = $2",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(i64::try_from(through).unwrap_or(i64::MAX))
        .execute(db.conn())
        .await?;
        Ok(())
    }

    /// The session's running agent attempt, if any: task, attempt and VM
    /// incarnation, as the live tail read needs them
    /// (`g5.chat.live-tail-recovery`).
    pub async fn running_session_attempt(
        &self,
        db: &mut ScopedDb,
        session_id: Uuid,
    ) -> Result<Option<(Uuid, i32, Uuid)>, AppError> {
        let row: Option<(Uuid, i32, Option<String>)> = sqlx::query_as(
            "SELECT id, attempt, vm_id FROM tasks
             WHERE chat_session_id = $1 AND kind = 'agent' AND status = 'running'
               AND vm_id IS NOT NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?;
        let Some((task_id, attempt, Some(vm_id))) = row else {
            return Ok(None);
        };
        let vm_id = Uuid::parse_str(&vm_id).map_err(|_| conflict("task VM identity is invalid"))?;
        Ok(Some((task_id, attempt, vm_id)))
    }

    /// Deliver a committed cancel or pause to the guest and report the
    /// runtime's disposition (`g5.chat.controls`). The intent is durable
    /// before this call; a failed delivery is `Undelivered`, and the
    /// attempt's follower redelivers it, so the caller reports it rather
    /// than failing a request whose Stop already committed.
    pub async fn deliver_workspace_task_cancel(
        &self,
        result: &WorkspaceTaskCancelResult,
    ) -> Result<CancelDelivery, AppError> {
        let Some(cancel) = &result.execution else {
            return Ok(CancelDelivery::NoExecution);
        };
        let exec_id = canonical_exec_id(cancel.task_id, cancel.attempt, cancel.vm_id);
        let handle = match self.workspace_runtime.get(cancel.workspace_id).await {
            Ok(Some(handle)) if handle.vm_id == cancel.vm_id && handle.epoch == cancel.epoch => {
                handle
            }
            Ok(_) => return Ok(CancelDelivery::IncarnationGone),
            Err(error) => {
                self.followers.poke(&exec_id);
                tracing::warn!(task_id = %cancel.task_id, %error,
                    "cancellation delivery could not read the runtime; the follower redelivers");
                return Ok(CancelDelivery::Undelivered);
            }
        };
        match self
            .workspace_runtime
            .modules_cancel(
                &handle,
                &RuntimeRequest::lifecycle(
                    cancel.workspace_id,
                    cancel.vm_id,
                    cancel.epoch,
                    unix_time_ms().saturating_add(15_000),
                    format!(
                        "task-cancel:{}:{}:{}",
                        cancel.task_id, cancel.attempt, cancel.vm_id
                    ),
                ),
                &exec_id,
                cancel.reason,
            )
            .await
        {
            Ok(reply) => {
                let disposition = reply["disposition"].as_str().unwrap_or("");
                tracing::info!(
                    target: "g5_api::live_relay",
                    task_id = %cancel.task_id,
                    attempt = cancel.attempt,
                    reason = cancel.reason,
                    disposition,
                    "cancellation delivered"
                );
                Ok(match disposition {
                    "already_finished" => CancelDelivery::AlreadyFinished,
                    "cancelled" => CancelDelivery::Cancelled,
                    _ => CancelDelivery::Signalling,
                })
            }
            Err(error) => {
                self.followers.poke(&exec_id);
                tracing::warn!(task_id = %cancel.task_id, attempt = cancel.attempt,
                    error = %error.message,
                    "cancellation delivery failed; the follower redelivers the committed intent");
                Ok(CancelDelivery::Undelivered)
            }
        }
    }

    /// Apply one dispatcher fact or control-plane fact under the caller's
    /// tenant transaction.  No other P4 task path may write lifecycle columns
    /// on a task carrying `vm_id`.
    pub async fn apply(
        &self,
        db: &mut ScopedDb,
        mut event: TaskLifecycleEvent,
    ) -> Result<ApplyResult, AppError> {
        event.replace_nul();
        let task_id = match &event {
            TaskLifecycleEvent::Journal(event) => event.task_id,
            TaskLifecycleEvent::StartDisposition { task_id, .. }
            | TaskLifecycleEvent::VmLost { task_id, .. }
            | TaskLifecycleEvent::RecoveredOutcome { task_id, .. } => *task_id,
        };
        // A provider request opening or settling leaves the task running but
        // changes whether the turn waits for the person
        // (chat g5.chat.activity.awaiting-input).
        let provider_request = matches!(
            &event,
            TaskLifecycleEvent::Journal(journal) if journal.kind.starts_with("provider_request.")
        );
        lock_agent_task_session_turn_allocation(db, task_id).await?;
        lock_preview_vm_row(db, task_id).await?;
        let result = self.apply_locked(db, event).await?;
        // A settled attempt or a turn that left `running` changes the chat's
        // activity (chat g5.chat.activity.refresh); streamed output does not.
        if !result.duplicate
            && (result.terminal || result.task_status != "running" || provider_request)
        {
            self.announce_chat_activity(db, task_id).await?;
        }
        Ok(result)
    }

    async fn apply_locked(
        &self,
        db: &mut ScopedDb,
        event: TaskLifecycleEvent,
    ) -> Result<ApplyResult, AppError> {
        match event {
            TaskLifecycleEvent::Journal(event) => self.apply_journal_event(db, event).await,
            TaskLifecycleEvent::StartDisposition {
                task_id,
                attempt,
                vm_id,
                epoch,
                disposition,
            } => {
                self.apply_start_disposition(db, task_id, attempt, vm_id, epoch, &disposition)
                    .await
            }
            TaskLifecycleEvent::VmLost {
                task_id,
                attempt,
                vm_id,
                epoch,
                evidence,
            } => {
                self.apply_vm_loss(db, task_id, attempt, vm_id, epoch, evidence)
                    .await
            }
            TaskLifecycleEvent::RecoveredOutcome {
                task_id,
                attempt,
                vm_id,
                epoch,
                outcome,
            } => {
                self.apply_recovered_outcome(db, task_id, attempt, vm_id, epoch, outcome)
                    .await
            }
        }
    }

    /// The owner's End for a Claude background wait on the session's running
    /// attempt. The live execution stops Claude's background work between
    /// turns and records a notice. The task is not cancelled: a clean exit
    /// succeeds, and a bounded stop settles interrupted and holds the task
    /// for Continue like Stop. The End names the attempt whose wait the owner
    /// saw, so a delayed or retried End never reaches the next attempt.
    /// Nothing durable changes here: a delivery that fails leaves the wait
    /// as it was, and repeating End is harmless.
    pub async fn end_session_background_wait(
        &self,
        mut db: ScopedDb,
        actor_user_id: Uuid,
        session_id: Uuid,
        expected_task_id: Uuid,
        expected_attempt: i32,
    ) -> Result<(), AppError> {
        let row = sqlx::query(
            "SELECT task.id, task.attempt, task.workspace_id, task.vm_id, task.epoch,
                    workspace.owner_user_id, attempt_row.policy_snapshot_ref
             FROM tasks AS task
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             LEFT JOIN task_attempts AS attempt_row
               ON attempt_row.org_id = task.org_id AND attempt_row.task_id = task.id
              AND attempt_row.attempt = task.attempt
             WHERE task.chat_session_id = $1
               AND task.kind = 'agent'
               AND task.ended_at IS NULL
               AND task.status = 'running'
               AND task.vm_id IS NOT NULL AND task.epoch IS NOT NULL
             ORDER BY task.created_at DESC, task.id DESC
             LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| conflict("session has no running agent attempt"))?;
        db.commit().await?;
        let owner_user_id: Option<Uuid> = row.try_get("owner_user_id")?;
        if owner_user_id != Some(actor_user_id) {
            return Err(AppError::from_error(G5Error::Forbidden {
                missing_scopes: Vec::new(),
            }));
        }
        // Only Claude keeps an attempt live for background work; Codex ends
        // its own background work with its turn. An attempt whose snapshot
        // does not name Claude has no wait to end.
        let snapshot: Option<Value> = row.try_get("policy_snapshot_ref")?;
        if snapshot_launch_value(snapshot.as_ref(), "provider") != Some("claude_code") {
            return Err(conflict(
                "the running agent attempt has no background wait to end",
            ));
        }
        let task_id: Uuid = row.try_get("id")?;
        let attempt: i32 = row.try_get("attempt")?;
        if (task_id, attempt) != (expected_task_id, expected_attempt) {
            // The wait the owner saw already ended; a delayed or retried End
            // must not stop the session's next attempt.
            return Err(conflict("that background wait has already ended"));
        }
        let workspace_id: Uuid = row.try_get("workspace_id")?;
        let vm_id = Uuid::parse_str(row.try_get::<String, _>("vm_id")?.as_str())
            .map_err(|_| conflict("workspace VM id is invalid"))?;
        let epoch = u64::try_from(row.try_get::<i64, _>("epoch")?)
            .map_err(|_| conflict("task epoch is negative"))?;
        let handle = self
            .workspace_runtime
            .owner_handle(workspace_id, actor_user_id)
            .await
            .map_err(AppError::from_error)?;
        if handle.vm_id != vm_id || handle.epoch != epoch {
            return Err(conflict("agent execution is stale"));
        }
        let request_id = Uuid::new_v4().to_string();
        let reply = self
            .workspace_runtime
            .modules_background_end(
                &handle,
                &RuntimeRequest::lifecycle(
                    workspace_id,
                    vm_id,
                    epoch,
                    unix_time_ms().saturating_add(15_000),
                    request_id.clone(),
                ),
                json!({
                    "task_id": task_id,
                    "attempt": attempt,
                    "exec_id": canonical_exec_id(task_id, attempt, vm_id),
                    "request_id": request_id,
                }),
            )
            .await
            .map_err(runtime_app_error)?;
        match reply["disposition"].as_str() {
            Some("delivered") => Ok(()),
            Some(disposition) => Err(conflict(format!(
                "the agent attempt did not take the end request: {disposition}"
            ))),
            None => Err(conflict("dispatcher end disposition is absent")),
        }
    }

    /// Accept an owner response durably before touching the VM.  A lost RPC
    /// response leaves `reply_pending`; the reconciler can safely redeliver the
    /// same immutable request id and digest through the dispatcher primitive.
    ///
    /// The route's authorization transaction holds the workspace row `FOR
    /// SHARE` (`require_current_workspace_owner`); claiming in that same
    /// transaction would upgrade it to `FOR UPDATE`, and two concurrent owner
    /// replies then deadlock on each other's shared lock. End the
    /// authorization transaction first and claim in a fresh one that takes
    /// the exclusive locks directly; the claim re-checks current ownership
    /// under those locks, so a transfer committing in between is still seen.
    /// Whether a native request still awaits its first acceptance. A reply
    /// that only replays an accepted answer must not record another Start
    /// (`g5.chat.start-and-send`); an unknown request counts as pending so
    /// the claim reports the real refusal.
    pub async fn native_request_pending(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        attempt: i32,
        provider_request_id: &str,
    ) -> Result<bool, AppError> {
        let pending: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT status = 'pending' FROM provider_requests
             WHERE task_id = $1 AND attempt = $2 AND provider_request_id = $3), true)",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(provider_request_id)
        .fetch_one(db.conn())
        .await?;
        Ok(pending)
    }

    pub async fn reply_to_native_request(
        &self,
        db: ScopedDb,
        actor_user_id: Uuid,
        input: NativeReplyInput,
    ) -> Result<NativeReplyOutcome, AppError> {
        let reopener = db.reopener();
        db.commit().await?;
        let mut db = reopener.open().await?;
        let claim = claim_native_reply(&mut db, actor_user_id, input).await?;
        // The claimed request no longer waits for the person, whether the
        // answer resumes the turn here or travels to the attempt
        // (chat g5.chat.activity.awaiting-input).
        self.announce_chat_activity(&mut db, claim.task_id).await?;
        db.commit().await?;
        self.deliver_claimed_native_reply(&claim).await
    }

    pub async fn deliver_claimed_native_reply(
        &self,
        claim: &ClaimedNativeReply,
    ) -> Result<NativeReplyOutcome, AppError> {
        if let Some(disposition) = claim.local_disposition {
            return Ok(NativeReplyOutcome {
                task_id: claim.task_id,
                attempt: claim.attempt,
                provider_request_id: claim.provider_request_id.clone(),
                request_id: claim.request_id.clone(),
                response_digest: claim.response_digest.clone(),
                delivery_disposition: disposition.to_owned(),
            });
        }
        let handle = self
            .workspace_runtime
            .owner_handle(claim.workspace_id, claim.owner_user_id)
            .await
            .map_err(AppError::from_error)?;
        if handle.org_id != claim.org_id
            || handle.vm_id != claim.vm_id
            || handle.epoch != claim.epoch
        {
            return Err(conflict("provider request execution is stale"));
        }
        let exec_id = canonical_exec_id(claim.task_id, claim.attempt, claim.vm_id);
        let request = RuntimeRequest::lifecycle(
            claim.workspace_id,
            claim.vm_id,
            claim.epoch,
            unix_time_ms().saturating_add(15_000),
            claim.request_id.clone(),
        );
        let reply = self
            .workspace_runtime
            .modules_input_respond(
                &handle,
                &request,
                json!({
                    "task_id": claim.task_id,
                    "attempt": claim.attempt,
                    "exec_id": exec_id,
                    "provider_request_id": claim.provider_request_id,
                    "request_id": claim.request_id,
                    "response_digest": claim.response_digest,
                    "validated_response": claim.validated_response,
                }),
            )
            .await
            .map_err(runtime_app_error)?;
        let disposition = reply["disposition"]
            .as_str()
            .ok_or_else(|| conflict("dispatcher reply disposition is absent"))?;
        if !matches!(disposition, "delivered" | "already_delivered") {
            return Err(conflict(format!(
                "provider response was not delivered: {disposition}"
            )));
        }
        Ok(NativeReplyOutcome {
            task_id: claim.task_id,
            attempt: claim.attempt,
            provider_request_id: claim.provider_request_id.clone(),
            request_id: claim.request_id.clone(),
            response_digest: claim.response_digest.clone(),
            delivery_disposition: disposition.to_string(),
        })
    }

    /// Lock and rehydrate accepted replies whose transport result was lost.
    /// The caller commits the scoped transaction before redelivering them.
    pub async fn claim_pending_native_replies(
        &self,
        db: &mut ScopedDb,
        limit: i64,
    ) -> Result<Vec<ClaimedNativeReply>, AppError> {
        let rows = sqlx::query(
            "SELECT request.org_id, task.workspace_id, workspace.owner_user_id,
                    request.task_id, request.attempt, task.vm_id, task.epoch,
                    request.provider_request_id, request.response
             FROM provider_requests AS request
             JOIN tasks AS task
               ON task.org_id = request.org_id AND task.id = request.task_id
             JOIN workspaces AS workspace
               ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
             WHERE request.status = 'reply_pending'
               AND (request.response_deadline_at IS NULL OR request.response_deadline_at > now())
               AND request.resume_session_id IS NULL
               AND task.status = 'running' AND task.attempt = request.attempt
               AND task.vm_id IS NOT NULL AND task.epoch IS NOT NULL
               AND request.response_author_id = workspace.owner_user_id
             ORDER BY request.created_at, request.task_id
             FOR UPDATE OF request, task SKIP LOCKED
             LIMIT $1",
        )
        .bind(limit.clamp(1, 128))
        .fetch_all(db.conn())
        .await?;
        rows.into_iter()
            .map(|row| {
                let response: Value = row.try_get("response")?;
                let vm_id = Uuid::parse_str(row.try_get::<String, _>("vm_id")?.as_str())
                    .map_err(|_| sqlx::Error::Protocol("workspace VM id is invalid".to_string()))?;
                let epoch = u64::try_from(row.try_get::<i64, _>("epoch")?)
                    .map_err(|_| sqlx::Error::Protocol("task epoch is negative".to_string()))?;
                Ok(ClaimedNativeReply {
                    local_disposition: None,
                    org_id: row.try_get("org_id")?,
                    workspace_id: row.try_get("workspace_id")?,
                    owner_user_id: row.try_get("owner_user_id")?,
                    task_id: row.try_get("task_id")?,
                    attempt: row.try_get("attempt")?,
                    vm_id,
                    epoch,
                    provider_request_id: row.try_get("provider_request_id")?,
                    request_id: response["request_id"]
                        .as_str()
                        .ok_or_else(|| {
                            sqlx::Error::Protocol(
                                "pending provider response has no request id".to_string(),
                            )
                        })?
                        .to_string(),
                    response_digest: response["digest"]
                        .as_str()
                        .ok_or_else(|| {
                            sqlx::Error::Protocol(
                                "pending provider response has no digest".to_string(),
                            )
                        })?
                        .to_string(),
                    validated_response: response.get("body").cloned().ok_or_else(|| {
                        sqlx::Error::Protocol("pending provider response has no body".to_string())
                    })?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Make API-clock expiry durable before asking the dispatcher to stop the
    /// process. A reply and timeout race on the provider-request row; only the
    /// winner can change `pending|reply_pending`.
    pub async fn claim_expired_native_requests(
        &self,
        db: &mut ScopedDb,
        limit: i64,
    ) -> Result<Vec<NativeRequestTimeout>, AppError> {
        let rows = sqlx::query(
            "SELECT request.org_id, task.workspace_id, request.task_id,
                    request.attempt, task.vm_id, task.epoch,
                    request.provider_request_id
             FROM provider_requests AS request
             JOIN tasks AS task
               ON task.org_id = request.org_id AND task.id = request.task_id
             WHERE request.status IN ('pending','reply_pending')
               AND request.response_deadline_at <= now()
               AND task.status = 'running' AND task.attempt = request.attempt
               AND task.vm_id IS NOT NULL AND task.epoch IS NOT NULL
             ORDER BY request.response_deadline_at, request.task_id
             FOR UPDATE OF request, task SKIP LOCKED
             LIMIT $1",
        )
        .bind(limit.clamp(1, 128))
        .fetch_all(db.conn())
        .await?;
        let mut claimed = Vec::with_capacity(rows.len());
        for row in rows {
            let task_id: Uuid = row.try_get("task_id")?;
            let attempt: i32 = row.try_get("attempt")?;
            let provider_request_id: String = row.try_get("provider_request_id")?;
            let invalidated = sqlx::query(
                "UPDATE provider_requests
                 SET status = 'invalidated', ended_at = now(),
                     end_reason = 'response_timeout'
                 WHERE task_id = $1 AND attempt = $2 AND provider_request_id = $3
                   AND status IN ('pending','reply_pending')",
            )
            .bind(task_id)
            .bind(attempt)
            .bind(&provider_request_id)
            .execute(db.conn())
            .await?;
            if invalidated.rows_affected() != 1 {
                continue;
            }
            sqlx::query(
                "UPDATE tasks
                 SET spec = jsonb_set(
                         spec, '{lifecycle}',
                         COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                             jsonb_build_object(
                                 'interrupt_reason','response_timeout',
                                 'provider_request_id',$3::text,
                                 'interrupt_requested_at',now()),
                         true),
                     updated_at = now()
                 WHERE id = $1 AND attempt = $2 AND status = 'running'",
            )
            .bind(task_id)
            .bind(attempt)
            .bind(&provider_request_id)
            .execute(db.conn())
            .await?;
            claimed.push(NativeRequestTimeout {
                org_id: row.try_get("org_id")?,
                workspace_id: row.try_get("workspace_id")?,
                task_id,
                attempt,
                vm_id: Uuid::parse_str(row.try_get::<String, _>("vm_id")?.as_str())
                    .map_err(|_| conflict("workspace VM identity is invalid"))?,
                epoch: u64::try_from(row.try_get::<i64, _>("epoch")?)
                    .map_err(|_| conflict("task epoch is invalid"))?,
                provider_request_id,
            });
        }
        Ok(claimed)
    }

    pub async fn cancel_expired_native_request(
        &self,
        timeout: &NativeRequestTimeout,
    ) -> Result<(), AppError> {
        let Some(handle) = self
            .workspace_runtime
            .get(timeout.workspace_id)
            .await
            .map_err(AppError::from_error)?
        else {
            return Ok(());
        };
        if handle.org_id != timeout.org_id
            || handle.vm_id != timeout.vm_id
            || handle.epoch != timeout.epoch
        {
            return Ok(());
        }
        let exec_id = canonical_exec_id(timeout.task_id, timeout.attempt, timeout.vm_id);
        self.workspace_runtime
            .modules_cancel(
                &handle,
                &RuntimeRequest::lifecycle(
                    timeout.workspace_id,
                    timeout.vm_id,
                    timeout.epoch,
                    unix_time_ms().saturating_add(15_000),
                    format!(
                        "native-timeout:{}:{}:{}",
                        timeout.task_id, timeout.attempt, timeout.provider_request_id
                    ),
                ),
                &exec_id,
                "response_timeout",
            )
            .await
            .map_err(runtime_app_error)?;
        Ok(())
    }

    async fn apply_journal_event(
        &self,
        db: &mut ScopedDb,
        event: JournalTaskEvent,
    ) -> Result<ApplyResult, AppError> {
        event.validate_identity()?;
        let mut segments = super::segment_map::SegmentMap::with_current(
            event.task_id,
            event.attempt,
            current_assistant_message_id(db, event.task_id, event.attempt).await?,
        );
        let result = self
            .apply_journal_batch_locked(
                db,
                &mut segments,
                vec![JournalBatchEvent {
                    event,
                    received_at: Utc::now(),
                    message_id: None,
                }],
            )
            .await?;
        Ok(ApplyResult {
            duplicate: result.applied == 0,
            ack_through: result.ack_through,
            task_status: result.task_status,
            terminal: result.terminal,
        })
    }

    /// Store one contiguous run of an attempt's journal events in the
    /// caller's transaction (`g5.chat.history-batches`): the attempt is
    /// locked once, every row is inserted in one statement, each touched
    /// assistant segment is refreshed once, the terminal event settles the
    /// attempt and the persisted prefix advances once. Events at or below
    /// the prefix are ignored; a gap or an event after the terminal one is
    /// refused without storing anything. `segments` is the caller's
    /// projection of which assistant segment owns each unit, the same one the
    /// live copies were published with; it is advanced here and seeded from
    /// persisted rows for units it has not seen.
    pub async fn apply_journal_batch(
        &self,
        db: &mut ScopedDb,
        segments: &mut super::segment_map::SegmentMap,
        events: Vec<JournalBatchEvent>,
    ) -> Result<BatchApplyResult, BatchApplyError> {
        let Some(first) = events.first() else {
            return Err(inconsistent("journal batch is empty"));
        };
        let task_id = first.event.task_id;
        let provider_request = events
            .iter()
            .any(|entry| entry.event.kind.starts_with("provider_request."));
        lock_agent_task_session_turn_allocation(db, task_id).await?;
        lock_preview_vm_row(db, task_id).await?;
        let result = self
            .apply_journal_batch_locked(db, segments, events)
            .await?;
        if result.applied > 0
            && (result.terminal || result.task_status != "running" || provider_request)
        {
            self.announce_chat_activity(db, task_id).await?;
        }
        Ok(result)
    }

    async fn apply_journal_batch_locked(
        &self,
        db: &mut ScopedDb,
        segments: &mut super::segment_map::SegmentMap,
        events: Vec<JournalBatchEvent>,
    ) -> Result<BatchApplyResult, BatchApplyError> {
        let Some(first) = events.first() else {
            return Err(inconsistent("journal batch is empty"));
        };
        let (task_id, attempt, vm_id, epoch) = (
            first.event.task_id,
            first.event.attempt,
            first.event.vm_id,
            first.event.epoch,
        );
        let mut events = events;
        for entry in &mut events {
            entry.event.replace_nul();
            entry.event.validate_identity()?;
            if entry.event.task_id != task_id
                || entry.event.attempt != attempt
                || entry.event.vm_id != vm_id
                || entry.event.epoch != epoch
            {
                return Err(inconsistent("journal batch mixes execution identities"));
            }
        }
        if segments.task_id() != task_id || segments.attempt() != attempt {
            return Err(inconsistent("segment map belongs to another attempt"));
        }
        // An identity the lock refuses (unknown, stale or replaced attempt) is
        // the batch's inconsistency, not a storage failure.
        let locked = match lock_attempt(db, task_id, attempt, vm_id, epoch).await {
            Ok(locked) => locked,
            Err(error) if matches!(error.inner, G5Error::Conflict { .. }) => {
                return Err(inconsistent(error.to_string()));
            }
            Err(error) => return Err(error.into()),
        };
        let persisted = locked.acked_event_seq;
        let total = events.len() as u64;
        let pending: Vec<JournalBatchEvent> = events
            .into_iter()
            .filter(|entry| entry.event.seq > persisted)
            .collect();
        let duplicates = total - pending.len() as u64;
        if pending.is_empty() {
            return Ok(BatchApplyResult {
                applied: 0,
                duplicates,
                ack_through: Some(persisted),
                terminal: is_terminal_status(&locked.task_status),
                task_status: locked.task_status,
                live: Vec::new(),
            });
        }
        let mut expected = persisted.saturating_add(1);
        for entry in &pending {
            if entry.event.seq != expected {
                return Err(inconsistent(format!(
                    "task event sequence gap for {task_id} attempt {attempt}: expected {expected}, received {}",
                    entry.event.seq
                )));
            }
            expected = expected.saturating_add(1);
        }
        if let Some(position) = pending.iter().position(|entry| entry.event.terminal) {
            if position + 1 != pending.len() {
                return Err(inconsistent(
                    "journal batch carries events after its terminal event",
                ));
            }
        }
        if locked.ended {
            return Err(inconsistent("task attempt already settled"));
        }
        seed_segment_owners(db, segments, &pending).await?;

        // Pass one, in event order: deterministic rejections, steering
        // evidence (which may start the next segment) and the owning
        // segment of every event. Module output is untrusted: a schema,
        // size or state rejection consumes its sequence as an error row, or
        // every pass would retry it forever (`g5.chat.native-request-boundary`).
        let mut prepared: Vec<Prepared> = Vec::with_capacity(pending.len());
        // A provider request id reused within one batch is caught here; the
        // per-event check only sees ids already stored.
        let mut opened_in_batch: std::collections::HashSet<String> = Default::default();
        for entry in pending {
            let mut projection_error = journal_projection_error(db, &locked, &entry.event).await?;
            if projection_error.is_none() && entry.event.kind == "provider_request.opened" {
                let id = entry.event.payload["provider_request_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                if !opened_in_batch.insert(id) {
                    projection_error = Some(
                        "provider request event rejected: provider_request_id was already used in this attempt"
                            .to_string(),
                    );
                }
            }
            if projection_error.is_none() && entry.event.kind == "input.shipped" {
                apply_input_shipped(db, &locked, &entry.event).await?;
                segments.observe_shipped(&entry.event.payload);
                // Persisted rows decide the current segment when the
                // evidence did not create one (repeated or superseded flush).
                segments.set_current(current_assistant_message_id(db, task_id, attempt).await?);
            }
            let message_id = match entry.message_id {
                Some(published) => {
                    if let Some((_, unit)) = super::segment_map::SegmentMap::unit_of(
                        &entry.event.kind,
                        &entry.event.payload,
                    ) {
                        segments.learn(&entry.event.kind, &unit, published);
                    }
                    published
                }
                None => segments.assign(&entry.event.kind, &entry.event.payload),
            };
            prepared.push(Prepared {
                event: entry.event,
                received_at: entry.received_at,
                message_id,
                projection_error,
            });
        }

        // One statement stores every row of the batch.
        let mut ids = Vec::with_capacity(prepared.len());
        let mut org_ids = Vec::with_capacity(prepared.len());
        let mut task_ids = Vec::with_capacity(prepared.len());
        let mut entry_types: Vec<String> = Vec::with_capacity(prepared.len());
        let mut contents = Vec::with_capacity(prepared.len());
        let mut metadata: Vec<Value> = Vec::with_capacity(prepared.len());
        let mut attempts = Vec::with_capacity(prepared.len());
        let mut vm_ids = Vec::with_capacity(prepared.len());
        let mut seqs = Vec::with_capacity(prepared.len());
        let mut created: Vec<DateTime<Utc>> = Vec::with_capacity(prepared.len());
        for item in &prepared {
            let mut entry = journal_timeline_entry(
                &item.event,
                item.message_id,
                locked.chat_session_id,
                item.received_at,
                false,
            );
            if let Some(error) = &item.projection_error {
                entry.entry_type = "error".to_string();
                entry.content = error.clone();
                entry.metadata["projection_error"] = Value::String(error.clone());
            }
            ids.push(entry.id);
            org_ids.push(locked.org_id);
            task_ids.push(task_id);
            entry_types.push(entry.entry_type);
            contents.push(entry.content);
            metadata.push(entry.metadata);
            attempts.push(attempt);
            vm_ids.push(vm_id.to_string());
            seqs.push(
                i64::try_from(item.event.seq)
                    .map_err(|_| conflict("event sequence exceeds bigint"))?,
            );
            created.push(item.received_at);
        }
        let inserted: Vec<i64> = sqlx::query_scalar(
            "INSERT INTO task_events(
                 id, org_id, task_id, entry_type, content, metadata,
                 attempt, vm_id, seq, created_at)
             SELECT * FROM UNNEST(
                 $1::uuid[], $2::uuid[], $3::uuid[], $4::text[], $5::text[], $6::jsonb[],
                 $7::int4[], $8::text[], $9::int8[], $10::timestamptz[])
             ON CONFLICT (task_id, attempt, vm_id, seq) DO NOTHING
             RETURNING seq",
        )
        .bind(&ids)
        .bind(&org_ids)
        .bind(&task_ids)
        .bind(&entry_types)
        .bind(&contents)
        .bind(&metadata)
        .bind(&attempts)
        .bind(&vm_ids)
        .bind(&seqs)
        .bind(&created)
        .fetch_all(db.conn())
        .await
        .map_err(event_content_rejection)?;
        if inserted.len() != prepared.len() {
            // The attempt row was locked and its watermark was behind these
            // sequences. An existing row therefore proves inconsistent
            // state, not a harmless duplicate that may advance the watermark.
            return Err(inconsistent(
                "task event exists beyond its acknowledgement watermark",
            ));
        }

        // Pass two, in event order: per-kind effects. Streaming marks and
        // segment refreshes are deferred to once per touched segment; the
        // terminal outcome applies them first and settles last.
        let mut streaming: std::collections::BTreeSet<Uuid> = Default::default();
        let mut refresh: std::collections::BTreeSet<Uuid> = Default::default();
        let mut live_kinds: Vec<(String, Value, Uuid)> = Vec::new();
        let mut demoted: std::collections::HashMap<usize, String> = Default::default();
        for (index, item) in prepared.iter().enumerate() {
            let event = &item.event;
            if !is_routine_stream_event(&event.kind) {
                live_kinds.push((event.kind.clone(), event.payload.clone(), item.message_id));
            }
            if item.projection_error.is_some() {
                continue;
            }
            match event.kind.as_str() {
                "input.accepted" => apply_input_accepted(db, &locked, event).await?,
                "input.steer_rejected" => {
                    super::agent_steering::apply_disposition(
                        db,
                        task_id,
                        attempt,
                        &event.payload,
                        super::agent_steering::SteerDisposition::NotSteerable,
                    )
                    .await?;
                }
                "provider.session" => apply_provider_session(db, &locked, event).await?,
                "message.delta" if locked.task_kind == TaskKind::Agent => {
                    streaming.insert(item.message_id);
                }
                "message.final" if locked.task_kind == TaskKind::Agent => {
                    refresh.insert(item.message_id);
                }
                "provider_request.opened"
                | "provider_request.replied"
                | "provider_request.invalidated" => {
                    if let Some(evidence) = apply_provider_request_event(db, &locked, event).await?
                    {
                        demote_journal_event_to_error(db, event, &evidence).await?;
                        demoted.insert(index, evidence);
                    }
                }
                "invocation.outcome" => {
                    flush_segment_effects(
                        db,
                        &locked,
                        task_id,
                        attempt,
                        &mut streaming,
                        &mut refresh,
                    )
                    .await?;
                    apply_outcome(db, &locked, event, false).await?;
                }
                "module.preview.ready" if locked.task_kind == TaskKind::Preview => {
                    // The port the guest listens on is the relay's target
                    // and the owner's "Serving on port"; a preview recorded
                    // from the committed configuration may have launched
                    // from a saved working tree that names another
                    // (`g5.preview.start-and-preview`).
                    if let Some(port) = event.payload["port"]
                        .as_u64()
                        .and_then(|port| u16::try_from(port).ok())
                        .filter(|port| *port > 0)
                    {
                        sqlx::query(
                            "UPDATE tasks
                             SET spec = jsonb_set(spec, '{preview,ready_port}', to_jsonb($2::int))
                             WHERE id = $1",
                        )
                        .bind(task_id)
                        .bind(i32::from(port))
                        .execute(db.conn())
                        .await?;
                    }
                    settle_preview_intent(db, locked.workspace_id, task_id, "running").await?;
                }
                "session.title" if locked.task_kind == TaskKind::Agent => {
                    apply_session_title(db, &locked, event).await?;
                }
                _ => {}
            }
        }
        flush_segment_effects(db, &locked, task_id, attempt, &mut streaming, &mut refresh).await?;

        let last_seq = prepared
            .last()
            .map(|item| item.event.seq)
            .unwrap_or(persisted);
        let advanced = sqlx::query(
            "UPDATE task_attempts
             SET acked_event_seq = $4
             WHERE task_id = $1 AND attempt = $2 AND vm_id = $3
               AND acked_event_seq = $5",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(i64::try_from(last_seq).map_err(|_| conflict("event sequence exceeds bigint"))?)
        .bind(i64::try_from(persisted).unwrap_or(i64::MAX))
        .execute(db.conn())
        .await?;
        if advanced.rows_affected() != 1 {
            return Err(inconsistent("task event watermark moved under the batch"));
        }
        let task_status = sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(db.conn())
            .await?;
        // The stored rows of the boundary kinds, published after commit so
        // the browser replaces its provisional copy with the row it can act
        // on: an accepted provider request carries its request status, a
        // rejected one is the error row the transcript will show
        // (`g5.chat.live-delivery`).
        let mut stored_rows = stored_boundary_rows(&locked, &prepared, &demoted);
        attach_native_request_status(db, task_id, attempt, &mut stored_rows).await?;
        let mut live: Vec<(Channel, WsEvent)> = Vec::with_capacity(stored_rows.len() * 2 + 1);
        for entry in stored_rows {
            if let Some(session_id) = locked.chat_session_id {
                live.push((
                    Channel::Chat(session_id),
                    WsEvent::ChatTimeline {
                        session_id,
                        task_id,
                        entry: entry.clone(),
                    },
                ));
            }
            live.push((
                Channel::Task(task_id),
                WsEvent::TaskTimeline { task_id, entry },
            ));
        }
        live.extend(journal_batch_live_events(
            locked.org_id,
            locked.chat_session_id,
            task_id,
            attempt,
            &live_kinds,
            &task_status,
        ));
        Ok(BatchApplyResult {
            applied: prepared.len() as u64,
            duplicates,
            ack_through: Some(last_seq),
            terminal: is_terminal_status(&task_status),
            task_status,
            live,
        })
    }

    async fn apply_start_disposition(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        attempt: i32,
        vm_id: Uuid,
        epoch: u64,
        disposition: &Value,
    ) -> Result<ApplyResult, AppError> {
        let locked = lock_attempt(db, task_id, attempt, vm_id, epoch).await?;
        let name = disposition["disposition"]
            .as_str()
            .ok_or_else(|| conflict("module invocation disposition is absent"))?;
        if !matches!(
            name,
            "started" | "already_running" | "already_finished" | "cancelled"
        ) {
            return Err(conflict(format!(
                "unsupported module invocation disposition {name}"
            )));
        }
        let confirmed = matches!(name, "started" | "already_running" | "already_finished");
        if confirmed {
            sqlx::query(
                "UPDATE task_attempts SET start_confirmed = true
                 WHERE task_id = $1 AND attempt = $2 AND vm_id = $3 AND epoch = $4
                   AND ended_at IS NULL",
            )
            .bind(task_id)
            .bind(attempt)
            .bind(vm_id.to_string())
            .bind(i64::try_from(epoch).map_err(|_| conflict("epoch exceeds bigint"))?)
            .execute(db.conn())
            .await?;
            sqlx::query(
                "UPDATE tasks SET start_confirmed = true, updated_at = now()
                 WHERE id = $1 AND attempt = $2 AND vm_id = $3 AND epoch = $4
                   AND status = 'running'",
            )
            .bind(task_id)
            .bind(attempt)
            .bind(vm_id.to_string())
            .bind(i64::try_from(epoch).map_err(|_| conflict("epoch exceeds bigint"))?)
            .execute(db.conn())
            .await?;
        }
        if name == "cancelled" && !locked.ended && locked.task_status == "running" {
            let cancel_requested = locked
                .task_spec
                .pointer("/lifecycle/cancel_requested")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let decision = if auth_before_delivery(&locked) {
                final_decision("failed", "cancelled_before_start")
            } else if locked.task_kind == TaskKind::Agent && !cancel_requested {
                unstarted_agent_interruption_policy(
                    &locked.task_spec,
                    locked.continuation_reason.as_deref(),
                )
            } else {
                apply_interruption_policy(
                    outcome_decision(
                        locked.task_kind,
                        &json!({"status":"cancelled","detail":{"before_start":true}}),
                        cancel_requested,
                    )?,
                    locked.task_kind,
                    cancel_requested,
                    &locked.task_spec,
                )
            };
            sqlx::query(
                "UPDATE task_attempts
                 SET ended_at = now(), outcome = $4, reason = $5
                 WHERE task_id = $1 AND attempt = $2 AND vm_id = $3
                   AND ended_at IS NULL",
            )
            .bind(task_id)
            .bind(attempt)
            .bind(vm_id.to_string())
            .bind(disposition.clone())
            .bind(decision.attempt_reason)
            .execute(db.conn())
            .await?;
            // A preview cancelled before its module accepted the start still
            // says why (`g5.preview.service.stop-reason`); the attempt keeps
            // the raw disposition.
            let task_outcome = if locked.task_kind == TaskKind::Preview {
                preview_outcome_with_reason(
                    &json!({"status": decision.task_status}),
                    &locked.task_spec,
                    cancel_requested,
                )
            } else {
                disposition.clone()
            };
            settle_task_row(
                db,
                &locked,
                task_id,
                attempt,
                (vm_id, epoch),
                &decision,
                Some(task_outcome),
            )
            .await?;
            if auth_before_delivery(&locked) {
                materialize_auth_failure(db, &locked, task_id, attempt, None).await?;
            }
            release_unaccepted_input(db, task_id, attempt, decision.terminal).await?;
            if decision.terminal {
                settle_dependents(db, &locked, task_id, decision.task_status).await?;
                if locked.task_kind == TaskKind::Agent {
                    ensure_next_queued_agent_task(
                        db,
                        locked.org_id,
                        locked.workspace_id,
                        locked.chat_session_id,
                        task_id,
                    )
                    .await?;
                }
            } else {
                hand_off_held_chat_turn(db, &locked, task_id).await?;
            }
            invalidate_attempt_requests(db, task_id, attempt, "cancelled_before_start").await?;
            return Ok(ApplyResult {
                duplicate: false,
                ack_through: Some(locked.acked_event_seq),
                task_status: decision.task_status.to_string(),
                terminal: decision.terminal,
            });
        }
        Ok(ApplyResult {
            duplicate: locked.ended || locked.task_status != "running",
            ack_through: Some(locked.acked_event_seq),
            terminal: is_terminal_status(&locked.task_status),
            task_status: locked.task_status,
        })
    }

    async fn apply_vm_loss(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        attempt: i32,
        vm_id: Uuid,
        epoch: u64,
        evidence: Value,
    ) -> Result<ApplyResult, AppError> {
        let locked = lock_attempt(db, task_id, attempt, vm_id, epoch).await?;
        if locked.ended {
            return Ok(ApplyResult {
                duplicate: true,
                ack_through: Some(locked.acked_event_seq),
                terminal: is_terminal_status(&locked.task_status),
                task_status: locked.task_status,
            });
        }
        let cancel_requested = locked
            .task_spec
            .pointer("/lifecycle/cancel_requested")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let decision = if auth_before_delivery(&locked) {
            final_decision("failed", "vm_lost")
        } else if cancel_requested {
            final_decision("cancelled", "cancelled")
        } else {
            apply_interruption_policy(
                vm_loss_decision(locked.task_kind),
                locked.task_kind,
                false,
                &locked.task_spec,
            )
        };
        let attempt_outcome = if cancel_requested {
            json!({"status":"cancelled","evidence":evidence})
        } else {
            json!({"status":"vm_lost","evidence":evidence})
        };
        sqlx::query(
            "UPDATE task_attempts
             SET ended_at = now(), outcome = $4, reason = $5
             WHERE task_id = $1 AND attempt = $2 AND vm_id = $3 AND ended_at IS NULL",
        )
        .bind(task_id)
        .bind(attempt)
        .bind(vm_id.to_string())
        .bind(attempt_outcome)
        .bind(decision.attempt_reason)
        .execute(db.conn())
        .await?;
        // A lost preview ends for good; its task outcome says why
        // (`g5.preview.service.stop-reason`), as the module-reported path does.
        // A Close under way ends `cancelled` like a finished one, so the
        // outcome also records that the VM was lost before cleanup finished
        // (`g5.ui.activity.preview-row`).
        let task_outcome = (locked.task_kind == TaskKind::Preview).then(|| {
            if cancel_requested {
                json!({"status": decision.task_status, "reason": "owner_cancel",
                       "detail": {"close": "lost"}})
            } else {
                let reason = locked
                    .task_spec
                    .pointer("/lifecycle/interrupt_reason")
                    .and_then(Value::as_str)
                    .map_or("vm_lost", preview_stop_reason);
                json!({"status": decision.task_status, "reason": reason})
            }
        });
        settle_task_row(
            db,
            &locked,
            task_id,
            attempt,
            (vm_id, epoch),
            &decision,
            task_outcome,
        )
        .await?;
        if locked.task_kind == TaskKind::Agent {
            refresh_assistant_message_content(db, &locked, task_id, attempt).await?;
            settle_existing_assistant_message(db, &locked, task_id, attempt, &decision).await?;
        }
        if locked.task_kind == TaskKind::Preview {
            settle_preview_intent(db, locked.workspace_id, task_id, "settled").await?;
        }
        if locked.task_kind == TaskKind::Agent {
            if decision.terminal {
                if auth_before_delivery(&locked) {
                    materialize_auth_failure(db, &locked, task_id, attempt, None).await?;
                }
                release_unaccepted_input(db, task_id, attempt, true).await?;
            } else if mark_reserved_input_unknown(db, task_id, attempt, "vm_lost").await? {
                sqlx::query(
                    "UPDATE tasks SET hold_reason = 'delivery_unknown', updated_at = now()
                     WHERE id = $1 AND status = 'waiting' AND attempt = $2",
                )
                .bind(task_id)
                .bind(attempt)
                .execute(db.conn())
                .await?;
            }
        }
        if decision.terminal {
            settle_dependents(db, &locked, task_id, decision.task_status).await?;
            if locked.task_kind == TaskKind::Agent {
                ensure_next_queued_agent_task(
                    db,
                    locked.org_id,
                    locked.workspace_id,
                    locked.chat_session_id,
                    task_id,
                )
                .await?;
            }
        } else {
            hand_off_held_chat_turn(db, &locked, task_id).await?;
        }
        invalidate_attempt_requests(db, task_id, attempt, "vm_lost").await?;
        Ok(ApplyResult {
            duplicate: false,
            ack_through: Some(locked.acked_event_seq),
            task_status: decision.task_status.to_string(),
            terminal: decision.terminal,
        })
    }

    async fn apply_recovered_outcome(
        &self,
        db: &mut ScopedDb,
        task_id: Uuid,
        attempt: i32,
        vm_id: Uuid,
        epoch: u64,
        outcome: Value,
    ) -> Result<ApplyResult, AppError> {
        let locked = lock_attempt(db, task_id, attempt, vm_id, epoch).await?;
        if locked.ended || locked.task_status != "running" {
            return Ok(ApplyResult {
                duplicate: true,
                ack_through: Some(locked.acked_event_seq),
                terminal: is_terminal_status(&locked.task_status),
                task_status: locked.task_status,
            });
        }
        apply_outcome(
            db,
            &locked,
            &JournalTaskEvent {
                task_id,
                attempt,
                vm_id,
                epoch,
                exec_id: canonical_exec_id(task_id, attempt, vm_id),
                seq: locked.acked_event_seq.saturating_add(1),
                kind: "invocation.outcome".to_string(),
                payload: outcome,
                terminal: true,
            },
            true,
        )
        .await?;
        let task_status = sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(db.conn())
            .await?;
        Ok(ApplyResult {
            duplicate: false,
            ack_through: Some(locked.acked_event_seq),
            terminal: is_terminal_status(&task_status),
            task_status,
        })
    }
}

async fn lock_agent_task_session_turn_allocation(
    db: &mut ScopedDb,
    task_id: Uuid,
) -> Result<(), AppError> {
    let session_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT chat_session_id FROM tasks WHERE id = $1 AND kind = 'agent'",
    )
    .bind(task_id)
    .fetch_optional(db.conn())
    .await?
    .flatten();
    if let Some(session_id) = session_id {
        crate::services::agents::chat_service::lock_session_turn_allocation(db.conn(), session_id)
            .await?;
    }
    Ok(())
}

/// Deterministic rejections of one journaled provider fact become consumable
/// projection evidence (`g5.chat.native-request-boundary`). Shape rules mirror
/// the `provider_requests` persistence contract; the duplicate check reads
/// under the attempt lock, which serializes every insert for this attempt, so
/// its answer is stable through commit. Transient database failures propagate.
async fn journal_projection_error(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<Option<String>, AppError> {
    if let Some(error) = provider_request_shape_error(locked, event) {
        return Ok(Some(error));
    }
    if event.kind != "provider_request.opened" {
        return Ok(None);
    }
    let already_used = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM provider_requests
         WHERE task_id = $1 AND attempt = $2 AND provider_request_id = $3)",
    )
    .bind(event.task_id)
    .bind(event.attempt)
    .bind(event.payload["provider_request_id"].as_str().unwrap_or(""))
    .fetch_one(db.conn())
    .await?;
    if already_used {
        return Ok(Some(
            "provider request event rejected: provider_request_id was already used in this attempt"
                .to_string(),
        ));
    }
    Ok(None)
}

fn provider_request_shape_error(
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Option<String> {
    if event.kind != "provider_request.opened" {
        return None;
    }
    let reject = |reason: &str| Some(format!("provider request event rejected: {reason}"));
    if locked.task_kind != TaskKind::Agent {
        return reject("only agent tasks may open provider requests");
    }
    let string_field = |name: &str, max: usize| {
        event.payload[name]
            .as_str()
            .is_some_and(|value| !value.is_empty() && value.len() <= max && value.trim() == value)
    };
    if !string_field("provider_request_id", 512) {
        return reject("provider_request_id is absent or invalid");
    }
    if !matches!(
        event.payload["request_kind"].as_str(),
        Some("question" | "permission")
    ) {
        return reject("request_kind is unsupported");
    }
    for (field, label) in [("payload", "payload"), ("display", "display")] {
        let Some(value) = event.payload.get(field) else {
            return reject(&format!("{label} is absent"));
        };
        if serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > MAX_NATIVE_REQUEST_BYTES) {
            return reject(&format!("{label} exceeds {MAX_NATIVE_REQUEST_BYTES} bytes"));
        }
    }
    if event.payload["request_kind"] == "question" {
        if let Some(id) = event
            .payload
            .get("resume_session_id")
            .filter(|value| !value.is_null())
        {
            if !id
                .as_str()
                .is_some_and(|id| !id.is_empty() && id.len() <= 512)
            {
                return reject("native resume session is invalid");
            }
        }
        return None;
    }
    let Some(deadline_ms) = event.payload["deadline_unix_ms"].as_i64() else {
        return reject("deadline is absent");
    };
    if DateTime::<Utc>::from_timestamp_millis(deadline_ms).is_none() {
        return reject("deadline is invalid");
    }
    None
}

/// Apply one `provider_request.*` fact inside a savepoint. A deterministic
/// rejection — any applier conflict, including a storage rejection of the
/// event's own content ([`event_content_rejection`]) — rolls back to the
/// savepoint and returns bounded evidence so the caller records it and the
/// sequence is consumed (`g5.chat.native-request-boundary`). Everything else
/// (connection loss, serialization or deadlock victims) propagates, keeping
/// the transient-retry guarantee.
/// One batch event after pass one: its receipt time, owning segment and any
/// deterministic rejection.
struct Prepared {
    event: JournalTaskEvent,
    received_at: DateTime<Utc>,
    message_id: Uuid,
    projection_error: Option<String>,
}

/// The rows the batch stored for its boundary kinds, as the transcript
/// will read them: an error row where projection rejected the event (in
/// pass one, or by rolling back its applier).
fn stored_boundary_rows(
    locked: &LockedAttempt,
    prepared: &[Prepared],
    demoted: &std::collections::HashMap<usize, String>,
) -> Vec<TimelineEntryPayload> {
    prepared
        .iter()
        .enumerate()
        .filter(|(_, item)| !is_routine_stream_event(&item.event.kind))
        .map(|(index, item)| {
            let mut entry = journal_timeline_entry(
                &item.event,
                item.message_id,
                locked.chat_session_id,
                item.received_at,
                false,
            );
            let error = item.projection_error.clone().or_else(|| {
                demoted
                    .get(&index)
                    .map(|evidence| truncate_utf8(evidence, MAX_TIMELINE_CONTENT_BYTES))
            });
            if let Some(error) = error {
                entry.entry_type = "error".to_string();
                entry.content = error.clone();
                entry.metadata["projection_error"] = Value::String(error);
            }
            entry
        })
        .collect()
}

/// Give each stored `provider_request.opened` row the request's status, end
/// reason and saved answer, as the timeline read does, so the browser can tell
/// an open request from one already answered in the same batch and can show
/// an undelivered or withdrawn question from the live republish alone
/// (`g5.chat.native-question-delivery-state`).
async fn attach_native_request_status(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    rows: &mut [TimelineEntryPayload],
) -> Result<(), AppError> {
    let ids: Vec<String> = rows
        .iter()
        .filter(|entry| {
            entry.entry_type != "error"
                && entry.metadata["kind"].as_str() == Some("provider_request.opened")
        })
        .filter_map(|entry| {
            entry.metadata["payload"]["provider_request_id"]
                .as_str()
                .map(str::to_string)
        })
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let statuses: Vec<(String, String, Option<String>, Option<Value>)> = sqlx::query_as(
        "SELECT provider_request_id, status, end_reason, response -> 'body'
         FROM provider_requests
         WHERE task_id = $1 AND attempt = $2 AND provider_request_id = ANY($3)",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(&ids)
    .fetch_all(db.conn())
    .await?;
    for entry in rows.iter_mut() {
        let Some(id) = entry.metadata["payload"]["provider_request_id"].as_str() else {
            continue;
        };
        if let Some((_, status, end_reason, answer)) =
            statuses.iter().find(|(request, ..)| request == id)
        {
            entry.metadata["nativeRequestStatus"] = Value::String(status.clone());
            entry.metadata["nativeRequestEndReason"] =
                end_reason.clone().map_or(Value::Null, Value::String);
            entry.metadata["nativeRequestAnswer"] = answer.clone().unwrap_or(Value::Null);
        }
    }
    Ok(())
}

async fn apply_provider_request_event(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<Option<String>, AppError> {
    sqlx::query("SAVEPOINT provider_request_apply")
        .execute(db.conn())
        .await?;
    let applied = match event.kind.as_str() {
        "provider_request.opened" => apply_provider_request_opened(db, locked, event).await,
        "provider_request.replied" => apply_provider_request_replied(db, locked, event).await,
        _ => apply_provider_request_invalidated(db, locked, event).await,
    };
    match applied {
        Ok(()) => {
            sqlx::query("RELEASE SAVEPOINT provider_request_apply")
                .execute(db.conn())
                .await?;
            Ok(None)
        }
        Err(error) => {
            let G5Error::Conflict { reason } = &error.inner else {
                return Err(error);
            };
            let evidence = format!("provider request event rejected: {reason}");
            sqlx::query("ROLLBACK TO SAVEPOINT provider_request_apply")
                .execute(db.conn())
                .await?;
            sqlx::query("RELEASE SAVEPOINT provider_request_apply")
                .execute(db.conn())
                .await?;
            Ok(Some(evidence))
        }
    }
}

/// Map a database rejection of this event's own content — SQLSTATE class 22
/// (data exception) or 23 (integrity violation), stable for the same event
/// under the attempt lock — to the deterministic conflict the savepoint
/// wrapper consumes as projection evidence. The evidence names only the
/// SQLSTATE and constraint, never row data. Anything else stays transient.
fn event_content_rejection(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(database_error) = &error {
        if let Some(code) = database_error.code() {
            if code.starts_with("22") || code.starts_with("23") {
                let constraint = database_error
                    .constraint()
                    .map(|name| format!(" {name}"))
                    .unwrap_or_default();
                return conflict(format!(
                    "storage refused the event (SQLSTATE {code}{constraint})"
                ));
            }
        }
    }
    error.into()
}

/// Rewrite an already stored journal event's timeline row into projection
/// error evidence after its applier was rolled back.
async fn demote_journal_event_to_error(
    db: &mut ScopedDb,
    event: &JournalTaskEvent,
    evidence: &str,
) -> Result<(), AppError> {
    let content = truncate_utf8(evidence, MAX_TIMELINE_CONTENT_BYTES);
    sqlx::query(
        "UPDATE task_events
         SET entry_type = 'error', content = $5,
             metadata = jsonb_set(metadata, '{projection_error}', to_jsonb($5::text), true)
         WHERE task_id = $1 AND attempt = $2 AND vm_id = $3 AND seq = $4",
    )
    .bind(event.task_id)
    .bind(event.attempt)
    .bind(event.vm_id.to_string())
    .bind(i64::try_from(event.seq).map_err(|_| conflict("event sequence exceeds bigint"))?)
    .bind(content)
    .execute(db.conn())
    .await?;
    Ok(())
}

async fn claim_native_reply(
    db: &mut ScopedDb,
    actor_user_id: Uuid,
    input: NativeReplyInput,
) -> Result<ClaimedNativeReply, AppError> {
    if input.attempt <= 0
        || input.request_id.is_empty()
        || input.request_id.len() > 128
        || input.provider_request_id.is_empty()
        || input.provider_request_id.len() > 512
    {
        return Err(conflict("invalid native reply identity"));
    }
    ensure_json_bound(
        &input.validated_response,
        MAX_NATIVE_REQUEST_BYTES,
        "validated response",
    )?;
    share_task_workspace(db, input.task_id).await?;
    // Owner mutations take workspace, then task, then request — never a task
    // and request row in one joined statement, whose plan-dependent order
    // could invert against journal projection, which holds the task/attempt
    // lock while it writes `provider_requests` rows.
    let task = sqlx::query(
        "SELECT task.org_id, task.workspace_id, workspace.owner_user_id,
                workspace.deleted_at IS NULL AS workspace_live,
                task.status AS task_status, task.attempt AS current_attempt,
                task.vm_id, task.epoch
         FROM tasks AS task
         JOIN workspaces AS workspace
           ON workspace.org_id = task.org_id AND workspace.id = task.workspace_id
         WHERE task.id = $1
         FOR UPDATE OF task",
    )
    .bind(input.task_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("provider request is absent"))?;
    let workspace_id: Uuid = task.try_get("workspace_id")?;
    // The route's authorization transaction committed before this claim (its
    // shared workspace lock must not be upgraded in place), so its admission
    // is stale: repeat it in full under the exclusive workspace lock before
    // anything durable changes. A deletion or transfer admitted in between
    // took the same workspace lock, so these reads are race-free, and
    // nothing may freeze an answer that admission refuses.
    if !task.try_get::<bool, _>("workspace_live")? {
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        }));
    }
    let owner_user_id = task
        .try_get::<Option<Uuid>, _>("owner_user_id")?
        .ok_or_else(|| conflict("workspace has no current owner"))?;
    if owner_user_id != actor_user_id {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    let transfer_pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_ownership_transfers
         WHERE workspace_id = $1 AND status = 'pending')",
    )
    .bind(workspace_id)
    .fetch_one(db.conn())
    .await?;
    if transfer_pending {
        return Err(AppError::from_error(G5Error::WorkspaceStopping));
    }
    let request = sqlx::query(
        "SELECT org_id, kind, status, resume_session_id, payload, response,
                response_deadline_at, end_reason
         FROM provider_requests
         WHERE task_id = $1 AND attempt = $2 AND provider_request_id = $3
         FOR UPDATE",
    )
    .bind(input.task_id)
    .bind(input.attempt)
    .bind(&input.provider_request_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("provider request is absent"))?;
    let org_id: Uuid = request.try_get("org_id")?;
    let native_resume = request
        .try_get::<Option<String>, _>("resume_session_id")?
        .is_some();
    let task_status: String = task.try_get("task_status")?;
    let status: String = request.try_get("status")?;
    let kind: String = request.try_get("kind")?;
    let payload: Value = request.try_get("payload")?;
    validate_native_response(&kind, &payload, &input.validated_response)?;
    let digest = response_digest(&input.validated_response)?;
    if status != "pending" {
        // An accepted answer never reopens the request. After owner
        // authorization, an identical retry replays the frozen acceptance
        // even when the request or execution has since ended
        // (`g5.chat.native-request-boundary`); a changed answer is rejected.
        let Some(response) = request.try_get::<Option<Value>, _>("response")? else {
            return Err(conflict(format!(
                "provider request is no longer answerable ({status})"
            )));
        };
        if response["digest"].as_str() != Some(digest.as_str())
            || response["body"] != input.validated_response
            || response["request_id"].as_str() != Some(input.request_id.as_str())
        {
            return Err(conflict(
                "provider request already accepted a different reply",
            ));
        }
        let live_redelivery = status == "reply_pending"
            && !native_resume
            && task_status == "running"
            && task.try_get::<i32, _>("current_attempt")? == input.attempt;
        if !live_redelivery {
            // An accepted answer replays from its stored key after settlement
            // (`g5.chat.native-request-boundary`). Only a request invalidated
            // because its saved session can no longer take the question —
            // restoration failed, the session advanced past it, or it was
            // never saved — is reported undeliverable, never newly queued
            // (`g5.chat.native-question-recovery`); ordinary settlement still
            // replays as queued for a later continuation.
            let undeliverable = status == "invalidated"
                && matches!(
                    request
                        .try_get::<Option<String>, _>("end_reason")?
                        .as_deref(),
                    Some(
                        "native_question_restore_failed"
                            | "native_session_advanced"
                            | "session_not_saved"
                    )
                );
            return Ok(ClaimedNativeReply {
                local_disposition: Some(if status == "responded" {
                    "already_delivered"
                } else if undeliverable {
                    "undeliverable"
                } else {
                    "queued"
                }),
                org_id,
                workspace_id,
                owner_user_id,
                task_id: input.task_id,
                attempt: input.attempt,
                vm_id: Uuid::nil(),
                epoch: 0,
                provider_request_id: input.provider_request_id,
                request_id: input.request_id,
                response_digest: digest,
                validated_response: input.validated_response,
            });
        }
        return Ok(ClaimedNativeReply {
            local_disposition: None,
            org_id,
            workspace_id,
            owner_user_id,
            task_id: input.task_id,
            attempt: input.attempt,
            vm_id: Uuid::parse_str(task.try_get::<String, _>("vm_id")?.as_str())
                .map_err(|_| conflict("workspace VM identity is invalid"))?,
            epoch: u64::try_from(task.try_get::<i64, _>("epoch")?)
                .map_err(|_| conflict("task epoch is invalid"))?,
            provider_request_id: input.provider_request_id,
            request_id: input.request_id,
            response_digest: digest,
            validated_response: input.validated_response,
        });
    }
    // First acceptance needs live admission; owner checks already passed.
    if native_resume {
        if !matches!(task_status.as_str(), "waiting" | "queued" | "running") {
            return Err(conflict("question task has ended"));
        }
        // Ready compute, or the Start consent this request recorded still
        // under way and unexpired (Start and answer, `g5.chat.start-and-send`):
        // both read from the one VM row under a share lock, so the ready,
        // expiry and cancellation transitions cannot interleave between them,
        // as the input binding's check does
        // (`agent_dispatch::bind_input_to_start_consent`).
        let (ready, starting) = sqlx::query_as::<_, (bool, bool)>(
            "SELECT (vm.status = 'running' AND vm.vm_id IS NOT NULL
                     AND vm.lease_expires_at > clock_timestamp()
                     AND workspace.deleted_at IS NULL
                     AND NOT EXISTS (SELECT 1 FROM workspace_ownership_transfers transfer
                         WHERE transfer.workspace_id = vm.workspace_id AND transfer.status = 'pending')),
                    ($2::text IS NOT NULL AND vm.start_intent ->> 'id' = $2
                     AND vm.start_intent ->> 'status' IN ('pending', 'queued_for_capacity', 'starting')
                     AND (vm.start_intent ->> 'expires_at')::timestamptz > clock_timestamp())
             FROM workspace_vms vm
             JOIN workspaces workspace ON workspace.id = vm.workspace_id
             WHERE vm.workspace_id = $1
             FOR SHARE OF vm",
        )
        .bind(workspace_id)
        .bind(input.pending_start.map(|intent_id| intent_id.to_string()))
        .fetch_optional(db.conn())
        .await?
        .unwrap_or((false, false));
        if !ready && !starting {
            return Err(conflict(
                "attach ready compute before answering the question",
            ));
        }
    } else if task_status != "running"
        || task.try_get::<i32, _>("current_attempt")? != input.attempt
    {
        return Err(conflict("provider request execution is not running"));
    }
    let vm_id = if native_resume {
        Uuid::nil()
    } else {
        Uuid::parse_str(task.try_get::<String, _>("vm_id")?.as_str())
            .map_err(|_| conflict("workspace VM identity is invalid"))?
    };
    let epoch = if native_resume {
        0
    } else {
        u64::try_from(task.try_get::<i64, _>("epoch")?)
            .map_err(|_| conflict("task epoch is invalid"))?
    };
    if request
        .try_get::<Option<DateTime<Utc>>, _>("response_deadline_at")?
        .is_some_and(|deadline| deadline <= Utc::now())
    {
        return Err(conflict("provider request response deadline elapsed"));
    }
    let updated = sqlx::query(
        "UPDATE provider_requests
         SET status = 'reply_pending', response_author_id = $5,
             response = jsonb_build_object(
                 'body',$4::jsonb,
                 'digest',$6,
                 'accepted_at',now(),
                 'request_id',$7)
         WHERE task_id = $1 AND attempt = $2 AND provider_request_id = $3
           AND status = 'pending'",
    )
    .bind(input.task_id)
    .bind(input.attempt)
    .bind(&input.provider_request_id)
    .bind(input.validated_response.clone())
    .bind(actor_user_id)
    .bind(&digest)
    .bind(&input.request_id)
    .execute(db.conn())
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict("provider request reply lost its acceptance race"));
    }
    if native_resume && task_status == "waiting" {
        sqlx::query(
            "UPDATE tasks SET status = 'queued', hold_reason = NULL,
            continuation_reason = 'suspended', spec = spec - 'lifecycle', updated_at = now()
            WHERE id = $1 AND status = 'waiting' AND hold_reason = 'awaiting_answer'",
        )
        .bind(input.task_id)
        .execute(db.conn())
        .await?;
    }
    Ok(ClaimedNativeReply {
        local_disposition: native_resume.then_some("queued"),
        org_id,
        workspace_id,
        owner_user_id,
        task_id: input.task_id,
        attempt: input.attempt,
        vm_id,
        epoch,
        provider_request_id: input.provider_request_id,
        request_id: input.request_id,
        response_digest: digest,
        validated_response: input.validated_response,
    })
}

async fn lock_attempt(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
) -> Result<LockedAttempt, AppError> {
    lock_attempt_identity(db, task_id, attempt, vm_id, epoch, true).await
}

// Only recorded steering completion may inspect an ended attempt. It still
// verifies its exact VM incarnation and never dispatches with historical authority.
async fn lock_attempt_identity(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    vm_id: Uuid,
    epoch: u64,
    require_current: bool,
) -> Result<LockedAttempt, AppError> {
    let row = sqlx::query(
        "SELECT task.org_id, task.workspace_id, task.kind, task.status,
                task.chat_session_id, task.continuation_reason, task.spec,
                attempt_row.policy_snapshot_ref, attempt_row.codex_auth_state,
                attempt_row.acked_event_seq, attempt_row.ended_at IS NOT NULL AS ended
         FROM tasks AS task
         JOIN task_attempts AS attempt_row
           ON attempt_row.org_id = task.org_id AND attempt_row.task_id = task.id
         WHERE task.id = $1
           AND (NOT $5 OR (task.attempt = $2 AND task.vm_id = $3 AND task.epoch = $4))
           AND attempt_row.attempt = $2 AND attempt_row.vm_id = $3
           AND attempt_row.epoch = $4
         FOR UPDATE OF task, attempt_row",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(vm_id.to_string())
    .bind(i64::try_from(epoch).map_err(|_| conflict("epoch exceeds bigint"))?)
    .bind(require_current)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("stale or unknown task execution"))?;
    let acked = row.try_get::<i64, _>("acked_event_seq")?;
    Ok(LockedAttempt {
        org_id: row.try_get("org_id")?,
        workspace_id: row.try_get("workspace_id")?,

        task_kind: row
            .try_get::<String, _>("kind")?
            .parse::<TaskKind>()
            .map_err(conflict)?,
        task_status: row.try_get("status")?,
        chat_session_id: row.try_get("chat_session_id")?,
        continuation_reason: row.try_get("continuation_reason")?,
        task_spec: row.try_get("spec")?,
        policy_snapshot: row.try_get("policy_snapshot_ref")?,
        codex_auth_state: row.try_get("codex_auth_state")?,
        acked_event_seq: u64::try_from(acked)
            .map_err(|_| conflict("negative attempt acknowledgement watermark"))?,
        ended: row.try_get("ended")?,
    })
}

/// Owner controls retain shared workspace authority through commit and never
/// upgrade it (`g5.chat.control-locks`): admission already holds the
/// workspace `FOR SHARE`, and a control that waited to upgrade behind another
/// control's share, while holding the session advisory lock that control
/// needs, deadlocked both (#966). The shared lock is taken here as well so a
/// direct service caller gets the same guarantee: transfer, stop, delete and
/// placement take the workspace exclusively and are excluded by it. Nothing
/// below writes the workspace row; `owner_user_id` is all it reads.
///
/// The canonical row-lock order across owner actions and journal projection
/// is: session turn allocation, workspace (shared), task and its attempt row,
/// then provider request rows. Implicit foreign-key KEY SHARE locks taken by
/// `provider_requests` writes land on an attempt row the same transaction
/// already holds, so they add no ordering edge.
/// A preview task's projection and controls write the VM row (its preview
/// intent), and the release that settles its work locks the VM row before
/// the tasks it settles. Every writer therefore takes the VM row before the
/// task row — workspace (shared), VM, then task — so a cancellation and a
/// readiness or outcome projection cannot wait on each other, and a
/// projection never writes the VM row of a workspace a transfer, stop or
/// delete holds exclusively (`g5.chat.control-locks`). The kind never
/// changes, so it is read without a lock; other kinds take nothing here.
async fn lock_preview_vm_row(db: &mut ScopedDb, task_id: Uuid) -> Result<(), AppError> {
    let preview_workspace: Option<Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM tasks WHERE id = $1 AND kind = 'preview'")
            .bind(task_id)
            .fetch_optional(db.conn())
            .await?;
    if let Some(workspace_id) = preview_workspace {
        // The one order every VM writer follows: workspace shared, VM, task.
        share_task_workspace(db, task_id).await?;
        sqlx::query("SELECT 1 FROM workspace_vms WHERE workspace_id = $1 FOR UPDATE")
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
    }
    Ok(())
}

async fn share_task_workspace(db: &mut ScopedDb, task_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "SELECT workspace.id FROM tasks task
         JOIN workspaces workspace ON workspace.org_id=task.org_id AND workspace.id=task.workspace_id
         WHERE task.id=$1 FOR SHARE OF workspace",
    )
    .bind(task_id)
    .fetch_optional(db.conn())
    .await?;
    Ok(())
}

async fn apply_input_accepted(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    if locked.task_kind != TaskKind::Agent {
        return Err(conflict("only agent tasks may accept session input"));
    }
    let input_id = payload_uuid(&event.payload, "input_id")?;
    let reservation_id = payload_uuid(&event.payload, "reservation_id")?;
    let revision = payload_i32(&event.payload, "revision")?;
    let delivery_generation = payload_i32(&event.payload, "delivery_generation")?;
    let system_notice_ids = payload_uuid_array(&event.payload, "system_notice_ids")?;
    let expected_notice_ids = payload_uuid_array(
        locked.task_spec.pointer("/agent").unwrap_or(&Value::Null),
        "systemNoticeIds",
    )?;
    if system_notice_ids != expected_notice_ids {
        return Err(conflict(
            "input.accepted system notices do not match the frozen reservation",
        ));
    }
    let row = sqlx::query(
        "UPDATE session_inputs
         SET delivery = 'delivered', delivered_at = now(),
             delivery_audit = delivery_audit || jsonb_build_array(
                 jsonb_build_object('event','input.accepted','seq',$8,
                                    'system_notice_ids',$9::jsonb))
         WHERE id = $1 AND org_id = $2 AND task_id = $3 AND attempt = $4
           AND delivery = 'reserved' AND reservation_id = $5
           AND reservation_revision = $6 AND revision = $6
           AND delivery_generation = $7
         RETURNING session_id, input_index, content, artifacts, input_origin, created_at",
    )
    .bind(input_id)
    .bind(locked.org_id)
    .bind(event.task_id)
    .bind(event.attempt)
    .bind(reservation_id)
    .bind(revision)
    .bind(delivery_generation)
    .bind(i64::try_from(event.seq).unwrap_or(i64::MAX))
    .bind(serde_json::to_value(&system_notice_ids)?)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("input.accepted does not match the frozen reservation"))?;
    let session_id: Uuid = row.try_get("session_id")?;
    if locked.chat_session_id != Some(session_id) {
        return Err(conflict("accepted input is not bound to the task session"));
    }
    if !system_notice_ids.is_empty() {
        let delivered = crate::services::agents::chat_service::deliver_reserved_notices(
            db.conn(),
            &system_notice_ids,
            session_id,
            event.task_id,
            event.attempt,
        )
        .await?;
        if delivered != system_notice_ids.len() as u64 {
            return Err(conflict(
                "input.accepted system notice delivery lost its state race",
            ));
        }
    }
    let delivered_content: String = row.try_get("content")?;
    let delivered_role = if row.try_get::<String, _>("input_origin")? == "continuation" {
        "system"
    } else {
        "user"
    };
    crate::services::agents::chat_service::record_accepted_input(
        db.conn(),
        crate::services::agents::chat_service::AcceptedInput {
            message: assistant_message(locked.org_id, session_id, event.task_id, event.attempt),
            input_id,
            role: delivered_role,
            content: &delivered_content,
            artifacts: row.try_get("artifacts")?,
            created_at: row.try_get("created_at")?,
        },
    )
    .await?;
    Ok(())
}

/// A positive RPC is independent delivery evidence. Normally journal order
/// projects it; after settlement there can be no more journal projection, so
/// materialize the retained input in the original attempt immediately.
pub(super) async fn record_steering_flush(
    db: &mut ScopedDb,
    claim: &super::agent_steering::ClaimedSteer,
) -> Result<(), AppError> {
    // This records an already-authorized execution fact, not a workspace
    // mutation. The session lock and exact attempt/input identity fence it.
    // Do not take workspace FOR UPDATE: a concurrent Send can hold FOR SHARE
    // while waiting for this session's turn-allocation lock.
    let locked = lock_attempt_identity(
        db,
        claim.task_id,
        claim.attempt,
        claim.vm_id,
        claim.epoch,
        false,
    )
    .await?;
    if locked.org_id != claim.org_id || locked.chat_session_id != Some(claim.session_id) {
        return Err(conflict("steering receipt session mismatch"));
    }
    let payload = claim.binding();
    let audit = json!([{
        "event":"steering.reserved", "reservation_id":claim.reservation_id,
        "task_id":claim.task_id, "attempt":claim.attempt,
        "revision":claim.revision, "delivery_generation":claim.delivery_generation,
    }]);
    let prior: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT task_id FROM session_inputs WHERE id=$1 AND session_id=$2 FOR UPDATE",
    )
    .bind(claim.input_id)
    .bind(claim.session_id)
    .fetch_optional(db.conn())
    .await?;
    let changed = sqlx::query(
        "UPDATE session_inputs SET steering_flushed_at=COALESCE(steering_flushed_at,now()),
            task_id=$2, attempt=$3, steered=true, reservation_id=$4, reservation_revision=$5,
            delivery='queued', withdrawn_at=NULL
         WHERE id=$1 AND session_id=$8 AND revision=$5 AND delivery_generation=$6
           AND delivery IN ('queued','withdrawn')
           AND ((task_id=$2 AND attempt=$3 AND reservation_id=$4 AND reservation_revision=$5 AND steered)
                OR delivery_audit @> $7::jsonb)")
        .bind(claim.input_id).bind(claim.task_id).bind(claim.attempt).bind(claim.reservation_id)
        .bind(claim.revision).bind(claim.delivery_generation).bind(audit).bind(claim.session_id)
        .execute(db.conn()).await?;
    if changed.rows_affected() == 0 {
        return Ok(());
    }
    if locked.ended {
        let previous = current_assistant_message(
            db,
            locked.org_id,
            claim.session_id,
            claim.task_id,
            claim.attempt,
        )
        .await?;
        let status: String = sqlx::query_scalar("SELECT status FROM chat_messages WHERE id=$1")
            .bind(previous.id)
            .fetch_one(db.conn())
            .await?;
        project_steered_input(db, &locked, claim.task_id, claim.attempt, &payload, None).await?;
        let mut message = assistant_message(
            locked.org_id,
            claim.session_id,
            claim.task_id,
            claim.attempt,
        );
        message.id = Uuid::new_v5(&claim.reservation_id, b"agent-steered-reply");
        crate::services::agents::chat_service::set_assistant_message_status(
            db.conn(),
            message,
            if matches!(status.as_str(), "pending" | "streaming") {
                "interrupted"
            } else {
                &status
            },
        )
        .await?;
        // A late receipt can remove the only input from the successor task.
        if let Some(Some(task)) = prior.filter(|task| *task != Some(claim.task_id)) {
            cancel_empty_steering_successor(db, task).await?;
        }
    }
    Ok(())
}

async fn cancel_empty_steering_successor(db: &mut ScopedDb, task: Uuid) -> Result<(), AppError> {
    let row = sqlx::query("UPDATE tasks SET status='cancelled', ended_at=now(), updated_at=now(),
        outcome=jsonb_build_object('status','cancelled','reason','input_already_delivered')
        WHERE id=$1 AND status='queued' AND continuation_reason IS NULL
          AND NOT EXISTS (SELECT 1 FROM task_attempts WHERE task_id=$1 AND ended_at IS NULL)
          AND NOT EXISTS (SELECT 1 FROM session_inputs WHERE task_id=$1 AND delivery IN ('queued','reserved','delivery_unknown'))
        RETURNING org_id,workspace_id,attempt")
        .bind(task).fetch_optional(db.conn()).await?;
    if let Some(row) = row {
        fail_task_dependents(
            db,
            row.try_get("org_id")?,
            row.try_get("workspace_id")?,
            task,
        )
        .await?;
        invalidate_attempt_requests(db, task, row.try_get("attempt")?, "input_already_delivered")
            .await?;
    }
    Ok(())
}

async fn settle_steering_inputs(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task: Uuid,
    attempt: i32,
) -> Result<(), AppError> {
    let receipts: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object(
        'input_id',id,'reservation_id',reservation_id,'revision',revision,'delivery_generation',delivery_generation)
        FROM session_inputs WHERE task_id=$1 AND attempt=$2 AND steered AND delivery='queued'
          AND steering_flushed_at IS NOT NULL ORDER BY input_index")
        .bind(task).bind(attempt).fetch_all(db.conn()).await?;
    for receipt in receipts {
        project_steered_input(db, locked, task, attempt, &receipt, None).await?;
    }
    sqlx::query("UPDATE session_inputs SET steered=false, attempt=NULL, reservation_id=NULL, reservation_revision=NULL,
        delivery_audit=delivery_audit || jsonb_build_array(jsonb_build_object('event','steering.queued'))
        WHERE task_id=$1 AND attempt=$2 AND steered AND delivery='queued' AND steering_flushed_at IS NULL")
        .bind(task).bind(attempt).execute(db.conn()).await?;
    Ok(())
}

async fn apply_input_shipped(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    project_steered_input(
        db,
        locked,
        event.task_id,
        event.attempt,
        &event.payload,
        Some(event.seq),
    )
    .await
}

async fn project_steered_input(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    payload: &Value,
    seq: Option<u64>,
) -> Result<(), AppError> {
    if locked.task_kind != TaskKind::Agent {
        return Ok(());
    }
    let input_id = payload_uuid(payload, "input_id")?;
    let reservation_id = payload_uuid(payload, "reservation_id")?;
    let revision = payload_i32(payload, "revision")?;
    let generation = payload_i32(payload, "delivery_generation")?;
    let row = sqlx::query("UPDATE session_inputs SET delivery='delivered', delivered_at=now(),
        delivery_audit=delivery_audit || jsonb_build_array(jsonb_strip_nulls(jsonb_build_object('event','input.shipped','seq',$8::bigint)))
        WHERE id=$1 AND org_id=$2 AND task_id=$3 AND attempt=$4 AND steered
          AND reservation_id=$5 AND revision=$6 AND reservation_revision=$6 AND delivery_generation=$7
          AND delivery='queued'
        RETURNING session_id, content, artifacts")
        .bind(input_id).bind(locked.org_id).bind(task_id).bind(attempt)
        .bind(reservation_id).bind(revision).bind(generation).bind(seq.map(|seq| i64::try_from(seq).unwrap_or(i64::MAX)))
        .fetch_optional(db.conn()).await?;
    // Repeated, superseded or mismatched flush evidence does not add rows.
    let Some(row) = row else {
        return Ok(());
    };
    let session_id: Uuid = row.try_get("session_id")?;
    if locked.chat_session_id != Some(session_id) {
        return Err(conflict("steered input session mismatch"));
    }
    refresh_assistant_message_content(db, locked, task_id, attempt).await?;
    let previous =
        current_assistant_message(db, locked.org_id, session_id, task_id, attempt).await?;
    crate::services::agents::chat_service::set_assistant_message_status(
        db.conn(),
        previous,
        "complete",
    )
    .await?;
    let mut message = assistant_message(locked.org_id, session_id, task_id, attempt);
    message.id = Uuid::new_v5(&reservation_id, b"agent-steered-reply");
    let content: String = row.try_get("content")?;
    crate::services::agents::chat_service::record_accepted_input(
        db.conn(),
        crate::services::agents::chat_service::AcceptedInput {
            message,
            input_id,
            role: "user",
            content: &content,
            artifacts: row.try_get("artifacts")?,
            created_at: Utc::now(),
        },
    )
    .await?;
    Ok(())
}

/// `session.title`: the workspace agent's one-line summary of what the
/// session is about, produced after its first turn. It only replaces a
/// platform-owned title (`title_source = 'derived'`, or a legacy row with no
/// title), so a name the user chose or cleared is never overwritten.
/// Malformed or empty titles are ignored rather than failing the turn's
/// event stream.
async fn apply_session_title(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let Some(title) = event.payload["title"].as_str() else {
        return Ok(());
    };
    crate::services::agents::chat_service::apply_generated_title(
        db.conn(),
        locked.org_id,
        session_id,
        title,
    )
    .await?;
    Ok(())
}

async fn apply_provider_session(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let provider_session_id = payload_string(&event.payload, "provider_session_id", 512)?;
    crate::services::agents::chat_service::record_provider_session(
        db.conn(),
        locked.org_id,
        session_id,
        &provider_session_id,
    )
    .await?;
    if let Some(provider) = event.payload["provider_id"].as_str() {
        if g5_internal_api::session_files::SessionFileProvider::parse(provider).is_some() {
            sqlx::query(
                "UPDATE chat_session_file_bundles
                 SET provider = $2, provider_session_id = $3, updated_at = now()
                 WHERE session_id = $1 AND state = 'pending'
                   AND fenced_task_id = $4 AND fenced_attempt = $5
                   AND fenced_vm_id = $6 AND fenced_epoch = $7",
            )
            .bind(session_id)
            .bind(provider)
            .bind(provider_session_id)
            .bind(event.task_id)
            .bind(event.attempt)
            .bind(event.vm_id.to_string())
            .bind(i64::try_from(event.epoch).map_err(|_| conflict("task epoch exceeds bigint"))?)
            .execute(db.conn())
            .await?;
        }
    }
    Ok(())
}

async fn apply_provider_request_opened(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    if locked.task_kind != TaskKind::Agent {
        return Err(conflict("only agent tasks may open provider requests"));
    }
    let provider_request_id = payload_string(&event.payload, "provider_request_id", 512)?;
    let request_kind = payload_string(&event.payload, "request_kind", 32)?;
    if !matches!(request_kind.as_str(), "question" | "permission") {
        return Err(conflict("provider request kind is unsupported"));
    }
    let payload = event.payload.get("payload").cloned().unwrap_or(Value::Null);
    let display = event
        .payload
        .get("display")
        .cloned()
        .ok_or_else(|| conflict("provider request display is absent"))?;
    ensure_json_bound(
        &payload,
        MAX_NATIVE_REQUEST_BYTES,
        "provider request payload",
    )?;
    ensure_json_bound(
        &display,
        MAX_NATIVE_REQUEST_BYTES,
        "provider request display",
    )?;
    let deadline = if request_kind == "question" {
        None
    } else {
        Some(
            DateTime::<Utc>::from_timestamp_millis(
                event.payload["deadline_unix_ms"]
                    .as_i64()
                    .ok_or_else(|| conflict("provider request deadline is absent"))?,
            )
            .ok_or_else(|| conflict("provider request deadline is invalid"))?,
        )
    };
    let resume_session_id = event.payload["resume_session_id"].as_str();
    let tool_name = event.payload["tool_name"].as_str();
    sqlx::query(
        "INSERT INTO provider_requests(
             org_id, task_id, attempt, provider_request_id, kind, tool_name,
             payload, display, response_deadline_at, resume_session_id, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                 LEAST(now(), $9 - interval '1 microsecond'))",
    )
    .bind(locked.org_id)
    .bind(event.task_id)
    .bind(event.attempt)
    .bind(provider_request_id)
    .bind(request_kind)
    .bind(tool_name)
    .bind(payload)
    .bind(display)
    .bind(deadline)
    .bind(resume_session_id)
    .execute(db.conn())
    .await
    .map_err(event_content_rejection)?;
    Ok(())
}

async fn apply_provider_request_replied(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    let provider_request_id = payload_string(&event.payload, "provider_request_id", 512)?;
    let digest = payload_digest(&event.payload, "response_digest")?;
    let source_attempt = if let Some(source) = event.payload["source_attempt"].as_i64() {
        let binding = locked
            .policy_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get("native_question_reply"))
            .ok_or_else(|| conflict("attempt has no native question binding"))?;
        if source <= 0
            || source >= i64::from(event.attempt)
            || binding["source_attempt"].as_i64() != Some(source)
            || binding["provider_request_id"].as_str() != Some(provider_request_id.as_str())
            || binding["response_digest"].as_str() != Some(digest.as_str())
            || binding["request_id"] != event.payload["request_id"]
        {
            return Err(conflict(
                "native answer evidence does not match the frozen restoration",
            ));
        }
        source as i32
    } else {
        event.attempt
    };

    if event.payload["evidence"].as_str() != Some("transport_write") {
        return Err(conflict("provider response lacks transport_write evidence"));
    }
    let disposition = event.payload["disposition"]
        .as_str()
        .unwrap_or("delivered")
        .chars()
        .take(256)
        .collect::<String>();
    let updated = sqlx::query(
        "UPDATE provider_requests
         SET status = 'responded', ended_at = now(), end_reason = $6,
             response = jsonb_set(response, '{delivery_evidence}',
                 jsonb_build_object('kind','transport_write','seq',$7::bigint), true)
         WHERE org_id = $1 AND task_id = $2 AND attempt = $3
           AND provider_request_id = $4 AND status = 'reply_pending'
           AND response ->> 'digest' = $5",
    )
    .bind(locked.org_id)
    .bind(event.task_id)
    .bind(source_attempt)
    .bind(&provider_request_id)
    .bind(&digest)
    .bind(disposition)
    .bind(i64::try_from(event.seq).unwrap_or(i64::MAX))
    .execute(db.conn())
    .await
    .map_err(event_content_rejection)?;
    if updated.rows_affected() != 1 {
        let stale_reply = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM provider_requests
                 WHERE org_id = $1 AND task_id = $2 AND attempt = $3
                   AND provider_request_id = $4 AND status = 'invalidated'
                   AND response ->> 'digest' = $5)",
        )
        .bind(locked.org_id)
        .bind(event.task_id)
        .bind(source_attempt)
        .bind(provider_request_id)
        .bind(digest)
        .fetch_one(db.conn())
        .await?;
        if stale_reply {
            // The API timeout owns the durable disposition. The dispatcher
            // can still observe transport completion afterward; accepting the
            // matching stale fact lets its ordered journal advance without
            // reviving the invalidated request.
            return Ok(());
        }
        return Err(conflict(
            "provider response event does not match one pending accepted reply",
        ));
    }
    if source_attempt != event.attempt {
        // One journal fact settles both the native answer and its reservation;
        // a crash cannot split positive answer delivery from input delivery.
        let mut accepted = event.clone();
        accepted.payload = event
            .payload
            .get("input_acceptance")
            .cloned()
            .ok_or_else(|| {
                // Transport-write evidence says the answer reached the
                // provider; absent acceptance evidence is the same
                // inconsistency as a mismatched one and must also stall.
                AppError::from_error(G5Error::Internal {
                    message: "restored answer has no input delivery evidence".to_string(),
                })
            })?;
        // The restored input and answer settle together
        // (`g5.chat.native-question-recovery`). A conflict from the durable
        // reservation is state inconsistency, not a rejectable provider fact:
        // escalate it past the savepoint's consumed-conflict path so the
        // journal stalls visibly (like a standalone `input.accepted`
        // conflict) instead of recording delivered input as rejected.
        apply_input_accepted(db, locked, &accepted)
            .await
            .map_err(|error| match &error.inner {
                G5Error::Conflict { reason } => AppError::from_error(G5Error::Internal {
                    message: format!("restored answer input acceptance failed: {reason}"),
                }),
                _ => error,
            })?;
    }
    Ok(())
}

async fn apply_provider_request_invalidated(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    let provider_request_id = payload_string(&event.payload, "provider_request_id", 512)?;
    let reason = event.payload["reason"]
        .as_str()
        .unwrap_or("execution_ended")
        .chars()
        .take(256)
        .collect::<String>();
    let source_attempt = if let Some(source) = event.payload["source_attempt"].as_i64() {
        let binding = locked
            .policy_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.get("native_question_reply"))
            .ok_or_else(|| conflict("attempt has no native question binding"))?;
        if reason != "provider_cancelled"
            || source <= 0
            || source >= i64::from(event.attempt)
            || binding["source_attempt"].as_i64() != Some(source)
            || binding["provider_request_id"].as_str() != Some(provider_request_id.as_str())
        {
            return Err(conflict(
                "native cancellation does not match the frozen restoration",
            ));
        }
        source as i32
    } else {
        event.attempt
    };
    sqlx::query(
        "UPDATE provider_requests
         SET status = 'invalidated', ended_at = now(), end_reason = $5
         WHERE org_id = $1 AND task_id = $2 AND attempt = $3
           AND provider_request_id = $4
           AND status IN ('pending','reply_pending')",
    )
    .bind(locked.org_id)
    .bind(event.task_id)
    .bind(source_attempt)
    .bind(provider_request_id)
    .bind(reason)
    .execute(db.conn())
    .await
    .map_err(event_content_rejection)?;
    Ok(())
}

async fn apply_outcome(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
    recovered: bool,
) -> Result<(), AppError> {
    if !event.terminal {
        return Err(conflict("invocation.outcome must be terminal"));
    }
    let cancel_requested = locked
        .task_spec
        .pointer("/lifecycle/cancel_requested")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut decision = if auth_before_delivery(locked) {
        // No prompt crossed the provider boundary. Authentication timeout,
        // owner stop and lost login processes end this turn; none can
        // manufacture a provider-session continuation.
        final_decision("failed", "provider_error")
    } else {
        apply_interruption_policy(
            outcome_decision(locked.task_kind, &event.payload, cancel_requested)?,
            locked.task_kind,
            cancel_requested,
            &locked.task_spec,
        )
    };
    // Claude's End that had to stop Claude ends a chat turn, as the chat's
    // Stop does (`g5.agents.lifecycle.chat-stop`); a turn another owner
    // follows stays held.
    if locked.task_kind == TaskKind::Agent
        && !decision.terminal
        && agent_result(&event.payload)["code"] == "background_wait_ended"
        && !turn_kept_for_its_owner(db, event.task_id).await?
    {
        decision = final_decision("cancelled", "interrupted");
    }
    let reported_delivery_unknown = locked.task_kind == TaskKind::Agent
        && (agent_result(&event.payload)["input_delivery_unknown"] == true
            || event.payload["detail"]["input_delivery_unknown"] == true);
    // The durable reservation is authoritative when a dispatcher fallback
    // cannot distinguish an accepted input from an in-flight write. Do this
    // inside the settlement transaction before choosing whether to hold.
    let input_delivery_unknown = if reported_delivery_unknown && !auth_before_delivery(locked) {
        mark_reserved_input_unknown(db, event.task_id, event.attempt, "provider_write_ambiguous")
            .await?
    } else {
        false
    };
    if input_delivery_unknown {
        decision = if cancel_requested {
            final_decision("cancelled", "cancelled")
        } else {
            AttemptDecision {
                task_status: "waiting",
                attempt_reason: if event.payload["status"] == "timed_out"
                    || agent_result(&event.payload)["status"] == "timed_out"
                {
                    "timed_out"
                } else {
                    "interrupted"
                },
                continuation_reason: Some("interrupted"),
                hold_reason: Some("delivery_unknown"),
                terminal: false,
            }
        };
    }
    // A provider process or attempt limit may end after the native tool was
    // deferred. That ends execution, not the owner's unanswered question.
    if !cancel_requested && !input_delivery_unknown {
        let deferred = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM provider_requests
            WHERE task_id = $1 AND attempt = $2 AND resume_session_id IS NOT NULL
              AND status IN ('pending','reply_pending'))",
        )
        .bind(event.task_id)
        .bind(event.attempt)
        .fetch_one(db.conn())
        .await?;
        if deferred {
            decision.task_status = "waiting";
            decision.continuation_reason = Some("suspended");
            decision.hold_reason = Some("awaiting_answer");
            decision.terminal = false;
        }
    }
    sqlx::query(
        "UPDATE task_attempts
         SET ended_at = now(), outcome = $4, reason = $5,
             session_files_state = COALESCE($6, session_files_state),
             session_manifest_generation = COALESCE($7, session_manifest_generation)
         WHERE task_id = $1 AND attempt = $2 AND vm_id = $3 AND ended_at IS NULL",
    )
    .bind(event.task_id)
    .bind(event.attempt)
    .bind(event.vm_id.to_string())
    .bind(event.payload.clone())
    .bind(decision.attempt_reason)
    .bind(agent_result(&event.payload)["session_files_state"].as_str())
    .bind(
        agent_result(&event.payload)["session_manifest_generation"]
            .as_i64()
            .and_then(|value| i32::try_from(value).ok()),
    )
    .execute(db.conn())
    .await?;

    if locked.task_kind == TaskKind::Agent {
        materialize_final_agent_message(
            db,
            locked,
            event.task_id,
            event.attempt,
            &event.payload,
            &decision,
        )
        .await?;
        apply_agent_session_outcome(db, locked, event.task_id, &event.payload).await?;
        // A turn that succeeds ends its reply with its ontology changes card
        // (`g5.chat.ontology-card`); stopped and failed turns have none.
        if decision.task_status == "succeeded" && locked.chat_session_id.is_some() {
            let message_id = current_assistant_message_id(db, event.task_id, event.attempt).await?;
            crate::services::agents::chat_ontology::attach_card(
                db.conn(),
                event.task_id,
                event.attempt,
                message_id,
            )
            .await?;
        }
        if decision.terminal {
            crate::services::agents::chat_ontology::drop_turn_record(db.conn(), event.task_id)
                .await?;
        }
    }
    let task_outcome = if locked.task_kind == TaskKind::Preview {
        preview_outcome_with_reason(&event.payload, &locked.task_spec, cancel_requested)
    } else {
        event.payload.clone()
    };
    settle_task_row(
        db,
        locked,
        event.task_id,
        event.attempt,
        (event.vm_id, event.epoch),
        &decision,
        Some(task_outcome),
    )
    .await?;
    if locked.task_kind == TaskKind::Agent {
        if may_restore_fenced_publication(recovered, &event.payload) {
            restore_fenced_publication_if_undelivered(db, locked, event).await?;
        }
        if input_delivery_unknown && cancel_requested {
            withdraw_unknown_input_on_cancel(db, event.task_id, event.attempt).await?;
            release_unaccepted_input(db, event.task_id, event.attempt, true).await?;
        } else if input_delivery_unknown {
            // Already recorded above; keep the input for an explicit owner choice.
        } else {
            release_unaccepted_input(db, event.task_id, event.attempt, decision.terminal).await?;
        }
    }
    if decision.terminal {
        settle_dependents(db, locked, event.task_id, decision.task_status).await?;
        if locked.task_kind == TaskKind::Preview {
            settle_preview_intent(db, locked.workspace_id, event.task_id, "settled").await?;
        }
        if locked.task_kind == TaskKind::Agent {
            ensure_next_queued_agent_task(
                db,
                locked.org_id,
                locked.workspace_id,
                locked.chat_session_id,
                event.task_id,
            )
            .await?;
        }
    } else {
        hand_off_held_chat_turn(db, locked, event.task_id).await?;
    }
    // An attempt that carried a frozen answer and ended for good without the
    // answer's transport-write evidence could not restore the question. Name
    // that on the request, whose accepted answer stays recorded
    // (`g5.chat.native-question-recovery`).
    let reason = if decision.terminal
        && locked
            .policy_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.get("native_question_reply").is_some())
    {
        "native_question_restore_failed"
    } else {
        "execution_ended"
    };
    invalidate_attempt_requests(db, event.task_id, event.attempt, reason).await?;
    Ok(())
}

pub(crate) async fn has_unanswered_native_question(
    db: &mut ScopedDb,
    task_id: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM provider_requests WHERE task_id = $1
        AND resume_session_id IS NOT NULL AND status IN ('pending','reply_pending'))",
    )
    .bind(task_id)
    .fetch_one(db.conn())
    .await?)
}

/// Whether another owner follows this agent turn: a workflow turn, or one the
/// platform started (its own message is the platform's, as a merge-conflict
/// resolution's is, which its merge run follows by task). Stop, the owner's
/// next message and Claude's End keep such a turn's hold for Continue.
pub(crate) async fn turn_kept_for_its_owner(
    db: &mut ScopedDb,
    task_id: Uuid,
) -> Result<bool, AppError> {
    let origin = sqlx::query_scalar::<_, String>(
        "SELECT input_origin FROM session_inputs
         WHERE task_id = $1 AND delivery <> 'withdrawn' AND input_origin <> 'continuation'
         ORDER BY input_index, id
         LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(db.conn())
    .await?;
    Ok(origin.as_deref() == Some("workflow"))
}

/// The owner's next message ends a held chat turn
/// (`g5.agents.lifecycle.chat-held-turn`), also one sent while the turn was
/// still stopping: once settlement holds the turn, an owner's message queued
/// behind the turn's own message ends it and starts the next turn. `apply`
/// holds the session's turn allocation lock.
async fn hand_off_held_chat_turn(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    if locked.task_kind != TaskKind::Agent
        || turn_kept_for_its_owner(db, task_id).await?
        || has_unanswered_native_question(db, task_id).await?
    {
        return Ok(());
    }
    let own_message = turn_message(db, task_id).await?;
    let follows = sqlx::query_scalar::<_, bool>(
        "SELECT task.status = 'waiting'
                AND task.hold_reason IS DISTINCT FROM 'delivery_unknown'
                AND EXISTS (
                    SELECT 1 FROM session_inputs AS input
                    WHERE input.task_id = task.id AND input.delivery = 'queued'
                      AND input.input_origin = 'human' AND input.id IS DISTINCT FROM $2)
         FROM tasks AS task WHERE task.id = $1",
    )
    .bind(task_id)
    .bind(own_message)
    .fetch_optional(db.conn())
    .await?
    .unwrap_or(false);
    if !follows {
        return Ok(());
    }
    end_chat_turn_now(db, task_id, ChatTurnEnd::NextMessage).await?;
    ensure_next_queued_agent_task(
        db,
        locked.org_id,
        locked.workspace_id,
        Some(session_id),
        task_id,
    )
    .await
}

/// End a chat turn that is queued or held: the task is cancelled, as Cancel
/// does, but the messages queued behind it stay queued for the next turn. The
/// caller holds the session's turn allocation lock and the task row, and
/// starts the next turn itself.
pub(crate) async fn end_chat_turn_now(
    db: &mut ScopedDb,
    task_id: Uuid,
    end: ChatTurnEnd,
) -> Result<(), AppError> {
    let (attempt, workspace_id) = sqlx::query_as::<_, (i32, Uuid)>(
        "UPDATE tasks
         SET status = 'cancelled',
             outcome = jsonb_build_object('status','cancelled','reason',$2::text),
             hold_reason = NULL, continuation_reason = NULL,
             ended_at = now(), updated_at = now()
         WHERE id = $1 AND kind = 'agent'
           AND status IN ('queued','waiting')
         RETURNING attempt, workspace_id",
    )
    .bind(task_id)
    .bind(match end {
        ChatTurnEnd::Stopped => "chat_stop",
        ChatTurnEnd::NextMessage => "next_message",
    })
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| conflict("chat turn is no longer queued or held"))?;
    // A continuation's own input never outlives its task. Stop withdraws the
    // turn's own message when the agent never received it; a new message
    // keeps it queued.
    let own_message = match end {
        ChatTurnEnd::Stopped => turn_message(db, task_id).await?,
        ChatTurnEnd::NextMessage => None,
    };
    sqlx::query(
        "UPDATE session_inputs
         SET delivery = 'withdrawn', withdrawn_at = now(),
             delivery_audit = delivery_audit || jsonb_build_array(
                 jsonb_build_object('event','turn_ended','reason',$3::text))
         WHERE task_id = $1
           AND delivery IN ('queued','reserved','delivery_unknown')
           AND (input_origin = 'continuation' OR id = $2)",
    )
    .bind(task_id)
    .bind(own_message)
    .bind(match end {
        ChatTurnEnd::Stopped => "chat_stop",
        ChatTurnEnd::NextMessage => "next_message",
    })
    .execute(db.conn())
    .await?;
    invalidate_attempt_requests(db, task_id, attempt, "chat_stop").await?;
    withdraw_task_system_notices(db, task_id, attempt, "turn_ended").await?;
    release_automatic_consent_without_work(db.conn(), workspace_id).await?;
    Ok(())
}

/// A chat turn's own message: the earliest one bound to its task that is not
/// withdrawn, delivered or not. Messages queued behind the turn come after it.
async fn turn_message(db: &mut ScopedDb, task_id: Uuid) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM session_inputs
         WHERE task_id = $1 AND delivery <> 'withdrawn' AND input_origin <> 'continuation'
         ORDER BY input_index, id
         LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(db.conn())
    .await?)
}

/// A normal terminal outcome can prove non-delivery from the reservation row.
/// Recovered outcomes never call this: missing journal events may hide delivery.
fn may_restore_fenced_publication(recovered: bool, outcome: &Value) -> bool {
    !recovered && agent_result(outcome)["provider_started"] == false
}

async fn restore_fenced_publication_if_undelivered(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    event: &JournalTaskEvent,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    crate::services::agents::chat_session_file_service::ChatSessionFileService::new()
        .restore_if_undelivered(
            db.conn(),
            session_id,
            event.task_id,
            event.attempt,
            crate::services::agents::chat_session_file_service::SessionFileIncarnation {
                vm_id: event.vm_id,
                epoch: event.epoch,
            },
        )
        .await?;
    Ok(())
}

pub(crate) async fn ensure_next_queued_agent_task(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    session_id: Option<Uuid>,
    settled_task_id: Uuid,
) -> Result<(), AppError> {
    let Some(session_id) = session_id else {
        return Ok(());
    };
    crate::services::agents::chat_service::lock_session_turn_allocation(db.conn(), session_id)
        .await?;
    let uncertain = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM session_inputs
        WHERE session_id=$1 AND delivery='delivery_unknown')",
    )
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;
    if uncertain {
        return Ok(());
    }
    let has_queued_input = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM session_inputs
             WHERE org_id = $1 AND session_id = $2 AND delivery = 'queued'
               AND input_origin <> 'continuation')",
    )
    .bind(org_id)
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;
    if !has_queued_input {
        return Ok(());
    }
    let active_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tasks
             WHERE org_id = $1 AND workspace_id = $2 AND chat_session_id = $3
               AND kind = 'agent' AND status IN ('queued','running','waiting'))",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;
    if active_exists {
        return Err(conflict(
            "settled agent task still has an active same-session task",
        ));
    }
    let created_by = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT owner_user_id FROM workspaces
         WHERE org_id = $1 AND id = $2",
    )
    .bind(org_id)
    .bind(workspace_id)
    .fetch_one(db.conn())
    .await?;
    let next_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks(
             id, name, status, retry_count,
             depends_on, input_tokens, output_tokens, cost_usd, org_id,
             workspace_id, kind, spec, attempt, start_confirmed,
             chat_session_id, created_by, created_at, updated_at)
         SELECT $1,'Agent turn','queued',0,'{}'::uuid[],0,0,0,
                org_id,workspace_id,'agent',spec - 'lifecycle',1,false,chat_session_id,$2,now(),now()
         FROM tasks WHERE id = $3 AND org_id = $4",
    )
    .bind(next_task_id)
    .bind(created_by)
    .bind(settled_task_id)
    .bind(org_id)
    .execute(db.conn())
    .await?;
    sqlx::query(
        "UPDATE session_inputs SET task_id = $1, attempt = NULL, steered = false, reservation_id = NULL, reservation_revision = NULL
         WHERE org_id = $2 AND session_id = $3 AND delivery = 'queued'
           AND input_origin <> 'continuation'",
    )
    .bind(next_task_id)
    .bind(org_id)
    .bind(session_id)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// The journal is the source for both the live placeholder and its settled
/// canonical row. A final replaces its own deltas; different native blocks
/// retain their first-seen order. Legacy events without block ids pair only
/// with their immediately preceding incremental run.
fn reduce_assistant_output(events: &[(String, Value)]) -> String {
    struct Block {
        id: String,
        deltas: String,
        final_content: Option<String>,
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut fallback: Option<String> = None;
    let mut next_fallback = 0usize;
    for (kind, payload) in events {
        if kind != "message.delta" && kind != "message.final" {
            fallback = None;
            continue;
        }
        let text = if kind == "message.final" {
            payload["content"].as_str()
        } else {
            payload["text_delta"].as_str()
        };
        let Some(text) = text else { continue };
        let explicit_id = payload["block_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(ToString::to_string);
        let id = explicit_id.clone().unwrap_or_else(|| {
            fallback
                .get_or_insert_with(|| {
                    next_fallback += 1;
                    format!("legacy-block-{next_fallback}")
                })
                .clone()
        });
        if kind == "message.final" && explicit_id.is_some() && !blocks.iter().any(|b| b.id == id) {
            if let Some(legacy_id) = fallback.as_ref() {
                if let Some(block) = blocks.iter_mut().find(|block| &block.id == legacy_id) {
                    block.id = id.clone();
                }
            }
        }
        let index = blocks
            .iter()
            .position(|block| block.id == id)
            .unwrap_or_else(|| {
                blocks.push(Block {
                    id,
                    deltas: String::new(),
                    final_content: None,
                });
                blocks.len() - 1
            });
        if kind == "message.final" {
            if blocks[index].final_content.is_none() {
                blocks[index].final_content = Some(text.to_string());
            }
            fallback = None;
        } else if blocks[index].final_content.is_none() {
            blocks[index].deltas.push_str(text);
        }
    }
    blocks
        .into_iter()
        .map(|block| block.final_content.unwrap_or(block.deltas))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn collect_assistant_message_content(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    message_id: Uuid,
) -> Result<String, AppError> {
    let rows = sqlx::query(
        "SELECT metadata ->> 'kind' AS kind,
                CASE WHEN metadata ->> 'kind' IN ('message.delta', 'message.final')
                     THEN metadata -> 'payload' ELSE 'null'::jsonb END AS payload
         FROM task_events WHERE task_id = $1 AND attempt = $2
           AND metadata ->> 'schema_version' = 'g5.task-event.v1'
           AND COALESCE(metadata ->> 'messageId', $4::text) = $3
         ORDER BY seq",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(message_id.to_string())
    .bind(canonical_agent_message_id(task_id, attempt).to_string())
    .fetch_all(db.conn())
    .await?;
    let events = rows
        .into_iter()
        .map(|row| Ok((row.try_get("kind")?, row.try_get("payload")?)))
        .collect::<Result<Vec<(String, Value)>, sqlx::Error>>()?;
    Ok(reduce_assistant_output(&events))
}

async fn refresh_assistant_message_content(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
) -> Result<(), AppError> {
    // A block started before a steering flush may still receive deltas after
    // its segment closes. Preserve those partial bytes on Stop and VM loss.
    for id in attempt_assistant_ids(db, task_id, attempt).await? {
        refresh_assistant_segment(db, locked, task_id, attempt, id).await?;
    }
    Ok(())
}

async fn refresh_assistant_segment(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    id: Uuid,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let mut message = assistant_message(locked.org_id, session_id, task_id, attempt);
    message.id = id;
    let content = collect_assistant_message_content(db, task_id, attempt, id).await?;
    crate::services::agents::chat_service::refresh_assistant_message(db.conn(), message, &content)
        .await?;
    Ok(())
}

/// Teach the segment map the persisted owner of every unit in `events` it
/// has not seen: one bounded query per batch, and none in steady state,
/// because a follower's map already assigned the units it published live.
async fn seed_segment_owners(
    db: &mut ScopedDb,
    segments: &mut super::segment_map::SegmentMap,
    events: &[JournalBatchEvent],
) -> Result<(), AppError> {
    let mut block_ids: Vec<String> = Vec::new();
    let mut call_ids: Vec<String> = Vec::new();
    for entry in events {
        let event = &entry.event;
        if segments.known_owner(&event.kind, &event.payload).is_some() {
            continue;
        }
        match super::segment_map::SegmentMap::unit_of(&event.kind, &event.payload) {
            Some(("block_id", unit)) => block_ids.push(unit),
            Some(("call_id", unit)) => call_ids.push(unit),
            _ => {}
        }
    }
    if block_ids.is_empty() && call_ids.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT metadata ->> 'kind' AS kind,
                metadata -> 'payload' ->> 'block_id' AS block_id,
                metadata -> 'payload' ->> 'call_id' AS call_id,
                metadata ->> 'messageId' AS message_id
         FROM task_events
         WHERE task_id = $1 AND attempt = $2
           AND metadata ->> 'schema_version' = 'g5.task-event.v1'
           AND (metadata -> 'payload' ->> 'block_id' = ANY($3)
                OR metadata -> 'payload' ->> 'call_id' = ANY($4))
         ORDER BY seq",
    )
    .bind(segments.task_id())
    .bind(segments.attempt())
    .bind(&block_ids)
    .bind(&call_ids)
    .fetch_all(db.conn())
    .await?;
    for row in rows {
        let kind: String = row.try_get("kind")?;
        let owner = row
            .try_get::<Option<String>, _>("message_id")?
            .and_then(|id| Uuid::parse_str(&id).ok());
        let Some(owner) = owner else { continue };
        let unit = match super::segment_map::unit_binding(&kind) {
            Some(("block_id", _)) => row.try_get::<Option<String>, _>("block_id")?,
            Some(("call_id", _)) => row.try_get::<Option<String>, _>("call_id")?,
            _ => None,
        };
        if let Some(unit) = unit {
            segments.learn(&kind, &unit, owner);
        }
    }
    Ok(())
}

/// The persisted owners of every unit of an attempt, for a follower that
/// starts mid-attempt (after an API restart or reclaim).
pub(crate) async fn seed_segment_map(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
) -> Result<super::segment_map::SegmentMap, AppError> {
    let mut segments = super::segment_map::SegmentMap::with_current(
        task_id,
        attempt,
        current_assistant_message_id(db, task_id, attempt).await?,
    );
    let rows = sqlx::query(
        "SELECT metadata ->> 'kind' AS kind,
                metadata -> 'payload' ->> 'block_id' AS block_id,
                metadata -> 'payload' ->> 'call_id' AS call_id,
                metadata ->> 'messageId' AS message_id
         FROM task_events
         WHERE task_id = $1 AND attempt = $2
           AND metadata ->> 'schema_version' = 'g5.task-event.v1'
           AND (metadata -> 'payload' ? 'block_id' OR metadata -> 'payload' ? 'call_id')
         ORDER BY seq",
    )
    .bind(task_id)
    .bind(attempt)
    .fetch_all(db.conn())
    .await?;
    for row in rows {
        let kind: String = row.try_get("kind")?;
        let owner = row
            .try_get::<Option<String>, _>("message_id")?
            .and_then(|id| Uuid::parse_str(&id).ok());
        let Some(owner) = owner else { continue };
        let unit = match super::segment_map::unit_binding(&kind) {
            Some(("block_id", _)) => row.try_get::<Option<String>, _>("block_id")?,
            Some(("call_id", _)) => row.try_get::<Option<String>, _>("call_id")?,
            _ => None,
        };
        if let Some(unit) = unit {
            segments.learn(&kind, &unit, owner);
        }
    }
    Ok(segments)
}

/// Apply a batch's deferred per-segment effects once each: a segment that
/// streamed is marked streaming, one with a completed block is rebuilt.
async fn flush_segment_effects(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    streaming: &mut std::collections::BTreeSet<Uuid>,
    refresh: &mut std::collections::BTreeSet<Uuid>,
) -> Result<(), AppError> {
    for id in std::mem::take(streaming) {
        if !refresh.contains(&id) {
            mark_assistant_message_streaming(db, locked, task_id, attempt, id).await?;
        }
    }
    for id in std::mem::take(refresh) {
        refresh_assistant_segment(db, locked, task_id, attempt, id).await?;
    }
    Ok(())
}

/// The canonical assistant message of one attempt in its conversation.
fn assistant_message(
    org_id: Uuid,
    session_id: Uuid,
    task_id: Uuid,
    attempt: i32,
) -> crate::services::agents::chat_service::AssistantMessage {
    crate::services::agents::chat_service::AssistantMessage {
        id: canonical_agent_message_id(task_id, attempt),
        org_id,
        session_id,
        task_id,
        attempt,
    }
}

async fn current_assistant_message_id(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM chat_messages WHERE task_id=$1 AND attempt=$2 AND role='assistant'
        ORDER BY message_index DESC LIMIT 1",
    )
    .bind(task_id)
    .bind(attempt)
    .fetch_optional(db.conn())
    .await?
    .unwrap_or_else(|| canonical_agent_message_id(task_id, attempt)))
}

async fn current_assistant_message(
    db: &mut ScopedDb,
    org_id: Uuid,
    session_id: Uuid,
    task_id: Uuid,
    attempt: i32,
) -> Result<crate::services::agents::chat_service::AssistantMessage, AppError> {
    let mut message = assistant_message(org_id, session_id, task_id, attempt);
    message.id = current_assistant_message_id(db, task_id, attempt).await?;
    Ok(message)
}

async fn attempt_assistant_ids(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
) -> Result<Vec<Uuid>, AppError> {
    let mut ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM chat_messages WHERE task_id=$1 AND attempt=$2 AND role='assistant'
        ORDER BY message_index",
    )
    .bind(task_id)
    .bind(attempt)
    .fetch_all(db.conn())
    .await?;
    if ids.is_empty() {
        ids.push(canonical_agent_message_id(task_id, attempt));
    }
    Ok(ids)
}

async fn mark_assistant_message_streaming(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    id: Uuid,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let mut message = assistant_message(locked.org_id, session_id, task_id, attempt);
    message.id = id;
    crate::services::agents::chat_service::mark_assistant_message_streaming(db.conn(), message)
        .await?;
    Ok(())
}

async fn settle_existing_assistant_message(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    decision: &AttemptDecision,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let status = if decision.task_status == "failed" {
        "error"
    } else {
        "stopped"
    };
    // A steering flush closed earlier segments. Stop/loss preserves them.
    let message =
        current_assistant_message(db, locked.org_id, session_id, task_id, attempt).await?;
    crate::services::agents::chat_service::set_assistant_message_status(db.conn(), message, status)
        .await?;
    Ok(())
}

async fn materialize_final_agent_message(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    outcome: &Value,
    decision: &AttemptDecision,
) -> Result<(), AppError> {
    if auth_before_delivery(locked) {
        return materialize_auth_failure(db, locked, task_id, attempt, Some(outcome)).await;
    }
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let status = match decision.task_status {
        "succeeded" => "complete",
        "failed" => "error",
        _ => "stopped",
    };
    let mut fallback = false;
    let current = current_assistant_message_id(db, task_id, attempt).await?;
    for id in attempt_assistant_ids(db, task_id, attempt).await? {
        let mut message = assistant_message(locked.org_id, session_id, task_id, attempt);
        message.id = id;
        let content = collect_assistant_message_content(db, task_id, attempt, id).await?;
        fallback |= crate::services::agents::chat_service::settle_final_assistant_message(
            db.conn(),
            message,
            &content,
            if id == current { status } else { "complete" },
        )
        .await?;
    }
    if fallback {
        withdraw_task_system_notices(db, task_id, attempt, "attempt_ended_before_acceptance")
            .await?;
    }
    Ok(())
}

/// Retain the failed turn in chat without claiming that the provider accepted
/// its input. In particular, Stop or VM loss can kill the module before it can
/// emit a notice; the control-plane outcome must still explain what happened.
async fn materialize_auth_failure(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    outcome: Option<&Value>,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let code = auth_failure_code(locked, outcome);
    let notice = g5_internal_api::codex_auth::auth_notice(code);
    // The journal survives a failed best-effort Ended HTTP update. Recover its
    // allowlisted reason without overriding a durable owner interruption.
    sqlx::query(
        "UPDATE task_attempts SET codex_auth_challenge_enc=NULL,
             codex_auth_state=jsonb_build_object('phase',$3::text,'code',$4::text)
         WHERE task_id=$1 AND attempt=$2",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(if code == "codex_auth_superseded" {
        "superseded"
    } else {
        "ended"
    })
    .bind(code)
    .execute(db.conn())
    .await?;
    let inputs = sqlx::query(
        "SELECT id, content, artifacts, created_at FROM session_inputs
         WHERE task_id = $1 AND attempt = $2 AND delivery = 'reserved'
           AND input_origin <> 'continuation'
         ORDER BY input_index, id",
    )
    .bind(task_id)
    .bind(attempt)
    .fetch_all(db.conn())
    .await?
    .into_iter()
    .map(|row| {
        Ok(crate::services::agents::chat_service::UnacceptedInput {
            input_id: row.try_get("id")?,
            content: row.try_get("content")?,
            artifacts: row.try_get("artifacts")?,
            created_at: row.try_get("created_at")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    crate::services::agents::chat_service::record_auth_failure_turn(
        db.conn(),
        assistant_message(locked.org_id, session_id, task_id, attempt),
        &inputs,
        notice,
    )
    .await?;
    Ok(())
}

fn auth_failure_code(locked: &LockedAttempt, outcome: Option<&Value>) -> &'static str {
    let state = locked.codex_auth_state.as_ref().unwrap_or(&Value::Null);
    let interrupt = locked.task_spec.pointer("/lifecycle/interrupt_reason");
    // Historical: an attempt that a message cancelled before follow-ups queued
    // during login (`g5.chat.login-before-prompt`) may still settle here.
    if state["phase"] == "superseded"
        || interrupt.and_then(Value::as_str) == Some("codex_auth_superseded")
    {
        return "codex_auth_superseded";
    }
    if interrupt.is_some()
        || locked
            .task_spec
            .pointer("/lifecycle/cancel_requested")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return "codex_auth_interrupted";
    }
    state["code"]
        .as_str()
        .and_then(crate::services::agents::codex_auth::terminal_auth_code)
        .or_else(|| {
            outcome
                .and_then(|outcome| agent_result(outcome)["code"].as_str())
                .and_then(crate::services::agents::codex_auth::terminal_auth_code)
        })
        .unwrap_or("codex_auth_interrupted")
}

async fn withdraw_task_system_notices(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    reason: &str,
) -> Result<(), AppError> {
    crate::services::agents::chat_service::withdraw_task_notices(
        db.conn(),
        task_id,
        attempt,
        reason,
    )
    .await?;
    Ok(())
}

async fn apply_agent_session_outcome(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    outcome: &Value,
) -> Result<(), AppError> {
    let Some(session_id) = locked.chat_session_id else {
        return Ok(());
    };
    let result = agent_result(outcome);
    let provider_session_id = result["provider_session_id"].as_str();
    let generation = result["session_manifest_generation"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok());
    crate::services::agents::chat_service::record_provider_session_state(
        db.conn(),
        locked.org_id,
        session_id,
        provider_session_id,
        generation,
    )
    .await?;
    let usage = &result["usage"];
    let input_tokens = usage["input_tokens"].as_i64().unwrap_or(0).max(0);
    let output_tokens = usage["output_tokens"].as_i64().unwrap_or(0).max(0);
    let cache_creation = usage["cache_creation_input_tokens"]
        .as_i64()
        .unwrap_or(0)
        .max(0);
    let cache_read = usage["cache_read_input_tokens"]
        .as_i64()
        .unwrap_or(0)
        .max(0);
    let cost = usage["total_cost_usd"]
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    if input_tokens != 0
        || output_tokens != 0
        || cache_creation != 0
        || cache_read != 0
        || cost != 0.0
    {
        crate::services::identity::spend_service::record_agent_turn_usage(
            db.conn(),
            crate::services::identity::spend_service::AgentTurnUsage {
                org_id: locked.org_id,
                workspace_id: locked.workspace_id,
                task_id,
                session_id,

                provider: snapshot_launch_value(locked.policy_snapshot.as_ref(), "provider")
                    .unwrap_or("unknown"),
                model: usage["model"]
                    .as_str()
                    .or_else(|| snapshot_launch_value(locked.policy_snapshot.as_ref(), "model"))
                    .unwrap_or("unknown"),
                input_tokens,
                output_tokens,
                cache_creation_tokens: cache_creation,
                cache_read_tokens: cache_read,
                cost_usd: cost,
                finish_reason: usage["finish_reason"].as_str(),
                duration_ms: usage["duration_ms"]
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok()),
            },
        )
        .await?;
    }
    Ok(())
}

async fn settle_dependents(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    final_status: &str,
) -> Result<(), AppError> {
    if final_status != "succeeded" {
        fail_task_dependents(db, locked.org_id, locked.workspace_id, task_id).await?;
    }
    Ok(())
}

pub(crate) async fn fail_task_dependents(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "WITH RECURSIVE blocked(id) AS (
             SELECT edge.task_id
             FROM task_dependencies AS edge
             WHERE edge.org_id = $1 AND edge.workspace_id = $2
               AND edge.depends_on_task_id = $3
             UNION
             SELECT edge.task_id
             FROM task_dependencies AS edge
             JOIN blocked ON blocked.id = edge.depends_on_task_id
             WHERE edge.org_id = $1 AND edge.workspace_id = $2
         )
         UPDATE tasks AS dependent
         SET status = 'failed',
             outcome = jsonb_build_object(
                 'status','failed', 'reason','dependency_failed',
                 'dependency_task_id',$3::text),
             hold_reason = NULL, continuation_reason = NULL,
             ended_at = now(), updated_at = now()
         FROM blocked
         WHERE dependent.id = blocked.id
           AND dependent.status IN ('queued','waiting')",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(task_id)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// A turn the owner must decide on does not qualify for an automatic start:
/// an unanswered clarification waits for its answer, and a session with an
/// input of unknown delivery waits for Redeliver.
const OWNER_DECISION_EXCLUSIONS: &str = "AND NOT EXISTS (
                       SELECT 1 FROM provider_requests AS request
                       WHERE request.task_id = task.id
                         AND request.resume_session_id IS NOT NULL
                         AND request.status = 'pending'
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM session_inputs AS uncertain
                       WHERE uncertain.session_id = task.chat_session_id
                         AND uncertain.delivery = 'delivery_unknown'
                   )";

/// What the last incarnation's platform interruption left for the lifecycle
/// owner to continue (`g5.lifecycle.start.automatic-continuation`). A running
/// attempt means the loss has not settled yet. Turns a lost VM left held
/// `vm_lost`, queued with the loss marker, or queued by a drain policy whose
/// attempt nevertheless settled `vm_lost`, count first; then turns a drain or
/// rollout stop left queued with their interrupt reason. Holds on the owner's decision (their Stop, an input
/// of unknown delivery, an unanswered clarification) are not platform
/// interruptions, and neither is a turn held for a start they must give. A
/// session whose input is of unknown delivery cannot dispatch until the owner
/// chooses Redeliver, so its turn does not qualify either, nor does a turn the
/// owner's own Stop was interrupting when the VM was lost (its lifecycle
/// marker names their decision). Loss evidence is
/// bound to the incarnation being decided (the attempt's or marker's epoch
/// equals the VM row's), so a hold or attempt from an earlier incarnation,
/// left in place by a Start that never reached Ready, is not read again.
pub(crate) async fn interrupted_work(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<InterruptedWork, sqlx::Error> {
    let verdict: Option<String> = sqlx::query_scalar(&format!(
        "WITH vm AS (SELECT epoch FROM workspace_vms WHERE workspace_id = $1)
         SELECT CASE
             WHEN EXISTS (
                 SELECT 1 FROM tasks
                 WHERE workspace_id = $1 AND kind = 'agent' AND status = 'running'
             ) THEN 'unsettled'
             WHEN EXISTS (
                 SELECT 1 FROM tasks AS task, vm
                 WHERE task.workspace_id = $1 AND task.kind = 'agent'
                   AND task.status = 'waiting' AND task.hold_reason = 'vm_lost'
                   AND COALESCE(task.spec #>> '{{lifecycle,interrupt_reason}}', 'vm_lost')
                       IN ('vm_lost','drain','rollout')
                   AND EXISTS (
                       SELECT 1 FROM task_attempts AS lost
                       WHERE lost.task_id = task.id AND lost.ended_at IS NOT NULL
                         AND lost.reason = 'vm_lost' AND lost.epoch = vm.epoch
                   )
                   {OWNER_DECISION_EXCLUSIONS}
             ) THEN 'vm_lost'
             WHEN EXISTS (
                 SELECT 1 FROM tasks AS task, vm
                 WHERE task.workspace_id = $1 AND task.kind = 'agent' AND task.status = 'queued'
                   AND ((task.spec #>> '{{lifecycle,interrupt_reason}}' = 'vm_lost'
                         AND (task.spec #>> '{{lifecycle,interrupt_epoch}}')::bigint = vm.epoch)
                        OR EXISTS (
                            SELECT 1 FROM task_attempts AS lost
                            WHERE lost.task_id = task.id AND lost.ended_at IS NOT NULL
                              AND lost.reason = 'vm_lost' AND lost.epoch = vm.epoch
                              AND lost.attempt = (
                                  SELECT max(attempt) FROM task_attempts
                                  WHERE task_id = task.id AND ended_at IS NOT NULL
                              )
                        ))
                   {OWNER_DECISION_EXCLUSIONS}
             ) THEN 'vm_lost'
             WHEN EXISTS (
                 SELECT 1 FROM tasks AS task
                 WHERE task.workspace_id = $1 AND task.kind = 'agent' AND task.status = 'queued'
                   AND task.spec #>> '{{lifecycle,interrupt_reason}}' IN ('drain','rollout')
                   {OWNER_DECISION_EXCLUSIONS}
             ) THEN 'host_stop'
         END",
    ))
    .bind(workspace_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(match verdict.as_deref() {
        Some("unsettled") => InterruptedWork::Unsettled,
        Some("vm_lost") => InterruptedWork::VmLost,
        Some("host_stop") => InterruptedWork::HostStop,
        _ => InterruptedWork::None,
    })
}

/// The turns an automatic continuation was recorded for that are still there
/// to run: the same turns the verdict counts, minus its incarnation binding,
/// because the consent may be replayed after a refused placement advanced the
/// epoch. A turn the owner's Continue re-queued keeps its lost attempt as
/// evidence; a turn queued after the interruption carries no consent.
pub(crate) async fn continuable_work_remains(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "SELECT EXISTS (
             SELECT 1 FROM tasks AS task
             WHERE task.workspace_id = $1 AND task.kind = 'agent'
               AND ((task.status = 'waiting' AND task.hold_reason = 'vm_lost'
                     AND COALESCE(task.spec #>> '{{lifecycle,interrupt_reason}}', 'vm_lost')
                         IN ('vm_lost','drain','rollout'))
                    OR (task.status = 'queued'
                        AND (task.spec #>> '{{lifecycle,interrupt_reason}}'
                                 IN ('vm_lost','drain','rollout')
                             OR EXISTS (
                                 SELECT 1 FROM task_attempts AS lost
                                 WHERE lost.task_id = task.id AND lost.ended_at IS NOT NULL
                                   AND lost.reason = 'vm_lost'
                                   AND lost.attempt = (
                                       SELECT max(attempt) FROM task_attempts
                                       WHERE task_id = task.id AND ended_at IS NOT NULL
                                   )
                             ))))
               {OWNER_DECISION_EXCLUSIONS}
         )",
    ))
    .bind(workspace_id)
    .fetch_one(&mut *conn)
    .await
}

/// An automatic continuation still waiting for capacity is scoped to the
/// interrupted turns: every path that ends the last of them (owner Cancel,
/// chat Stop, a withdrawal that leaves a turn without input, a new message
/// taking over a held turn) cancels it here, in the same transaction, so
/// placement cannot start an empty workspace
/// (`g5.lifecycle.start.automatic-continuation`).
pub(crate) async fn release_automatic_consent_without_work(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    // Serialize the check with every other path ending a turn of this
    // workspace: two chats whose last turns end concurrently would each see
    // the other's turn still there. The row is locked only while an automatic
    // consent waits (the VM is stopped then, so no stop holds it).
    let waiting: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM workspace_vms
         WHERE workspace_id = $1
           AND start_intent ->> 'kind' = 'automatic_continuation'
           AND start_intent ->> 'status' IN ('pending', 'queued_for_capacity')
         FOR UPDATE",
    )
    .bind(workspace_id)
    .fetch_optional(&mut *conn)
    .await?;
    if waiting.is_none() {
        return Ok(());
    }
    if !continuable_work_remains(&mut *conn, workspace_id).await? {
        crate::services::workspace::workspace_vm_service::cancel_waiting_automatic_continuation_in_tx(
            conn,
            workspace_id,
        )
        .await?;
    }
    Ok(())
}

/// Apply the non-running half of a workspace stop at the task authority.
/// Preview processes never survive a VM lifecycle boundary. Agent work is
/// held or retained according to the same reason policy used for a running
/// attempt's interrupted outcome.
pub(crate) async fn prepare_workspace_tasks_for_stop(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    reason: &str,
    operation_id: Uuid,
) -> Result<(), AppError> {
    cancel_queued_workspace_previews(db.conn(), workspace_id, "workspace_stop").await?;

    let (status, hold_reason) = match reason {
        "drain" | "rollout" => ("queued", None),
        "idle" => ("waiting", Some("idle_stop")),
        "credential" | "capacity" | "member_deactivated" | "execution_hold" => {
            ("waiting", Some("start_required"))
        }
        "transfer" => ("waiting", Some("transfer")),
        _ => ("waiting", Some("owner_stop")),
    };
    sqlx::query(
        "UPDATE tasks
         SET status = $2, hold_reason = $3,
             spec = jsonb_set(
                 spec, '{lifecycle}',
                 COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                     jsonb_build_object(
                         'interrupt_operation_id',$4::text,
                         'interrupt_reason',$5::text,
                         'interrupt_requested_at',now()),
                 true),
             updated_at = now()
         WHERE workspace_id = $1 AND kind IN ('agent','command') AND status = 'queued'",
    )
    .bind(workspace_id)
    .bind(status)
    .bind(hold_reason)
    .bind(operation_id)
    .bind(reason)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// Mark the running tasks of one incarnation interrupted by a stop operation,
/// returning each task's attempt and acknowledged event sequence.
pub(crate) async fn mark_running_tasks_interrupted(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    vm_id: Uuid,
    epoch: u64,
    operation_id: Uuid,
    reason: &str,
) -> Result<Vec<(Uuid, i32, i64)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, i32, i64)>(
        "WITH interrupted AS (
             UPDATE tasks
             SET spec = jsonb_set(
                     spec,
                     '{lifecycle}',
                     COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                         jsonb_build_object(
                             'interrupt_operation_id',$4::text,
                             'interrupt_reason',$5::text,
                             'interrupt_requested_at',now()),
                     true),
                 updated_at = now()
             WHERE workspace_id = $1 AND status = 'running'
               AND vm_id = $2 AND epoch = $3
             RETURNING id, attempt
         )
         SELECT interrupted.id, interrupted.attempt, attempt.acked_event_seq
         FROM interrupted
         JOIN task_attempts AS attempt
           ON attempt.task_id = interrupted.id
          AND attempt.attempt = interrupted.attempt",
    )
    .bind(workspace_id)
    .bind(vm_id.to_string())
    .bind(i64::try_from(epoch).unwrap_or(i64::MAX))
    .bind(operation_id)
    .bind(reason)
    .fetch_all(conn)
    .await
}

/// Record that the interrupts of a stop operation were committed.
pub(crate) async fn commit_task_interrupts(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    operation_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(
                 spec,
                 '{lifecycle}',
                 COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                     jsonb_build_object(
                         'interrupt_committed_at',now(),
                         'interrupt_commit_operation_id',$2::text),
                 true),
             updated_at = now()
         WHERE workspace_id = $1
           AND spec #>> '{lifecycle,interrupt_operation_id}' = $2::text",
    )
    .bind(workspace_id)
    .bind(operation_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Settle Agents after a VM release, in the releasing transaction.
pub(crate) async fn settle_agents_after_release(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    settlement: ReleaseSettlement,
) -> Result<(), sqlx::Error> {
    match settlement {
        ReleaseSettlement::Lost(reason) => {
            cancel_queued_workspace_previews(conn, workspace_id, reason).await?;
            mark_queued_agent_turns_lost(conn, workspace_id, reason).await?;
            crate::services::agents::chat_service::withdraw_workspace_notices(
                conn,
                workspace_id,
                reason,
            )
            .await
        }
        ReleaseSettlement::Stopped => {
            crate::services::agents::chat_service::withdraw_workspace_notices(
                conn,
                workspace_id,
                "workspace_stopped",
            )
            .await
        }
    }
}

/// A queued agent turn was active behind the lost compute
/// (`g5.chat.activity.turns`). It stays queued, and its lifecycle marker
/// records the loss so the lifecycle owner can tell it from a turn queued
/// behind stopped compute (`g5.lifecycle.start.automatic-continuation`).
/// Only a release that lost a VM which had reached Ready is a loss of running
/// compute: it left the VM `failed`, or `stopped` because the lease expired
/// before an unfinished stop reached the host (`lease_expired_during_stop`).
/// An exit that completes a confirmed stop settles through the same path and
/// is not a loss, and a Start that failed before Ready ends its own consent
/// (`g5.lifecycle.start`) rather than interrupting work. The marker carries
/// the lost incarnation's epoch so a later incarnation's decision does not
/// read it as its own. Dispatch removes the marker as it does a stop's.
async fn mark_queued_agent_turns_lost(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tasks
         SET spec = jsonb_set(
                 spec, '{lifecycle}',
                 COALESCE(spec -> 'lifecycle', '{}'::jsonb) ||
                     jsonb_build_object(
                         'interrupt_reason', 'vm_lost',
                         'interrupt_detail', $2::text,
                         'interrupt_epoch', (SELECT epoch FROM workspace_vms
                                             WHERE workspace_id = $1),
                         'interrupt_requested_at', now()),
                 true),
             updated_at = now()
         WHERE workspace_id = $1 AND kind = 'agent' AND status = 'queued'
           AND EXISTS (
               SELECT 1 FROM workspace_vms AS vm
               WHERE vm.workspace_id = $1 AND vm.started_at IS NOT NULL
                 AND (vm.status = 'failed'
                      OR vm.last_error ->> 'reason' = 'lease_expired_during_stop')
           )",
    )
    .bind(workspace_id)
    .bind(reason)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Cancel preview work which has no live attempt when its workspace loses or
/// deliberately leaves its current VM lifecycle. Running previews are settled
/// by the ordinary attempt loss/stop projection.
pub(crate) async fn cancel_queued_workspace_previews(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tasks
         SET status = 'cancelled',
             outcome = jsonb_build_object('status','cancelled','reason',$2::text),
             hold_reason = NULL, continuation_reason = NULL,
             start_consent_id = NULL,
             ended_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND kind = 'preview'
           AND status IN ('queued','waiting')",
    )
    .bind(workspace_id)
    .bind(reason)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_intent = CASE
                 WHEN preview_intent IS NULL THEN NULL
                 ELSE jsonb_set(preview_intent, '{status}', '\"settled\"') END,
             updated_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Transfer is an ownership-generation boundary. Executions and live native
/// requests from the former owner are already quiesced by the workspace stop.
/// Preserve logical work and queued input under an explicit new-owner hold.
pub(crate) async fn hold_workspace_tasks_for_transfer(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE task_attempts AS attempt
         SET ended_at = COALESCE(attempt.ended_at, now()),
             reason = COALESCE(attempt.reason, 'cancelled'),
             outcome = COALESCE(attempt.outcome,
                 jsonb_build_object('status','cancelled','reason','ownership_transferred'))
         FROM tasks AS task
         WHERE task.workspace_id = $1 AND task.kind = 'preview'
           AND task.id = attempt.task_id AND attempt.ended_at IS NULL",
    )
    .bind(workspace_id)
    .execute(db.conn())
    .await?;
    cancel_queued_workspace_previews(db.conn(), workspace_id, "ownership_transferred").await?;
    sqlx::query(
        "UPDATE tasks
         SET status = 'cancelled',
             outcome = jsonb_build_object(
                 'status','cancelled','reason','ownership_transferred'),
             hold_reason = NULL, continuation_reason = NULL,
             vm_id = NULL, epoch = NULL, start_confirmed = false,
             ended_at = now(), updated_at = now()
         WHERE workspace_id = $1 AND kind = 'preview' AND status = 'running'",
    )
    .bind(workspace_id)
    .execute(db.conn())
    .await?;
    let running = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tasks
         WHERE workspace_id = $1 AND kind IN ('agent','command') AND status = 'running')",
    )
    .bind(workspace_id)
    .fetch_one(db.conn())
    .await?;
    if running {
        return Err(conflict(
            "workspace transfer still has a running task after lifecycle quiesce",
        ));
    }
    sqlx::query(
        "UPDATE provider_requests AS request
         SET status = 'invalidated', ended_at = now(),
             end_reason = 'ownership_transferred'
         FROM tasks AS task
         WHERE task.workspace_id = $1 AND task.id = request.task_id
           AND request.status IN ('pending','reply_pending')
           AND request.resume_session_id IS NULL",
    )
    .bind(workspace_id)
    .execute(db.conn())
    .await?;
    sqlx::query(
        "UPDATE tasks
         SET status = 'waiting', outcome = NULL,
             hold_reason = 'transfer',
             vm_id = NULL, epoch = NULL, start_confirmed = false,
             ended_at = NULL, updated_at = now()
         WHERE workspace_id = $1 AND kind IN ('agent','command')
           AND status IN ('queued','waiting')
           AND (hold_reason IS NULL OR hold_reason IN (
               'owner_stop','idle_stop','transfer','start_required','vm_lost'))",
    )
    .bind(workspace_id)
    .execute(db.conn())
    .await?;
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_intent = CASE
                 WHEN preview_intent IS NULL THEN NULL
                 ELSE jsonb_set(preview_intent, '{status}', '\"settled\"') END,
             updated_at = now()
         WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .execute(db.conn())
    .await?;
    Ok(())
}

/// An explicit workspace Resume authorizes held logical work for the new
/// running incarnation. Delivery-unknown and response-timeout holds still
/// require their dedicated owner decisions.
pub(crate) async fn release_workspace_tasks_after_resume(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tasks
         SET status = 'queued', hold_reason = NULL,
             spec = spec - 'lifecycle', updated_at = now()
         WHERE workspace_id = $1 AND kind IN ('agent','command')
           AND status = 'waiting'
           AND hold_reason IN (
               'owner_stop','idle_stop','transfer','start_required','vm_lost')
           AND NOT EXISTS (SELECT 1 FROM provider_requests request WHERE request.task_id = tasks.id
               AND request.resume_session_id IS NOT NULL AND request.status = 'pending')",
    )
    .bind(workspace_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn settle_task_row(
    db: &mut ScopedDb,
    locked: &LockedAttempt,
    task_id: Uuid,
    attempt: i32,
    incarnation: (Uuid, u64),
    decision: &AttemptDecision,
    outcome: Option<Value>,
) -> Result<(), AppError> {
    let (vm_id, epoch) = incarnation;
    settle_steering_inputs(db, locked, task_id, attempt).await?;
    sqlx::query(
        "UPDATE task_attempts
         SET codex_auth_challenge_enc=NULL,
             codex_auth_state=CASE
                 WHEN codex_auth_state ->> 'phase' IN ('checking','waiting')
                 THEN jsonb_build_object('phase','ended','code','codex_auth_interrupted')
                 ELSE codex_auth_state END
         WHERE task_id=$1 AND attempt=$2",
    )
    .bind(task_id)
    .bind(attempt)
    .execute(db.conn())
    .await?;
    let mut decision = decision.clone();
    if !decision.terminal && decision.hold_reason != Some("delivery_unknown") {
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM provider_requests
            WHERE task_id = $1 AND resume_session_id IS NOT NULL
              AND status IN ('pending','reply_pending') ORDER BY attempt DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?;
        if status.as_deref() == Some("pending") {
            decision.task_status = "waiting";
            decision.hold_reason = Some("awaiting_answer");
        } else if status.as_deref() == Some("reply_pending")
            && decision.hold_reason == Some("awaiting_answer")
        {
            decision.task_status = "queued";
            decision.hold_reason = None;
        } else if status.is_none() && decision.hold_reason == Some("awaiting_answer") {
            // The deferred question this journaled suspension waited on was
            // invalidated (owner pause/stop, timeout) before the suspension
            // applied. Nothing can answer it any more; hold for the owner
            // instead of an impossible answer
            // (`g5.chat.native-request-boundary`).
            decision.hold_reason = Some("owner_stop");
        }
    }
    let updated = sqlx::query(
        "UPDATE tasks
         SET status = $5,
             outcome = CASE WHEN $6 THEN $7 ELSE NULL END,
             continuation_reason = $8,
             hold_reason = $9,
             vm_id = NULL, epoch = NULL, start_confirmed = false,
             ended_at = CASE WHEN $6 THEN now() ELSE NULL END,
             updated_at = now()
         WHERE id = $1 AND attempt = $2 AND vm_id = $3 AND epoch = $4
           AND status = 'running'",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(vm_id.to_string())
    .bind(i64::try_from(epoch).map_err(|_| conflict("epoch exceeds bigint"))?)
    .bind(decision.task_status)
    .bind(decision.terminal)
    .bind(outcome)
    .bind(decision.continuation_reason)
    .bind(decision.hold_reason)
    .execute(db.conn())
    .await?;
    if updated.rows_affected() != 1 {
        return Err(conflict("task execution is no longer current"));
    }
    Ok(())
}

async fn mark_reserved_input_unknown(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    reason: &str,
) -> Result<bool, AppError> {
    let updated = sqlx::query(
        "UPDATE session_inputs
         SET delivery = 'delivery_unknown',
             delivery_audit = delivery_audit || jsonb_build_array(
                 jsonb_build_object('event','delivery_unknown','reason',$3))
         WHERE task_id = $1 AND attempt = $2 AND delivery = 'reserved'",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(reason)
    .execute(db.conn())
    .await?;
    Ok(updated.rows_affected() != 0)
}

async fn withdraw_unknown_input_on_cancel(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE session_inputs
         SET delivery = 'withdrawn', withdrawn_at = now(),
             delivery_audit = delivery_audit || jsonb_build_array(
                 jsonb_build_object('event','task_cancelled'))
         WHERE task_id = $1 AND attempt = $2 AND delivery = 'delivery_unknown'",
    )
    .bind(task_id)
    .bind(attempt)
    .execute(db.conn())
    .await?;
    Ok(())
}

async fn release_unaccepted_input(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    terminal: bool,
) -> Result<(), AppError> {
    if terminal {
        sqlx::query(
            "UPDATE session_inputs
             SET delivery = 'withdrawn', withdrawn_at = now(),
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object('event','cancelled_before_acceptance'))
             WHERE task_id = $1 AND attempt = $2 AND delivery = 'reserved'",
        )
        .bind(task_id)
        .bind(attempt)
        .execute(db.conn())
        .await?;
        withdraw_task_system_notices(db, task_id, attempt, "attempt_ended_before_acceptance")
            .await?;
    } else {
        sqlx::query(
            "UPDATE session_inputs
             SET delivery = CASE
                     WHEN input_origin = 'continuation' THEN 'withdrawn'
                     ELSE 'queued'
                 END,
                 withdrawn_at = CASE
                     WHEN input_origin = 'continuation' THEN now()
                     ELSE NULL
                 END,
                 reservation_id = NULL,
                 reservation_revision = NULL,
                 delivery_audit = delivery_audit || jsonb_build_array(
                     jsonb_build_object(
                         'event', CASE
                             WHEN input_origin = 'continuation'
                             THEN 'continuation_withdrawn_before_acceptance'
                             ELSE 'released_before_acceptance'
                         END))
             WHERE task_id = $1 AND attempt = $2 AND delivery = 'reserved'",
        )
        .bind(task_id)
        .bind(attempt)
        .execute(db.conn())
        .await?;
    }
    Ok(())
}

pub(crate) async fn invalidate_attempt_requests(
    db: &mut ScopedDb,
    task_id: Uuid,
    attempt: i32,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE provider_requests
         SET status = 'invalidated', ended_at = now(), end_reason = $3
         WHERE task_id = $1 AND (attempt = $2 OR resume_session_id IS NOT NULL)
           AND status IN ('pending','reply_pending')
           AND (resume_session_id IS NULL OR $3 IN ('owner_cancel','owner_pause','chat_stop')
               OR EXISTS (SELECT 1 FROM tasks WHERE id = $1 AND status IN ('succeeded','failed','cancelled')))",
    )
    .bind(task_id)
    .bind(attempt)
    .bind(reason)
    .execute(db.conn())
    .await?;
    Ok(())
}

fn outcome_decision(
    kind: TaskKind,
    outcome: &Value,
    cancel_requested: bool,
) -> Result<AttemptDecision, AppError> {
    let outer = outcome["status"]
        .as_str()
        .ok_or_else(|| conflict("module outcome status is absent"))?;
    let inner = agent_result(outcome)["status"].as_str();
    if kind != TaskKind::Agent {
        return Ok(match outer {
            "succeeded" => final_decision("succeeded", "exit_code"),
            "cancelled" => final_decision("cancelled", "cancelled"),
            "timed_out" | "limit_exceeded" => final_decision("failed", "timed_out"),
            "interrupted" => final_decision("failed", "interrupted"),
            "failed" => final_decision("failed", "provider_error"),
            other => return Err(conflict(format!("unsupported module outcome {other}"))),
        });
    }
    let effective = if outer == "succeeded" {
        inner.unwrap_or("succeeded")
    } else {
        outer
    };
    Ok(match effective {
        "succeeded" => final_decision("succeeded", "exit_code"),
        "cancelled" if cancel_requested => final_decision("cancelled", "cancelled"),
        // A cancellation the owner asked for is final whatever interruption
        // reached the attempt first (`g5.agents.lifecycle.cancel`).
        "interrupted" if cancel_requested => final_decision("cancelled", "interrupted"),
        "suspended" if cancel_requested => final_decision("cancelled", "suspended"),
        "cancelled" | "interrupted" => AttemptDecision {
            task_status: "waiting",
            attempt_reason: if effective == "cancelled" {
                "cancelled"
            } else {
                "interrupted"
            },
            continuation_reason: Some("interrupted"),
            hold_reason: Some("owner_stop"),
            terminal: false,
        },
        "suspended" if agent_result(outcome)["code"] == "native_question_deferred" => {
            AttemptDecision {
                task_status: "waiting",
                attempt_reason: "suspended",
                continuation_reason: Some("suspended"),
                hold_reason: Some("awaiting_answer"),
                terminal: false,
            }
        }
        // The adapter deferred a question but its saved session no longer
        // offers it (the process went on, or capture failed) and has already
        // invalidated the request: hold for the owner, not for an answer
        // (`g5.chat.native-question-recovery`).
        "suspended" if agent_result(outcome)["code"] == "native_question_lost" => AttemptDecision {
            task_status: "waiting",
            attempt_reason: "suspended",
            continuation_reason: Some("suspended"),
            hold_reason: Some("owner_stop"),
            terminal: false,
        },
        "suspended" => AttemptDecision {
            task_status: "waiting",
            attempt_reason: "suspended",
            continuation_reason: Some("suspended"),
            hold_reason: Some("response_timeout"),
            terminal: false,
        },
        "timed_out" => final_decision("failed", "timed_out"),
        "limit_exceeded" => final_decision("failed", "limit_exceeded"),
        "failed" => final_decision("failed", "provider_error"),
        other => return Err(conflict(format!("unsupported agent outcome {other}"))),
    })
}

/// A preview's cancelled outcome says why it ended
/// (`g5.preview.service.stop-reason`): the owner's cancel, or the workspace
/// stop that interrupted it. The module reports a bare "cancelled" either way.
/// A preview lost with its VM (`apply_vm_loss`) or cancelled before its start
/// was accepted (`apply_start_disposition`) is settled with the same reasons.
fn preview_outcome_with_reason(
    payload: &Value,
    task_spec: &Value,
    cancel_requested: bool,
) -> Value {
    let mut outcome = payload.clone();
    if outcome["status"] != "cancelled" || outcome["reason"].is_string() {
        return outcome;
    }
    let reason = if cancel_requested {
        Some("owner_cancel")
    } else {
        task_spec
            .pointer("/lifecycle/interrupt_reason")
            .and_then(Value::as_str)
            .map(preview_stop_reason)
    };
    if let (Some(reason), Some(object)) = (reason, outcome.as_object_mut()) {
        object.insert("reason".to_string(), Value::String(reason.to_string()));
    }
    outcome
}

/// The owner-facing reason for a stop interrupt, in the hold-reason vocabulary.
fn preview_stop_reason(interrupt_reason: &str) -> &'static str {
    match interrupt_reason {
        "idle" => "idle_stop",
        "transfer" => "transfer",
        "credential" | "capacity" | "member_deactivated" | "execution_hold" => "start_required",
        "drain" | "rollout" => "host_drain",
        "user" | "delete" => "owner_stop",
        _ => "workspace_stop",
    }
}

fn auth_before_delivery(locked: &LockedAttempt) -> bool {
    locked.task_kind == TaskKind::Agent
        && locked
            .codex_auth_state
            .as_ref()
            .is_some_and(|state| state["phase"] != "ready")
}

fn apply_interruption_policy(
    decision: AttemptDecision,
    kind: TaskKind,
    cancel_requested: bool,
    task_spec: &Value,
) -> AttemptDecision {
    if kind != TaskKind::Agent || cancel_requested || decision.terminal {
        return decision;
    }
    match task_spec
        .pointer("/lifecycle/interrupt_reason")
        .and_then(Value::as_str)
    {
        Some("response_timeout") => AttemptDecision {
            task_status: "waiting",
            attempt_reason: decision.attempt_reason,
            continuation_reason: Some("suspended"),
            hold_reason: Some("response_timeout"),
            terminal: false,
        },
        Some("idle") => AttemptDecision {
            task_status: "waiting",
            attempt_reason: decision.attempt_reason,
            continuation_reason: Some("interrupted"),
            hold_reason: Some("idle_stop"),
            terminal: false,
        },
        Some("transfer") => AttemptDecision {
            task_status: "waiting",
            attempt_reason: decision.attempt_reason,
            continuation_reason: Some("interrupted"),
            hold_reason: Some("transfer"),
            terminal: false,
        },
        Some("drain" | "rollout") => AttemptDecision {
            task_status: "queued",
            attempt_reason: decision.attempt_reason,
            continuation_reason: Some("interrupted"),
            hold_reason: None,
            terminal: false,
        },
        Some("credential" | "capacity" | "member_deactivated" | "execution_hold") => {
            AttemptDecision {
                task_status: "waiting",
                attempt_reason: decision.attempt_reason,
                continuation_reason: Some("interrupted"),
                hold_reason: Some("start_required"),
                terminal: false,
            }
        }
        _ => decision,
    }
}

/// A cancelled start disposition proves the module never accepted the turn.
/// Retry the original reserved input instead of fabricating a continuation
/// turn that would have no provider session or accepted conversation state.
fn unstarted_agent_interruption_policy(
    task_spec: &Value,
    continuation_reason: Option<&str>,
) -> AttemptDecision {
    let (task_status, hold_reason) = match task_spec
        .pointer("/lifecycle/interrupt_reason")
        .and_then(Value::as_str)
    {
        Some("idle") => ("waiting", Some("idle_stop")),
        Some("transfer") => ("waiting", Some("transfer")),
        Some("credential" | "capacity" | "member_deactivated" | "execution_hold") => {
            ("waiting", Some("start_required"))
        }
        Some("user" | "delete") => ("waiting", Some("owner_stop")),
        Some("response_timeout") => ("waiting", Some("response_timeout")),
        _ => ("queued", None),
    };
    AttemptDecision {
        task_status,
        attempt_reason: "cancelled_before_start",
        continuation_reason: match continuation_reason {
            Some("suspended") => Some("suspended"),
            Some("vm_lost") => Some("vm_lost"),
            Some(_) => Some("interrupted"),
            None => None,
        },
        hold_reason,
        terminal: false,
    }
}

fn vm_loss_decision(kind: TaskKind) -> AttemptDecision {
    match kind {
        TaskKind::Agent => AttemptDecision {
            task_status: "waiting",
            attempt_reason: "vm_lost",
            continuation_reason: Some("vm_lost"),
            hold_reason: Some("vm_lost"),
            terminal: false,
        },
        TaskKind::Command | TaskKind::Preview => final_decision("failed", "vm_lost"),
    }
}

fn final_decision(task_status: &'static str, attempt_reason: &'static str) -> AttemptDecision {
    AttemptDecision {
        task_status,
        attempt_reason,
        continuation_reason: None,
        hold_reason: None,
        terminal: true,
    }
}

fn agent_result(outcome: &Value) -> &Value {
    outcome.get("result").unwrap_or(&Value::Null)
}

pub fn canonical_agent_message_id(task_id: Uuid, attempt: i32) -> Uuid {
    Uuid::new_v5(&task_id, format!("agent-attempt:{attempt}").as_bytes())
}

pub fn response_digest(response: &Value) -> Result<String, AppError> {
    let mut bytes = Vec::new();
    write_canonical_json(response, &mut bytes)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn write_canonical_json(value: &Value, output: &mut Vec<u8>) -> Result<(), AppError> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            serde_json::to_writer(output, value)?;
        }
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_canonical_json(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            output.push(b'{');
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                serde_json::to_writer(&mut *output, key)?;
                output.push(b':');
                write_canonical_json(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn validate_native_response(kind: &str, payload: &Value, response: &Value) -> Result<(), AppError> {
    let object = response
        .as_object()
        .ok_or_else(|| conflict("validated response must be an object"))?;
    match kind {
        "question" => {
            g5_contracts::types::native_question::resolve_native_question_answers(
                payload, response,
            )
            .map_err(conflict)?;
        }
        "permission" => {
            let behavior = object.get("behavior").and_then(Value::as_str);
            if !matches!(behavior, Some("allow" | "deny"))
                || object
                    .keys()
                    .any(|key| !matches!(key.as_str(), "behavior" | "updated_input" | "message"))
            {
                return Err(conflict("permission response is invalid"));
            }
            if behavior == Some("allow")
                && object
                    .get("updated_input")
                    .is_some_and(|value| !value.is_object())
            {
                return Err(conflict("permission updated_input must be an object"));
            }
        }
        _ => return Err(conflict("provider request kind is unsupported")),
    }
    Ok(())
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn runtime_app_error(error: crate::services::workspace_runtime::RuntimeRpcError) -> AppError {
    let typed = match error.message.as_str() {
        "WORKSPACE_STOPPING" => G5Error::WorkspaceStopping,
        "STALE_INCARNATION" | "LEASE_EXPIRED" | "stale_execution" => G5Error::Conflict {
            reason: error.message,
        },
        _ if error.code == 403 => G5Error::Forbidden {
            missing_scopes: Vec::new(),
        },
        _ if error.code == 409 => G5Error::Conflict {
            reason: error.message,
        },
        _ => G5Error::ServiceUnavailable {
            message: format!("workspace runtime call failed: {}", error.message),
        },
    };
    AppError::from_error(typed)
}

fn timeline_type(kind: &str, payload: &Value) -> &'static str {
    match kind {
        "input.accepted" => "user_message",
        "message.final" => "agent_message",
        "message.delta" => "output",
        "thought.final" => "thought",
        "tool.started" => "tool_call",
        "tool.completed" => "tool_result",
        "notice" if payload["level"].as_str() == Some("error") => "error",
        _ => "checkpoint",
    }
}

fn timeline_content(kind: &str, payload: &Value) -> String {
    let content = payload["content"]
        .as_str()
        .or_else(|| payload["text_delta"].as_str())
        .or_else(|| payload["message"].as_str())
        .unwrap_or(kind);
    truncate_utf8(content, MAX_TIMELINE_CONTENT_BYTES)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn payload_uuid(payload: &Value, field: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(
        payload[field]
            .as_str()
            .ok_or_else(|| conflict(format!("{field} is absent")))?,
    )
    .map_err(|_| conflict(format!("{field} is not a UUID")))
}

fn payload_uuid_array(payload: &Value, field: &str) -> Result<Vec<Uuid>, AppError> {
    let Some(values) = payload.get(field) else {
        return Ok(Vec::new());
    };
    values
        .as_array()
        .ok_or_else(|| conflict(format!("{field} is not an array")))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| conflict(format!("{field} contains a non-string UUID")))
                .and_then(|value| {
                    Uuid::parse_str(value)
                        .map_err(|_| conflict(format!("{field} contains an invalid UUID")))
                })
        })
        .collect()
}

fn payload_i32(payload: &Value, field: &str) -> Result<i32, AppError> {
    let value = payload[field]
        .as_i64()
        .ok_or_else(|| conflict(format!("{field} is absent")))?;
    i32::try_from(value).map_err(|_| conflict(format!("{field} exceeds integer range")))
}

fn payload_string(payload: &Value, field: &str, max: usize) -> Result<String, AppError> {
    let value = payload[field]
        .as_str()
        .ok_or_else(|| conflict(format!("{field} is absent")))?;
    if value.is_empty() || value.len() > max {
        return Err(conflict(format!("{field} has invalid length")));
    }
    Ok(value.to_string())
}

fn payload_digest(payload: &Value, field: &str) -> Result<String, AppError> {
    let value = payload_string(payload, field, 64)?.to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(conflict(format!("{field} is not a SHA-256 digest")));
    }
    Ok(value)
}

fn ensure_json_bound(value: &Value, max: usize, name: &str) -> Result<(), AppError> {
    if serde_json::to_vec(value)?.len() > max {
        return Err(conflict(format!("{name} exceeds {max} bytes")));
    }
    Ok(())
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "cancelled")
}

/// The identity of a preview start request, which a replay must match
/// (`g5.runtime.preview.admission`): the repository and the profile.
pub fn preview_request_digest(repo_id: Uuid, profile: Option<&str>) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!({"repo_id": repo_id, "profile": profile}))
                .expect("preview request digest serializes")
        )
    )
}

/// The request identity recorded before the configured port left the
/// digest; replays of intents recorded then still match it.
pub fn legacy_preview_request_digest(repo_id: Uuid, profile: Option<&str>, port: u16) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!({"repo_id": repo_id, "profile": profile, "port": port}))
                .expect("preview request digest serializes")
        )
    )
}

pub(crate) async fn settle_preview_intent(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    task_id: Uuid,
    status: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE workspace_vms
         SET preview_intent = jsonb_set(preview_intent, '{status}', to_jsonb($3::text), false),
             updated_at = now()
         WHERE workspace_id = $1 AND preview_intent ->> 'task_id' = $2::text",
    )
    .bind(workspace_id)
    .bind(task_id)
    .bind(status)
    .execute(db.conn())
    .await?;
    Ok(())
}

fn conflict(reason: impl Into<String>) -> AppError {
    AppError::from_error(G5Error::Conflict {
        reason: reason.into(),
    })
}

#[async_trait::async_trait]
impl AgentWorkSettlement for TaskService {
    async fn settle_after_release(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        settlement: ReleaseSettlement,
    ) -> Result<(), sqlx::Error> {
        settle_agents_after_release(conn, workspace_id, settlement).await
    }

    async fn release_tasks_after_resume(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        release_workspace_tasks_after_resume(conn, workspace_id).await
    }
}

/// The durable attempt identity a replayed record binds to: the dispatcher
/// cannot choose a different task, attempt, VM or epoch during recovery.
fn bind_replay_event(
    execution: &StoppedExecution,
    event: &Value,
) -> Result<JournalTaskEvent, AppError> {
    journal_event_from_replay(
        execution.task_id,
        execution.attempt,
        execution.vm_id,
        execution.epoch,
        &execution.exec_id,
        event,
    )
}

#[async_trait::async_trait]
impl WorkspaceAgentWork for TaskService {
    async fn interrupted_work(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
    ) -> Result<InterruptedWork, sqlx::Error> {
        interrupted_work(conn, workspace_id).await
    }

    async fn continuable_work_remains(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        continuable_work_remains(conn, workspace_id).await
    }

    async fn announce_automatic_continuation(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        content: &str,
        operation_id: Uuid,
        details: Value,
    ) -> Result<(), sqlx::Error> {
        crate::services::agents::chat_service::notify_working_agents(
            conn,
            workspace_id,
            content,
            operation_id,
            details,
        )
        .await
    }

    async fn prepare_tasks_for_stop(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        reason: &str,
        operation_id: Uuid,
    ) -> Result<(), AppError> {
        prepare_workspace_tasks_for_stop(db, workspace_id, reason, operation_id).await
    }

    async fn mark_running_tasks_interrupted(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        vm_id: Uuid,
        epoch: u64,
        operation_id: Uuid,
        reason: &str,
    ) -> Result<Vec<(Uuid, i32, i64)>, sqlx::Error> {
        mark_running_tasks_interrupted(conn, workspace_id, vm_id, epoch, operation_id, reason).await
    }

    async fn commit_task_interrupts(
        &self,
        conn: &mut PgConnection,
        workspace_id: Uuid,
        operation_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        commit_task_interrupts(conn, workspace_id, operation_id).await
    }

    fn validate_replay_event(
        &self,
        execution: &StoppedExecution,
        event: &Value,
    ) -> Result<ReplayRecord, AppError> {
        let bound = bind_replay_event(execution, event)?;
        Ok(ReplayRecord {
            terminal: bound.terminal,
            event: event.clone(),
        })
    }

    async fn apply_replay_event(
        &self,
        db: &mut ScopedDb,
        execution: &StoppedExecution,
        record: &ReplayRecord,
    ) -> Result<Option<u64>, AppError> {
        let event = bind_replay_event(execution, &record.event)?;
        let applied = self.apply(db, TaskLifecycleEvent::Journal(event)).await?;
        Ok(applied.ack_through)
    }

    async fn apply_replay_batch(
        &self,
        db: &mut ScopedDb,
        execution: &StoppedExecution,
        records: &[ReplayRecord],
    ) -> Result<Option<u64>, AppError> {
        // Each drained record gets its own increasing receipt time: the
        // stored `created_at` orders the timeline.
        let base = Utc::now();
        let mut events = Vec::with_capacity(records.len());
        for (index, record) in records.iter().enumerate() {
            events.push(JournalBatchEvent {
                event: bind_replay_event(execution, &record.event)?,
                received_at: base + chrono::Duration::microseconds(index as i64),
                message_id: None,
            });
        }
        let mut segments = seed_segment_map(db, execution.task_id, execution.attempt).await?;
        let applied = self.apply_journal_batch(db, &mut segments, events).await?;
        for (channel, event) in &applied.live {
            self.event_bus.publish(channel, event);
        }
        Ok(applied.ack_through)
    }

    async fn retire_live_follower(&self, execution: &StoppedExecution) {
        self.followers.retire(&execution.exec_id).await;
    }

    async fn settle_stopped_execution(
        &self,
        db: &mut ScopedDb,
        execution: &StoppedExecution,
        outcome: StoppedExecutionOutcome,
    ) -> Result<(), AppError> {
        let StoppedExecution {
            task_id,
            attempt,
            vm_id,
            epoch,
            ..
        } = *execution;
        let event = match outcome {
            StoppedExecutionOutcome::Cancelled { reason } => TaskLifecycleEvent::StartDisposition {
                task_id,
                attempt,
                vm_id,
                epoch,
                disposition: json!({"disposition": "cancelled", "reason": reason}),
            },
            StoppedExecutionOutcome::Recovered { outcome } => {
                TaskLifecycleEvent::RecoveredOutcome {
                    task_id,
                    attempt,
                    vm_id,
                    epoch,
                    outcome,
                }
            }
            StoppedExecutionOutcome::Missing => TaskLifecycleEvent::VmLost {
                task_id,
                attempt,
                vm_id,
                epoch,
                evidence: json!({"reason": "stopped_execution_missing"}),
            },
        };
        self.apply(db, event).await.map(|_| ())
    }

    async fn hold_tasks_for_transfer(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<(), AppError> {
        // No chat.activity hint here: a transfer can revoke a subscriber's
        // read access in this transaction, and a revoked socket must receive
        // no application data. The held turns were active, so remaining
        // readers' lists re-read on their own (g5.chat.activity.refresh).
        hold_workspace_tasks_for_transfer(db, workspace_id).await
    }

    async fn append_lifecycle_notice(
        &self,
        conn: &mut PgConnection,
        session_id: Uuid,
        content: &str,
        operation_id: Uuid,
        stop_reason: &str,
        deadline_at: &str,
    ) -> Result<(), sqlx::Error> {
        crate::services::agents::chat_service::append_lifecycle_notice(
            conn,
            session_id,
            content,
            operation_id,
            stop_reason,
            deadline_at,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_reducer_pairs_native_blocks_and_keeps_unfinished_output() {
        let events = vec![
            ("message.delta".into(), json!({"text_delta":"Par"})),
            (
                "message.final".into(),
                json!({"block_id":"native-1:0","content":"Part one."}),
            ),
            ("tool.started".into(), json!({})),
            (
                "message.final".into(),
                json!({"block_id":"native-2:0","content":"Answer."}),
            ),
            (
                "message.final".into(),
                json!({"block_id":"native-2:0","content":"Answer."}),
            ),
            (
                "message.delta".into(),
                json!({"block_id":"native-3:0","text_delta":"Partial"}),
            ),
        ];
        assert_eq!(
            reduce_assistant_output(&events),
            "Part one.\n\nAnswer.\n\nPartial"
        );
    }

    fn module_outcome(outer: &str, inner: Option<&str>) -> Value {
        json!({
            "status": outer,
            "exit_code": 0,
            "result": inner.map(|status| json!({"status":status}))
        })
    }

    #[test]
    fn only_a_known_pre_spawn_outcome_can_restore_a_fence() {
        let pre_spawn = json!({"result":{"provider_started":false}});
        let rejected_resume = json!({"result":{"provider_started":true}});
        let old_outcome = json!({"result":{}});
        assert!(may_restore_fenced_publication(false, &pre_spawn));
        assert!(!may_restore_fenced_publication(true, &pre_spawn));
        assert!(!may_restore_fenced_publication(false, &rejected_resume));
        assert!(!may_restore_fenced_publication(false, &old_outcome));
    }

    #[test]
    fn exact_execution_identity_is_canonical() {
        let task = Uuid::parse_str("0199c0de-1111-7000-8000-00000000beef").unwrap();
        let vm = Uuid::parse_str("0199c0de-0000-7000-8000-00000000abcd").unwrap();
        assert_eq!(
            canonical_exec_id(task, 3, vm),
            "task.0199c0de-1111-7000-8000-00000000beef.3.0199c0de-0000-7000-8000-00000000abcd"
        );
    }

    #[test]
    fn final_agent_outcomes_settle_the_logical_task() {
        assert_eq!(
            outcome_decision(
                TaskKind::Agent,
                &module_outcome("succeeded", Some("succeeded")),
                false
            )
            .unwrap(),
            final_decision("succeeded", "exit_code")
        );
        assert_eq!(
            outcome_decision(TaskKind::Agent, &module_outcome("failed", None), false).unwrap(),
            final_decision("failed", "provider_error")
        );
        assert_eq!(
            outcome_decision(TaskKind::Agent, &module_outcome("cancelled", None), true).unwrap(),
            final_decision("cancelled", "cancelled")
        );
        for (outcome, reason) in [
            (module_outcome("interrupted", None), "interrupted"),
            (
                module_outcome("succeeded", Some("interrupted")),
                "interrupted",
            ),
            (module_outcome("succeeded", Some("suspended")), "suspended"),
        ] {
            assert_eq!(
                outcome_decision(TaskKind::Agent, &outcome, true).unwrap(),
                final_decision("cancelled", reason),
                "a requested cancellation is final after {outcome}"
            );
        }
    }

    #[test]
    fn lifecycle_interruptions_hold_the_same_logical_agent_task() {
        let interrupted =
            outcome_decision(TaskKind::Agent, &module_outcome("cancelled", None), false).unwrap();
        assert_eq!(interrupted.task_status, "waiting");
        assert_eq!(interrupted.continuation_reason, Some("interrupted"));
        assert_eq!(interrupted.hold_reason, Some("owner_stop"));
        assert!(!interrupted.terminal);

        let suspended = outcome_decision(
            TaskKind::Agent,
            &module_outcome("succeeded", Some("suspended")),
            false,
        )
        .unwrap();
        assert_eq!(suspended.task_status, "waiting");
        assert_eq!(suspended.continuation_reason, Some("suspended"));
        assert_eq!(suspended.hold_reason, Some("response_timeout"));
    }

    #[test]
    fn vm_loss_never_respawns_preview_and_holds_agent() {
        assert_eq!(vm_loss_decision(TaskKind::Agent).task_status, "waiting");
        let preview = vm_loss_decision(TaskKind::Preview);
        assert_eq!(preview.task_status, "failed");
        assert!(preview.terminal);
    }

    #[test]
    fn response_digest_uses_canonical_serialized_body() {
        let body = json!({"answers":{"q1":"yes"}});
        let reordered = serde_json::from_str::<Value>(
            r#"{"nested":{"z":2,"a":1},"answers":{"q2":"no","q1":"yes"}}"#,
        )
        .unwrap();
        let canonical_order = serde_json::from_str::<Value>(
            r#"{"answers":{"q1":"yes","q2":"no"},"nested":{"a":1,"z":2}}"#,
        )
        .unwrap();
        assert_eq!(response_digest(&body).unwrap().len(), 64);
        assert_eq!(
            response_digest(&body).unwrap(),
            response_digest(&body).unwrap()
        );
        assert_eq!(
            response_digest(&reordered).unwrap(),
            response_digest(&canonical_order).unwrap()
        );
    }

    #[test]
    fn oversized_native_request_is_a_consumable_projection_error() {
        let locked = LockedAttempt {
            org_id: Uuid::nil(),
            workspace_id: Uuid::nil(),

            task_kind: TaskKind::Agent,
            task_status: "running".to_string(),
            chat_session_id: Some(Uuid::nil()),
            continuation_reason: None,
            task_spec: json!({}),
            policy_snapshot: None,
            codex_auth_state: None,
            acked_event_seq: 0,
            ended: false,
        };
        let event = JournalTaskEvent {
            task_id: Uuid::nil(),
            attempt: 1,
            vm_id: Uuid::nil(),
            epoch: 1,
            exec_id: "test".to_string(),
            seq: 1,
            kind: "provider_request.opened".to_string(),
            payload: json!({
                "provider_request_id": "request-1",
                "request_kind": "permission",
                "payload": {"content":"x".repeat(MAX_NATIVE_REQUEST_BYTES)},
                "display": {},
                "deadline_unix_ms": Utc::now().timestamp_millis() + 60_000,
            }),
            terminal: false,
        };

        let error = provider_request_shape_error(&locked, &event)
            .expect("oversized module event should be rejected without wedging replay");
        assert!(error.contains("payload exceeds"));

        let mut padded = event;
        padded.payload["payload"] = json!({});
        padded.payload["provider_request_id"] = json!(" request-1 ");
        let error = provider_request_shape_error(&locked, &padded)
            .expect("an untrimmed identity violates the persistence contract");
        assert!(error.contains("provider_request_id"));
    }

    #[test]
    fn native_response_shape_keeps_question_and_permission_semantics_distinct() {
        let payload = json!({"questions":[{"question":"q","options":[{"label":"yes"}]}]});
        assert!(validate_native_response(
            "question",
            &payload,
            &json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[0]}}})
        )
        .is_ok());
        assert!(
            validate_native_response("question", &payload, &json!({"behavior":"allow"})).is_err()
        );
        assert!(
            validate_native_response("permission", &json!({}), &json!({"behavior":"deny"})).is_ok()
        );
        assert!(validate_native_response(
            "permission",
            &json!({}),
            &json!({"g5AnswersByIndex":{}})
        )
        .is_err());
    }

    #[test]
    fn native_answers_validate_structure_even_when_question_texts_collide() {
        let payload = json!({"questions":[
            {"question":"[REDACTED]","options":[{"label":"[REDACTED]"}]},
            {"question":"[REDACTED]","options":[{"label":"[REDACTED]"}]}
        ]});
        let answer = json!({"g5AnswersByIndex":{
            "0":{"selectedOptionIndexes":[0]},
            "1":{"selectedOptionIndexes":[],"otherText":"A custom answer 🟦"}
        }});
        assert!(validate_native_response("question", &payload, &answer).is_ok());
        for malformed in [
            json!({}),
            json!({"questions":[]}),
            json!({"questions":[{"question":" ","options":[]}]} ),
        ] {
            assert!(validate_native_response("question", &malformed, &answer).is_err());
        }
        assert!(
            validate_native_response("question", &payload, &json!({"g5AnswersByIndex":{}}))
                .is_err()
        );
    }

    #[test]
    fn timeline_truncation_preserves_utf8_boundaries() {
        let content = "é".repeat(MAX_TIMELINE_CONTENT_BYTES);
        let truncated = truncate_utf8(&content, MAX_TIMELINE_CONTENT_BYTES);
        assert!(truncated.len() <= MAX_TIMELINE_CONTENT_BYTES);
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn workspace_preview_view_uses_browser_contract_field_names() {
        let id = Uuid::nil();
        let value = serde_json::to_value(WorkspacePreviewView {
            workspace_id: id,
            task_id: id,
            status: "running".to_string(),
            repo_id: id,
            profile: Some("web".to_string()),
            port: 8080,
            generation: 1,
            ready: true,
            pending_start: false,
            closing: true,
            close: None,
            ending: None,
        })
        .unwrap();
        assert_eq!(value["workspaceId"], id.to_string());
        assert_eq!(value["pendingStart"], false);
        assert!(value.get("ending").is_none());
        assert_eq!(value["closing"], true);
        assert!(value["close"].is_null());
        assert_eq!(value["taskId"], id.to_string());
        assert_eq!(value["repoId"], id.to_string());
        assert!(value.get("task_id").is_none());
    }
    /// Activity's preview projection (ui `g5.ui.activity.preview-row`): the
    /// stage from the live task's facts, the ending from its outcome.
    #[test]
    fn preview_run_projection_names_stages_and_endings() {
        let spec = json!({"preview": {"profile": "full-stack", "port": 3000},
                          "lifecycle": {}});
        let closing_spec = json!({"preview": {"profile": "full-stack", "port": 3000},
                                  "lifecycle": {"cancel_requested": true}});
        let events = |started, ready| PreviewRunEvents { started, ready };
        let stage = |status, spec: &Value, ev| project_preview_run(status, spec, None, ev).stage;
        assert_eq!(stage("queued", &spec, events(false, false)), Some("queued"));
        assert_eq!(
            stage("running", &spec, events(false, false)),
            Some("starting")
        );
        assert_eq!(
            stage("running", &spec, events(true, false)),
            Some("building")
        );
        assert_eq!(stage("running", &spec, events(true, true)), Some("serving"));
        assert_eq!(
            stage("running", &closing_spec, events(true, true)),
            Some("closing")
        );
        assert_eq!(stage("succeeded", &spec, events(true, true)), None);

        let projected = project_preview_run("running", &spec, None, events(true, true));
        assert_eq!(projected.profile.as_deref(), Some("full-stack"));
        assert_eq!(projected.port, Some(3000));
        assert!(projected.ending.is_none());

        let ending = |status, outcome: Value| {
            project_preview_run(status, &spec, Some(&outcome), events(true, true))
                .ending
                .expect("ended")
        };
        assert_eq!(
            ending("succeeded", json!({"status": "succeeded"})).kind,
            "ended"
        );

        let closed = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "owner_cancel", "detail": {"close": "failed"}}),
        );
        assert_eq!(
            (closed.kind, closed.close.as_deref()),
            ("closed", Some("failed"))
        );
        let stopped = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "idle_stop"}),
        );
        assert_eq!(
            (stopped.kind, stopped.reason.as_deref()),
            ("stopped", Some("idle_stop"))
        );
        let unstarted_stop = ending("cancelled", json!({"status": "cancelled"}));
        assert_eq!(
            (unstarted_stop.kind, unstarted_stop.reason),
            ("stopped", None)
        );
        assert_eq!(unstarted_stop.cause, None, "the module had started");
        let before_start = project_preview_run(
            "cancelled",
            &spec,
            Some(&json!({"status": "cancelled"})),
            events(false, false),
        )
        .ending
        .expect("ended");
        assert_eq!(
            (before_start.kind, before_start.cause),
            ("stopped", Some("unstarted"))
        );
        let lost_closing = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "owner_cancel", "detail": {"close": "lost"}}),
        );
        assert_eq!(
            (lost_closing.kind, lost_closing.close.as_deref()),
            ("closed", Some("lost"))
        );

        let crashed = ending(
            "failed",
            json!({"status": "failed", "result": {"status": "app_exited", "app_exit_code": 7,
                   "app_signal": null, "stderr_tail": "boom", "message": "exited"}}),
        );
        assert_eq!(
            (crashed.kind, crashed.cause),
            ("failed", Some("app_exited"))
        );
        assert_eq!((crashed.exit_code, crashed.signal), (Some(7), None));
        let module = ending(
            "failed",
            json!({"status": "failed", "result": {"message": "profile has no command"}}),
        );
        assert_eq!(module.cause, Some("module"));
        assert_eq!(module.message.as_deref(), Some("profile has no command"));
        let timed_out = ending("failed", json!({"status": "timed_out", "detail": {}}));
        assert_eq!(timed_out.cause, Some("timed_out"));
        let too_much = ending("failed", json!({"status": "limit_exceeded", "detail": {}}));
        assert_eq!(too_much.cause, Some("limit_exceeded"));
        // A queued preview cancelled by a VM or host loss is a loss, not a stop.
        let lost_queued = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "vm_lost"}),
        );
        assert_eq!(
            (lost_queued.kind, lost_queued.cause),
            ("failed", Some("vm_lost"))
        );
        let host_gone = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "host_force_terminated"}),
        );
        assert_eq!(
            (host_gone.kind, host_gone.cause),
            ("failed", Some("vm_lost"))
        );
        assert_eq!(host_gone.reason.as_deref(), Some("vm_lost"));
        let vm_never_started = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "vm_start_failed"}),
        );
        assert_eq!(vm_never_started.kind, "did_not_start");
        let start_ended = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "start_ended"}),
        );
        assert_eq!(start_ended.kind, "did_not_start");
        assert_eq!(start_ended.reason.as_deref(), Some("start_ended"));
        assert_eq!(
            start_ended.message.as_deref(),
            Some("the workspace Start ended before the preview ran")
        );
        let transferred = ending(
            "cancelled",
            json!({"status": "cancelled", "reason": "ownership_transferred"}),
        );
        assert_eq!(
            (transferred.kind, transferred.reason.as_deref()),
            ("stopped", Some("transfer"))
        );
        let interrupted = ending(
            "failed",
            json!({"status": "interrupted", "detail": {"reason": "supervision_lost"}}),
        );
        assert_eq!(interrupted.cause, Some("interrupted"));
        let lost = ending("failed", json!({"status": "failed", "reason": "vm_lost"}));
        assert_eq!(
            (lost.cause, lost.reason.as_deref()),
            (Some("vm_lost"), Some("vm_lost"))
        );
        let lost_stopping = ending("failed", json!({"status": "failed", "reason": "idle_stop"}));
        assert_eq!(lost_stopping.cause, Some("vm_lost"));
        assert_eq!(lost_stopping.reason.as_deref(), Some("idle_stop"));
        let dependency = ending(
            "failed",
            json!({"status": "failed", "reason": "dependency_failed"}),
        );
        assert_eq!(
            dependency.cause,
            Some("module"),
            "an unknown reason is not a lost VM"
        );
        assert_eq!(dependency.message.as_deref(), Some("dependency failed"));
        let admission = ending(
            "failed",
            json!({"status": "failed", "reason": "dispatch_admission_failed",
                   "detail": "Conflict: a preview is already running"}),
        );
        assert_eq!(admission.kind, "did_not_start");
        assert_eq!(
            admission.message.as_deref(),
            Some("a preview is already running")
        );
    }

    #[test]
    fn preview_tasks_are_named_by_their_profile() {
        assert_eq!(
            preview_task_name(Some("full-stack")),
            "Preview · full-stack"
        );
        assert_eq!(preview_task_name(None), "Preview · default profile");
    }
}
