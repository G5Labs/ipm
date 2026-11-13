//! Comments, review-thread replies and thread resolution on a provider PR
//! from G5 (`g5d pr comment`, `g5d pr resolve`; g5.governance.pr.conversation).
//!
//! Writes act under the acting person's own GitHub identity, the workspace
//! owner's for the workspace's agent, and reach only PRs in repositories
//! attached to the workspace. A comment or reply is recorded with its exact
//! payload and hidden marker before it is sent (the issue-comment publication
//! ledger), so a replay of its idempotency key never sends again: an unclear
//! outcome is reconciled by finding the marker. Resolution is idempotent and
//! re-read when its outcome is unclear. None of these confer approval.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use g5_auth::Scope;
use g5_contracts::types::WorkspaceId;
use serde_json::Value;

use crate::errors::AppError;
use crate::middleware::AuthContext;
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::issue_comments::{validate_body, CommentPublicationDTO};
pub(crate) use crate::services::workspace_pull_request_provider_lifecycle::{
    publish_pull_request_comment, set_review_thread_resolution, PullRequestCommentRequest,
};
use crate::services::workspace_pull_request_provider_lifecycle::{
    valid_thread_id, validation, ReviewThreadResolutionRequest,
};
use crate::services::AppState;

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route(
            "/comments",
            post(post_pull_request_comment),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/threads/resolve",
            post(resolve_review_thread),
            &[Scope::WorkspacesWrite],
        )
        .into_router()
}

/// The provider write the request names, checked before anything is recorded.
fn check_request(request: &PullRequestCommentRequest, max_key: usize) -> Result<(), AppError> {
    let key = &request.idempotency_key;
    if key.is_empty() || key.trim() != key || key.len() > max_key {
        return Err(validation(
            "idempotencyKey",
            &format!("must be 1-{max_key} characters without surrounding whitespace"),
        ));
    }
    validate_body(&request.body).map_err(AppError::from_error)?;
    match &request.reply_to {
        Some(thread) if !valid_thread_id(thread) => {
            Err(validation("replyTo", "must be a review thread ID"))
        }
        None if request.selector.is_none() => Err(validation(
            "selector",
            "a GitHub PR number or URL is required unless replying in a review thread",
        )),
        _ => Ok(()),
    }
}

/// Comment on, or reply in a review thread of, a GitHub PR attached to the
/// workspace, as the caller's GitHub account. Commenting is collaborative:
/// it needs workspace access and current organization write authority, not
/// ownership.
async fn post_pull_request_comment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(workspace_id): Path<WorkspaceId>,
    Json(request): Json<PullRequestCommentRequest>,
) -> Result<(StatusCode, Json<ResponseEnvelope<CommentPublicationDTO>>), AppError> {
    let workspace_id = workspace_id.into_uuid();
    let author = auth.require_user_id().map_err(AppError::from_error)?;
    check_request(&request, 200)?;
    let (record, fresh) = publish_pull_request_comment(
        &state,
        || crate::routes::external_issues::comment_db(&state, &auth, workspace_id),
        workspace_id,
        author,
        "person",
        &request,
    )
    .await?;
    tracing::info!(%workspace_id, publication_id = %record.id, state = %record.state, "pull request comment submitted");
    let status = if fresh {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(ResponseEnvelope::ok(record.to_dto()))))
}

async fn resolve_review_thread(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(workspace_id): Path<WorkspaceId>,
    Json(request): Json<ReviewThreadResolutionRequest>,
) -> Result<Json<ResponseEnvelope<Value>>, AppError> {
    let workspace_id = workspace_id.into_uuid();
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let db = crate::routes::external_issues::comment_db(&state, &auth, workspace_id).await?;
    set_review_thread_resolution(&state, db, workspace_id, actor, &request)
        .await
        .map(|data| Json(ResponseEnvelope::ok(data)))
}

/// Internal-route checks: agent keys share the owner's namespace with their
/// own comments, so they carry a prefix within the same length bound.
pub(crate) fn agent_request(
    mut request: PullRequestCommentRequest,
) -> Result<PullRequestCommentRequest, AppError> {
    check_request(&request, 190)?;
    request.idempotency_key = format!("agent:{}", request.idempotency_key);
    Ok(request)
}
