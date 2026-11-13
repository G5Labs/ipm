//! Preview profile route handlers (`g5/g5-api/routes/preview_profiles`).
//!
//! Named preview profiles (`g5/g5-common/types` D18) let a workspace declare
//! several runnable apps — each a committed `.g5/preview_profiles/<name>.yaml`
//! with its own `command`/`port`/`working_dir`/`env`/`cleanup_command` — so the preview launcher
//! can pick which one a `preview` task boots. Three branch-scoped endpoints:
//!
//! 1. `GET /preview-applications` — discover valid repo-local launch configs.
//! 2. `GET /preview-profiles` — list committed named profiles (read-only).
//! 3. `POST /preview-profiles` — create a profile by COMMITTING its YAML.
//!
//! A preview can only boot from a committed ref, so creation is commit-based —
//! there is no draft variant (chat intent C5: "commit and preview"). Handlers
//! are thin: resolve the branch, then delegate to `WorkspaceService`.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace_repo;
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::workspace_mutation_policy::ensure_workspace_mutable;
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::error::G5Error;

pub use g5_api_foundation::contracts::preview_profiles::{
    CommitPreviewProfileResult, PreviewApplicationDTO, PreviewProfileDTO, PreviewProfileInput,
};

// ─── Path Parameters ───────────────────────────────────────────────────────

/// Path parameters for the preview-profiles endpoints.
#[derive(Debug, Deserialize)]
pub struct BranchPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
}

/// Query parameters for the list endpoint. `mode` selects which ref to list
/// profiles from, mirroring the preview a launch in that mode would boot:
/// `unmerged` (default) → the workspace branch HEAD; `merged` → the base branch
/// HEAD. Listing from the previewed ref keeps the picker consistent with what
/// task-admission validation and the host will actually resolve.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProfilesQuery {
    #[serde(default)]
    pub mode: Option<String>,
    /// Applications only: `live` (default) discovers from the attached
    /// owner's checkout; `committed` lists what the previewed ref commits,
    /// the source a start while the VM is not running records
    /// (`g5.preview.start-and-preview`).
    #[serde(default)]
    pub source: Option<String>,
}

// ─── DTOs ──────────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/workspaces/:workspaceId/repos/:repoId/preview-profiles`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePreviewProfileBody {
    pub name: String,
    pub command: String,
    pub port: u16,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional shell command the guest runs after the app stops.
    #[serde(default)]
    pub cleanup_command: Option<String>,
    /// CAS guard: the branch head the client based this create on. When set and
    /// stale, the create is rejected so a concurrent advance is not clobbered.
    #[serde(default)]
    pub expected_head_commit: Option<String>,
    /// When false (default), a name collision at head is a conflict.
    #[serde(default)]
    pub overwrite: bool,
}

// ─── Router ────────────────────────────────────────────────────────────────

/// Preview-profile routes nested under
/// `/api/v1/workspaces/:workspaceId/repos/:repoId/preview-profiles`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_preview_profiles), &[Scope::WorkspacesRead])
        .audited_route("/", post(create_preview_profile), &[Scope::WorkspacesWrite])
        .into_router()
}

/// Read-only discovery route nested under
/// `/api/v1/workspaces/:workspaceId/repos/:repoId/preview-applications`.
pub fn applications_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/",
            get(list_preview_applications),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/repos/:repoId/preview-profiles`
///
/// Lists the branch's committed preview profiles (newest-name-sorted). Returns
/// an empty array when the workspace declares none.
async fn list_preview_profiles(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Query(query): Query<ListProfilesQuery>,
) -> Result<Json<ResponseEnvelope<Vec<PreviewProfileDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let _claims = extract_user_claims(&auth)?;

    // `merged` lists from the base branch (what a merged preview boots); any
    // other value (incl. the default) lists from the workspace branch HEAD.
    let merged = query.mode.as_deref() == Some("merged");

    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let profiles = state
        .workspace_service
        .list_preview_profiles(&mut db, &branch, merged)
        .await?;

    Ok(Json(ResponseEnvelope::ok(profiles)))
}

/// `GET /api/v1/workspaces/:workspaceId/repos/:repoId/preview-applications`
///
/// Lists valid workspace-default and named-profile configurations from the
/// attached owner's live checkout, which is also the source used at launch.
async fn list_preview_applications(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Query(query): Query<ListProfilesQuery>,
) -> Result<Json<ResponseEnvelope<Vec<PreviewApplicationDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let applications = match query.source.as_deref() {
        None | Some("live") => {
            state
                .task_service
                .list_live_preview_applications(
                    branch.workspace_id,
                    user.user_uuid(),
                    branch.repo_id,
                )
                .await?
        }
        Some("committed") => {
            let merged = query.mode.as_deref() == Some("merged");
            state
                .workspace_service
                .list_committed_preview_applications(&mut db, &branch, merged)
                .await?
        }
        Some(other) => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "source",
                    "message": format!("unknown preview application source `{other}`"),
                }),
            }))
        }
    };
    Ok(Json(ResponseEnvelope::ok(applications)))
}

/// `POST /api/v1/workspaces/:workspaceId/repos/:repoId/preview-profiles`
///
/// Creates a preview profile by committing `.g5/preview_profiles/<name>.yaml`
/// to the branch. Validates name + shape before any git I/O.
async fn create_preview_profile(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Json(body): Json<CreatePreviewProfileBody>,
) -> Result<Json<ResponseEnvelope<CommitPreviewProfileResult>>, AppError> {
    let db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:write"],
    )
    .await?;
    let user_id = auth.require_user_id()?;
    let result = create_preview_profile_with(
        &state,
        db,
        params.workspace_id,
        params.repo_id,
        user_id,
        body,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

/// Validate and save a preview profile into `workspace_id`'s live checkout as
/// `user_id`, its owner, on a scope already admitted for the repository.
/// Shared with the workspace agent.
pub(crate) async fn create_preview_profile_with(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
    user_id: Uuid,
    body: CreatePreviewProfileBody,
) -> Result<CommitPreviewProfileResult, AppError> {
    // Validate the name as a safe file-stem and the shape, before any git work,
    // so the client gets precise field-level errors (the service re-validates as
    // defense in depth).
    g5_contracts::types::validate_preview_profile_name(&body.name).map_err(|e| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "name", "message": e }),
        })
    })?;
    if body.command.trim().is_empty() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "command", "message": "command must be non-empty" }),
        }));
    }
    if !(1..=65535).contains(&body.port) {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "port", "message": "port must be in 1..=65535" }),
        }));
    }
    if body
        .cleanup_command
        .as_deref()
        .is_some_and(|command| command.trim().is_empty())
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "cleanupCommand", "message": "cleanupCommand must be non-empty when set" }),
        }));
    }

    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, workspace_id, repo_id)
        .await?;

    // Saving a preview profile mutates the attached owner's live checkout.
    // A later workspace flush advances Git. This route guard is the
    // authoritative read-only enforcement boundary.
    ensure_workspace_mutable(&mut db, branch.workspace_id, "preview_profiles.create").await?;

    let input = PreviewProfileInput {
        name: body.name,
        command: body.command,
        port: body.port,
        working_dir: body.working_dir,
        env: body.env,
        description: body.description,
        cleanup_command: body.cleanup_command,
        expected_head_commit: body.expected_head_commit,
        overwrite: body.overwrite,
    };

    let result = state
        .workspace_service
        .save_preview_profile(
            db,
            &branch,
            user_id,
            input,
            state.workspace_runtime.as_ref(),
        )
        .await?;

    Ok(result)
}

/// Rejects non-user (service) tokens, mirroring `files.rs`.
fn extract_user_claims(auth: &AuthContext) -> Result<&g5_auth::JwtClaims, AppError> {
    match auth {
        AuthContext::User(claims) => Ok(claims),
        _ => Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["user_auth_required".into()],
        })),
    }
}
