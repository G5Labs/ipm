use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workspace — maps to the `workspaces` table (API_AND_STORAGE.md §2.2).
///
/// A workspace is the top-level unit of work in G5. It lives directly under an
/// org, declares which repos it touches via `WorkspaceRepositoryState`, and supports
/// cloning, dependency DAGs, and identity-based read access.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub is_base: bool,
    pub description: Option<String>,
    pub cloned_from_id: Option<Uuid>,
    pub docs_repo_id: Option<Uuid>,
    /// Nullable only during the P4.1→P4.5 fail-closed ownership migration.
    #[sqlx(default)]
    pub owner_user_id: Option<Uuid>,
    /// Fences Start/Resume consent and ownership/lifecycle replacement races.
    #[sqlx(default)]
    pub lifecycle_generation: i64,
    pub push_mode: String,
    /// Durable workspace mutation policy: `read_write` (default) or
    /// `read_only`. Source of truth for whether workspace-owned content may be
    /// mutated; distinct from `is_base` and live workspace file reviewer mode. See
    /// `read-only-workspaces-design.md` and models/workspace spec [I2b].
    pub mutation_policy: String,
    /// Transient read-write → read-only transition marker. `Some("freezing")`
    /// while admission is stopped and already-admitted agents drain; `None` in
    /// every steady state. The reverse transition is instantaneous. See
    /// `read-only-workspace-transition-design.md` and models/workspace [I2c].
    pub mutation_policy_transition: Option<String>,
    /// Actor of the last committed `mutation_policy` change (audit).
    pub mutation_policy_updated_by: Option<Uuid>,
    /// Time of the last committed `mutation_policy` change (audit).
    pub mutation_policy_updated_at: Option<DateTime<Utc>>,
    /// Merge destination override. NULL = merge to the fork parent
    /// (`cloned_from_id`); when set, merge flows prefer it. [D19]
    pub merge_target_workspace_id: Option<Uuid>,
    /// The user a personal workspace is bound to; `None` for Base and working
    /// workspaces. With `is_base` it stores the registry kind
    /// (`g5.workspace.kind`); read the kind through [`Workspace::kind`].
    #[sqlx(default)]
    pub personal_user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    /// The immutable registry kind (`g5.workspace.kind`).
    pub fn kind(&self) -> g5_contracts::types::WorkspaceKind {
        g5_contracts::types::WorkspaceKind::from_registry(self.is_base, self.personal_user_id)
    }
}

/// Request-scoped view of a canonical repository member. Identity and policy
/// come from workspace membership; the current version is observed in Git.
/// This is deliberately not a SQL row model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRepositoryState {
    pub workspace_id: Uuid,
    pub org_id: Uuid,
    pub repo_id: Uuid,
    pub branch_name: String,
    pub head_commit: String,
    pub tracking_mode: String,
    pub materialization_status: String,
}

/// WorkspaceSettings — maps to the `workspace_settings` table (API_AND_STORAGE.md §2.11).
///
/// Configures VM lifecycle, bounded execution and spend caps for a workspace.
/// Has a strict 1:1 relationship with `workspaces`
/// (the PK is `workspace_id`, not a surrogate UUID). One row is created
/// automatically when a workspace is created.
/// Concurrent agent and command turns a workspace admits unless its owner sets
/// another value (`g5.agents.admission.default-slots`). Equals the
/// `workspace_settings.bounded_execution_slots` column default (migration 070),
/// and the VM module catalog admits this many agent turns.
pub const DEFAULT_BOUNDED_EXECUTION_SLOTS: i32 = 16;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceSettings {
    pub workspace_id: Uuid,
    pub org_id: Uuid,
    pub max_spend_usd: Option<Decimal>,
    pub spend_period: String,
    pub spend_period_start: Option<DateTime<Utc>>,
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub idle_timeout_secs: i32,
    pub response_deadline_secs: i32,
    pub bounded_execution_slots: i32,
    pub permission_policy: Option<serde_json::Value>,
    /// The owner's opt-in for this workspace's agent to change other
    /// workspaces (`g5.security.agent-other-workspaces`).
    pub agent_may_modify_other_workspaces: bool,
    pub updated_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}
