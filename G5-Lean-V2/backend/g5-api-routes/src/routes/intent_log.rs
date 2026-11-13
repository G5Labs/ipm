//! Public, read-only **Intent Log** projection endpoint.
//!
//! Spec: `g5/g5-api/routes/intent_log`. Mirrors the grounding/trace route's
//! branch-scoping and overlay-aware resolution, delegating all projection logic
//! to [`crate::services::IntentLogService`].
//!
//! Route:
//! - `GET /api/v1/intent-log`

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};

use crate::db::scoped::scoped_db_for_workspace_repo;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::grounding::capture_grounding_read_set;
use crate::services::{AppState, IntentLogProjection, IntentLogQuery, IntentLogResponse};
use g5_auth::Scope;

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_intent_log), &[Scope::WorkspacesRead])
        .into_router()
}

/// `GET /api/v1/intent-log?workspaceId=…`
///
/// Returns the normalized, filtered, paginated Intent Log projection for the
/// requested workspace branch. Authorization is branch-level (`workspaces:read`),
/// matching `/api/v1/grounding`. Malformed sources yield partial rows plus
/// diagnostics in `data.meta.diagnostics` rather than failing the request.
async fn get_intent_log(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<IntentLogQuery>,
) -> Result<Json<crate::routes::ResponseEnvelope<IntentLogResponse>>, AppError> {
    let workspace_id = query.workspace_id;

    // Branch-scoped tenant DB + access enforcement (same path as grounding).
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        workspace_id,
        query.repo_id,
        &["workspaces:read"],
    )
    .await?;

    let captured = capture_grounding_read_set(&state, &mut db, workspace_id, query.repo_id).await?;
    let projection = state
        .intent_log_service
        .project(&captured.repo_contexts, &captured.repo_views, &query)
        .await?;
    let IntentLogProjection { response, .. } = projection;

    Ok(Json(crate::routes::ResponseEnvelope::ok(response)))
}
