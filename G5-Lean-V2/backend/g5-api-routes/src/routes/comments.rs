//! Anchored-comment route handlers under two path prefixes.
//!
//! A comment belongs to one line of one file (`scope = 'line'`), identified
//! by workspace, repository and path. Legacy node-level threads stay
//! readable; new ones are refused. Review-suggestion threads are reached only
//! through the review routes, never here.
//!
//! Endpoints span 2 path prefixes:
//! - `/api/v1/workspaces/:workspaceId/comments` — list, create
//! - `/api/v1/comments/:id` — update, delete, replies
//!
//! Handlers translate requests and errors; `CommentService` decides who may
//! act (`g5.documents.comments.authority`, `.replies`) and publishes events
//! after commit.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{
    ensure_workspace_user_ids, resolve_comment_context, scoped_db_for_comment,
    scoped_db_for_workspace, scoped_system_db_for_workspace,
};
use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::comment_service;
use crate::services::AppState;
use g5_auth::Scope;

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for comment-scoped endpoints (`/api/v1/comments/:id/*`).
#[derive(Debug, Deserialize)]
pub struct CommentPathParams {
    pub id: Uuid,
}

/// Path parameters for workspace-scoped endpoints (`/api/v1/workspaces/:workspaceId/*`).
#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    #[serde(rename = "workspaceId")]
    pub workspace_id: Uuid,
}

// ─── Query Parameter Types ───────────────────────────────────────────────────

/// Query parameters for `GET /api/v1/workspaces/:workspaceId/comments`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListCommentsQuery {
    /// Exact repository owning a line-comment file path.
    pub repo_id: Option<Uuid>,
    /// Filter by file path (for line-scoped comments).
    pub file_path: Option<String>,
    /// Filter by branch name (for line-scoped comments).
    pub branch: Option<String>,
    /// Pagination: page number (default 1).
    pub page: Option<i64>,
    /// Pagination: items per page (default 50, max 100).
    pub per_page: Option<i64>,
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Request body for `POST /api/v1/workspaces/:workspaceId/comments`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCommentRequest {
    pub id: Option<Uuid>,
    pub scope: String,
    pub content: String,
    /// For scope = 'line'.
    pub repo_id: Option<Uuid>,
    /// For scope = 'line'.
    pub file_path: Option<String>,
    pub line_number: Option<i32>,
    pub line_hash: Option<String>,
    pub content_hash: Option<String>,
    pub anchor_context: Option<String>,
    pub commit_sha: Option<String>,
    /// A label only: it never selects a thread. Omitted, the server records
    /// the workspace's branch (`g5.documents.comments.identity`).
    pub origin_branch: Option<String>,
    /// Optional: explicit @mention user IDs resolved by the client UI.
    pub mention_user_ids: Option<Vec<Uuid>>,
}

/// Request body for `PATCH /api/v1/comments/:id`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCommentRequest {
    pub content: Option<String>,
    pub resolved: Option<bool>,
}

/// Request body for `POST /api/v1/comments/:id/replies`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddReplyRequest {
    pub id: Option<Uuid>,
    pub content: String,
    /// Optional: explicit @mention user IDs resolved by the client UI.
    pub mention_user_ids: Option<Vec<Uuid>>,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// The primary comment response DTO.
///
/// UUID fields are serialized as `String` (hyphenated UUID format).
/// DateTime fields are serialized as ISO 8601 strings.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentDTO {
    pub id: String,
    pub workspace_id: String,
    pub author_id: String,
    pub author_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_avatar_url: Option<String>,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    pub content: String,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
    pub origin_branch: String,
    pub reply_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A comment reply as returned by the API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyDTO {
    pub id: String,
    pub comment_id: String,
    pub author_id: String,
    pub author_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_avatar_url: Option<String>,
    pub content: String,
    pub created_at: String,
}

/// Response from the delete comment endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCommentResponse {
    pub deleted: bool,
}

// ─── From Implementations ────────────────────────────────────────────────────

impl From<comment_service::CommentDTO> for CommentDTO {
    fn from(c: comment_service::CommentDTO) -> Self {
        CommentDTO {
            id: c.id.to_string(),
            workspace_id: c.workspace_id.to_string(),
            author_id: c.author_id.to_string(),
            author_name: c.author_name,
            author_avatar_url: c.author_avatar,
            scope: c.scope,
            repo_id: c.repo_id.map(|id| id.to_string()),
            file_path: c.file_path,
            line_number: c.line_number,
            line_hash: c.line_hash,
            content_hash: c.content_hash,
            anchor_context: c.anchor_context,
            commit_sha: c.commit_sha,
            content: c.content,
            resolved: c.resolved,
            resolved_at: c.resolved_at,
            origin_branch: c.origin_branch,
            reply_count: c.reply_count,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl From<comment_service::CommentReplyDTO> for ReplyDTO {
    fn from(r: comment_service::CommentReplyDTO) -> Self {
        ReplyDTO {
            id: r.id.to_string(),
            comment_id: r.comment_id.to_string(),
            author_id: r.author_id.to_string(),
            author_name: r.author_name,
            author_avatar_url: r.author_avatar,
            content: r.content,
            created_at: r.created_at,
        }
    }
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Comment-scoped routes nested under `/api/v1/comments`.
///
/// Endpoints:
/// - `PATCH /:id` — update / resolve comment (owner resolves with write scope, author edits with read scope)
/// - `DELETE /:id` — delete comment (owner)
/// - `GET /:id/replies` — list replies
/// - `POST /:id/replies` — add reply (any reader)
pub fn comment_detail_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        // Editing one's own text needs read scope; resolving or reopening
        // needs write scope and the owner, checked in the handler.
        .audited_route("/:id", patch(update_comment), &[Scope::WorkspacesRead])
        .audited_route("/:id", delete(delete_comment), &[Scope::WorkspacesWrite])
        .audited_route("/:id/replies", get(get_replies), &[Scope::WorkspacesRead])
        .audited_route("/:id/replies", post(add_reply), &[Scope::WorkspacesRead])
        .into_router()
}

/// Workspace-scoped comment routes nested under
/// `/api/v1/workspaces/:workspaceId`.
///
/// Endpoints:
/// - `GET /comments` — list comments (filterable)
/// - `POST /comments` — create comment
pub fn workspace_comments_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_comments), &[Scope::WorkspacesRead])
        .audited_route("/", post(create_comment), &[Scope::WorkspacesRead])
        .into_router()
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/workspaces/:workspaceId/comments`
///
/// Lists comments in a workspace, filterable by node ID, file path, and branch.
/// Supports pagination via `page` and `perPage` query parameters.
/// Returns an empty array (not an error) if no comments match.
async fn list_comments(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<ListCommentsQuery>,
) -> Result<Json<ResponseEnvelope<Vec<CommentDTO>>>, AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        "Comment endpoint called: list_comments"
    );

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;

    let filters = comment_service::CommentFilters {
        repo_id: query.repo_id,
        file_path: query.file_path,
        branch: query.branch,
        resolved: None,
    };

    let comments = state
        .comment_service
        .list_comments(&mut db, params.workspace_id, filters)
        .await?;

    let dtos: Vec<CommentDTO> = comments.into_iter().map(CommentDTO::from).collect();

    // Build pagination metadata
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 100);
    let total = dtos.len() as i64;

    let meta = crate::routes::PaginationMeta {
        page,
        per_page,
        total,
    };

    Ok(Json(ResponseEnvelope::paginated(dtos, meta)))
}

/// `POST /api/v1/workspaces/:workspaceId/comments`
///
/// Starts an anchored line thread. Anyone who can read the workspace may
/// start one, with no running VM (`g5.documents.comments.authority`); the
/// service checks the fixed audience itself. Node-scoped comments
/// are retired and refused.
/// Returns 201 Created with the newly created comment.
async fn create_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<CommentDTO>>), AppError> {
    tracing::debug!(
        workspace_id = %params.workspace_id,
        scope = %body.scope,
        "Comment endpoint called: create_comment"
    );

    let mut db =
        scoped_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"]).await?;

    let user_id = auth.require_user_id()?;

    let mention_user_ids = body.mention_user_ids.clone().unwrap_or_default();
    let mut system_db =
        scoped_system_db_for_workspace(&state, &auth, params.workspace_id, &["workspaces:read"])
            .await?;
    ensure_workspace_user_ids(
        &mut system_db,
        params.workspace_id,
        &mention_user_ids,
        "mentionUserIds",
    )
    .await?;
    system_db.commit().await.map_err(AppError::from)?;
    let content_preview = body.content.clone();

    let service_req = comment_service::CreateCommentRequest {
        id: body.id,
        scope: body.scope,
        content: body.content,
        repo_id: body.repo_id,
        file_path: body.file_path,
        line_number: body.line_number,
        line_hash: body.line_hash,
        content_hash: body.content_hash,
        anchor_context: body.anchor_context,
        commit_sha: body.commit_sha,
        origin_branch: body.origin_branch,
        commit_range_base: None,
        commit_range_head: None,
    };

    let comment = state
        .comment_service
        .create_comment(&mut db, params.workspace_id, user_id, service_req)
        .await?;

    let comment_id_uuid: Uuid = comment.id;
    let author_name = comment.author_name.clone();
    state
        .notification_service
        .process_mentions_best_effort(
            &mut db,
            params.workspace_id,
            user_id,
            &author_name,
            Some(comment_id_uuid),
            None,
            &content_preview,
            &mention_user_ids,
        )
        .await;

    let dto: CommentDTO = comment.into();

    tracing::info!(
        workspace_id = %params.workspace_id,
        comment_id = %dto.id,
        scope = %dto.scope,
        "Comment endpoint succeeded: create_comment"
    );

    db.commit().await.map_err(AppError::from)?;
    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

/// `PATCH /api/v1/comments/:id`
///
/// Updates a comment's content or resolves/unresolves it. Both fields are
/// optional; the client sends only the fields it wants to update.
///
/// A content edit is the author's, who may be any reader of the workspace
/// (`g5.documents.comments.authority`), so the route is registered with read
/// scope. Resolving or reopening is the current owner's and needs the
/// credential's write scope: the route middleware checked only read scope, so
/// the handler checks the credential itself before opening the write-scoped
/// transaction, which `scoped_db_for_comment` admits only for the owner; the
/// service checks the owner again.
async fn update_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<CommentPathParams>,
    Json(body): Json<UpdateCommentRequest>,
) -> Result<Json<ResponseEnvelope<CommentDTO>>, AppError> {
    tracing::debug!(
        comment_id = %params.id,
        "Comment endpoint called: update_comment"
    );

    let scopes: &[&str] = if body.resolved.is_some() {
        if !auth.scopes().contains(&Scope::WorkspacesWrite) {
            return Err(AppError::from(g5_contracts::G5Error::Forbidden {
                missing_scopes: vec![Scope::WorkspacesWrite.to_string()],
            }));
        }
        &["workspaces:write"]
    } else {
        &["workspaces:read"]
    };
    let mut db = scoped_db_for_comment(&state, &auth, params.id, scopes).await?;

    let user_id = auth.require_user_id()?;

    let service_req = comment_service::UpdateCommentRequest {
        content: body.content,
        resolved: body.resolved,
    };

    let comment = state
        .comment_service
        .update_comment(&mut db, params.id, user_id, service_req)
        .await?;

    let dto: CommentDTO = comment.into();

    tracing::info!(
        comment_id = %params.id,
        "Comment endpoint succeeded: update_comment"
    );

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/comments/:id/replies`
///
/// Lists all replies for a comment thread, ordered by creation time.
async fn get_replies(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<CommentPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<ReplyDTO>>>, AppError> {
    let mut db = scoped_db_for_comment(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(
        comment_id = %params.id,
        "Comment endpoint called: get_replies"
    );

    let replies = state
        .comment_service
        .get_replies(&mut db, params.id)
        .await?;
    let dtos: Vec<ReplyDTO> = replies.into_iter().map(ReplyDTO::from).collect();

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `POST /api/v1/comments/:id/replies`
///
/// Adds a reply to an anchored thread. Anyone who can read the workspace may
/// reply, with no running VM (`g5.documents.comments.replies`).
/// Returns 201 Created with the newly created reply.
async fn add_reply(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<CommentPathParams>,
    Json(body): Json<AddReplyRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<ReplyDTO>>), AppError> {
    tracing::debug!(
        comment_id = %params.id,
        "Comment endpoint called: add_reply"
    );

    let mut db = scoped_db_for_comment(&state, &auth, params.id, &["workspaces:read"]).await?;

    let user_id = auth.require_user_id()?;

    let mention_user_ids = body.mention_user_ids.clone().unwrap_or_default();
    let comment_context = resolve_comment_context(&mut db, params.id).await?;
    let mut system_db = scoped_system_db_for_workspace(
        &state,
        &auth,
        comment_context.workspace_id,
        &["workspaces:read"],
    )
    .await?;
    ensure_workspace_user_ids(
        &mut system_db,
        comment_context.workspace_id,
        &mention_user_ids,
        "mentionUserIds",
    )
    .await?;
    system_db.commit().await.map_err(AppError::from)?;
    let content_preview = body.content.clone();

    let reply = state
        .comment_service
        .add_reply_with_id(&mut db, params.id, user_id, body.content, body.id)
        .await?;

    state
        .notification_service
        .process_mentions_best_effort(
            &mut db,
            comment_context.workspace_id,
            user_id,
            &reply.author_name,
            Some(params.id),
            Some(reply.id),
            &content_preview,
            &mention_user_ids,
        )
        .await;

    let dto: ReplyDTO = reply.into();

    tracing::info!(
        comment_id = %params.id,
        reply_id = %dto.id,
        "Comment endpoint succeeded: add_reply"
    );

    db.commit().await.map_err(AppError::from)?;
    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

/// `DELETE /api/v1/comments/:id`
///
/// Deletes a comment and all its replies (cascade delete via the database
/// FK constraint).
async fn delete_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<CommentPathParams>,
) -> Result<Json<ResponseEnvelope<DeleteCommentResponse>>, AppError> {
    let mut db = scoped_db_for_comment(&state, &auth, params.id, &["workspaces:write"]).await?;
    tracing::debug!(
        comment_id = %params.id,
        "Comment endpoint called: delete_comment"
    );

    let user_id = auth.require_user_id()?;

    state
        .comment_service
        .delete_comment(&mut db, params.id, user_id)
        .await?;

    tracing::info!(
        comment_id = %params.id,
        "Comment endpoint succeeded: delete_comment"
    );

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(DeleteCommentResponse {
        deleted: true,
    })))
}
