//! Owner-only preview service lifecycle. A start needs the running VM, or
//! records Start consent with the preview in one request
//! (`g5.preview.start-and-preview`), the shape of chat's Start and send.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use g5_auth::Scope;
use g5_contracts::types::WorkspaceId;

use crate::db::scoped::{scoped_db_for_workspace, scoped_db_for_workspace_repo};
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::task_lifecycle::{ActivePreview, StartWorkspacePreview};
use crate::services::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StartPreviewRequest {
    pub(crate) repo_id: Uuid,
    #[serde(default)]
    pub(crate) profile: Option<String>,
    pub(crate) idempotency_key: String,
    /// Start workspace and preview: record Start consent together with the
    /// preview, which runs once that Start brings the VM to ready
    /// (`g5.preview.start-and-preview`).
    #[serde(default)]
    pub(crate) start_compute: Option<bool>,
}

/// The queued preview, with the Start it recorded when the request carried
/// `startCompute`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartPreviewResponse {
    #[serde(flatten)]
    pub(crate) preview: crate::services::task_lifecycle::WorkspacePreviewView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start_compute: Option<crate::routes::start_and_send::StartComputeDTO>,
}

pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(get_preview), &[Scope::WorkspacesRead])
        .audited_route("/", post(start_preview), &[Scope::WorkspacesWrite])
        .audited_route("/cancel", post(cancel_preview), &[Scope::WorkspacesWrite])
        .into_router()
}

async fn get_preview(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(workspace_id): Path<WorkspaceId>,
) -> Result<
    Json<ResponseEnvelope<Option<crate::services::task_lifecycle::WorkspacePreviewView>>>,
    AppError,
> {
    let workspace_id = workspace_id.into_uuid();
    let mut db = scoped_db_for_workspace(&state, &auth, workspace_id, &["workspaces:read"]).await?;
    let preview = state
        .task_service
        .current_workspace_preview(&mut db, workspace_id, user.user_uuid())
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(preview)))
}

async fn start_preview(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(workspace_id): Path<WorkspaceId>,
    Json(body): Json<StartPreviewRequest>,
) -> Result<Json<ResponseEnvelope<StartPreviewResponse>>, AppError> {
    let workspace_id = workspace_id.into_uuid();
    // A request admission would refuse for its shape must not start the
    // workspace first: the key is checked here as admission checks it.
    if body.idempotency_key.trim() != body.idempotency_key
        || body.idempotency_key.is_empty()
        || body.idempotency_key.len() > 256
    {
        return Err(AppError::from_error(
            g5_contracts::error::G5Error::Conflict {
                reason: "preview idempotency key is invalid".to_string(),
            },
        ));
    }
    // Refusals that need no lock come before any consent is recorded, so a
    // request that cannot record a preview does not start the workspace:
    // the configuration the branch commits (the source while the VM is not
    // running) and the one-preview rule. A retry of a request whose
    // response was lost replays the recorded preview and must not record a
    // second Start: the first one carries it. Admission itself decides again
    // under the VM row lock.
    // The repository is a secondary identifier of this request: the scope
    // binds it to the workspace before anything reads it.
    let mut db = scoped_db_for_workspace_repo(
        &state,
        &auth,
        workspace_id,
        body.repo_id,
        &["workspaces:write"],
    )
    .await?;
    let preflight = state
        .task_service
        .preview_start_preflight(
            &mut db,
            workspace_id,
            user.user_uuid(),
            &body.idempotency_key,
        )
        .await?;
    let replay = preflight.replay;
    let start_compute = body.start_compute.unwrap_or(false);
    // The committed configuration is read whenever the VM is not running,
    // so admission names the reason a preview cannot be recorded (no Start
    // of the owner's to wait for, a Start they cannot bind to) rather than
    // failing for the configuration it never read. A non-owner reads
    // nothing here; admission refuses them.
    let committed_application = if !preflight.owner || preflight.vm_running || replay {
        None
    } else {
        let branch = state
            .workspace_service
            .get_workspace_repository(&mut db, workspace_id, body.repo_id)
            .await?;
        let committed = state
            .workspace_service
            .read_committed_preview_application(&mut db, &branch, body.profile.as_deref())
            .await?;
        if committed.is_none() {
            let reason = if preflight.start_under_way || preflight.starting {
                "the preview configuration is not committed on the branch; it can be launched once the workspace is running"
            } else {
                "the preview configuration is not committed on the branch; commit it or start the workspace first"
            };
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::Conflict {
                    reason: reason.to_string(),
                },
            ));
        }
        committed
    };
    if preflight.owner && !replay {
        if let Some(active) = preflight.active_preview {
            let reason = match active {
                ActivePreview::WaitingForStart => {
                    "a preview is already waiting for the workspace to start; close it before starting another"
                }
                ActivePreview::Running => {
                    "a preview is already running; close it before starting another"
                }
            };
            return Err(AppError::from_error(
                g5_contracts::error::G5Error::Conflict {
                    reason: reason.to_string(),
                },
            ));
        }
    }
    db.rollback().await.map_err(AppError::from)?;

    let start = if start_compute && !replay {
        Some(crate::routes::start_and_send::begin(&state, &auth, workspace_id).await?)
    } else {
        None
    };
    let result = start_preview_with(
        &state,
        &user,
        &auth,
        workspace_id,
        body,
        committed_application,
        start.as_ref(),
    )
    .await;
    if result.is_err() {
        if let Some(start) = &start {
            crate::routes::start_and_send::abandon(&state, &auth, start).await;
        }
    }
    result
}

async fn start_preview_with(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    body: StartPreviewRequest,
    committed_application: Option<
        g5_api_foundation::contracts::preview_profiles::PreviewApplicationDTO,
    >,
    start: Option<&crate::routes::start_and_send::CompoundStart>,
) -> Result<Json<ResponseEnvelope<StartPreviewResponse>>, AppError> {
    let mut db = scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:write"]).await?;
    let started = state
        .task_service
        .start_workspace_preview(
            &mut db,
            StartWorkspacePreview {
                workspace_id,
                actor_user_id: user.user_uuid(),
                repo_id: body.repo_id,
                profile: body.profile,
                idempotency_key: body.idempotency_key,
                preview_host_suffix: state.preview_host_suffix.clone(),
                committed_application,
                // A recorded consent is checked by admission even when Resume
                // reported the VM attached: the row must still carry it.
                start_intent_id: start
                    .filter(|start| start.bind || start.recorded)
                    .map(|start| start.intent_id),
            },
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(StartPreviewResponse {
        preview: started,
        start_compute: start.map(crate::routes::start_and_send::StartComputeDTO::from),
    })))
}

async fn cancel_preview(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(workspace_id): Path<WorkspaceId>,
) -> Result<
    Json<ResponseEnvelope<crate::services::task_lifecycle::WorkspaceTaskCancelResult>>,
    AppError,
> {
    let workspace_id = workspace_id.into_uuid();
    let db = scoped_db_for_workspace(&state, &auth, workspace_id, &["workspaces:write"]).await?;
    let cancellation = cancel_preview_with(&state, db, workspace_id, user.user_uuid()).await?;
    Ok(Json(ResponseEnvelope::ok(cancellation)))
}

/// Close `workspace_id`'s current preview for its owner `actor`, on a scope
/// the caller already holds, then deliver the cancellation. Shared with the
/// workspace agent.
pub(crate) async fn cancel_preview_with(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    actor: Uuid,
) -> Result<crate::services::task_lifecycle::WorkspaceTaskCancelResult, AppError> {
    let preview = state
        .task_service
        .current_workspace_preview(&mut db, workspace_id, actor)
        .await?
        .ok_or_else(|| {
            AppError::from_error(g5_contracts::error::G5Error::NotFound {
                resource: "workspace preview".to_string(),
            })
        })?;
    let cancellation = state
        .task_service
        .request_workspace_task_cancel(&mut db, actor, preview.task_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    if let Err(error) = state
        .task_service
        .deliver_workspace_task_cancel(&cancellation)
        .await
    {
        tracing::warn!(task_id = %cancellation.task_id, %error, "preview cancellation remains pending");
    }
    Ok(cancellation)
}
