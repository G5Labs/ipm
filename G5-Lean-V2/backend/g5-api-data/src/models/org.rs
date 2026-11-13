use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Organization entity — the top-level grouping that owns repos, workspaces, and members.
///
/// Maps to the `orgs` table (API_AND_STORAGE.md §2.1).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Org {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

/// Pending invitation to join an organization, supporting a state machine of
/// `pending → accepted | declined | expired`.
///
/// Maps to the `org_invitations` table (API_AND_STORAGE.md §2.1).
/// Role is stored as `String`; valid values are `'admin'`, `'member'`, `'viewer'`
/// (note: `'owner'` is NOT a valid invitation role).
/// Status is stored as `String`; valid values are `'pending'`, `'accepted'`, `'declined'`, `'expired'`.
/// The service layer enforces validation and state transitions.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrgInvitation {
    pub id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub role: String,
    pub invited_by: Uuid,
    pub status: String,
    pub accepted_by: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// VM idle timeout a new workspace starts with while its organization has no
/// administrator-set default (`g5.lifecycle.idle.settings`). Equals the
/// `workspace_settings.idle_timeout_secs` column default (migration 072).
pub const DEFAULT_IDLE_TIMEOUT_SECS: i32 = 28_800;

/// Idle timeouts a workspace or organization default may hold, matching the
/// `workspace_settings.idle_timeout_secs` CHECK.
pub const IDLE_TIMEOUT_SECS_RANGE: std::ops::RangeInclusive<i64> = 60..=86_400;

/// Organization VM quota while its administrators have set none
/// (`g5.runtime.placement.capacity`).
pub const DEFAULT_MAX_RUNNING_VMS: i64 = 10;

/// VM quotas an administrator may set (`g5.runtime.placement.capacity`).
pub const MAX_RUNNING_VMS_RANGE: std::ops::RangeInclusive<i64> = 0..=100;

/// How long a managed cloud host stays empty before automatic removal while
/// its organization has set no delay (`g5.architecture.host_idle_retirement`).
pub const DEFAULT_IDLE_HOST_RETIRE_SECS: i32 = 3_600;

/// Removal delays an administrator may set.
pub const IDLE_HOST_RETIRE_SECS_RANGE: std::ops::RangeInclusive<i64> = 900..=86_400;

/// Free current slots managed capacity keeps ready in the organization's
/// demand region while its administrators have set no floor
/// (`g5.architecture.host_demand_capacity`).
pub const DEFAULT_MIN_FREE_SLOTS: i32 = 1;

/// Free-slot floors an administrator may set; 0 turns the floor off.
pub const MIN_FREE_SLOTS_RANGE: std::ops::RangeInclusive<i64> = 0..=100;

/// Starting, running and stopping VMs, plus earlier waiting Starts, one owner
/// may hold while the organization has set no per-owner limit
/// (`g5.runtime.placement.owner-limit`).
pub const DEFAULT_MAX_RUNNING_VMS_PER_OWNER: i64 = 5;

/// Per-owner VM limits an administrator may set; 0 refuses every Start.
pub const MAX_RUNNING_VMS_PER_OWNER_RANGE: std::ops::RangeInclusive<i64> = 0..=100;

/// Live workspaces, plus pending forks and transfers to them, one owner may
/// hold while the organization has set no per-owner limit
/// (`g5.workspace.owner-limit`).
pub const DEFAULT_MAX_WORKSPACES_PER_OWNER: i64 = 50;

/// Per-owner workspace limits an administrator may set; 0 refuses every new
/// workspace.
pub const MAX_WORKSPACES_PER_OWNER_RANGE: std::ops::RangeInclusive<i64> = 0..=1_000;
