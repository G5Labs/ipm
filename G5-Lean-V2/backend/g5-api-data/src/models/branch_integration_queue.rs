//! Branch Integration Queue model — pending integration cycles.
//!
//! Maps to `remote_integration_queue`, keyed by `(workspace_id, repo_id)`.
//! Commit bursts and background ticks collapse into one pending pickup.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::repositories::branch_integration_queue::{
    enqueue_authorized_branch_integration, enqueue_branch_integration,
    enqueue_branch_integration_at, enqueue_branch_integration_for_advance,
    enqueue_branch_integration_for_advance_with_trigger, enqueue_branch_integration_for_workspace,
    PushAuthority, WorkspaceEnqueueConflict,
};

/// String constants for the `trigger_kind` column.
///
/// The queue subset is narrower than `remote_sync_runs.trigger_kind`:
/// only triggers that produce queue rows. Advisory pings and webhook
/// receipts do not enqueue — they either write the advisory directly
/// (webhook) or refresh the advisory inline (ping).
pub mod trigger_kind {
    pub const COMMIT: &str = "commit";
    pub const MANUAL: &str = "manual";
    pub const WORKSPACE_OPEN: &str = "workspace_open";
    pub const PUSH: &str = "push";
    pub const APPROVED_REBASE_PUBLISH: &str = "approved_rebase_publish";
    pub const BACKGROUND_INBOUND: &str = "background_inbound";
}

#[cfg(test)]
mod tests {
    use super::trigger_kind;

    #[test]
    fn queue_trigger_constants_match_the_database_contract() {
        assert_eq!(trigger_kind::COMMIT, "commit");
        assert_eq!(trigger_kind::MANUAL, "manual");
        assert_eq!(trigger_kind::WORKSPACE_OPEN, "workspace_open");
        assert_eq!(trigger_kind::PUSH, "push");
        assert_eq!(
            trigger_kind::APPROVED_REBASE_PUBLISH,
            "approved_rebase_publish"
        );
        assert_eq!(trigger_kind::BACKGROUND_INBOUND, "background_inbound");
    }
}

/// `remote_integration_queue` row — passive struct, sqlx-mapped.
///
/// Each `remote_integrations` policy has AT MOST one
/// queue row at a time (PK constraint). UPSERTs preserve the latest
/// `requested_at` and `trigger_kind`. Explicit `push` wins over every
/// implicit trigger so a user publication gesture is never demoted; commit
/// follows explicit manual fetch, then outranks passive polling. This preserves
/// the user's inbound intent even when automatic pulls are disabled.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BranchIntegrationQueueRow {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub org_id: Uuid,
    pub requested_at: DateTime<Utc>,
    /// One of `trigger_kind::*`.
    pub trigger_kind: String,
    pub attempts: i32,
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// Free-text error context from the most recent failed attempt. MUST
    /// be passed through the credential scrubber before persistence per
    /// auth_identity_access.security_view.md D16.
    pub last_error: Option<String>,
    /// Worker instance UUID currently holding this row (NULL when idle).
    pub locked_by: Option<Uuid>,
    /// Lease expiry. On worker crash, expiry returns the row to the
    /// queue without manual intervention.
    pub locked_until: Option<DateTime<Utc>>,
    /// Committed authority advance that most recently requested this projection.
    /// `None` identifies manual, workspace-open, and legacy queue work.
    pub advance_id: Option<Uuid>,
    pub requested_by_user_id: Option<Uuid>,
    pub lifecycle_generation: Option<i64>,
}
