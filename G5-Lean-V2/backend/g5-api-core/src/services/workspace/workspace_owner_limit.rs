//! Per-owner workspace limit (`g5.workspace.owner-limit`).
//!
//! An owner's count is their live non-Base workspaces in the organization,
//! plus their pending forks and pending ownership transfers to them. Fork and
//! transfer admission take [`lock_owner`] before counting, and before any
//! workspace row lock, so competing admissions for one owner are serialized
//! without deadlocking each other.

use g5_contracts::error::{G5Error, OwnerLimitKind};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::services::org_admin_settings;

/// Serialize workspace-limit admissions for one user. Keyed by the user, not
/// the organization: transfer admission takes it before it has read the
/// workspace's organization.
pub(crate) async fn lock_owner(conn: &mut PgConnection, user_id: Uuid) -> Result<(), G5Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("g5.workspace-owner-limit:{user_id}"))
        .execute(conn)
        .await
        .map(|_| ())
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to lock owner workspace limit: {error}"),
        })
}

/// Refuse when `owner` already holds the organization's per-owner limit.
/// The caller holds [`lock_owner`] for `owner` until it commits the new claim.
pub(crate) async fn ensure_below_limit(
    conn: &mut PgConnection,
    org_id: Uuid,
    owner: Uuid,
) -> Result<(), G5Error> {
    let limit = org_admin_settings::max_workspaces_per_owner(&mut *conn, org_id)
        .await?
        .max(0);
    let owned = sqlx::query_scalar::<_, i64>(
        "SELECT
             (SELECT count(*) FROM workspaces
              WHERE org_id = $1 AND owner_user_id = $2
                AND deleted_at IS NULL AND NOT is_base)
           + (SELECT count(*) FROM workspace_forks
              WHERE org_id = $1 AND owner_user_id = $2 AND status = 'pending')
           + (SELECT count(*) FROM workspace_ownership_transfers
              WHERE org_id = $1 AND to_user_id = $2 AND status = 'pending')",
    )
    .bind(org_id)
    .bind(owner)
    .fetch_one(conn)
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Failed to count owned workspaces: {error}"),
    })?;
    if owned >= limit {
        return Err(G5Error::OwnerLimitReached {
            kind: OwnerLimitKind::Workspaces,
            limit,
        });
    }
    Ok(())
}
