//! Typed control-plane contract for lifecycle-owned workspace flushes.
//!
//! These messages cross the dispatcher/API boundary. They deliberately carry
//! repository identities and Git proofs, never filesystem paths supplied by
//! the control plane or credentials returned to the guest.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlushReason {
    Shutdown,
    Fork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequencerState {
    None,
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushStartRequest {
    pub reason: FlushReason,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushStartResponse {
    pub flush_id: Uuid,
    pub workspace_id: Uuid,
    pub vm_id: Uuid,
    pub epoch: u64,
    pub reason: FlushReason,
    pub required_repo_ids: Vec<Uuid>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushRepoReceipt {
    pub repo_id: Uuid,
    pub branch_oid: String,
    pub local_branch_oid: String,
    pub local_head_oid: String,
    pub head_ref: Option<String>,
    pub sequencer: SequencerState,
    pub snapshot_oid: String,
    pub dirty: bool,
    /// Paths whose bytes in `snapshot_oid` differ from `local_head_oid`.
    /// Older receipts predate this projection, so deserialization treats the
    /// missing field as an unknown/empty set while retaining `dirty`.
    #[serde(default)]
    pub dirty_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushCompleteRequest {
    pub repos: Vec<FlushRepoReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushCompleteResponse {
    pub flush_id: Uuid,
    pub complete_at: DateTime<Utc>,
    pub repos: Vec<FlushRepoReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushLimit {
    pub per_file_bytes: u64,
    pub tree_bytes: u64,
    pub pack_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushOffender {
    pub repo_id: Uuid,
    pub path: Option<String>,
    pub bytes: u64,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushFailureRequest {
    pub code: String,
    pub fingerprint: String,
    pub limits: FlushLimit,
    #[serde(default)]
    pub offenders: Vec<FlushOffender>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlushFailureResponse {
    pub flush_id: Uuid,
    pub blocked_since: DateTime<Utc>,
}
