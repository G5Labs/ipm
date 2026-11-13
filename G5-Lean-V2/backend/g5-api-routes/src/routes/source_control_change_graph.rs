//! Read-only workspace Source Control current-change graph route.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::scoped::{scoped_db_for_workspace, scoped_db_for_workspace_repo};
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::{
    AppState, ChangeGraphComparison, ChangeGraphNodeDetailsResponse, ChangeGraphResponse,
};
use g5_auth::Scope;

#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceNodePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
    #[serde(rename = "nodeId")]
    pub node_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangeGraphQuery {
    #[serde(default)]
    pub comparison: ChangeGraphComparison,
    /// "Human intent" lens: when true, filter the graph to nodes whose diff
    /// contains at least one human-tagged (`is_human`) changed I/D record.
    #[serde(rename = "humanIntent", default)]
    pub human_intent: bool,
}

#[derive(Debug, Deserialize)]
pub struct NodeDetailsQuery {
    pub revision: String,
    #[serde(default)]
    pub comparison: ChangeGraphComparison,
    #[serde(rename = "humanIntent", default)]
    pub human_intent: bool,
}

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/ontology-baseline",
            get(get_ontology_baseline),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/change-graph",
            get(get_change_graph),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/change-graph/nodes/:nodeId/details",
            get(get_node_details),
            &[Scope::WorkspacesRead],
        )
        .into_router()
        .merge(super::ontology_changes::router())
}

/// Saved edits are captured for All changes only; the projection refuses
/// Uncommitted only before reading anything.
async fn live_content(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    comparison: ChangeGraphComparison,
) -> Result<Option<super::change_graph_live::LiveCapture>, AppError> {
    if comparison != ChangeGraphComparison::Workspace {
        return Ok(None);
    }
    super::change_graph_live::capture(state, user, auth, workspace_id)
        .await
        .map(Some)
}

async fn get_change_graph(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<ChangeGraphQuery>,
) -> Result<Json<ResponseEnvelope<ChangeGraphResponse>>, AppError> {
    let live = live_content(&state, &user, &auth, params.workspace_id, query.comparison).await?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let graph = state
        .source_control_change_graph_service
        .project(
            &mut db,
            params.workspace_id,
            query.comparison,
            query.human_intent,
            live.as_ref()
                .map(|live| &live.content)
                .unwrap_or(&Default::default()),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    if let Some(live) = &live {
        live.recheck(&state, &auth, params.workspace_id).await?;
    }
    Ok(Json(ResponseEnvelope::ok(graph)))
}

async fn get_node_details(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspaceNodePathParams>,
    Query(query): Query<NodeDetailsQuery>,
) -> Result<Json<ResponseEnvelope<ChangeGraphNodeDetailsResponse>>, AppError> {
    let live = live_content(&state, &user, &auth, params.workspace_id, query.comparison).await?;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let details = state
        .source_control_change_graph_service
        .node_details(
            &mut db,
            params.workspace_id,
            &params.node_id,
            &query.revision,
            query.comparison,
            query.human_intent,
            live.as_ref()
                .map(|live| &live.content)
                .unwrap_or(&Default::default()),
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    if let Some(live) = &live {
        live.recheck(&state, &auth, params.workspace_id).await?;
    }
    Ok(Json(ResponseEnvelope::ok(details)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OntologyBaselineQuery {
    repo_id: Uuid,
    path: String,
}

async fn get_ontology_baseline(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<OntologyBaselineQuery>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::source_control_change_graph_service::OntologyComparisonBaseline,
        >,
    >,
    AppError,
> {
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        params.workspace_id,
        query.repo_id,
        &["workspaces:read"],
    )
    .await?;
    let repositories = state
        .source_control_change_graph_service
        .ontology_comparison_repositories(&mut db, params.workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    let result = state
        .source_control_change_graph_service
        .ontology_baseline(
            &repositories,
            params.workspace_id,
            query.repo_id,
            &query.path,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}
