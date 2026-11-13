//! Permission route handlers for OAuth scope management.
//!
//! This module implements the HTTP route handlers for the Permissions API
//! (API_AND_STORAGE.md §3.16). Endpoints span 3 path prefixes for managing
//! OAuth scopes at org, user, and workspace levels:
//!
//! - `/api/v1/auth/me/scopes` — current user's effective scopes + scope check
//! - `/api/v1/orgs/:orgId/scopes` — org default scopes + user scope management
//! - `/api/v1/workspaces/:workspaceId/permissions` — workspace permission overrides
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to `PermissionService`, and format the response using `ResponseEnvelope`.
//! No business logic resides in the handlers.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{
    ensure_org_user_id, resolve_org_id, resolve_org_id_for_workspace, scoped_system_db_for_org,
    scoped_system_db_for_workspace,
};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::permission_service;
use crate::services::AppState;
use g5_auth::Scope;
use g5_authz::ScopedDb;

// ─── Path Parameter Types ──────────────────────────────────────────────────────

/// Path parameters for org-scoped endpoints (`/api/v1/orgs/:orgId/scopes`).
#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

/// Path parameters for org member-scoped endpoints
/// (`/api/v1/orgs/:orgId/members/:userId/scopes`).
#[derive(Debug, Deserialize)]
pub struct OrgMemberPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    #[serde(rename = "userId")]
    pub user_id: Uuid,
}

/// Path parameters for workspace-scoped endpoints
/// (`/api/v1/workspaces/:workspaceId/permissions`).
#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

/// Path parameters for workspace user-scoped endpoints
/// (`/api/v1/workspaces/:workspaceId/permissions/:userId`).
#[derive(Debug, Deserialize)]
pub struct WorkspaceUserPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "userId")]
    pub user_id: Uuid,
}

// ─── Query Parameter Types ─────────────────────────────────────────────────────

/// Query parameters for `GET /api/v1/auth/me/scopes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyScopesQuery {
    /// If provided, includes workspace-level permission overrides in the
    /// effective scope calculation.
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// ─── Request DTOs ──────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/auth/check`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCheckRequest {
    /// Array of scope strings to check (e.g., `["workspaces:write", "tasks:execute"]`).
    pub scopes: Vec<String>,
    /// If provided, includes workspace-level overrides in the check.
    pub workspace_id: Option<Uuid>,
}

/// Request body for `PUT /api/v1/orgs/:orgId/scopes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOrgScopesRequest {
    /// Full replacement set of org default scope strings.
    pub scopes: Vec<String>,
}

/// Request body for `PUT /api/v1/orgs/:orgId/members/:userId/scopes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetUserScopesRequest {
    /// Full replacement set of scope strings.
    pub scopes: Vec<String>,
    /// ISO 8601 timestamp. If provided, all grants expire at this time.
    /// If null/absent, grants are permanent.
    pub expires_at: Option<String>,
}

/// Request body for `PUT /api/v1/workspaces/:workspaceId/permissions/:userId`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetWorkspaceOverrideRequest {
    /// Full replacement set of scope strings.
    pub scopes: Vec<String>,
}

// ─── Response DTOs ─────────────────────────────────────────────────────────────

/// Response for `GET /api/v1/auth/me/scopes`.
/// Shows effective scopes with a breakdown of where each scope comes from.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveScopesResponse {
    pub scopes: Vec<String>,
    pub org_defaults: Vec<String>,
    pub user_grants: Vec<String>,
    pub workspace_overrides: Vec<String>,
}

/// Response for `POST /api/v1/auth/check`.
/// Returns whether the user has the requested scopes plus any missing ones.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCheckResponse {
    pub allowed: bool,
    pub missing: Vec<String>,
}

/// Response for org default scope endpoints.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgScopesResponse {
    pub org_id: String,
    pub scopes: Vec<String>,
}

/// Response for user scope endpoints.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserScopesResponse {
    pub org_id: String,
    pub user_id: String,
    pub grants: Vec<UserScopeGrantEntry>,
}

/// Individual user scope grant entry in the response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserScopeGrantEntry {
    pub id: String,
    pub scope: String,
    pub granted_by: Option<String>,
    pub granted_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Individual workspace permission override entry in the response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOverrideEntry {
    pub id: String,
    pub workspace_id: String,
    pub user_id: String,
    pub scope: String,
    pub granted_by: Option<String>,
    pub granted_at: String,
}

/// Response for `DELETE /api/v1/workspaces/:workspaceId/permissions/:userId`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveOverrideResponse {
    pub removed: bool,
}

// ─── From Implementations ──────────────────────────────────────────────────────

impl From<permission_service::EffectiveScopesDTO> for EffectiveScopesResponse {
    fn from(dto: permission_service::EffectiveScopesDTO) -> Self {
        EffectiveScopesResponse {
            scopes: dto.scopes,
            org_defaults: dto.org_defaults,
            user_grants: dto.user_grants,
            workspace_overrides: dto.workspace_overrides,
        }
    }
}

impl From<permission_service::ScopeCheckResult> for ScopeCheckResponse {
    fn from(dto: permission_service::ScopeCheckResult) -> Self {
        ScopeCheckResponse {
            allowed: dto.allowed,
            missing: dto.missing,
        }
    }
}

impl From<permission_service::UserScopeGrantDTO> for UserScopeGrantEntry {
    fn from(dto: permission_service::UserScopeGrantDTO) -> Self {
        UserScopeGrantEntry {
            id: dto.id.to_string(),
            scope: dto.scope,
            granted_by: dto.granted_by.map(|id| id.to_string()),
            granted_at: dto.granted_at,
            expires_at: dto.expires_at,
        }
    }
}

impl From<permission_service::WorkspaceOverrideDTO> for WorkspaceOverrideEntry {
    fn from(dto: permission_service::WorkspaceOverrideDTO) -> Self {
        WorkspaceOverrideEntry {
            id: dto.id.to_string(),
            workspace_id: dto.workspace_id.to_string(),
            user_id: dto.user_id.to_string(),
            scope: dto.scope,
            granted_by: dto.granted_by.map(|id| id.to_string()),
            granted_at: dto.granted_at,
        }
    }
}

// ─── Routers ───────────────────────────────────────────────────────────────────

/// Auth-scoped permission routes nested under `/api/v1/auth/me/scopes`.
///
/// Endpoints:
/// - `GET /` — get current user's effective scopes with breakdown
/// - `POST /check` — check whether the current user has a set of scopes
///
/// Both endpoints require only authentication — no specific scope.
pub fn my_scopes_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/",
            get(get_my_scopes),
            "Any authenticated user can view their own scopes",
        )
        .exempt_route(
            "/check",
            post(check_scopes),
            "Any authenticated user can check their own scopes",
        )
        .into_router()
}

/// Org-scoped permission routes nested under `/api/v1/orgs/:orgId/scopes`.
///
/// Endpoints:
/// - `GET /` — get org default scopes (requires `workspaces:read`)
/// - `PUT /` — set org default scopes (requires `org:admin`)
/// - `GET /members/:userId/scopes` — get user's scopes in org (requires `workspaces:read`)
/// - `PUT /members/:userId/scopes` — set user's scopes in org (requires `org:admin`)
pub fn org_scopes_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_org_default_scopes), &[Scope::WorkspacesRead])
        .audited_route("/", put(set_org_default_scopes), &[Scope::OrgAdmin])
        .audited_route(
            "/members/:userId/scopes",
            get(get_user_scopes),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/members/:userId/scopes",
            put(set_user_scopes),
            &[Scope::OrgAdmin],
        )
        .into_router()
}

/// Workspace-scoped permission routes nested under
/// `/api/v1/workspaces/:workspaceId/permissions`.
///
/// Endpoints:
/// - `GET /` — list all permission overrides for a workspace (requires `workspaces:read`)
/// - `PUT /:userId` — set per-workspace permission overrides (requires `org:admin`)
/// - `DELETE /:userId` — remove per-workspace overrides (requires `org:admin`)
pub fn workspace_permissions_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_workspace_overrides), &[Scope::WorkspacesRead])
        .audited_route(
            "/:userId",
            put(set_workspace_override).delete(remove_workspace_override),
            &[Scope::OrgAdmin],
        )
        .into_router()
}

// ─── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /api/v1/auth/me/scopes`
///
/// Returns the effective scopes for the currently authenticated user within
/// their current org context, with a breakdown of where each scope comes from
/// (org defaults, user grants, workspace overrides).
async fn get_my_scopes(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<MyScopesQuery>,
) -> Result<Json<ResponseEnvelope<EffectiveScopesResponse>>, AppError> {
    tracing::debug!("Permission endpoint called: get_my_scopes");

    let (mut db, org_id) =
        scoped_system_db_for_effective_scopes(&state, &auth, query.workspace_id).await?;
    let user_id = auth.require_user_id()?;

    let dto = state
        .permission_service
        .resolve_effective_scopes_detailed(&mut db, user_id, org_id, query.workspace_id)
        .await?;
    let response: EffectiveScopesResponse = dto.into();
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `POST /api/v1/auth/me/scopes/check`
///
/// Checks whether the currently authenticated user has a specific set of scopes.
/// Returns a boolean result with a list of any missing scopes.
///
/// **Note:** This endpoint always returns HTTP 200 — missing scopes are NOT an
/// error. The client decides what to do with the result.
async fn check_scopes(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<ScopeCheckRequest>,
) -> Result<Json<ResponseEnvelope<ScopeCheckResponse>>, AppError> {
    tracing::debug!("Permission endpoint called: check_scopes");

    let (mut db, org_id) =
        scoped_system_db_for_effective_scopes(&state, &auth, body.workspace_id).await?;
    let user_id = auth.require_user_id()?;

    let result = state
        .permission_service
        .check_scopes(&mut db, user_id, org_id, &body.scopes, body.workspace_id)
        .await?;
    let response: ScopeCheckResponse = result.into();
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `GET /api/v1/orgs/:orgId/scopes`
///
/// Lists the default scopes for an organization. These scopes are automatically
/// granted to every member of the org.
async fn get_org_default_scopes(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<OrgScopesResponse>>, AppError> {
    tracing::debug!(
        org_id = %params.org_id,
        "Permission endpoint called: get_org_default_scopes"
    );

    let org_id = resolve_org_id(&auth)?;
    if params.org_id != org_id {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec!["same-org".into()],
            },
        ));
    }
    let mut db = scoped_system_db_for_org(&state, &auth, org_id, &["workspaces:read"]).await?;

    let scopes = state
        .permission_service
        .get_org_default_scopes(&mut db, org_id)
        .await?;
    let response = OrgScopesResponse {
        org_id: org_id.to_string(),
        scopes,
    };
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `PUT /api/v1/orgs/:orgId/scopes`
///
/// Replaces the org's default scopes with the provided set. This is a full
/// replacement — not additive. Requires `org:admin` scope.
async fn set_org_default_scopes(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(body): Json<SetOrgScopesRequest>,
) -> Result<Json<ResponseEnvelope<OrgScopesResponse>>, AppError> {
    tracing::debug!(
        org_id = %params.org_id,
        "Permission endpoint called: set_org_default_scopes"
    );

    let org_id = resolve_org_id(&auth)?;
    if params.org_id != org_id {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec!["same-org".into()],
            },
        ));
    }
    let mut db = scoped_system_db_for_org(&state, &auth, org_id, &["org:admin"]).await?;

    let user_id = auth.require_user_id()?;

    let scopes = state
        .permission_service
        .set_org_default_scopes(&mut db, org_id, body.scopes, user_id)
        .await?;
    let response = OrgScopesResponse {
        org_id: org_id.to_string(),
        scopes,
    };
    tracing::info!(
        org_id = %org_id,
        set_by = %user_id,
        "Permission endpoint succeeded: set_org_default_scopes"
    );
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `GET /api/v1/orgs/:orgId/scopes/members/:userId/scopes`
///
/// Lists a user's additional scope grants within an org. Returns full grant
/// details including expiration and who granted them.
async fn get_user_scopes(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
) -> Result<Json<ResponseEnvelope<UserScopesResponse>>, AppError> {
    tracing::debug!(
        org_id = %params.org_id,
        user_id = %params.user_id,
        "Permission endpoint called: get_user_scopes"
    );

    let org_id = resolve_org_id(&auth)?;
    if params.org_id != org_id {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec!["same-org".into()],
            },
        ));
    }
    let mut db = scoped_system_db_for_org(&state, &auth, org_id, &["workspaces:read"]).await?;
    ensure_org_user_id(&mut db, org_id, params.user_id, "userId").await?;

    let grants = state
        .permission_service
        .get_user_scopes(&mut db, org_id, params.user_id)
        .await?;
    let response = UserScopesResponse {
        org_id: org_id.to_string(),
        user_id: params.user_id.to_string(),
        grants: grants.into_iter().map(UserScopeGrantEntry::from).collect(),
    };
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `PUT /api/v1/orgs/:orgId/scopes/members/:userId/scopes`
///
/// Replaces a user's scope grants within an org. Full replacement — not additive.
/// Requires `org:admin` scope.
async fn set_user_scopes(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgMemberPathParams>,
    Json(body): Json<SetUserScopesRequest>,
) -> Result<Json<ResponseEnvelope<UserScopesResponse>>, AppError> {
    tracing::debug!(
        org_id = %params.org_id,
        user_id = %params.user_id,
        "Permission endpoint called: set_user_scopes"
    );

    let org_id = resolve_org_id(&auth)?;
    if params.org_id != org_id {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec!["same-org".into()],
            },
        ));
    }
    let mut db = scoped_system_db_for_org(&state, &auth, org_id, &["org:admin"]).await?;
    ensure_org_user_id(&mut db, org_id, params.user_id, "userId").await?;

    let granted_by = auth.require_user_id()?;

    // Parse optional expires_at from ISO 8601 string to DateTime<Utc>.
    let expires_at = body
        .expires_at
        .as_ref()
        .map(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|_| {
                    AppError::from_error(g5_contracts::error::G5Error::ValidationError {
                        details: serde_json::json!({
                            "message": "Invalid expiresAt format. Expected ISO 8601."
                        }),
                    })
                })
        })
        .transpose()?;

    let grants = state
        .permission_service
        .set_user_scopes(
            &mut db,
            org_id,
            params.user_id,
            body.scopes,
            expires_at,
            granted_by,
        )
        .await?;
    let response = UserScopesResponse {
        org_id: org_id.to_string(),
        user_id: params.user_id.to_string(),
        grants: grants.into_iter().map(UserScopeGrantEntry::from).collect(),
    };
    tracing::info!(
        org_id = %org_id,
        user_id = %params.user_id,
        granted_by = %granted_by,
        "Permission endpoint succeeded: set_user_scopes"
    );
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `GET /api/v1/workspaces/:workspaceId/permissions`
///
/// Lists all permission overrides for a workspace, showing per-user scope
/// additions. Returns an empty array (not an error) if no overrides exist.
async fn list_workspace_overrides(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<Vec<WorkspaceOverrideEntry>>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Permission endpoint called: list_workspace_overrides"
    );

    let mut db =
        scoped_system_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"])
            .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();

    let overrides = state
        .permission_service
        .get_workspace_overrides(&mut db, org_id, params.workspace_id)
        .await?;
    let entries: Vec<WorkspaceOverrideEntry> = overrides
        .into_iter()
        .map(WorkspaceOverrideEntry::from)
        .collect();
    Ok(Json(ResponseEnvelope::ok(entries)))
}

/// `PUT /api/v1/workspaces/:workspaceId/permissions/:userId`
///
/// Sets per-workspace permission overrides for a specific user. Replaces all
/// existing overrides for the (workspace, user) pair. Requires `org:admin` scope.
async fn set_workspace_override(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceUserPathParams>,
    Json(body): Json<SetWorkspaceOverrideRequest>,
) -> Result<Json<ResponseEnvelope<Vec<WorkspaceOverrideEntry>>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        user_id = %params.user_id,
        "Permission endpoint called: set_workspace_override"
    );

    let mut db =
        scoped_system_db_for_workspace(&state, &auth, params.workspace_id, &["org:admin"]).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    ensure_org_user_id(&mut db, org_id, params.user_id, "userId").await?;

    let granted_by = auth.require_user_id()?;

    let overrides = state
        .permission_service
        .set_workspace_override(
            &mut db,
            org_id,
            params.workspace_id,
            params.user_id,
            body.scopes,
            granted_by,
        )
        .await?;
    let entries: Vec<WorkspaceOverrideEntry> = overrides
        .into_iter()
        .map(WorkspaceOverrideEntry::from)
        .collect();
    tracing::info!(
        workspace_id = %params.workspace_id,
        user_id = %params.user_id,
        granted_by = %granted_by,
        "Permission endpoint succeeded: set_workspace_override"
    );
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(entries)))
}

/// `DELETE /api/v1/workspaces/:workspaceId/permissions/:userId`
///
/// Removes all per-workspace permission overrides for a specific user,
/// reverting them to org-level permissions. Idempotent — succeeds even if
/// no overrides exist.
async fn remove_workspace_override(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceUserPathParams>,
) -> Result<Json<ResponseEnvelope<RemoveOverrideResponse>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        user_id = %params.user_id,
        "Permission endpoint called: remove_workspace_override"
    );

    let mut db =
        scoped_system_db_for_workspace(&state, &auth, params.workspace_id, &["org:admin"]).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    ensure_org_user_id(&mut db, org_id, params.user_id, "userId").await?;

    let _granted_by = auth.require_user_id()?;

    state
        .permission_service
        .remove_workspace_override(&mut db, org_id, params.workspace_id, params.user_id)
        .await?;
    tracing::info!(
        workspace_id = %params.workspace_id,
        user_id = %params.user_id,
        "Permission endpoint succeeded: remove_workspace_override"
    );
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(RemoveOverrideResponse {
        removed: true,
    })))
}

// ─── Helpers ───────────────────────────────────────────────────────────────────

async fn scoped_system_db_for_effective_scopes(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Option<Uuid>,
) -> Result<(ScopedDb, Uuid), AppError> {
    if let Some(workspace_id) = workspace_id {
        let mut db =
            scoped_system_db_for_workspace(state, auth, workspace_id, &["workspaces:read"]).await?;
        let org_id = resolve_org_id_for_workspace(&mut db, workspace_id).await?;
        return Ok((db, org_id));
    }

    let org_id = resolve_org_id(auth)?;
    let db = scoped_system_db_for_org(state, auth, org_id, &["workspaces:read"]).await?;
    Ok((db, org_id))
}
