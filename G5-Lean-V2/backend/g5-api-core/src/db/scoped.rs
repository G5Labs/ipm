//! Per-request ScopedDb creation helpers.
//!
//! Provides convenience functions for route handlers to mint a
//! [`DbCapability`] and create a [`ScopedDb`] from the current
//! [`AuthContext`].
//!
//! # Usage (M3+ migration pattern)
//!
//! ```ignore
//! use crate::db::scoped::{scoped_db_for_org, scoped_db_for_orgs};
//!
//! async fn list_workspaces(
//!     State(state): State<Arc<AppState>>,
//!     auth: AuthContext,
//!     Path(params): Path<OrgPathParams>,
//! ) -> Result<Json<...>, AppError> {
//!     let mut db = scoped_db_for_org(
//!         &state, &auth, params.org_id, &["workspaces:read"],
//!     ).await?;
//!
//!     // All queries through db.conn() are RLS-scoped to this org
//!     let rows = sqlx::query("SELECT ...")
//!         .fetch_all(db.conn())
//!         .await?;
//!
//!     db.commit().await?;
//!     Ok(Json(rows))
//! }
//! ```

use std::sync::Arc;

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use g5_authz::{AuthzError, DbCapability, ScopedDb};
use g5_contracts::types::ids::{OrgId, SessionId, UserId};

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::services::AppState;

use g5_contracts::error::G5Error;

// ---------------------------------------------------------------------------
// Internal: extract actor UUID from AuthContext
// ---------------------------------------------------------------------------

fn actor_user_id(auth: &AuthContext) -> Result<UserId, AppError> {
    let id_str = auth
        .actor_id()
        .ok_or_else(|| AppError::from(G5Error::Unauthorized))?;
    let uuid = id_str.as_ref().parse::<Uuid>().map_err(|_| {
        AppError::from(G5Error::Internal {
            message: "Invalid actor ID in auth context".into(),
        })
    })?;
    Ok(UserId::from_uuid(uuid))
}

/// Extract and parse the org ID from the auth context.
///
/// Returns the org UUID from the authenticated principal's claims.
pub fn resolve_org_id(auth: &AuthContext) -> Result<Uuid, AppError> {
    let org_str = auth.selected_org_id().map_err(AppError::from)?;
    org_str.parse::<Uuid>().map_err(|_| {
        AppError::from(G5Error::Internal {
            message: "Invalid org_id in auth context".into(),
        })
    })
}

// ---------------------------------------------------------------------------
// Entity-scoped org resolution helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceScopeContext {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
}

fn not_found(resource: impl Into<String>) -> AppError {
    AppError::from(G5Error::NotFound {
        resource: resource.into(),
    })
}

fn forbidden() -> AppError {
    AppError::from(G5Error::Forbidden {
        missing_scopes: vec![],
    })
}

fn auth_user_uuid(auth: &AuthContext) -> Result<Option<Uuid>, AppError> {
    auth.user_id()
        .map(|id| {
            id.parse::<Uuid>().map_err(|_| {
                AppError::from(G5Error::Internal {
                    message: "Invalid user ID in auth context".into(),
                })
            })
        })
        .transpose()
}

fn scopes_require_current_workspace_owner(scopes: &[&str]) -> bool {
    scopes.iter().any(|scope| {
        matches!(
            *scope,
            "workspaces:write" | "tasks:execute" | "tasks:write" | "sessions:write" | "docs:write"
        )
    })
}

/// Open the tenant transaction that will perform an entity-scoped operation
/// and, for user mutations, keep a shared lock on the workspace authority row
/// until that operation commits. A transfer takes the conflicting update lock,
/// so an old-owner request cannot pass admission and execute after cutover.
async fn workspace_scoped_db_for_org(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    workspace_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    workspace_scoped_db_for_org_locked(state, auth, org_id, workspace_id, scopes, false).await
}

async fn workspace_scoped_db_for_org_locked(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    workspace_id: Uuid,
    scopes: &[&str],
    update_workspace: bool,
) -> Result<ScopedDb, AppError> {
    let mut db = open_capability_db(state, &state.tenant_pool, auth, org_id, scopes).await?;
    // Scope authorization itself holds the workspace FOR SHARE. Take the
    // strongest lock first, before either permission or ownership admission,
    // otherwise concurrent checkpoint writers deadlock upgrading those locks.
    if update_workspace {
        sqlx::query("SELECT id FROM workspaces WHERE id=$1 AND org_id=$2 FOR UPDATE")
            .bind(workspace_id)
            .bind(org_id)
            .fetch_optional(db.conn())
            .await?;
    }
    authorize_org_scopes(&mut db, auth, org_id, Some(workspace_id), scopes).await?;
    if auth.is_user() && scopes_require_current_workspace_owner(scopes) {
        let user_id = auth_user_uuid(auth)?.ok_or_else(|| AppError::from(G5Error::Unauthorized))?;
        require_current_workspace_owner(&mut db, org_id, workspace_id, user_id).await?;
    }
    Ok(db)
}

/// Hold workspace ownership through the caller's mutation, including when the
/// workspace is discovered from a configuration row on an org-scoped route.
pub(crate) async fn require_current_workspace_owner(
    db: &mut ScopedDb,
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    let owner = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT workspace.owner_user_id
             FROM workspaces AS workspace
             WHERE workspace.id = $1 AND workspace.org_id = $2
               AND workspace.deleted_at IS NULL
             FOR SHARE OF workspace",
    )
    .bind(workspace_id)
    .bind(org_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| not_found(format!("workspace {workspace_id}")))?;
    if owner != Some(user_id) {
        return Err(forbidden());
    }
    // Read the transfer state with a fresh READ COMMITTED snapshot after the
    // workspace lock is acquired. A transfer may have committed its pending
    // row while we waited for its workspace lock, without changing the owner.
    let transfer_pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_ownership_transfers
         WHERE workspace_id = $1 AND status = 'pending')",
    )
    .bind(workspace_id)
    .fetch_one(db.conn())
    .await?;
    if transfer_pending {
        return Err(AppError::from(G5Error::WorkspaceStopping));
    }
    Ok(())
}

async fn enforce_task_owner_for_user(
    db: &mut ScopedDb,
    auth: &AuthContext,
    task_id: Uuid,
) -> Result<(), AppError> {
    let Some(user_id) = auth_user_uuid(auth)? else {
        return Ok(());
    };

    let visible = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT COALESCE((
            SELECT w.owner_user_id = $2
            FROM tasks t
            JOIN workspaces w ON w.id = t.workspace_id
            WHERE t.id = $1
        ), false)
        "#,
    )
    .bind(task_id)
    .bind(user_id)
    .fetch_one(db.conn())
    .await?;

    if visible {
        Ok(())
    } else {
        // Tasks remain workspace-visible for reads. Only the current owner
        // may mutate them; missing or cross-org tasks were rejected earlier.
        Err(forbidden())
    }
}

async fn query_scope_context(
    conn: &mut PgConnection,
    sql: &str,
    id: Uuid,
    resource: &str,
) -> Result<WorkspaceScopeContext, AppError> {
    let row = sqlx::query_as::<_, (Uuid, Uuid)>(sql)
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| not_found(format!("{resource} {id}")))?;

    Ok(WorkspaceScopeContext {
        org_id: row.0,
        workspace_id: row.1,
    })
}

/// Resolve the org_id and workspace_id for a workspace under the current RLS scope.
pub async fn resolve_workspace_context(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    query_scope_context(
        db.conn(),
        "SELECT org_id, id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        workspace_id,
        "workspace",
    )
    .await
}

/// Resolve a canonical workspace repository after verifying membership.
pub async fn resolve_workspace_repo_context(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    let context = resolve_workspace_context(db, workspace_id).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_repos WHERE workspace_id=$1 AND repo_id=$2)",
    )
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_one(db.conn())
    .await?;
    if !exists {
        return Err(AppError::from_error(G5Error::NotFound {
            resource: format!("workspace repository {workspace_id}/{repo_id}"),
        }));
    }
    Ok(context)
}

/// Resolve the org_id and workspace_id for a task under the current RLS scope.
pub async fn resolve_task_context(
    db: &mut ScopedDb,
    task_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    query_scope_context(
        db.conn(),
        r#"
        SELECT w.org_id, w.id
        FROM tasks t
        JOIN workspaces w ON w.id = t.workspace_id
        WHERE t.id = $1
          AND w.deleted_at IS NULL
        "#,
        task_id,
        "task",
    )
    .await
}

/// Resolve the org_id and workspace_id for a doc under the current RLS scope.
pub async fn resolve_doc_context(
    db: &mut ScopedDb,
    doc_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    query_scope_context(
        db.conn(),
        r#"
        SELECT w.org_id, d.workspace_id
        FROM docs d
        JOIN workspaces w ON w.id = d.workspace_id
        WHERE d.id = $1
          AND w.deleted_at IS NULL
        "#,
        doc_id,
        "doc",
    )
    .await
}

/// Resolve the org/workspace context for a chat session under the current RLS scope.
pub async fn resolve_chat_session_context(
    db: &mut ScopedDb,
    session_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    query_scope_context(
        db.conn(),
        r#"
        SELECT w.org_id, s.workspace_id
        FROM chat_sessions s
        JOIN workspaces w ON w.id = s.workspace_id
        WHERE s.id = $1
          AND w.deleted_at IS NULL
        "#,
        session_id,
        "chat session",
    )
    .await
}

/// Resolve the org/workspace context for a comment under the current RLS scope.
pub async fn resolve_comment_context(
    db: &mut ScopedDb,
    comment_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    query_scope_context(
        db.conn(),
        r#"
        SELECT w.org_id, ci.workspace_id
        FROM comment_index ci
        JOIN workspaces w ON w.id = ci.workspace_id
        WHERE ci.id = $1
          AND w.deleted_at IS NULL
        "#,
        comment_id,
        "comment",
    )
    .await
}

/// Resolve the org/workspace context for a notification under the current RLS scope.
pub async fn resolve_notification_context(
    db: &mut ScopedDb,
    notification_id: Uuid,
) -> Result<WorkspaceScopeContext, AppError> {
    query_scope_context(
        db.conn(),
        r#"
        SELECT w.org_id, n.workspace_id
        FROM notifications n
        JOIN workspaces w ON w.id = n.workspace_id
        WHERE n.id = $1
          AND w.deleted_at IS NULL
        "#,
        notification_id,
        "notification",
    )
    .await
}

pub async fn resolve_org_id_for_workspace(
    db: &mut ScopedDb,
    workspace_id: Uuid,
) -> Result<Uuid, AppError> {
    Ok(resolve_workspace_context(db, workspace_id).await?.org_id)
}

// ---------------------------------------------------------------------------
// System actor identity
// ---------------------------------------------------------------------------

/// Well-known system actor UUID for server-owned background work.
///
/// Background tasks such as task-unblock orchestration have no request-bound
/// auth context, so they use this deterministic UUID for capability minting
/// and audit attribution.
const SYSTEM_ACTOR_UUID: Uuid = Uuid::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

// ---------------------------------------------------------------------------
// Org-scoped helpers
// ---------------------------------------------------------------------------

pub(crate) fn scope_strings(scopes: &[&str]) -> Vec<String> {
    scopes.iter().map(|scope| scope.to_string()).collect()
}

/// Hold target authority in the transaction that performs the operation. A
/// workspace contributes overrides only when resolved inside this organization.
/// Route middleware separately enforces the token's scope ceiling.
pub async fn authorize_org_scopes(
    db: &mut ScopedDb,
    auth: &AuthContext,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    scopes: &[&str],
) -> Result<(), AppError> {
    let claims = auth.claims().ok_or_else(forbidden)?;
    if claims.iss.as_deref() == Some(crate::middleware::auth_middleware::API_KEY_ISSUER)
        && claims.org != org_id.to_string()
    {
        return Err(forbidden());
    }
    crate::services::PermissionService::require_user_scopes_on_conn(
        db.conn(),
        auth.require_user_id().map_err(AppError::from)?,
        org_id,
        workspace_id,
        scopes,
    )
    .await
    .map_err(AppError::from)
}

async fn open_authorized_db(
    state: &Arc<AppState>,
    pool: &PgPool,
    auth: &AuthContext,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut db = open_capability_db(state, pool, auth, org_id, scopes).await?;
    authorize_org_scopes(&mut db, auth, org_id, workspace_id, scopes).await?;
    Ok(db)
}

/// Establish verified tenant context without taking permission row locks.
/// Callers must still authorize the requested scopes before returning it.
async fn open_capability_db(
    state: &Arc<AppState>,
    pool: &PgPool,
    auth: &AuthContext,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let capability = state
        .authz
        .mint_db_capability(
            actor_user_id(auth)?,
            &[OrgId::from_uuid(org_id)],
            &scope_strings(scopes),
        )
        .await
        .map_err(AppError::from)?;
    ScopedDb::from_capability(pool, capability)
        .await
        .map_err(AppError::from)
}

/// Create a `ScopedDb` scoped to a single org.
///
/// Steps:
/// 1. Extract `actor_id` from `AuthContext`
/// 2. Mint a `DbCapability` via `mint_db_capability`, which checks the
///    authenticated user's `org_memberships` row.
/// 3. Create `ScopedDb::from_capability` (begins tx + sets RLS context).
/// 4. Check and lock required current target-org authority in that transaction.
///
/// The caller MUST call `db.commit().await?` after all queries, or the
/// transaction will be rolled back on drop.
pub async fn scoped_db_for_org(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    open_authorized_db(state, &state.tenant_pool, auth, org_id, None, scopes).await
}

/// Create a `ScopedDb` over the system pool scoped to a single org.
///
/// This is for request paths that need the caller's authz context but must
/// query system/auth tables that are intentionally unavailable to the tenant
/// pool role.
pub async fn scoped_system_db_for_org(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    open_authorized_db(state, &state.system_pool, auth, org_id, None, scopes).await
}

/// Membership transitions lock the organization before the actor/target and
/// authority rows. This avoids competing administrator reductions upgrading
/// each other's admission locks. Authority is resolved again after all waits.
pub async fn scoped_system_db_for_membership_transition(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<ScopedDb, AppError> {
    let mut db =
        open_capability_db(state, &state.system_pool, auth, org_id, &["org:admin"]).await?;
    crate::services::OrgService::lock_membership_transition(
        db.conn(),
        org_id,
        auth.require_user_id()?,
        user_id,
    )
    .await?;
    authorize_org_scopes(&mut db, auth, org_id, None, &["org:admin"]).await?;
    Ok(db)
}

/// Create a `ScopedDb` for server-owned background work.
///
/// This function is used by code paths that are not tied to a request-bound
/// auth context, such as fire-and-forget orchestration after escalation
/// resolution. It uses [`SYSTEM_ACTOR_UUID`] as the actor and mints a
/// service capability, bypassing org-membership checks.
///
/// Use this only for server background work where the org_id has already been
/// resolved from the target entity.
pub async fn scoped_db_for_system_org(
    state: &Arc<AppState>,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    scoped_db_for_system_org_with(state.authz.as_ref(), &state.tenant_pool, org_id, scopes).await
}

/// [`scoped_db_for_system_org`] for background code that carries its own
/// authz service and tenant pool instead of an [`AppState`] (the
/// maintenance workers).
pub async fn scoped_db_for_system_org_with(
    authz: &dyn g5_authz::AuthzService,
    tenant_pool: &sqlx::PgPool,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let actor_id = UserId::from_uuid(SYSTEM_ACTOR_UUID);
    let scope_strings = scope_strings(scopes);

    let capability = authz
        .mint_service_db_capability(actor_id, OrgId::from_uuid(org_id), &scope_strings)
        .await
        .map_err(AppError::from)?;

    let db = ScopedDb::from_capability(tenant_pool, capability)
        .await
        .map_err(AppError::from)?;

    Ok(db)
}

/// Create a `ScopedDb` for a specific user actor and org.
///
/// Used by non-HTTP middleware callsites (for example WebSocket auth and git
/// smart-HTTP auth) that already have concrete actor/org IDs rather than an
/// `AuthContext`.
pub async fn scoped_db_for_user_actor_in_org(
    state: &Arc<AppState>,
    user_id: Uuid,
    org_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    scoped_db_for_user_actor_in_org_with_lock_timeout(state, user_id, org_id, scopes, None).await
}

/// Admit the same user authority with an optional server-side lock deadline.
/// Background continuations must release blocked queries even if their caller
/// cancels the future; client cancellation alone only queues SQLx rollback.
pub async fn scoped_db_for_user_actor_in_org_with_lock_timeout(
    state: &Arc<AppState>,
    user_id: Uuid,
    org_id: Uuid,
    scopes: &[&str],
    lock_timeout: Option<std::time::Duration>,
) -> Result<ScopedDb, AppError> {
    let capability = state
        .authz
        .mint_db_capability(
            UserId::from_uuid(user_id),
            &[OrgId::from_uuid(org_id)],
            &scope_strings(scopes),
        )
        .await
        .map_err(AppError::from)?;

    let mut db = ScopedDb::from_capability(&state.tenant_pool, capability)
        .await
        .map_err(AppError::from)?;
    if let Some(timeout) = lock_timeout {
        sqlx::query("SELECT set_config('lock_timeout', $1, true)")
            .bind(format!("{}ms", timeout.as_millis().max(1)))
            .execute(db.conn())
            .await?;
    }
    crate::services::PermissionService::require_user_scopes_on_conn(
        db.conn(),
        user_id,
        org_id,
        None,
        scopes,
    )
    .await
    .map_err(AppError::from)?;
    Ok(db)
}

async fn scoped_resolution_db_for_auth(
    state: &Arc<AppState>,
    auth: &AuthContext,
) -> Result<ScopedDb, AppError> {
    scoped_db_for_user_orgs(state, auth, &[]).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceAccessResult {
    Allowed,
    Forbidden,
    NotFound,
}

/// How long an access decision must stay true (`g5.security.current-authority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessHold {
    /// A read: the decision describes this moment only.
    Momentary,
    /// A transactional write: current active membership and the workspace row
    /// stay share-locked on `db` until commit. Membership withdrawal, role
    /// change, transfer and deletion cannot invalidate admitted authority.
    UntilCommit,
}

pub async fn check_user_workspace_access(
    system_pool: &PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<WorkspaceAccessResult, AppError> {
    check_user_workspace_access_with(
        system_pool,
        db,
        user_id,
        org_id,
        workspace_id,
        AccessHold::Momentary,
    )
    .await
}

pub async fn check_user_workspace_access_with(
    system_pool: &PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
    hold: AccessHold,
) -> Result<WorkspaceAccessResult, AppError> {
    let org_role = match hold {
        AccessHold::Momentary => sqlx::query_scalar::<_, String>(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(system_pool)
        .await?,
        AccessHold::UntilCommit => sqlx::query_scalar::<_, String>(
            "SELECT role FROM authz.lock_org_membership($1, $2) WHERE deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(db.conn())
        .await?,
    };

    let Some(org_role) = org_role else {
        return Ok(WorkspaceAccessResult::NotFound);
    };

    Ok(
        workspace_access_for_role_with(db.conn(), &org_role, user_id, org_id, workspace_id, hold)
            .await?,
    )
}

/// Whether a member holding `org_role` in `org_id` may read `workspace_id`
/// (`g5.workspace.audience`): organization administrators read
/// ordinary and personal workspaces. Other members read ordinary workspaces
/// and their own personal workspace. Formal runtimes remain excluded.
pub(crate) async fn workspace_access_for_role(
    conn: &mut PgConnection,
    org_role: &str,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<WorkspaceAccessResult, sqlx::Error> {
    workspace_access_for_role_with(
        conn,
        org_role,
        user_id,
        org_id,
        workspace_id,
        AccessHold::Momentary,
    )
    .await
}

/// [`workspace_access_for_role`] with a chosen [`AccessHold`]. The row-level
/// workspace row stays locked for transactional writes; formal-review RLS
/// remains an independent restriction.
pub(crate) async fn workspace_access_for_role_with(
    conn: &mut PgConnection,
    org_role: &str,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
    hold: AccessHold,
) -> Result<WorkspaceAccessResult, sqlx::Error> {
    let lock = match hold {
        AccessHold::Momentary => "",
        AccessHold::UntilCommit => "FOR SHARE",
    };
    let allowed = sqlx::query_scalar::<_, bool>(&format!(
        "SELECT authz.workspace_audience_allows(personal_user_id, owner_user_id,
                    agent_approval_review_id, $3, $4)
         FROM workspaces
         WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL
           AND agent_approval_review_id IS NULL
         {lock}"
    ))
    .bind(workspace_id)
    .bind(org_id)
    .bind(user_id)
    .bind(matches!(org_role, "admin"))
    .fetch_optional(&mut *conn)
    .await?;

    Ok(match allowed {
        Some(true) => WorkspaceAccessResult::Allowed,
        Some(false) => WorkspaceAccessResult::Forbidden,
        None => WorkspaceAccessResult::NotFound,
    })
}

pub async fn user_can_access_workspace(
    system_pool: &PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<bool, AppError> {
    Ok(matches!(
        check_user_workspace_access(system_pool, db, user_id, org_id, workspace_id).await?,
        WorkspaceAccessResult::Allowed
    ))
}

pub async fn ensure_user_workspace_access(
    system_pool: &PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), AppError> {
    ensure_user_workspace_access_with(
        system_pool,
        db,
        user_id,
        org_id,
        workspace_id,
        AccessHold::Momentary,
    )
    .await
}

/// [`ensure_user_workspace_access`] for a transactional write on `db`: the
/// access relied on stays held until `db` commits
/// ([`AccessHold::UntilCommit`]).
pub async fn ensure_user_workspace_access_until_commit(
    system_pool: &PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), AppError> {
    ensure_user_workspace_access_with(
        system_pool,
        db,
        user_id,
        org_id,
        workspace_id,
        AccessHold::UntilCommit,
    )
    .await
}

async fn ensure_user_workspace_access_with(
    system_pool: &PgPool,
    db: &mut ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Uuid,
    hold: AccessHold,
) -> Result<(), AppError> {
    match check_user_workspace_access_with(system_pool, db, user_id, org_id, workspace_id, hold)
        .await?
    {
        WorkspaceAccessResult::Allowed => Ok(()),
        WorkspaceAccessResult::Forbidden => Err(AppError::from(G5Error::Forbidden {
            missing_scopes: vec![],
        })),
        WorkspaceAccessResult::NotFound => Err(not_found(format!("workspace {workspace_id}"))),
    }
}

/// Create a `ScopedDb` scoped to the org that owns a workspace.
pub async fn scoped_db_for_workspace(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    scoped_db_for_workspace_locked(state, auth, workspace_id, scopes, false).await
}

/// A mutation that updates the workspace row itself must take its write lock
/// before the usual shared owner fence, avoiding concurrent lock upgrades.
pub async fn scoped_db_for_workspace_update(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    scoped_db_for_workspace_locked(state, auth, workspace_id, scopes, true).await
}

async fn scoped_db_for_workspace_locked(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    scopes: &[&str],
    update_workspace: bool,
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_workspace_context(&mut resolver, workspace_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    let org_id = context.org_id;
    workspace_scoped_db_for_org_locked(state, auth, org_id, workspace_id, scopes, update_workspace)
        .await
}

/// Create a system-pool `ScopedDb` scoped to the org that owns a workspace.
///
/// This is the system-table counterpart to [`scoped_db_for_workspace`].
pub async fn scoped_system_db_for_workspace(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_workspace_context(&mut resolver, workspace_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    open_authorized_db(
        state,
        &state.system_pool,
        auth,
        context.org_id,
        Some(workspace_id),
        scopes,
    )
    .await
}

/// Create a scoped connection after authorizing a workspace repository.
pub async fn scoped_db_for_workspace_repo(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_workspace_repo_context(&mut resolver, workspace_id, repo_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

/// Create a `ScopedDb` scoped to the org and workspace that own a task,
/// for **mutation** routes.
///
/// Resolves the task's workspace, enforces the workspace audience, and requires
/// the current workspace owner. Read routes use `scoped_db_for_task_view`,
/// which skips the mutation ownership requirement.
pub async fn scoped_db_for_task(
    state: &Arc<AppState>,
    auth: &AuthContext,
    task_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_task_context(&mut resolver, task_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    enforce_task_owner_for_user(&mut resolver, auth, task_id).await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

/// Create a `ScopedDb` for **read-only** task access.
///
/// Resolves the task's owning workspace and enforces the fixed workspace audience:
/// chat execution rows are workspace-visible in listings and reads (issue
/// #899, security_policy/role_based_access_control D10). Backs the task read
/// routes (`get_task`, `get_task_timeline`, `get_task_diff`); mutation
/// routes keep `scoped_db_for_task`.
pub async fn scoped_db_for_task_view(
    state: &Arc<AppState>,
    auth: &AuthContext,
    task_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_task_context(&mut resolver, task_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

pub async fn scoped_db_for_chat_session(
    state: &Arc<AppState>,
    auth: &AuthContext,
    session_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_chat_session_context(&mut resolver, session_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

/// Create a `ScopedDb` for **read-only** chat-session access.
///
/// Resolves the session's owning workspace and enforces the fixed workspace audience, but does NOT require the caller to be the session's `created_by`
/// owner: any user who can access the workspace may view the session's
/// conversation history. This backs the read routes (`get_session`,
/// `get_messages`, `get_session_timeline`) per the chat session visibility
/// contract (@ref software_architecture/g5-api/routes/chat#D0). Mutation
/// mutation routes use `scoped_db_for_chat_session`, whose write scopes enforce
/// the workspace's current owner.
pub async fn scoped_db_for_chat_session_view(
    state: &Arc<AppState>,
    auth: &AuthContext,
    session_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_chat_session_context(&mut resolver, session_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

pub async fn scoped_db_for_doc(
    state: &Arc<AppState>,
    auth: &AuthContext,
    doc_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_doc_context(&mut resolver, doc_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

pub async fn scoped_db_for_comment(
    state: &Arc<AppState>,
    auth: &AuthContext,
    comment_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_comment_context(&mut resolver, comment_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    workspace_scoped_db_for_org(state, auth, context.org_id, context.workspace_id, scopes).await
}

pub async fn scoped_db_for_notification(
    state: &Arc<AppState>,
    auth: &AuthContext,
    notification_id: Uuid,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let mut resolver = scoped_resolution_db_for_auth(state, auth).await?;
    let context = resolve_notification_context(&mut resolver, notification_id).await?;
    enforce_workspace_access_for_user(
        state,
        &mut resolver,
        auth,
        context.org_id,
        context.workspace_id,
    )
    .await?;
    resolver.rollback().await.map_err(AppError::from)?;
    scoped_db_for_org(state, auth, context.org_id, scopes).await
}

fn ownership_validation_error(
    field: &'static str,
    message: &'static str,
    details: serde_json::Value,
) -> AppError {
    AppError::from(G5Error::ValidationError {
        details: serde_json::json!({
            "field": field,
            "message": message,
            "details": details,
        }),
    })
}

pub fn ensure_workspace_org_id(
    workspace_org_id: Uuid,
    org_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    if workspace_org_id == org_id {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Org ID does not own this workspace",
            serde_json::json!({ "workspace_org_id": workspace_org_id, "org_id": org_id }),
        ))
    }
}

pub async fn ensure_workspace_doc_id(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    doc_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM docs WHERE id = $1 AND workspace_id = $2)")
            .bind(doc_id)
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Document ID is not linked to this workspace",
            serde_json::json!({ "workspace_id": workspace_id, "doc_id": doc_id }),
        ))
    }
}

pub async fn ensure_workspace_repo_id(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
             SELECT 1 \
             FROM workspace_repos \
             WHERE workspace_id = $1 \
               AND repo_id = $2 \
         )",
    )
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_one(db.conn())
    .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Repo ID is not linked to this workspace",
            serde_json::json!({ "workspace_id": workspace_id, "repo_id": repo_id }),
        ))
    }
}

pub async fn ensure_org_repo_id(
    db: &mut ScopedDb,
    org_id: Uuid,
    repo_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM repos WHERE id = $1 AND org_id = $2)")
            .bind(repo_id)
            .bind(org_id)
            .fetch_one(db.conn())
            .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Repo ID is not linked to this org",
            serde_json::json!({ "org_id": org_id, "repo_id": repo_id }),
        ))
    }
}

pub async fn ensure_workspace_task_id(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    task_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    // Direct task ownership must match the authorized workspace.
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
             SELECT 1 \
             FROM tasks t \
             WHERE t.id = $1 \
               AND t.workspace_id = $2 \
         )",
    )
    .bind(task_id)
    .bind(workspace_id)
    .fetch_one(db.conn())
    .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Task ID is not linked to this workspace",
            serde_json::json!({ "workspace_id": workspace_id, "task_id": task_id }),
        ))
    }
}

/// Bind a remote merge attempt to the authorized workspace and repository.
pub async fn ensure_workspace_repo_merge_run_id(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
    merge_run_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_merge_runs
         WHERE id = $1 AND workspace_id = $2 AND repo_id = $3
           AND source_kind = 'remote_tracking')",
    )
    .bind(merge_run_id)
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_one(db.conn())
    .await?;
    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Merge run is not linked to this workspace repository",
            serde_json::json!({ "workspace_id": workspace_id, "repo_id": repo_id, "merge_run_id": merge_run_id }),
        ))
    }
}

pub async fn ensure_session_message_id(
    db: &mut ScopedDb,
    session_id: Uuid,
    message_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM chat_messages WHERE id = $1 AND session_id = $2)",
    )
    .bind(message_id)
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Chat message ID is not linked to this session",
            serde_json::json!({ "session_id": session_id, "message_id": message_id }),
        ))
    }
}

/// A chat turn the caller may act on: either a persisted `chat_messages` row
/// or a `session_inputs` row of this session. Send and start return the
/// input id as the turn's message id, and no `chat_messages` row exists for
/// it until the runtime accepts the turn, so an owner stopping a queued turn
/// must be admitted by its input id.
pub async fn ensure_session_turn_id(
    db: &mut ScopedDb,
    session_id: Uuid,
    turn_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM chat_messages WHERE id = $1 AND session_id = $2)
             OR EXISTS(SELECT 1 FROM session_inputs WHERE id = $1 AND session_id = $2)",
    )
    .bind(turn_id)
    .bind(session_id)
    .fetch_one(db.conn())
    .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Chat message ID is not linked to this session",
            serde_json::json!({ "session_id": session_id, "message_id": turn_id }),
        ))
    }
}

pub async fn ensure_session_approval_id(
    db: &mut ScopedDb,
    session_id: Uuid,
    approval_id: &str,
    field: &'static str,
) -> Result<(), AppError> {
    let session_id_str = session_id.to_string();
    let belongs: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM task_events te
            JOIN tasks t ON t.id = te.task_id
            WHERE t.chat_session_id = $1
              AND te.metadata ->> 'approvalId' = $2
              AND te.metadata ->> 'providerEvent' = 'provided_agent.approval.requested'
        )
        "#,
    )
    .bind(&session_id_str)
    .bind(approval_id)
    .fetch_one(db.conn())
    .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Approval ID is not linked to this session",
            serde_json::json!({ "session_id": session_id, "approval_id": approval_id }),
        ))
    }
}

pub async fn ensure_org_user_id(
    db: &mut ScopedDb,
    org_id: Uuid,
    user_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    ensure_org_user_ids(db, org_id, &[user_id], field).await
}

pub async fn ensure_org_user_ids(
    db: &mut ScopedDb,
    org_id: Uuid,
    user_ids: &[Uuid],
    field: &'static str,
) -> Result<(), AppError> {
    if user_ids.is_empty() {
        return Ok(());
    }

    let existing_user_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM org_memberships WHERE org_id = $1 AND user_id = ANY($2)",
    )
    .bind(org_id)
    .bind(user_ids)
    .fetch_all(db.conn())
    .await?;

    if existing_user_ids.len() == user_ids.len() {
        return Ok(());
    }

    let missing_user_ids: Vec<Uuid> = user_ids
        .iter()
        .filter(|user_id| !existing_user_ids.contains(user_id))
        .copied()
        .collect();

    Err(ownership_validation_error(
        field,
        "Some user IDs are not members of this org",
        serde_json::json!({ "org_id": org_id, "missing_user_ids": missing_user_ids }),
    ))
}

pub async fn ensure_workspace_user_ids(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    user_ids: &[Uuid],
    field: &'static str,
) -> Result<(), AppError> {
    if user_ids.is_empty() {
        return Ok(());
    }

    let allowed_user_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT om.user_id
        FROM workspaces w
        JOIN org_memberships om ON om.org_id = w.org_id
        WHERE w.id = $1 AND w.deleted_at IS NULL
          AND om.user_id = ANY($2) AND om.deactivated_at IS NULL
          AND authz.workspace_audience_allows(w.personal_user_id, w.owner_user_id,
              w.agent_approval_review_id, om.user_id, om.role = 'admin')
        "#,
    )
    .bind(workspace_id)
    .bind(user_ids)
    .fetch_all(db.conn())
    .await?;

    if allowed_user_ids.len() == user_ids.len() {
        return Ok(());
    }

    let missing_user_ids: Vec<Uuid> = user_ids
        .iter()
        .filter(|user_id| !allowed_user_ids.contains(user_id))
        .copied()
        .collect();

    Err(ownership_validation_error(
        field,
        "Some user IDs do not have access to this workspace",
        serde_json::json!({ "workspace_id": workspace_id, "missing_user_ids": missing_user_ids }),
    ))
}

pub async fn ensure_public_user_id(
    pool: &PgPool,
    user_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    if exists {
        Ok(())
    } else {
        Err(not_found(format!("{field} {user_id}")))
    }
}

pub async fn ensure_user_invitation_id(
    pool: &PgPool,
    invitation_id: Uuid,
    user_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let row: Option<(String, bool)> = sqlx::query_as(
        r#"
        SELECT
            oi.status,
            EXISTS (
                SELECT 1
                FROM users u
                WHERE u.id = $2
                  AND lower(u.email) = lower(oi.email)
            ) AS email_matches
        FROM org_invitations oi
        WHERE oi.id = $1
        "#,
    )
    .bind(invitation_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some((status, email_matches)) = row else {
        return Err(not_found(format!("{field} {invitation_id}")));
    };

    if status != "pending" {
        return Err(AppError::from(G5Error::Conflict {
            reason: "Invitation is no longer pending".to_string(),
        }));
    }

    if !email_matches {
        return Err(AppError::from(G5Error::Conflict {
            reason: "Email does not match invitation".to_string(),
        }));
    }

    Ok(())
}

pub async fn ensure_org_invitation_id(
    db: &mut ScopedDb,
    org_id: Uuid,
    invitation_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM org_invitations WHERE id = $1 AND org_id = $2)",
    )
    .bind(invitation_id)
    .bind(org_id)
    .fetch_one(db.conn())
    .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "Invitation ID is not linked to this org",
            serde_json::json!({ "org_id": org_id, "invitation_id": invitation_id }),
        ))
    }
}

pub async fn ensure_api_key_id(
    db: &mut ScopedDb,
    org_id: Uuid,
    key_id: Uuid,
    field: &'static str,
) -> Result<(), AppError> {
    let belongs: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM api_keys WHERE id = $1 AND org_id = $2)")
            .bind(key_id)
            .bind(org_id)
            .fetch_one(db.conn())
            .await?;

    if belongs {
        Ok(())
    } else {
        Err(ownership_validation_error(
            field,
            "API key ID is not linked to this org",
            serde_json::json!({ "org_id": org_id, "key_id": key_id }),
        ))
    }
}

async fn enforce_workspace_access_for_user(
    state: &Arc<AppState>,
    db: &mut ScopedDb,
    auth: &AuthContext,
    org_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), AppError> {
    if !auth.is_user() {
        return Ok(());
    }

    let user_id = auth.require_user_id().map_err(AppError::from)?;
    ensure_user_workspace_access(&state.system_pool, db, user_id, org_id, workspace_id).await
}

/// Create an unscoped system-pool `ScopedDb` for an authenticated actor.
///
/// This is for an endpoint where org context is intentionally derived inside
/// the one transaction that also writes system metadata. The service must mint
/// and activate an explicit bounded capability before touching tenant rows.
pub async fn scoped_db_for_actor_unscoped(
    state: &Arc<AppState>,
    auth: &AuthContext,
) -> Result<ScopedDb, AppError> {
    let capability = DbCapability {
        org_ids: vec![],
        actor_id: actor_user_id(auth)?,
        scopes: vec![],
        session_id: SessionId::new(),
        ttl_secs: 3600,
    };

    let db = ScopedDb::unscoped(&state.system_pool, capability)
        .await
        .map_err(AppError::from)?;

    Ok(db)
}

/// Create a `ScopedDb` scoped to multiple orgs.
///
/// Used for cross-org endpoints (dashboard, global search, notifications)
/// where the user may have access to several orgs simultaneously.
pub async fn scoped_db_for_orgs(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_ids: &[Uuid],
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    let actor_id = actor_user_id(auth)?;
    let scope_strings: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    let typed_org_ids: Vec<OrgId> = org_ids.iter().map(|&id| OrgId::from_uuid(id)).collect();

    let capability = state
        .authz
        .mint_db_capability(actor_id, &typed_org_ids, &scope_strings)
        .await
        .map_err(AppError::from)?;

    let mut db = ScopedDb::from_capability(&state.tenant_pool, capability)
        .await
        .map_err(AppError::from)?;
    let mut ordered_orgs = org_ids.to_vec();
    ordered_orgs.sort_unstable();
    ordered_orgs.dedup();
    for org_id in ordered_orgs {
        authorize_org_scopes(&mut db, auth, org_id, None, scopes).await?;
    }
    Ok(db)
}

/// Create a `ScopedDb` scoped to ALL orgs the user is a member of.
///
/// Queries `AuthzService::user_org_ids` first, then mints a capability
/// for all of them. Used when the handler doesn't have a specific org_id
/// from the URL path (e.g., `/api/v1/notifications`).
pub async fn scoped_db_for_user_orgs(
    state: &Arc<AppState>,
    auth: &AuthContext,
    scopes: &[&str],
) -> Result<ScopedDb, AppError> {
    if auth.claims().is_some_and(|claims| {
        claims.iss.as_deref() == Some(crate::middleware::auth_middleware::API_KEY_ISSUER)
    }) {
        return scoped_db_for_org(state, auth, resolve_org_id(auth)?, scopes).await;
    }
    let actor_id = actor_user_id(auth)?;
    let org_ids = state
        .authz
        .user_org_ids(actor_id)
        .await
        .map_err(AppError::from)?;

    if org_ids.is_empty() {
        return Err(AppError::from(AuthzError::NotMember(Uuid::nil())));
    }

    let raw_org_ids: Vec<Uuid> = org_ids.iter().map(|id| id.into_uuid()).collect();
    scoped_db_for_orgs(state, auth, &raw_org_ids, scopes).await
}

// ---------------------------------------------------------------------------
// Unscoped (dev/test only)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn scope_strings_conversion() {
        let scopes = &["workspaces:read", "workspaces:write"];
        let strings: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        assert_eq!(strings, vec!["workspaces:read", "workspaces:write"]);
    }
}
