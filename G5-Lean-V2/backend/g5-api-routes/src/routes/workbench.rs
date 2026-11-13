//! Workbench configuration route (workbench `g5.workbench.actions`,
//! `g5.workbench.persona`).
//!
//! `GET /api/v1/workspaces/:workspaceId/workbench` serves the per-phase ribbon
//! action set and the phase personas resolved from ONE committed snapshot of
//! the workspace's docs branch over the shipped built-ins
//! (`services/agents/workbench_config.rs`). The handler is thin: authorize the
//! workspace read, then delegate. It never branches on the UI layout — the
//! configuration is data every layout may read.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::scoped::scoped_db_for_workspace;
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::AppState;
use g5_auth::Scope;

pub use g5_api_foundation::contracts::workbench::WorkbenchConfigDTO;

#[derive(Debug, Deserialize)]
pub struct WorkbenchPathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

/// Routes nested under `/api/v1/workspaces/:workspaceId/workbench`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_workbench_config), &[Scope::WorkspacesRead])
        .into_router()
}

/// `GET /api/v1/workspaces/:workspaceId/workbench`
///
/// Built-ins when nothing is committed, the committed override when it is
/// valid, or the built-ins plus a precise error when it is invalid; the
/// `revision` pins the snapshot a templated chat start is resolved against.
async fn get_workbench_config(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkbenchPathParams>,
) -> Result<Json<ResponseEnvelope<WorkbenchConfigDTO>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let config = state
        .workspace_service
        .load_workbench_config(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(config)))
}
