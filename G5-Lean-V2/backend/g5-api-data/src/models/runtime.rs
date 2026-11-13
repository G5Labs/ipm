//! Passive row models for workspace runtime configuration and execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Config {
    pub org_id: Uuid,
    pub scope: String,
    pub scope_id: Uuid,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

/// Additive P4.1 settings projection. The retained `WorkspaceSettings` model
/// intentionally stays source-compatible with pre-cutover query macros and
/// DTOs until the owning producers move to these typed authorities.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RuntimeWorkspaceSettings {
    pub workspace_id: Uuid,
    pub org_id: Uuid,
    pub idle_timeout_secs: i32,
    pub response_deadline_secs: i32,
    pub bounded_execution_slots: i32,
    pub permission_policy: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceRepo {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub root_path: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceOwnershipTransfer {
    pub id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub requested_by: Uuid,
    pub idempotency_key: String,
    pub request_digest: String,
    pub requested_at: DateTime<Utc>,
    pub status: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceFork {
    pub id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Option<Uuid>,
    pub requested_by: Uuid,
    pub owner_user_id: Uuid,
    pub idempotency_key: String,
    pub request_digest: String,
    pub proposed_name: String,
    pub status: String,
    pub repo_manifest: serde_json::Value,
    pub source_flush_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceVm {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub host_id: Option<Uuid>,
    pub vm_id: Option<String>,
    pub epoch: i64,
    pub status: String,
    pub image_ref: Option<String>,
    pub image_digest: Option<String>,
    pub image_source: Option<serde_json::Value>,
    pub image_oid_vector: Option<serde_json::Value>,
    pub module_catalog_version: Option<String>,
    pub boot_spec: Option<serde_json::Value>,
    pub source_manifest_digest: Option<String>,
    pub lease_sequence: i64,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub start_deadline_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub stop_reason: Option<String>,
    pub stop_operation_id: Option<Uuid>,
    pub stop_generation: Option<i64>,
    pub stop_retry_at: Option<DateTime<Utc>>,
    pub stop_deadline_at: Option<DateTime<Utc>>,
    pub pending_stop_reason: Option<String>,
    pub pending_stop_deadline_at: Option<DateTime<Utc>>,
    pub nudged_at: Option<DateTime<Utc>>,
    pub start_intent: Option<serde_json::Value>,
    pub preview_generation: i64,
    pub preview_intent: Option<serde_json::Value>,
    pub idle_retry_after: Option<DateTime<Utc>>,
    pub last_flush_id: Option<Uuid>,
    pub unflushed_loss_at: Option<DateTime<Utc>>,
    pub session_harvest_loss_at: Option<DateTime<Utc>>,
    pub last_error: Option<serde_json::Value>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkerVmEvent {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub host_id: Uuid,
    pub vm_id: String,
    pub seq: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RuntimeTask {
    pub id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub task_type: String,
    pub kind: Option<String>,
    pub status: String,
    pub spec: serde_json::Value,
    pub outcome: Option<serde_json::Value>,
    pub attempt: i32,
    pub vm_id: Option<String>,
    pub epoch: Option<i64>,
    pub start_confirmed: bool,
    pub chat_session_id: Option<Uuid>,
    pub continuation_reason: Option<String>,
    pub hold_reason: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskAttempt {
    pub org_id: Uuid,
    pub task_id: Uuid,
    pub attempt: i32,
    pub vm_id: Option<String>,
    pub epoch: Option<i64>,
    pub start_confirmed: bool,
    pub policy_snapshot_ref: Option<serde_json::Value>,
    pub session_files_state: String,
    pub session_manifest_generation: i32,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub outcome: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub acked_event_seq: i64,
}

/// One dependency edge: `task_id` waits for `depends_on_task_id`. This table
/// is the only dependency authority (`g5.agents.dependencies`): task readers
/// derive a task's `depends_on` list from it, and allocation leaves the legacy
/// `tasks.depends_on` array empty. A migration-003 trigger still copies edges
/// from that array when it changes, skipping any edge that would close a
/// cycle; no production statement changes it.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskDependency {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub depends_on_task_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskEvent {
    pub id: Uuid,
    pub org_id: Uuid,
    pub task_id: Uuid,
    pub attempt: Option<i32>,
    pub vm_id: Option<String>,
    pub seq: Option<i64>,
    pub entry_type: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProviderRequest {
    pub id: Uuid,
    pub org_id: Uuid,
    pub task_id: Uuid,
    pub attempt: i32,
    pub provider_request_id: String,
    pub kind: String,
    pub tool_name: Option<String>,
    pub payload: serde_json::Value,
    pub display: serde_json::Value,
    pub status: String,
    pub response: Option<serde_json::Value>,
    pub response_author_id: Option<Uuid>,
    pub response_deadline_at: Option<DateTime<Utc>>,
    pub resume_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SessionInput {
    pub id: Uuid,
    pub org_id: Uuid,
    pub session_id: Uuid,
    pub input_index: i64,
    pub content: String,
    pub artifacts: serde_json::Value,
    pub revision: i32,
    pub input_origin: String,
    pub template_ref: Option<String>,
    pub template_revision: Option<String>,
    pub delivery: String,
    pub reservation_id: Option<Uuid>,
    pub reservation_revision: Option<i32>,
    pub delivery_generation: i32,
    pub task_id: Option<Uuid>,
    pub attempt: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub withdrawn_at: Option<DateTime<Utc>>,
    pub delivery_audit: serde_json::Value,
}
