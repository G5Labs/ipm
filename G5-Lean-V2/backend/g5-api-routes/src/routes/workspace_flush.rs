//! Incarnation-authenticated lifecycle endpoints for receipt-first flushes.

use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use g5_contracts::types::WorkspaceId;
use g5_internal_api::workspace_flush::{
    FlushCompleteRequest, FlushFailureRequest, FlushStartRequest,
};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::WorkspaceIncarnationContext;
use crate::services::workspace_flush_service::{WorkspaceFlushError, WorkspaceFlushService};
use crate::services::AppState;

pub fn router() -> Router<Arc<AppState>> {
    // These internal routes use the workspace-incarnation principal installed
    // by the global auth middleware, rather than an API/OAuth scope layer.
    Router::new()
        .route("/:workspaceId/flushes/start", post(start))
        .route("/:workspaceId/flushes/:flushId/complete", post(complete))
        .route("/:workspaceId/flushes/:flushId/fail", post(fail))
}

async fn start(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(WorkspaceIncarnationContext(claims)): axum::extract::Extension<
        WorkspaceIncarnationContext,
    >,
    AxumPath(workspace_id): AxumPath<WorkspaceId>,
    Json(request): Json<FlushStartRequest>,
) -> Result<impl IntoResponse, FlushRouteError> {
    let workspace_id = workspace_id.into_uuid();
    require_workspace(&claims, workspace_id)?;
    let receipt = WorkspaceFlushService::new(
        state.system_pool.clone(),
        state.repo_storage.clone(),
        state.git_bin.clone(),
    )
    .start(&claims, request.reason, &request.idempotency_key)
    .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn complete(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(WorkspaceIncarnationContext(claims)): axum::extract::Extension<
        WorkspaceIncarnationContext,
    >,
    AxumPath((workspace_id, flush_id)): AxumPath<(WorkspaceId, Uuid)>,
    Json(request): Json<FlushCompleteRequest>,
) -> Result<impl IntoResponse, FlushRouteError> {
    let workspace_id = workspace_id.into_uuid();
    require_workspace(&claims, workspace_id)?;
    tracing::warn!(%workspace_id, %flush_id, "workspace flush completion received");
    let receipt = WorkspaceFlushService::new(
        state.system_pool.clone(),
        state.repo_storage.clone(),
        state.git_bin.clone(),
    )
    .complete(&claims, flush_id, request.repos)
    .await?;
    tracing::warn!(%workspace_id, %flush_id, "workspace flush completion receipt committed");
    state.event_bus.publish(
        &crate::ws::Channel::Workspace(workspace_id),
        &crate::ws::WsEvent::WorkspaceFlushCompleted {
            workspace_id,
            flush_id,
        },
    );
    Ok((StatusCode::OK, Json(receipt)))
}

async fn fail(
    State(state): State<Arc<AppState>>,
    axum::extract::Extension(WorkspaceIncarnationContext(claims)): axum::extract::Extension<
        WorkspaceIncarnationContext,
    >,
    AxumPath((workspace_id, flush_id)): AxumPath<(WorkspaceId, Uuid)>,
    Json(request): Json<FlushFailureRequest>,
) -> Result<impl IntoResponse, FlushRouteError> {
    let workspace_id = workspace_id.into_uuid();
    require_workspace(&claims, workspace_id)?;
    let result = WorkspaceFlushService::new(
        state.system_pool.clone(),
        state.repo_storage.clone(),
        state.git_bin.clone(),
    )
    .fail(&claims, flush_id, &request)
    .await?;
    Ok((StatusCode::OK, Json(result)))
}

fn require_workspace(
    claims: &g5_auth::WorkspaceIncarnationClaims,
    workspace_id: Uuid,
) -> Result<(), FlushRouteError> {
    if claims.workspace_id != workspace_id {
        return Err(FlushRouteError::Unauthorized);
    }
    Ok(())
}

enum FlushRouteError {
    Unauthorized,
    Service(WorkspaceFlushError),
}

impl From<WorkspaceFlushError> for FlushRouteError {
    fn from(value: WorkspaceFlushError) -> Self {
        Self::Service(value)
    }
}

impl IntoResponse for FlushRouteError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            Self::Service(WorkspaceFlushError::Proof(code)) => (StatusCode::CONFLICT, code),
            Self::Service(WorkspaceFlushError::GitUnavailable(message)) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error": {"code": "FLUSH_UNAVAILABLE", "message": message}})),
                )
                    .into_response();
            }
            Self::Service(WorkspaceFlushError::StaleIncarnation) => {
                (StatusCode::CONFLICT, "STALE_INCARNATION")
            }
            Self::Service(WorkspaceFlushError::MemberVector) => {
                (StatusCode::CONFLICT, "FLUSH_INCOMPLETE_VECTOR")
            }
            Self::Service(WorkspaceFlushError::FlushInProgress) => {
                (StatusCode::CONFLICT, "FLUSH_IN_PROGRESS")
            }
            Self::Service(WorkspaceFlushError::CompletionConflict) => {
                (StatusCode::CONFLICT, "FLUSH_COMPLETION_CONFLICT")
            }
            Self::Service(WorkspaceFlushError::NotFound) => {
                (StatusCode::NOT_FOUND, "FLUSH_REF_UNKNOWN")
            }
            Self::Service(WorkspaceFlushError::Invalid(_)) => {
                (StatusCode::BAD_REQUEST, "INVALID_FLUSH_REQUEST")
            }
            Self::Service(WorkspaceFlushError::Database(error)) => {
                // The guest retries a 503 blindly; the cause is only visible here.
                tracing::error!(error = %error, "workspace flush database error");
                (StatusCode::SERVICE_UNAVAILABLE, "FLUSH_UNAVAILABLE")
            }
        };
        (status, Json(json!({"error":{"code":code}}))).into_response()
    }
}
