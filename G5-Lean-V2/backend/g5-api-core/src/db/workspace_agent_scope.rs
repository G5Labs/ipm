//! Which workspaces the agent in a workspace VM may operate on and read.
//!
//! A VM credential names one live incarnation of one workspace. The agent acts
//! for that workspace's current owner (`g5.security.agent-workspace-scope`):
//!
//! - **operations** reach any live workspace in the same organization with
//!   the same current owner: the caller's own and the owner's other
//!   workspaces, never Base, a formal review workspace or a review copy;
//! - **reads** reach any workspace the owner can currently read in the UI
//!   (`g5.security.agent-read-scope`).
//!
//! An operation on any workspace other than the caller's own, and forking,
//! also needs the owner's opt-in on the caller's workspace
//! (`g5.security.agent-other-workspaces`).
//!
//! Every request rechecks the caller incarnation, the target's kind and its
//! current owner. The reused domain operations then run under the
//! owner's authority and recheck ownership under their own locks; the VM
//! credential never becomes a person session.

use std::sync::Arc;

use g5_auth::WorkspaceIncarnationClaims;
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use sqlx::Row;
use uuid::Uuid;

use g5_contracts::types::ids::{OrgId, UserId};
use serde::Serialize;

use super::scoped::{ensure_user_workspace_access, scope_strings, scoped_db_for_user_actor_in_org};
use crate::errors::AppError;
use crate::services::identity::workspace_incarnation::{
    current_incarnation, current_incarnation_on_system, IncarnationLock,
};
use crate::services::{AppState, PermissionService};

/// What an operation accepts as its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTargetNeed {
    /// The caller's own workspace or another of its owner's.
    SelfOrOther,
    /// Another of the owner's workspaces, never the caller's own: chat
    /// mutations (`g5.security.agent-chat-control`) and deletion
    /// (`g5.security.agent-workspace-delete`).
    OtherOnly,
}

/// How an admitted target relates to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTarget {
    SelfWorkspace,
    /// Another workspace of the same current owner.
    Other,
}

fn forbidden() -> AppError {
    AppError::from(G5Error::Forbidden {
        missing_scopes: Vec::new(),
    })
}

/// Open a database scope acting as the credential's owner, after checking
/// that the credential's incarnation is still current.
///
/// RLS, active membership and every owner check then apply exactly as for the
/// owner's browser.
pub async fn agent_owner_db(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut db =
        scoped_db_for_user_actor_in_org(state, claims.owner_user_id, claims.org_id, scopes).await?;
    require_current_caller(&mut db, claims).await?;
    Ok(db)
}

/// Refuse a credential whose incarnation is no longer current.
pub async fn require_current_caller(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
) -> Result<(), AppError> {
    current_incarnation(db, claims, IncarnationLock::Share)
        .await?
        .map(|_| ())
        .ok_or_else(forbidden)
}

/// Refuse when the caller's owner has turned off its agent's changes to other
/// workspaces (`g5.security.agent-other-workspaces`). A workspace without a
/// settings row has the default, on.
pub async fn require_other_workspaces_allowed(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
) -> Result<(), AppError> {
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT agent_may_modify_other_workspaces
         FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(claims.workspace_id)
    .fetch_optional(db.conn())
    .await?
    .unwrap_or(true);
    if allowed {
        Ok(())
    } else {
        Err(AppError::from(G5Error::AgentOtherWorkspacesDisabled))
    }
}

/// Admit `target_id` for an operation, holding the target row `FOR SHARE`.
///
/// Any target other than the caller's own workspace first needs the owner's
/// setting to allow it, checked before the target is looked up. A refusal does not say
/// whether the workspace exists, so a target identifier alone reveals and
/// grants nothing.
pub async fn agent_target(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    target_id: Uuid,
    need: AgentTargetNeed,
) -> Result<AgentTarget, AppError> {
    if target_id != claims.workspace_id {
        require_other_workspaces_allowed(db, claims).await?;
    }
    agent_relation(db, claims, target_id, need).await
}

/// How `target_id` relates to the caller under agent scope, without the
/// opt-in: [`agent_target`] for operations, or alone to describe a workspace
/// the caller only reads.
pub async fn agent_relation(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    target_id: Uuid,
    need: AgentTargetNeed,
) -> Result<AgentTarget, AppError> {
    // Base belongs to the organization, not to whoever created it; it stays
    // a fork source (see the fork route).
    let row = sqlx::query(
        "SELECT owner_user_id, is_base, agent_approval_review_id IS NOT NULL AS formal_review
         FROM workspaces
         WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL
         FOR SHARE",
    )
    .bind(target_id)
    .bind(claims.org_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(forbidden)?;
    if row.try_get::<bool, _>("formal_review")?
        || row.try_get::<bool, _>("is_base")?
        || row.try_get::<Option<Uuid>, _>("owner_user_id")? != Some(claims.owner_user_id)
    {
        return Err(forbidden());
    }
    let relation = if target_id == claims.workspace_id {
        AgentTarget::SelfWorkspace
    } else {
        AgentTarget::Other
    };
    if relation == AgentTarget::SelfWorkspace && need == AgentTargetNeed::OtherOnly {
        return Err(forbidden());
    }
    if relation == AgentTarget::Other {
        // A reader's review copy is not a workspace the agent coordinates,
        // even when the same person owns it.
        crate::services::identity::workspace_mutation_policy::ensure_not_review_workspace(
            db,
            claims.org_id,
            target_id,
            "Workspace agent operation",
        )
        .await
        .map_err(|error| match error {
            G5Error::ValidationError { .. } => forbidden(),
            other => AppError::from(other),
        })?;
    }
    Ok(relation)
}

/// Admit `source_id` as a fork source: the caller's own workspace, another
/// under [`agent_target`], or the organization's Base when the owner can read
/// it (`g5.security.agent-workspace-scope`). The fork itself refuses a
/// personal source and holds the source for the copy.
pub async fn agent_fork_source(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    source_id: Uuid,
) -> Result<(), AppError> {
    let is_base = sqlx::query_scalar::<_, bool>(
        "SELECT is_base FROM workspaces WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(source_id)
    .bind(claims.org_id)
    .fetch_optional(db.conn())
    .await?
    .unwrap_or(false);
    if is_base {
        agent_read_target(state, db, claims, source_id).await
    } else {
        agent_target(db, claims, source_id, AgentTargetNeed::SelfOrOther)
            .await
            .map(|_| ())
    }
}

/// Admit `target_id` for a read: any workspace the owner can currently read,
/// rechecked as the UI's chat reads recheck it.
pub async fn agent_read_target(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
    target_id: Uuid,
) -> Result<(), AppError> {
    ensure_user_workspace_access(
        &state.system_pool,
        db,
        claims.owner_user_id,
        claims.org_id,
        target_id,
    )
    .await
    .map_err(|error| match error.inner {
        G5Error::NotFound { .. } => forbidden(),
        _ => error,
    })
}

/// [`agent_owner_db`] on the system pool, for operations on system tables
/// such as organization memberships (stop by user). The incarnation is read
/// through the definer lookup, which the system role may call.
pub async fn agent_owner_system_db(
    state: &Arc<AppState>,
    claims: &WorkspaceIncarnationClaims,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let capability = state
        .authz
        .mint_db_capability(
            UserId::from_uuid(claims.owner_user_id),
            &[OrgId::from_uuid(claims.org_id)],
            &scope_strings(scopes),
        )
        .await
        .map_err(AppError::from)?;
    let mut db = ScopedDb::from_capability(&state.system_pool, capability)
        .await
        .map_err(AppError::from)?;
    PermissionService::require_user_scopes_on_conn(
        db.conn(),
        claims.owner_user_id,
        claims.org_id,
        None,
        scopes,
    )
    .await?;
    current_incarnation_on_system(db.conn(), claims, IncarnationLock::Share)
        .await?
        .ok_or_else(forbidden)?;
    Ok(db)
}

/// A direct child of the caller, with its lifecycle at a glance.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentChild {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// `stopped` when the workspace has no VM row yet.
    pub vm_status: String,
    pub start_intent_status: Option<String>,
    pub stop_reason: Option<String>,
    pub queued_tasks: i64,
    pub running_tasks: i64,
    pub waiting_tasks: i64,
}

/// The caller's direct children: live, same owner, not a formal review
/// workspace, newest first. Review copies are excluded by [`agent_target`]
/// when one is addressed; this listing names them too rarely to filter here.
pub async fn list_agent_children(
    db: &mut ScopedDb,
    claims: &WorkspaceIncarnationClaims,
) -> Result<Vec<AgentChild>, AppError> {
    Ok(sqlx::query_as::<_, AgentChild>(
        "SELECT w.id, w.name, w.description, w.created_at,
                COALESCE(vm.status, 'stopped') AS vm_status,
                vm.start_intent ->> 'status' AS start_intent_status,
                vm.stop_reason,
                (SELECT count(*) FROM tasks t WHERE t.workspace_id = w.id AND t.status = 'queued') AS queued_tasks,
                (SELECT count(*) FROM tasks t WHERE t.workspace_id = w.id AND t.status = 'running') AS running_tasks,
                (SELECT count(*) FROM tasks t WHERE t.workspace_id = w.id AND t.status = 'waiting') AS waiting_tasks
         FROM workspaces w
         LEFT JOIN workspace_vms vm ON vm.workspace_id = w.id
         WHERE w.org_id = $1 AND w.cloned_from_id = $2 AND w.owner_user_id = $3
           AND w.deleted_at IS NULL AND w.agent_approval_review_id IS NULL
         ORDER BY w.created_at DESC, w.id",
    )
    .bind(claims.org_id)
    .bind(claims.workspace_id)
    .bind(claims.owner_user_id)
    .fetch_all(db.conn())
    .await?)
}

/// What a probe reports about a readable workspace's identity.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkspaceSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_user_id: Option<Uuid>,
    pub cloned_from_id: Option<Uuid>,
    pub is_base: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The identity of a workspace already admitted by [`agent_read_target`].
pub async fn workspace_summary(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<AgentWorkspaceSummary, AppError> {
    sqlx::query_as::<_, AgentWorkspaceSummary>(
        "SELECT id, name, description, owner_user_id, cloned_from_id, is_base, created_at
         FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(forbidden)
}

/// Whether `task_id` is a chat (agent) task: chat-task control never
/// reaches the caller's own workspace (`g5.security.agent-chat-control`),
/// other tasks such as a preview also admit it.
pub async fn task_is_chat(db: &mut ScopedDb, task_id: Uuid) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>("SELECT kind = 'agent' FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(forbidden)
}
