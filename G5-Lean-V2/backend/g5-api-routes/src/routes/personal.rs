//! Personal workspace routes and membership hooks (`g5.personal.*`,
//! G5-Lean#624).
//!
//! - `GET /api/v1/orgs/:orgId/me` — the caller's personal workspace, created
//!   first if it is missing (`g5.personal.routes`).
//! - `GET /api/v1/orgs/:orgId/users/:handle/workspace` — a personal workspace
//!   by handle, for anyone its read rule admits (`g5.personal.access`).
//!
//! OrgService coordinates personal retirement inside the membership transaction.
//! Routes call [`after_retirement`] / [`after_membership_gain`] after commit.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{scoped_db_for_org, scoped_db_for_workspace};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::ResponseEnvelope;
use crate::services::personal_workspace::{self, PersonalWorkspace, RetiredPersonalWorkspace};
use crate::services::AppState;
use g5_contracts::error::G5Error;

/// `GET /api/v1/orgs/:orgId/me`, nested at that path.
pub fn me_router() -> Router<Arc<AppState>> {
    crate::routes::audited::AuditedRouter::new()
        .audited_route(
            "/",
            get(get_my_personal_workspace),
            &[g5_auth::Scope::WorkspacesRead],
        )
        .into_router()
}

/// `GET /api/v1/orgs/:orgId/users/:handle/workspace`, nested at
/// `/api/v1/orgs/:orgId/users`.
pub fn users_router() -> Router<Arc<AppState>> {
    crate::routes::audited::AuditedRouter::new()
        .audited_route(
            "/:handle/workspace",
            get(get_personal_workspace_by_handle),
            &[g5_auth::Scope::WorkspacesRead],
        )
        .into_router()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalOrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct PersonalHandlePathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
    pub handle: String,
}

/// A personal workspace's identity.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalWorkspaceDTO {
    pub workspace_id: String,
    pub org_id: String,
    pub user_id: String,
    pub repo_id: String,
    pub name: String,
    pub handle: String,
}

impl From<PersonalWorkspace> for PersonalWorkspaceDTO {
    fn from(record: PersonalWorkspace) -> Self {
        Self {
            handle: record.handle().to_string(),
            workspace_id: record.workspace_id.to_string(),
            org_id: record.org_id.to_string(),
            user_id: record.user_id.to_string(),
            repo_id: record.repo_id.to_string(),
            name: record.name,
        }
    }
}

/// `GET /api/v1/orgs/:orgId/me`
///
/// Ensures and returns the caller's personal workspace. Viewers, who cannot
/// own workspaces, are refused.
pub async fn get_my_personal_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<PersonalOrgPathParams>,
) -> Result<Json<ResponseEnvelope<PersonalWorkspaceDTO>>, AppError> {
    let user_id = auth.require_user_id()?;
    // Admits only a current member of the organization.
    let db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    db.rollback().await.map_err(AppError::from)?;
    let record = personal_workspace::ensure(&state, params.org_id, user_id)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::Forbidden {
                missing_scopes: vec!["workspace_owner_eligible".to_string()],
            })
        })?;
    Ok(Json(ResponseEnvelope::ok(record.into())))
}

/// `GET /api/v1/orgs/:orgId/users/:handle/workspace`
///
/// Resolves a personal workspace by handle. The owner and current
/// organization administrators may read it; anyone else gets not-found.
pub async fn get_personal_workspace_by_handle(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<PersonalHandlePathParams>,
) -> Result<Json<ResponseEnvelope<PersonalWorkspaceDTO>>, AppError> {
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let record = personal_workspace::find_by_handle(db.conn(), params.org_id, &params.handle)
        .await?
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: "personal workspace".to_string(),
            })
        })?;
    db.rollback().await.map_err(AppError::from)?;
    // The workspace read rule decides; a refusal reads as not-found.
    let workspace_id = record.workspace_id;
    match scoped_db_for_workspace(&state, &auth, workspace_id, &["workspaces:read"]).await {
        Ok(workspace_db) => {
            workspace_db.rollback().await.map_err(AppError::from)?;
        }
        Err(_) => {
            return Err(AppError::from_error(G5Error::NotFound {
                resource: "personal workspace".to_string(),
            }));
        }
    }
    Ok(Json(ResponseEnvelope::ok(record.into())))
}

/// After the retiring membership change commits, stop the retired workspace's
/// VM in the background. A failure is left to the lifecycle reconciler.
pub fn after_retirement(
    state: &Arc<AppState>,
    org_id: Uuid,
    retired: Option<RetiredPersonalWorkspace>,
) {
    personal_workspace::stop_after_retirement(state, org_id, retired);
}

/// After a membership change that makes `user_id` an eligible owner commits,
/// create their personal workspace. A failure is logged, never returned: the
/// change has already succeeded, and the reconciler and `/{org}/me` retry.
pub async fn after_membership_gain(state: &Arc<AppState>, org_id: Uuid, user_id: Uuid) {
    personal_workspace::ensure_after_membership_change(state, org_id, user_id).await;
}
