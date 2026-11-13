//! Operator-facing remote integration state, attempt history and retries.

use crate::services::git::repo_store::ScopedRepoStorage;
use chrono::{DateTime, Utc};
use g5_contracts::{error::G5Error, types::WorkspaceRepoKey};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::services::{remote_integrations, workspace_repositories, PaginationParams};
use crate::{errors::AppError, models::branch_sync_run::BranchSyncRun};

/// Policy plus current Git observations for one canonical membership.
pub struct BranchTrackingRepo {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub repo_branch_name: String,
    pub tracking_mode: String,
    pub head_commit: String,
    pub remote_branch: Option<String>,
    pub baseline_commit: Option<String>,
    pub sync_paused_reason: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub remote_tip_advisory_sha: Option<String>,
    pub remote_tip_advisory_observed_at: Option<DateTime<Utc>>,
    pub latest_run_status: Option<String>,
    /// The half-merged workspace PR holding this member's auto-pull
    /// (`g5.governance.remote.pull`).
    pub import_held_by: Option<remote_integrations::ImportHold>,
    /// The G5 head no longer contains the watermark: an accepted workspace
    /// rewrite awaiting publication (`g5.governance.remote.workspace-rewrite`).
    pub rewrite_pending: bool,
}

pub struct BranchTrackingSummary {
    pub workspace_id: Uuid,
    pub repos: Vec<BranchTrackingRepo>,
}

pub async fn load_branch_tracking_summary(
    conn: &mut PgConnection,
    storage: &ScopedRepoStorage,
    key: WorkspaceRepoKey,
) -> Result<BranchTrackingSummary, AppError> {
    let member = workspace_repositories::load_workspace_repositories(conn, key.workspace_id, false)
        .await?
        .into_iter()
        .find(|member| member.repo_id == key.repo_id && member.org_id == key.org_id)
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace repository {}/{}", key.workspace_id, key.repo_id),
        })?;
    let versions = workspace_repositories::read_repository_versions(storage, vec![member]).await?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| G5Error::Internal {
            message: "Repository capture omitted a member".into(),
        })?;
    let policy = remote_integrations::load(conn, key, false).await?;
    let observation = match &policy {
        Some(policy) => remote_integrations::observed_tip_or_none(conn, policy).await,
        None => None,
    };
    let (baseline_commit, rewrite_pending) = if policy.is_some() {
        let path = storage.repo_path(key.org_id, key.repo_id)?;
        let branch = version.member.branch_name.clone();
        let head = version.head_commit.clone();
        tokio::task::spawn_blocking(move || -> Result<_, G5Error> {
            let repo = g5_scm::GitRepo::open(&path)?;
            let watermark = repo.read_sync_watermark(&branch)?;
            let rewritten = match watermark.as_deref() {
                Some(watermark) if watermark != head => !repo.is_ancestor_of(watermark, &head)?,
                _ => false,
            };
            Ok((watermark, rewritten))
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Watermark read failed: {error}"),
        })??
    } else {
        (None, false)
    };
    let latest_run_status = sqlx::query_scalar(
        "SELECT status FROM remote_sync_runs WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3
           AND kind = 'integration' ORDER BY started_at DESC, id DESC LIMIT 1"
    ).bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).fetch_optional(&mut *conn).await?;
    let import_held_by = if policy.is_some() {
        remote_integrations::import_hold(conn, key.workspace_id, key.repo_id).await?
    } else {
        None
    };
    Ok(BranchTrackingSummary {
        workspace_id: key.workspace_id,
        repos: vec![BranchTrackingRepo {
            workspace_id: key.workspace_id,
            repo_id: key.repo_id,
            repo_name: version.member.repo_name,
            repo_branch_name: version.member.branch_name,
            tracking_mode: if policy.is_some() {
                "remote_tracking"
            } else {
                "pinned"
            }
            .into(),
            head_commit: version.head_commit,
            remote_branch: policy.as_ref().map(|policy| policy.remote_branch.clone()),
            baseline_commit,
            sync_paused_reason: policy
                .as_ref()
                .and_then(|policy| policy.sync_paused_reason.clone()),
            last_sync_at: policy.and_then(|policy| policy.last_sync_at),
            remote_tip_advisory_sha: observation
                .as_ref()
                .map(|observation| observation.oid.clone()),
            remote_tip_advisory_observed_at: observation.map(|observation| observation.observed_at),
            latest_run_status,
            import_held_by,
            rewrite_pending,
        }],
    })
}

pub struct SyncRunListResult {
    pub items: Vec<BranchSyncRun>,
    pub total: i64,
}

pub async fn list_sync_runs_for_branch(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
    pagination: &PaginationParams,
) -> Result<SyncRunListResult, AppError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM remote_sync_runs WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3"
    ).bind(key.org_id).bind(key.workspace_id).bind(key.repo_id).fetch_one(&mut *conn).await?;
    let items = sqlx::query_as(
        "SELECT * FROM remote_sync_runs WHERE org_id = $1 AND workspace_id = $2 AND repo_id = $3
         ORDER BY started_at DESC, id DESC LIMIT $4 OFFSET $5",
    )
    .bind(key.org_id)
    .bind(key.workspace_id)
    .bind(key.repo_id)
    .bind(pagination.limit())
    .bind(pagination.offset())
    .fetch_all(&mut *conn)
    .await?;
    Ok(SyncRunListResult { items, total })
}

pub use crate::services::git::remote_integrations::RetryOutcome;

pub async fn retry_sync_runs_for_branch(
    conn: &mut PgConnection,
    key: WorkspaceRepoKey,
) -> Result<RetryOutcome, AppError> {
    remote_integrations::retry(conn, key).await
}
