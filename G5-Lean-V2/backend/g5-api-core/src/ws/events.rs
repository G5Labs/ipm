//! WebSocket event type definitions for the G5 API real-time event hub.
//!
//! This module defines all server-pushed WebSocket event types that flow from
//! the server to connected browser clients over the multiplexed WebSocket
//! connection (`WS /api/v1/ws?token=<jwt>`).
//!
//! The [`WsEvent`] enum is a serde-tagged union of all server-pushed event
//! types from API_AND_STORAGE.md §4.2. Supporting DTO structs are used as
//! fields within event variants.
//!
//! # Serialization
//!
//! All types serialize to camelCase JSON matching the TypeScript API contract.
//! The `WsEvent` enum uses `#[serde(tag = "type")]` for internally-tagged
//! serialization, producing JSON objects with a `"type"` discriminator field.
//!
//! # Example
//!
//! ```json
//! { "type": "task.status", "taskId": "uuid-here", "status": "running" }
//! { "type": "chat.chunk", "sessionId": "uuid-here", "messageId": "uuid-here", "delta": "Hello" }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Receive-pack hint fields, flattened into `workspace.branch_updated`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchUpdateHint {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub old: String,
    pub new: String,
    pub actor: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// WsEvent — Top-level WebSocket event enum
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level WebSocket event enum. Covers all server-pushed event types
/// from API_AND_STORAGE.md §4.2. Uses serde internal tagging on the `"type"`
/// field to discriminate event kinds.
///
/// Each variant maps to a specific dot-notation event type string (e.g.,
/// `"task.status"`, `"chat.chunk"`). The internally-tagged format produces
/// flat JSON objects with the `"type"` field at the top level, matching the
/// TypeScript client's `event.type` switch pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    // ── Task events ──────────────────────────────────────────────────────
    /// Task status change within queued/running/waiting/succeeded/failed/cancelled.
    #[serde(rename = "task.status")]
    TaskStatus {
        #[serde(rename = "taskId")]
        task_id: Uuid,
        status: String,
    },

    /// New timeline entry streamed from the agent.
    #[serde(rename = "task.timeline")]
    TaskTimeline {
        #[serde(rename = "taskId")]
        task_id: Uuid,
        entry: TimelineEntryPayload,
    },

    // ── Workspace events ─────────────────────────────────────────────────
    /// Workspace sync status change (behind/ahead counts).
    #[serde(rename = "workspace.sync")]
    WorkspaceSync {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "behindBase")]
        behind_base: i32,
        #[serde(rename = "aheadOfBase")]
        ahead_of_base: i32,
    },

    /// Workspace branch update hint; consumers reconcile heads from Git.
    #[serde(rename = "workspace.branch_updated")]
    WorkspaceBranchUpdated {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "repoId")]
        repo_id: Uuid,
        branch: String,
        #[serde(rename = "headCommit")]
        head_commit: String,
        #[serde(flatten)]
        hint: Option<BranchUpdateHint>,
    },

    /// Successful explicit Save checkpoint; ordinary Git events do not emit it.
    #[serde(rename = "workspace.source_control_saved")]
    WorkspaceSourceControlSaved {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        generation: i64,
    },

    /// PR evidence/provider state changed after a pushed commit.
    #[serde(rename = "workspace.pull_request_updated")]
    WorkspacePullRequestUpdated {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "pullRequestId")]
        pull_request_id: Uuid,
    },

    /// A complete verified durable receipt replaced the workspace's Git-only
    /// read source. Clients invalidate any cached receipt-backed tree.
    #[serde(rename = "workspace.flush_completed")]
    WorkspaceFlushCompleted {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "flushId")]
        flush_id: Uuid,
    },

    /// A watched path in the live workspace changed inside the VM.
    #[serde(rename = "workspace.file_changed")]
    WorkspaceFileChanged {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "repoId")]
        repo_id: Uuid,
        path: String,
        #[serde(rename = "rescanRequired")]
        rescan_required: bool,
    },

    /// Durable write authority moved to a different organization member.
    /// Every subscribed browser must discard live-owner payloads immediately.
    #[serde(rename = "workspace.ownership_transferred")]
    WorkspaceOwnershipTransferred {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "fromUserId")]
        from_user_id: Uuid,
        #[serde(rename = "toUserId")]
        to_user_id: Uuid,
        #[serde(rename = "transferId")]
        transfer_id: Uuid,
    },

    /// Workspace mutation-policy (read-only) changed via the transition
    /// endpoint. Emitted only after the DB transaction commits (freeze→read_only
    /// commit, or a revert / RO→RW flip), per the branch-updated event
    /// discipline. Browser clients update their read-only affordances (banner,
    /// disabled controls) and reconnect live clients to recompute edit
    /// capability. Design: `read-only-workspace-transition-design.md`,
    /// `software_architecture/g5-api/services/workspace_service` [I5].
    #[serde(rename = "workspace.policy_changed")]
    WorkspacePolicyChanged {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        /// True when the workspace is now read-only (`mutation_policy =
        /// 'read_only'`).
        #[serde(rename = "readOnly")]
        read_only: bool,
        /// The transient transition marker after the change: `"freezing"` while
        /// a read-write → read-only drain is still in progress, otherwise
        /// `null` in every steady state.
        #[serde(skip_serializing_if = "Option::is_none")]
        transition: Option<String>,
    },

    /// Remote-tip advisory SHA for a remote-tracking branch changed.
    /// Emitted by the advisory pinger on dirty branches (per Phase 3.C
    /// of `designs/REMOTE_REPO_TRACKING.md`). Carries no head movement;
    /// the UI uses it to show a "remote has changed" indicator while
    /// the workspace is dirty.
    #[serde(rename = "workspace.remote_tip_advisory_updated")]
    RemoteTipAdvisoryUpdated {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "repoId")]
        repo_id: Uuid,
        #[serde(rename = "remoteBranch")]
        remote_branch: String,
        #[serde(rename = "remoteTipAdvisorySha")]
        remote_tip_advisory_sha: String,
    },

    /// Spend limit hit.
    #[serde(rename = "workspace.spend_limit_reached")]
    SpendLimitReached {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "periodSpendUsd")]
        period_spend_usd: f64,
        #[serde(rename = "limitSpendUsd")]
        limit_spend_usd: f64,
        reason: String,
    },

    /// Spend limit cleared.
    #[serde(rename = "workspace.spend_limit_cleared")]
    SpendLimitCleared {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        reason: String,
    },

    /// Settings changed by another user.
    #[serde(rename = "workspace.settings_updated")]
    SettingsUpdated {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "updatedBy")]
        updated_by: Uuid,
        changes: Vec<String>,
    },

    // ── Chat events ──────────────────────────────────────────────────────
    /// Transient native-provider allowance observation; never replayed.
    #[serde(rename = "chat.subscription_usage")]
    ChatSubscriptionUsage {
        #[serde(rename = "sessionId")]
        session_id: Uuid,
        #[serde(rename = "authMode")]
        auth_mode: g5_contracts::types::AgentAuthModeV1,
        #[serde(rename = "accountId")]
        account_id: Option<Uuid>,
        usage: g5_internal_api::subscription_usage::SubscriptionUsage,
    },
    /// Streaming LLM response delta.
    #[serde(rename = "chat.chunk")]
    ChatChunk {
        #[serde(rename = "sessionId")]
        session_id: Uuid,
        #[serde(rename = "messageId")]
        message_id: Uuid,
        delta: String,
        #[serde(rename = "messageIndex", skip_serializing_if = "Option::is_none")]
        message_index: Option<i32>,
        #[serde(rename = "createdAt", skip_serializing_if = "Option::is_none")]
        created_at: Option<DateTime<Utc>>,
    },

    /// A live timeline row of a conversation's running attempt, published
    /// from the guest journal before it is stored (`g5.chat.live-delivery`).
    /// The entry's id equals the stored row's id, so the browser holds one
    /// fact for both.
    #[serde(rename = "chat.timeline")]
    ChatTimeline {
        #[serde(rename = "sessionId")]
        session_id: Uuid,
        #[serde(rename = "taskId")]
        task_id: Uuid,
        entry: TimelineEntryPayload,
    },

    /// User message content was replaced after initial persistence.
    #[serde(rename = "chat.message_updated")]
    ChatMessageUpdated {
        #[serde(rename = "sessionId")]
        session_id: Uuid,
        #[serde(rename = "messageId")]
        message_id: Uuid,
    },

    /// LLM response complete with usage stats and citations.
    #[serde(rename = "chat.done")]
    ChatDone {
        #[serde(rename = "sessionId")]
        session_id: Uuid,
        #[serde(rename = "messageId")]
        message_id: Uuid,
        usage: ChatUsagePayload,
        citations: Vec<CitationPayload>,
    },

    /// A chat's turn activity or the recipient's inspection of it changed
    /// (chat g5.chat.activity.refresh). Content-free: clients refetch the
    /// authoritative session list. `sessionId` is absent when a workspace-wide
    /// transition changed several chats at once.
    #[serde(rename = "chat.activity")]
    ChatActivity {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none", default)]
        session_id: Option<Uuid>,
    },

    /// LLM error.
    #[serde(rename = "chat.error")]
    ChatError {
        #[serde(rename = "sessionId")]
        session_id: Uuid,
        #[serde(rename = "messageId")]
        message_id: Uuid,
        error: String,
        message: String,
    },

    // ── Comment events ───────────────────────────────────────────────────
    /// New comment created.
    #[serde(rename = "comment.new")]
    CommentNew {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "commentId")]
        comment_id: Uuid,
        scope: String,
        #[serde(rename = "originBranch")]
        origin_branch: String,
    },

    /// New reply added to a comment.
    #[serde(rename = "comment.reply")]
    CommentReply {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "commentId")]
        comment_id: Uuid,
        #[serde(rename = "replyId")]
        reply_id: Uuid,
        #[serde(rename = "authorName")]
        author_name: String,
    },

    /// A vote on a referenced question was cast, changed or withdrawn
    /// (`g5.documents.question.votes`); cards re-read the set's tally.
    #[serde(rename = "question.vote")]
    QuestionVote {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        path: String,
        #[serde(rename = "questionId")]
        question_id: String,
    },

    /// A comment was resolved, reopened or edited.
    #[serde(rename = "comment.updated")]
    CommentUpdated {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "commentId")]
        comment_id: Uuid,
        resolved: bool,
    },

    /// A comment thread was deleted with its replies.
    #[serde(rename = "comment.deleted")]
    CommentDeleted {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "commentId")]
        comment_id: Uuid,
    },

    // ── Notification events ──────────────────────────────────────────────
    /// Aggregated operational notification.
    #[serde(rename = "notification")]
    Notification {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        notification: NotificationPayload,
    },

    /// A document's content was updated (e.g. by a worker completing a view job).
    #[serde(rename = "doc.content_changed")]
    DocContentChanged {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "docId")]
        doc_id: String,
    },

    /// A document was created.
    #[serde(rename = "doc.created")]
    DocCreated {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "docId")]
        doc_id: Uuid,
    },

    /// A document was deleted.
    #[serde(rename = "doc.deleted")]
    DocDeleted {
        #[serde(rename = "workspaceId")]
        workspace_id: Uuid,
        #[serde(rename = "docId")]
        doc_id: String,
    },
}

impl WsEvent {
    /// Serialize this event to a JSON string suitable for sending
    /// over a WebSocket text frame.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Returns the event type string (e.g., `"task.status"`, `"chat.chunk"`).
    /// Useful for logging and metrics without full serialization.
    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            WsEvent::TaskStatus { .. } => "task.status",
            WsEvent::TaskTimeline { .. } => "task.timeline",
            WsEvent::WorkspaceSync { .. } => "workspace.sync",
            WsEvent::WorkspaceBranchUpdated { .. } => "workspace.branch_updated",
            WsEvent::WorkspaceSourceControlSaved { .. } => "workspace.source_control_saved",
            WsEvent::WorkspacePullRequestUpdated { .. } => "workspace.pull_request_updated",
            WsEvent::WorkspaceFlushCompleted { .. } => "workspace.flush_completed",
            WsEvent::WorkspaceFileChanged { .. } => "workspace.file_changed",
            WsEvent::WorkspaceOwnershipTransferred { .. } => "workspace.ownership_transferred",
            WsEvent::WorkspacePolicyChanged { .. } => "workspace.policy_changed",
            WsEvent::RemoteTipAdvisoryUpdated { .. } => "workspace.remote_tip_advisory_updated",
            WsEvent::SpendLimitReached { .. } => "workspace.spend_limit_reached",
            WsEvent::SpendLimitCleared { .. } => "workspace.spend_limit_cleared",
            WsEvent::SettingsUpdated { .. } => "workspace.settings_updated",
            WsEvent::ChatChunk { .. } => "chat.chunk",
            WsEvent::ChatSubscriptionUsage { .. } => "chat.subscription_usage",
            WsEvent::ChatTimeline { .. } => "chat.timeline",
            WsEvent::ChatMessageUpdated { .. } => "chat.message_updated",
            WsEvent::ChatDone { .. } => "chat.done",
            WsEvent::ChatError { .. } => "chat.error",
            WsEvent::ChatActivity { .. } => "chat.activity",
            WsEvent::CommentNew { .. } => "comment.new",
            WsEvent::CommentReply { .. } => "comment.reply",
            WsEvent::QuestionVote { .. } => "question.vote",
            WsEvent::CommentUpdated { .. } => "comment.updated",
            WsEvent::CommentDeleted { .. } => "comment.deleted",
            WsEvent::Notification { .. } => "notification",
            WsEvent::DocContentChanged { .. } => "doc.content_changed",
            WsEvent::DocCreated { .. } => "doc.created",
            WsEvent::DocDeleted { .. } => "doc.deleted",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Supporting DTO Structs
// ─────────────────────────────────────────────────────────────────────────────

/// Timeline entry payload matching `TimelineEntryDTO` from §3.4.2.
///
/// Contains the full timeline entry data streamed from the agent during
/// task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntryPayload {
    pub id: Uuid,
    pub entry_type: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Chat usage statistics returned with `chat.done` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatUsagePayload {
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Citation reference to ontology nodes or file ranges.
///
/// All fields are optional because a citation may reference a node by ID,
/// a file range by path and lines, or both.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_snippet: Option<String>,
}

/// Minimal user summary for presence events.
///
/// Compact user identity embedded in event payloads.
/// user representation is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummaryPayload {
    pub id: Uuid,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

/// Full notification object for the `notification` event type.
///
/// Matches the notification payload from API_AND_STORAGE.md §4.2.
/// Contains severity, category, message, and optional action buttons.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPayload {
    pub id: Uuid,
    /// Severity level: `"info"`, `"warning"`, or `"error"`.
    pub severity: String,
    /// Category: `"task_blocked"`, `"build_failure"`, `"test_failure"`,
    /// `"spend_warning"`, `"spend_limit"`, `"agent_error"`,
    /// `"worker_terminated"`, `"merge_conflict"`, `"mention"`.
    pub category: String,
    pub title: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    /// Comment that triggered this notification (for mention notifications).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<Uuid>,
    /// Reply that triggered this notification (for reply mention notifications).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_id: Option<Uuid>,
    pub actions: Vec<NotificationAction>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Action button within a notification.
///
/// Represents a clickable action in the notification UI. The `action` field
/// determines the behavior: `"navigate"` opens a URL, `"open_chat"` opens
/// a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub label: String,
    /// Action type: `"navigate"` or `"open_chat"`.
    pub action: String,
    pub target: String,
}

/// Readiness for a worker-local provided agent provider.
/// Option shown for an approval/permission request.
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_task_status_serialization() {
        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "running".to_string(),
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "task.status");
        assert_eq!(parsed["taskId"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(parsed["status"], "running");
    }

    #[test]
    fn test_task_status_event_type() {
        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "succeeded".to_string(),
        };
        assert_eq!(event.event_type(), "task.status");
    }

    #[test]
    fn test_chat_chunk_serialization() {
        let session_id = Uuid::nil();
        let message_id = Uuid::nil();
        let event = WsEvent::ChatChunk {
            session_id,
            message_id,
            delta: "Hello".to_string(),
            message_index: None,
            created_at: None,
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "chat.chunk");
        assert_eq!(parsed["sessionId"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(parsed["messageId"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(parsed["delta"], "Hello");
    }

    #[test]
    fn test_chat_message_updated_serialization() {
        let event = WsEvent::ChatMessageUpdated {
            session_id: Uuid::nil(),
            message_id: Uuid::nil(),
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "chat.message_updated");
        assert_eq!(parsed["sessionId"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(parsed["messageId"], "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_chat_done_serialization() {
        let event = WsEvent::ChatDone {
            session_id: Uuid::nil(),
            message_id: Uuid::nil(),
            usage: ChatUsagePayload {
                input_tokens: 100,
                output_tokens: 50,
            },
            citations: vec![CitationPayload {
                file_path: Some("src/main.rs".to_string()),
                start_line: Some(10),
                end_line: Some(20),
                content_snippet: Some("fn main() {}".to_string()),
            }],
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "chat.done");
        assert_eq!(parsed["usage"]["inputTokens"], 100);
        assert_eq!(parsed["usage"]["outputTokens"], 50);
        assert!(parsed["citations"][0].get("nodeId").is_none());
        assert_eq!(parsed["citations"][0]["filePath"], "src/main.rs");
        assert_eq!(parsed["citations"][0]["startLine"], 10);
        assert_eq!(parsed["citations"][0]["endLine"], 20);
        assert_eq!(parsed["citations"][0]["contentSnippet"], "fn main() {}");
    }

    #[test]
    fn test_citation_optional_fields_omitted() {
        let citation = CitationPayload {
            file_path: Some("test.rs".to_string()),
            start_line: None,
            end_line: None,
            content_snippet: None,
        };
        let json = serde_json::to_string(&citation).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("nodeId").is_none());
        assert_eq!(parsed["filePath"], "test.rs");
        assert!(parsed.get("startLine").is_none());
        assert!(parsed.get("endLine").is_none());
        assert!(parsed.get("contentSnippet").is_none());
    }

    #[test]
    fn test_notification_serialization() {
        let event = WsEvent::Notification {
            workspace_id: Uuid::nil(),
            notification: NotificationPayload {
                id: Uuid::nil(),
                severity: "warning".to_string(),
                category: "spend_warning".to_string(),
                title: "Spend Warning".to_string(),
                message: "You are approaching your spend limit.".to_string(),
                task_id: None,
                comment_id: None,
                reply_id: None,
                actions: vec![NotificationAction {
                    label: "View Spend".to_string(),
                    action: "navigate".to_string(),
                    target: "/settings/spend".to_string(),
                }],
                created_at: DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
                expires_at: None,
            },
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "notification");
        assert_eq!(parsed["notification"]["severity"], "warning");
        assert_eq!(parsed["notification"]["category"], "spend_warning");
        assert!(parsed["notification"].get("taskId").is_none());
        assert_eq!(parsed["notification"]["actions"][0]["label"], "View Spend");
        assert_eq!(parsed["notification"]["actions"][0]["action"], "navigate");
        assert!(parsed["notification"].get("expiresAt").is_none());
    }

    #[test]
    fn test_workspace_sync_serialization() {
        let event = WsEvent::WorkspaceSync {
            workspace_id: Uuid::nil(),
            behind_base: 3,
            ahead_of_base: 1,
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "workspace.sync");
        assert_eq!(
            parsed["workspaceId"],
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(parsed["behindBase"], 3);
        assert_eq!(parsed["aheadOfBase"], 1);
    }

    #[test]
    fn test_workspace_branch_updated_serialization() {
        let event = WsEvent::WorkspaceBranchUpdated {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            branch: "main".to_string(),
            head_commit: "abc123def456".to_string(),
            hint: None,
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "workspace.branch_updated");
        assert_eq!(
            parsed["workspaceId"],
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(
            parsed["workspaceId"],
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(parsed["repoId"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(parsed["branch"], "main");
        assert_eq!(parsed["headCommit"], "abc123def456");
    }

    #[test]
    fn test_workspace_ownership_transferred_serialization() {
        let event = WsEvent::WorkspaceOwnershipTransferred {
            workspace_id: Uuid::nil(),
            from_user_id: Uuid::from_u128(1),
            to_user_id: Uuid::from_u128(2),
            transfer_id: Uuid::from_u128(3),
        };
        let parsed: serde_json::Value = serde_json::from_str(&event.to_json().unwrap()).unwrap();
        assert_eq!(parsed["type"], "workspace.ownership_transferred");
        assert_eq!(parsed["workspaceId"], Uuid::nil().to_string());
        assert_eq!(parsed["fromUserId"], Uuid::from_u128(1).to_string());
        assert_eq!(parsed["toUserId"], Uuid::from_u128(2).to_string());
        assert_eq!(parsed["transferId"], Uuid::from_u128(3).to_string());
    }

    #[test]
    fn test_spend_limit_reached_serialization() {
        let event = WsEvent::SpendLimitReached {
            workspace_id: Uuid::nil(),
            period_spend_usd: 95.50,
            limit_spend_usd: 100.00,
            reason: "Monthly spend limit exceeded".to_string(),
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "workspace.spend_limit_reached");
        assert_eq!(parsed["periodSpendUsd"], 95.5);
        assert_eq!(parsed["limitSpendUsd"], 100.0);
        assert_eq!(parsed["reason"], "Monthly spend limit exceeded");
    }

    #[test]
    fn test_user_summary_avatar_url_omitted_when_none() {
        let user = UserSummaryPayload {
            id: Uuid::nil(),
            display_name: "Bob".to_string(),
            avatar_url: None,
        };
        let json = serde_json::to_string(&user).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["displayName"], "Bob");
        assert!(parsed.get("avatarUrl").is_none());
    }

    #[test]
    fn test_comment_new_has_no_retired_node_identity() {
        let event = WsEvent::CommentNew {
            workspace_id: Uuid::nil(),
            comment_id: Uuid::nil(),
            scope: "line".to_string(),
            origin_branch: "main".to_string(),
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "comment.new");
        assert_eq!(parsed["scope"], "line");
        assert!(parsed.get("nodeId").is_none());
        assert_eq!(parsed["originBranch"], "main");
    }

    #[test]
    fn test_task_timeline_serialization() {
        let event = WsEvent::TaskTimeline {
            task_id: Uuid::nil(),
            entry: TimelineEntryPayload {
                id: Uuid::nil(),
                entry_type: "thought".to_string(),
                content: "Analyzing the code...".to_string(),
                metadata: serde_json::json!({"step": 1}),
                created_at: DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            },
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "task.timeline");
        assert_eq!(parsed["entry"]["entryType"], "thought");
        assert_eq!(parsed["entry"]["content"], "Analyzing the code...");
        assert_eq!(parsed["entry"]["metadata"]["step"], 1);
    }

    #[test]
    fn test_all_event_types_covered() {
        // Verify event_type() returns distinct strings for the core public variants.
        let event_types = vec![
            WsEvent::TaskStatus {
                task_id: Uuid::nil(),
                status: "queued".to_string(),
            }
            .event_type(),
            WsEvent::TaskTimeline {
                task_id: Uuid::nil(),
                entry: TimelineEntryPayload {
                    id: Uuid::nil(),
                    entry_type: "thought".to_string(),
                    content: "".to_string(),
                    metadata: serde_json::Value::Null,
                    created_at: Utc::now(),
                },
            }
            .event_type(),
            WsEvent::WorkspaceSync {
                workspace_id: Uuid::nil(),
                behind_base: 0,
                ahead_of_base: 0,
            }
            .event_type(),
            WsEvent::WorkspaceBranchUpdated {
                workspace_id: Uuid::nil(),
                repo_id: Uuid::nil(),
                branch: "".to_string(),
                head_commit: "".to_string(),
                hint: None,
            }
            .event_type(),
            WsEvent::RemoteTipAdvisoryUpdated {
                workspace_id: Uuid::nil(),
                repo_id: Uuid::nil(),
                remote_branch: "".to_string(),
                remote_tip_advisory_sha: "".to_string(),
            }
            .event_type(),
            WsEvent::SpendLimitReached {
                workspace_id: Uuid::nil(),
                period_spend_usd: 0.0,
                limit_spend_usd: 0.0,
                reason: "".to_string(),
            }
            .event_type(),
            WsEvent::SpendLimitCleared {
                workspace_id: Uuid::nil(),
                reason: "".to_string(),
            }
            .event_type(),
            WsEvent::SettingsUpdated {
                workspace_id: Uuid::nil(),
                updated_by: Uuid::nil(),
                changes: vec![],
            }
            .event_type(),
            WsEvent::ChatChunk {
                session_id: Uuid::nil(),
                message_id: Uuid::nil(),
                delta: "".to_string(),
                message_index: None,
                created_at: None,
            }
            .event_type(),
            WsEvent::ChatMessageUpdated {
                session_id: Uuid::nil(),
                message_id: Uuid::nil(),
            }
            .event_type(),
            WsEvent::ChatDone {
                session_id: Uuid::nil(),
                message_id: Uuid::nil(),
                usage: ChatUsagePayload {
                    input_tokens: 0,
                    output_tokens: 0,
                },
                citations: vec![],
            }
            .event_type(),
            WsEvent::ChatError {
                session_id: Uuid::nil(),
                message_id: Uuid::nil(),
                error: "".to_string(),
                message: "".to_string(),
            }
            .event_type(),
            WsEvent::ChatActivity {
                workspace_id: Uuid::nil(),
                session_id: None,
            }
            .event_type(),
            WsEvent::CommentNew {
                workspace_id: Uuid::nil(),
                comment_id: Uuid::nil(),
                scope: "".to_string(),
                origin_branch: "".to_string(),
            }
            .event_type(),
            WsEvent::CommentReply {
                workspace_id: Uuid::nil(),
                comment_id: Uuid::nil(),
                reply_id: Uuid::nil(),
                author_name: "".to_string(),
            }
            .event_type(),
            WsEvent::QuestionVote {
                workspace_id: Uuid::nil(),
                path: "".to_string(),
                question_id: "".to_string(),
            }
            .event_type(),
            WsEvent::CommentUpdated {
                workspace_id: Uuid::nil(),
                comment_id: Uuid::nil(),
                resolved: true,
            }
            .event_type(),
            WsEvent::CommentDeleted {
                workspace_id: Uuid::nil(),
                comment_id: Uuid::nil(),
            }
            .event_type(),
            WsEvent::Notification {
                workspace_id: Uuid::nil(),
                notification: NotificationPayload {
                    id: Uuid::nil(),
                    severity: "".to_string(),
                    category: "".to_string(),
                    title: "".to_string(),
                    message: "".to_string(),
                    task_id: None,
                    comment_id: None,
                    reply_id: None,
                    actions: vec![],
                    created_at: Utc::now(),
                    expires_at: None,
                },
            }
            .event_type(),
        ];

        assert_eq!(event_types.len(), 19);

        // Verify all are unique
        let mut unique: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for et in &event_types {
            assert!(unique.insert(et), "Duplicate event type: {}", et);
        }
        assert_eq!(unique.len(), 19);
    }

    #[test]
    fn test_round_trip_deserialization() {
        // Verify that to_json() output can be deserialized back to WsEvent
        let event = WsEvent::TaskStatus {
            task_id: Uuid::nil(),
            status: "running".to_string(),
        };
        let json = event.to_json().unwrap();
        let deserialized: WsEvent = serde_json::from_str(&json).unwrap();

        // Verify the deserialized event matches
        match deserialized {
            WsEvent::TaskStatus { task_id, status } => {
                assert_eq!(task_id, Uuid::nil());
                assert_eq!(status, "running");
            }
            _ => panic!("Expected TaskStatus variant"),
        }
    }

    #[test]
    fn test_chat_error_serialization() {
        let event = WsEvent::ChatError {
            session_id: Uuid::nil(),
            message_id: Uuid::nil(),
            error: "context_length_exceeded".to_string(),
            message: "The conversation is too long.".to_string(),
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "chat.error");
        assert_eq!(parsed["error"], "context_length_exceeded");
        assert_eq!(parsed["message"], "The conversation is too long.");
    }

    #[test]
    fn test_settings_updated_serialization() {
        let event = WsEvent::SettingsUpdated {
            workspace_id: Uuid::nil(),
            updated_by: Uuid::nil(),
            changes: vec![
                "max_concurrent_tasks".to_string(),
                "max_spend_usd".to_string(),
            ],
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "workspace.settings_updated");
        assert_eq!(parsed["updatedBy"], "00000000-0000-0000-0000-000000000000");
        assert_eq!(parsed["changes"][0], "max_concurrent_tasks");
        assert_eq!(parsed["changes"][1], "max_spend_usd");
    }

    #[test]
    fn test_spend_limit_cleared_serialization() {
        let event = WsEvent::SpendLimitCleared {
            workspace_id: Uuid::nil(),
            reason: "Period reset".to_string(),
        };
        let json = event.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["type"], "workspace.spend_limit_cleared");
        assert_eq!(parsed["reason"], "Period reset");
    }
}
