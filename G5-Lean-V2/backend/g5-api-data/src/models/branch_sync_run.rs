//! Branch Sync Run model — one row per integration attempt or advisory ping.
//!
//! Maps to the `remote_sync_runs` table (created in migration
//! the baseline schema, originally migration 114). This is the audit trail for the
//! remote-repo-tracking integration pipeline.
//!
//! See ontology: `g5/g5-api/models/branch_sync_run` (Phase 1).
//! See arch view: `views/arch_views/remote_repo_tracking.arch_view.md` D29.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// String constants for the `kind` column.
pub mod kind {
    /// Integration cycle (fetch + classify + act).
    pub const INTEGRATION: &str = "integration";
    /// Advisory ping (cheap `ls-remote`, advisory columns only).
    pub const ADVISORY: &str = "advisory";
}

/// String constants for the `trigger_kind` column.
///
/// Mirrors the CHECK constraint values on `remote_sync_runs.trigger_kind`.
pub mod trigger_kind {
    /// Workspace-side commit (worker commit, draft commit, merge completion).
    pub const COMMIT: &str = "commit";
    /// Operator-invoked `POST /workspaces/{id}/sync`.
    pub const MANUAL: &str = "manual";
    /// Workspace-open hook.
    pub const WORKSPACE_OPEN: &str = "workspace_open";
    /// Advisory pinger tick on a dirty branch.
    pub const PING: &str = "ping";
    /// Explicit PushToRemote action (publishes regardless of push_mode).
    pub const PUSH: &str = "push";
    /// Approved reset-below-baseline candidate linearization and publication.
    pub const APPROVED_REBASE_PUBLISH: &str = "approved_rebase_publish";
    /// SCM webhook receipt (Phase B).
    pub const WEBHOOK: &str = "webhook";
    /// Background inbound scheduler tick on a clean tracked branch.
    pub const BACKGROUND_INBOUND: &str = "background_inbound";
}

/// String constants for the `status` column.
///
/// Mirrors the CHECK constraint values. The integration worker records one
/// of these per cycle; a background cycle that repeats the latest run's
/// observation extends that run instead (`g5.governance.remote.sync-history`).
pub mod status {
    /// Tips are equal; no action taken.
    pub const NOOP: &str = "noop";
    /// Mirror was ahead; workspace head advanced via fast-forward.
    pub const IMPORTED: &str = "imported";
    /// Provider objects and refs refreshed; workspace files are unchanged.
    pub const FETCHED: &str = "fetched";
    /// Workspace was ahead; push to customer remote succeeded.
    pub const PUSHED: &str = "pushed";
    /// True divergence (or watermark violation); merge run opened.
    pub const DIVERGED: &str = "diverged";
    /// Workspace dirty at lock time; cycle short-circuited.
    pub const DIRTY_SKIPPED: &str = "dirty_skipped";
    /// Advisory ping changed `remote_tip_advisory_sha`.
    pub const ADVISORY_UPDATED: &str = "advisory_updated";
    /// No usable token (owner not org member, no SCM connection, decrypt failure).
    pub const AUTH_REQUIRED: &str = "auth_required";
    /// Push attempt failed (non-fast-forward retry handled separately).
    pub const PUSH_FAILED: &str = "push_failed";
    /// Fetch attempt failed (network, auth, branch deleted upstream).
    pub const FETCH_FAILED: &str = "fetch_failed";
    /// An open divergence run completed automatically after its proven
    /// resolution landed; tracking resumed.
    pub const RESOLVED: &str = "resolved";
}

/// `remote_sync_runs` row — passive struct, sqlx-mapped.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BranchSyncRun {
    pub id: Uuid,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    /// User whose SCM token was used on this cycle. Required for
    /// forensic attribution per auth_identity_access.security_view.md D13.
    pub acting_user_id: Uuid,
    /// One of `kind::{INTEGRATION, ADVISORY}`.
    pub kind: String,
    /// One of `trigger_kind::*`.
    pub trigger_kind: String,
    /// One of `status::*`.
    pub status: String,
    pub old_workspace_tip: Option<String>,
    pub new_workspace_tip: Option<String>,
    pub old_mirror_tip: Option<String>,
    pub new_mirror_tip: Option<String>,
    pub new_baseline: Option<String>,
    /// Free-text error context. MUST be passed through the credential
    /// scrubber before persistence per auth_identity_access D16.
    pub error_detail: Option<String>,
    pub merge_run_id: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
