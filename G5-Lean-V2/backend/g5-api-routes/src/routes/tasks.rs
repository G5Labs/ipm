//! Public task history and owner-controlled workspace-runtime actions.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::db::scoped::{scoped_db_for_task, scoped_db_for_task_view};
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::task_lifecycle::{NativeReplyInput, NativeReplyOutcome};
use crate::services::AppState;
use g5_auth::Scope;
use g5_contracts::types::ids::TaskId;

pub use g5_api_foundation::contracts::tasks::{
    TaskDTO, TaskExecutionAttemptDTO, TaskFileDiff, TimelineEntryDTO,
};

pub fn task_detail_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/:id", get(get_task), &[Scope::TasksRead])
        .audited_route("/:id/attempts", get(get_task_attempts), &[Scope::TasksRead])
        .audited_route("/:id/timeline", get(get_task_timeline), &[Scope::TasksRead])
        .audited_route("/:id/continue", post(continue_task), &[Scope::TasksExecute])
        .audited_route("/:id/cancel", post(cancel_task), &[Scope::TasksExecute])
        .audited_route(
            "/:id/provider-requests/:providerRequestId/reply",
            post(reply_to_provider_request),
            &[Scope::TasksExecute],
        )
        .into_router()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptHistoryQuery {
    #[serde(default)]
    pub cursor: Option<i32>,
    #[serde(default = "default_page_limit")]
    pub limit: i64,
}

fn default_page_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestReplyPath {
    pub id: TaskId,
    pub provider_request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequestReplyRequest {
    pub attempt: i32,
    pub request_id: String,
    pub validated_response: serde_json::Value,
    /// Start and answer (`g5.chat.start-and-send`): record Start consent and
    /// accept this answer for the held turn in one owner request.
    #[serde(default)]
    pub start_compute: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestReplyResponse {
    #[serde(flatten)]
    pub outcome: NativeReplyOutcome,
    /// The Start a `startCompute: true` reply recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_compute: Option<crate::routes::start_and_send::StartComputeDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineResponse {
    pub entries: Vec<TimelineEntryDTO>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttemptHistoryResponse {
    pub attempts: Vec<TaskExecutionAttemptDTO>,
    pub cursor: Option<i32>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskActionResponse {
    pub task_id: String,
    pub status: String,
}

async fn get_task(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
) -> Result<Json<ResponseEnvelope<TaskDTO>>, AppError> {
    let task_id = task_id.into_uuid();
    let mut db = scoped_db_for_task_view(&state, &auth, task_id, &["workspaces:read"]).await?;
    let task = state.task_service.get_task(&mut db, task_id).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(task)))
}

async fn get_task_attempts(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
    Query(query): Query<AttemptHistoryQuery>,
) -> Result<Json<ResponseEnvelope<TaskAttemptHistoryResponse>>, AppError> {
    let task_id = task_id.into_uuid();
    let mut db = scoped_db_for_task_view(&state, &auth, task_id, &["workspaces:read"]).await?;
    let (attempts, cursor, has_more) = state
        .task_service
        .list_task_attempts(&mut db, task_id, query.cursor, query.limit.clamp(1, 200))
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(TaskAttemptHistoryResponse {
        attempts,
        cursor,
        has_more,
    })))
}

async fn get_task_timeline(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<ResponseEnvelope<TimelineResponse>>, AppError> {
    let task_id = task_id.into_uuid();
    let mut db = scoped_db_for_task_view(&state, &auth, task_id, &["workspaces:read"]).await?;
    let (entries, cursor, has_more) = state
        .task_service
        .get_task_timeline(&mut db, task_id, query.cursor, query.limit.clamp(1, 200))
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(TimelineResponse {
        entries,
        cursor,
        has_more,
    })))
}

async fn continue_task(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
) -> Result<Json<ResponseEnvelope<TaskActionResponse>>, AppError> {
    let task_id = task_id.into_uuid();
    let mut db = scoped_db_for_task(&state, &auth, task_id, &["workspaces:write"]).await?;
    let status = state
        .task_service
        .continue_workspace_task(&mut db, user.user_uuid(), task_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(TaskActionResponse {
        task_id: task_id.to_string(),
        status,
    })))
}

async fn cancel_task(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
) -> Result<Json<ResponseEnvelope<TaskActionResponse>>, AppError> {
    let task_id = task_id.into_uuid();
    let mut db = scoped_db_for_task(&state, &auth, task_id, &["workspaces:write"]).await?;
    let result = state
        .task_service
        .request_workspace_task_cancel(&mut db, user.user_uuid(), task_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    if let Err(error) = state
        .task_service
        .deliver_workspace_task_cancel(&result)
        .await
    {
        tracing::warn!(%task_id, %error, "durable task cancellation delivery remains pending");
    }
    Ok(Json(ResponseEnvelope::ok(TaskActionResponse {
        task_id: task_id.to_string(),
        status: result.status,
    })))
}

async fn reply_to_provider_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(path): Path<ProviderRequestReplyPath>,
    Json(body): Json<ProviderRequestReplyRequest>,
) -> Result<Json<ResponseEnvelope<ProviderRequestReplyResponse>>, AppError> {
    let task_id = path.id.into_uuid();
    // Start and answer: record the owner's Start consent first, as a send
    // does, so the held turn's answer can be accepted while the VM starts.
    let start = if body.start_compute.unwrap_or(false) {
        let mut db = scoped_db_for_task(&state, &auth, task_id, &["workspaces:write"]).await?;
        let context = crate::db::scoped::resolve_task_context(&mut db, task_id).await?;
        // A retry of an answer already accepted replays the frozen reply
        // (`g5.chat.native-request-boundary`) and must not record a second
        // Start: only a request still pending starts compute here.
        let still_pending = state
            .task_service
            .native_request_pending(&mut db, task_id, body.attempt, &path.provider_request_id)
            .await?;
        db.rollback().await.map_err(AppError::from)?;
        if still_pending {
            Some(crate::routes::start_and_send::begin(&state, &auth, context.workspace_id).await?)
        } else {
            None
        }
    } else {
        None
    };
    let pending_start = start
        .as_ref()
        .filter(|start| start.bind)
        .map(|start| start.intent_id);
    let result = async {
        let db = scoped_db_for_task(&state, &auth, task_id, &["workspaces:write"]).await?;
        state
            .task_service
            .reply_to_native_request(
                db,
                user.user_uuid(),
                NativeReplyInput {
                    task_id,
                    attempt: body.attempt,
                    provider_request_id: path.provider_request_id,
                    request_id: body.request_id,
                    validated_response: body.validated_response,
                    pending_start,
                },
            )
            .await
    }
    .await;
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(start) = &start {
                crate::routes::start_and_send::abandon(&state, &auth, start).await;
            }
            return Err(error);
        }
    };
    Ok(Json(ResponseEnvelope::ok(ProviderRequestReplyResponse {
        outcome,
        start_compute: start
            .as_ref()
            .map(crate::routes::start_and_send::StartComputeDTO::from),
    })))
}

pub fn workspace_tasks_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_workspace_tasks), &[Scope::TasksRead])
        .into_router()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskListQuery {
    cursor: Option<String>,
    #[serde(default)]
    active_only: bool,
    #[serde(default = "default_page_limit")]
    limit: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskListResponse {
    tasks: Vec<g5_api_foundation::contracts::tasks::TaskListEntry>,
    cursor: Option<String>,
    has_more: bool,
}

async fn list_workspace_tasks(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(workspace_id): Path<uuid::Uuid>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<ResponseEnvelope<TaskListResponse>>, AppError> {
    let mut db = crate::db::scoped::scoped_db_for_workspace(
        &state,
        &auth,
        workspace_id,
        &["workspaces:read"],
    )
    .await?;
    let (tasks, cursor, has_more) = state
        .task_service
        .list_workspace_tasks(
            &mut db,
            workspace_id,
            query.cursor.as_deref(),
            query.active_only,
            query.limit,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(TaskListResponse {
        tasks,
        cursor,
        has_more,
    })))
}
