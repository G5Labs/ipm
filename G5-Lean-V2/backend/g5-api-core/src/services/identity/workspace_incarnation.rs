//! Whether a workspace VM token is still current: the one reader of
//! `g5.security.admission`'s owner/VM/epoch/lease check, held by the
//! workspace lifecycle, which owns VM custody.
//!
//! A token names an organization, owner, VM and epoch. It is current while the
//! workspace is live and still owned by that owner, and the workspace's VM is
//! that VM and epoch, in custody (`starting`, `running` or `stopping`), with a
//! lease that has not expired. Callers add their own rules on top: a branch
//! push needs a running VM, a bootstrap graph no pending transfer.

use g5_auth::WorkspaceIncarnationClaims;
use g5_authz::ScopedDb;
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// Which rows the read locks, always the workspace row before the VM row,
/// the lifecycle's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncarnationLock {
    /// None beyond what the caller's transaction already holds. On the system
    /// pool the authority lookup still takes the workspace row `FOR SHARE`.
    None,
    /// The workspace row and the VM row `FOR SHARE`.
    Share,
    /// The workspace row `FOR SHARE` and the VM row `FOR UPDATE`.
    Update,
}

/// The current incarnation a token names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveIncarnation {
    pub org_id: Uuid,
    pub owner_user_id: Uuid,
    /// `None` for a Base workspace, which never starts a VM.
    pub branch_name: Option<String>,
    /// `starting`, `running` or `stopping`.
    pub status: String,
    pub pending_transfer: bool,
    /// The workspace's repositories, in id order.
    pub repo_ids: Vec<Uuid>,
}

/// The rule, over the workspace authority `w` and the VM row `vm`, with the
/// token bound as `$2` organization, `$3` owner, `$4` VM and `$5` epoch.
const CURRENT: &str = "w.deleted_at IS NULL AND w.org_id = $2 AND w.owner_user_id = $3
           AND vm.vm_id = $4 AND vm.epoch = $5
           AND vm.status IN ('starting', 'running', 'stopping')
           AND vm.lease_expires_at > clock_timestamp()";

/// The token's incarnation if it is current, read on a connection scoped to
/// the token's organization.
pub async fn current_incarnation(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    lock: IncarnationLock,
) -> Result<Option<LiveIncarnation>, sqlx::Error> {
    let workspace_lock = match lock {
        IncarnationLock::None => "",
        IncarnationLock::Share | IncarnationLock::Update => "FOR SHARE",
    };
    // The columns of `authz.lookup_workspace_runtime_authority`, which only
    // the system role may call.
    let authority = format!(
        "(SELECT workspace.org_id, workspace.id AS workspace_id, workspace.owner_user_id,
                 CASE WHEN workspace.is_base THEN NULL ELSE workspace.branch_name END AS branch_name,
                 workspace.deleted_at,
                 EXISTS (SELECT 1 FROM workspace_ownership_transfers transfer
                         WHERE transfer.workspace_id = workspace.id
                           AND transfer.status = 'pending') AS pending_transfer
          FROM workspaces workspace WHERE workspace.id = $1 {workspace_lock})"
    );
    read(db.conn(), &authority, claims, lock).await
}

/// The token's incarnation if it is current, read on the system pool through
/// the workspace authority lookup.
pub async fn current_incarnation_on_system(
    conn: &mut PgConnection,
    claims: &WorkspaceIncarnationClaims,
    lock: IncarnationLock,
) -> Result<Option<LiveIncarnation>, sqlx::Error> {
    read(
        conn,
        "authz.lookup_workspace_runtime_authority($1)",
        claims,
        lock,
    )
    .await
}

async fn read(
    conn: &mut PgConnection,
    authority: &str,
    claims: &WorkspaceIncarnationClaims,
    lock: IncarnationLock,
) -> Result<Option<LiveIncarnation>, sqlx::Error> {
    let live = read_once(conn, authority, claims, lock).await?;
    if lock == IncarnationLock::None || live.is_none() {
        return Ok(live);
    }
    // PostgreSQL rechecks a row it waited for only if the row changed, so a
    // lease that ran out during the wait is caught by reading again with the
    // rows locked.
    read_once(conn, authority, claims, IncarnationLock::None).await
}

async fn read_once(
    conn: &mut PgConnection,
    authority: &str,
    claims: &WorkspaceIncarnationClaims,
    lock: IncarnationLock,
) -> Result<Option<LiveIncarnation>, sqlx::Error> {
    let vm_lock = match lock {
        IncarnationLock::None => "",
        IncarnationLock::Share => "FOR SHARE OF vm",
        IncarnationLock::Update => "FOR UPDATE OF vm",
    };
    let Ok(epoch) = i64::try_from(claims.epoch) else {
        return Ok(None);
    };
    let row = sqlx::query(&format!(
        "SELECT w.org_id, w.owner_user_id, w.branch_name, w.pending_transfer, vm.status,
                ARRAY(SELECT member.repo_id FROM workspace_repos member
                      WHERE member.org_id = w.org_id AND member.workspace_id = w.workspace_id
                      ORDER BY member.repo_id) AS repo_ids
         FROM {authority} AS w
         JOIN workspace_vms AS vm
           ON vm.org_id = w.org_id AND vm.workspace_id = w.workspace_id
         WHERE {CURRENT}
         {vm_lock}"
    ))
    .bind(claims.workspace_id)
    .bind(claims.org_id)
    .bind(claims.owner_user_id)
    .bind(claims.vm_id.to_string())
    .bind(epoch)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|row| {
        Ok(LiveIncarnation {
            org_id: row.try_get("org_id")?,
            owner_user_id: row.try_get("owner_user_id")?,
            branch_name: row.try_get("branch_name")?,
            status: row.try_get("status")?,
            pending_transfer: row.try_get("pending_transfer")?,
            repo_ids: row.try_get("repo_ids")?,
        })
    })
    .transpose()
}

/// Whether a current personal-workspace incarnation may read `repo_id` as a
/// reference checkout: a current member of its organization's Base
/// (`g5.workspace.identity.reference`). Rechecked on every request, so it
/// follows Base's current members. Reads only; pushes stay confined to the
/// incarnation's own members. `db` is scoped to the token's organization.
pub async fn personal_reference_readable(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let Ok(epoch) = i64::try_from(claims.epoch) else {
        return Ok(false);
    };
    sqlx::query_scalar::<_, bool>(&format!(
        "SELECT EXISTS (
             SELECT 1 FROM workspaces AS w
             JOIN workspace_vms AS vm ON vm.org_id = w.org_id AND vm.workspace_id = w.id
             WHERE w.id = $1 AND w.personal_user_id IS NOT NULL AND {CURRENT})
         AND EXISTS (
             SELECT 1 FROM workspace_repos AS member
             JOIN workspaces AS base
               ON base.id = member.workspace_id AND base.org_id = member.org_id
             WHERE base.org_id = $2 AND base.is_base AND base.deleted_at IS NULL
               AND member.repo_id = $6)"
    ))
    .bind(claims.workspace_id)
    .bind(claims.org_id)
    .bind(claims.owner_user_id)
    .bind(claims.vm_id.to_string())
    .bind(epoch)
    .bind(repo_id)
    .fetch_one(db.conn())
    .await
}

/// The owner's personal workspace a current incarnation reaches through its
/// mount (`g5.personal.mount`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalMountTarget {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    /// The personal workspace's branch, the only ref a mount push may move.
    pub branch_name: String,
}

/// Whether a current incarnation of a workspace that is neither personal nor
/// a formal review workspace reaches `repo_id` as its owner's mounted
/// personal repository: the member of the live personal workspace bound to
/// the incarnation workspace's current owner, in the same organization
/// (`g5.personal.mount`). Rechecked on every request, so retirement,
/// transfer or a newer incarnation ends it at once. `db` is scoped to the
/// token's organization.
pub async fn personal_mount_target(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<Option<PersonalMountTarget>, sqlx::Error> {
    personal_mount_target_on(db.conn(), claims, repo_id).await
}

/// [`personal_mount_target`] in its own read, scoped to the token's
/// organization, which the rule then fences against the current workspace,
/// owner, VM and epoch.
pub async fn personal_mount_target_for_incarnation(
    state: &std::sync::Arc<crate::services::AppState>,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<Option<PersonalMountTarget>, crate::errors::AppError> {
    let mut db =
        crate::db::scoped::scoped_db_for_system_org(state, claims.org_id, &["workspaces:read"])
            .await?;
    let target = personal_mount_target(&mut db, claims, repo_id)
        .await
        .map_err(crate::errors::AppError::from)?;
    db.rollback().await.map_err(crate::errors::AppError::from)?;
    Ok(target)
}

/// Whether a current incarnation may read `repo_id` beyond its own members,
/// in one read scoped to the token's organization: a personal workspace's
/// reference checkout ([`personal_reference_readable`]) or any other
/// workspace's mounted personal repository ([`personal_mount_target`]).
pub async fn personal_read_allowed_for_incarnation(
    state: &std::sync::Arc<crate::services::AppState>,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<bool, crate::errors::AppError> {
    let mut db =
        crate::db::scoped::scoped_db_for_system_org(state, claims.org_id, &["workspaces:read"])
            .await?;
    let readable = personal_reference_readable(&mut db, claims, repo_id)
        .await
        .map_err(crate::errors::AppError::from)?
        || personal_mount_target(&mut db, claims, repo_id)
            .await
            .map_err(crate::errors::AppError::from)?
            .is_some();
    db.rollback().await.map_err(crate::errors::AppError::from)?;
    Ok(readable)
}

/// [`personal_mount_target`] on a caller's connection.
pub async fn personal_mount_target_on(
    conn: &mut PgConnection,
    claims: &WorkspaceIncarnationClaims,
    repo_id: Uuid,
) -> Result<Option<PersonalMountTarget>, sqlx::Error> {
    let Ok(epoch) = i64::try_from(claims.epoch) else {
        return Ok(None);
    };
    let row = sqlx::query(&format!(
        "SELECT personal.org_id, personal.id, personal.branch_name
         FROM workspaces AS w
         JOIN workspace_vms AS vm ON vm.org_id = w.org_id AND vm.workspace_id = w.id
         JOIN workspaces AS personal
           ON personal.org_id = w.org_id
          AND personal.personal_user_id = w.owner_user_id
          AND personal.deleted_at IS NULL
         JOIN workspace_repos AS member
           ON member.org_id = personal.org_id AND member.workspace_id = personal.id
          AND member.repo_id = $6
         WHERE w.id = $1 AND w.personal_user_id IS NULL
           AND w.agent_approval_review_id IS NULL AND {CURRENT}"
    ))
    .bind(claims.workspace_id)
    .bind(claims.org_id)
    .bind(claims.owner_user_id)
    .bind(claims.vm_id.to_string())
    .bind(epoch)
    .bind(repo_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|row| {
        Ok(PersonalMountTarget {
            org_id: row.try_get("org_id")?,
            workspace_id: row.try_get("id")?,
            branch_name: row.try_get("branch_name")?,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::CURRENT;

    #[test]
    fn the_rule_requires_a_live_owned_leased_incarnation() {
        for clause in [
            "w.deleted_at IS NULL",
            "w.org_id = $2",
            "w.owner_user_id = $3",
            "vm.vm_id = $4",
            "vm.epoch = $5",
            "vm.status IN ('starting', 'running', 'stopping')",
            "vm.lease_expires_at > clock_timestamp()",
        ] {
            assert!(CURRENT.contains(clause), "the rule requires {clause}");
        }
    }
}
