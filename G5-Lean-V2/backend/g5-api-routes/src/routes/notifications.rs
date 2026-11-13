//! Notification route handlers.
//!
//! Endpoints:
//! - `GET  /api/v1/workspaces/:workspaceId/notifications`     — list user's notifications
//! - `PATCH /api/v1/notifications/:id`                         — mark one notification read
//! - `POST  /api/v1/workspaces/:workspaceId/notifications/read-all` — mark all read

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{scoped_db_for_notification, scoped_db_for_workspace};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::notification_service;
use crate::services::AppState;
use g5_auth::Scope;

// ─── Path Parameter Types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct NotificationPathParams {
    pub id: Uuid,
}

// ─── Query Parameters ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListNotificationsQuery {
    /// If true, only return unread notifications.
    pub unread_only: Option<bool>,
    /// Max results (default 50, max 100).
    pub limit: Option<i64>,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDTO {
    pub id: String,
    pub workspace_id: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_id: Option<String>,
    pub actions: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_avatar: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkAllReadResponse {
    pub updated: u64,
}

/// Body for `POST /read-all`: `{}` marks all of the recipient's unread
/// notifications in the workspace read; with `ids`, only that set (an empty
/// set marks nothing). The body is required, so a selection can never be
/// mistaken for "all" by any framing of the request.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MarkAllReadRequest {
    pub ids: Option<Vec<Uuid>>,
}

// ─── From Implementations ────────────────────────────────────────────────────

impl From<notification_service::NotificationDTO> for NotificationDTO {
    fn from(n: notification_service::NotificationDTO) -> Self {
        NotificationDTO {
            id: n.id.to_string(),
            workspace_id: n.workspace_id.to_string(),
            category: n.category,
            severity: n.severity,
            title: n.title,
            message: n.message,
            task_id: n.task_id.map(|id| id.to_string()),
            comment_id: n.comment_id.map(|id| id.to_string()),
            reply_id: n.reply_id.map(|id| id.to_string()),
            actions: n.actions,
            read_at: n.read_at,
            expires_at: n.expires_at,
            created_at: n.created_at,
            actor_name: n.actor_name,
            actor_avatar: n.actor_avatar,
        }
    }
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Workspace-scoped notification routes:
/// - `GET  /` — list notifications
/// - `POST /read-all` — mark all read (`{}`), or only the `ids` in the body
///
/// Marking read changes only the recipient's own inbox rows, so it needs
/// read access to the workspace alone, whatever the recipient's role and
/// whoever owns the workspace (g5.user.notifications): the read scope here
/// is what admits non-owner recipients and organization viewers, since the
/// write scope would demand current ownership of the workspace.
pub fn workspace_notifications_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_notifications), &[Scope::WorkspacesRead])
        .audited_route("/read-all", post(mark_all_read), &[Scope::WorkspacesRead])
        .into_router()
}

/// Notification-detail routes:
/// - `PATCH /:id` — mark one notification read (read access suffices, as above)
pub fn notification_detail_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/:id", patch(mark_read), &[Scope::WorkspacesRead])
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/notifications`
///
/// Lists notifications for the authenticated user in the given workspace.
async fn list_notifications(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<ListNotificationsQuery>,
) -> Result<Json<ResponseEnvelope<Vec<NotificationDTO>>>, AppError> {
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let user_id = auth.require_user_id()?;

    let filters = notification_service::NotificationFilters {
        unread_only: query.unread_only,
        limit: query.limit,
    };

    let notifications = state
        .notification_service
        .list_notifications(&mut db, params.workspace_id, user_id, filters)
        .await?;

    let dtos: Vec<NotificationDTO> = notifications
        .into_iter()
        .map(NotificationDTO::from)
        .collect();

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `PATCH /api/v1/notifications/:id`
///
/// Marks a single notification as read.
async fn mark_read(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<NotificationPathParams>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let mut db = scoped_db_for_notification(&state, &auth, params.id, &["workspaces:read"]).await?;
    let user_id = auth.require_user_id()?;

    state
        .notification_service
        .mark_read(&mut db, params.id, user_id)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(serde_json::json!({
        "read": true
    }))))
}

/// `POST /api/v1/workspaces/:workspaceId/notifications/read-all`
///
/// Marks the user's unread notifications in the workspace as read: all of
/// them for `{}`, or only the `ids` in the body. The body is required and
/// must parse, so a malformed or missing selection is refused before any
/// read state changes rather than widening into "all".
async fn mark_all_read(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(request): Json<MarkAllReadRequest>,
) -> Result<Json<ResponseEnvelope<MarkAllReadResponse>>, AppError> {
    let ids = request.ids;
    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;
    let user_id = auth.require_user_id()?;

    let updated = state
        .notification_service
        .mark_all_read(&mut db, params.workspace_id, user_id, ids.as_deref())
        .await?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(MarkAllReadResponse { updated })))
}
