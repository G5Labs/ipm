use super::merge_override::MergeOverrideIntent;
use crate::services::git::repo_store::{RepoKey, ScopedRepoStorage, TenantRepoStorage};
use chrono::{DateTime, Utc};
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::types::{AuthorityTarget, MergeSourceHead};
use g5_scm::{CommitRangeMessages, GitRepo};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::{AppError, G5Error};
use std::sync::Arc;

use crate::services::advance_consumer::{
    consumer_receipt_result, load_consumer_input, record_consumer_receipt, register_consumer_input,
    LandedCutRecord,
};
use crate::services::branch_commit::{
    acquire_workspace_commit_fence, BranchCommitMember, FenceMode, LandedBranchCommit,
    PreparedBranchCommit,
};
use crate::services::workspace_merge_service::{MergeAdmissionEvidence, MergeAdmissionGate};
use crate::services::workspace_merge_service::{
    MergePreflightDecision, MergePreflightRepoOutcome, MergePreflightResponse,
    PreparedLocalWorkspaceMerge, PreparedWorkspaceMergeApplication,
    PreparedWorkspaceMergePreflight, WorkspaceMergePreflightMode, WorkspaceMergeRunSummary,
    WorkspaceMergeService,
};
use crate::services::EventBus;

pub(crate) const WORKSPACE_PULL_REQUEST_MERGE_CONSUMER: &str = "workspace_pull_request_merge";
// Retained only to recognize in-flight receipts from the old Base-only flow.
pub(crate) const PROVIDER_FIRST_BASE_RECEIPT_CLAIM_CONSUMER: &str =
    "provider_first_base_receipt_claim";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePullRequestSummary {
    pub id: Uuid,
    /// Names its repositories in storage (`g5.repo.storage-tenancy`).
    #[serde(skip)]
    pub org_id: Uuid,
    /// Its number, serialized as its name `PR######`, which also names its
    /// local repository pull requests (`g5.git.pr-local-number`).
    #[serde(
        serialize_with = "crate::services::git::pull_request_adapters::serialize_g5_pull_request_name"
    )]
    pub number: i64,
    pub source_workspace_id: Uuid,
    pub source_workspace_name: String,
    pub target_workspace_id: Uuid,
    pub target_workspace_name: String,
    pub status: String,
    pub local_merge_status: String,
    pub provider_status: String,
    #[serde(default)]
    #[sqlx(skip)]
    pub provider_repos: Vec<WorkspacePullRequestProviderRepoSummary>,
    /// One per member the PR changes, each from its adapter
    /// (`g5.git.pr-repository`).
    #[serde(default)]
    #[sqlx(skip)]
    pub repository_pull_requests:
        Vec<crate::services::git::pull_request_adapters::RepositoryPullRequestSummary>,
    pub title: String,
    pub description_override: Option<String>,
    pub source_heads: serde_json::Value,
    pub target_heads: serde_json::Value,
    pub preflight: serde_json::Value,
    pub last_preflight_fingerprint: String,
    pub blocking_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub merged_at: Option<DateTime<Utc>>,
    /// Who performed the merge.
    pub merged_by: Option<Uuid>,
    /// The override a merged pull request landed under
    /// (g5.workspace.merge.admin-override.formal), from its recorded
    /// admission; `None` for an ordinary merge and while open.
    #[serde(default)]
    #[sqlx(skip)]
    pub merge_override: Option<crate::services::git::merge_override::MergeOverrideRecord>,
    /// Whether the source workspace forwards pushes to the provider (Auto-push
    /// on); when off, a push does not reach the PR until it is republished
    /// (`g5.git.pr-cli.manual-forwarding`).
    pub source_auto_push: bool,
    /// Whether the source or target workspace is deleted: its PRs stay
    /// readable, closed by the deletion (`g5.workspace.delete.pull-requests`,
    /// `g5.git.pr-reads`).
    pub source_workspace_deleted: bool,
    pub target_workspace_deleted: bool,
    /// The merge admission every shipping surface shows
    /// (g5.governance.approval.requirements): for an open pull request the
    /// Integration Engineering requirements evaluated against its recorded
    /// source cut, for a merged one the admission it landed under. `None`
    /// when the source cut is not a complete head vector.
    #[serde(default)]
    #[sqlx(skip)]
    pub merge_admission: Option<MergeAdmissionEvidence>,
    /// The recorded column, read with the row and projected above.
    #[serde(skip)]
    pub recorded_merge_admission: Option<serde_json::Value>,
    /// Merge when ready (g5.git.pr-auto-merge); `None` while it is off.
    pub auto_merge_request: Option<
        sqlx::types::Json<crate::services::workspace_pull_request_auto_merge::AutoMergeRequest>,
    >,
    /// The effective source-owner choice: explicit PR value or personal default.
    /// Successful merge freezes it, independently of enabling merge when ready.
    pub stop_vm_after_merge: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePullRequestProviderRepoSummary {
    pub id: Uuid,
    pub repo_id: Uuid,
    pub provider: String,
    pub provider_repo_full_name: String,
    pub source_remote_branch: String,
    pub target_remote_branch: String,
    pub provider_pr_number: Option<i64>,
    pub provider_pr_url: Option<String>,
    pub provider_status: String,
    pub source_head_sha: Option<String>,
    pub target_head_sha: Option<String>,
    /// Exact admitted target, present only after a G5 attempt was accepted.
    pub merge_target_head_sha: Option<String>,
    pub merge_pending: bool,
    pub provider_head_sha: Option<String>,
    pub provider_base_sha: Option<String>,
    pub last_error: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    /// GitHub had already merged this repository when G5 came to merge it
    /// (g5.git.provider-merged-outside).
    pub merged_outside_g5: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspacePullRequestResult {
    pub pull_request: WorkspacePullRequestSummary,
    pub already_existed: bool,
    pub dirty_drafts: Vec<WorkspaceDirtyDraftSummary>,
}

/// Spec D14: commit-derived PR text preview for the Prepare-to-Ship modal.
/// `derived = false` carries the legacy workspace-name title (the body may
/// still come from the Project Doc). `source` (spec D16) attributes the
/// body: the workspace PR-body Project Doc, commit-derived content, or the
/// legacy fallback. `derived` keeps its title meaning for compatibility.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePullRequestTextPreview {
    pub title: String,
    pub body: String,
    pub derived: bool,
    pub source: WorkspacePullRequestTextSource,
}

/// Spec D16: where the previewed PR body came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspacePullRequestTextSource {
    /// The workspace PR-body Project Doc (`docs/<workspace_id>/<slug>.md`)
    /// at the docs repository's snapshot head.
    ProjectDoc,
    /// Commit-derived content per specs D10/D14.
    Derived,
    /// Legacy workspace-name title with an empty body.
    Legacy,
}

pub struct PreparedWorkspacePullRequestCreate {
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    actor_id: Uuid,
    org_id: Uuid,
    /// The branches the two workspaces own, which a rename never changes
    /// (`g5.workspace.branch`); publish revalidates these, not the names.
    source_branch: Option<String>,
    target_branch: Option<String>,
    title: String,
    title_override: Option<String>,
    description_override: DescriptionOverrideChange,
    dirty_drafts: Vec<WorkspaceDirtyDraftSummary>,
    preflight: PreparedWorkspaceMergePreflight,
}

pub struct PreparedWorkspacePullRequestRefresh {
    observed: WorkspacePullRequestRow,
    path_workspace_id: Uuid,
    actor_id: Uuid,
    preflight: PreparedWorkspaceMergePreflight,
}

/// One active dirty draft in the source or target workspace at Create PR
/// time. Dirty drafts never block create/refresh (spec D1 step 5 / job
/// service D30); this summary tells the user whose uncommitted drafts are
/// excluded from the PR.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDirtyDraftSummary {
    pub workspace_id: Uuid,
    pub workspace_role: String,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub dirty_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeWorkspacePullRequestResult {
    pub pull_request: WorkspacePullRequestSummary,
    pub run: WorkspaceMergeRunSummary,
    #[serde(skip)]
    pub authority_advance: Option<PreparedBranchCommit>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeWorkspacePullRequestOutcome {
    Merged(Box<MergeWorkspacePullRequestResult>),
    #[serde(skip)]
    PreparedLocal(Box<PreparedLocalWorkspacePullRequestMerge>),
    Blocked {
        reason: String,
    },
    /// A target with remote bindings must merge on the provider first and
    /// import its accepted history before completing the local PR.
    ProviderFirstMerge(Box<ProviderFirstMergeRequest>),
}

pub struct PreparedLocalWorkspacePullRequestMerge {
    pull_request_id: Uuid,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    actor_id: Uuid,
    preflight: MergePreflightResponse,
    merge: PreparedWorkspaceMergeApplication,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspacePullRequestMergeConsumerInput {
    pull_request_id: Uuid,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    actor_id: Uuid,
    preflight_fingerprint: String,
    evidence: PreflightEvidence,
}

/// Identity for a provider-first workspace merge (Base and ordinary workspaces).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderFirstMergeRequest {
    pub pull_request_id: Uuid,
    pub org_id: Uuid,
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Uuid,
    pub actor_id: Uuid,
    /// Target heads this merge's fresh preflight classified; the provider
    /// merge verifies the provider against them (g5.git.pr-recovery-cut).
    /// Absent from requests persisted before #582.
    #[serde(default)]
    pub target_heads: Vec<ProviderMergeTargetHead>,
}

/// One repository's target head as the merge's fresh preflight classified it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMergeTargetHead {
    pub repo_id: Uuid,
    pub head_commit: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WorkspacePullRequestRole {
    Source,
    #[default]
    Target,
    All,
}

#[derive(Clone)]
pub struct CoordinatedRepoPublicationService {
    merge_admission: Arc<dyn MergeAdmissionGate>,
    adapters: Arc<crate::services::git::pull_request_adapters::PullRequestAdapters>,
    /// Identity-service lookups only; workspace/PR reads remain tenant scoped.
    system_pool: sqlx::PgPool,
}

struct PullRequestEventInsert<'a> {
    org_id: Uuid,
    pull_request_id: Uuid,
    event_type: &'a str,
    actor_id: Uuid,
    evidence: Option<&'a PreflightEvidence>,
    message: Option<String>,
}

impl CoordinatedRepoPublicationService {
    pub fn new(
        merge_admission: Arc<dyn MergeAdmissionGate>,
        adapters: Arc<crate::services::git::pull_request_adapters::PullRequestAdapters>,
        system_pool: sqlx::PgPool,
    ) -> Self {
        Self {
            merge_admission,
            adapters,
            system_pool,
        }
    }

    #[allow(clippy::too_many_arguments)] // route call surface; D15 added the description override
    pub async fn prepare_workspace_pull_request_create(
        &self,
        mut db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        repo_storage: TenantRepoStorage,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        actor_id: Uuid,
        description: Option<String>,
        title_override: Option<String>,
    ) -> Result<PreparedWorkspacePullRequestCreate, G5Error> {
        let description_override = normalize_description_override(description)?;
        self.reject_base_source_workspace(&mut db, source_workspace_id)
            .await?;
        let dirty_drafts = self
            .load_dirty_draft_summaries(&mut db, source_workspace_id, target_workspace_id)
            .await?;
        let names = self
            .load_workspace_names(&mut db, source_workspace_id, target_workspace_id)
            .await?;
        let repo_storage = repo_storage.scoped(&db);
        let preflight = workspace_merge_service
            .prepare_transaction_free_workspace_merge_preflight(
                db,
                source_workspace_id,
                target_workspace_id,
                WorkspaceMergePreflightMode::EvidenceCapture,
            )
            .await?;
        let title = if let Some(title) = &title_override {
            title.clone()
        } else {
            derive_local_pr_title(
                repo_storage,
                names.org_id,
                preflight.response(),
                names.source_docs_repo_id,
            )
            .await
            .unwrap_or_else(|| format!("{} into {}", names.source_name, names.target_name))
        };
        Ok(PreparedWorkspacePullRequestCreate {
            source_workspace_id,
            target_workspace_id,
            actor_id,
            org_id: names.org_id,
            source_branch: names.source_branch,
            target_branch: names.target_branch,
            title,
            title_override,
            description_override,
            dirty_drafts,
            preflight,
        })
    }

    /// Persist PR evidence, then check as the transaction's last locks that
    /// it is one committed cut of each workspace (`g5.fork.source.cut`):
    /// provider PRs are created at these heads.
    pub async fn apply_prepared_workspace_pull_request_create(
        &self,
        db: &mut ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        prepared: PreparedWorkspacePullRequestCreate,
    ) -> Result<CreateWorkspacePullRequestResult, G5Error> {
        let cuts = WorkspaceMergeService::preflight_cuts(&prepared.preflight);
        let result = self
            .apply_prepared_workspace_pull_request_create_unverified(
                db,
                workspace_merge_service,
                prepared,
            )
            .await?;
        workspace_merge_service
            .verify_committed_cuts(db, &cuts, "pull_request_create")
            .await?;
        Ok(result)
    }

    async fn apply_prepared_workspace_pull_request_create_unverified(
        &self,
        db: &mut ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        prepared: PreparedWorkspacePullRequestCreate,
    ) -> Result<CreateWorkspacePullRequestResult, G5Error> {
        self.reject_base_source_workspace(db, prepared.source_workspace_id)
            .await?;
        workspace_merge_service
            .revalidate_transaction_free_workspace_merge_preflight(db, &prepared.preflight)
            .await?;
        let names = self
            .load_workspace_names(
                db,
                prepared.source_workspace_id,
                prepared.target_workspace_id,
            )
            .await?;
        // Deletion closes every open PR of the workspace in the transaction
        // that removes it (`g5.workspace.delete.pull-requests`): hold both
        // live rows so a PR cannot open beside a concurrent deletion.
        let live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM (SELECT id FROM workspaces \
             WHERE id = ANY($1) AND deleted_at IS NULL FOR SHARE) live",
        )
        .bind([prepared.source_workspace_id, prepared.target_workspace_id])
        .fetch_one(db.conn())
        .await?;
        if live != 2 {
            return Err(G5Error::Conflict {
                reason: "The source or target workspace was deleted".to_string(),
            });
        }
        if names.org_id != prepared.org_id
            || names.source_branch != prepared.source_branch
            || names.target_branch != prepared.target_branch
        {
            return Err(G5Error::Conflict {
                reason: "Workspace PR identities changed after preflight; retry".to_string(),
            });
        }
        let preflight = prepared.preflight.response();
        let local_merge_status = classify_local_merge_status(preflight);
        let evidence = evidence_from_preflight(preflight)?;

        if let Some(existing) = self
            .load_open_pr_for_pair(
                db,
                prepared.source_workspace_id,
                prepared.target_workspace_id,
            )
            .await?
        {
            // Spec D15: a supplied description refreshes the stored override on
            // the idempotent path; an absent one never clears it.
            if let Some(title) = &prepared.title_override {
                self.store_title_override(db, existing.id, title).await?;
            }
            self.store_description_override(db, existing.id, &prepared.description_override)
                .await?;
            let summary = self
                .refresh_pull_request(
                    db,
                    existing.id,
                    prepared.actor_id,
                    preflight,
                    local_merge_status,
                    "refreshed",
                )
                .await?;
            return Ok(CreateWorkspacePullRequestResult {
                pull_request: summary,
                already_existed: true,
                dirty_drafts: prepared.dirty_drafts,
            });
        }

        let row_id = Uuid::new_v4();
        // The number trigger skips numbers the organization has used, so only
        // a concurrent creation that took the same number fails its unique
        // key; take another (`g5.git.pr-local-number`).
        let mut attempt = 0;
        let inserted_id: Option<Uuid> = loop {
            attempt += 1;
            sqlx::query("SAVEPOINT pull_request_number")
                .execute(db.conn())
                .await?;
            let inserted = sqlx::query_scalar(
                r#"
                    INSERT INTO workspace_pull_requests (
                        id, org_id, source_workspace_id, target_workspace_id, status,
                        local_merge_status, provider_status, title, description_override,
                        source_heads, target_heads, preflight, last_preflight_fingerprint,
                        created_by, created_at, updated_at
                    )
                    VALUES (
                        $1, $2, $3, $4, 'open',
                        $5, 'local_only', $6, $7,
                        $8, $9, $10, $11, $12,
                        now(), now()
                    )
                    ON CONFLICT (source_workspace_id, target_workspace_id)
                        WHERE status = 'open'
                        DO NOTHING
                    RETURNING id
                    "#,
            )
            .bind(row_id)
            .bind(names.org_id)
            .bind(prepared.source_workspace_id)
            .bind(prepared.target_workspace_id)
            .bind(local_merge_status)
            .bind(&prepared.title)
            .bind(prepared.description_override.insert_value())
            .bind(&evidence.source_heads)
            .bind(&evidence.target_heads)
            .bind(&evidence.preflight)
            .bind(&preflight.preflight_fingerprint)
            .bind(prepared.actor_id)
            .fetch_optional(db.conn())
            .await;
            match inserted {
                Err(sqlx::Error::Database(error))
                    if attempt < 3
                        && error.constraint() == Some("workspace_pull_requests_org_number_key") =>
                {
                    sqlx::query("ROLLBACK TO SAVEPOINT pull_request_number")
                        .execute(db.conn())
                        .await?;
                }
                inserted => {
                    let inserted = inserted?;
                    sqlx::query("RELEASE SAVEPOINT pull_request_number")
                        .execute(db.conn())
                        .await?;
                    break inserted;
                }
            }
        };

        if inserted_id.is_none() {
            if let Some(existing) = self
                .load_open_pr_for_pair(
                    db,
                    prepared.source_workspace_id,
                    prepared.target_workspace_id,
                )
                .await?
            {
                if let Some(title) = &prepared.title_override {
                    self.store_title_override(db, existing.id, title).await?;
                }
                self.store_description_override(db, existing.id, &prepared.description_override)
                    .await?;
                let summary = self
                    .refresh_pull_request(
                        db,
                        existing.id,
                        prepared.actor_id,
                        preflight,
                        local_merge_status,
                        "refreshed",
                    )
                    .await?;
                return Ok(CreateWorkspacePullRequestResult {
                    pull_request: summary,
                    already_existed: true,
                    dirty_drafts: prepared.dirty_drafts,
                });
            }
            return Err(G5Error::Conflict {
                reason: "Open workspace pull request changed during create".to_string(),
            });
        }

        self.insert_event(
            db,
            PullRequestEventInsert {
                org_id: names.org_id,
                pull_request_id: row_id,
                event_type: "created",
                actor_id: prepared.actor_id,
                evidence: Some(&evidence),
                message: None,
            },
        )
        .await?;

        let summary = self.load_pr_summary(db, row_id).await?;
        Ok(CreateWorkspacePullRequestResult {
            pull_request: summary,
            already_existed: false,
            dirty_drafts: prepared.dirty_drafts,
        })
    }

    async fn load_dirty_draft_summaries(
        &self,
        _db: &mut ScopedDb,
        _source_workspace_id: Uuid,
        _target_workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceDirtyDraftSummary>, G5Error> {
        Ok(Vec::new())
    }

    pub async fn list_workspace_pull_requests(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        role: WorkspacePullRequestRole,
        status: Option<&str>,
    ) -> Result<Vec<WorkspacePullRequestSummary>, G5Error> {
        let rows = match role {
            WorkspacePullRequestRole::Source => {
                self.list_prs(db, "source", workspace_id, status).await?
            }
            WorkspacePullRequestRole::Target => {
                self.list_prs(db, "target", workspace_id, status).await?
            }
            WorkspacePullRequestRole::All => self.list_prs(db, "all", workspace_id, status).await?,
        };
        Ok(rows)
    }

    pub async fn merge_workspace_pull_request(
        &self,
        mut db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        target_workspace_id: Uuid,
        pull_request_id: Uuid,
        actor_id: Uuid,
        merge_override: Option<&MergeOverrideIntent>,
    ) -> Result<MergeWorkspacePullRequestOutcome, G5Error> {
        let observed = self
            .load_pr_for_target_workspace(&mut db, pull_request_id, target_workspace_id)
            .await?;
        if observed.status != "open" {
            return Err(G5Error::Conflict {
                reason: format!("Workspace pull request is {}", observed.status),
            });
        }

        let provider_summary = self.load_pr_summary(&mut db, pull_request_id).await?;
        let provider_first =
            crate::services::git::remote_integrations::has_bindings(db.conn(), target_workspace_id)
                .await?
                || !provider_summary.provider_repos.is_empty();
        let scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close workspace PR merge capture transaction: {error}"),
        })?;

        // Remote-bound targets use provider acceptance regardless of the Base flag.
        // No local target ref moves before the provider has accepted the merge.
        if provider_first {
            let preflight_db =
                Self::open_merge_scope(&scope, "provider-first PR preflight").await?;
            let preflight = workspace_merge_service
                .prepare_transaction_free_workspace_merge_preflight(
                    preflight_db,
                    observed.source_workspace_id,
                    target_workspace_id,
                    WorkspaceMergePreflightMode::TargetMergeExecution,
                )
                .await?;
            if classify_local_merge_status(preflight.response()) != "ready"
                && provider_summary.provider_status != "merged"
                && !provider_summary
                    .provider_repos
                    .iter()
                    .any(|repo| repo.provider_status == "merged")
            {
                return self
                    .persist_blocked_workspace_pull_request_merge(
                        &scope,
                        &observed,
                        actor_id,
                        preflight.into_response(),
                    )
                    .await;
            }
            // Admission before any provider mutation
            // (g5.governance.approval.requirements): the evidence is pinned
            // on the pull request so the import after provider acceptance
            // re-reads it instead of evaluating again. Once any provider
            // repository accepted the merge (all of them, or a partial
            // acceptance a failed sibling left behind) the admission recorded
            // for the same cut stands: the provider merge is irreversible,
            // so approvals withdrawn since cannot strand it. The route
            // reconciles unacknowledged provider outcomes before this read.
            let provider_accepted = provider_summary.provider_status == "merged"
                || provider_summary
                    .provider_repos
                    .iter()
                    .any(|repo| repo.provider_status == "merged");
            self.admit_pull_request_merge(
                &scope,
                workspace_merge_service,
                &observed,
                &preflight,
                provider_accepted,
                merge_override,
            )
            .await?;
            return Ok(MergeWorkspacePullRequestOutcome::ProviderFirstMerge(
                Box::new(ProviderFirstMergeRequest {
                    pull_request_id,
                    org_id: observed.org_id,
                    source_workspace_id: observed.source_workspace_id,
                    target_workspace_id,
                    actor_id,
                    target_heads: preflight
                        .response()
                        .repo_results
                        .iter()
                        .map(|repo| ProviderMergeTargetHead {
                            repo_id: repo.repo_id,
                            head_commit: repo.target_head.clone(),
                        })
                        .collect(),
                }),
            ));
        }

        let preflight_db = Self::open_merge_scope(&scope, "local PR merge preflight").await?;
        match workspace_merge_service
            .prepare_local_workspace_pull_request_merge(
                preflight_db,
                observed.source_workspace_id,
                target_workspace_id,
                actor_id,
                merge_override,
            )
            .await?
        {
            PreparedLocalWorkspaceMerge::Blocked(preflight) => {
                self.persist_blocked_workspace_pull_request_merge(
                    &scope, &observed, actor_id, preflight,
                )
                .await
            }
            PreparedLocalWorkspaceMerge::Ready(merge) => {
                let preflight = merge.preflight().clone();
                Ok(MergeWorkspacePullRequestOutcome::PreparedLocal(Box::new(
                    PreparedLocalWorkspacePullRequestMerge {
                        pull_request_id,
                        org_id: observed.org_id,
                        source_workspace_id: observed.source_workspace_id,
                        target_workspace_id,
                        actor_id,
                        preflight,
                        merge: *merge,
                    },
                )))
            }
        }
    }

    async fn open_merge_scope(scope: &ScopedDbReopener, phase: &str) -> Result<ScopedDb, G5Error> {
        scope.open().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to open {phase} transaction: {error}"),
        })
    }

    /// Evaluate and record merge admission for the pinned preflight cut, in
    /// its own committed transaction under the target's requirement locks.
    /// When the provider has already accepted the merge, an admission
    /// recorded for the same cut stands even if approvals changed since.
    async fn admit_pull_request_merge(
        &self,
        scope: &ScopedDbReopener,
        workspace_merge_service: &WorkspaceMergeService,
        observed: &WorkspacePullRequestRow,
        preflight: &PreparedWorkspaceMergePreflight,
        provider_accepted: bool,
        merge_override: Option<&MergeOverrideIntent>,
    ) -> Result<MergeAdmissionEvidence, G5Error> {
        let mut db = Self::open_merge_scope(scope, "pull request merge admission").await?;
        let retained_attempt: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND (merge_pending OR merge_accepted))")
            .bind(observed.id).fetch_one(db.conn()).await?;
        if provider_accepted || retained_attempt {
            if let Some(recorded) = self.recorded_merge_admission(&mut db, observed.id).await? {
                let pinned = WorkspaceMergeService::preflight_cuts(preflight)
                    .into_iter()
                    .find(|(workspace_id, _)| *workspace_id == observed.source_workspace_id)
                    .map(|(_, heads)| heads)
                    .unwrap_or_default();
                let mut recorded_heads = recorded
                    .source_heads
                    .iter()
                    .map(|head| {
                        (
                            head.repo_id.clone(),
                            head.branch_name.clone(),
                            head.head_commit.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                recorded_heads.sort();
                let mut pinned_heads = pinned
                    .into_iter()
                    .map(|(repo_id, branch, head)| (repo_id.to_string(), branch, head))
                    .collect::<Vec<_>>();
                pinned_heads.sort();
                if recorded.admitted && recorded_heads == pinned_heads {
                    db.rollback().await.map_err(|error| G5Error::Internal {
                        message: format!("Failed to close merge admission read: {error}"),
                    })?;
                    return Ok(recorded);
                }
            }
            if retained_attempt {
                // The route reconciles terminal provider outcomes before this
                // guard, so source drift cannot prevent releasing that target.
                // Reusing admission must not dispatch or import a different cut.
                return Err(G5Error::Conflict { reason: "An unresolved or accepted provider merge retains its original source admission; reconcile it before admitting another revision".into() });
            }
            // An outside merge may have no G5 attempt or prior admission. It
            // still needs ordinary admission before G5 imports its history.
        }
        let admission = workspace_merge_service
            .admit_prepared_pull_request_merge(&mut db, preflight, merge_override)
            .await?;
        let evidence = serde_json::to_value(&admission).map_err(|error| G5Error::Internal {
            message: format!("Failed to serialize merge admission evidence: {error}"),
        })?;
        sqlx::query(
            r#"
            UPDATE workspace_pull_requests
               SET merge_admission = $1, updated_at = now()
             WHERE id = $2 AND status = 'open'
            "#,
        )
        .bind(&evidence)
        .bind(observed.id)
        .execute(db.conn())
        .await?;
        // An override is recorded with its admission, before any provider
        // effect, so one GitHub then refuses stays visible.
        self.insert_override_event(&mut db, observed.org_id, observed.id, &admission)
            .await?;
        db.commit().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to commit merge admission evidence: {error}"),
        })?;
        Ok(admission)
    }

    async fn recorded_merge_admission(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<Option<MergeAdmissionEvidence>, G5Error> {
        let recorded: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT merge_admission FROM workspace_pull_requests WHERE id = $1")
                .bind(pull_request_id)
                .fetch_optional(db.conn())
                .await?
                .flatten();
        recorded
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| G5Error::Internal {
                message: format!("Recorded merge admission evidence is malformed: {error}"),
            })
    }

    async fn persist_blocked_workspace_pull_request_merge(
        &self,
        scope: &ScopedDbReopener,
        observed: &WorkspacePullRequestRow,
        actor_id: Uuid,
        preflight: MergePreflightResponse,
    ) -> Result<MergeWorkspacePullRequestOutcome, G5Error> {
        let mut db = Self::open_merge_scope(scope, "blocked PR evidence application").await?;
        let locked = self
            .lock_pr_for_target_workspace(&mut db, observed.id, observed.target_workspace_id)
            .await?;
        if locked.status != "open"
            || locked.org_id != observed.org_id
            || locked.source_workspace_id != observed.source_workspace_id
            || locked.target_workspace_id != observed.target_workspace_id
        {
            return Err(G5Error::Conflict {
                reason: "Workspace pull request changed before blocked evidence application"
                    .to_string(),
            });
        }

        let local_merge_status = classify_local_merge_status(&preflight);
        let evidence = evidence_from_preflight(&preflight)?;
        self.update_pr_evidence(
            &mut db,
            observed.id,
            local_merge_status,
            &preflight,
            &evidence,
        )
        .await?;
        let reason = preflight_message(&preflight)
            .unwrap_or_else(|| "Workspace pull request is not mergeable".to_string());
        self.insert_event(
            &mut db,
            PullRequestEventInsert {
                org_id: observed.org_id,
                pull_request_id: observed.id,
                event_type: "merge_blocked",
                actor_id,
                evidence: Some(&evidence),
                message: Some(reason.clone()),
            },
        )
        .await?;
        db.commit().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to commit blocked workspace PR evidence: {error}"),
        })?;
        Ok(MergeWorkspacePullRequestOutcome::Blocked { reason })
    }

    pub async fn prepare_provider_workspace_pull_request_merge(
        &self,
        mut db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        request: ProviderFirstMergeRequest,
        imports: Vec<
            crate::services::git::workspace_pull_request_provider_lifecycle::AcceptedProviderMerge,
        >,
    ) -> Result<PreparedLocalWorkspacePullRequestMerge, G5Error> {
        let observed = self
            .load_pr_for_target_workspace(
                &mut db,
                request.pull_request_id,
                request.target_workspace_id,
            )
            .await?;
        let provider_summary = self
            .load_pr_summary(&mut db, request.pull_request_id)
            .await?;
        if observed.status != "open"
            || !(provider_summary.provider_status == "merged"
                || (provider_summary.provider_status == "local_only"
                    && provider_summary.provider_repos.is_empty()))
            || observed.org_id != request.org_id
            || observed.source_workspace_id != request.source_workspace_id
        {
            return Err(G5Error::Conflict {
                reason: "Provider merge identity or acceptance changed before import".into(),
            });
        }
        // Re-read the admission pinned before the provider merge
        // (g5.governance.merge): approvals that changed since do not strand
        // an accepted provider PR. Without a recorded one (a caller that did
        // not go through the merge route) the preparation admits itself.
        let admission = self
            .recorded_merge_admission(&mut db, request.pull_request_id)
            .await?;
        let merge = workspace_merge_service
            .prepare_provider_workspace_pull_request_merge(
                db,
                request.source_workspace_id,
                request.target_workspace_id,
                request.actor_id,
                imports,
                admission,
            )
            .await?;
        Ok(PreparedLocalWorkspacePullRequestMerge {
            pull_request_id: request.pull_request_id,
            org_id: request.org_id,
            source_workspace_id: request.source_workspace_id,
            target_workspace_id: request.target_workspace_id,
            actor_id: request.actor_id,
            preflight: merge.preflight().clone(),
            merge,
        })
    }

    // LEAN-TODO(P6.4): retires with workspace pull requests.
    pub async fn apply_prepared_workspace_pull_request_merge(
        &self,
        db: &mut ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        prepared: PreparedLocalWorkspacePullRequestMerge,
    ) -> Result<MergeWorkspacePullRequestResult, G5Error> {
        let overridden = prepared
            .merge
            .approval_admission()
            .filter(|admission| admission.merge_override.is_some())
            .cloned();
        if let Some(record) = overridden
            .as_ref()
            .and_then(|admission| admission.merge_override.as_ref())
        {
            // Preparation released the admission locks. Recheck before local
            // effects, except recovery of a cut already accepted by a provider:
            // that completes the recorded admission, not a new override.
            let provider_accepted: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos
                 WHERE pull_request_id = $1 AND (merge_accepted OR provider_status = 'merged'))",
            )
            .bind(prepared.pull_request_id)
            .fetch_one(db.conn())
            .await?;
            if !provider_accepted {
                // Match application's fence order before taking the new
                // authority row lock. Reacquiring these fences is reentrant.
                let mut workspaces = [prepared.source_workspace_id, prepared.target_workspace_id];
                workspaces.sort();
                for workspace_id in workspaces {
                    acquire_workspace_commit_fence(db.conn(), workspace_id).await?;
                }
                workspace_merge_service
                    .require_current_override_authority(
                        db,
                        prepared.source_workspace_id,
                        prepared.target_workspace_id,
                        record,
                    )
                    .await?;
            }
        }
        let admission = prepared
            .merge
            .approval_admission()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to serialize merge admission evidence: {error}"),
            })?;
        let completed = workspace_merge_service
            .apply_prepared_workspace_merge(db, prepared.merge)
            .await?;
        let locked = self
            .lock_pr_for_target_workspace(
                db,
                prepared.pull_request_id,
                prepared.target_workspace_id,
            )
            .await?;
        if locked.status != "open"
            || locked.org_id != prepared.org_id
            || locked.source_workspace_id != prepared.source_workspace_id
            || locked.target_workspace_id != prepared.target_workspace_id
        {
            return Err(G5Error::Conflict {
                reason: "Workspace pull request changed before merge publication".to_string(),
            });
        }
        // A provider-first merge recorded its admission, and its override
        // event, before the provider merge; only an override first recorded
        // here (a local merge) gets its event now.
        let already_recorded = self
            .recorded_merge_admission(db, prepared.pull_request_id)
            .await?
            .and_then(|recorded| recorded.merge_override);
        // The merge record shows the admission it landed under.
        sqlx::query("UPDATE workspace_pull_requests SET merge_admission = $1 WHERE id = $2")
            .bind(&admission)
            .bind(prepared.pull_request_id)
            .execute(db.conn())
            .await?;
        if let Some(admission) = overridden
            .as_ref()
            .filter(|admission| already_recorded != admission.merge_override)
        {
            self.insert_override_event(db, prepared.org_id, prepared.pull_request_id, admission)
                .await?;
        }

        let authority_advance =
            completed
                .authority_advance
                .as_ref()
                .ok_or_else(|| G5Error::Internal {
                    message:
                        "Prepared local workspace PR merge did not produce an authority advance"
                            .to_string(),
                })?;
        let evidence = evidence_from_landed_target_cut(
            &prepared.preflight,
            authority_advance.workspace_id(),
            authority_advance.members(),
            prepared.target_workspace_id,
        )?;
        register_consumer_input(
            db.conn(),
            authority_advance.advance_id(),
            WORKSPACE_PULL_REQUEST_MERGE_CONSUMER,
            &WorkspacePullRequestMergeConsumerInput {
                pull_request_id: prepared.pull_request_id,
                org_id: prepared.org_id,
                source_workspace_id: prepared.source_workspace_id,
                target_workspace_id: prepared.target_workspace_id,
                actor_id: prepared.actor_id,
                preflight_fingerprint: prepared.preflight.preflight_fingerprint.clone(),
                evidence,
            },
        )
        .await?;
        let pull_request = self.load_pr_summary(db, prepared.pull_request_id).await?;
        Ok(MergeWorkspacePullRequestResult {
            pull_request,
            run: completed.run,
            authority_advance: completed.authority_advance,
        })
    }

    /// Mark the workspace pull request merged inside the landing transaction of
    /// its target cut, after validating the completed target merge (see
    /// [`Self::land_workspace_pull_request_merge`]). A receipt already recorded
    /// for the advance makes it a no-op that returns `false`.
    // LEAN-TODO(P6.4): retires with workspace pull requests.
    pub async fn apply_workspace_pull_request_merge_consumer(
        &self,
        db: &mut ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        cut: &LandedCutRecord,
    ) -> Result<bool, G5Error> {
        if consumer_receipt_result::<bool>(
            db.conn(),
            cut.advance_id,
            PROVIDER_FIRST_BASE_RECEIPT_CLAIM_CONSUMER,
        )
        .await?
        .is_none()
            && load_consumer_input::<ProviderFirstMergeRequest>(
                db.conn(),
                cut.advance_id,
                PROVIDER_FIRST_BASE_RECEIPT_CLAIM_CONSUMER,
            )
            .await?
            .is_some()
        {
            record_consumer_receipt(
                db.conn(),
                cut.advance_id,
                PROVIDER_FIRST_BASE_RECEIPT_CLAIM_CONSUMER,
                Some(&true),
            )
            .await?;
        }
        if consumer_receipt_result::<bool>(
            db.conn(),
            cut.advance_id,
            WORKSPACE_PULL_REQUEST_MERGE_CONSUMER,
        )
        .await?
        .is_some()
        {
            return Ok(false);
        }
        let Some(input) = load_consumer_input::<WorkspacePullRequestMergeConsumerInput>(
            db.conn(),
            cut.advance_id,
            WORKSPACE_PULL_REQUEST_MERGE_CONSUMER,
        )
        .await?
        else {
            return Ok(false);
        };
        if cut.target
            != (AuthorityTarget::Workspace {
                workspace_id: input.target_workspace_id,
            })
        {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Workspace pull request {} has no landed target publication",
                    input.pull_request_id
                ),
            });
        }
        workspace_merge_service
            .require_completed_target_merge(
                db,
                cut.advance_id,
                input.source_workspace_id,
                input.target_workspace_id,
            )
            .await?;

        let evidence_members = input
            .evidence
            .target_heads
            .as_array()
            .ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "pull_request_id": input.pull_request_id,
                    "message": "Workspace pull request merge evidence has no complete target vector",
                }),
            })?;
        if evidence_members.len() != cut.members.len()
            || cut.members.iter().any(|repo| {
                !evidence_members.iter().any(|member| {
                    member
                        .get("repoId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok())
                        == Some(repo.repo_id)
                        && member
                            .get("workspaceId")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| Uuid::parse_str(value).ok())
                            == Some(repo.workspace_id)
                        && member.get("headCommit").and_then(serde_json::Value::as_str)
                            == Some(repo.proposed_new_head.as_str())
                })
            })
        {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Workspace pull request {} evidence differs from its committed target cut",
                    input.pull_request_id
                ),
            });
        }

        let locked = self
            .lock_pr_for_target_workspace(db, input.pull_request_id, input.target_workspace_id)
            .await?;
        if locked.status != "open"
            || locked.org_id != input.org_id
            || locked.source_workspace_id != input.source_workspace_id
            || locked.target_workspace_id != input.target_workspace_id
        {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Workspace pull request {} changed before receipt reconciliation",
                    input.pull_request_id
                ),
            });
        }

        // Resolve while finalizing the locked PR and freeze this result in the
        // merge transaction. Later personal-default changes cannot rearm it.
        let stop_choice = self
            .effective_stop_choices(db, &[input.pull_request_id])
            .await?
            .remove(&input.pull_request_id)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace pull request {}", input.pull_request_id),
            })?;
        let merged_at = Utc::now();
        let merged_rows = sqlx::query(
            r#"
            UPDATE workspace_pull_requests
               SET status = 'merged',
                   local_merge_status = 'merged',
                   source_heads = $1,
                   target_heads = $2,
                   preflight = $3,
                   last_preflight_fingerprint = $4,
                   merged_by = $5,
                   merged_at = $6,
                   stop_vm_after_merge = $8,
                   updated_at = $6
             WHERE id = $7
               AND status = 'open'
            "#,
        )
        .bind(&input.evidence.source_heads)
        .bind(&input.evidence.target_heads)
        .bind(&input.evidence.preflight)
        .bind(&input.preflight_fingerprint)
        .bind(input.actor_id)
        .bind(merged_at)
        .bind(input.pull_request_id)
        .bind(stop_choice)
        .execute(db.conn())
        .await?
        .rows_affected();
        if merged_rows != 1 {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Workspace pull request {} is no longer open",
                    input.pull_request_id
                ),
            });
        }
        self.insert_event(
            db,
            PullRequestEventInsert {
                org_id: input.org_id,
                pull_request_id: input.pull_request_id,
                event_type: "merged",
                actor_id: input.actor_id,
                evidence: Some(&input.evidence),
                message: None,
            },
        )
        .await?;
        // A merge that waived G5's requirements decides nothing for anyone:
        // the pull request's pending review requests close as bypassed
        // (g5.governance.approval.request-status).
        if self
            .recorded_merge_admission(db, input.pull_request_id)
            .await?
            .and_then(|admission| admission.merge_override)
            .is_some_and(|record| record.waives_g5())
        {
            sqlx::query(
                r#"
                UPDATE approval_requests
                   SET state = 'bypassed',
                       closed_at = transaction_timestamp(),
                       updated_at = transaction_timestamp()
                 WHERE org_id = $1
                   AND state = 'pending'
                   AND (pull_request_id = $2
                        OR (without_pull_request
                            AND source_workspace_id = (SELECT source_workspace_id
                                                         FROM workspace_pull_requests
                                                        WHERE id = $2)))
                "#,
            )
            .bind(input.org_id)
            .bind(input.pull_request_id)
            .execute(db.conn())
            .await?;
        }
        record_consumer_receipt(
            db.conn(),
            cut.advance_id,
            WORKSPACE_PULL_REQUEST_MERGE_CONSUMER,
            Some(&true),
        )
        .await?;
        Ok(true)
    }

    /// Land a local workspace pull request with its completed merge run, merged
    /// state, evidence event and receipt in the same transaction as the target
    /// cut. A finalization failure leaves the refs unchanged and the PR open.
    // LEAN-TODO(P6.4): retires with workspace pull requests.
    pub async fn land_workspace_pull_request_merge(
        &self,
        workspace_merge_service: &WorkspaceMergeService,
        advance: PreparedBranchCommit,
        db: ScopedDb,
        event_bus: &EventBus,
        repo_storage: &TenantRepoStorage,
    ) -> Result<LandedBranchCommit, AppError> {
        let service = self.clone();
        let workspace_merge_service = workspace_merge_service.clone();
        let advance_id = advance.advance_id();
        let workspace_id = advance.workspace_id();
        let target = advance.target().clone();
        let (landed, ()) = advance
            .land_with(
                db,
                event_bus,
                repo_storage,
                FenceMode::Blocking,
                move |db, members| {
                    Box::pin(async move {
                        let cut = LandedCutRecord {
                            advance_id,
                            workspace_id,
                            target,
                            members: members.to_vec(),
                        };
                        if !service
                            .apply_workspace_pull_request_merge_consumer(db, &workspace_merge_service, &cut)
                            .await?
                        {
                            return Err(G5Error::Internal {
                                message: format!(
                                    "Workspace pull request merge {advance_id} has no unsettled pull request input to land with"
                                ),
                            }
                            .into());
                        }
                        Ok::<(), AppError>(())
                    })
                },
            )
            .await
            .map_err(AppError::from)?;
        Ok(landed)
    }

    pub async fn prepare_workspace_pull_request_refresh(
        &self,
        db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        path_workspace_id: Uuid,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<PreparedWorkspacePullRequestRefresh, G5Error> {
        self.prepare_pull_request_refresh(
            db,
            workspace_merge_service,
            path_workspace_id,
            pull_request_id,
            actor_id,
            WorkspaceMergePreflightMode::EvidenceCapture,
        )
        .await
    }

    pub async fn prepare_published_workspace_pull_request_refresh(
        &self,
        db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        source_workspace_id: Uuid,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<PreparedWorkspacePullRequestRefresh, G5Error> {
        self.prepare_pull_request_refresh(
            db,
            workspace_merge_service,
            source_workspace_id,
            pull_request_id,
            actor_id,
            WorkspaceMergePreflightMode::PublishedEvidenceCapture,
        )
        .await
    }

    async fn prepare_pull_request_refresh(
        &self,
        mut db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        path_workspace_id: Uuid,
        pull_request_id: Uuid,
        actor_id: Uuid,
        mode: WorkspaceMergePreflightMode,
    ) -> Result<PreparedWorkspacePullRequestRefresh, G5Error> {
        let observed = self
            .load_pr_for_path_workspace(&mut db, pull_request_id, path_workspace_id)
            .await?;
        if observed.status != "open" {
            return Err(G5Error::Conflict {
                reason: format!("Workspace pull request is {}", observed.status),
            });
        }

        let preflight = workspace_merge_service
            .prepare_transaction_free_workspace_merge_preflight(
                db,
                observed.source_workspace_id,
                observed.target_workspace_id,
                mode,
            )
            .await?;
        Ok(PreparedWorkspacePullRequestRefresh {
            observed,
            path_workspace_id,
            actor_id,
            preflight,
        })
    }

    pub async fn apply_prepared_workspace_pull_request_refresh(
        &self,
        db: &mut ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        prepared: PreparedWorkspacePullRequestRefresh,
    ) -> Result<WorkspacePullRequestSummary, G5Error> {
        let cuts = WorkspaceMergeService::preflight_cuts(&prepared.preflight);
        workspace_merge_service
            .revalidate_transaction_free_workspace_merge_preflight(db, &prepared.preflight)
            .await?;
        let locked = self
            .lock_pr_for_path_workspace(db, prepared.observed.id, prepared.path_workspace_id)
            .await?;
        if locked.status != "open"
            || locked.org_id != prepared.observed.org_id
            || locked.source_workspace_id != prepared.observed.source_workspace_id
            || locked.target_workspace_id != prepared.observed.target_workspace_id
        {
            return Err(G5Error::Conflict {
                reason: "Workspace pull request changed after preflight; retry".to_string(),
            });
        }
        let preflight = prepared.preflight.response();
        let local_merge_status = classify_local_merge_status(preflight);
        let summary = self
            .refresh_pull_request(
                db,
                prepared.observed.id,
                prepared.actor_id,
                preflight,
                local_merge_status,
                "refreshed",
            )
            .await?;
        // The refreshed evidence is one committed cut of each workspace
        // (`g5.fork.source.cut`); these fences are the transaction's last locks.
        workspace_merge_service
            .verify_committed_cuts(db, &cuts, "pull_request_refresh")
            .await?;
        Ok(summary)
    }

    /// Close every open PR into or out of a workspace being deleted, as an
    /// ordinary close does, each with a closed event that names the deletion
    /// (`g5.workspace.delete.pull-requests`). Deletion runs it in its own
    /// transaction before removing the workspace, and again in the
    /// tombstone's transaction for any opened since; PR creation holds both
    /// live workspace rows, so none can open beside that. Provider PRs are
    /// left as they are.
    pub async fn close_deleted_workspace_pull_requests(
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_id: Uuid,
    ) -> Result<u64, G5Error> {
        let closed: Vec<(Uuid, Uuid, bool)> = sqlx::query_as(
            r#"
            UPDATE workspace_pull_requests
               SET status = 'closed',
                   local_merge_status = 'closed',
                   updated_at = now()
             WHERE status = 'open'
               AND (source_workspace_id = $1 OR target_workspace_id = $1)
            RETURNING org_id, id, source_workspace_id = $1
            "#,
        )
        .bind(workspace_id)
        .fetch_all(db.conn())
        .await?;
        for (org_id, pull_request_id, source_deleted) in &closed {
            sqlx::query(
                "INSERT INTO workspace_pull_request_events \
                 (org_id, pull_request_id, event_type, actor_id, message) \
                 VALUES ($1, $2, 'closed', $3, $4)",
            )
            .bind(org_id)
            .bind(pull_request_id)
            .bind(actor_id)
            .bind(deleted_workspace_close_message(*source_deleted))
            .execute(db.conn())
            .await?;
        }
        Ok(closed.len() as u64)
    }

    pub async fn close_workspace_pull_request(
        &self,
        db: &mut ScopedDb,
        path_workspace_id: Uuid,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<WorkspacePullRequestSummary, G5Error> {
        let locked = self
            .lock_pr_for_path_workspace(db, pull_request_id, path_workspace_id)
            .await?;
        if locked.status != "open" {
            return Err(G5Error::Conflict {
                reason: format!("Workspace pull request is {}", locked.status),
            });
        }

        let updated = sqlx::query(
            r#"
            UPDATE workspace_pull_requests
               SET status = 'closed',
                   local_merge_status = 'closed',
                   updated_at = now()
             WHERE id = $1
               AND status = 'open'
            "#,
        )
        .bind(pull_request_id)
        .execute(db.conn())
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(G5Error::Conflict {
                reason: "Workspace pull request is no longer open".to_string(),
            });
        }
        self.insert_event(
            db,
            PullRequestEventInsert {
                org_id: locked.org_id,
                pull_request_id,
                event_type: "closed",
                actor_id,
                evidence: None,
                message: None,
            },
        )
        .await?;
        self.load_pr_summary(db, pull_request_id).await
    }

    async fn reject_base_source_workspace(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        let is_base: Option<bool> = sqlx::query_scalar(
            "SELECT is_base FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(source_workspace_id)
        .fetch_optional(db.conn())
        .await?;
        match is_base {
            Some(true) => Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Base workspace cannot create a source pull request",
                    "source_workspace_id": source_workspace_id,
                }),
            }),
            Some(false) => Ok(()),
            None => Err(G5Error::NotFound {
                resource: format!("workspace {source_workspace_id}"),
            }),
        }
    }

    /// Spec D14: commit-derived PR text a create would use, computed from the
    /// same evidence-capture preflight and [D10] repo-selection rule, without
    /// writing any rows. Never fails because derivation failed.
    pub async fn preview_workspace_pull_request_text(
        &self,
        mut db: ScopedDb,
        workspace_merge_service: &WorkspaceMergeService,
        repo_storage: TenantRepoStorage,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<WorkspacePullRequestTextPreview, G5Error> {
        self.reject_base_source_workspace(&mut db, source_workspace_id)
            .await?;
        let names = self
            .load_workspace_names(&mut db, source_workspace_id, target_workspace_id)
            .await?;
        let repo_storage = repo_storage.scoped(&db);
        let preflight = workspace_merge_service
            .prepare_transaction_free_workspace_merge_preflight(
                db,
                source_workspace_id,
                target_workspace_id,
                WorkspaceMergePreflightMode::EvidenceCapture,
            )
            .await?;
        // Spec D16: the workspace PR-body Project Doc at the docs repo's
        // snapshot head is the preferred body source, independent of the
        // [D10] repo selection that drives the title.
        let source_names: Vec<String> = std::iter::once(names.source_name.clone())
            .chain(names.source_former_names.iter().cloned())
            .collect();
        let org_id = names.org_id;
        let pr_body_doc = read_workspace_pr_body_doc(
            repo_storage.clone(),
            org_id,
            preflight.response(),
            names.source_docs_repo_id,
            source_workspace_id,
            &source_names,
        )
        .await;
        let derived = derive_local_pr_text(
            repo_storage.clone(),
            org_id,
            preflight.response(),
            names.source_docs_repo_id,
        )
        .await
        .map(|(_, _, text)| text);
        Ok(match (derived, pr_body_doc) {
            (Some(text), Some(body)) => WorkspacePullRequestTextPreview {
                title: text.title,
                body,
                derived: true,
                source: WorkspacePullRequestTextSource::ProjectDoc,
            },
            (Some(text), None) => WorkspacePullRequestTextPreview {
                title: text.title,
                body: text.body,
                derived: true,
                source: WorkspacePullRequestTextSource::Derived,
            },
            (None, Some(body)) => WorkspacePullRequestTextPreview {
                title: format!("{} into {}", names.source_name, names.target_name),
                body,
                derived: false,
                source: WorkspacePullRequestTextSource::ProjectDoc,
            },
            (None, None) => WorkspacePullRequestTextPreview {
                title: format!("{} into {}", names.source_name, names.target_name),
                body: String::new(),
                derived: false,
                source: WorkspacePullRequestTextSource::Legacy,
            },
        })
    }

    async fn store_title_override(
        &self,
        db: &mut ScopedDb,
        id: Uuid,
        title: &str,
    ) -> Result<(), G5Error> {
        let changed = sqlx::query(
            "UPDATE workspace_pull_requests SET title=$2 WHERE id=$1 AND status='open'",
        )
        .bind(id)
        .bind(title)
        .execute(db.conn())
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(G5Error::Conflict {
                reason: "Workspace pull request is no longer open; retry".into(),
            });
        }
        Ok(())
    }

    /// Spec D15: apply the three-state description change on the idempotent
    /// create path — `Unchanged` never touches the stored value, `Clear`
    /// nulls it, `Set` replaces it. Status-guarded: a PR that closed or
    /// merged after the open-PR lookup is left untouched.
    async fn store_description_override(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        change: &DescriptionOverrideChange,
    ) -> Result<(), G5Error> {
        let new_value: Option<&str> = match change {
            DescriptionOverrideChange::Unchanged => return Ok(()),
            DescriptionOverrideChange::Clear => None,
            DescriptionOverrideChange::Set(text) => Some(text),
        };
        sqlx::query(
            r#"
            UPDATE workspace_pull_requests
            SET description_override = $2, updated_at = now()
            WHERE id = $1
              AND status = 'open'
            "#,
        )
        .bind(pull_request_id)
        .bind(new_value)
        .execute(db.conn())
        .await?;
        Ok(())
    }

    /// `g5d pr edit` (`g5.git.pr-cli.edit`): store a new title and/or saved
    /// description on the open G5 PR from `source` into `target`, under the
    /// PR row lock, with no source save or publication. Returns its ID.
    pub async fn edit_open_pull_request_text(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        title: Option<&str>,
        description: Option<String>,
    ) -> Result<Uuid, G5Error> {
        let description = normalize_description_override(description)?;
        let Some(row) = self
            .load_open_pr_for_pair(db, source_workspace_id, target_workspace_id)
            .await?
        else {
            return Err(G5Error::NotFound {
                resource: "open G5 pull request from this workspace to that target".into(),
            });
        };
        // A close that won the lock wins: never edit, or touch, a closed PR.
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
                .bind(row.id)
                .fetch_optional(db.conn())
                .await?;
        if status.as_deref() != Some("open") {
            return Err(G5Error::NotFound {
                resource: "open G5 pull request from this workspace to that target".into(),
            });
        }
        if let Some(title) = title {
            self.store_title_override(db, row.id, title).await?;
        }
        self.store_description_override(db, row.id, &description)
            .await?;
        sqlx::query("UPDATE workspace_pull_requests SET updated_at=now() WHERE id=$1")
            .bind(row.id)
            .execute(db.conn())
            .await?;
        Ok(row.id)
    }

    async fn load_open_pr_for_pair(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<Option<WorkspacePullRequestRow>, G5Error> {
        Ok(sqlx::query_as::<_, WorkspacePullRequestRow>(
            r#"
            SELECT pr.id, pr.org_id, pr.source_workspace_id, pr.target_workspace_id, pr.status
            FROM workspace_pull_requests pr
            JOIN workspaces sw ON sw.id = pr.source_workspace_id
            JOIN workspaces tw ON tw.id = pr.target_workspace_id
            WHERE pr.source_workspace_id = $1
              AND pr.target_workspace_id = $2
              AND pr.status = 'open'
              AND sw.deleted_at IS NULL
              AND tw.deleted_at IS NULL
            "#,
        )
        .bind(source_workspace_id)
        .bind(target_workspace_id)
        .fetch_optional(db.conn())
        .await?)
    }

    async fn refresh_pull_request(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
        preflight: &MergePreflightResponse,
        local_merge_status: &str,
        event_type: &str,
    ) -> Result<WorkspacePullRequestSummary, G5Error> {
        let evidence = evidence_from_preflight(preflight)?;
        self.update_pr_evidence(
            db,
            pull_request_id,
            local_merge_status,
            preflight,
            &evidence,
        )
        .await?;
        let org_id: Uuid =
            sqlx::query_scalar("SELECT org_id FROM workspace_pull_requests WHERE id = $1")
                .bind(pull_request_id)
                .fetch_one(db.conn())
                .await?;
        self.insert_event(
            db,
            PullRequestEventInsert {
                org_id,
                pull_request_id,
                event_type,
                actor_id,
                evidence: Some(&evidence),
                message: None,
            },
        )
        .await?;
        self.load_pr_summary(db, pull_request_id).await
    }

    async fn update_pr_evidence(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        local_merge_status: &str,
        preflight: &MergePreflightResponse,
        evidence: &PreflightEvidence,
    ) -> Result<(), G5Error> {
        sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(pull_request_id)
            .fetch_optional(db.conn())
            .await?;
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos WHERE pull_request_id=$1 AND (merge_pending OR merge_accepted))")
            .bind(pull_request_id).fetch_one(db.conn()).await?;
        if pending {
            return Err(G5Error::Conflict { reason: "Provider merge outcome is unresolved; reconcile it before refreshing PR evidence".into() });
        }
        sqlx::query(
            r#"
            UPDATE workspace_pull_requests
               SET local_merge_status = $1,
                   source_heads = $2,
                   target_heads = $3,
                   preflight = $4,
                   last_preflight_fingerprint = $5,
                   updated_at = now()
             WHERE id = $6
               AND status = 'open'
            "#,
        )
        .bind(local_merge_status)
        .bind(&evidence.source_heads)
        .bind(&evidence.target_heads)
        .bind(&evidence.preflight)
        .bind(&preflight.preflight_fingerprint)
        .bind(pull_request_id)
        .execute(db.conn())
        .await?;
        Ok(())
    }

    /// The `merge_overridden` event of an admission that an override admitted:
    /// the overriding person and their reason.
    async fn insert_override_event(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        pull_request_id: Uuid,
        admission: &MergeAdmissionEvidence,
    ) -> Result<(), G5Error> {
        let Some(record) = admission.merge_override.as_ref() else {
            return Ok(());
        };
        self.insert_event(
            db,
            PullRequestEventInsert {
                org_id,
                pull_request_id,
                event_type: "merge_overridden",
                actor_id: record.actor_id,
                evidence: None,
                message: Some(record.reason.clone()),
            },
        )
        .await
    }

    async fn insert_event(
        &self,
        db: &mut ScopedDb,
        event: PullRequestEventInsert<'_>,
    ) -> Result<(), G5Error> {
        sqlx::query(
            r#"
            INSERT INTO workspace_pull_request_events (
                org_id, pull_request_id, event_type, actor_id,
                source_heads, target_heads, preflight, message
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.org_id)
        .bind(event.pull_request_id)
        .bind(event.event_type)
        .bind(event.actor_id)
        .bind(event.evidence.map(|value| value.source_heads.clone()))
        .bind(event.evidence.map(|value| value.target_heads.clone()))
        .bind(event.evidence.map(|value| value.preflight.clone()))
        .bind(event.message)
        .execute(db.conn())
        .await?;
        Ok(())
    }

    async fn load_workspace_names(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<WorkspaceNames, G5Error> {
        let mut names = sqlx::query_as::<_, WorkspaceNames>(
            r#"
            SELECT sw.org_id,
                   sw.name AS source_name,
                   tw.name AS target_name,
                   sw.branch_name AS source_branch,
                   tw.branch_name AS target_branch,
                   sw.docs_repo_id AS source_docs_repo_id
            FROM workspaces sw
            JOIN workspaces tw ON tw.id = $2
            WHERE sw.id = $1
              AND sw.org_id = tw.org_id
              AND sw.deleted_at IS NULL
              AND tw.deleted_at IS NULL
            "#,
        )
        .bind(source_workspace_id)
        .bind(target_workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Source and target workspaces must exist in the same org",
                "source_workspace_id": source_workspace_id,
                "target_workspace_id": target_workspace_id,
            }),
        })?;
        names.source_former_names =
            crate::services::workspace_names::former_names(db.conn(), source_workspace_id).await?;
        Ok(names)
    }

    async fn lock_pr_for_target_workspace(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<WorkspacePullRequestRow, G5Error> {
        sqlx::query_as::<_, WorkspacePullRequestRow>(
            r#"
            SELECT pr.id, pr.org_id, pr.source_workspace_id, pr.target_workspace_id, pr.status
            FROM workspace_pull_requests pr
            JOIN workspaces sw ON sw.id = pr.source_workspace_id
            JOIN workspaces tw ON tw.id = pr.target_workspace_id
            WHERE pr.id = $1
              AND pr.target_workspace_id = $2
              AND sw.deleted_at IS NULL
              AND tw.deleted_at IS NULL
            FOR UPDATE OF pr
            "#,
        )
        .bind(pull_request_id)
        .bind(target_workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!(
                "workspace pull request {pull_request_id} for target workspace {target_workspace_id}"
            ),
        })
    }

    async fn load_pr_for_target_workspace(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<WorkspacePullRequestRow, G5Error> {
        sqlx::query_as::<_, WorkspacePullRequestRow>(
            r#"
            SELECT pr.id, pr.org_id, pr.source_workspace_id, pr.target_workspace_id, pr.status
            FROM workspace_pull_requests pr
            JOIN workspaces sw ON sw.id = pr.source_workspace_id
            JOIN workspaces tw ON tw.id = pr.target_workspace_id
            WHERE pr.id = $1
              AND pr.target_workspace_id = $2
              AND sw.deleted_at IS NULL
              AND tw.deleted_at IS NULL
            "#,
        )
        .bind(pull_request_id)
        .bind(target_workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!(
                "workspace pull request {pull_request_id} for target workspace {target_workspace_id}"
            ),
        })
    }

    async fn lock_pr_for_path_workspace(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        path_workspace_id: Uuid,
    ) -> Result<WorkspacePullRequestRow, G5Error> {
        sqlx::query_as::<_, WorkspacePullRequestRow>(
            r#"
            SELECT pr.id, pr.org_id, pr.source_workspace_id, pr.target_workspace_id, pr.status
            FROM workspace_pull_requests pr
            JOIN workspaces sw ON sw.id = pr.source_workspace_id
            JOIN workspaces tw ON tw.id = pr.target_workspace_id
            WHERE pr.id = $1
              AND (pr.source_workspace_id = $2 OR pr.target_workspace_id = $2)
              AND sw.deleted_at IS NULL
              AND tw.deleted_at IS NULL
            FOR UPDATE OF pr
            "#,
        )
        .bind(pull_request_id)
        .bind(path_workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!(
                "workspace pull request {pull_request_id} for workspace {path_workspace_id}"
            ),
        })
    }

    async fn load_pr_for_path_workspace(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        path_workspace_id: Uuid,
    ) -> Result<WorkspacePullRequestRow, G5Error> {
        sqlx::query_as::<_, WorkspacePullRequestRow>(
            r#"
            SELECT pr.id, pr.org_id, pr.source_workspace_id, pr.target_workspace_id, pr.status
            FROM workspace_pull_requests pr
            JOIN workspaces sw ON sw.id = pr.source_workspace_id
            JOIN workspaces tw ON tw.id = pr.target_workspace_id
            WHERE pr.id = $1
              AND (pr.source_workspace_id = $2 OR pr.target_workspace_id = $2)
              AND sw.deleted_at IS NULL
              AND tw.deleted_at IS NULL
            "#,
        )
        .bind(pull_request_id)
        .bind(path_workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!(
                "workspace pull request {pull_request_id} for workspace {path_workspace_id}"
            ),
        })
    }

    async fn load_pr_summary(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<WorkspacePullRequestSummary, G5Error> {
        let mut summary = sqlx::query_as::<_, WorkspacePullRequestSummary>(PR_SUMMARY_SQL_BY_ID)
            .bind(pull_request_id)
            .fetch_optional(db.conn())
            .await?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace pull request {pull_request_id}"),
            })?;
        self.attach_stop_choices(db, std::slice::from_mut(&mut summary))
            .await?;
        self.attach_provider_repos(db, std::slice::from_mut(&mut summary))
            .await?;
        self.adapters
            .attach(db, std::slice::from_mut(&mut summary))
            .await?;
        self.attach_merge_admission(db, std::slice::from_mut(&mut summary))
            .await?;
        Ok(summary)
    }

    /// Project the merge admission onto each summary: open pull requests are
    /// evaluated against their recorded source cut (lock-free, like the
    /// approvals page), merged ones show the admission they landed under.
    async fn attach_merge_admission(
        &self,
        db: &mut ScopedDb,
        summaries: &mut [WorkspacePullRequestSummary],
    ) -> Result<(), G5Error> {
        for summary in summaries {
            if summary.status != "open" {
                summary.merge_admission = summary
                    .recorded_merge_admission
                    .take()
                    .and_then(|value| serde_json::from_value(value).ok());
                if summary.status == "merged" {
                    summary.merge_override = summary
                        .merge_admission
                        .as_ref()
                        .and_then(|admission| admission.merge_override.clone());
                }
                continue;
            }
            let Ok(source_heads) =
                serde_json::from_value::<Vec<MergeSourceHead>>(summary.source_heads.clone())
            else {
                continue;
            };
            if source_heads.is_empty() {
                continue;
            }
            let org_id: Option<Uuid> =
                sqlx::query_scalar("SELECT org_id FROM workspaces WHERE id = $1")
                    .bind(summary.source_workspace_id)
                    .fetch_optional(db.conn())
                    .await?;
            let Some(org_id) = org_id else {
                continue;
            };
            summary.merge_admission = Some(
                self.merge_admission
                    .evaluate(
                        db,
                        org_id,
                        summary.source_workspace_id,
                        summary.target_workspace_id,
                        &source_heads,
                    )
                    .await?,
            );
        }
        Ok(())
    }

    async fn list_prs(
        &self,
        db: &mut ScopedDb,
        role: &str,
        workspace_id: Uuid,
        status: Option<&str>,
    ) -> Result<Vec<WorkspacePullRequestSummary>, G5Error> {
        let normalized_status = status.filter(|value| *value != "all");
        let mut summaries = sqlx::query_as::<_, WorkspacePullRequestSummary>(PR_SUMMARY_SQL_LIST)
            .bind(role)
            .bind(workspace_id)
            .bind(normalized_status)
            .bind(None::<Vec<Uuid>>)
            .fetch_all(db.conn())
            .await?;
        self.attach_stop_choices(db, &mut summaries).await?;
        self.attach_provider_repos(db, &mut summaries).await?;
        self.adapters.attach(db, &mut summaries).await?;
        self.attach_merge_admission(db, &mut summaries).await?;
        Ok(summaries)
    }

    /// Resolve an override's source through the PR owner, retaining the path
    /// target binding used by ordinary merge admission.
    pub async fn pull_request_source_for_target(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<Uuid, G5Error> {
        Ok(self
            .load_pr_for_target_workspace(db, pull_request_id, target_workspace_id)
            .await?
            .source_workspace_id)
    }

    pub async fn load_workspace_pull_request_summary(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<WorkspacePullRequestSummary, G5Error> {
        self.load_pr_summary(db, pull_request_id).await
    }

    /// The summaries of these PRs, in the order given; the caller has already
    /// decided they are readable (`g5.git.pr-reads`).
    pub async fn list_workspace_pull_requests_by_ids(
        &self,
        db: &mut ScopedDb,
        ids: &[Uuid],
    ) -> Result<Vec<WorkspacePullRequestSummary>, G5Error> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut summaries = sqlx::query_as::<_, WorkspacePullRequestSummary>(PR_SUMMARY_SQL_LIST)
            .bind("ids")
            .bind(Uuid::nil())
            .bind(None::<&str>)
            .bind(ids)
            .fetch_all(db.conn())
            .await?;
        self.attach_stop_choices(db, &mut summaries).await?;
        self.attach_provider_repos(db, &mut summaries).await?;
        self.adapters.attach(db, &mut summaries).await?;
        self.attach_merge_admission(db, &mut summaries).await?;
        summaries.sort_by_key(|summary| ids.iter().position(|id| *id == summary.id));
        Ok(summaries)
    }

    /// Resolve defaults only for source owners obtained from tenant-scoped PRs.
    /// Explicit choices, including the frozen result of a merge, need no lookup.
    async fn effective_stop_choices(
        &self,
        db: &mut ScopedDb,
        ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, bool>, G5Error> {
        let rows: Vec<(Uuid, Option<bool>, Option<Uuid>)> = sqlx::query_as(
            "SELECT pr.id, pr.stop_vm_after_merge, w.owner_user_id
             FROM workspace_pull_requests pr
             JOIN workspaces w ON w.id = pr.source_workspace_id
             WHERE pr.id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(db.conn())
        .await?;
        let owners: Vec<_> = rows
            .iter()
            .filter(|(_, choice, _)| choice.is_none())
            .filter_map(|(_, _, owner)| *owner)
            .collect();
        let defaults = crate::services::user_service::UserService::merge_stop_defaults(
            &self.system_pool,
            &owners,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, choice, owner)| {
                (
                    id,
                    choice.unwrap_or_else(|| {
                        owner
                            .and_then(|id| defaults.get(&id).copied())
                            .unwrap_or(false)
                    }),
                )
            })
            .collect())
    }

    async fn attach_stop_choices(
        &self,
        db: &mut ScopedDb,
        summaries: &mut [WorkspacePullRequestSummary],
    ) -> Result<(), G5Error> {
        if summaries.is_empty() {
            return Ok(());
        }
        let ids: Vec<_> = summaries.iter().map(|summary| summary.id).collect();
        let choices = self.effective_stop_choices(db, &ids).await?;
        for summary in summaries {
            summary.stop_vm_after_merge = choices.get(&summary.id).copied().unwrap_or(false);
        }
        Ok(())
    }

    async fn attach_provider_repos(
        &self,
        db: &mut ScopedDb,
        summaries: &mut [WorkspacePullRequestSummary],
    ) -> Result<(), G5Error> {
        if summaries.is_empty() {
            return Ok(());
        }
        let ids = summaries
            .iter()
            .map(|summary| summary.id)
            .collect::<Vec<_>>();
        let rows = sqlx::query_as::<_, ProviderRepoSummaryRow>(
            r#"
            SELECT pull_request_id,
                   id,
                   repo_id,
                   provider,
                   provider_repo_full_name,
                   source_remote_branch,
                   target_remote_branch,
                   provider_pr_number,
                   provider_pr_url,
                   provider_status,
                   source_head_sha,
                   target_head_sha,
                   CASE WHEN merge_accepted THEN merge_target_head_sha END AS merge_target_head_sha,
                   merge_pending,
                   provider_head_sha,
                   provider_base_sha,
                   last_error,
                   last_synced_at,
                   updated_at,
                   merged_outside_g5
              FROM workspace_pull_request_provider_repos
             WHERE pull_request_id = ANY($1)
             ORDER BY provider_repo_full_name, source_remote_branch
            "#,
        )
        .bind(&ids)
        .fetch_all(db.conn())
        .await?;
        let mut grouped: std::collections::HashMap<
            Uuid,
            Vec<WorkspacePullRequestProviderRepoSummary>,
        > = std::collections::HashMap::new();
        for row in rows {
            grouped
                .entry(row.pull_request_id)
                .or_default()
                .push(row.into_summary());
        }
        for summary in summaries {
            summary.provider_repos = grouped.remove(&summary.id).unwrap_or_default();
        }
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspaceNames {
    org_id: Uuid,
    source_name: String,
    target_name: String,
    source_branch: Option<String>,
    target_branch: Option<String>,
    source_docs_repo_id: Option<Uuid>,
    /// The source's former names, newest first: the PR body document is named
    /// after the workspace, so one written before a rename is found under a
    /// former name (`g5.workspace.former-names`).
    #[sqlx(skip)]
    source_former_names: Vec<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspacePullRequestRow {
    id: Uuid,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ProviderRepoSummaryRow {
    pull_request_id: Uuid,
    id: Uuid,
    repo_id: Uuid,
    provider: String,
    provider_repo_full_name: String,
    source_remote_branch: String,
    target_remote_branch: String,
    provider_pr_number: Option<i64>,
    provider_pr_url: Option<String>,
    provider_status: String,
    source_head_sha: Option<String>,
    target_head_sha: Option<String>,
    merge_target_head_sha: Option<String>,
    merge_pending: bool,
    provider_head_sha: Option<String>,
    provider_base_sha: Option<String>,
    last_error: Option<String>,
    last_synced_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    merged_outside_g5: bool,
}

impl ProviderRepoSummaryRow {
    fn into_summary(self) -> WorkspacePullRequestProviderRepoSummary {
        WorkspacePullRequestProviderRepoSummary {
            id: self.id,
            repo_id: self.repo_id,
            provider: self.provider,
            provider_repo_full_name: self.provider_repo_full_name,
            source_remote_branch: self.source_remote_branch,
            target_remote_branch: self.target_remote_branch,
            provider_pr_number: self.provider_pr_number,
            provider_pr_url: self.provider_pr_url,
            provider_status: self.provider_status,
            source_head_sha: self.source_head_sha,
            target_head_sha: self.target_head_sha,
            merge_target_head_sha: self.merge_target_head_sha,
            merge_pending: self.merge_pending,
            provider_head_sha: self.provider_head_sha,
            provider_base_sha: self.provider_base_sha,
            last_error: self.last_error,
            last_synced_at: self.last_synced_at,
            updated_at: self.updated_at,
            merged_outside_g5: self.merged_outside_g5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreflightEvidence {
    source_heads: serde_json::Value,
    target_heads: serde_json::Value,
    preflight: serde_json::Value,
}

const PR_SUMMARY_SQL_BY_ID: &str = r#"
    SELECT pr.id,
           pr.org_id,
           pr.number,
           pr.source_workspace_id,
           sw.name AS source_workspace_name,
           pr.target_workspace_id,
           tw.name AS target_workspace_name,
           pr.status,
           pr.local_merge_status,
           pr.provider_status,
           pr.title,
           pr.description_override,
           pr.source_heads,
           pr.target_heads,
           pr.preflight,
           pr.last_preflight_fingerprint,
           pr.merge_admission AS recorded_merge_admission,
           COALESCE(
               (SELECT last_error FROM workspace_pull_request_provider_repos
                WHERE pull_request_id = pr.id AND last_error IS NOT NULL
                ORDER BY repo_id LIMIT 1),
               (SELECT CASE WHEN event_type = 'provider_failed' THEN message END
                FROM workspace_pull_request_events
                WHERE pull_request_id = pr.id AND pr.status = 'open'
                  AND event_type IN ('provider_failed','provider_updated')
                  AND source_heads IS NOT NULL
                ORDER BY created_at DESC, id DESC LIMIT 1),
               CASE WHEN pr.local_merge_status NOT IN ('ready','merged','closed') THEN
                   COALESCE(pr.preflight ->> 'message', (
                       SELECT repo_result ->> 'message'
                       FROM jsonb_array_elements(pr.preflight -> 'repoResults') AS repo_result
                       WHERE repo_result ? 'message'
                         AND repo_result ->> 'message' IS NOT NULL
                         AND repo_result ->> 'message' <> ''
                       LIMIT 1
                   ))
               END
           ) AS blocking_message,
           pr.created_at,
           pr.updated_at,
           pr.merged_at,
           pr.merged_by,
           CASE WHEN pr.auto_merge_enabled_by IS NULL THEN NULL ELSE jsonb_build_object(
               'enabledBy', pr.auto_merge_enabled_by,
               'enabledAt', pr.auto_merge_enabled_at,
               'mergeMethod', 'REBASE',
               'reason', pr.auto_merge_reason,
               'checkedAt', pr.auto_merge_checked_at
           ) END AS auto_merge_request,
           COALESCE(pr.stop_vm_after_merge, false) AS stop_vm_after_merge,
           (sw.push_mode = 'auto') AS source_auto_push,
           (sw.deleted_at IS NOT NULL) AS source_workspace_deleted,
           (tw.deleted_at IS NOT NULL) AS target_workspace_deleted
    FROM workspace_pull_requests pr
    JOIN workspaces sw ON sw.id = pr.source_workspace_id
    JOIN workspaces tw ON tw.id = pr.target_workspace_id
    WHERE pr.id = $1
"#;

const PR_SUMMARY_SQL_LIST: &str = r#"
    SELECT pr.id,
           pr.org_id,
           pr.number,
           pr.source_workspace_id,
           sw.name AS source_workspace_name,
           pr.target_workspace_id,
           tw.name AS target_workspace_name,
           pr.status,
           pr.local_merge_status,
           pr.provider_status,
           pr.title,
           pr.description_override,
           pr.source_heads,
           pr.target_heads,
           pr.preflight,
           pr.last_preflight_fingerprint,
           pr.merge_admission AS recorded_merge_admission,
           COALESCE(
               (SELECT last_error FROM workspace_pull_request_provider_repos
                WHERE pull_request_id = pr.id AND last_error IS NOT NULL
                ORDER BY repo_id LIMIT 1),
               (SELECT CASE WHEN event_type = 'provider_failed' THEN message END
                FROM workspace_pull_request_events
                WHERE pull_request_id = pr.id AND pr.status = 'open'
                  AND event_type IN ('provider_failed','provider_updated')
                  AND source_heads IS NOT NULL
                ORDER BY created_at DESC, id DESC LIMIT 1),
               CASE WHEN pr.local_merge_status NOT IN ('ready','merged','closed') THEN
                   COALESCE(pr.preflight ->> 'message', (
                       SELECT repo_result ->> 'message'
                       FROM jsonb_array_elements(pr.preflight -> 'repoResults') AS repo_result
                       WHERE repo_result ? 'message'
                         AND repo_result ->> 'message' IS NOT NULL
                         AND repo_result ->> 'message' <> ''
                       LIMIT 1
                   ))
               END
           ) AS blocking_message,
           pr.created_at,
           pr.updated_at,
           pr.merged_at,
           pr.merged_by,
           CASE WHEN pr.auto_merge_enabled_by IS NULL THEN NULL ELSE jsonb_build_object(
               'enabledBy', pr.auto_merge_enabled_by,
               'enabledAt', pr.auto_merge_enabled_at,
               'mergeMethod', 'REBASE',
               'reason', pr.auto_merge_reason,
               'checkedAt', pr.auto_merge_checked_at
           ) END AS auto_merge_request,
           COALESCE(pr.stop_vm_after_merge, false) AS stop_vm_after_merge,
           (sw.push_mode = 'auto') AS source_auto_push,
           (sw.deleted_at IS NOT NULL) AS source_workspace_deleted,
           (tw.deleted_at IS NOT NULL) AS target_workspace_deleted
    FROM workspace_pull_requests pr
    JOIN workspaces sw ON sw.id = pr.source_workspace_id
    JOIN workspaces tw ON tw.id = pr.target_workspace_id
    WHERE (
        ($1 = 'all' AND (pr.source_workspace_id = $2 OR pr.target_workspace_id = $2))
        OR ($1 = 'source' AND pr.source_workspace_id = $2)
        OR ($1 = 'target' AND pr.target_workspace_id = $2)
        OR ($1 = 'ids' AND pr.id = ANY($4::uuid[]))
    )
      AND ($3::text IS NULL OR pr.status = $3)
    ORDER BY pr.updated_at DESC, pr.created_at DESC
    "#;

/// The closed event's message for a PR closed by its workspace's deletion.
pub fn deleted_workspace_close_message(source_deleted: bool) -> &'static str {
    if source_deleted {
        "Closed because its source workspace was deleted"
    } else {
        "Closed because its target workspace was deleted"
    }
}

fn classify_local_merge_status(preflight: &MergePreflightResponse) -> &'static str {
    match preflight.decision {
        MergePreflightDecision::FastForwardOrNoOp
        | MergePreflightDecision::CleanMergeNoGitConflicts => "ready",
        MergePreflightDecision::GitConflicts => "conflict",
        MergePreflightDecision::WorkflowRequired => {
            // A WorkflowRequired repo with recorded actual refs diverged from its
            // planned heads (stale snapshot); one without them could not be read
            // or analyzed at all. Blocked-family evidence wins over stale.
            let any_blocked_family = preflight.repo_results.iter().any(|repo| {
                matches!(repo.outcome, MergePreflightRepoOutcome::WorkflowRequired)
                    && repo.actual_source_head.is_none()
                    && repo.actual_target_head.is_none()
            });
            if any_blocked_family {
                "blocked"
            } else {
                "stale"
            }
        }
    }
}

fn evidence_from_preflight(
    preflight: &MergePreflightResponse,
) -> Result<PreflightEvidence, G5Error> {
    let source_heads = serde_json::Value::Array(
        preflight
            .repo_results
            .iter()
            .map(|repo| {
                serde_json::json!({
                    "repoId": repo.repo_id,
                    "workspaceId": repo.source_workspace_id,
                    "branchName": repo.source_branch_name,
                    "headCommit": repo.source_head,
                })
            })
            .collect::<Vec<_>>(),
    );
    let target_heads = serde_json::Value::Array(
        preflight
            .repo_results
            .iter()
            .map(|repo| {
                serde_json::json!({
                    "repoId": repo.repo_id,
                    "workspaceId": repo.target_workspace_id,
                    "branchName": repo.target_branch_name,
                    "headCommit": repo.target_head,
                    "sourceWorkspaceId": repo.source_workspace_id,
                })
            })
            .collect::<Vec<_>>(),
    );
    let preflight = serde_json::to_value(preflight).map_err(|err| G5Error::Internal {
        message: format!("failed to serialize workspace PR preflight: {err}"),
    })?;
    Ok(PreflightEvidence {
        source_heads,
        target_heads,
        preflight,
    })
}

fn evidence_from_landed_target_cut(
    preflight: &MergePreflightResponse,
    cut_workspace_id: Uuid,
    members: &[BranchCommitMember],
    target_workspace_id: Uuid,
) -> Result<PreflightEvidence, G5Error> {
    if cut_workspace_id != target_workspace_id || members.len() != preflight.repo_results.len() {
        return Err(G5Error::Internal {
            message: "Workspace PR merge authority result is not the complete target cut"
                .to_string(),
        });
    }

    let mut evidence = evidence_from_preflight(preflight)?;
    evidence.target_heads = serde_json::Value::Array(
        preflight
            .repo_results
            .iter()
            .map(|repo| {
                let result = members
                    .iter()
                    .find(|result| {
                        result.repo_id == repo.repo_id
                            && result.workspace_id == repo.target_workspace_id
                    })
                    .ok_or_else(|| G5Error::Internal {
                        message: format!(
                            "Workspace PR merge result omitted target repo {} branch {}",
                            repo.repo_id, repo.target_workspace_id
                        ),
                    })?;
                if result.expected_old_head != repo.target_head {
                    return Err(G5Error::Conflict {
                        reason: format!(
                            "Workspace PR merge receipt head for target repo {} changed before finalization",
                            repo.repo_id
                        ),
                    });
                }
                Ok(serde_json::json!({
                    "repoId": repo.repo_id,
                    "workspaceId": repo.target_workspace_id,
                    "branchName": repo.target_branch_name,
                    "headCommit": result.proposed_new_head,
                    "sourceWorkspaceId": repo.source_workspace_id,
                }))
            })
            .collect::<Result<Vec<_>, G5Error>>()?,
    );
    Ok(evidence)
}

fn preflight_message(preflight: &MergePreflightResponse) -> Option<String> {
    preflight
        .repo_results
        .iter()
        .find_map(|repo| repo.message.clone())
        .or_else(|| Some(preflight.message.clone()).filter(|message| !message.is_empty()))
}

/// Spec D10/D11 commit-derivation bounds shared by the local-title and
/// provider-PR paths.
pub(crate) const PR_DERIVATION_COMMIT_CAP: usize = 50;
pub(crate) const PR_TITLE_MAX_CHARS: usize = 256;
const PR_BULLET_SUBJECT_MAX_CHARS: usize = 120;
const PR_SINGLE_BODY_MAX_CHARS: usize = 4000;
/// Spec D11 ceiling for the complete rendered provider body (derived content
/// including its dynamic fence, separator, and provenance line). The per-part
/// caps above bound every composition well under this value, which itself
/// stays under GitHub's 65536-char body limit; the provider payload builder
/// still guards against it defensively.
pub(crate) const PR_PROVIDER_BODY_MAX_CHARS: usize = 60_000;
/// Longest accepted PR description: the body ceiling less the provenance line.
pub(crate) const PR_DESCRIPTION_MAX_CHARS: usize =
    PR_PROVIDER_BODY_MAX_CHARS - PR_OVERRIDE_PROVENANCE_RESERVE_CHARS;

/// Spec D15: validation headroom reserved for the provider provenance line so
/// an accepted description override always fits under the [D11] ceiling once
/// the provenance line is appended (the over-ceiling fallback in the provider
/// payload builder stays a defensive backstop, not a reachable path for
/// validated input). Comfortably above any real provenance line (fixed text +
/// UUID + two workspace names).
pub(crate) const PR_OVERRIDE_PROVENANCE_RESERVE_CHARS: usize = 1_024;

/// Commit-derived PR text for one repo's `merge_base..source_head` range.
#[derive(Debug, Clone)]
pub(crate) struct DerivedPrText {
    pub title: String,
    /// Neutralized markdown body (subjects in code spans, single-commit body
    /// in a fenced block) so commit text cannot trigger GitHub mentions or
    /// issue-closing keywords. Empty when the derivation has no body content.
    pub body: String,
}

pub(crate) fn derive_pr_text(range: &CommitRangeMessages) -> Option<DerivedPrText> {
    match range.commits.as_slice() {
        [] => None,
        [only] => Some(DerivedPrText {
            title: truncate_chars(&only.subject, PR_TITLE_MAX_CHARS),
            body: if only.body.is_empty() {
                String::new()
            } else {
                fenced_code_block(&truncate_chars(&only.body, PR_SINGLE_BODY_MAX_CHARS))
            },
        }),
        commits => {
            let title = truncate_chars(&commits[0].subject, PR_TITLE_MAX_CHARS);
            let mut body = String::new();
            for commit in commits {
                let subject = truncate_chars(
                    &commit.subject.replace('`', "'"),
                    PR_BULLET_SUBJECT_MAX_CHARS,
                );
                body.push_str("- `");
                body.push_str(&subject);
                body.push_str("`\n");
            }
            if range.omitted_count > 0 {
                body.push_str(&format!("\n…and {} more commits\n", range.omitted_count));
            }
            Some(DerivedPrText { title, body })
        }
    }
}

/// Spec D16: the docs repository's snapshot `source_head` from a preflight,
/// resolved through the source workspace's `docs_repo_id`. `None` when the
/// workspace has no docs repo or the preflight does not carry it — both mean
/// "no PR-body Project Doc", never an error.
pub(crate) fn docs_repo_source_head(
    preflight: &MergePreflightResponse,
    source_docs_repo_id: Option<Uuid>,
) -> Option<(Uuid, String)> {
    let docs_repo_id = source_docs_repo_id?;
    preflight
        .repo_results
        .iter()
        .find(|repo| repo.repo_id == docs_repo_id)
        .map(|repo| (repo.repo_id, repo.source_head.clone()))
}

/// Spec D16: read the workspace PR-body Project Doc
/// (`g5_paths::workspace_pr_body_doc_path`) at the docs repository's preflight
/// snapshot head on a blocking task. Every failure — no docs repo, docs repo
/// absent from the preflight, unreadable repo, missing/non-UTF-8/blank/
/// oversize file, or a join failure — yields `None` and never fails the
/// caller.
pub(crate) async fn read_workspace_pr_body_doc(
    repo_storage: ScopedRepoStorage,
    org_id: Uuid,
    preflight: &MergePreflightResponse,
    source_docs_repo_id: Option<Uuid>,
    source_workspace_id: Uuid,
    source_workspace_names: &[String],
) -> Option<String> {
    let (docs_repo_id, docs_source_head) = docs_repo_source_head(preflight, source_docs_repo_id)?;
    let docs_key = repo_storage.key(org_id, docs_repo_id).ok()?;
    // The document is named after the workspace; try the current name first,
    // then each former name (`g5.workspace.former-names`).
    let doc_paths: Vec<String> = source_workspace_names
        .iter()
        .map(|name| {
            g5_contracts::g5_paths::workspace_pr_body_doc_path(
                &source_workspace_id.to_string(),
                name,
            )
        })
        .collect();
    match tokio::task::spawn_blocking(move || {
        doc_paths.iter().find_map(|doc_path| {
            read_workspace_pr_body_doc_sync(&repo_storage, &docs_key, &docs_source_head, doc_path)
        })
    })
    .await
    {
        Ok(doc) => doc,
        Err(err) => {
            tracing::warn!(error = %err, "PR-body Project Doc read task failed; ignoring the doc");
            None
        }
    }
}

/// Spec D16 synchronous read of the PR-body Project Doc at `doc_path` in the
/// docs repository's committed `source_head`. Returns the trimmed content only
/// when it is present, UTF-8, non-blank, and within the provider body ceiling
/// minus the provenance reserve; every other outcome yields `None`. Synchronous
/// git I/O — call from a blocking task, not the async request path.
fn read_workspace_pr_body_doc_sync(
    repo_storage: &TenantRepoStorage,
    docs_key: &RepoKey,
    source_head: &str,
    doc_path: &str,
) -> Option<String> {
    let bytes = crate::services::workspace_reader::read_file_at_commit_sync(
        repo_storage,
        docs_key,
        source_head,
        doc_path,
    )
    .ok()?;
    normalize_pr_body_doc(bytes)
}

/// Spec D16 inner read: the PR-body Project Doc for (`workspace_id`,
/// `workspace_name`) at `source_head` in an already-opened repository. Split
/// from [`read_workspace_pr_body_doc_sync`] so the route derivation and the
/// usability rules (UTF-8, non-blank, ceiling) are unit-testable against a
/// temp repository without a `RepoStorage`.
#[cfg(test)]
pub(crate) fn read_pr_body_doc_at(
    repo: &GitRepo,
    source_head: &str,
    workspace_id: &str,
    workspace_name: &str,
) -> Option<String> {
    let doc_path = g5_contracts::g5_paths::workspace_pr_body_doc_path(workspace_id, workspace_name);
    let bytes = crate::services::workspace_reader::read_test_file_at_commit_sync(
        repo,
        source_head,
        &doc_path,
    )
    .ok()?;
    normalize_pr_body_doc(bytes)
}

fn normalize_pr_body_doc(bytes: Vec<u8>) -> Option<String> {
    let text = String::from_utf8(bytes).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let limit = PR_PROVIDER_BODY_MAX_CHARS - PR_OVERRIDE_PROVENANCE_RESERVE_CHARS;
    if trimmed.chars().count() > limit {
        tracing::debug!(
            "pr_body doc read: the workspace PR-body Project Doc exceeds the body ceiling; ignoring"
        );
        return None;
    }
    Some(trimmed.to_string())
}

/// Per-repo derivation. `Ok(None)` means the range has no usable commits (a
/// skippable outcome for D10 repo selection); `Err` is a hard failure (path
/// resolution, repository open, merge base, or the walk itself) that callers
/// must map to the legacy fallback, never to another repo's commits.
/// Synchronous git I/O — call from a blocking task, not the async request
/// path.
pub(crate) fn derive_repo_pr_text(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    repo_id: Uuid,
    target_head: &str,
    source_head: &str,
) -> Result<Option<DerivedPrText>, G5Error> {
    let path = repo_storage.repo_path(org_id, repo_id)?;
    let repo = GitRepo::open(&path)?;
    let range = repo.commit_messages_ahead(target_head, source_head, PR_DERIVATION_COMMIT_CAP)?;
    Ok(derive_pr_text(&range))
}

/// Spec D10 repo selection: candidates ordered by repo id (explicitly, never
/// coupled to preflight SQL ordering), non-docs repos before the docs repo; a
/// NULL `docs_repo_id` classifies no repo as docs. Repos with zero usable
/// commits are skipped; any hard derivation failure abandons derivation
/// entirely (legacy title) instead of continuing to a later repo. Git work
/// runs on a blocking task.
/// Spec D10 repo selection: candidates ordered by repo id (explicitly, never
/// coupled to preflight SQL ordering), non-docs repos before the docs repo; a
/// NULL `docs_repo_id` classifies no repo as docs. Repos with zero usable
/// commits are skipped; any hard derivation failure abandons derivation
/// entirely (legacy title) instead of continuing to a later repo. Git work
/// runs on a blocking task.
async fn derive_local_pr_text(
    repo_storage: ScopedRepoStorage,
    org_id: Uuid,
    preflight: &MergePreflightResponse,
    source_docs_repo_id: Option<Uuid>,
) -> Option<(Uuid, String, DerivedPrText)> {
    let mut repos: Vec<(Uuid, String, String)> = preflight
        .repo_results
        .iter()
        .map(|repo| {
            (
                repo.repo_id,
                repo.target_head.clone(),
                repo.source_head.clone(),
            )
        })
        .collect();
    repos.sort_by_key(|(repo_id, _, _)| *repo_id);
    let (code_repos, docs_repos): (Vec<_>, Vec<_>) = repos
        .into_iter()
        .partition(|(repo_id, _, _)| Some(*repo_id) != source_docs_repo_id);
    let join = tokio::task::spawn_blocking(move || {
        for (repo_id, target_head, source_head) in code_repos.into_iter().chain(docs_repos) {
            match derive_repo_pr_text(&repo_storage, org_id, repo_id, &target_head, &source_head) {
                Ok(Some(text)) => return Some((repo_id, source_head, text)),
                Ok(None) => continue,
                Err(err) => {
                    tracing::debug!(
                        %repo_id,
                        error = %err,
                        "PR title derivation failed; using legacy workspace-name title"
                    );
                    return None;
                }
            }
        }
        None
    })
    .await;
    match join {
        Ok(text) => text,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "PR title derivation task failed; using legacy workspace-name title"
            );
            None
        }
    }
}

async fn derive_local_pr_title(
    repo_storage: ScopedRepoStorage,
    org_id: Uuid,
    preflight: &MergePreflightResponse,
    source_docs_repo_id: Option<Uuid>,
) -> Option<String> {
    derive_local_pr_text(repo_storage, org_id, preflight, source_docs_repo_id)
        .await
        .map(|(_, _, text)| text.title)
}

/// Char-boundary-safe truncation with a trailing ellipsis.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", truncated.trim_end())
}

/// Wraps text in a fence longer than any backtick run inside it, so the
/// content can never terminate the fence early.
fn fenced_code_block(text: &str) -> String {
    let mut max_run = 0usize;
    let mut run = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            run += 1;
            max_run = max_run.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat((max_run + 1).max(3));
    format!("{fence}\n{text}\n{fence}\n")
}

/// Spec D15 three-state description change: an absent field never touches the
/// stored value, a blank one explicitly clears it, text sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DescriptionOverrideChange {
    Unchanged,
    Clear,
    Set(String),
}

impl DescriptionOverrideChange {
    fn insert_value(&self) -> Option<&str> {
        match self {
            DescriptionOverrideChange::Set(text) => Some(text),
            _ => None,
        }
    }
}

/// Spec D15: trims a caller-supplied PR description; text over the provider
/// body ceiling is rejected outright rather than truncated or silently
/// dropped later.
fn normalize_description_override(
    description: Option<String>,
) -> Result<DescriptionOverrideChange, G5Error> {
    let Some(raw) = description else {
        return Ok(DescriptionOverrideChange::Unchanged);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DescriptionOverrideChange::Clear);
    }
    let char_count = trimmed.chars().count();
    let limit = PR_PROVIDER_BODY_MAX_CHARS - PR_OVERRIDE_PROVENANCE_RESERVE_CHARS;
    if char_count > limit {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": format!(
                    "PR description is too long ({char_count} characters; limit {limit})"
                ),
            }),
        });
    }
    Ok(DescriptionOverrideChange::Set(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::workspace_merge_service::{
        MergePreflightRepoOutcome, MergePreflightRepoResult,
    };

    fn repo_result(
        outcome: MergePreflightRepoOutcome,
        actual_source_head: Option<&str>,
        actual_target_head: Option<&str>,
    ) -> MergePreflightRepoResult {
        MergePreflightRepoResult {
            repo_id: Uuid::new_v4(),
            source_workspace_id: Uuid::new_v4(),
            target_workspace_id: Uuid::new_v4(),
            source_branch_name: "feat/source".to_string(),
            target_branch_name: "feat/target".to_string(),
            source_head: "1111111111111111111111111111111111111111".to_string(),
            target_head: "2222222222222222222222222222222222222222".to_string(),
            actual_source_head: actual_source_head.map(str::to_string),
            actual_target_head: actual_target_head.map(str::to_string),
            outcome,
            conflict_paths: Vec::new(),
            message: None,
        }
    }

    fn preflight(
        decision: MergePreflightDecision,
        repo_results: Vec<MergePreflightRepoResult>,
    ) -> MergePreflightResponse {
        MergePreflightResponse {
            source_kind: "workspace".to_string(),
            source_workspace_id: Some(Uuid::new_v4()),
            target_workspace_id: Some(Uuid::new_v4()),
            decision,
            can_direct_merge: false,
            preflight_fingerprint: "fp".to_string(),
            repo_results,
            message: String::new(),
        }
    }

    #[test]
    fn landed_target_receipt_matches_old_head_and_records_new_merge_head() {
        let target_workspace_id = Uuid::new_v4();
        let repo = repo_result(MergePreflightRepoOutcome::FastForward, None, None);
        let mut response = preflight(
            MergePreflightDecision::FastForwardOrNoOp,
            vec![repo.clone()],
        );
        response.target_workspace_id = Some(target_workspace_id);
        let merged_head = "3333333333333333333333333333333333333333";
        let mut members = vec![BranchCommitMember {
            workspace_id: repo.target_workspace_id,
            repo_id: repo.repo_id,
            branch_name: repo.target_branch_name.clone(),
            transition: g5_contracts::types::WorkspaceHeadTransitionKind::Advance,
            expected_old_head: repo.target_head.clone(),
            proposed_new_head: merged_head.to_string(),
        }];

        let evidence = evidence_from_landed_target_cut(
            &response,
            target_workspace_id,
            &members,
            target_workspace_id,
        )
        .expect("a merge receipt may advance beyond its captured old target head");
        assert_eq!(evidence.target_heads[0]["headCommit"], merged_head);

        members[0].expected_old_head = "4444444444444444444444444444444444444444".to_string();
        assert!(matches!(
            evidence_from_landed_target_cut(
                &response,
                target_workspace_id,
                &members,
                target_workspace_id
            ),
            Err(G5Error::Conflict { .. })
        ));
    }

    #[test]
    fn workflow_required_with_recorded_actual_refs_classifies_stale() {
        let response = preflight(
            MergePreflightDecision::WorkflowRequired,
            vec![repo_result(
                MergePreflightRepoOutcome::WorkflowRequired,
                Some("3333333333333333333333333333333333333333"),
                Some("2222222222222222222222222222222222222222"),
            )],
        );
        assert_eq!(classify_local_merge_status(&response), "stale");
    }

    #[test]
    fn workflow_required_without_actual_refs_classifies_blocked() {
        let response = preflight(
            MergePreflightDecision::WorkflowRequired,
            vec![
                repo_result(
                    MergePreflightRepoOutcome::WorkflowRequired,
                    Some("3333333333333333333333333333333333333333"),
                    Some("2222222222222222222222222222222222222222"),
                ),
                repo_result(MergePreflightRepoOutcome::WorkflowRequired, None, None),
            ],
        );
        assert_eq!(classify_local_merge_status(&response), "blocked");
    }

    #[test]
    fn clean_and_conflict_decisions_keep_their_statuses() {
        let ready = preflight(MergePreflightDecision::FastForwardOrNoOp, Vec::new());
        assert_eq!(classify_local_merge_status(&ready), "ready");
        let conflict = preflight(MergePreflightDecision::GitConflicts, Vec::new());
        assert_eq!(classify_local_merge_status(&conflict), "conflict");
    }

    fn range_commit(subject: &str, body: &str) -> g5_scm::RangeCommitMessage {
        g5_scm::RangeCommitMessage {
            sha: "0123456789012345678901234567890123456789".to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn derive_pr_text_empty_range_yields_none() {
        let range = CommitRangeMessages {
            commits: vec![],
            omitted_count: 0,
        };
        assert!(derive_pr_text(&range).is_none());
    }

    #[test]
    fn derive_pr_text_single_commit_uses_subject_and_fenced_body() {
        let range = CommitRangeMessages {
            commits: vec![range_commit(
                "Fix login redirect",
                "Detail line.\n\nFixes #123",
            )],
            omitted_count: 0,
        };
        let text = derive_pr_text(&range).unwrap();
        assert_eq!(text.title, "Fix login redirect");
        assert_eq!(text.body, "```\nDetail line.\n\nFixes #123\n```\n");
    }

    #[test]
    fn derive_pr_text_single_commit_without_body_has_empty_body() {
        let range = CommitRangeMessages {
            commits: vec![range_commit("Standalone change", "")],
            omitted_count: 0,
        };
        let text = derive_pr_text(&range).unwrap();
        assert_eq!(text.title, "Standalone change");
        assert_eq!(text.body, "");
    }

    #[test]
    fn derive_pr_text_multi_commit_titles_oldest_and_bullets_all() {
        let range = CommitRangeMessages {
            commits: vec![
                range_commit("Primary feature work", "long body"),
                range_commit("Address review round 1", ""),
                range_commit("Fix `broken` markup @user", ""),
            ],
            omitted_count: 2,
        };
        let text = derive_pr_text(&range).unwrap();
        assert_eq!(text.title, "Primary feature work");
        // Subjects are neutralized inside code spans (backticks stripped so
        // the span cannot terminate early) and the omitted count is exact.
        assert_eq!(
            text.body,
            "- `Primary feature work`\n- `Address review round 1`\n- `Fix 'broken' markup @user`\n\n…and 2 more commits\n"
        );
    }

    #[test]
    fn derive_pr_text_truncates_long_titles_on_char_boundaries() {
        let long_subject = "é".repeat(300);
        let range = CommitRangeMessages {
            commits: vec![range_commit(&long_subject, "")],
            omitted_count: 0,
        };
        let text = derive_pr_text(&range).unwrap();
        assert_eq!(text.title.chars().count(), PR_TITLE_MAX_CHARS);
        assert!(text.title.ends_with('…'));
    }

    #[test]
    fn derive_pr_text_truncates_long_bullet_subjects() {
        let long_subject = "s".repeat(500);
        let range = CommitRangeMessages {
            commits: vec![range_commit(&long_subject, ""), range_commit("Short", "")],
            omitted_count: 0,
        };
        let text = derive_pr_text(&range).unwrap();
        let first_bullet = text.body.lines().next().unwrap();
        // "- `" + 120 capped chars (119 + ellipsis) + "`"
        assert_eq!(first_bullet.chars().count(), 4 + 120);
        assert!(first_bullet.ends_with("…`"));
    }

    #[test]
    fn derived_bodies_stay_under_the_provider_body_ceiling() {
        // Pathological single commit: max-length body that is all backticks,
        // which maximizes the dynamic fence the neutralization adds.
        let backtick_body = "`".repeat(PR_SINGLE_BODY_MAX_CHARS + 100);
        let single = derive_pr_text(&CommitRangeMessages {
            commits: vec![range_commit("Subject", &backtick_body)],
            omitted_count: 0,
        })
        .unwrap();
        // Worst multi-commit case: the commit cap of max-length subjects.
        let long_subject = "s".repeat(PR_BULLET_SUBJECT_MAX_CHARS + 50);
        let multi = derive_pr_text(&CommitRangeMessages {
            commits: (0..PR_DERIVATION_COMMIT_CAP)
                .map(|_| range_commit(&long_subject, ""))
                .collect(),
            omitted_count: 12345,
        })
        .unwrap();
        let provenance_reserve = 200;
        for body in [&single.body, &multi.body] {
            assert!(
                body.chars().count() + provenance_reserve < PR_PROVIDER_BODY_MAX_CHARS,
                "worst-case derived body plus provenance must fit the D11 ceiling: {} chars",
                body.chars().count()
            );
        }
    }

    #[test]
    fn fenced_code_block_outruns_backtick_runs_in_content() {
        let block = fenced_code_block("has ```三 backticks");
        assert!(
            block.starts_with("````\n"),
            "fence must exceed inner run: {block}"
        );
        assert!(block.ends_with("\n````\n"));
    }

    #[test]
    fn workspace_pr_reads_keep_deleted_workspaces_and_writes_require_live_ones() {
        // Reads keep a deleted side's PRs (`g5.git.pr-reads`) and say so.
        for sql in [PR_SUMMARY_SQL_BY_ID, PR_SUMMARY_SQL_LIST] {
            assert!(!sql.contains("deleted_at IS NULL"), "{sql}");
            assert!(sql.contains("AS source_workspace_deleted"));
            assert!(sql.contains("AS target_workspace_deleted"));
        }

        let source = include_str!("coordinated_repo_publication.rs");
        for helper in [
            "async fn load_open_pr_for_pair",
            "async fn load_workspace_names",
            "async fn lock_pr_for_target_workspace",
            "async fn lock_pr_for_path_workspace",
        ] {
            let start = source.find(helper).expect("helper exists");
            let body = &source[start..];
            let end = body.find("\n    async fn ").unwrap_or(body.len());
            let helper_body = &body[..end];
            assert!(
                helper_body.contains("sw.deleted_at IS NULL")
                    && helper_body.contains("tw.deleted_at IS NULL"),
                "{helper} must require live source and target workspaces"
            );
        }
    }
    #[test]
    fn normalize_description_override_maps_absent_blank_and_text_to_three_states() {
        assert_eq!(
            normalize_description_override(None).unwrap(),
            DescriptionOverrideChange::Unchanged
        );
        assert_eq!(
            normalize_description_override(Some("   ".to_string())).unwrap(),
            DescriptionOverrideChange::Clear
        );
        assert_eq!(
            normalize_description_override(Some("  keep me  ".to_string())).unwrap(),
            DescriptionOverrideChange::Set("keep me".to_string())
        );
        assert_eq!(
            DescriptionOverrideChange::Set("keep me".to_string()).insert_value(),
            Some("keep me")
        );
        assert_eq!(DescriptionOverrideChange::Clear.insert_value(), None);
        assert_eq!(DescriptionOverrideChange::Unchanged.insert_value(), None);
    }

    #[test]
    fn normalize_description_override_rejects_over_ceiling_text() {
        let limit = PR_PROVIDER_BODY_MAX_CHARS - PR_OVERRIDE_PROVENANCE_RESERVE_CHARS;
        let at_limit = "x".repeat(limit);
        assert!(matches!(
            normalize_description_override(Some(at_limit)).unwrap(),
            DescriptionOverrideChange::Set(_)
        ));
        let oversized = "x".repeat(limit + 1);
        let error = normalize_description_override(Some(oversized)).unwrap_err();
        assert!(matches!(error, G5Error::ValidationError { .. }));
    }

    const DOC_WORKSPACE_ID: &str = "ddaa679c-467a-4c09-ba29-b67d323e7117";
    const DOC_WORKSPACE_NAME: &str = "Davide PR/body improvement";

    /// Spec D16: temp docs repo with a committed workspace PR-body Project
    /// Doc at `docs/<workspace_id>/<slug>.md`; returns the repo handle, its
    /// tempdir guard, and the commit SHA.
    fn temp_docs_repo_with_pr_body_doc(content: &str) -> (GitRepo, tempfile::TempDir, String) {
        let (repo, dir) = GitRepo::init_temp();
        let doc_path = g5_contracts::g5_paths::workspace_pr_body_doc_path(
            DOC_WORKSPACE_ID,
            DOC_WORKSPACE_NAME,
        );
        assert_eq!(
            doc_path,
            format!("docs/{DOC_WORKSPACE_ID}/davide-pr-body-improvement.md")
        );
        let abs = dir.path().join(&doc_path);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(&abs, content).unwrap();
        repo.add_paths([doc_path.as_str()]).unwrap();
        let sha = repo
            .commit("add pr body doc", &g5_scm::GitAuthor::test_author())
            .unwrap();
        (repo, dir, sha)
    }

    #[test]
    fn read_pr_body_doc_at_returns_trimmed_committed_content() {
        let (repo, _dir, sha) =
            temp_docs_repo_with_pr_body_doc("  ## Summary\n\nDoes the thing.\n\n");
        assert_eq!(
            read_pr_body_doc_at(&repo, &sha, DOC_WORKSPACE_ID, DOC_WORKSPACE_NAME).as_deref(),
            Some("## Summary\n\nDoes the thing.")
        );
        // The route follows the workspace name: another name resolves to a
        // different (absent) doc, and the retired `.g5/pr_body.md` is never
        // consulted.
        assert_eq!(
            read_pr_body_doc_at(&repo, &sha, DOC_WORKSPACE_ID, "renamed workspace"),
            None
        );
    }

    #[test]
    fn read_pr_body_doc_at_rejects_blank_oversize_and_missing() {
        // Blank after trimming → no body source.
        let (repo, _dir, sha) = temp_docs_repo_with_pr_body_doc("   \n\t\n");
        assert_eq!(
            read_pr_body_doc_at(&repo, &sha, DOC_WORKSPACE_ID, DOC_WORKSPACE_NAME),
            None
        );

        // Over the ceiling minus the provenance reserve → ignored.
        let limit = PR_PROVIDER_BODY_MAX_CHARS - PR_OVERRIDE_PROVENANCE_RESERVE_CHARS;
        let (repo, _dir, sha) = temp_docs_repo_with_pr_body_doc(&"x".repeat(limit + 1));
        assert_eq!(
            read_pr_body_doc_at(&repo, &sha, DOC_WORKSPACE_ID, DOC_WORKSPACE_NAME),
            None
        );

        // At the limit → usable.
        let (repo, _dir, sha) = temp_docs_repo_with_pr_body_doc(&"x".repeat(limit));
        assert!(read_pr_body_doc_at(&repo, &sha, DOC_WORKSPACE_ID, DOC_WORKSPACE_NAME).is_some());

        // Missing doc at the commit (only the retired `.g5/pr_body.md` exists)
        // → no body source, no error.
        let (repo, dir) = GitRepo::init_temp();
        std::fs::create_dir_all(dir.path().join(".g5")).unwrap();
        std::fs::write(dir.path().join(".g5/pr_body.md"), "legacy body").unwrap();
        repo.add_paths([".g5/pr_body.md"]).unwrap();
        let sha = repo
            .commit("legacy pr body only", &g5_scm::GitAuthor::test_author())
            .unwrap();
        assert_eq!(
            read_pr_body_doc_at(&repo, &sha, DOC_WORKSPACE_ID, DOC_WORKSPACE_NAME),
            None
        );

        // Bogus commit SHA → no body source, no error.
        assert_eq!(
            read_pr_body_doc_at(&repo, "not-a-sha", DOC_WORKSPACE_ID, DOC_WORKSPACE_NAME),
            None
        );
    }

    #[test]
    fn docs_repo_source_head_resolves_only_the_docs_repo() {
        let docs_repo = Uuid::new_v4();
        let code_repo = Uuid::new_v4();
        let mut code = repo_result(MergePreflightRepoOutcome::CleanMerge, None, None);
        code.repo_id = code_repo;
        code.source_head = "c0de".to_string();
        let mut docs = repo_result(MergePreflightRepoOutcome::CleanMerge, None, None);
        docs.repo_id = docs_repo;
        docs.source_head = "d0c5".to_string();
        let preflight = preflight(
            MergePreflightDecision::CleanMergeNoGitConflicts,
            vec![code, docs],
        );
        assert_eq!(
            docs_repo_source_head(&preflight, Some(docs_repo)),
            Some((docs_repo, "d0c5".to_string()))
        );
        // No docs repo, or a docs repo the preflight does not carry → no doc.
        assert_eq!(docs_repo_source_head(&preflight, None), None);
        assert_eq!(
            docs_repo_source_head(&preflight, Some(Uuid::new_v4())),
            None
        );
    }

    #[test]
    fn preview_text_source_serializes_camel_case() {
        // Spec D16: additive `source` attribution on the D14 preview DTO.
        let preview = WorkspacePullRequestTextPreview {
            title: "t".to_string(),
            body: "b".to_string(),
            derived: true,
            source: WorkspacePullRequestTextSource::ProjectDoc,
        };
        let json = serde_json::to_value(&preview).unwrap();
        assert_eq!(json["source"], "projectDoc");
        assert_eq!(
            serde_json::to_value(WorkspacePullRequestTextSource::Derived).unwrap(),
            "derived"
        );
        assert_eq!(
            serde_json::to_value(WorkspacePullRequestTextSource::Legacy).unwrap(),
            "legacy"
        );
    }

    #[test]
    fn store_description_override_update_is_status_guarded() {
        // Spec D15: the refresh write must never land on a closed/merged row;
        // close and merge hold the row under FOR UPDATE, so the status guard
        // makes a raced write a no-op instead of mutating a settled PR.
        let source = include_str!("coordinated_repo_publication.rs");
        let start = source
            .find("async fn store_description_override")
            .expect("helper exists");
        let body = &source[start..start + 1200];
        assert!(
            body.contains("AND status = 'open'"),
            "store_description_override UPDATE must be status-guarded"
        );
    }
}
