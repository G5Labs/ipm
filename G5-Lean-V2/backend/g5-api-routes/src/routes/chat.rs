//! Chat session route handlers under two path prefixes.
//!
//! This module implements the HTTP route handlers for the Chat Sessions API
//! (API_AND_STORAGE.md §3.6) and the session-related Execution Intervention
//! endpoints (API_AND_STORAGE.md §3.8).
//!
//! Endpoints span 2 path prefixes:
//! - `/api/v1/workspaces/:workspaceId/sessions` — list, create, search
//! - `/api/v1/sessions/:id` — detail, messages, send, stop, resume, fork,
//!   summarize and attachment upload/download
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to `ChatService`, and format the response using `ResponseEnvelope`.
//! No business logic resides in the handlers.

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{
    ensure_session_message_id, ensure_session_turn_id, resolve_chat_session_context,
    resolve_org_id_for_workspace, scoped_db_for_chat_session, scoped_db_for_chat_session_view,
    scoped_db_for_workspace,
};
use crate::errors::{AppError, G5Error};
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::{PaginationMeta, ResponseEnvelope};
use crate::services::chat_activity;
use crate::services::chat_ontology;
use crate::services::chat_service;
use crate::services::task_lifecycle::ChatTurnStop;
use crate::services::workbench_config::{
    follow_view_message, persona_first_message, workbench_action_message,
};
use crate::services::workspace_messages::{
    MessageTarget, PersonaExpansion, WorkbenchIdentity, WorkspaceMessage,
};
use crate::services::workspace_mutation_policy::ensure_workspace_mutable;
use crate::services::AppState;
use chrono::{DateTime, Utc};
use g5_api_foundation::contracts::workbench::{
    WorkbenchActionKind, WorkbenchConfigDTO, WorkbenchPhase, WorkbenchStartContextDTO,
};
use g5_auth::Scope;
use g5_authz::ScopedDb;
use g5_contracts::claude_models::{
    claude_model_provider_from_anthropic_base_url, resolve_claude_chat_model_for_provider,
    validate_claude_provider_route, ClaudeProviderRouteIssue, ANTHROPIC_BASE_URL_ENV,
    CLAUDE_CODE_OAUTH_TOKEN_ENV,
};
use g5_contracts::types::{
    AgentAccountSelectionV1, AgentAuthModeV1, AgentLaunchFieldPreferenceV1,
    AgentLaunchPreferencesV1, AgentLaunchSelectionV1, ChatAgentProvider, PersistedAgentLaunchV1,
    ProviderAccountKind, ProviderAccountScope,
};

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for session-scoped endpoints (`/api/v1/sessions/:id/*`).
#[derive(Debug, Deserialize)]
pub struct SessionPathParams {
    pub id: Uuid,
}

/// Path parameters for workspace-scoped endpoints
/// (`/api/v1/workspaces/:workspaceId/sessions/*`).
#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

/// Path parameters for the stop generation endpoint
/// (`/api/v1/sessions/:id/messages/:mid/stop`).
#[derive(Debug, Deserialize)]
pub struct StopGenerationPathParams {
    pub id: Uuid,
    pub mid: Uuid,
}

/// Path parameters for the attachment content read (routes/chat D30)
/// (`/api/v1/sessions/:id/attachments/:attachmentId/content`).
#[derive(Debug, Deserialize)]
pub struct AttachmentContentPathParams {
    pub id: Uuid,
    #[serde(rename = "attachmentId")]
    pub attachment_id: Uuid,
}

fn claude_provider_route_validation_error(
    issue: ClaudeProviderRouteIssue,
    workspace_id: Uuid,
) -> AppError {
    AppError::from(G5Error::ValidationError {
        details: serde_json::json!({
            "field": "runtimeConfig",
            "code": issue.code(),
            "message": issue.message(),
            "workspaceId": workspace_id,
        }),
    })
}

/// Resolve a Claude chat model against the PROJECTED effective config
/// (routes/chat D32, config_service D12): the launch resolver's own account
/// projection is applied with `account` — the next turn's account preference
/// — so an org whose raw keys form an invalid route pair (OAuth token plus an
/// OpenRouter URL) no longer rejects a model when the selected account fixes
/// the route. `inherit_configured` reproduces the account-less resolution
/// byte-for-byte when no assignment exists.
async fn resolve_claude_chat_model_for_workspace(
    state: &AppState,
    db: &mut g5_authz::ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    requested_model: Option<&str>,
    account: AgentLaunchFieldPreferenceV1<AgentAccountSelectionV1>,
) -> Result<String, AppError> {
    let preferences = AgentLaunchPreferencesV1 {
        provider: AgentLaunchFieldPreferenceV1::Explicit(ChatAgentProvider::ClaudeCode),
        account,
        ..AgentLaunchPreferencesV1::default()
    };
    let effective = state
        .config_service
        .resolve_effective_for_agent_launch(
            db,
            org_id,
            Some(workspace_id),
            Some(user_id),
            &[],
            &preferences,
            &state.provider_account_service,
        )
        .await?;
    let base_url = effective
        .secrets
        .get(ANTHROPIC_BASE_URL_ENV)
        .map(String::as_str)
        .or_else(|| {
            effective
                .settings
                .get(ANTHROPIC_BASE_URL_ENV)
                .and_then(serde_json::Value::as_str)
        });
    validate_claude_provider_route(
        base_url,
        effective
            .secrets
            .get(CLAUDE_CODE_OAUTH_TOKEN_ENV)
            .map(String::as_str),
    )
    .map_err(|issue| claude_provider_route_validation_error(issue, workspace_id))?;
    let provider = claude_model_provider_from_anthropic_base_url(base_url);
    Ok(resolve_claude_chat_model_for_provider(
        requested_model,
        provider,
    ))
}

// ─── Query Parameter Types ───────────────────────────────────────────────────

/// Query parameters for `GET /api/v1/workspaces/:workspaceId/sessions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsQuery {
    /// Optional presentation scope retained for node-oriented history views.
    pub scope_type: Option<String>,
    /// Pagination: page number (default 1).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Pagination: items per page (default 50, max 100).
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    50
}

/// Query parameters for `GET /api/v1/workspaces/:workspaceId/sessions/search`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSessionsQuery {
    /// Full-text search query string.
    pub q: String,
    /// Pagination: page number (default 1).
    #[serde(default = "default_page")]
    pub page: i64,
    /// Pagination: items per page (default 50, max 100).
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

/// Query parameters for `GET /api/v1/sessions/:id/messages`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMessagesQuery {
    /// Cursor: message_index of the last message in the previous page.
    pub cursor: Option<i32>,
    /// Number of messages to return (default 50).
    #[serde(default = "default_message_limit")]
    pub limit: i32,
    /// Latest-window read: return the LAST `limit` rows (ascending) instead of
    /// the first page. Honored only when `cursor` is absent; a request with
    /// both follows the forward cursor (routes/chat I6).
    #[serde(default)]
    pub tail: bool,
}

fn default_message_limit() -> i32 {
    50
}

/// Query parameters for `GET /api/v1/sessions/:id/timeline`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionTimelineQuery {
    /// Cursor: RFC 3339 timestamp of the last entry in the previous page.
    pub cursor: Option<String>,
    /// Number of entries to return (default 50).
    #[serde(default = "default_session_timeline_limit")]
    pub limit: i64,
}

fn default_session_timeline_limit() -> i64 {
    200
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/workspaces/:workspaceId/sessions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequestDTO {
    pub scope_type: Option<String>,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
}

/// Request body for `PATCH /api/v1/sessions/:id`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionRequestDTO {
    pub title: Option<String>,
}

/// Request body for `POST /api/v1/sessions/:id/messages`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequestDTO {
    pub content: String,
    pub context: Option<MessageContextDTO>,
    pub model: Option<String>,
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub agent_provider: Option<ChatAgentProvider>,
    /// The account for this turn (routes/chat D33). Omitted, the chat's
    /// saved next-turn account applies. Once the turn is accepted, the choice
    /// becomes the chat's saved account.
    #[serde(default)]
    pub account: Option<AgentAccountSelectionV1>,
    /// Optional inline file attachments for this turn (routes/chat D19).
    /// Omitted ⇒ prior behavior (backward compatible).
    #[serde(default)]
    pub attachments: Option<Vec<InlineChatAttachmentDTO>>,
    /// Start and send (`g5.personal.start-and-send`): record Start consent and
    /// queue this message bound to it, in one owner request.
    #[serde(default)]
    pub start_compute: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditQueuedInputRequestDTO {
    pub expected_revision: i32,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawQueuedInputRequestDTO {
    pub expected_revision: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInputPathParams {
    pub id: Uuid,
    pub input_id: Uuid,
}

/// One inline file attachment uploaded with a chat send/start request
/// (routes/chat D19). Bytes are persisted into `chat_attachments` and the
/// manifest is persisted on the queued input before the agent task is
/// published, so the workspace runtime materializes the file for that turn.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineChatAttachmentDTO {
    pub file_name: String,
    #[serde(default)]
    pub content_type: Option<String>,
    pub content_base64: String,
}

/// Request body for atomic first-turn chat start.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionRequestDTO {
    pub scope_type: Option<String>,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub content: String,
    pub context: Option<MessageContextDTO>,
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub agent_provider: Option<ChatAgentProvider>,
    #[serde(default)]
    pub agent_launch: Option<AgentLaunchSelectionV1>,
    /// Optional inline file attachments for the first turn (routes/chat D19).
    /// Omitted ⇒ prior behavior (backward compatible).
    #[serde(default)]
    pub attachments: Option<Vec<InlineChatAttachmentDTO>>,
    /// Names the request so a retry returns the chat the first request
    /// started instead of starting another.
    #[serde(default)]
    pub request_key: Option<String>,
    /// Workbench persona start (workbench `g5.workbench.persona`,
    /// `g5.workbench.actions`): the persona (`be` | `se` | `ie`), the ribbon
    /// action whose template becomes the first message, the context the
    /// template serializes, and the configuration revision the browser
    /// resolved them against. `persona` and `configRevision` travel together;
    /// all four join the request identity.
    #[serde(default)]
    pub persona: Option<String>,
    /// Ribbon phase the action was clicked in: with `actionId` it selects
    /// exactly that phase's template (ids repeat across phases), whatever
    /// persona the action is overridden to.
    #[serde(default)]
    pub phase: Option<String>,
    /// The ribbon action's template key (not a record identifier).
    #[serde(default, rename = "actionId")]
    pub workbench_action: Option<String>,
    #[serde(default)]
    pub workbench_context: Option<WorkbenchStartContextDTO>,
    #[serde(default)]
    pub config_revision: Option<String>,
    /// Start and send (`g5.personal.start-and-send`).
    #[serde(default)]
    pub start_compute: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentUploadRequestDTO {
    pub message_id: Uuid,
    pub file_name: String,
    #[serde(default)]
    pub content_type: Option<String>,
    pub content_base64: String,
}

/// Context attached to a message (matching TypeScript `SendMessageRequest`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageContextDTO {
    pub file_ranges: Option<Vec<FileRangeDTO>>,
    pub task_id: Option<String>,
    pub active_file_path: Option<String>,
    pub open_file_paths: Option<Vec<String>>,
    pub project_references: Option<Vec<ProjectReferenceDTO>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReferenceDTO {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub path: Option<String>,
    pub doc_id: Option<String>,
    pub branch_id: Option<String>,
}

/// A file range reference within message context.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRangeDTO {
    pub path: String,
    pub start_line: i32,
    pub end_line: i32,
}

/// Request body for `POST /api/v1/sessions/:id/fork`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkSessionRequestDTO {
    pub from_message_index: i32,
    pub title: Option<String>,
    /// "Follow view" (workbench `g5.workbench.persona`): fork and continue
    /// under this persona. The fork, its persona and its first input commit
    /// together; an empty or queued-only source starts a new session instead.
    #[serde(default)]
    pub persona: Option<String>,
    /// Configuration revision the browser resolved the persona against; when
    /// present and stale the request is refused before anything is created.
    #[serde(default)]
    pub config_revision: Option<String>,
    /// Names the request so a retry returns the first fork.
    #[serde(default)]
    pub request_key: Option<String>,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// Session in the list view (summary).
///
/// UUID fields are serialized as `String` (hyphenated UUID format).
/// DateTime fields are serialized as ISO 8601 strings.
/// `total_cost_usd` is converted from `Decimal` to `f64` for JSON.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItemDTO {
    pub id: String,
    pub workspace_id: String,
    pub scope_type: String,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub agent_provider: ChatAgentProvider,
    pub provider_state: serde_json::Value,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub message_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub forked_from_id: Option<String>,
    pub linked_commits: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub can_send: bool,
    /// Persona the session started with (workbench `g5.workbench.persona`).
    pub persona: Option<String>,
    /// Turn activity and the caller's inspection state (chat
    /// `g5.chat.activity`); omitted from search results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<chat_activity::SessionActivity>,
}

/// Detailed session view (includes `forked_at_index`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetailDTO {
    pub id: String,
    pub workspace_id: String,
    pub scope_type: String,
    pub scope_ref: Option<String>,
    pub model: Option<String>,
    pub agent_provider: ChatAgentProvider,
    pub provider_state: serde_json::Value,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub message_count: i32,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub forked_from_id: Option<String>,
    pub forked_at_index: Option<i32>,
    pub linked_commits: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub can_send: bool,
    /// Persona the session started with (workbench `g5.workbench.persona`);
    /// `null` for sessions started outside the workbench.
    pub persona: Option<String>,
    /// Frozen launch auth mode for this session, sourced from the session's
    /// persisted `PersistedAgentLaunchV1.resolved.auth_mode`
    /// (system_invariants/external_agent_provider_contract D11 — never
    /// re-resolved client-side). `None` when the session has no persisted
    /// launch yet. Additive-optional: absent for old payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<AgentAuthModeV1>,
    /// Legacy optional field. Current session reads omit usage; native
    /// observations reach the browser through the live chat channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_usage: Option<SubscriptionUsageDTO>,
    /// Frozen provider account of the session's latest persisted launch
    /// (routes/chat D31): `{ id, kind, scope }` read from
    /// `PersistedAgentLaunchV1.resolved`, never re-resolved and never the
    /// live assignment. Omitted when the session has no persisted launch or
    /// its latest launch names no account. Additive-optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<SessionAccountDTO>,
    /// The committed next-turn account default (routes/chat D34): the active
    /// provider namespace's `settings.account`, `null` when the namespace has
    /// never carried one. A read of persisted session state, never a
    /// resolution; distinct from `account`, the frozen account of the latest
    /// allocated turn.
    pub next_account: Option<AgentAccountSelectionV1>,
    /// Turn activity and the caller's inspection state (chat
    /// `g5.chat.activity`); present on session reads, omitted from mutation
    /// responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<chat_activity::SessionActivity>,
}

/// Non-secret projection of the frozen account triple
/// (external_agent_provider_contract D11): id, closed kind, and scope only —
/// no name, no hint, nothing secret-derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAccountDTO {
    pub id: String,
    pub kind: ProviderAccountKind,
    pub scope: ProviderAccountScope,
}

/// Display-only subscription-usage projection of a
/// [`crate::models::ProviderQuotaSnapshot`] for the composer usage chip
/// (agent_interaction_ux D8). Mirrors the display-relevant snapshot fields
/// only; org/workspace/provider/auth-mode key components and internal
/// created/updated bookkeeping stay server-side. Never gates dispatch and
/// never carries credential material (provider quota snapshot D13).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionUsageDTO {
    pub five_hour_utilization: Option<f32>,
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    pub seven_day_utilization: Option<f32>,
    pub seven_day_reset_at: Option<DateTime<Utc>>,
    pub representative_claim: Option<String>,
    pub unified_status: Option<String>,
    pub captured_at: DateTime<Utc>,
}

impl From<crate::models::ProviderQuotaSnapshot> for SubscriptionUsageDTO {
    fn from(snapshot: crate::models::ProviderQuotaSnapshot) -> Self {
        SubscriptionUsageDTO {
            five_hour_utilization: snapshot.five_hour_utilization,
            five_hour_reset_at: snapshot.five_hour_reset_at,
            seven_day_utilization: snapshot.seven_day_utilization,
            seven_day_reset_at: snapshot.seven_day_reset_at,
            representative_claim: snapshot.representative_claim,
            unified_status: snapshot.unified_status,
            captured_at: snapshot.captured_at,
        }
    }
}

/// A single chat message.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageDTO {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub artifacts: Option<serde_json::Value>,
    pub citations: serde_json::Value,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub status: String,
    pub message_index: i32,
    pub created_at: String,
    pub is_error_notice: bool,
    /// The execution attempt that produced the message, when it has one
    /// (chat `g5.chat.inspection.acknowledge`).
    pub task_id: Option<String>,
    pub attempt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex_auth_failure: Option<chat_service::CodexAuthFailureDTO>,
}

/// Paginated message list response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageListResponseDTO {
    pub messages: Vec<ChatMessageDTO>,
    pub cursor: Option<i32>,
    pub has_more: bool,
}

/// Response from `POST /sessions/:id/messages`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResponseDTO {
    pub message_id: String,
    pub status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    /// The Start recorded with a Start-and-send request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_compute: Option<crate::routes::start_and_send::StartComputeDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponseDTO {
    pub session: SessionDetailDTO,
    pub message: SendMessageResponseDTO,
    pub assistant_message_id: String,
    pub task_id: String,
}

/// Response from `POST /sessions/:id/fork`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkSessionResponseDTO {
    pub session_id: String,
    pub messages_copied: i32,
}

/// Response from `POST /sessions/:id/messages/:mid/stop`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopGenerationResponseDTO {
    pub stopped: bool,
    /// What the Stop established (`g5.chat.controls`): `signalling`,
    /// `already_finished` or `cancelled` from the runtime, `ended` for a
    /// queued or held turn, `withdrawn`, `already_ended`, or `undelivered`
    /// when the intent committed but the runtime call failed and the
    /// attempt's follower redelivers it.
    pub disposition: &'static str,
}

/// Response from `GET /sessions/:id/live-tail` (`g5.chat.live-tail-recovery`):
/// the running attempt's events the API has fetched from the guest journal
/// but not yet stored, after the persisted watermark.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTailResponseDTO {
    /// `none` (no running attempt), `live` (entries are complete from
    /// `persisted_through + 1`) or `catching_up` (no follower in this
    /// process yet, or its buffer is being rebuilt; retry).
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persisted_through: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_through: Option<u64>,
    pub entries: Vec<chat_service::SessionTimelineEntry>,
}

/// Request for `POST /sessions/:id/background/end`: the attempt whose wait
/// the owner saw. A wait that already ended is never applied to the next
/// attempt of the session.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndBackgroundWaitRequestDTO {
    pub task_id: Uuid,
    pub attempt: i32,
}

/// Response from `POST /sessions/:id/background/end`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndBackgroundWaitResponseDTO {
    pub delivered: bool,
}

/// Response from `GET /sessions/:id/timeline`.
/// Wrapped in `ResponseEnvelope` like all other endpoints so the client's
/// `api.get()` can extract it via `envelope.data`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTimelineResponseDTO {
    pub entries: Vec<chat_service::SessionTimelineEntry>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachmentResponseDTO {
    pub id: String,
    pub session_id: String,
    pub message_id: Option<String>,
    pub file_name: String,
    pub content_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub scan_status: String,
    pub storage_key: String,
    pub provider_mount_path: String,
    pub created_at: String,
}

/// Provider-state JSON namespace for per-session control settings. Codex
/// sessions persist under `codex`; every other provided agent uses the
/// historical `claudeCode` namespace (keeping the Claude path byte-identical).
/// Matches the client's `providerState.<ns>.settings` contract (g5-common types
/// ontology D19; codex_support view §3.5).
pub(super) fn provider_state_namespace(agent_provider: ChatAgentProvider) -> &'static str {
    match agent_provider {
        ChatAgentProvider::Codex => "codex",
        _ => "claudeCode",
    }
}

pub(super) async fn frozen_chat_permission_mode(
    state: &AppState,
    db: &mut ScopedDb,
    session_id: Uuid,
    agent_provider: ChatAgentProvider,
    provider_state: &serde_json::Value,
) -> Result<Option<String>, AppError> {
    if let Some(initial_launch) = state
        .chat_service
        .initial_agent_launch_for_session(db, session_id)
        .await?
    {
        return Ok(Some(
            chat_service::canonical_chat_permission_mode(
                agent_provider,
                &initial_launch.resolved.permission_mode.value,
            )
            .to_string(),
        ));
    }

    let persisted_mode = provider_state
        .get(provider_state_namespace(agent_provider))
        .and_then(|namespace| namespace.get("settings"))
        .and_then(|settings| settings.get("permissionMode"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|mode| chat_service::canonical_chat_permission_mode(agent_provider, mode).to_string());
    Ok(persisted_mode)
}

/// The `account` launch preference of a later turn (routes/chat D33): the
/// sender's explicit choice, else the chat's saved next-turn account, else
/// the account its latest turn ran on (chats started before the saved
/// account existed), else the effective assignment. The composer hydrates
/// its Account field in the same order, so an untouched field and an
/// omitted `account` name the same account.
fn follow_up_account_preference(
    requested: Option<AgentAccountSelectionV1>,
    saved: Option<AgentAccountSelectionV1>,
    previous_launch: Option<&PersistedAgentLaunchV1>,
) -> AgentLaunchFieldPreferenceV1<AgentAccountSelectionV1> {
    requested
        .or(saved)
        .or_else(|| previous_launch.map(|launch| chat_service::next_turn_account(&launch.resolved)))
        .map_or(
            AgentLaunchFieldPreferenceV1::InheritConfigured,
            AgentLaunchFieldPreferenceV1::Explicit,
        )
}

/// The committed next-turn account of the active provider namespace
/// (`provider_state.<ns>.settings.account`, routes/chat D32/D33/D34). `None`
/// when the namespace has never carried one; a value that is not a valid
/// `AgentAccountSelectionV1` is a typed failure, never a silent fallback.
fn persisted_session_account(
    provider_state: &serde_json::Value,
    agent_provider: ChatAgentProvider,
) -> Result<Option<AgentAccountSelectionV1>, AppError> {
    let Some(value) = provider_state
        .get(provider_state_namespace(agent_provider))
        .and_then(|namespace| namespace.get("settings"))
        .and_then(|settings| settings.get("account"))
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    serde_json::from_value::<AgentAccountSelectionV1>(value.clone())
        .map(Some)
        .map_err(|error| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "account",
                    "code": "invalid_persisted_session_account",
                    "message": format!("persisted session account setting is invalid: {error}"),
                }),
            })
        })
}

fn reject_changed_chat_permission_mode(requested: &str, frozen: &str) -> AppError {
    AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({
            "field": "permissionMode",
            "code": "immutable_chat_permission_mode",
            "message": format!(
                "permissionMode is fixed to {frozen} for this chat; received {requested}"
            ),
        }),
    })
}

fn public_session_provider_state(
    provider_state: &serde_json::Value,
    agent_provider: ChatAgentProvider,
) -> serde_json::Value {
    let namespace = provider_state_namespace(agent_provider);
    let Some(settings) = provider_state
        .get(namespace)
        .and_then(|value| value.get("settings"))
        .filter(|value| value.is_object())
    else {
        return serde_json::json!({});
    };

    let mut public_settings = serde_json::Map::new();
    for key in ["permissionMode", "model", "effort"] {
        if let Some(value) = settings.get(key).and_then(serde_json::Value::as_str) {
            public_settings.insert(key.to_string(), serde_json::json!(value));
        }
    }
    // routes/chat D32: the committed next-turn account rides beside the
    // string settings (id + kind only — nothing secret-derived).
    if let Some(account) = settings.get("account").filter(|value| value.is_object()) {
        public_settings.insert("account".to_string(), account.clone());
    }

    if public_settings.is_empty() {
        return serde_json::json!({});
    }

    let mut namespace_obj = serde_json::Map::new();
    namespace_obj.insert(
        namespace.to_string(),
        serde_json::json!({ "settings": serde_json::Value::Object(public_settings) }),
    );
    serde_json::Value::Object(namespace_obj)
}

// ─── From Implementations ────────────────────────────────────────────────────

impl From<chat_service::SessionListItem> for SessionListItemDTO {
    fn from(s: chat_service::SessionListItem) -> Self {
        SessionListItemDTO {
            id: s.id.to_string(),
            workspace_id: s.workspace_id.to_string(),
            scope_type: s.scope_type,
            scope_ref: s.scope_ref,
            model: s.model,
            agent_provider: s.agent_provider,
            provider_state: public_session_provider_state(&s.provider_state, s.agent_provider),
            title: s.title,
            summary: s.summary,
            tags: s.tags,
            message_count: s.message_count,
            total_input_tokens: s.total_input_tokens,
            total_output_tokens: s.total_output_tokens,
            total_cost_usd: s.total_cost_usd,
            forked_from_id: s.forked_from_id.map(|id| id.to_string()),
            linked_commits: s.linked_commits,
            created_at: s.created_at,
            updated_at: s.updated_at,
            can_send: s.can_send,
            persona: s.persona,
            activity: s.activity,
        }
    }
}

impl From<chat_service::SessionDetail> for SessionDetailDTO {
    fn from(s: chat_service::SessionDetail) -> Self {
        SessionDetailDTO {
            id: s.id.to_string(),
            workspace_id: s.workspace_id.to_string(),
            scope_type: s.scope_type,
            scope_ref: s.scope_ref,
            model: s.model,
            agent_provider: s.agent_provider,
            next_account: persisted_session_account(&s.provider_state, s.agent_provider)
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        session_id = %s.id,
                        error = %error,
                        "Session detail hides a malformed persisted next-turn account"
                    );
                    None
                }),
            provider_state: public_session_provider_state(&s.provider_state, s.agent_provider),
            title: s.title,
            summary: s.summary,
            tags: s.tags,
            message_count: s.message_count,
            total_input_tokens: s.total_input_tokens,
            total_output_tokens: s.total_output_tokens,
            total_cost_usd: s.total_cost_usd,
            forked_from_id: s.forked_from_id.map(|id| id.to_string()),
            forked_at_index: s.forked_at_index,
            linked_commits: s.linked_commits,
            created_at: s.created_at,
            updated_at: s.updated_at,
            can_send: s.can_send,
            persona: s.persona,
            activity: s.activity,
            // Populated by `enrich_session_launch` at the read
            // path that has async DB access; the sync conversion leaves them
            // unset (additive-optional).
            auth_mode: None,
            subscription_usage: None,
            account: None,
        }
    }
}

impl From<chat_service::ChatMessageDTO> for ChatMessageDTO {
    fn from(m: chat_service::ChatMessageDTO) -> Self {
        ChatMessageDTO {
            id: m.id.to_string(),
            session_id: m.session_id.to_string(),
            role: m.role,
            content: m.content,
            artifacts: m.artifacts,
            citations: m.citations,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            status: m.status,
            message_index: m.message_index,
            created_at: m.created_at,
            is_error_notice: m.is_error_notice,
            task_id: m.task_id.map(|id| id.to_string()),
            attempt: m.attempt,
            codex_auth_failure: m.codex_auth_failure,
        }
    }
}

impl From<chat_service::MessageListResponse> for MessageListResponseDTO {
    fn from(r: chat_service::MessageListResponse) -> Self {
        MessageListResponseDTO {
            messages: r.messages.into_iter().map(ChatMessageDTO::from).collect(),
            cursor: r.cursor,
            has_more: r.has_more,
        }
    }
}

impl From<chat_service::MessageSummary> for SendMessageResponseDTO {
    fn from(s: chat_service::MessageSummary) -> Self {
        SendMessageResponseDTO {
            message_id: s.assistant_message_id.to_string(),
            status: s.status,
            created_at: s.assistant_created_at.to_rfc3339(),
            input_id: None,
            task_id: None,
            delivery: None,
            start_compute: None,
        }
    }
}

impl From<chat_service::ForkSessionResponse> for ForkSessionResponseDTO {
    fn from(r: chat_service::ForkSessionResponse) -> Self {
        ForkSessionResponseDTO {
            session_id: r.session_id.to_string(),
            messages_copied: r.messages_copied,
        }
    }
}

impl From<chat_service::ChatAttachmentDTO> for ChatAttachmentResponseDTO {
    fn from(attachment: chat_service::ChatAttachmentDTO) -> Self {
        ChatAttachmentResponseDTO {
            id: attachment.id.to_string(),
            session_id: attachment.session_id.to_string(),
            message_id: attachment.message_id.map(|id| id.to_string()),
            file_name: attachment.file_name,
            content_type: attachment.content_type,
            size_bytes: attachment.size_bytes,
            sha256: attachment.sha256,
            scan_status: attachment.scan_status,
            storage_key: attachment.storage_key,
            provider_mount_path: attachment.provider_mount_path,
            created_at: attachment.created_at.to_rfc3339(),
        }
    }
}

/// Map optional wire inline attachments to the service inline-attachment type.
/// Omitted/`null` ⇒ empty (prior behavior).
fn map_inline_attachments(
    attachments: Option<Vec<InlineChatAttachmentDTO>>,
) -> Vec<chat_service::InlineChatAttachment> {
    attachments
        .unwrap_or_default()
        .into_iter()
        .map(|item| chat_service::InlineChatAttachment {
            file_name: item.file_name,
            content_type: item.content_type,
            content_base64: item.content_base64,
        })
        .collect()
}

/// Buffered-body limit for chat send/start. The 10 MiB raw aggregate attachment
/// cap encodes to roughly 13.3 MiB of base64; 16 MiB leaves room for message and
/// context fields while replacing Axum's 2 MiB default (routes/chat D19).
const CHAT_SEND_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

/// Only inert raster formats are served inline. All other stored types are
/// forced to an opaque download regardless of their claimed MIME type (D30).
const SAFE_INLINE_ATTACHMENT_CONTENT_TYPES: [&str; 4] =
    ["image/png", "image/jpeg", "image/gif", "image/webp"];

fn is_safe_inline_attachment_content_type(content_type: &str) -> bool {
    SAFE_INLINE_ATTACHMENT_CONTENT_TYPES.contains(&content_type.to_ascii_lowercase().as_str())
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Workspace-scoped session routes — registered under
/// `/api/v1/workspaces/:workspaceId/sessions`.
///
/// Endpoints:
/// - `GET /` — list sessions (paginated, filterable)
/// - `POST /` — create session
/// - `GET /search` — full-text search sessions
pub fn workspace_sessions_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_sessions), &[Scope::SessionsRead])
        .audited_route("/", post(create_session), &[Scope::SessionsWrite])
        .audited_route("/start", post(start_session), &[Scope::SessionsWrite])
        .audited_route("/search", get(search_sessions), &[Scope::SessionsRead])
        .into_router()
        // `/start` accepts inline base64 file attachments; raise the buffered
        // body limit to fit the bounded aggregate attachment set (D19).
        .layer(DefaultBodyLimit::max(CHAT_SEND_BODY_LIMIT_BYTES))
}

/// Session-scoped routes — registered under `/api/v1/sessions`.
///
/// Endpoints:
/// - `GET /:id` — session detail
/// - `GET /:id/messages` — full message history (cursor-paginated)
/// - `POST /:id/messages` — send message (triggers streaming)
/// - `POST /:id/messages/:mid/stop` — stop generation
/// - `POST /:id/background/end` — end the wait on Claude's background work
/// - `POST /:id/fork` — fork from message index
/// - `GET /:id/task` — newest live task for the session (routes/chat I19)
///
pub fn session_detail_router() -> Router<Arc<AppState>> {
    let router = AuditedRouter::new()
        .audited_route(
            "/:id/codex-auth",
            get(get_codex_auth),
            &[Scope::SessionsRead],
        )
        .audited_route("/:id", get(get_session), &[Scope::SessionsRead])
        .audited_route("/:id", patch(update_session), &[Scope::SessionsWrite])
        .audited_route("/:id/task", get(get_session_task), &[Scope::SessionsRead])
        // A reader's own inspection state, not workspace content
        // (chat g5.chat.inspection.acknowledge).
        .audited_route(
            "/:id/inspection",
            put(record_session_inspection),
            &[Scope::SessionsRead],
        )
        .audited_route("/:id/messages", get(get_messages), &[Scope::SessionsRead])
        .audited_route("/:id/messages", post(send_message), &[Scope::SessionsWrite])
        .audited_route(
            "/:id/inputs",
            get(list_session_inputs),
            &[Scope::SessionsRead],
        )
        .audited_route(
            "/:id/inputs/:inputId",
            patch(edit_queued_input),
            &[Scope::SessionsWrite],
        )
        .audited_route(
            "/:id/inputs/:inputId/withdraw",
            post(withdraw_queued_input),
            &[Scope::SessionsWrite],
        )
        .audited_route(
            "/:id/inputs/:inputId/redeliver",
            post(redeliver_unknown_input),
            &[Scope::SessionsWrite],
        )
        .audited_route(
            "/:id/inputs/:inputId/cancel",
            post(cancel_unknown_input),
            &[Scope::SessionsWrite],
        )
        .audited_route(
            "/:id/messages/:mid/stop",
            post(stop_generation),
            &[Scope::SessionsWrite],
        )
        .audited_route(
            "/:id/background/end",
            post(end_background_wait),
            &[Scope::SessionsWrite],
        )
        // The owner's edit of a statement on the newest ontology changes card
        // (chat g5.chat.ontology-card.edit).
        .audited_route(
            "/:id/messages/:mid/ontology-card/edits",
            post(edit_ontology_card),
            &[Scope::SessionsWrite],
        )
        .audited_route(
            "/:id/timeline",
            get(get_session_timeline),
            &[Scope::SessionsRead],
        )
        .audited_route(
            "/:id/live-tail",
            get(get_session_live_tail),
            &[Scope::SessionsRead],
        )
        .audited_route(
            "/:id/attachments",
            post(upload_attachment),
            &[Scope::SessionsWrite],
        )
        // Session-fenced attachment byte read (routes/chat D30) — serves
        // user-origin and agent-origin rows for direct browser rendering.
        .audited_route(
            "/:id/attachments/:attachmentId/content",
            get(download_attachment_content),
            &[Scope::SessionsRead],
        )
        .audited_route("/:id/fork", post(fork_session), &[Scope::SessionsWrite]);

    router
        .into_router()
        // `/:id/messages` and `/:id/attachments` accept inline base64 file
        // attachments; raise the buffered body limit past Axum's 2 MiB default
        // so the bounded aggregate attachment set fits (routes/chat D19).
        .layer(DefaultBodyLimit::max(CHAT_SEND_BODY_LIMIT_BYTES))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/sessions`
///
/// Lists chat sessions for a workspace, paginated and filterable by
/// scope_type, in turn-activity order with each chat's activity for the
/// caller (chat `g5.chat.activity.order`).
/// Returns an empty array (not an error) if no sessions match.
async fn list_sessions(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<ResponseEnvelope<Vec<SessionListItemDTO>>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Chat endpoint called: list_sessions"
    );

    let viewer = auth.require_user_id()?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;

    let filters = chat_service::SessionListFilters {
        scope_type: query.scope_type,
    };

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let pagination = crate::services::PaginationParams { page, per_page };

    let result = state
        .chat_service
        .list_sessions(&mut db, params.workspace_id, viewer, filters, &pagination)
        .await?;

    let dtos: Vec<SessionListItemDTO> = result
        .data
        .into_iter()
        .map(SessionListItemDTO::from)
        .collect();

    let meta = PaginationMeta {
        page,
        per_page,
        total: result.total,
    };

    Ok(Json(ResponseEnvelope::paginated(dtos, meta)))
}

/// `POST /api/v1/workspaces/:workspaceId/sessions`
///
/// Creates a new interactive chat session in the workspace.
/// Returns 201 Created with the newly created session detail.
async fn create_session(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CreateSessionRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<SessionDetailDTO>>), AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Chat endpoint called: create_session"
    );

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let user_id = auth.require_user_id()?;
    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    ensure_workspace_mutable(&mut db, params.workspace_id, "chat.create_session").await?;

    // When the caller pins a Claude Code model, normalize it with the shared
    // provider-aware alias resolver before persisting, matching
    // `start_session`/`send_message` (chat route model-normalization rule) so
    // the stored session model is never a raw cross-provider slug. An omitted
    // model is left unset so the first-turn send resolves the effective default
    // — `create_session` does not create a first turn, so it must not fill one.
    let effective_model = match body.model.as_deref() {
        Some(requested) => {
            let org_id = resolve_org_id_for_workspace(&mut db, params.workspace_id).await?;
            let user_id = auth.require_user_id()?;
            Some(
                resolve_claude_chat_model_for_workspace(
                    &state,
                    &mut db,
                    org_id,
                    params.workspace_id,
                    user_id,
                    Some(requested),
                    // No session exists yet: the first turn resolves the
                    // effective assignment unless the launch names an account.
                    AgentLaunchFieldPreferenceV1::InheritConfigured,
                )
                .await?,
            )
        }
        None => None,
    };

    let req = chat_service::CreateSessionRequest {
        scope_type: body.scope_type,
        scope_ref: body.scope_ref,
        model: effective_model,
        title: body.title,
        agent_provider: ChatAgentProvider::ClaudeCode,
        derived_title: false,
        persona: None,
        persona_instructions: None,
    };

    let session = state
        .chat_service
        .create_session(&mut db, params.workspace_id, user_id, req)
        .await?;

    let dto: SessionDetailDTO = session.into();

    tracing::info!(
        workspace_id = %params.workspace_id,
        session_id = %dto.id,
        "Chat endpoint succeeded: create_session"
    );

    db.commit().await.map_err(AppError::from)?;
    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

/// `POST /api/v1/workspaces/:workspaceId/sessions/start`
///
/// Atomically creates a session and first message. Provider startup preflight
/// runs before durable rows are inserted, allowing provider setup errors to keep
/// the UI in draft/no-session state.
async fn start_session(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<StartSessionRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<StartSessionResponseDTO>>), AppError> {
    let start = if body.start_compute.unwrap_or(false) {
        // Admit the writer before recording its consent; Resume rechecks
        // ownership under its own locks.
        let mut db =
            scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"])
                .await?;
        // A retry of a start whose response was lost replays the claimed
        // request (`g5.chat.request-key`) and must not record a second Start:
        // the first one already carries the queued input.
        let replay = match body.request_key.as_deref() {
            Some(key) => {
                let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
                let user_id = auth.require_user_id()?;
                state
                    .workspace_messenger
                    .request_key_claimed(&mut db, org_id, user_id, key)
                    .await?
            }
            None => false,
        };
        db.rollback().await.map_err(AppError::from)?;
        if replay {
            None
        } else {
            Some(crate::routes::start_and_send::begin(&state, &auth, params.workspace_id).await?)
        }
    } else {
        None
    };
    let result =
        start_session_with(state.clone(), auth.clone(), params, body, start.as_ref()).await;
    if result.is_err() {
        if let Some(start) = &start {
            crate::routes::start_and_send::abandon(&state, &auth, start).await;
        }
    }
    result
}

async fn start_session_with(
    state: Arc<AppState>,
    auth: AuthContext,
    params: WorkspacePathParams,
    body: StartSessionRequestDTO,
    start: Option<&crate::routes::start_and_send::CompoundStart>,
) -> Result<(StatusCode, Json<ResponseEnvelope<StartSessionResponseDTO>>), AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:write"]).await?;
    let org_id = resolve_org_id_for_workspace(&mut db, params.workspace_id).await?;
    let user_id = auth.require_user_id()?;
    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    ensure_workspace_mutable(&mut db, params.workspace_id, "chat.start_session").await?;

    if body.agent_launch.is_some()
        && (body.agent_provider.is_some()
            || body.model.is_some()
            || body.permission_mode.is_some()
            || body.effort.is_some())
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "agentLaunch",
                "message": "agentLaunch cannot be combined with legacy agentProvider/model/permissionMode/effort fields",
            }),
        }));
    }
    let attachments = map_inline_attachments(body.attachments);
    let attachment_digest =
        crate::services::chat_service::inline_attachment_request_digest(&attachments)?;
    let workbench = workbench_identity(
        body.persona.as_deref(),
        body.phase.as_deref(),
        body.workbench_action.clone(),
        body.workbench_context.as_ref(),
        body.config_revision.as_deref(),
    )?;
    let message = WorkspaceMessage {
        org_id,
        workspace_id: params.workspace_id,
        actor_user_id: user_id,
        target: MessageTarget::NewSession {
            title: body.title,
            scope_type: body.scope_type,
            scope_ref: body.scope_ref,
        },
        content: body.content,
        agent_launch: body.agent_launch,
        legacy_preferences: crate::services::config_service::legacy_agent_launch_preferences(
            body.agent_provider,
            body.model.as_deref(),
            body.permission_mode.as_deref(),
            body.effort.as_deref(),
        ),
        attachment_digest,
        request_key: body.request_key,
        workbench,
    };
    // A persona start (workbench g5.workbench.persona) resolves the template
    // and the persona instructions only after the request key is claimed, from
    // the configuration snapshot the browser pinned, so a lost-response retry
    // replays the original session without re-reading Git
    // (g5.conversation.inbox.reservation). A stale revision is refused before
    // any session or input exists; the claim rolls back with the request.
    let sent = match (&message.workbench, body.workbench_context.as_ref()) {
        (Some(start), context) => {
            if let Some(replayed) = state.workspace_messenger.claim(&mut db, &message).await? {
                replayed
            } else {
                let config = state
                    .workspace_service
                    .load_workbench_config(&mut db, params.workspace_id)
                    .await?;
                let expansion = persona_start_expansion(&config, start, context, &message.content)?;
                state
                    .workspace_messenger
                    .send_claimed(&mut db, message, Some(expansion))
                    .await?
            }
        }
        (None, _) => state.workspace_messenger.send(&mut db, message).await?,
    };
    // Attachments belong to the first request; a replay already has them.
    if !sent.replayed {
        state
            .chat_service
            .persist_inline_session_input_attachments(
                &mut db,
                sent.session.id,
                org_id,
                user_id,
                sent.queued.input_id,
                attachments,
            )
            .await?;
    }
    let queued = sent.queued;
    // A replayed request already bound its input with the first request.
    if let (Some(start), false) = (start, sent.replayed) {
        crate::routes::start_and_send::bind(&mut db, start, queued.input_id).await?;
    }
    let dto = StartSessionResponseDTO {
        session: sent.session.into(),
        message: SendMessageResponseDTO {
            message_id: queued.input_id.to_string(),
            status: queued.task_status,
            created_at: queued.created_at.to_rfc3339(),
            input_id: Some(queued.input_id.to_string()),
            task_id: Some(queued.task_id.to_string()),
            delivery: Some(queued.delivery),
            start_compute: start.map(crate::routes::start_and_send::StartComputeDTO::from),
        },
        assistant_message_id: queued.input_id.to_string(),
        task_id: queued.task_id.to_string(),
    };

    db.commit().await.map_err(AppError::from)?;
    let status = if sent.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(ResponseEnvelope::ok(dto))))
}

/// `GET /api/v1/workspaces/:workspaceId/sessions/search?q=...`
///
/// Full-text search over session titles and summaries using Postgres GIN index.
/// Results are ordered by relevance score.
async fn search_sessions(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<SearchSessionsQuery>,
) -> Result<Json<ResponseEnvelope<Vec<SessionListItemDTO>>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        query = %query.q,
        "Chat endpoint called: search_sessions"
    );

    auth.require_user_id()?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);

    let pagination = crate::services::PaginationParams { page, per_page };

    let result = state
        .chat_service
        .search_sessions(&mut db, params.workspace_id, &query.q, &pagination)
        .await?;

    let dtos: Vec<SessionListItemDTO> = result
        .data
        .into_iter()
        .map(SessionListItemDTO::from)
        .collect();

    let meta = PaginationMeta {
        page,
        per_page,
        total: result.total,
    };

    Ok(Json(ResponseEnvelope::paginated(dtos, meta)))
}

/// Expose the frozen launch auth/account; live meter values arrive only over WebSocket.
async fn enrich_session_launch(
    state: &AppState,
    db: &mut ScopedDb,
    session_id: Uuid,
    dto: &mut SessionDetailDTO,
) {
    let launch = match state
        .chat_service
        .agent_launch_for_session(db, session_id)
        .await
    {
        Ok(Some(launch)) => launch,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(
                session_id = %session_id,
                error = %error,
                "Skipping display-only launch enrichment: launch lookup failed"
            );
            return;
        }
    };
    let auth_mode = launch.resolved.auth_mode;
    dto.auth_mode = Some(auth_mode);
    dto.account = launch.resolved.account().map(|account| SessionAccountDTO {
        id: account.id.to_string(),
        kind: account.kind,
        scope: account.scope,
    });
}

/// `GET /api/v1/sessions/:id`
///
/// Fetches a single session with all metadata.
/// Returns 404 if the session doesn't exist.
async fn get_session(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
) -> Result<Json<ResponseEnvelope<SessionDetailDTO>>, AppError> {
    let viewer = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(
        session_id = %params.id,
        "Chat endpoint called: get_session"
    );

    let session = state
        .chat_service
        .get_session_enriched(&mut db, params.id, viewer)
        .await?;

    let session_id = session.id;
    let mut dto: SessionDetailDTO = session.into();
    enrich_session_launch(&state, &mut db, session_id, &mut dto).await;

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// Body of `PUT /api/v1/sessions/:id/inspection`: the finished result the
/// caller's browser displayed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordSessionInspectionRequestDTO {
    pub task_id: Uuid,
    /// `null` for a turn that ended without any attempt.
    pub attempt: Option<i32>,
}

/// `PUT /api/v1/sessions/:id/inspection`
///
/// Records that the caller saw one finished result of the chat (chat
/// `g5.chat.inspection.acknowledge`). Any reader of the chat records their
/// own state; the watermark only moves forward. Returns the chat's activity
/// for the caller; `409` when the result is not a finished result of the chat.
async fn record_session_inspection(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Json(body): Json<RecordSessionInspectionRequestDTO>,
) -> Result<Json<ResponseEnvelope<chat_activity::SessionActivity>>, AppError> {
    let viewer = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;
    let outcome =
        chat_activity::record_inspection(&mut db, params.id, viewer, body.task_id, body.attempt)
            .await?;
    db.commit().await.map_err(AppError::from)?;
    if outcome.advanced {
        chat_activity::publish_inspection(
            &state.event_bus,
            outcome.workspace_id,
            params.id,
            viewer,
        );
    }
    Ok(Json(ResponseEnvelope::ok(outcome.activity)))
}

/// `GET /api/v1/sessions/:id/task`
///
/// The session's newest live task (routes/chat I19, issue #1597 review): the
/// server-authoritative session→task resolution the client uses for the
/// background-agents card instead of scanning windowed task listings.
/// Reuses the same resolver lifecycle uses, so sibling/recovery-task
/// selection can never diverge from server behavior. `data: null` when the
/// session has no live task — a normal state, not an error; `404` stays
/// reserved for absent or cross-org sessions.
async fn get_session_task(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
) -> Result<Json<ResponseEnvelope<Option<crate::routes::tasks::TaskDTO>>>, AppError> {
    auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(
        session_id = %params.id,
        "Chat endpoint called: get_session_task"
    );

    let found = state
        .task_service
        .find_active_task_for_session(&mut db, params.id)
        .await?;
    let dto = match found {
        Some((task_id, _status)) => {
            let mut task = state.task_service.get_task(&mut db, task_id).await?;
            if task.status == "running" {
                if let Some(attempt) = task.latest_attempt.as_ref().map(|a| a.attempt_number) {
                    task.background_wait = state
                        .task_service
                        .live_background_wait(&mut db, task_id, attempt)
                        .await?;
                    task.native_steering_available = task.background_wait.is_none()
                        && state
                            .task_service
                            .native_steering_available(&mut db, task_id)
                            .await?;
                }
            }
            Some(task)
        }
        None => None,
    };

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `PATCH /api/v1/sessions/:id`
///
/// Updates lightweight session metadata such as the user-editable title.
async fn update_session(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Json(body): Json<UpdateSessionRequestDTO>,
) -> Result<Json<ResponseEnvelope<SessionDetailDTO>>, AppError> {
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    tracing::debug!(
        session_id = %params.id,
        "Chat endpoint called: update_session"
    );

    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    let ws_id = resolve_chat_session_context(&mut db, params.id)
        .await?
        .workspace_id;
    ensure_workspace_mutable(&mut db, ws_id, "chat.update_session").await?;

    let mut session = state
        .chat_service
        .update_session_title(&mut db, params.id, body.title)
        .await?;
    state
        .chat_service
        .enrich_session_detail(&mut db, &mut session)
        .await?;

    let dto: SessionDetailDTO = session.into();

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/sessions/:id/messages`
///
/// Fetches paginated messages for a session, ordered by `message_index`.
/// Uses cursor-based pagination.
/// Returns 404 if the session doesn't exist.
async fn get_messages(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Query(query): Query<GetMessagesQuery>,
) -> Result<Json<ResponseEnvelope<MessageListResponseDTO>>, AppError> {
    tracing::debug!(
        session_id = %params.id,
        cursor = ?query.cursor,
        limit = query.limit,
        tail = query.tail,
        "Chat endpoint called: get_messages"
    );

    let limit = query.limit.clamp(1, 500);
    // Tail is only meaningful without a cursor; forward pagination wins when
    // both are supplied (routes/chat I6).
    let tail = query.tail && query.cursor.is_none();

    auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;

    let result = state
        .chat_service
        .get_messages(&mut db, params.id, query.cursor, limit, tail)
        .await?;

    let dto: MessageListResponseDTO = result.into();

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/sessions/:id/timeline`
///
/// Retrieve the paginated timeline of operational activity across all tasks
/// associated with this chat session.  Uses cursor-based pagination.
///
/// Read access is workspace-scoped (`scoped_db_for_chat_session_view`): chat
/// tasks are workspace-visible per issue #899, so the timeline's task
/// metadata no longer requires session ownership (routes/chat spec D0).
async fn get_session_timeline(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Query(query): Query<GetSessionTimelineQuery>,
) -> Result<Json<ResponseEnvelope<SessionTimelineResponseDTO>>, AppError> {
    tracing::debug!(
        session_id = %params.id,
        cursor = ?query.cursor,
        limit = query.limit,
        "Chat endpoint called: get_session_timeline"
    );

    auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;

    let limit = query.limit.clamp(1, 200);
    let (entries, cursor, has_more) = state
        .chat_service
        .get_session_timeline(&mut db, params.id, query.cursor, limit)
        .await?;

    Ok(Json(ResponseEnvelope::ok(SessionTimelineResponseDTO {
        entries,
        cursor,
        has_more,
    })))
}

/// A follow-up input for an existing chat, as the composer sends it.
pub(crate) struct FollowUp {
    pub content: String,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub effort: Option<String>,
    pub agent_provider: Option<ChatAgentProvider>,
    pub account: Option<AgentAccountSelectionV1>,
    pub attachments: Option<Vec<InlineChatAttachmentDTO>>,
}

/// Queue a follow-up in `session_id` for `user_id`, the workspace owner,
/// inheriting the chat's launch as the composer does: the provider is fixed,
/// the permission mode frozen after the first turn, model, effort and account
/// fall back to the chat's saved and previous choices (routes/chat D33).
///
/// Shared by the UI's send and the workspace agent's child follow-up
/// (`g5.story.agent-chat-child`); the caller has admitted the session and
/// owns the transaction and any steering after commit.
pub(crate) async fn queue_follow_up(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
    follow_up: FollowUp,
) -> Result<crate::services::agent_dispatch::QueuedAgentInput, AppError> {
    // Freeze a bare session's first send before a competing send reads its
    // defaults. Enqueue takes this same lock before touching session rows.
    chat_service::lock_session_turn_allocation(db.conn(), session_id).await?;
    let current_session = state.chat_service.get_session(db, session_id).await?;
    let requested_provider = follow_up
        .agent_provider
        .unwrap_or(current_session.agent_provider);
    if requested_provider != current_session.agent_provider {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "The agent provider is fixed for an existing chat. Start a new chat to use a different provider.".to_string(),
        }));
    }
    let previous_launch = state
        .chat_service
        .agent_launch_for_session(db, session_id)
        .await?;
    let frozen_permission = frozen_chat_permission_mode(
        state,
        db,
        session_id,
        current_session.agent_provider,
        &current_session.provider_state,
    )
    .await?;
    if let (Some(requested), Some(frozen)) = (
        follow_up.permission_mode.as_deref(),
        frozen_permission.as_deref(),
    ) {
        if requested != frozen {
            return Err(reject_changed_chat_permission_mode(requested, frozen));
        }
    }
    let model = follow_up
        .model
        .as_deref()
        .or(current_session.model.as_deref())
        .or_else(|| {
            previous_launch
                .as_ref()
                .map(|launch| launch.resolved.model.value.as_str())
        });
    let permission = frozen_permission
        .as_deref()
        .or(follow_up.permission_mode.as_deref())
        .or_else(|| {
            previous_launch
                .as_ref()
                .map(|launch| launch.resolved.permission_mode.value.as_str())
        });
    let effort = follow_up.effort.as_deref().or_else(|| {
        previous_launch
            .as_ref()
            .map(|launch| launch.resolved.effort.value.as_str())
    });
    let mut turn_preferences = crate::services::config_service::legacy_agent_launch_preferences(
        Some(requested_provider),
        model,
        permission,
        effort,
    );
    // Only a turn that names no account reads the saved one, so a malformed
    // saved account fails that turn (typed) while a named account replaces
    // it and recovers the chat.
    let saved_account = match follow_up.account {
        Some(_) => None,
        None => persisted_session_account(
            &current_session.provider_state,
            current_session.agent_provider,
        )?,
    };
    turn_preferences.account = follow_up_account_preference(
        follow_up.account.clone(),
        saved_account,
        previous_launch.as_ref(),
    );
    let persisted_launch = state
        .config_service
        .resolve_launch(
            db,
            org_id,
            workspace_id,
            Some(user_id),
            None,
            turn_preferences,
            &state.provider_account_service,
        )
        .await?;
    let attachments = map_inline_attachments(follow_up.attachments);
    let initial_settings = if frozen_permission.is_none() {
        Some(chat_service::first_turn_settings(
            &persisted_launch.resolved,
        )?)
    } else {
        None
    };
    let queued = state
        .task_service
        .enqueue_agent_input(
            db,
            crate::services::agent_dispatch::EnqueueAgentInput {
                task_name: None,
                session_id,
                actor_user_id: user_id,
                content: follow_up.content,
                artifacts: serde_json::json!([]),
                input_origin: "human".to_string(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: persisted_launch.preferences,
            },
        )
        .await?;
    // A bare session saves its first-turn defaults here. On later turns, an
    // account the sender names becomes the chat's saved account, so the
    // chat keeps it until the sender names another (routes/chat D33). Other
    // choices travel with each input and are not saved (chat_agent I42).
    // Written after the enqueue, which takes the session-turn lock before the
    // session row as every turn allocation does.
    let settings_patch = match initial_settings {
        Some(settings) => Some(settings),
        None => follow_up
            .account
            .as_ref()
            .map(chat_service::next_turn_account_setting)
            .transpose()?,
    };
    if let Some(settings_patch) = settings_patch {
        state
            .chat_service
            .patch_provider_state_namespace(
                db,
                session_id,
                provider_state_namespace(current_session.agent_provider),
                serde_json::json!({}),
                settings_patch,
                serde_json::json!({}),
            )
            .await?;
    }
    state
        .chat_service
        .persist_inline_session_input_attachments(
            db,
            session_id,
            org_id,
            user_id,
            queued.input_id,
            attachments,
        )
        .await?;
    Ok(queued)
}

/// `POST /api/v1/sessions/:id/messages`
///
/// Persists a user message and triggers LLM processing. The actual LLM
/// response is streamed via WebSocket events (`chat.chunk`, `chat.done`,
/// `chat.error`) — this endpoint only persists the user message and creates
/// a placeholder assistant message.
///
/// Returns 201 with the pending assistant message ID.
async fn send_message(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Json(body): Json<SendMessageRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<SendMessageResponseDTO>>), AppError> {
    let start = if body.start_compute.unwrap_or(false) {
        let mut db =
            scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
        let session_context = resolve_chat_session_context(&mut db, params.id).await?;
        db.rollback().await.map_err(AppError::from)?;
        Some(
            crate::routes::start_and_send::begin(&state, &auth, session_context.workspace_id)
                .await?,
        )
    } else {
        None
    };
    let result = send_message_with(state.clone(), auth.clone(), params, body, start.as_ref()).await;
    if result.is_err() {
        if let Some(start) = &start {
            crate::routes::start_and_send::abandon(&state, &auth, start).await;
        }
    }
    result
}

async fn send_message_with(
    state: Arc<AppState>,
    auth: AuthContext,
    params: SessionPathParams,
    body: SendMessageRequestDTO,
    start: Option<&crate::routes::start_and_send::CompoundStart>,
) -> Result<(StatusCode, Json<ResponseEnvelope<SendMessageResponseDTO>>), AppError> {
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    let session_context = resolve_chat_session_context(&mut db, params.id).await?;
    let org_id = session_context.org_id;
    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    ensure_workspace_mutable(&mut db, session_context.workspace_id, "chat.send_message").await?;
    tracing::debug!(
        session_id = %params.id,
        "Chat endpoint called: send_message"
    );

    let user_id = auth.require_user_id()?;

    let queued = queue_follow_up(
        &state,
        &mut db,
        org_id,
        session_context.workspace_id,
        params.id,
        user_id,
        FollowUp {
            content: body.content,
            model: body.model,
            permission_mode: body.permission_mode,
            effort: body.effort,
            agent_provider: body.agent_provider,
            account: body.account,
            attachments: body.attachments,
        },
    )
    .await?;
    if let Some(start) = start {
        crate::routes::start_and_send::bind(&mut db, start, queued.input_id).await?;
    }
    let steer = state
        .task_service
        .reserve_agent_steer(&mut db, user_id, params.id, queued.input_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;

    if let Some(claim) = steer {
        // This work records the result of an already-authorized input. It must
        // outlive a disconnected request and must not require the former owner
        // to retain authority after the accepted transaction commits.
        let state = state.clone();
        let completion = tokio::spawn(async move {
            let deps = crate::db::maintenance::TenantMaintenanceDeps {
                system_pool: state.system_pool.clone(),
                tenant_pool: state.tenant_pool.clone(),
                authz: state.authz.clone(),
                repo_storage: state.repo_storage.clone(),
            };
            state
                .task_service
                .deliver_claimed_steer(&deps, &claim)
                .await
        });
        // Observe completion independently too: a slow RPC or blocked receipt
        // transaction must not delay returning the committed acceptance.
        let input_id = queued.input_id;
        tokio::spawn(async move {
            match completion.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%input_id, %error,
                    "accepted input steering completion failed; journal and settlement retain recovery"),
                Err(error) => tracing::error!(%input_id, %error,
                    "accepted input steering task aborted; journal and settlement retain recovery"),
            }
        });
    }
    let dto = SendMessageResponseDTO {
        message_id: queued.input_id.to_string(),
        status: queued.task_status.clone(),
        created_at: queued.created_at.to_rfc3339(),
        input_id: Some(queued.input_id.to_string()),
        task_id: Some(queued.task_id.to_string()),
        delivery: Some(queued.delivery.clone()),
        start_compute: start.map(crate::routes::start_and_send::StartComputeDTO::from),
    };
    tracing::info!(session_id = %params.id, message_id = %dto.message_id,
        "Chat endpoint succeeded: send_message");

    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

async fn get_codex_auth(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;
    let result = state
        .task_service
        .session_codex_auth(&mut db, params.id, user_id)
        .await?;
    // The service fences its two owner-only reads with a workspace share lock.
    // Release it before replying, rather than queuing rollback when db drops.
    db.commit().await.map_err(AppError::from)?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(ResponseEnvelope::ok(result)),
    ))
}

async fn list_session_inputs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<crate::services::agent_dispatch::SessionInputView>>>, AppError>
{
    auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;
    let inputs = state
        .task_service
        .list_session_inputs(&mut db, params.id)
        .await?;
    Ok(Json(ResponseEnvelope::ok(inputs)))
}

async fn edit_queued_input(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionInputPathParams>,
    Json(body): Json<EditQueuedInputRequestDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::agent_dispatch::SessionInputView>>, AppError> {
    let user_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    let input = state
        .task_service
        .edit_queued_session_input(
            &mut db,
            user_id,
            params.id,
            params.input_id,
            body.expected_revision,
            body.content,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(input)))
}

async fn withdraw_queued_input(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionInputPathParams>,
    Json(body): Json<WithdrawQueuedInputRequestDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::agent_dispatch::SessionInputView>>, AppError> {
    let user_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    let input = state
        .task_service
        .withdraw_queued_session_input(
            &mut db,
            user_id,
            params.id,
            params.input_id,
            body.expected_revision,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(input)))
}

async fn redeliver_unknown_input(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionInputPathParams>,
) -> Result<Json<ResponseEnvelope<crate::services::agent_dispatch::SessionInputView>>, AppError> {
    let user_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    let input = state
        .task_service
        .authorize_unknown_input_redelivery(&mut db, user_id, params.id, params.input_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(input)))
}

async fn cancel_unknown_input(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionInputPathParams>,
) -> Result<Json<ResponseEnvelope<crate::services::agent_dispatch::SessionInputView>>, AppError> {
    let user_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    let input = state
        .task_service
        .cancel_unknown_session_input(&mut db, user_id, params.id, params.input_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(input)))
}

/// `POST /api/v1/sessions/:id/messages/:mid/stop`
///
/// Ends the chat turn that `mid` names, the input id a send returned or the
/// turn's reply (`g5.agents.lifecycle.chat-stop`). The reply keeps its partial
/// output, marked stopped, and queued messages run next; a message still
/// waiting behind another turn is withdrawn alone. `stopped` is false when the
/// turn had already ended. Returns 422 for an id outside the session.
async fn stop_generation(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<StopGenerationPathParams>,
) -> Result<Json<ResponseEnvelope<StopGenerationResponseDTO>>, AppError> {
    tracing::debug!(
        session_id = %params.id,
        message_id = %params.mid,
        "Chat endpoint called: stop_generation"
    );

    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    // The id the client holds is the one send/start returned: the session
    // input's id, which only gains a chat_messages row once the runtime
    // accepts the turn. Stopping a still-queued turn is the common case.
    ensure_session_turn_id(&mut db, params.id, params.mid, "messageId").await?;

    let user_id = auth.require_user_id()?;

    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    let ws_id = resolve_chat_session_context(&mut db, params.id)
        .await?
        .workspace_id;
    ensure_workspace_mutable(&mut db, ws_id, "chat.stop_generation").await?;

    let stop = state
        .task_service
        .end_chat_turn(&mut db, user_id, params.id, params.mid)
        .await?;

    tracing::info!(
        session_id = %params.id,
        message_id = %params.mid,
        "Chat endpoint succeeded: stop_generation"
    );

    db.commit().await.map_err(AppError::from)?;
    // The intent is committed: a failed runtime call is reported as
    // undelivered and redelivered by the attempt's follower, never as a
    // failed Stop (`g5.chat.controls`).
    let disposition = match &stop {
        ChatTurnStop::Stopped(stop) => state
            .task_service
            .deliver_workspace_task_cancel(stop)
            .await?
            .as_str(),
        ChatTurnStop::Withdrawn => "withdrawn",
        ChatTurnStop::AlreadyEnded => "already_ended",
    };
    Ok(Json(ResponseEnvelope::ok(StopGenerationResponseDTO {
        stopped: !matches!(stop, ChatTurnStop::AlreadyEnded),
        disposition,
    })))
}

/// `GET /api/v1/sessions/:id/live-tail`
///
/// The unpersisted tail of the session's running attempt, read from this
/// process's follower (`g5.chat.live-tail-recovery`). Read authority is the
/// transcript's; the entries were redacted by the guest.
async fn get_session_live_tail(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
) -> Result<Json<ResponseEnvelope<LiveTailResponseDTO>>, AppError> {
    auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session_view(&state, &auth, params.id, &["workspaces:read"]).await?;
    let running = state
        .task_service
        .running_session_attempt(&mut db, params.id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let Some((task_id, attempt, vm_id)) = running else {
        return Ok(Json(ResponseEnvelope::ok(LiveTailResponseDTO {
            state: "none",
            task_id: None,
            attempt: None,
            persisted_through: None,
            fetched_through: None,
            entries: Vec::new(),
        })));
    };
    let tail = state.followers.tail(task_id, attempt, vm_id);
    let Some(tail) = tail.filter(|tail| tail.complete) else {
        return Ok(Json(ResponseEnvelope::ok(LiveTailResponseDTO {
            state: "catching_up",
            task_id: Some(task_id),
            attempt: Some(attempt),
            persisted_through: None,
            fetched_through: None,
            entries: Vec::new(),
        })));
    };
    Ok(Json(ResponseEnvelope::ok(LiveTailResponseDTO {
        state: "live",
        task_id: Some(task_id),
        attempt: Some(attempt),
        persisted_through: Some(tail.persisted_through),
        fetched_through: Some(tail.fetched_through),
        entries: tail
            .entries
            .into_iter()
            .map(|entry| chat_service::SessionTimelineEntry {
                id: entry.id.to_string(),
                entry_type: entry.entry_type,
                content: entry.content,
                metadata: entry.metadata,
                created_at: entry.created_at.to_rfc3339(),
            })
            .collect(),
    })))
}

/// `POST /api/v1/sessions/:id/messages/:mid/ontology-card/edits`
///
/// The workspace owner replaces one added or modified statement's text on the
/// newest ontology changes card, while no turn of the chat runs
/// (`g5.chat.ontology-card.edit`). The file is written only when its statement
/// still reads as the card shows it, under the editor's version check; then the
/// edit is recorded on the card and the next turn's agent is told. A retry with
/// the same key completes a recording that failed without writing again.
async fn edit_ontology_card(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<StopGenerationPathParams>,
    Json(body): Json<chat_ontology::CardEditRequest>,
) -> Result<Json<ResponseEnvelope<chat_ontology::CardEdit>>, AppError> {
    let session_id = params.id;
    let message_id = params.mid;
    let user_id = auth.require_user_id()?;
    let mut db =
        scoped_db_for_chat_session(&state, &auth, session_id, &["workspaces:write"]).await?;
    let workspace_id = resolve_chat_session_context(&mut db, session_id)
        .await?
        .workspace_id;
    ensure_workspace_mutable(&mut db, workspace_id, "chat.edit_ontology_card").await?;
    chat_ontology::require_workspace_owner(&mut db, workspace_id, user_id).await?;
    let path = crate::routes::files::sanitize_path(&body.path)?;
    if path != body.path {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({"field": "path", "error": "must be a normalized path"}),
        }));
    }
    let expected_body =
        match chat_ontology::plan_edit(&mut db, session_id, message_id, &body).await? {
            chat_ontology::EditPlan::Recorded(edit) => {
                db.rollback().await.map_err(AppError::from)?;
                return Ok(Json(ResponseEnvelope::ok(edit)));
            }
            chat_ontology::EditPlan::Write { expected_body } => expected_body,
        };
    let root_path = chat_ontology::member_root_path(&mut db, workspace_id, body.repo_id).await?;
    super::workspaces::ensure_workspace_file_mutation_allowed(
        &mut db,
        workspace_id,
        user_id,
        body.repo_id,
    )
    .await?;
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;

    // The owner's running incarnation: the same live read and conditional
    // write the editor uses, never a committed fallback.
    let handle = crate::services::workspace_file_save::expected_owner_handle(
        &state.workspace_runtime,
        workspace_id,
        user_id,
        None,
        "start the workspace to edit the card",
    )
    .await?;
    let live = crate::services::workspace_file_save::read_live_file(
        &state.workspace_runtime,
        &handle,
        user_id,
        body.repo_id,
        &path,
    )
    .await?;
    if live.encoding != "utf-8" {
        return Err(AppError::from(G5Error::Conflict {
            reason: "the ontology file is not text".into(),
        }));
    }
    if let Some(edited) = chat_ontology::replace_statement_body(
        &live.content,
        &body.statement_id,
        &expected_body,
        &body.text,
    )? {
        super::workspaces::write_live_workspace_file(
            &state,
            &scope,
            &handle,
            user_id,
            body.repo_id,
            &path,
            &edited,
            "utf-8",
            Some(&live.hash),
        )
        .await?;
    }

    let mut db =
        scoped_db_for_chat_session(&state, &auth, session_id, &["workspaces:write"]).await?;
    let edit = chat_ontology::record_edit(
        &mut db,
        session_id,
        message_id,
        &body,
        user_id,
        root_path.as_deref(),
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;
    state.event_bus.publish(
        &crate::ws::Channel::Chat(session_id),
        &crate::ws::WsEvent::ChatMessageUpdated {
            session_id,
            message_id,
        },
    );
    tracing::info!(%session_id, %message_id, statement = %body.statement_id, "ontology card statement edited");
    Ok(Json(ResponseEnvelope::ok(edit)))
}

/// `POST /api/v1/sessions/:id/background/end`
///
/// The owner ends the named attempt's wait on Claude's background work. The
/// attempt stops that work; when Claude then exits cleanly the attempt
/// succeeds, and when Claude has to be stopped the turn ends as the chat's
/// Stop ends it (a workflow turn is held for Continue). Either way queued input
/// runs next. Returns 409 when that attempt is no longer the session's running
/// attempt or could not take the request.
async fn end_background_wait(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Json(body): Json<EndBackgroundWaitRequestDTO>,
) -> Result<Json<ResponseEnvelope<EndBackgroundWaitResponseDTO>>, AppError> {
    let session_id = params.id;
    let mut db =
        scoped_db_for_chat_session(&state, &auth, session_id, &["workspaces:write"]).await?;
    let user_id = auth.require_user_id()?;
    let ws_id = resolve_chat_session_context(&mut db, session_id)
        .await?
        .workspace_id;
    ensure_workspace_mutable(&mut db, ws_id, "chat.end_background_wait").await?;
    state
        .task_service
        .end_session_background_wait(db, user_id, session_id, body.task_id, body.attempt)
        .await?;
    tracing::info!(session_id = %session_id, "Chat endpoint succeeded: end_background_wait");
    Ok(Json(ResponseEnvelope::ok(EndBackgroundWaitResponseDTO {
        delivered: true,
    })))
}

/// `POST /api/v1/sessions/:id/fork`
///
/// Creates a new session with messages copied from the source session up to
/// a specified message index.
/// Returns 201 with the new session ID and count of copied messages.
async fn fork_session(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Json(body): Json<ForkSessionRequestDTO>,
) -> Result<(StatusCode, Json<ResponseEnvelope<ForkSessionResponseDTO>>), AppError> {
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    tracing::debug!(
        session_id = %params.id,
        from_message_index = body.from_message_index,
        "Chat endpoint called: fork_session"
    );

    let _user_id = auth.require_user_id()?;

    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    let ws_id = resolve_chat_session_context(&mut db, params.id)
        .await?
        .workspace_id;
    ensure_workspace_mutable(&mut db, ws_id, "chat.fork_session").await?;

    let dto: ForkSessionResponseDTO = if let Some(persona) = body.persona.as_deref() {
        // "Follow view" (workbench g5.workbench.persona): the fork, its
        // persona with frozen instructions and its first inbox input commit
        // together under one request identity through the messenger, so no
        // persona-labelled fork exists without its instructions. The copied
        // transcript is historical context, cut at the last completed
        // assistant unit while a turn is active; an empty or queued-only
        // source starts a new session instead.
        let phase = WorkbenchPhase::parse(persona).ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({ "field": "persona", "message": "must be one of be, se, ie" }),
            })
        })?;
        let user_id = auth.require_user_id()?;
        let org_id = resolve_org_id_for_workspace(&mut db, ws_id).await?;
        // The request identity hashes exactly the submitted fields (source
        // session, requested index, title, persona, revision), never the
        // source's mutable state: the key is claimed FIRST, the effective
        // boundary (or the new-session decision) is resolved once after the
        // claim and frozen on the created session (`forked_from_id`,
        // `forked_at_index`), and a replay reads it back from there.
        let mut message = WorkspaceMessage {
            org_id,
            workspace_id: ws_id,
            actor_user_id: user_id,
            target: MessageTarget::ForkSession {
                source_session_id: params.id,
                from_message_index: body.from_message_index,
                title: body.title.clone(),
            },
            content: String::new(),
            agent_launch: None,
            legacy_preferences: Default::default(),
            attachment_digest: None,
            request_key: body.request_key,
            workbench: Some(WorkbenchIdentity {
                persona: persona.to_string(),
                phase: None,
                action_id: Some("follow-view".to_string()),
                context: None,
                config_revision: body.config_revision.clone().unwrap_or_default(),
            }),
        };
        let sent = if let Some(replayed) =
            state.workspace_messenger.claim(&mut db, &message).await?
        {
            replayed
        } else {
            let config = state
                .workspace_service
                .load_workbench_config(&mut db, ws_id)
                .await?;
            if let Some(pinned) = body.config_revision.as_deref() {
                ensure_workbench_revision(&config, pinned)?;
            }
            let boundary = state
                .chat_service
                .persona_fork_boundary(&mut db, params.id, body.from_message_index)
                .await?;
            let copied = boundary.map_or(0, |index| index + 1);
            message.target = match boundary {
                Some(from_message_index) => MessageTarget::ForkSession {
                    source_session_id: params.id,
                    from_message_index,
                    title: body.title,
                },
                None => MessageTarget::NewSession {
                    title: body
                        .title
                        .or_else(|| Some(format!("{} chat", phase.display_name()))),
                    scope_type: None,
                    scope_ref: None,
                },
            };
            let instructions = config.personas.phase(phase).text.clone();
            let expansion = PersonaExpansion {
                persona: persona.to_string(),
                content: persona_first_message(&instructions, &follow_view_message(phase, copied)),
                instructions,
            };
            state
                .workspace_messenger
                .send_claimed(&mut db, message, Some(expansion))
                .await?
        };
        ForkSessionResponseDTO {
            session_id: sent.session.id.to_string(),
            // The frozen decision: a fork copied `forked_at_index + 1` rows, a
            // fresh session copied none.
            messages_copied: sent.session.forked_at_index.map_or(0, |index| index + 1),
        }
    } else {
        let req = chat_service::ForkSessionRequest {
            from_message_index: body.from_message_index,
            title: body.title,
            persona: None,
        };
        state
            .chat_service
            .fork_session(&mut db, params.id, req)
            .await?
            .into()
    };

    tracing::info!(
        session_id = %params.id,
        new_session_id = %dto.session_id,
        messages_copied = dto.messages_copied,
        "Chat endpoint succeeded: fork_session"
    );

    db.commit().await.map_err(AppError::from)?;

    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

/// `POST /api/v1/sessions/:id/attachments`
///
/// Persists an uploaded chat attachment and returns storage metadata.
async fn upload_attachment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<SessionPathParams>,
    Json(body): Json<AttachmentUploadRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<ChatAttachmentResponseDTO>>,
    ),
    AppError,
> {
    let mut db =
        scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:write"]).await?;
    let context = resolve_chat_session_context(&mut db, params.id).await?;
    // Read-only workspaces block direct chat-content writes (DirectWorkspaceWrite).
    ensure_workspace_mutable(&mut db, context.workspace_id, "chat.upload_attachment").await?;
    ensure_session_message_id(&mut db, params.id, body.message_id, "messageId").await?;
    let user_id = auth.require_user_id()?;
    let attachment = state
        .chat_service
        .create_attachment(
            &mut db,
            params.id,
            context.org_id,
            user_id,
            chat_service::CreateAttachmentRequest {
                message_id: body.message_id,
                file_name: body.file_name,
                content_type: body.content_type,
                content_base64: body.content_base64,
            },
        )
        .await?;
    let dto = ChatAttachmentResponseDTO::from(attachment);

    tracing::info!(
        session_id = %params.id,
        attachment_id = %dto.id,
        "Chat endpoint succeeded: upload_attachment"
    );
    db.commit().await.map_err(AppError::from)?;
    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

/// `GET /api/v1/sessions/:id/attachments/:attachmentId/content` (routes/chat
/// D30): session-fenced raw attachment bytes for direct browser rendering —
/// serves user-origin and agent-origin (`agent-screenshots/`) rows alike. No
/// JSON envelope: the consumer is an `<img>` element or a file save, and the
/// 5 MiB attachment cap bounds the body.
async fn download_attachment_content(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<AttachmentContentPathParams>,
) -> Result<axum::response::Response, AppError> {
    let mut db = scoped_db_for_chat_session(&state, &auth, params.id, &["workspaces:read"]).await?;
    let (file_name, content_type, bytes) = state
        .chat_service
        .get_attachment_content(&mut db, params.id, params.attachment_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;

    let mut response = axum::response::Response::new(axum::body::Body::from(bytes));
    // Inline rendering is limited to four inert raster formats. SVG, HTML,
    // unknown, and missing MIME values download as opaque octet-stream bytes;
    // `nosniff` stops the browser from second-guessing either path.
    let stored_type = content_type.unwrap_or_default();
    let inline_image = is_safe_inline_attachment_content_type(&stored_type);
    let content_type_value = if inline_image {
        stored_type
    } else {
        "application/octet-stream".to_string()
    };
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(&content_type_value).unwrap_or(
            axum::http::HeaderValue::from_static("application/octet-stream"),
        ),
    );
    response.headers_mut().insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    // The filename is advisory for saves; a name that cannot ride a header
    // value falls back to a safe constant rather than failing the read.
    let disposition_kind = if inline_image { "inline" } else { "attachment" };
    let disposition = format!(
        "{disposition_kind}; filename=\"{}\"",
        file_name.replace('"', "")
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&disposition)
            .unwrap_or(axum::http::HeaderValue::from_static("attachment")),
    );
    Ok(response)
}

#[cfg(test)]
mod attachment_content_tests {
    use super::*;

    #[test]
    fn only_safe_raster_types_are_inline_renderable() {
        for content_type in SAFE_INLINE_ATTACHMENT_CONTENT_TYPES {
            assert!(is_safe_inline_attachment_content_type(content_type));
        }
        assert!(is_safe_inline_attachment_content_type("IMAGE/PNG"));
        for content_type in [
            "image/svg+xml",
            "text/html",
            "application/pdf",
            "application/octet-stream",
            "",
        ] {
            assert!(!is_safe_inline_attachment_content_type(content_type));
        }
    }
}

#[cfg(test)]
mod subscription_usage_dto_tests {
    use super::*;
    use crate::models::ProviderQuotaSnapshot;

    fn snapshot() -> ProviderQuotaSnapshot {
        ProviderQuotaSnapshot {
            org_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            provider: "claude_code".to_string(),
            auth_mode: "anthropic_oauth".to_string(),
            account_id: Uuid::nil(),
            // f32-exact fractions so the JSON round-trip is bit-precise.
            five_hour_utilization: Some(0.5),
            five_hour_reset_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
            seven_day_utilization: Some(0.25),
            seven_day_reset_at: Some(DateTime::<Utc>::from_timestamp(1_700_600_000, 0).unwrap()),
            representative_claim: Some("five_hour".to_string()),
            unified_status: Some("allowed".to_string()),
            captured_at: DateTime::<Utc>::from_timestamp(1_699_990_000, 0).unwrap(),
            created_at: DateTime::<Utc>::from_timestamp(1_699_990_000, 0).unwrap(),
            updated_at: DateTime::<Utc>::from_timestamp(1_699_990_000, 0).unwrap(),
        }
    }

    /// The display-only projection mirrors only the usage-relevant snapshot
    /// fields in camelCase and drops the key/bookkeeping columns
    /// (agent_interaction_ux D8; provider quota snapshot D13).
    #[test]
    fn subscription_usage_dto_serializes_display_fields_camel_case() {
        let dto = SubscriptionUsageDTO::from(snapshot());
        let value = serde_json::to_value(&dto).expect("serialize DTO");
        assert_eq!(value["fiveHourUtilization"], 0.5);
        assert_eq!(value["sevenDayUtilization"], 0.25);
        assert_eq!(value["representativeClaim"], "five_hour");
        assert_eq!(value["unifiedStatus"], "allowed");
        assert!(value.get("capturedAt").is_some());
        assert!(value.get("fiveHourResetAt").is_some());
        // Key + internal bookkeeping columns never leak to the client.
        assert!(value.get("orgId").is_none());
        assert!(value.get("workspaceId").is_none());
        assert!(value.get("provider").is_none());
        assert!(value.get("authMode").is_none());
        assert!(value.get("createdAt").is_none());
        assert!(value.get("updatedAt").is_none());
    }

    /// The frozen auth mode serializes as the same snake_case wire string the
    /// client gates the usage chip on (`anthropic_oauth`), and both additive
    /// fields are omitted when unset so old payloads stay byte-identical.
    #[test]
    fn session_detail_auth_mode_serializes_snake_case_and_omits_when_absent() {
        let value = serde_json::to_value(Some(AgentAuthModeV1::AnthropicOauth))
            .expect("serialize auth mode");
        assert_eq!(value, serde_json::json!("anthropic_oauth"));

        // A None auth_mode/subscription_usage pair is skipped entirely —
        // asserted on the REAL `SessionDetailDTO`, so removing its
        // `skip_serializing_if` attributes fails this test rather than a
        // stand-in probe struct that merely re-proves serde semantics.
        let dto = SessionDetailDTO {
            id: Uuid::nil().to_string(),
            workspace_id: Uuid::nil().to_string(),
            scope_type: "workspace".to_string(),
            scope_ref: None,
            model: None,
            agent_provider: ChatAgentProvider::ClaudeCode,
            provider_state: serde_json::json!({}),
            title: None,
            summary: None,
            tags: Vec::new(),
            message_count: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            forked_from_id: None,
            forked_at_index: None,
            linked_commits: Vec::new(),
            created_at: "2026-08-19T00:00:00Z".to_string(),
            updated_at: "2026-08-19T00:00:00Z".to_string(),
            can_send: true,
            persona: None,
            auth_mode: None,
            subscription_usage: None,
            account: None,
            next_account: None,
            activity: None,
        };
        let value = serde_json::to_value(&dto).expect("serialize session detail DTO");
        assert!(value.get("authMode").is_none());
        assert!(value.get("subscriptionUsage").is_none());
        assert!(value.get("account").is_none());
    }
}

#[cfg(test)]
mod provider_state_namespace_tests {
    use super::*;

    #[test]
    fn namespace_is_codex_for_codex_else_claude_code() {
        assert_eq!(provider_state_namespace(ChatAgentProvider::Codex), "codex");
        assert_eq!(
            provider_state_namespace(ChatAgentProvider::ClaudeCode),
            "claudeCode"
        );
    }

    #[test]
    fn public_provider_state_surfaces_codex_namespace_only() {
        let raw = serde_json::json!({
            "codex": { "settings": {
                "permissionMode": "never/workspace-write",
                "model": "gpt-5.5",
                "effort": "xhigh"
            } }
        });
        let public = public_session_provider_state(&raw, ChatAgentProvider::Codex);
        assert_eq!(
            public["codex"]["settings"]["permissionMode"],
            "never/workspace-write"
        );
        assert_eq!(public["codex"]["settings"]["model"], "gpt-5.5");
        assert_eq!(public["codex"]["settings"]["effort"], "xhigh");
        // A Codex session must NOT read the claudeCode namespace.
        let cross = serde_json::json!({ "claudeCode": { "settings": { "model": "claude" } } });
        assert_eq!(
            public_session_provider_state(&cross, ChatAgentProvider::Codex),
            serde_json::json!({})
        );
    }

    #[test]
    fn public_provider_state_claude_namespace_unchanged() {
        let raw = serde_json::json!({
            "claudeCode": { "settings": {
                "permissionMode": "bypassPermissions",
                "model": "claude-x",
                "effort": "high"
            } }
        });
        let public = public_session_provider_state(&raw, ChatAgentProvider::ClaudeCode);
        assert_eq!(
            public["claudeCode"]["settings"]["permissionMode"],
            "bypassPermissions"
        );
        assert_eq!(public["claudeCode"]["settings"]["model"], "claude-x");
        assert_eq!(public["claudeCode"]["settings"]["effort"], "high");
    }
}

/// The workbench part of a start request's identity, or `None` for an
/// ordinary start. `persona` and `configRevision` travel together; `actionId`
/// and the context need them.
fn workbench_identity(
    persona: Option<&str>,
    phase: Option<&str>,
    action_id: Option<String>,
    context: Option<&WorkbenchStartContextDTO>,
    config_revision: Option<&str>,
) -> Result<Option<WorkbenchIdentity>, AppError> {
    let invalid = |message: &str| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "persona", "message": message }),
        })
    };
    match (persona, config_revision) {
        (None, None) => {
            if action_id.is_some() || context.is_some() || phase.is_some() {
                return Err(invalid(
                    "actionId, phase and workbenchContext need persona and configRevision",
                ));
            }
            Ok(None)
        }
        (Some(persona), Some(config_revision)) => {
            if WorkbenchPhase::parse(persona).is_none() {
                return Err(invalid("must be one of be, se, ie"));
            }
            if config_revision.trim().is_empty() {
                return Err(invalid("configRevision must name the workbench revision"));
            }
            match (action_id.as_deref(), phase) {
                (Some(_), None) => {
                    return Err(invalid("actionId needs the phase it was clicked in"));
                }
                (_, Some(phase)) if WorkbenchPhase::parse(phase).is_none() => {
                    return Err(invalid("phase must be one of be, se, ie"));
                }
                _ => {}
            }
            let context = context
                .map(serde_json::to_value)
                .transpose()
                .map_err(|error| {
                    AppError::from_error(G5Error::Internal {
                        message: format!("workbench context: {error}"),
                    })
                })?;
            Ok(Some(WorkbenchIdentity {
                persona: persona.to_string(),
                phase: phase.map(str::to_string),
                action_id,
                context,
                config_revision: config_revision.to_string(),
            }))
        }
        _ => Err(invalid("persona and configRevision are required together")),
    }
}

/// Refuse a start whose browser-pinned revision no longer matches the
/// committed configuration (409 `CONFLICT`, `details.code =
/// WORKBENCH_CONFIG_STALE`): the browser refetches and the user retries.
fn ensure_workbench_revision(config: &WorkbenchConfigDTO, pinned: &str) -> Result<(), AppError> {
    if config.revision == pinned {
        return Ok(());
    }
    // The refusal body is not audited; the log is the only trace of which
    // revision a browser pinned against which one was current (issue #1142).
    tracing::warn!(
        requested_revision = pinned,
        current_revision = %config.revision,
        "workbench start refused: pinned configuration revision is stale"
    );
    Err(AppError::from_error(G5Error::ConflictDetails {
        reason: "The workbench configuration changed; reload it and try again".to_string(),
        details: serde_json::json!({
            "code": "WORKBENCH_CONFIG_STALE",
            "requestedRevision": pinned,
            "currentRevision": config.revision,
        }),
    }))
}

/// Resolve a persona start against the pinned configuration: the persona's
/// instructions and the action's template (or the user's own text) become
/// the first input (`g5.workbench.persona`; `generateChatPrompt.ts` rule).
fn persona_start_expansion(
    config: &WorkbenchConfigDTO,
    start: &WorkbenchIdentity,
    context: Option<&WorkbenchStartContextDTO>,
    user_text: &str,
) -> Result<PersonaExpansion, AppError> {
    ensure_workbench_revision(config, &start.config_revision)?;
    let phase = WorkbenchPhase::parse(&start.persona).ok_or_else(|| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "persona", "message": "must be one of be, se, ie" }),
        })
    })?;
    let instructions = config.personas.phase(phase).text.clone();
    let body = match start.action_id.as_deref() {
        Some(action_id) => {
            // Exactly the clicked phase's action: ids repeat across phases and
            // the persona may be overridden to another phase.
            let action_phase = start
                .phase
                .as_deref()
                .and_then(WorkbenchPhase::parse)
                .unwrap_or(phase);
            let action = config
                .actions
                .find_action_in(action_phase, action_id)
                .ok_or_else(|| {
                    AppError::from_error(G5Error::ValidationError {
                        details: serde_json::json!({
                            "field": "actionId",
                            "message": format!(
                                "unknown workbench action `{action_id}` in phase `{}`",
                                action_phase.as_str()
                            ),
                        }),
                    })
                })?;
            if action.kind != Some(WorkbenchActionKind::Chat) {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "actionId",
                        "message": format!("workbench action `{action_id}` does not start a chat"),
                    }),
                }));
            }
            workbench_action_message(
                action.prompt.as_deref().unwrap_or_default(),
                action_phase,
                context,
                user_text,
            )
        }
        None => {
            if user_text.trim().is_empty() {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "content",
                        "message": "a persona start without an action needs message text",
                    }),
                }));
            }
            user_text.trim().to_string()
        }
    };
    Ok(PersonaExpansion {
        persona: start.persona.clone(),
        content: persona_first_message(&instructions, &body),
        instructions,
    })
}
