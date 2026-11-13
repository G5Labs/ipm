//! Explainer template route handlers (`g5/g5-api/routes/explainer_templates`).
//!
//! Explainer style templates are the prompt briefs the Create Explainer modal
//! offers as starting points (plan `explainer-create-cutover` v1). Six
//! built-ins ship as `g5-common` constants (`g5/g5-common/types/core` D26);
//! saved templates are committed `.g5/explainer_templates/<slug>.yaml` files
//! (YAML shape `{ name, prompt }`) — a structural clone of the
//! preview-profiles committed-per-file config pattern. Two endpoints under
//! `/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates`:
//!
//! 1. `GET  /` — list built-ins merged with the branch's committed saved
//!    templates (read-only).
//! 2. `POST /` — save a template by COMMITTING the YAML to the branch.
//!
//! Handlers are thin: resolve the branch, then delegate to
//! `WorkspaceService`.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace_repo;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::workspace_mutation_policy::ensure_workspace_mutable;
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::error::G5Error;
use g5_contracts::types::BUILT_IN_EXPLAINER_TEMPLATES;

pub use g5_api_foundation::contracts::explainer_templates::{
    CommitExplainerTemplateResult, ExplainerTemplateDTO, ExplainerTemplateInput,
    ExplainerTemplateSource, SavedExplainerTemplate,
};

// ─── Path Parameters ───────────────────────────────────────────────────────

/// Path parameters for the explainer-templates endpoints.
#[derive(Debug, Deserialize)]
pub struct BranchPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
}

// ─── DTOs ──────────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateExplainerTemplateBody {
    pub name: String,
    pub slug: String,
    pub prompt: String,
    /// CAS guard: the branch head the client based this save on. When set and
    /// stale, the save is rejected so a concurrent advance is not clobbered.
    #[serde(default)]
    pub expected_head_commit: Option<String>,
    /// When false (default), a slug collision at head is a conflict.
    #[serde(default)]
    pub overwrite: bool,
}

// ─── Router ────────────────────────────────────────────────────────────────

/// Explainer-template routes nested under
/// `/api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_explainer_templates), &[Scope::WorkspacesRead])
        .audited_route(
            "/",
            post(create_explainer_template),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates`
///
/// Lists the six built-ins (catalog order, `source: "builtin"`) followed by
/// the branch's committed saved templates (`source: "saved"`, slug-sorted).
/// Built-ins always list, even when the branch has no saved templates.
async fn list_explainer_templates(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<ExplainerTemplateDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let _claims = extract_user_claims(&auth)?;

    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;
    let saved = state
        .workspace_service
        .list_explainer_templates(&mut db, &branch)
        .await?;

    let mut templates: Vec<ExplainerTemplateDTO> = BUILT_IN_EXPLAINER_TEMPLATES
        .iter()
        .map(|t| ExplainerTemplateDTO {
            name: t.name.to_string(),
            slug: t.slug.to_string(),
            prompt: t.prompt.to_string(),
            source: ExplainerTemplateSource::Builtin,
        })
        .collect();
    templates.extend(saved.into_iter().map(ExplainerTemplateDTO::from));

    Ok(Json(ResponseEnvelope::ok(templates)))
}

/// `POST /api/v1/workspaces/:workspaceId/repos/:repoId/explainer-templates`
///
/// Saves a template by committing `.g5/explainer_templates/<slug>.yaml` to
/// the branch. Validates slug + shape before any git I/O; reserved built-in
/// slugs are always rejected (422), regardless of `overwrite`.
async fn create_explainer_template(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<BranchPathParams>,
    Json(body): Json<CreateExplainerTemplateBody>,
) -> Result<Json<ResponseEnvelope<CommitExplainerTemplateResult>>, AppError> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        params.repo_id,
        &["workspaces:write"],
    )
    .await?;
    let user_id = auth.require_user_id()?;

    // Validate the slug as a safe kebab file-stem that is not reserved, plus
    // the shape, before any git work, so the client gets precise field-level
    // errors (the service re-validates as defense in depth).
    g5_contracts::types::validate_explainer_template_slug_for_save(&body.slug).map_err(|e| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "slug", "message": e }),
        })
    })?;
    if body.name.trim().is_empty() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "name", "message": "name must be non-empty" }),
        }));
    }
    if body.prompt.trim().is_empty() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({ "field": "prompt", "message": "prompt must be non-empty" }),
        }));
    }

    let branch = state
        .workspace_service
        .get_workspace_repository(&mut db, params.workspace_id, params.repo_id)
        .await?;

    // Saving a template mutates the attached owner's live checkout. A later
    // workspace flush advances Git. This route guard is the authoritative
    // read-only enforcement boundary.
    ensure_workspace_mutable(&mut db, branch.workspace_id, "explainer_templates.create").await?;

    let input = ExplainerTemplateInput {
        name: body.name,
        slug: body.slug,
        prompt: body.prompt,
        expected_head_commit: body.expected_head_commit,
        overwrite: body.overwrite,
    };

    let result = state
        .workspace_service
        .save_explainer_template(
            db,
            &branch,
            user_id,
            input,
            state.workspace_runtime.as_ref(),
        )
        .await?;

    Ok(Json(ResponseEnvelope::ok(result)))
}

/// Rejects non-user (service) tokens, mirroring `preview_profiles.rs`.
fn extract_user_claims(auth: &AuthContext) -> Result<&g5_auth::JwtClaims, AppError> {
    match auth {
        AuthContext::User(claims) => Ok(claims),
        _ => Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["user_auth_required".into()],
        })),
    }
}
