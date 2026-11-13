//! Merge with override: the preview of what an override would waive and the
//! merge that waives exactly that (g5.workspace.merge.admin-override,
//! g5.git.provider-override, g5.workspace.merge.admin-override.formal).
//!
//! Both routes ask [`merge_override_authority`] who may waive what; nothing
//! here tests roles. The UI offers the action from the preview's `authority`.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use http::HeaderMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::workspaces::{
    merge_pull_request_as, policy_checks_snapshot, MergeWorkspacePullRequestResponseDTO,
    PullRequestMerger, WorkspacePullRequestPathParams,
};
use crate::errors::AppError;
use crate::middleware::{require_browser_session, AuthContext, AuthenticatedUser};
use crate::routes::ResponseEnvelope;
use crate::services::git::merge_override::{
    actor_display_name, blocking_checks, current_merge_override_authority,
    merge_override_authority, owns_source_workspace, waived_approvals, MergeOverrideAuthority,
    MergeOverrideIntent, MergeOverrideSubject, OverrideBlockers, WaivedCheckRepository,
};
use crate::services::AppState;
use g5_contracts::error::G5Error;
use g5_contracts::types::{AuthorityTarget, MergeSourceHead};

/// Longest override reason kept on the record.
const MAX_REASON_CHARS: usize = 2000;

/// What one override confirmation carries into the merge body: the person's
/// reason, the digest of the set they were shown, and their authority as
/// [`merge_override_authority`] decided it for this request.
pub(crate) struct MergeOverrideRequest {
    reason: String,
    digest: String,
    authority: MergeOverrideAuthority,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOverrideRequestDTO {
    pub reason: String,
    pub confirmed: bool,
    pub digest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOverridePreviewDTO {
    /// The source version the blockers belong to; the digest covers it.
    pub source_heads: Vec<MergeSourceHead>,
    pub blockers: OverrideBlockers,
    /// Mechanical blockers no override waives; while any are present the
    /// merge stays unavailable.
    pub not_overridable: Vec<String>,
    pub authority: MergeOverrideAuthority,
    /// The caller owns the pull request's source workspace.
    pub self_override: bool,
    pub digest: String,
}

/// `GET /workspaces/:id/pull-requests/:pullRequestId/merge-override`: what an
/// override would waive now, and whether the caller may.
pub async fn preview_merge_override(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    headers: HeaderMap,
    Path(params): Path<WorkspacePullRequestPathParams>,
) -> Result<Json<ResponseEnvelope<MergeOverridePreviewDTO>>, AppError> {
    let user_id = user.user_uuid();
    let authority = authority_for(&state, &auth, &headers, user_id, &params).await?;
    let merger = PullRequestMerger::Person {
        auth: &auth,
        user_id,
    };
    let preview = override_preview(&state, &merger, &params, authority).await?;
    Ok(Json(ResponseEnvelope::ok(preview)))
}

/// `POST /workspaces/:id/pull-requests/:pullRequestId/merge-override`: merge,
/// waiving exactly the blockers the person was shown.
pub async fn merge_with_override(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    headers: HeaderMap,
    Path(params): Path<WorkspacePullRequestPathParams>,
    Json(body): Json<MergeOverrideRequestDTO>,
) -> Result<Json<ResponseEnvelope<MergeWorkspacePullRequestResponseDTO>>, AppError> {
    let reason = body.reason.trim();
    if reason.is_empty() || reason.chars().count() > MAX_REASON_CHARS {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "reason": format!("Give a reason of at most {MAX_REASON_CHARS} characters."),
            }),
        }
        .into());
    }
    if !body.confirmed || body.digest.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "confirmed": "Confirm the requirements and rules that will be waived.",
            }),
        }
        .into());
    }
    let user_id = user.user_uuid();
    let authority = authority_for(&state, &auth, &headers, user_id, &params).await?;
    let merger = PullRequestMerger::Person {
        auth: &auth,
        user_id,
    };
    let request = MergeOverrideRequest {
        reason: reason.to_string(),
        digest: body.digest,
        authority,
    };
    let merged = merge_pull_request_as(&state, &merger, &params, Some(&request)).await?;
    Ok(Json(ResponseEnvelope::ok(merged)))
}

/// Establish the facts the authority rule decides on and ask it. Refuses a
/// caller who may make neither override: not a browser session, or no merge
/// authority over the pull request.
async fn authority_for(
    state: &Arc<AppState>,
    auth: &AuthContext,
    headers: &HeaderMap,
    user_id: Uuid,
    params: &WorkspacePullRequestPathParams,
) -> Result<MergeOverrideAuthority, AppError> {
    let browser_session = require_browser_session(auth, headers).is_ok();
    let merger = PullRequestMerger::Person { auth, user_id };
    let authority = match merger
        .target_db(state, params.id, params.pull_request_id)
        .await
    {
        Ok(mut db) => {
            let source_workspace_id = state
                .coordinated_repo_publication_service
                .pull_request_source_for_target(&mut db, params.pull_request_id, params.id)
                .await?;
            let authority = current_merge_override_authority(
                &mut db,
                source_workspace_id,
                params.id,
                browser_session,
            )
            .await?;
            db.rollback().await.map_err(AppError::from)?;
            authority
        }
        Err(error) if matches!(error.inner, G5Error::Forbidden { .. }) => {
            merge_override_authority(MergeOverrideSubject {
                may_merge: false,
                org_admin: false,
                browser_session,
            })
        }
        Err(error) => return Err(error),
    };
    if !authority.g5.is_allowed() && !authority.github.is_allowed() {
        return Err(G5Error::Forbidden {
            missing_scopes: vec![if browser_session {
                "merge_authority".into()
            } else {
                "browser_session_required".into()
            }],
        }
        .into());
    }
    Ok(authority)
}

/// What an override would waive now. Its digest is what the merge compares:
/// the same source cut a merge would pin, approvals evaluated against it,
/// the policy checks and GitHub's merge states read fresh.
async fn override_preview(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    params: &WorkspacePullRequestPathParams,
    authority: MergeOverrideAuthority,
) -> Result<MergeOverridePreviewDTO, AppError> {
    let mut db = merger
        .target_db(state, params.id, params.pull_request_id)
        .await?;
    let org_id = db.primary_org_id()?.into_uuid();
    let summary = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(&mut db, params.pull_request_id)
        .await?;
    if summary.status != "open" {
        return Err(G5Error::Conflict {
            reason: format!("Workspace pull request is {}", summary.status),
        }
        .into());
    }
    let checks = observed_policy_checks(state, merger, &mut db, &summary).await?;
    let github = state
        .workspace_pull_request_provider_lifecycle_service
        .blocked_provider_repositories(&mut db, summary.id, merger.actor())
        .await?;
    let self_override =
        owns_source_workspace(&mut db, summary.source_workspace_id, merger.actor()).await?;
    db.rollback().await.map_err(AppError::from)?;

    let source_db = merger
        .workspace_read_db(state, summary.source_workspace_id)
        .await?;
    let cut = crate::services::workspace_reader::read_committed_cut(
        source_db,
        &state.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: summary.source_workspace_id,
        },
        "merge override preview",
    )
    .await?;
    let source_heads: Vec<MergeSourceHead> = cut
        .repos
        .iter()
        .map(|repo| MergeSourceHead {
            repo_id: repo.repo_id.to_string(),
            branch_name: repo.branch_name.clone(),
            workspace_id: repo.workspace_id.to_string(),
            head_commit: repo.head_commit.clone(),
        })
        .collect();
    let mut eval_db = merger
        .target_db(state, params.id, params.pull_request_id)
        .await?;
    let admission = state
        .workspace_merge_service
        .evaluate_merge_admission(
            &mut eval_db,
            org_id,
            summary.source_workspace_id,
            summary.target_workspace_id,
            &source_heads,
        )
        .await?;
    eval_db.rollback().await.map_err(AppError::from)?;

    let approvals = waived_approvals(&admission);
    let mut not_overridable = Vec::new();
    if !admission.admitted && approvals.is_empty() {
        not_overridable.push(
            admission
                .reason
                .clone()
                .unwrap_or_else(|| "Merge admission refused".to_string()),
        );
    }
    if matches!(
        summary.local_merge_status.as_str(),
        "conflict" | "stale" | "blocked"
    ) {
        not_overridable.push(summary.blocking_message.clone().unwrap_or_else(|| {
            format!(
                "The branch is not ready to merge ({}).",
                summary.local_merge_status
            )
        }));
    }
    let blockers = OverrideBlockers::new(approvals, checks, github);
    let digest = blockers.digest(&source_heads);
    Ok(MergeOverridePreviewDTO {
        source_heads,
        blockers,
        not_overridable,
        authority,
        self_override,
        digest,
    })
}

/// The override's intent at execution: the blockers outside the approval
/// admission read fresh, which the admission then compares with the shown
/// set together with the approvals it evaluates under its locks.
pub(crate) async fn override_intent(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    db: &mut g5_authz::ScopedDb,
    params: &WorkspacePullRequestPathParams,
    request: &MergeOverrideRequest,
) -> Result<MergeOverrideIntent, AppError> {
    let actor_id = merger.actor();
    let summary = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(db, params.pull_request_id)
        .await?;
    let checks = observed_policy_checks(state, merger, db, &summary).await?;
    let github = state
        .workspace_pull_request_provider_lifecycle_service
        .blocked_provider_repositories(db, summary.id, actor_id)
        .await?;
    let self_override = owns_source_workspace(db, summary.source_workspace_id, actor_id).await?;
    let actor_name = actor_display_name(&state.system_pool, actor_id).await?;
    Ok(MergeOverrideIntent {
        actor_id,
        actor_name,
        reason: request.reason.clone(),
        self_override,
        expected_digest: request.digest.clone(),
        authority: request.authority,
        checks,
        github,
    })
}

/// The checks that block under the organization's external-checks policy,
/// empty while the policy is off.
async fn observed_policy_checks(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    db: &mut g5_authz::ScopedDb,
    summary: &crate::services::coordinated_repo_publication::WorkspacePullRequestSummary,
) -> Result<Vec<WaivedCheckRepository>, AppError> {
    let org_id = db.primary_org_id()?.into_uuid();
    if !crate::services::config_service::org_external_checks_block_merge(db, org_id).await? {
        return Ok(Vec::new());
    }
    Ok(
        match policy_checks_snapshot(state, merger, summary, merger.actor()).await? {
            None => Vec::new(),
            Some(Ok(snapshot)) => blocking_checks(&snapshot),
            Some(Err(())) => blocking_checks(&serde_json::Value::Null),
        },
    )
}
