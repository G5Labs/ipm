//! Request/response DTO types for the `/api/internal/*` endpoints.
//!
//! Both `g5d` and `g5-api` import these types from `g5-internal-api` for
//! type-safe serialization. DTOs always use `#[serde(rename_all = "camelCase")]`
//! matching the g5-api JSON convention.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Worker-attributed platform issue reporting
// ---------------------------------------------------------------------------

/// Typed context accepted by the worker-only platform issue intake endpoint.
/// It deliberately carries no reporter identity or authorization material:
/// the API resolves both server-side from the worker's active task/session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InternalPlatformIssueContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breadcrumbs: Vec<String>,
}

/// Shared worker-attributed platform issue request body.
///
/// This is intentionally separate from the public multipart request: the
/// worker path accepts no screenshot and rejects all unknown fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InternalPlatformIssueReportRequest {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub severity: String,
    #[serde(default)]
    pub context: InternalPlatformIssueContext,
}

/// Repository-scoped identity and immutable content evidence retained in
/// historical task outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResultArtifactRef {
    pub repository: String,
    pub repo_relative_path: String,
    pub sha256: String,
    pub byte_length: u64,
}

/// Historical per-repository publication result used by approval audit
/// readers. New agent work pushes the workspace branch directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitBranchResult {
    pub branch_id: String,
    pub repo_name: String,
    pub branch_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Usage Reporting
// ---------------------------------------------------------------------------

fn default_usage_request_type() -> String {
    "chat".to_string()
}

/// POST /api/internal/usage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecordRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_creation_tokens: i64,
    #[serde(default)]
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    #[serde(default = "default_usage_request_type")]
    pub request_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i32>,
}

/// Response from POST /api/internal/usage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageAcknowledgement {
    pub inserted: i64,
}

// ---------------------------------------------------------------------------
// Provider Quota Snapshot Reporting
// ---------------------------------------------------------------------------

/// Provider id component of the provider-quota snapshot key stamped by the
/// historical quota producer and read back by the session/workspace display
/// endpoints. Shared here so the upsert/read key pair cannot drift
/// (external_agent_provider_contract D13).
pub const PROVIDER_QUOTA_PROVIDER_CLAUDE_CODE: &str = "claude_code";

/// Auth-mode component of the provider-quota snapshot key (the Anthropic
/// OAuth/subscription mode). Paired with
/// [`PROVIDER_QUOTA_PROVIDER_CLAUDE_CODE`]; see that constant for why the key
/// lives in the shared wire-contract crate.
pub const PROVIDER_QUOTA_AUTH_MODE_ANTHROPIC_OAUTH: &str = "anthropic_oauth";

/// Maximum accepted byte length for the `provider` / `auth_mode` key
/// components and the free-text `representative_claim` / `unified_status`
/// fields (measured with `str::len`, i.e. UTF-8 bytes — matching the server
/// validator). Producers must truncate below this bound before reporting;
/// the server rejects longer values with a 400.
pub const PROVIDER_QUOTA_TEXT_MAX_LEN: usize = 200;

/// `POST /api/internal/provider-quota`
///
/// Passive, display-only provider subscription-quota snapshot. The native
/// runtime currently has no collector; stored snapshots remain readable
/// (system_invariants/external_agent_provider_contract D13). This is
/// provider-owned budget evidence: it never gates dispatch, never joins the
/// dollar/token cost-accounting `usage_records` path, and never carries
/// credential material.
///
/// It mirrors [`UsageRecordRequest`]'s execution-identity carrier: `task_id`
/// (required — a report without a valid task UUID is rejected with a 400;
/// `#[serde(default)]` keeps a missing wire field a typed 400 rather than a
/// deserialize rejection) and optional `session_id`. Worker identity is not
/// carried on the wire — it is derived server-side from the authenticated
/// worker session — and `org_id`/`workspace_id` are resolved server-side from
/// the task/session, never trusted from the client. `provider` and `auth_mode`
/// complete the latest-wins upsert key `(org, workspace, provider, auth_mode)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaReportRequest {
    #[serde(default)]
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Provider id (e.g. `"claude_code"`) — part of the snapshot key.
    pub provider: String,
    /// Frozen auth mode (e.g. the Anthropic OAuth/subscription mode) — part of
    /// the snapshot key so the same provider under a different auth mode keeps a
    /// separate current snapshot.
    pub auth_mode: String,
    /// Five-hour window utilization as a fraction in `0.0..=1.0`. Absent when
    /// the header was missing or unparseable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_utilization: Option<f32>,
    /// When the five-hour window resets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    /// Seven-day window utilization as a fraction in `0.0..=1.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_utilization: Option<f32>,
    /// When the seven-day window resets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_reset_at: Option<DateTime<Utc>>,
    /// Provider-supplied representative claim string, sanitized/truncated by the
    /// reporter before it reaches this endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub representative_claim: Option<String>,
    /// Provider-supplied unified status string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified_status: Option<String>,
    /// Capture timestamp stamped by the reporting proxy. This is the latest-wins
    /// ordering key: a report whose `captured_at` is not strictly newer than the
    /// stored snapshot is ignored.
    pub captured_at: DateTime<Utc>,
}

/// Response from `POST /api/internal/provider-quota`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuotaAcknowledgement {
    /// `true` when the snapshot was stored (first insert or a strictly-newer
    /// replacement); `false` when the report was ignored as not newer than the
    /// stored snapshot.
    pub stored: bool,
}

// ---------------------------------------------------------------------------
// Provider session files — `chat_session_files` (design v6 §8.4). The bytes
// travel zstd-compressed outside these DTOs; see `crate::session_files`.
// ---------------------------------------------------------------------------

/// One stored provider session file, without its bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionFileManifestItem {
    pub provider: crate::session_files::SessionFileProvider,
    pub provider_session_id: String,
    /// Relative to the provider home (`projects/...` or `sessions/...`).
    pub relative_path: String,
    pub sha256: String,
    pub size: i64,
    pub updated_at: DateTime<Utc>,
}

/// GET /api/internal/chat-sessions/:sessionId/session-files/manifest
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionFilesManifestResponse {
    pub files: Vec<ChatSessionFileManifestItem>,
}

/// One file named by a complete provider-session publication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublishedSessionFile {
    pub relative_path: String,
    pub sha256: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileBundlePublication {
    pub generation: uuid::Uuid,
    pub provider: crate::session_files::SessionFileProvider,
    pub provider_session_id: String,
    pub files: Vec<PublishedSessionFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileBundleBegin {
    pub generation: uuid::Uuid,
    pub task_id: uuid::Uuid,
    pub attempt: i32,
    pub reservation_id: uuid::Uuid,
    /// Workspace-relative working directory used for Claude's project key.
    pub cwd_relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingSessionFileBundle {
    pub session_id: uuid::Uuid,
    pub generation: uuid::Uuid,
    pub provider: crate::session_files::SessionFileProvider,
    pub provider_session_id: String,
    pub cwd_relative_path: String,
}

/// PUT /api/internal/chat-sessions/:sessionId/session-files
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PutChatSessionFileResponse {
    pub provider: crate::session_files::SessionFileProvider,
    pub provider_session_id: String,
    pub relative_path: String,
    pub sha256: String,
    pub size: i64,
    /// `false` when the stored file already held these bytes (a retried or
    /// unchanged upload); `updated_at` is then the earlier write's.
    pub changed: bool,
    pub updated_at: DateTime<Utc>,
}

/// GET /api/internal/chat-attachments/:attachmentId/content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachmentContentResponse {
    pub attachment_id: String,
    pub session_id: String,
    #[serde(default)]
    pub input_id: Option<String>,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_base64: String,
}

/// POST /api/internal/chat-sessions/:sessionId/attachments (routes/internal
/// D35): worker-owned agent-screenshot upload, persisted per chat_service D63
/// (`created_by` NULL, assistant-message binding, `agent-screenshots/`
/// storage-key prefix).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAgentChatAttachmentRequest {
    pub message_id: String,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub content_base64: String,
}

/// Response for POST /api/internal/chat-sessions/:sessionId/attachments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAgentChatAttachmentResponse {
    pub attachment_id: String,
    pub sha256: String,
    pub size_bytes: i64,
}

// ---------------------------------------------------------------------------
// View workflow DTOs
// ---------------------------------------------------------------------------

/// PUT /api/internal/docs/:id/content
/// Worker asks the API to refresh indexed doc metadata after task completion.
/// The payload content is used only to verify the current repo-backed file and
/// recompute projection fields such as hashes and linked-doc metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocContentRequest {
    pub content: String,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_file_manifest_contract_round_trips() {
        let response = ChatSessionFilesManifestResponse { files: vec![] };
        let encoded = serde_json::to_value(&response).unwrap();
        assert_eq!(encoded, serde_json::json!({"files": []}));
        let decoded: ChatSessionFilesManifestResponse = serde_json::from_value(encoded).unwrap();
        assert!(decoded.files.is_empty());
    }
}
