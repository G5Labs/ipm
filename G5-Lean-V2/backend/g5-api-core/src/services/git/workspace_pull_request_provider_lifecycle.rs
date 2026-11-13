#[path = "workspace_pull_request_provider_merge.rs"]
mod merge;
#[path = "workspace_pull_request_read.rs"]
mod read;
pub use read::{
    external_checks_verdict, read_checks, AttachedPullRequestDTO, ExternalChecksVerdict,
    PullRequestAttachmentsDTO, PullRequestListQuery, PullRequestReadQuery, PullRequestReader,
    ReadablePullRequestFilter, ReadablePullRequestListQuery, LOCAL_ATTACHMENTS_REFUSAL,
    LOCAL_CONVERSATION_REFUSAL,
};
#[path = "workspace_pull_request_conversation.rs"]
mod conversation;
#[path = "workspace_pull_request_description.rs"]
mod description;
pub use conversation::{
    publish_pull_request_comment, set_review_thread_resolution, valid_thread_id, validation,
    ConversationTarget, PullRequestCommentRequest, ReviewThreadResolutionRequest,
};
pub use description::{
    DescriptionComparison, DescriptionSource, ProviderDescriptionView, PullRequestDescriptionText,
    PullRequestDescriptionView, MAX_DESCRIPTION_BATCH,
};

use g5_authz::ScopedDb;
use g5_scm::providers::{
    CreatePullRequest, GithubMergeResponse as GitHubMergeResponse, GithubPull as GitHubPull,
    ProviderClients, ProviderEndpoints, ProviderError, RepositoryCapability, RepositoryClient,
    RepositoryGrant, RepositoryIdentity,
};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::auth::RepositoryOperation;
use crate::errors::G5Error;
use crate::git_runtime::{remote_branch_tip, NETWORK_DEADLINE};
use crate::services::coordinated_repo_publication::{
    derive_repo_pr_text, read_workspace_pr_body_doc, DerivedPrText, ProviderMergeTargetHead,
    PR_PROVIDER_BODY_MAX_CHARS,
};
use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use crate::services::workspace_merge_service::{
    MergeAdmissionEvidence, MergeAdmissionGate, MergePreflightResponse, WorkflowRequiredGate,
};
use g5_contracts::types::MergeSourceHead;
use g5_scm::{GitAuth, GitRepo};

/// Outer budget for the whole provider source pre-push (repo open, `origin`
/// realignment, head checks, and the push itself). Kept modestly above
/// g5-common's internal `FORCE_WITH_LEASE_PUSH_TIMEOUT` (120s) so the lease
/// subprocess can finish or be killed on its own deadline and surface its
/// specific error, instead of racing this outer timeout to ambiguous
/// attribution.
const PROVIDER_SOURCE_PUSH_TIMEOUT: Duration = Duration::from_secs(180);
const STALE_PROVIDER_MERGE_COMPLETION: &str =
    "Provider merge completion belongs to a stale attempt; reconcile the provider outcome";

#[derive(Clone)]
pub struct WorkspacePullRequestProviderLifecycleService {
    system_pool: PgPool,
    repo_storage: TenantRepoStorage,
    github_api_base_url: String,
    external_url: String,
    providers: Result<ProviderClients, ProviderError>,
    github_web_base_url: String,
    credentials: Arc<crate::auth::RepositoryCredentials>,
    merge_admission: Arc<dyn MergeAdmissionGate>,
    /// Base-branch required checks, briefly cached per repository and branch.
    required_checks: Arc<g5_scm::providers::RequiredChecksCache>,
    /// The adapter binding and GitHub publication rule (`g5.git.pr-adapter`).
    adapters: Arc<super::pull_request_adapters::PullRequestAdapters>,
}

/// What an explicit create/update asks of provider PRs beyond synchronization.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderPullRequestWrite<'a> {
    /// An explicit title, applied to existing provider PRs too.
    pub title: Option<&'a str>,
    /// An explicit description was submitted: existing provider PRs get the
    /// resulting body.
    pub body: bool,
    /// Open a replacement for a provider PR merged or closed outside G5.
    pub replace: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderRepoStatus {
    Open,
    ActionRequired,
    Failed,
    Merged,
}

impl ProviderRepoStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ActionRequired => "action_required",
            Self::Failed => "failed",
            Self::Merged => "merged",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ProviderEventType {
    Created,
    Updated,
    ActionRequired,
    Failed,
    Merged,
}

impl ProviderEventType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "provider_created",
            Self::Updated => "provider_updated",
            Self::ActionRequired => "provider_action_required",
            Self::Failed => "provider_failed",
            Self::Merged => "provider_merged",
        }
    }
}

struct ProviderStatusRecord<'a> {
    status: ProviderRepoStatus,
    pull: Option<&'a GitHubPull>,
    last_error: Option<String>,
    event_type: ProviderEventType,
    actor_id: Uuid,
}

struct ExistingProviderStatusRecord<'a> {
    status: ProviderRepoStatus,
    pull: Option<&'a GitHubPull>,
    merge_sha: Option<String>,
    last_error: Option<String>,
    event_type: ProviderEventType,
    actor_id: Uuid,
}

struct ProviderMergePlan {
    row: ProviderRepoRow,
    token: String,
    read_token: String,
    pull: GitHubPull,
    /// The verified target head the provider merges onto.
    target_head: String,
    /// GitHub reports the repository blocked and the merge's GitHub override
    /// lets its actor merge it past the rules, with `token` their own
    /// (g5.git.provider-override).
    bypass: bool,
}

/// GitHub's `mergeable_state` while branch protection or rulesets are unmet.
const MERGE_STATE_BLOCKED: &str = "blocked";
/// GitHub's `mergeable_state` while it is still computing mergeability.
const MERGE_STATE_UNKNOWN: &str = "unknown";
/// Bounded re-reads of a merge state GitHub is still computing.
const MERGE_STATE_REREADS: usize = 3;
const MERGE_STATE_REREAD_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// Why a repository GitHub reports blocked does not merge
/// (g5.git.provider-rules).
fn provider_blocked_reason(number: i64, repository: &str) -> String {
    format!(
        "GitHub reports PR #{number} in {repository} as blocked by its branch protection or \
         rulesets, so G5 will not merge it. Meet GitHub's requirements, or someone whose GitHub \
         account may bypass them can use Merge with override."
    )
}

struct ProviderMergeCapture {
    context: PullRequestProviderContext,
    rows: Vec<ProviderRepoRow>,
    target: g5_contracts::types::CommittedBindingSnapshot,
    admission: Option<MergeAdmissionEvidence>,
}

enum ProviderMergePreflight {
    Ready(Box<ProviderMergePlan>),
    AlreadyMerged,
    NotReady,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PullRequestProviderContext {
    id: Uuid,
    org_id: Uuid,
    status: String,
    source_workspace_id: Uuid,
    source_workspace_name: String,
    /// Source workspace `docs_repo_id`; locates the PR-body Project Doc
    /// (spec D16). NULL means no doc.
    source_docs_repo_id: Option<Uuid>,
    #[sqlx(default)]
    source_org_slug: String,
    target_workspace_name: String,
    external_url: String,
    preflight: serde_json::Value,
    description_override: Option<String>,
    title: String,
    /// Spec D16: the workspace PR-body Project Doc read ONCE per lifecycle
    /// run from the docs repo's preflight snapshot head; every candidate repo
    /// (and the 422 retry) applies this same body. Populated by
    /// `load_context`, never a column.
    #[sqlx(default)]
    pr_body_doc: Option<String>,
    /// The store narrowed to the loading transaction's scope
    /// (`g5.repo.storage-tenancy`). Set by `load_context_record`.
    #[sqlx(skip)]
    repos: Option<ScopedRepoStorage>,
}

#[derive(Debug, Clone)]
struct ProviderCandidate {
    repo_id: Uuid,
    provider_full_name: String,
    git_url: String,
    source_local_branch_name: String,
    source_remote_branch: String,
    target_remote_branch: String,
    source_head: String,
    target_head: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ProviderRepoRow {
    id: Uuid,
    org_id: Uuid,
    pull_request_id: Uuid,
    repo_id: Uuid,
    provider: String,
    provider_repo_full_name: String,
    git_url: String,
    source_remote_branch: String,
    target_remote_branch: String,
    provider_pr_number: Option<i64>,
    provider_status: String,
    source_head_sha: Option<String>,
    merge_attempt_id: Option<Uuid>,
    merge_execution_id: Option<Uuid>,
    merge_target_head_sha: Option<String>,
    merge_pending: bool,
    merge_accepted: bool,
    /// The provider PR automatic refresh observed merged or closed
    /// (g5.git.pr-refresh.completed-provider).
    provider_completed_pr_number: Option<i64>,
}

impl ProviderRepoRow {
    /// Automatic refresh already observed this row's current provider PR
    /// completed, so it refuses without another provider read.
    fn automatic_refresh_paused(&self) -> bool {
        self.provider_pr_number.is_some()
            && self.provider_completed_pr_number == self.provider_pr_number
    }
}

/// Automatic refresh's refusal of a provider PR GitHub reports merged or
/// closed. Typed so the scan does not log each paused pass as a new failure.
pub fn provider_pull_request_completed() -> G5Error {
    G5Error::ConflictDetails {
        reason: "Provider PR has completed; automatic refresh is paused for merge recovery".into(),
        details: serde_json::json!({"kind": PROVIDER_COMPLETED_KIND}),
    }
}

const PROVIDER_COMPLETED_KIND: &str = "pull_request_provider_completed";

/// Whether `error` is [`provider_pull_request_completed`].
pub fn is_provider_pull_request_completed(error: &G5Error) -> bool {
    matches!(error, G5Error::ConflictDetails { details, .. } if details["kind"] == PROVIDER_COMPLETED_KIND)
}

/// What automatic refresh may do after checking its provider PRs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "Paused must be committed and refused with provider_pull_request_completed"]
pub enum AutomaticRefreshAdmission {
    Allowed,
    /// A provider PR was just observed completed and that observation was
    /// written under the PR lock: commit it, then refuse with
    /// [`provider_pull_request_completed`].
    Paused,
}

/// Verified remote member: accepted provider history, or source history already
/// contained in the unchanged target. Publication rechecks these exact heads
/// and target policy under the workspace fences before moving any ref.
pub struct AcceptedProviderMerge {
    pub(crate) workspace_id: Uuid,
    pub(crate) repo_id: Uuid,
    pub(crate) remote_branch: String,
    pub(crate) source_head: String,
    pub(crate) expected_target_head: String,
    // A member already contained in the target preserves its exact head and
    // synchronization state. Only a verified provider import advances those.
    pub(crate) convergence: Option<
        crate::services::git::remote_integrations::convergence::PreparedRemoteTrackingConvergence,
    >,
}

#[derive(Debug)]
struct ProviderHttpError {
    status: Option<u16>,
    message: String,
    /// The provider's own (redacted) message, without G5's diagnostics.
    provider_message: String,
    rate_limited: bool,
    effect: g5_scm::FailureEffect,
}
impl From<ProviderError> for ProviderHttpError {
    fn from(error: ProviderError) -> Self {
        Self {
            status: error.status,
            message: error.to_string(),
            provider_message: error.message,
            rate_limited: error.rate_limited,
            effect: error.effect,
        }
    }
}
impl ProviderHttpError {
    fn is_action_required(&self) -> bool {
        !self.rate_limited && matches!(self.status, Some(401 | 403))
    }
    /// GitHub's merge endpoint answers 405 when it refuses the merge itself
    /// (unrebaseable branch, unmet branch protection, disallowed method).
    fn is_merge_refusal(&self) -> bool {
        !self.rate_limited && self.status == Some(405)
    }
}

/// Pinned source history relative to the verified target, read from the local
/// repository to explain a rebase GitHub refuses (issue #582).
#[derive(Debug)]
struct SourceHistory {
    merges: Vec<g5_scm::CommitSummary>,
    contains_target: bool,
}
impl SourceHistory {
    fn is_linear_on_target(&self) -> bool {
        self.contains_target && self.merges.is_empty()
    }
}

impl WorkspacePullRequestProviderLifecycleService {
    pub async fn prepare_merged_target_import(
        &self,
        mut db: ScopedDb,
        pull_request_id: Uuid,
        target_workspace_id: Uuid,
        actor_id: Uuid,
    ) -> Result<Vec<AcceptedProviderMerge>, G5Error> {
        use crate::services::git::remote_integrations::convergence::{
            PreparedRemoteTrackingConvergence, RemoteTrackingConvergenceStatus,
        };
        let context = self.load_context(&mut db, pull_request_id).await?;
        let rows = self.load_provider_rows(&mut db, pull_request_id).await?;
        let repos = self.repo_storage.scoped(&db);
        let source = crate::services::git::workspace_reader::resolve_committed_snapshot(
            &mut db,
            &self.repo_storage,
            g5_contracts::types::AuthorityTarget::Workspace {
                workspace_id: context.source_workspace_id,
            },
        )
        .await?;
        let targets = crate::services::git::remote_integrations::capture_repositories(
            db.conn(),
            repos.clone(),
            target_workspace_id,
            false,
        )
        .await?;
        if rows.iter().any(|row| {
            row.provider != "github"
                || !targets.iter().any(|target| {
                    target.repo_id == row.repo_id
                        && target.org_id == row.org_id
                        && target.remote_branch.as_ref() == Some(&row.target_remote_branch)
                })
        }) {
            return Err(G5Error::Conflict {
                reason: "Provider PR records no longer match the target remote bindings".into(),
            });
        }
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close provider import capture: {error}"),
        })?;
        let mut imports = Vec::new();
        for target in targets
            .into_iter()
            .filter(|target| target.remote_branch.is_some())
        {
            let row = rows.iter().find(|row| {
                row.repo_id == target.repo_id
                    && row.org_id == target.org_id
                    && row.provider == "github"
                    && Some(&row.target_remote_branch) == target.remote_branch.as_ref()
            });
            let Some(row) = row else {
                // PR creation intentionally skips identical/already-included
                // members. Prove that omission against immutable current heads
                // again; a new source commit must require a provider PR.
                let source_member =
                    source
                        .repo(target.repo_id)
                        .map_err(|reason| G5Error::Conflict {
                            reason: reason.to_string(),
                        })?;
                self.require_source_in_target(
                    &repos,
                    context.org_id,
                    target.repo_id,
                    &source_member.head_commit,
                    &target.head_commit,
                )?;
                imports.push(AcceptedProviderMerge {
                    workspace_id: target_workspace_id,
                    repo_id: target.repo_id,
                    remote_branch: target.remote_branch.unwrap(),
                    source_head: source_member.head_commit.clone(),
                    expected_target_head: target.head_commit,
                    convergence: None,
                });
                continue;
            };
            let expected_source =
                row.source_head_sha
                    .as_deref()
                    .ok_or_else(|| G5Error::Conflict {
                        reason: "Provider merge has no pinned source head".into(),
                    })?;
            let auth = self
                .credentials
                .resolve(
                    row.org_id,
                    row.repo_id,
                    actor_id,
                    &row.git_url,
                    RepositoryOperation::ReadPullRequests,
                )
                .await?;
            let token = token_from_auth(&auth).ok_or_else(|| G5Error::Conflict {
                reason: "Provider merge verification requires repository credentials".into(),
            })?;
            let number = row.provider_pr_number.ok_or_else(|| G5Error::Conflict {
                reason: "Provider PR number is missing".into(),
            })?;
            let pull = self
                .get_pull_request(&token, &row.provider_repo_full_name, number)
                .await
                .map_err(provider_error_to_g5)?;
            if !pull.merged.unwrap_or(false) || pull.head.sha != expected_source {
                return Err(G5Error::Conflict {
                    reason: format!("GitHub PR #{number} has not merged the pinned source head"),
                });
            }
            let merged_head = pull.merge_commit_sha.ok_or_else(|| G5Error::Conflict {
                reason: format!("GitHub PR #{number} has no accepted merge commit"),
            })?;
            let auth = self
                .credentials
                .resolve(
                    row.org_id,
                    row.repo_id,
                    actor_id,
                    &row.git_url,
                    RepositoryOperation::ReadGit,
                )
                .await?;
            let fetched = crate::services::git::remote_integrations::prepare_pull(
                &repos,
                crate::services::git::remote_integrations::PullInput {
                    key: repos.key(row.org_id, row.repo_id)?,
                    git_url: row.git_url.clone(),
                    remote_branch: row.target_remote_branch.clone(),
                    local_head: target.head_commit.clone(),
                    watermark: target.baseline_commit.clone(),
                    observed_remote_tip: None,
                },
                auth,
            )
            .await?;
            let path = repos.repo_path(row.org_id, row.repo_id)?;
            let repo = GitRepo::open(&path)?;
            if (merged_head != fetched.remote_tip
                && !repo.is_ancestor_of(&merged_head, &fetched.remote_tip)?)
                || (target.head_commit != fetched.remote_tip
                    && !repo.is_ancestor_of(&target.head_commit, &fetched.remote_tip)?)
            {
                return Err(G5Error::Conflict {
                    reason: format!("GitHub PR #{number} merged, but its target history changed; reconcile the target and retry importing"),
                });
            }
            imports.push(AcceptedProviderMerge {
                workspace_id: target_workspace_id,
                repo_id: row.repo_id,
                remote_branch: row.target_remote_branch.clone(),
                source_head: expected_source.to_owned(),
                expected_target_head: target.head_commit.clone(),
                convergence: Some(PreparedRemoteTrackingConvergence {
                    workspace_id: target_workspace_id,
                    repo_id: row.repo_id,
                    remote_branch: row.target_remote_branch.clone(),
                    expected_tracking_mode: target.tracking_mode,
                    expected_remote_branch: target.remote_branch,
                    expected_old_head: target.head_commit.clone(),
                    proposed_new_head: fetched.remote_tip.clone(),
                    expected_baseline_commit: target.baseline_commit,
                    expected_git_url: row.git_url.clone(),
                    expected_sync_paused_reason: target.sync_paused_reason,
                    acting_user_id: actor_id,
                    trigger_kind: "manual".into(),
                    status: if target.head_commit == fetched.remote_tip {
                        RemoteTrackingConvergenceStatus::Noop
                    } else {
                        RemoteTrackingConvergenceStatus::Imported
                    },
                }),
            });
        }
        if imports.is_empty() {
            return Err(G5Error::Conflict {
                reason: "Provider merge target lost its remote bindings".into(),
            });
        }
        Ok(imports)
    }

    fn require_source_in_target(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        repo_id: Uuid,
        source_head: &str,
        target_head: &str,
    ) -> Result<(), G5Error> {
        let path = repos.repo_path(org_id, repo_id)?;
        let repo = GitRepo::open(&path)?;
        if source_head != target_head && !repo.is_ancestor_of(source_head, target_head)? {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Remote target repository {repo_id} has source changes without a matching provider PR; update the PR before merging"
                ),
            });
        }
        Ok(())
    }

    pub fn new(
        system_pool: PgPool,
        repo_storage: TenantRepoStorage,
        github_api_base_url: String,
        github_web_base_url: String,
        external_url: String,
        credentials: Arc<crate::auth::RepositoryCredentials>,
        merge_admission: Arc<dyn MergeAdmissionGate>,
    ) -> Self {
        let adapters = Arc::new(super::pull_request_adapters::PullRequestAdapters::new(
            credentials.origin_policy().clone(),
            &github_api_base_url,
            &github_web_base_url,
        ));
        Self {
            system_pool,
            repo_storage,
            github_api_base_url,
            external_url,
            providers: ProviderClients::new(),
            github_web_base_url,
            credentials,
            merge_admission,
            required_checks: Arc::default(),
            adapters,
        }
    }

    pub async fn create_or_update_provider_pull_requests(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
        title: Option<&str>,
    ) -> Result<(), G5Error> {
        let write = ProviderPullRequestWrite {
            title,
            ..ProviderPullRequestWrite::default()
        };
        self.publish_provider_pull_requests(db, pull_request_id, actor_id, write)
            .await
    }

    /// Create or update this G5 PR's provider PRs for an explicit create/update.
    pub async fn publish_provider_pull_requests(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
        write: ProviderPullRequestWrite<'_>,
    ) -> Result<(), G5Error> {
        self.synchronize_provider_pull_requests(db, pull_request_id, actor_id, false, write)
            .await
    }

    /// Continue an existing PR after publication; never push Git implicitly.
    pub async fn synchronize_after_push(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<(), G5Error> {
        let write = ProviderPullRequestWrite::default();
        self.synchronize_provider_pull_requests(db, pull_request_id, actor_id, true, write)
            .await
    }

    /// `g5d pr edit` (`g5.git.pr-cli.edit`): apply an explicit title and/or
    /// the published body to this G5 PR's open provider PRs. Nothing is
    /// pushed and no provider PR is opened; each repository's outcome is
    /// recorded on its provider row.
    pub async fn edit_provider_pull_requests(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
        title: Option<&str>,
        body: bool,
    ) -> Result<(), G5Error> {
        let context = self.load_context(db, pull_request_id).await?;
        if context.status != "open" {
            return Ok(());
        }
        let recorded = self.load_provider_rows(db, pull_request_id).await?;
        let mut attempted = false;
        for repo in parse_preflight(&context.preflight)?.repo_results {
            let Some((row, number)) = recorded.iter().find_map(|row| {
                (row.repo_id == repo.repo_id
                    && !matches!(row.provider_status.as_str(), "merged" | "closed"))
                .then_some(row.provider_pr_number.map(|number| (row, number)))
                .flatten()
            }) else {
                continue;
            };
            let Some(candidate) = self
                .load_provider_candidate(db, context.org_id, &repo)
                .await?
            else {
                continue;
            };
            attempted = true;
            let auth = self
                .resolve_publication_auth(
                    db,
                    false,
                    context.org_id,
                    candidate.repo_id,
                    actor_id,
                    &candidate.git_url,
                    RepositoryOperation::WritePullRequests,
                )
                .await;
            let not_open = |number: i64| {
                (
                    true,
                    format!("GitHub PR #{number} is no longer open, so it was not edited."),
                )
            };
            let edited = match auth.map(|auth| token_from_auth(&auth)) {
                // Never send a recorded number to another repository.
                _ if row.provider_repo_full_name != candidate.provider_full_name => Err((
                    true,
                    "The recorded GitHub repository changed, so its PR was not edited.".to_owned(),
                )),
                Ok(Some(token)) => match self
                    .get_pull_request(&token, &candidate.provider_full_name, number)
                    .await
                {
                    Err(err) => Err((err.is_action_required(), err.message)),
                    Ok(pull) if pull.merged.unwrap_or(false) || pull.state != "open" => {
                        Err(not_open(number))
                    }
                    Ok(_) => {
                        let body = if body {
                            Some(
                                self.build_provider_pr_payload(&context, &candidate)
                                    .await
                                    .body,
                            )
                        } else {
                            None
                        };
                        match self
                            .update_pull_request(
                                &token,
                                &candidate.provider_full_name,
                                number,
                                title,
                                body.as_deref(),
                            )
                            .await
                        {
                            Err(err) => Err((err.is_action_required(), err.message)),
                            Ok(pull) if pull.merged.unwrap_or(false) || pull.state != "open" => {
                                Err(not_open(number))
                            }
                            Ok(pull) => Ok(pull),
                        }
                    }
                },
                Ok(None) => Err((
                    true,
                    "GitHub credential is required to edit this PR".to_owned(),
                )),
                Err(error) => Err((true, error.to_string())),
            };
            match edited {
                Ok(pull) => {
                    self.record_provider_status(
                        db,
                        &context,
                        &candidate,
                        ProviderStatusRecord {
                            status: ProviderRepoStatus::Open,
                            pull: Some(&pull),
                            last_error: None,
                            event_type: ProviderEventType::Updated,
                            actor_id,
                        },
                    )
                    .await?
                }
                Err((action_required, message)) => {
                    self.update_existing_provider_status(
                        db,
                        row,
                        ExistingProviderStatusRecord {
                            status: if action_required {
                                ProviderRepoStatus::ActionRequired
                            } else {
                                ProviderRepoStatus::Failed
                            },
                            pull: None,
                            merge_sha: None,
                            last_error: Some(format!(
                                "{message} Correct the cause, then rerun g5d pr edit with the same text."
                            )),
                            event_type: ProviderEventType::Failed,
                            actor_id,
                        },
                    )
                    .await?
                }
            }
        }
        if attempted {
            self.recompute_aggregate_provider_status(db, pull_request_id)
                .await?;
        }
        Ok(())
    }

    /// Local preflight can already contain new source heads before publication.
    /// Discover required missing provider outcomes using the same eligibility as sync.
    pub async fn has_missing_provider_pull_request(
        &self,
        db: &mut ScopedDb,
        id: Uuid,
    ) -> Result<bool, G5Error> {
        let Some((org, evidence)) = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
            "SELECT org_id,preflight FROM workspace_pull_requests WHERE id=$1 AND status='open'",
        )
        .bind(id)
        .fetch_optional(db.conn())
        .await?
        else {
            return Ok(false);
        };
        let recorded = self.load_provider_rows(db, id).await?;
        for repo in parse_preflight(&evidence)?.repo_results {
            if repo_needs_provider_pr(&repo)
                && !recorded.iter().any(|row| row.repo_id == repo.repo_id)
                && self
                    .load_provider_candidate(db, org, &repo)
                    .await?
                    .is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Called under the PR lock before automatic evidence is refreshed. An
    /// accepted provider merge keeps its original pinned version for import/retry.
    /// A provider PR first observed completed here is recorded in `db`
    /// ([`AutomaticRefreshAdmission::Paused`]) so later passes refuse without
    /// reading GitHub (g5.git.pr-refresh.completed-provider).
    pub async fn ensure_automatic_refresh_allowed(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<AutomaticRefreshAdmission, G5Error> {
        // Validate refresh's canonical binding before reconciliation performs
        // any provider reads, retaining the existing no-HTTP refusal boundary.
        let mut rows = self.load_provider_rows(db, pull_request_id).await?;
        for row in rows.iter().filter(|row| row.provider_pr_number.is_some()) {
            if self
                .provider_endpoints()?
                .repository_name_from_clone_url(&row.git_url)
                .as_deref()
                != Some(&row.provider_repo_full_name)
            {
                return Err(G5Error::Conflict {
                    reason:
                        "Recorded provider PR repository no longer matches the attached repository"
                            .into(),
                });
            }
        }
        // Merge recovery owns an unresolved merge: its reconciliation is not
        // committed here, so neither is an observation that follows it.
        let reconciled = rows.iter().any(|row| row.merge_pending);
        if reconciled {
            self.reconcile_provider_acceptance(db, pull_request_id, actor_id)
                .await?;
            rows = self.load_provider_rows(db, pull_request_id).await?;
        }
        for row in rows {
            if row.merge_pending {
                return Err(G5Error::Conflict { reason: "Provider merge outcome is unresolved; retry the merge to reconcile it before refreshing".into() });
            }
            if row.merge_accepted
                || row.provider_status == "merged"
                || row.provider_status == "closed"
                || row.automatic_refresh_paused()
            {
                return Err(provider_pull_request_completed());
            }
            let Some(number) = row.provider_pr_number else {
                continue;
            };
            let auth = self
                .credentials
                .resolve_in_scope(
                    db,
                    row.org_id,
                    row.repo_id,
                    actor_id,
                    &row.git_url,
                    RepositoryOperation::ReadPullRequests,
                )
                .await?;
            let token = token_from_auth(&auth).ok_or_else(|| G5Error::Conflict {
                reason: "GitHub credential is required to refresh this PR".into(),
            })?;
            let pull = self
                .get_pull_request(&token, &row.provider_repo_full_name, number)
                .await
                .map_err(provider_error_to_g5)?;
            if pull.merged.unwrap_or(false) || pull.state != "open" {
                if reconciled {
                    return Err(provider_pull_request_completed());
                }
                sqlx::query(
                    "UPDATE workspace_pull_request_provider_repos
                        SET provider_completed_pr_number=$2, updated_at=now()
                      WHERE id=$1 AND provider_pr_number=$2",
                )
                .bind(row.id)
                .bind(number)
                .execute(db.conn())
                .await?;
                return Ok(AutomaticRefreshAdmission::Paused);
            }
        }
        Ok(AutomaticRefreshAdmission::Allowed)
    }

    /// Whether automatic refresh already observed a provider PR of this pull
    /// request completed: it then refuses before preparing evidence and without
    /// a provider read. Discovery only; the locked check above still decides.
    pub async fn automatic_refresh_paused(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<bool, G5Error> {
        let rows = self.load_provider_rows(db, pull_request_id).await?;
        Ok(!rows.iter().any(|row| row.merge_pending)
            && rows.iter().any(ProviderRepoRow::automatic_refresh_paused))
    }

    async fn synchronize_provider_pull_requests(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
        after_push: bool,
        write: ProviderPullRequestWrite<'_>,
    ) -> Result<(), G5Error> {
        let title = write.title;
        // Public source admission is a shared workspace lock. Serialize accepted
        // local titles with their provider effects on the PR row itself, retaining
        // source-before-PR lock order and this lock until the caller commits.
        sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(pull_request_id)
            .fetch_optional(db.conn())
            .await?;
        let context = self.load_context(db, pull_request_id).await?;
        if context.status != "open" {
            return Ok(());
        }
        let mut recorded = self.load_provider_rows(db, pull_request_id).await?;
        if recorded.iter().any(|row| row.merge_pending) {
            self.reconcile_provider_acceptance(db, pull_request_id, actor_id)
                .await?;
            recorded = self.load_provider_rows(db, pull_request_id).await?;
        }
        if recorded
            .iter()
            .any(|row| row.merge_pending || row.merge_accepted)
        {
            return Err(G5Error::Conflict { reason: "Provider merge outcome is unresolved; retry the merge before publication or replacement".into() });
        }
        // Explicit title intent belongs to this request. The PR lock prevents
        // newer acceptance during provider effects; compare after waiting so an
        // older request cannot overwrite a title accepted in the local/provider gap.
        if title.is_some_and(|title| title != context.title) {
            return Err(G5Error::Conflict {
                reason: "A newer PR title was accepted; inspect the current PR and retry with --title if needed".into(),
            });
        }
        let preflight = parse_preflight(&context.preflight)?;
        let mut attempted = false;

        for repo in preflight.repo_results {
            let gate = repo.workflow_required_gate();
            let source_blocked = matches!(gate, WorkflowRequiredGate::SourceBlocked);
            let target_only_drift = matches!(gate, WorkflowRequiredGate::TargetOnlyDrift);
            let existing = recorded
                .iter()
                .find(|row| row.repo_id == repo.repo_id && row.provider_pr_number.is_some());
            if !source_blocked && !repo_needs_provider_pr(&repo) && existing.is_none() {
                // No remaining diff and no recorded PR: never create an empty PR.
                continue;
            }
            let Some(candidate) = self
                .load_provider_candidate(db, context.org_id, &repo)
                .await?
            else {
                continue;
            };
            attempted = true;
            if source_blocked {
                // Preserve the precise preflight reason (issue #869) instead
                // of collapsing every shape into one generic message.
                let reason = match repo.message.as_deref() {
                    Some(message) => format!(
                        "workspace branch snapshot is stale or unreadable ({message}); sync/refresh the workspace before provider PR update"
                    ),
                    None => "workspace branch snapshot is stale or unreadable; sync/refresh the workspace before provider PR update"
                        .to_string(),
                };
                if after_push {
                    return Err(G5Error::Conflict { reason });
                }
                self.record_provider_status(
                    db,
                    &context,
                    &candidate,
                    ProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        last_error: Some(reason),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                continue;
            }
            if target_only_drift {
                tracing::info!(
                    pull_request_id = %context.id,
                    repo_id = %candidate.repo_id,
                    expected_target_head = %repo.target_head,
                    actual_target_head = ?repo.actual_target_head,
                    "provider PR proceeding despite stale local target snapshot; target-side drift is advisory (issue #869)"
                );
            }
            if let Err(err) = self
                .create_or_update_one(
                    db,
                    &context,
                    &candidate,
                    actor_id,
                    write,
                    after_push,
                    existing.filter(|_| after_push || !repo_needs_provider_pr(&repo)),
                    existing,
                )
                .await
            {
                if after_push {
                    // Roll back the evidence cut on uncertain provider outcomes,
                    // especially completion racing the earlier observation.
                    return Err(err);
                }
                tracing::warn!(
                    pull_request_id = %context.id,
                    repo_id = %candidate.repo_id,
                    error = %err,
                    "GitHub PR provider create/update failed after local workspace PR create"
                );
                if let Some(row) = existing.filter(|_| !repo_needs_provider_pr(&repo)) {
                    self.update_existing_provider_status(
                        db,
                        row,
                        ExistingProviderStatusRecord {
                            status: if row.provider_status == "merged" {
                                ProviderRepoStatus::Merged
                            } else {
                                ProviderRepoStatus::Failed
                            },
                            pull: None,
                            merge_sha: None,
                            last_error: Some(err.to_string()),
                            event_type: ProviderEventType::Failed,
                            actor_id,
                        },
                    )
                    .await?;
                    continue;
                }
                self.record_provider_status(
                    db,
                    &context,
                    &candidate,
                    ProviderStatusRecord {
                        status: ProviderRepoStatus::Failed,
                        pull: None,
                        last_error: Some(err.to_string()),
                        event_type: ProviderEventType::Failed,
                        actor_id,
                    },
                )
                .await?;
            }
        }

        if attempted {
            self.recompute_aggregate_provider_status(db, pull_request_id)
                .await?;
        }
        if attempted && !after_push {
            let recovery = if title.is_some() {
                "Correct the cause, then retry g5d pr update with the same --title to apply the title to GitHub."
            } else {
                "Correct the cause, then choose Update PR or run g5d pr update."
            };
            sqlx::query(
                "UPDATE workspace_pull_request_provider_repos
                SET last_error=concat_ws(' ',last_error,$2)
                WHERE pull_request_id=$1 AND provider_status IN ('failed','action_required')
                  AND right(COALESCE(last_error,''),length($2)) <> $2",
            )
            .bind(pull_request_id)
            .bind(recovery)
            .execute(db.conn())
            .await?;
        }
        Ok(())
    }

    /// `target_heads` are the target heads the merge's fresh preflight
    /// classified. GitHub's live target must match them; the target recorded
    /// at publication is not consulted, so a target move needs no
    /// republication (g5.git.pr-recovery-cut).
    async fn execute_provider_merges(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        target_heads: &[ProviderMergeTargetHead],
        actor_id: Uuid,
        execution_id: Uuid,
        dispatched: &mut std::collections::HashSet<Uuid>,
    ) -> Result<(), G5Error> {
        let ProviderMergeCapture {
            context,
            rows,
            target,
            admission,
        } = self.capture_provider_merge(db, pull_request_id).await?;
        Self::lock_merge_targets(db, &rows).await?;
        if rows
            .iter()
            .any(|row| row.merge_pending && row.merge_execution_id != Some(execution_id))
        {
            return Err(G5Error::Conflict {
                reason: "Provider merge attempt was superseded; retry to reconcile it".into(),
            });
        }
        let mut plans = Vec::new();
        let mut blocked_before_provider_mutation = false;
        for row in rows {
            if row.provider != "github" {
                continue;
            }
            if row.provider_pr_number.is_none() {
                self.update_existing_provider_status(
                    db,
                    &row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::Failed,
                        pull: None,
                        merge_sha: None,
                        last_error: Some("Provider PR number is missing".to_string()),
                        event_type: ProviderEventType::Failed,
                        actor_id,
                    },
                )
                .await?;
                blocked_before_provider_mutation = true;
                continue;
            }
            let classified_target = row.merge_target_head_sha.as_deref().or_else(|| {
                target_heads
                    .iter()
                    .find(|head| head.repo_id == row.repo_id)
                    .map(|head| head.head_commit.as_str())
            });
            let bypass_allowed = admission
                .as_ref()
                .and_then(|admission| admission.merge_override.as_ref())
                .is_some_and(|record| record.bypasses_github(actor_id, row.repo_id));
            match self
                .preflight_one_provider_pr_for_merge(
                    db,
                    &row,
                    classified_target,
                    &context.target_workspace_name,
                    actor_id,
                    bypass_allowed,
                )
                .await
            {
                Ok(ProviderMergePreflight::Ready(plan)) => {
                    if !target.repos.iter().any(|member| {
                        member.repo_id == row.repo_id && member.head_commit == plan.target_head
                    }) {
                        return Err(G5Error::Conflict {
                            reason: format!(
                                "{} changed while merging; merge again",
                                context.target_workspace_name
                            ),
                        });
                    }
                    plans.push(*plan);
                }
                Ok(ProviderMergePreflight::AlreadyMerged) => {}
                Ok(ProviderMergePreflight::NotReady) => {
                    blocked_before_provider_mutation = true;
                }
                Err(err) => {
                    tracing::warn!(
                        pull_request_id = %context.id,
                        repo_id = %row.repo_id,
                        error = %err,
                        "GitHub PR provider merge preflight failed before provider mutation"
                    );
                    self.update_existing_provider_status(
                        db,
                        &row,
                        ExistingProviderStatusRecord {
                            status: ProviderRepoStatus::Failed,
                            pull: None,
                            merge_sha: None,
                            last_error: Some(err.to_string()),
                            event_type: ProviderEventType::Failed,
                            actor_id,
                        },
                    )
                    .await?;
                    blocked_before_provider_mutation = true;
                }
            }
        }

        if blocked_before_provider_mutation {
            self.recompute_aggregate_provider_status(db, pull_request_id)
                .await?;
            return Ok(());
        }

        // Repositories merged past GitHub's rules go first, so GitHub refusing
        // the person's bypass stops the merge before any other repository's
        // irreversible provider merge (g5.git.provider-override).
        plans.sort_by_key(|plan| !plan.bypass);
        for plan in plans {
            let failed_row = plan.row.clone();
            let repo_id = failed_row.repo_id;
            let bypass = plan.bypass;
            dispatched.insert(repo_id);
            if let Err(err) = self
                .merge_one_provider_pr_after_preflight(
                    db,
                    plan,
                    &context.target_workspace_name,
                    actor_id,
                )
                .await
            {
                tracing::warn!(
                    pull_request_id = %context.id,
                    repo_id = %repo_id,
                    error = %err,
                    "GitHub PR provider merge failed after local workspace PR merge"
                );
                if matches!(&err, G5Error::Conflict { reason } if reason == STALE_PROVIDER_MERGE_COMPLETION)
                {
                    return Err(err);
                }
                self.update_existing_provider_status(
                    db,
                    &failed_row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::Failed,
                        pull: None,
                        merge_sha: None,
                        last_error: Some(err.to_string()),
                        event_type: ProviderEventType::Failed,
                        actor_id,
                    },
                )
                .await?;
                if bypass {
                    break;
                }
            }
        }

        self.recompute_aggregate_provider_status(db, pull_request_id)
            .await?;
        Ok(())
    }

    async fn capture_provider_merge(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<ProviderMergeCapture, G5Error> {
        // Repository edits cannot change the provider identity between capture
        // and dispatch. NOWAIT avoids reversing a concurrent removal's lock order.
        sqlx::query("SELECT r.id FROM repos r JOIN workspace_pull_request_provider_repos p ON p.repo_id=r.id AND p.org_id=r.org_id WHERE p.pull_request_id=$1 ORDER BY r.id FOR SHARE OF r NOWAIT")
            .bind(pull_request_id).fetch_all(db.conn()).await.map_err(|error| {
                if error.as_database_error().is_some_and(|e| e.code().as_deref()==Some("55P03")) {
                    G5Error::Conflict { reason: "Provider repository is being changed; retry the merge".into() }
                } else { error.into() }
            })?;
        let (source_id, target_id): (Uuid,Uuid) = sqlx::query_as(
            "SELECT source_workspace_id,target_workspace_id FROM workspace_pull_requests WHERE id=$1")
            .bind(pull_request_id).fetch_one(db.conn()).await?;
        let mut workspaces = [source_id, target_id];
        workspaces.sort();
        for workspace in workspaces {
            crate::services::branch_commit::acquire_workspace_read_fence(db.conn(), workspace)
                .await?;
        }
        // Keep the existing source-before-PR order used by publication.
        sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(pull_request_id)
            .fetch_optional(db.conn())
            .await?;
        let context = self.load_context(db, pull_request_id).await?;
        if context.status != "open" {
            return Err(G5Error::Conflict {
                reason: "Workspace pull request is no longer open".into(),
            });
        }
        let rows = self.load_provider_rows(db, pull_request_id).await?;
        let changed_identity: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos p JOIN repos r ON r.id=p.repo_id AND r.org_id=p.org_id WHERE p.pull_request_id=$1 AND (r.provider IS DISTINCT FROM p.provider OR lower(r.provider_full_name) IS DISTINCT FROM lower(p.provider_repo_full_name)))")
            .bind(pull_request_id).fetch_one(db.conn()).await?;
        if changed_identity {
            return Err(G5Error::Conflict {
                reason:
                    "Provider PR repository identity changed; reconcile its binding before merging"
                        .into(),
            });
        }
        let target_workspace_id: Uuid = sqlx::query_scalar(
            "SELECT target_workspace_id FROM workspace_pull_requests WHERE id=$1",
        )
        .bind(pull_request_id)
        .fetch_one(db.conn())
        .await?;
        let bindings = crate::services::git::remote_integrations::remote_branches(
            db.conn(),
            target_workspace_id,
            false,
        )
        .await?;
        if rows.iter().any(|row| {
            row.provider != "github"
                || bindings.get(&row.repo_id) != Some(&row.target_remote_branch)
        }) {
            return Err(G5Error::Conflict {
                reason: "Provider PR records no longer match the target remote bindings".into(),
            });
        }
        let source = crate::services::git::workspace_reader::resolve_committed_snapshot(
            db,
            &self.repo_storage,
            g5_contracts::types::AuthorityTarget::Workspace {
                workspace_id: context.source_workspace_id,
            },
        )
        .await?;
        let target = crate::services::git::workspace_reader::resolve_committed_snapshot(
            db,
            &self.repo_storage,
            g5_contracts::types::AuthorityTarget::Workspace {
                workspace_id: target_workspace_id,
            },
        )
        .await?;
        // Validate every omitted remote member before accepting any provider
        // PR. An unchanged member needs no empty PR, but changed members must
        // never silently fall through to a local merge.
        for repo_id in bindings.keys() {
            if !rows.iter().any(|row| row.repo_id == *repo_id) {
                let source_member = source.repo(*repo_id).map_err(|reason| G5Error::Conflict {
                    reason: reason.to_string(),
                })?;
                let target_member = target.repo(*repo_id).map_err(|reason| G5Error::Conflict {
                    reason: reason.to_string(),
                })?;
                self.require_source_in_target(
                    &self.repo_storage.scoped(&*db),
                    source.org_id,
                    *repo_id,
                    &source_member.head_commit,
                    &target_member.head_commit,
                )?;
            }
        }
        if rows.is_empty() {
            self.set_aggregate_provider_status(db, pull_request_id, "local_only")
                .await?;
            return Ok(ProviderMergeCapture {
                context,
                rows,
                target,
                admission: None,
            });
        }
        if rows.iter().any(|row| {
            !source.repos.iter().any(|member| {
                member.repo_id == row.repo_id
                    && row.source_head_sha.as_deref() == Some(member.head_commit.as_str())
            })
        }) {
            return Err(G5Error::Conflict {
                reason:
                    "Workspace source changed since PR publication; update the PR before merging"
                        .into(),
            });
        }
        // Admission identity before any provider mutation
        // (g5.governance.approval.requirements, g5.governance.merge): the
        // recorded admission must name exactly the cut the provider is about
        // to merge — this source, this target and this source head vector
        // (the published heads just checked against it). A push and PR update
        // after the admission refuse here, before any provider merge, instead
        // of after the provider accepted an unapproved revision.
        let admission = self
            .require_admission_for_provider_merge(
                db,
                pull_request_id,
                context.org_id,
                context.source_workspace_id,
                target_workspace_id,
                &source,
            )
            .await?;

        // Reservation and dispatch each retain current override authority.
        // Already accepted provider cuts instead keep their recovery admission.
        if !rows
            .iter()
            .any(|row| row.merge_accepted || row.provider_status == "merged")
        {
            if let Some(record) = admission.merge_override.as_ref() {
                super::merge_override::require_current_override_authority(
                    self.merge_admission.as_ref(),
                    db,
                    context.source_workspace_id,
                    target_workspace_id,
                    record,
                )
                .await?;
            }
        }

        Ok(ProviderMergeCapture {
            context,
            rows,
            target,
            admission: Some(admission),
        })
    }

    #[allow(clippy::too_many_arguments)] // One provider publication context and its recorded evidence.
    #[allow(clippy::too_many_arguments)]
    async fn create_or_update_one(
        &self,
        db: &mut ScopedDb,
        context: &PullRequestProviderContext,
        candidate: &ProviderCandidate,
        actor_id: Uuid,
        write: ProviderPullRequestWrite<'_>,
        after_push: bool,
        recorded: Option<&ProviderRepoRow>,
        // This G5 PR's provider PR for the repository, even when sync looks
        // up the open PR by branch instead.
        prior: Option<&ProviderRepoRow>,
    ) -> Result<(), G5Error> {
        let title = write.title;
        if recorded
            .is_some_and(|row| row.provider_status == "merged" || row.provider_status == "closed")
        {
            return Err(G5Error::Conflict {
                reason: "Provider PR has completed; finish merge recovery before updating this PR"
                    .into(),
            });
        }
        let auth = match self
            .resolve_publication_auth(
                db,
                after_push,
                context.org_id,
                candidate.repo_id,
                actor_id,
                &candidate.git_url,
                RepositoryOperation::WritePullRequests,
            )
            .await
        {
            Ok(auth) => auth,
            Err(message) => {
                if after_push {
                    return Err(message);
                }
                self.record_provider_status(
                    db,
                    context,
                    candidate,
                    ProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        last_error: Some(message.to_string()),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(());
            }
        };
        let token = match token_from_auth(&auth) {
            Some(token) => token,
            None => {
                if after_push {
                    return Err(G5Error::Conflict {
                        reason: "GitHub credential is required to refresh this PR".into(),
                    });
                }
                self.record_provider_status(
                    db,
                    context,
                    candidate,
                    ProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        last_error: Some(
                            "GitHub credential is required for provider PR creation".to_string(),
                        ),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(());
            }
        };

        if let Some(row) = recorded.filter(|_| !after_push) {
            if row.provider_repo_full_name != candidate.provider_full_name {
                return Err(G5Error::Conflict {
                    reason: "Recorded provider repository changed".into(),
                });
            }
            let number = row.provider_pr_number.ok_or_else(|| G5Error::Conflict {
                reason: "Existing provider PR number is missing".into(),
            })?;
            let pull = self
                .get_pull_request(&token, &candidate.provider_full_name, number)
                .await
                .map_err(provider_error_to_g5)?;
            if pull.merged.unwrap_or(false) || pull.state != "open" {
                return Err(G5Error::Conflict {
                    reason:
                        "Provider PR has completed; finish merge recovery before updating this PR"
                            .into(),
                });
            }
        }

        // A recorded provider PR that GitHub reports merged or closed is never
        // replaced silently: only an explicit `create --new` opens another.
        if let Some(row) = prior.filter(|_| recorded.is_none() && !after_push) {
            if let Some(number) = row.provider_pr_number {
                let completed = match row.provider_status.as_str() {
                    "merged" | "closed" => Some(row.provider_status.clone()),
                    _ => {
                        let pull = self
                            .get_pull_request(&token, &candidate.provider_full_name, number)
                            .await
                            .map_err(provider_error_to_g5)?;
                        if pull.merged.unwrap_or(false) {
                            Some("merged".to_owned())
                        } else if pull.state != "open" {
                            Some("closed".to_owned())
                        } else {
                            None
                        }
                    }
                };
                if let Some(state) = completed.filter(|_| !write.replace) {
                    let message = format!(
                        "GitHub PR #{number} was {state} outside this G5 PR, so G5 did not open a replacement. Run `g5d pr create --new` to open one."
                    );
                    self.update_existing_provider_status(
                        db,
                        row,
                        ExistingProviderStatusRecord {
                            status: if row.provider_status == "merged" {
                                ProviderRepoStatus::Merged
                            } else {
                                ProviderRepoStatus::ActionRequired
                            },
                            pull: None,
                            merge_sha: None,
                            last_error: Some(message),
                            event_type: ProviderEventType::ActionRequired,
                            actor_id,
                        },
                    )
                    .await?;
                    return Ok(());
                }
            }
        }

        let auth = self
            .resolve_publication_auth(
                db,
                after_push,
                context.org_id,
                candidate.repo_id,
                actor_id,
                &candidate.git_url,
                if after_push {
                    RepositoryOperation::ReadGit
                } else {
                    RepositoryOperation::WriteGit
                },
            )
            .await?;

        let source_publication = if after_push {
            // Remote integration owns automatic/manual push policy. A successful
            // internal Git push cannot grant an extra external write here.
            match self
                .published_remote_head(candidate, &candidate.source_remote_branch, &auth)
                .await
            {
                Ok(head) if head == candidate.source_head => Ok(()),
                // Remote integration is still pushing it: a later refresh converges.
                Ok(_) => {
                    return Err(
                        crate::services::workspace_pull_request_create::provider_branch_behind(),
                    )
                }
                Err(err) => Err(format!("Could not verify published source: {err}")),
            }
        } else {
            self.ensure_provider_source_branch_pushed(
                &self.repo_storage.scoped(&*db),
                context.org_id,
                candidate,
                &auth,
            )
            .await
        };
        if let Err(err) = source_publication {
            if after_push {
                return Err(G5Error::Conflict { reason: err });
            }
            self.record_provider_status(
                db,
                context,
                candidate,
                ProviderStatusRecord {
                    status: ProviderRepoStatus::ActionRequired,
                    pull: None,
                    last_error: Some(format!(
                        "Could not push source branch {} to {} before creating provider PR: {err}",
                        candidate.source_local_branch_name, candidate.source_remote_branch
                    )),
                    event_type: ProviderEventType::ActionRequired,
                    actor_id,
                },
            )
            .await?;
            return Ok(());
        }

        let target_observation = if after_push {
            self.published_remote_head(candidate, &candidate.target_remote_branch, &auth)
                .await
        } else {
            remote_branch_tip(
                &candidate.git_url,
                &candidate.target_remote_branch,
                &auth,
                NETWORK_DEADLINE,
            )
            .await
        };
        let target_tip = match target_observation {
            Ok(tip) => tip,
            Err(err) => {
                if after_push {
                    return Err(err);
                }
                self.record_provider_status(
                    db,
                    context,
                    candidate,
                    ProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        last_error: Some(format!(
                            "Target branch {} is unavailable for provider PR creation: {err}",
                            candidate.target_remote_branch
                        )),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(());
            }
        };
        if target_tip != candidate.target_head {
            // The remote target branch advancing past the locally captured
            // snapshot is advisory, never a create/update precondition
            // (coordinated_repo_publication.semlift.md D13 /
            // core_workflow/merge_workflow D31, issue #869): GitHub itself
            // opens PRs against the moved base, and the actual-vs-expected
            // SHAs stay observable on the provider row as
            // `provider_base_sha` vs `target_head_sha`.
            tracing::info!(
                pull_request_id = %context.id,
                repo_id = %candidate.repo_id,
                target_remote_branch = %candidate.target_remote_branch,
                observed_target_tip = %target_tip,
                expected_target_head = %candidate.target_head,
                "provider PR proceeding despite remote target branch advance; divergence is advisory"
            );
        }

        let lookup = if let Some(row) = recorded {
            if row.provider_repo_full_name != candidate.provider_full_name {
                return Err(G5Error::Conflict {
                    reason: "Recorded provider repository changed".into(),
                });
            }
            let number = row.provider_pr_number.ok_or_else(|| G5Error::Conflict {
                reason: "Existing provider PR number is missing".into(),
            })?;
            self.get_pull_request(&token, &candidate.provider_full_name, number)
                .await
                .map(Some)
        } else {
            self.find_open_pull_request(
                &token,
                &candidate.provider_full_name,
                &candidate.source_remote_branch,
                &candidate.target_remote_branch,
            )
            .await
        };
        let existing = match lookup {
            Ok(existing) => existing,
            Err(err) if err.is_action_required() => {
                if after_push {
                    return Err(provider_error_to_g5(err));
                }
                self.record_provider_status(
                    db,
                    context,
                    candidate,
                    ProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        last_error: Some(err.message),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(());
            }
            Err(err) => return Err(provider_error_to_g5(err)),
        };
        let (mut pull, event_type) = if let Some(pull) = existing {
            if (after_push || recorded.is_some())
                && (pull.merged.unwrap_or(false) || pull.state != "open")
            {
                return Err(G5Error::Conflict {
                    reason: "Provider PR completed during refresh; preserve its merge version"
                        .into(),
                });
            }
            (pull, ProviderEventType::Updated)
        } else {
            // Discovery/reconciliation and branch publication keep their App
            // credentials. Only a new PR uses the initiating person's grant.
            let auth = self
                .resolve_publication_auth(
                    db,
                    after_push,
                    context.org_id,
                    candidate.repo_id,
                    actor_id,
                    &candidate.git_url,
                    RepositoryOperation::CreatePullRequest,
                )
                .await
                .and_then(|auth| {
                    token_from_auth(&auth).ok_or_else(|| G5Error::Conflict {
                        reason: "GitHub credential is required for provider PR creation"
                            .to_string(),
                    })
                });
            let token = match auth {
                Ok(token) => token,
                Err(message) => {
                    if after_push {
                        return Err(message);
                    }
                    self.record_provider_status(
                        db,
                        context,
                        candidate,
                        ProviderStatusRecord {
                            status: ProviderRepoStatus::ActionRequired,
                            pull: None,
                            last_error: Some(message.to_string()),
                            event_type: ProviderEventType::ActionRequired,
                            actor_id,
                        },
                    )
                    .await?;
                    return Ok(());
                }
            };
            let mut payload = self.build_provider_pr_payload(context, candidate).await;
            if let Some(title) = title {
                payload.title = title.to_owned();
            }
            match self.create_pull_request(&token, candidate, &payload).await {
                Ok(pull) => (pull, ProviderEventType::Created),
                Err(err) if err.status == Some(422) && !err.rate_limited => {
                    let raced = self
                        .find_open_pull_request(
                            &token,
                            &candidate.provider_full_name,
                            &candidate.source_remote_branch,
                            &candidate.target_remote_branch,
                        )
                        .await
                        .map_err(provider_error_to_g5)?;
                    if let Some(pull) = raced {
                        (pull, ProviderEventType::Updated)
                    } else if payload.derived {
                        // Spec D11: a derived payload rejected with 422 and no
                        // raced open PR gets one retry with the legacy payload;
                        // the retry re-applies the D16 body precedence (the
                        // workspace PR-body Project Doc already read for this
                        // run, then the D15 description override on top).
                        tracing::warn!(
                            pull_request_id = %context.id,
                            repo_id = %candidate.repo_id,
                            error = %err.message,
                            "derived provider PR payload rejected; retrying with legacy payload"
                        );
                        let mut legacy = legacy_retry_payload(context);
                        if let Some(title) = title {
                            legacy.title = title.to_owned();
                        }
                        match self.create_pull_request(&token, candidate, &legacy).await {
                            Ok(pull) => (pull, ProviderEventType::Created),
                            Err(err2) if err2.status == Some(422) && !err2.rate_limited => {
                                let pull = self
                                    .find_open_pull_request(
                                        &token,
                                        &candidate.provider_full_name,
                                        &candidate.source_remote_branch,
                                        &candidate.target_remote_branch,
                                    )
                                    .await
                                    .map_err(provider_error_to_g5)?
                                    .ok_or_else(|| G5Error::Conflict {
                                        reason: err2.message.clone(),
                                    })?;
                                (pull, ProviderEventType::Updated)
                            }
                            Err(err2) if err2.is_action_required() => {
                                if after_push {
                                    return Err(provider_error_to_g5(err2));
                                }
                                self.record_provider_status(
                                    db,
                                    context,
                                    candidate,
                                    ProviderStatusRecord {
                                        status: ProviderRepoStatus::ActionRequired,
                                        pull: None,
                                        last_error: Some(err2.message),
                                        event_type: ProviderEventType::ActionRequired,
                                        actor_id,
                                    },
                                )
                                .await?;
                                return Ok(());
                            }
                            Err(err2) => return Err(provider_error_to_g5(err2)),
                        }
                    } else {
                        return Err(G5Error::Conflict {
                            reason: err.message.clone(),
                        });
                    }
                }
                Err(err) if err.is_action_required() => {
                    if after_push {
                        return Err(provider_error_to_g5(err));
                    }
                    self.record_provider_status(
                        db,
                        context,
                        candidate,
                        ProviderStatusRecord {
                            status: ProviderRepoStatus::ActionRequired,
                            pull: None,
                            last_error: Some(err.message),
                            event_type: ProviderEventType::ActionRequired,
                            actor_id,
                        },
                    )
                    .await?;
                    return Ok(());
                }
                Err(err) => return Err(provider_error_to_g5(err)),
            }
        };

        // Explicit title and description reach existing provider PRs; a new
        // PR was created with them. Omission never reasserts stored text.
        let reused = matches!(event_type, ProviderEventType::Updated);
        let body = if reused && write.body {
            Some(
                self.build_provider_pr_payload(context, candidate)
                    .await
                    .body,
            )
        } else {
            None
        };
        if reused && (title.is_some() || body.is_some()) {
            match self
                .update_pull_request(
                    &token,
                    &candidate.provider_full_name,
                    pull.number,
                    title,
                    body.as_deref(),
                )
                .await
            {
                Ok(updated) => pull = updated,
                Err(err) => {
                    if after_push {
                        return Err(provider_error_to_g5(err));
                    }
                    self.record_provider_status(
                        db,
                        context,
                        candidate,
                        ProviderStatusRecord {
                            status: if err.is_action_required() {
                                ProviderRepoStatus::ActionRequired
                            } else {
                                ProviderRepoStatus::Failed
                            },
                            pull: Some(&pull),
                            last_error: Some(err.message),
                            event_type: ProviderEventType::Failed,
                            actor_id,
                        },
                    )
                    .await?;
                    return Ok(());
                }
            }
        }

        if after_push
            && (pull.head.sha != candidate.source_head
                || pull.merged.unwrap_or(false)
                || pull.state != "open")
        {
            return Err(G5Error::Conflict {
                reason: "Provider head changed during refresh; retry publication".into(),
            });
        }
        self.record_provider_status(
            db,
            context,
            candidate,
            ProviderStatusRecord {
                status: ProviderRepoStatus::Open,
                pull: Some(&pull),
                last_error: None,
                event_type,
                actor_id,
            },
        )
        .await?;
        Ok(())
    }

    async fn ensure_provider_source_branch_pushed(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        candidate: &ProviderCandidate,
        auth: &GitAuth,
    ) -> Result<(), String> {
        // Observe the current remote source tip. When it already matches the
        // captured source head there is nothing to publish. Otherwise the
        // observed tip is the fast-forward-vs-force-with-lease decision input
        // for the push below (mirroring explicit push-to-remote): `Some(tip)`
        // is a present remote branch, `None` an absent one. A probe failure
        // that is not a clean `NotFound` (network/auth) also degrades to
        // `None` — the push below then makes a plain fast-forward attempt
        // (the pre-existing behavior) and its own error is authoritative —
        // but is logged so the degraded decision input is observable.
        let observed_remote_tip = match remote_branch_tip(
            &candidate.git_url,
            &candidate.source_remote_branch,
            auth,
            NETWORK_DEADLINE,
        )
        .await
        {
            Ok(tip) if tip == candidate.source_head => return Ok(()),
            Ok(tip) => Some(tip),
            Err(G5Error::NotFound { .. }) => None,
            Err(err) => {
                tracing::warn!(
                    repo_id = %candidate.repo_id,
                    remote_branch = %candidate.source_remote_branch,
                    error = %err,
                    "provider source pre-push probe failed; proceeding with a plain fast-forward attempt"
                );
                None
            }
        };

        self.push_provider_source_branch(
            repos,
            org_id,
            candidate,
            auth.clone(),
            observed_remote_tip,
        )
        .await?;

        let verified_tip = remote_branch_tip(
            &candidate.git_url,
            &candidate.source_remote_branch,
            auth,
            NETWORK_DEADLINE,
        )
        .await
        .map_err(|err| {
            format!(
                "source branch {} could not be verified after push: {err}",
                candidate.source_remote_branch
            )
        })?;
        if verified_tip != candidate.source_head {
            return Err(format!(
                "source branch {} is at {} after push, expected {}",
                candidate.source_remote_branch, verified_tip, candidate.source_head
            ));
        }
        Ok(())
    }

    async fn push_provider_source_branch(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        candidate: &ProviderCandidate,
        auth: GitAuth,
        observed_remote_tip: Option<String>,
    ) -> Result<(), String> {
        let local_path = repos
            .repo_path(org_id, candidate.repo_id)
            .map_err(|error| {
                format!(
                    "repository {} has no local filesystem path for provider push: {error}",
                    candidate.repo_id
                )
            })?;
        let git_url = candidate.git_url.clone();
        let local_branch = candidate.source_local_branch_name.clone();
        let remote_branch = candidate.source_remote_branch.clone();
        let expected_source_head = candidate.source_head.clone();

        let git = crate::git_runtime::network(PROVIDER_SOURCE_PUSH_TIMEOUT)
            .map_err(|err| err.to_string())?;
        let join = tokio::time::timeout(
            PROVIDER_SOURCE_PUSH_TIMEOUT,
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let repo = GitRepo::open(&local_path).map_err(|err| err.to_string())?;
                repo.ensure_remote_url("origin", &git_url)
                    .map_err(|err| err.to_string())?;
                let local_head = repo
                    .resolve_branch_head(&local_branch)
                    .map_err(|err| err.to_string())?;
                if local_head != expected_source_head {
                    return Err(format!(
                        "local workspace branch {local_branch} is at {local_head}, expected {expected_source_head}"
                    ));
                }
                // Publish using the same fast-forward-vs-force-with-lease
                // decision as explicit push-to-remote
                // (@ref core_workflow/external_git_integration#D8,
                // software_architecture/g5-api/services/coordinated_repo_publication#D4):
                // a plain fast-forward when the remote branch is absent or its
                // observed tip is an ancestor of the captured head, and a
                // force-with-lease scoped to the observed remote tip when a
                // platform rebase rewrote history so the remote tip is no longer
                // an ancestor.
                repo.publish_commit(
                    &git,
                    "origin",
                    &expected_source_head,
                    &remote_branch,
                    observed_remote_tip.as_deref(),
                    &auth,
                )
                .map_err(|err| err.to_string())?;
                Ok(())
            }),
        )
        .await
        .map_err(|_| {
            format!(
                "timed out pushing {} to {}",
                candidate.source_local_branch_name, candidate.source_remote_branch
            )
        })?;

        join.map_err(|err| format!("provider push task failed: {err}"))?
    }

    /// Require an admission for exactly the committed source cut about to be
    /// merged at the provider, under the pull request lock the caller holds.
    /// A recorded admission (pinned by the merge route) must be admitted and
    /// name this source, this target and this head vector; without one (a
    /// caller that did not go through the merge route) the merge admits
    /// itself for this exact cut under the policy locks and records it.
    async fn require_admission_for_provider_merge(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        org_id: Uuid,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        source: &g5_contracts::types::CommittedBindingSnapshot,
    ) -> Result<MergeAdmissionEvidence, G5Error> {
        let mut cut: Vec<MergeSourceHead> = source
            .repos
            .iter()
            .map(|repo| MergeSourceHead {
                repo_id: repo.repo_id.to_string(),
                branch_name: repo.branch_name.clone(),
                workspace_id: repo.workspace_id.to_string(),
                head_commit: repo.head_commit.clone(),
            })
            .collect();
        cut.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
        let recorded: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT merge_admission FROM workspace_pull_requests WHERE id = $1")
                .bind(pull_request_id)
                .fetch_optional(db.conn())
                .await?
                .flatten();
        let admission = match recorded {
            Some(recorded) => {
                serde_json::from_value::<MergeAdmissionEvidence>(recorded).map_err(|error| {
                    G5Error::Internal {
                        message: format!("Recorded merge admission evidence is malformed: {error}"),
                    }
                })?
            }
            None => {
                self.merge_admission
                    .lock(db, org_id, target_workspace_id)
                    .await?;
                let admission = self
                    .merge_admission
                    .evaluate(db, org_id, source_workspace_id, target_workspace_id, &cut)
                    .await?;
                let evidence =
                    serde_json::to_value(&admission).map_err(|error| G5Error::Internal {
                        message: format!("Failed to serialize merge admission evidence: {error}"),
                    })?;
                sqlx::query(
                    "UPDATE workspace_pull_requests SET merge_admission = $1, updated_at = now()
                     WHERE id = $2 AND status = 'open'",
                )
                .bind(&evidence)
                .bind(pull_request_id)
                .execute(db.conn())
                .await?;
                admission
            }
        };
        if !admission.admitted {
            return Err(G5Error::Conflict {
                reason: admission.reason.clone().unwrap_or_else(|| {
                    "Integration Engineering approval required before merging".to_string()
                }),
            });
        }
        let mut recorded_heads = admission.source_heads.clone();
        recorded_heads.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
        if admission.source_workspace_id != source_workspace_id
            || admission.target_workspace_id != target_workspace_id
            || recorded_heads != cut
        {
            return Err(G5Error::Conflict {
                reason: "Source revision changed since the merge admission; request approval for the current revision and retry the merge".to_string(),
            });
        }
        Ok(admission)
    }

    /// Reconcile provider outcomes the database never acknowledged
    /// (g5.governance.merge): a provider PR GitHub reports merged at the
    /// published source head is recorded as merged before the merge route
    /// admits the pull request again, so a lost acknowledgement or a partial
    /// acceptance keeps the admission pinned for that cut instead of being
    /// re-evaluated — and possibly refused — after an irreversible provider
    /// merge. Provider read failures leave the row as it is.
    pub async fn reconcile_provider_acceptance(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<(), G5Error> {
        sqlx::query("SELECT id FROM workspace_pull_requests WHERE id=$1 FOR UPDATE")
            .bind(pull_request_id)
            .fetch_optional(db.conn())
            .await?;
        let rows = self.load_provider_rows(db, pull_request_id).await?;
        let mut acknowledged = false;
        for row in rows.iter().filter(|row| {
            row.provider_pr_number.is_some()
                && !matches!(row.provider_status.as_str(), "merged" | "closed")
        }) {
            let Some(number) = row.provider_pr_number else {
                continue;
            };
            let auth = match self
                .credentials
                .resolve_in_scope(
                    db,
                    row.org_id,
                    row.repo_id,
                    actor_id,
                    &row.git_url,
                    RepositoryOperation::ReadPullRequests,
                )
                .await
            {
                Ok(auth) => auth,
                Err(error) => {
                    tracing::warn!(
                        pull_request_id = %pull_request_id,
                        repo_id = %row.repo_id,
                        %error,
                        "provider acceptance reconciliation has no provider credential"
                    );
                    continue;
                }
            };
            let Some(token) = token_from_auth(&auth) else {
                continue;
            };
            let pull = match self
                .get_pull_request(&token, &row.provider_repo_full_name, number)
                .await
            {
                Ok(pull) => pull,
                Err(error) => {
                    tracing::warn!(
                        pull_request_id = %pull_request_id,
                        repo_id = %row.repo_id,
                        error = ?error,
                        "provider acceptance reconciliation could not read the provider PR"
                    );
                    continue;
                }
            };
            if pull
                .base
                .ref_name
                .as_deref()
                .is_some_and(|branch| branch != row.target_remote_branch)
            {
                continue;
            }
            if !pull.merged.unwrap_or(false) && pull.state == "closed" && row.merge_pending {
                sqlx::query("UPDATE workspace_pull_request_provider_repos SET merge_pending=false WHERE id=$1 AND merge_attempt_id=$2 AND merge_execution_id=$3")
                    .bind(row.id).bind(row.merge_attempt_id).bind(row.merge_execution_id).execute(db.conn()).await?;
            }
            if pull.merged.unwrap_or(false)
                && row.source_head_sha.as_deref() == Some(pull.head.sha.as_str())
            {
                self.update_existing_provider_status(
                    db,
                    row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::Merged,
                        pull: Some(&pull),
                        merge_sha: None,
                        last_error: None,
                        event_type: ProviderEventType::Merged,
                        actor_id,
                    },
                )
                .await?;
                self.mark_merged_outside(db, row, actor_id).await?;
                acknowledged = true;
            }
        }
        if acknowledged {
            self.recompute_aggregate_provider_status(db, pull_request_id)
                .await?;
        }
        Ok(())
    }

    async fn preflight_one_provider_pr_for_merge(
        &self,
        db: &mut ScopedDb,
        row: &ProviderRepoRow,
        classified_target: Option<&str>,
        target_workspace_name: &str,
        actor_id: Uuid,
        bypass_allowed: bool,
    ) -> Result<ProviderMergePreflight, G5Error> {
        let Some(number) = row.provider_pr_number else {
            return Err(G5Error::Conflict {
                reason: "Provider PR number is missing".to_string(),
            });
        };
        let auth = match self
            .resolve_github_auth(
                row.org_id,
                row.repo_id,
                actor_id,
                &row.git_url,
                RepositoryOperation::ReadPullRequests,
            )
            .await
        {
            Ok(auth) => auth,
            Err(message) => {
                self.update_existing_provider_status(
                    db,
                    row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        merge_sha: None,
                        last_error: Some(message),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(ProviderMergePreflight::NotReady);
            }
        };
        let Some(token) = token_from_auth(&auth) else {
            self.update_existing_provider_status(
                db,
                row,
                ExistingProviderStatusRecord {
                    status: ProviderRepoStatus::ActionRequired,
                    pull: None,
                    merge_sha: None,
                    last_error: Some(
                        "GitHub credential is required for provider PR merge".to_string(),
                    ),
                    event_type: ProviderEventType::ActionRequired,
                    actor_id,
                },
            )
            .await?;
            return Ok(ProviderMergePreflight::NotReady);
        };
        let pull = match self
            .get_pull_request(&token, &row.provider_repo_full_name, number)
            .await
        {
            Ok(pull) => pull,
            Err(err) if err.is_action_required() => {
                self.update_existing_provider_status(
                    db,
                    row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        merge_sha: None,
                        last_error: Some(err.message),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(ProviderMergePreflight::NotReady);
            }
            Err(err) => return Err(provider_error_to_g5(err)),
        };
        if (row.merge_accepted || row.provider_status == "merged") && !pull.merged.unwrap_or(false)
        {
            return Err(G5Error::Conflict { reason: "Provider no longer confirms the accepted merge; reconcile its history before retrying".into() });
        }
        if row.source_head_sha.as_deref() != Some(pull.head.sha.as_str()) {
            return Err(G5Error::Conflict {
                reason: format!("GitHub PR #{} head differs from the pinned source", number),
            });
        }
        // A PR retargeted on GitHub would merge into another branch while the
        // pinned target's tip still matches below.
        if let Some(base_branch) = pull.base.ref_name.as_deref() {
            if base_branch != row.target_remote_branch {
                return Err(G5Error::Conflict {
                    reason: format!(
                        "GitHub PR #{} targets {}, expected {}",
                        number, base_branch, row.target_remote_branch
                    ),
                });
            }
        }
        if pull.merged.unwrap_or(false) {
            self.update_existing_provider_status(
                db,
                row,
                ExistingProviderStatusRecord {
                    status: ProviderRepoStatus::Merged,
                    pull: Some(&pull),
                    merge_sha: None,
                    last_error: None,
                    event_type: ProviderEventType::Merged,
                    actor_id,
                },
            )
            .await?;
            // A repository this pull request's earlier attempt merged is
            // already recorded merged; only a newly found merge is GitHub's.
            if row.provider_status != "merged" {
                self.mark_merged_outside(db, row, actor_id).await?;
            }
            return Ok(ProviderMergePreflight::AlreadyMerged);
        }
        if pull.state != "open" {
            return Err(G5Error::Conflict {
                reason: format!("GitHub PR #{} is {}", number, pull.state),
            });
        }
        let auth = self
            .credentials
            .resolve(
                row.org_id,
                row.repo_id,
                actor_id,
                &row.git_url,
                RepositoryOperation::ReadGit,
            )
            .await?;
        if let Some(expected_source_head) = row.source_head_sha.as_deref() {
            if pull.head.sha != expected_source_head {
                return Err(G5Error::Conflict {
                    reason: format!("GitHub PR #{} head changed before provider merge", number),
                });
            }
            let source_tip = remote_branch_tip(
                &row.git_url,
                &row.source_remote_branch,
                &auth,
                NETWORK_DEADLINE,
            )
            .await
            .map_err(|err| G5Error::Conflict {
                reason: format!(
                    "GitHub PR #{} source branch {} cannot be verified before provider merge: {}",
                    number, row.source_remote_branch, err
                ),
            })?;
            if source_tip != expected_source_head {
                return Err(G5Error::Conflict {
                    reason: format!(
                        "GitHub PR #{} source branch {} is at {}, expected {}",
                        number, row.source_remote_branch, source_tip, expected_source_head
                    ),
                });
            }
        }
        // GitHub's `base.sha` is fixed when the PR is opened (issue #551), and
        // the target recorded at publication goes stale whenever the target
        // moves (issue #582). The safety property is GitHub's live target tip
        // matching the target this merge's fresh preflight classified, so
        // GitHub rebases onto history G5 checked.
        let expected_target_head = classified_target.ok_or_else(|| G5Error::Conflict {
            reason: format!(
                "Merge to {target_workspace_name} did not check its {} branch; merge again",
                row.target_remote_branch
            ),
        })?;
        let target_tip = remote_branch_tip(
            &row.git_url,
            &row.target_remote_branch,
            &auth,
            NETWORK_DEADLINE,
        )
        .await
        .map_err(|err| G5Error::Conflict {
            reason: format!(
                "GitHub PR #{} target branch {} cannot be verified before provider merge: {}",
                number, row.target_remote_branch, err
            ),
        })?;
        if target_tip != expected_target_head {
            // Target workspaces import provider history and never push it.
            return Err(G5Error::Conflict {
                reason: format!(
                    "GitHub {branch} is at {target_tip}, but {target_workspace_name} is at {expected_target_head}: {target_workspace_name} hasn't pulled the latest {branch} yet. It pulls automatically, usually within a minute; merge again after that, or fetch on {target_workspace_name}",
                    branch = row.target_remote_branch,
                ),
            });
        }
        // Merge to Base always rebases. GitHub drops merge commits while
        // replaying, so a branch carrying one (or a replay conflict) is
        // refused; explain it before calling the merge endpoint. A branch
        // proven linear on the target makes the flag stale: GitHub's own
        // answer then decides.
        if pull.rebaseable == Some(false) {
            let history = self
                .source_history(&self.repo_storage.scoped(&*db), row, expected_target_head)
                .await;
            if history
                .as_ref()
                .is_some_and(SourceHistory::is_linear_on_target)
            {
                tracing::warn!(
                    repo_id = %row.repo_id,
                    "GitHub reports a linear PR branch as unrebaseable"
                );
            } else {
                return Err(G5Error::Conflict {
                    reason: unrebaseable_reason(
                        number,
                        &row.target_remote_branch,
                        target_workspace_name,
                        history.as_ref(),
                        None,
                    ),
                });
            }
        }
        // No merge bypasses GitHub's rules silently (g5.git.provider-rules):
        // GitHub's merge endpoint bypasses them for any caller allowed to,
        // with no opt-in. Only the person whose GitHub override covers this
        // repository merges it while blocked, as their own GitHub account.
        let merge_state = self
            .settled_merge_state(
                &token,
                &row.provider_repo_full_name,
                number,
                pull.mergeable_state.clone(),
            )
            .await;
        let bypass = match merge_state.as_deref() {
            Some(MERGE_STATE_BLOCKED) if bypass_allowed => true,
            Some(MERGE_STATE_BLOCKED) | Some(MERGE_STATE_UNKNOWN) => {
                let reason = if merge_state.as_deref() == Some(MERGE_STATE_BLOCKED) {
                    provider_blocked_reason(number, &row.provider_repo_full_name)
                } else {
                    format!(
                        "GitHub is still working out whether PR #{number} in {} can merge; \
                         merge again in a moment.",
                        row.provider_repo_full_name
                    )
                };
                self.update_existing_provider_status(
                    db,
                    row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        merge_sha: None,
                        last_error: Some(reason),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(ProviderMergePreflight::NotReady);
            }
            _ => false,
        };
        // GitHub's merge endpoint requires contents:write, while reading the
        // PR above needs only pull_requests:read. Preflight both before any
        // repository in a coordinated PR is merged.
        let merge_auth = self
            .credentials
            .resolve(
                row.org_id,
                row.repo_id,
                actor_id,
                &row.git_url,
                if bypass {
                    RepositoryOperation::MergePullRequestWithBypass
                } else {
                    RepositoryOperation::MergePullRequest
                },
            )
            .await?;
        let read_token = token;
        let token = token_from_auth(&merge_auth).ok_or_else(|| G5Error::Conflict {
            reason: "Provider merge requires repository credentials".into(),
        })?;
        Ok(ProviderMergePreflight::Ready(Box::new(ProviderMergePlan {
            row: row.clone(),
            token,
            read_token,
            pull,
            target_head: expected_target_head.to_owned(),
            bypass,
        })))
    }

    /// GitHub's merge state, re-read a few times while GitHub is still
    /// computing it. A read failure keeps the last state seen.
    async fn settled_merge_state(
        &self,
        token: &str,
        repository: &str,
        number: i64,
        mut state: Option<String>,
    ) -> Option<String> {
        for _ in 0..MERGE_STATE_REREADS {
            if state.as_deref() != Some(MERGE_STATE_UNKNOWN) {
                break;
            }
            tokio::time::sleep(MERGE_STATE_REREAD_DELAY).await;
            match self.get_pull_request(token, repository, number).await {
                Ok(pull) => state = pull.mergeable_state,
                Err(_) => break,
            }
        }
        state
    }

    /// Record a repository G5 found already merged on GitHub when it came to
    /// merge it (g5.git.provider-merged-outside). A durable target attempt
    /// records the selected cut, not who caused an observed provider effect;
    /// an unacknowledged invocation retains this conservative attribution.
    async fn mark_merged_outside(
        &self,
        db: &mut ScopedDb,
        row: &ProviderRepoRow,
        actor_id: Uuid,
    ) -> Result<(), G5Error> {
        let marked = sqlx::query(
            "UPDATE workspace_pull_request_provider_repos SET merged_outside_g5 = true
             WHERE id = $1 AND org_id = $2 AND NOT merged_outside_g5",
        )
        .bind(row.id)
        .bind(row.org_id)
        .execute(db.conn())
        .await?
        .rows_affected();
        if marked == 1 {
            sqlx::query(
                "INSERT INTO workspace_pull_request_events (
                     org_id, pull_request_id, event_type, actor_id, message
                 ) VALUES ($1, $2, 'provider_merged_outside', $3, $4)",
            )
            .bind(row.org_id)
            .bind(row.pull_request_id)
            .bind(actor_id)
            .bind(&row.provider_repo_full_name)
            .execute(db.conn())
            .await?;
        }
        Ok(())
    }

    /// The provider repositories of an open pull request that GitHub reports
    /// blocked by branch protection or rulesets (g5.git.provider-rules), for
    /// an override's shown set. Repositories already merged, unreadable or
    /// still being computed are left out: the merge's own preflight refuses
    /// those.
    pub async fn blocked_provider_repositories(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_id: Uuid,
    ) -> Result<Vec<crate::services::git::merge_override::BlockedProviderRepository>, G5Error> {
        let rows = self.load_provider_rows(db, pull_request_id).await?;
        let mut blocked = Vec::new();
        for row in rows
            .iter()
            .filter(|row| !matches!(row.provider_status.as_str(), "merged" | "closed"))
        {
            let Some(number) = row.provider_pr_number else {
                continue;
            };
            let Ok(auth) = self
                .resolve_github_auth(
                    row.org_id,
                    row.repo_id,
                    actor_id,
                    &row.git_url,
                    RepositoryOperation::ReadPullRequests,
                )
                .await
            else {
                continue;
            };
            let Some(token) = token_from_auth(&auth) else {
                continue;
            };
            let Ok(pull) = self
                .get_pull_request(&token, &row.provider_repo_full_name, number)
                .await
            else {
                continue;
            };
            if pull.merged.unwrap_or(false) || pull.state != "open" {
                continue;
            }
            let state = self
                .settled_merge_state(
                    &token,
                    &row.provider_repo_full_name,
                    number,
                    pull.mergeable_state.clone(),
                )
                .await;
            if state.as_deref() == Some(MERGE_STATE_BLOCKED) {
                blocked.push(
                    crate::services::git::merge_override::BlockedProviderRepository {
                        repo_id: row.repo_id,
                        repository: row.provider_repo_full_name.clone(),
                        head_sha: pull.head.sha.clone(),
                        merge_state: MERGE_STATE_BLOCKED.to_string(),
                    },
                );
            }
        }
        Ok(blocked)
    }

    async fn merge_one_provider_pr_after_preflight(
        &self,
        db: &mut ScopedDb,
        plan: ProviderMergePlan,
        target_workspace_name: &str,
        actor_id: Uuid,
    ) -> Result<(), G5Error> {
        let row = plan.row;
        let token = plan.token;
        let number = row.provider_pr_number.ok_or_else(|| G5Error::Conflict {
            reason: "Provider PR number is missing".to_string(),
        })?;
        let pull = plan.pull;
        let result = self
            .merge_pull_request(&token, &row.provider_repo_full_name, number, &pull.head.sha)
            .await;
        if result
            .as_ref()
            .is_err_and(|error| error.effect == g5_scm::FailureEffect::Refused)
        {
            self.release_refused_attempt(db, &row).await?;
        }
        let merge = match result {
            Ok(merge) => merge,
            // A denied bypass stops the batch before any ordinary repository
            // merges, including authentication/permission refusals (401/403).
            Err(err) if plan.bypass && (err.is_merge_refusal() || err.is_action_required()) => {
                tracing::warn!(
                    repo_id = %row.repo_id,
                    error = %err.message,
                    "GitHub refused a merge with GitHub override"
                );
                return Err(G5Error::Conflict {
                    reason: format!(
                        "Your GitHub account can't bypass the rules on PR #{number} in {}: {}",
                        row.provider_repo_full_name, err.provider_message
                    ),
                });
            }
            Err(err) if err.is_action_required() => {
                self.update_existing_provider_status(
                    db,
                    &row,
                    ExistingProviderStatusRecord {
                        status: ProviderRepoStatus::ActionRequired,
                        pull: None,
                        merge_sha: None,
                        last_error: Some(err.message),
                        event_type: ProviderEventType::ActionRequired,
                        actor_id,
                    },
                )
                .await?;
                return Ok(());
            }
            Err(err) if err.is_merge_refusal() => {
                tracing::warn!(
                    repo_id = %row.repo_id,
                    error = %err.message,
                    "GitHub refused the provider merge"
                );
                let history = self
                    .source_history(&self.repo_storage.scoped(&*db), &row, &plan.target_head)
                    .await;
                return Err(G5Error::Conflict {
                    reason: unrebaseable_reason(
                        number,
                        &row.target_remote_branch,
                        target_workspace_name,
                        history.as_ref(),
                        Some(&err.provider_message),
                    ),
                });
            }
            Err(err) => return Err(provider_error_to_g5(err)),
        };
        if !merge.merged {
            return Err(G5Error::Conflict {
                reason: merge
                    .message
                    .unwrap_or_else(|| format!("GitHub PR #{} did not merge", number)),
            });
        }
        // Name the target the provider actually merged onto.
        let settled = sqlx::query(
            "UPDATE workspace_pull_request_provider_repos SET target_head_sha=$1, merge_pending=false, merge_accepted=true WHERE id=$2 AND org_id=$3 AND merge_attempt_id=$4 AND merge_execution_id=$5 AND merge_pending",
        )
        .bind(&plan.target_head)
        .bind(row.id)
        .bind(row.org_id)
        .bind(row.merge_attempt_id)
        .bind(row.merge_execution_id)
        .execute(db.conn())
        .await?
        .rows_affected();
        if settled != 1 {
            return Err(G5Error::Conflict {
                reason: STALE_PROVIDER_MERGE_COMPLETION.into(),
            });
        }
        let merged_pull = self
            .get_pull_request(&plan.read_token, &row.provider_repo_full_name, number)
            .await
            .unwrap_or(pull);
        self.update_existing_provider_status(
            db,
            &row,
            ExistingProviderStatusRecord {
                status: ProviderRepoStatus::Merged,
                pull: Some(&merged_pull),
                merge_sha: merge.sha,
                last_error: None,
                event_type: ProviderEventType::Merged,
                actor_id,
            },
        )
        .await?;
        Ok(())
    }

    /// The pinned source relative to `target`. Best effort: `None` when the
    /// local repository cannot answer.
    async fn source_history(
        &self,
        repos: &ScopedRepoStorage,
        row: &ProviderRepoRow,
        target: &str,
    ) -> Option<SourceHistory> {
        let source = row.source_head_sha.clone()?;
        let target = target.to_owned();
        let path = repos.repo_path(row.org_id, row.repo_id).ok()?;
        tokio::task::spawn_blocking(move || -> Result<SourceHistory, G5Error> {
            let repo = GitRepo::open(&path)?;
            Ok(SourceHistory {
                merges: repo.merge_commits_between(&target, &source, 2)?,
                contains_target: source == target || repo.is_ancestor_of(&target, &source)?,
            })
        })
        .await
        .ok()?
        .inspect_err(|error| {
            tracing::debug!(repo_id = %row.repo_id, %error, "Pinned source history unavailable")
        })
        .ok()
    }

    async fn load_context(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<PullRequestProviderContext, G5Error> {
        let (mut context, source_names) = self.load_context_record(db, pull_request_id).await?;
        context.pr_body_doc = self.read_context_pr_body_doc(&context, &source_names).await;
        Ok(context)
    }

    /// The PR's record without its Project Doc, and the source workspace's
    /// current and former names the doc may be filed under (none when the
    /// preflight cannot name a docs head). Database reads only.
    async fn load_context_record(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<(PullRequestProviderContext, Vec<String>), G5Error> {
        let mut context = sqlx::query_as::<_, PullRequestProviderContext>(
            r#"
            SELECT pr.id,
                   pr.org_id,
                   pr.status,
                   pr.source_workspace_id,
                   sw.name AS source_workspace_name,
                   sw.docs_repo_id AS source_docs_repo_id,
                   tw.name AS target_workspace_name,
                   $2::TEXT AS external_url,
                   pr.preflight,
                   pr.description_override, pr.title
              FROM workspace_pull_requests pr
              JOIN workspaces sw ON sw.id = pr.source_workspace_id
              JOIN workspaces tw ON tw.id = pr.target_workspace_id
             WHERE pr.id = $1
            "#,
        )
        .bind(pull_request_id)
        .bind(&self.external_url)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace pull request {pull_request_id}"),
        })?;

        context.repos = Some(self.repo_storage.scoped(&*db));
        // The tenant role intentionally cannot read `orgs`. Resolve the slug only after
        // the scoped query has authorized and identified this pull request's organization.
        context.source_org_slug =
            sqlx::query_scalar::<_, String>("SELECT slug FROM orgs WHERE id = $1")
                .bind(context.org_id)
                .fetch_optional(&self.system_pool)
                .await?
                .ok_or_else(|| G5Error::NotFound {
                    resource: format!("organization {}", context.org_id),
                })?;

        // The document is named after the workspace; a body written before a
        // rename lives under a former name (`g5.workspace.former-names`).
        let mut source_names = Vec::new();
        if parse_preflight(&context.preflight).is_ok() {
            source_names.push(context.source_workspace_name.clone());
            source_names.extend(
                crate::services::workspace_names::former_names(
                    db.conn(),
                    context.source_workspace_id,
                )
                .await?,
            );
        }
        Ok((context, source_names))
    }

    /// Spec D16: the workspace PR-body Project Doc at the docs repo's
    /// preflight snapshot head. An unparseable preflight is the caller's error
    /// to surface; here it simply means no doc.
    async fn read_context_pr_body_doc(
        &self,
        context: &PullRequestProviderContext,
        source_names: &[String],
    ) -> Option<String> {
        let preflight = parse_preflight(&context.preflight).ok()?;
        read_workspace_pr_body_doc(
            context.repos.clone()?,
            context.org_id,
            &preflight,
            context.source_docs_repo_id,
            context.source_workspace_id,
            source_names,
        )
        .await
    }

    async fn load_provider_candidate(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        repo: &crate::services::workspace_merge_service::MergePreflightRepoResult,
    ) -> Result<Option<ProviderCandidate>, G5Error> {
        let Some(target) = self
            .adapters
            .github_publication_target(
                db.conn(),
                org_id,
                repo.source_workspace_id,
                repo.target_workspace_id,
                repo.repo_id,
            )
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(ProviderCandidate {
            repo_id: target.repo_id,
            provider_full_name: target.provider_full_name,
            git_url: target.git_url,
            source_local_branch_name: repo.source_branch_name.clone(),
            source_remote_branch: target.source_remote_branch,
            target_remote_branch: target.target_remote_branch,
            source_head: repo.source_head.clone(),
            target_head: repo.target_head.clone(),
        }))
    }

    async fn load_provider_rows(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<Vec<ProviderRepoRow>, G5Error> {
        Ok(sqlx::query_as::<_, ProviderRepoRow>(
            r#"
            SELECT p.id,
                   p.org_id,
                   p.pull_request_id,
                   p.repo_id,
                   p.provider,
                   p.provider_repo_full_name,
                   r.git_url,
                   p.source_remote_branch,
                   p.target_remote_branch,
                   p.provider_pr_number,
                   p.provider_status,
                   p.source_head_sha, p.merge_attempt_id, p.merge_execution_id,
                   p.merge_target_head_sha, p.merge_pending, p.merge_accepted,
                   p.provider_completed_pr_number
              FROM workspace_pull_request_provider_repos p
              JOIN repos r
                ON r.id = p.repo_id
               AND r.org_id = p.org_id
             WHERE p.pull_request_id = $1
            "#,
        )
        .bind(pull_request_id)
        .fetch_all(db.conn())
        .await?)
    }

    #[allow(clippy::too_many_arguments)]
    async fn resolve_publication_auth(
        &self,
        db: &mut ScopedDb,
        in_scope: bool,
        org: Uuid,
        repo: Uuid,
        actor: Uuid,
        url: &str,
        operation: RepositoryOperation,
    ) -> Result<GitAuth, G5Error> {
        if in_scope {
            self.credentials
                .resolve_in_scope(db, org, repo, actor, url, operation)
                .await
        } else {
            self.credentials
                .resolve(org, repo, actor, url, operation)
                .await
        }
    }

    async fn resolve_github_auth(
        &self,
        org_id: Uuid,
        repo_id: Uuid,
        actor_id: Uuid,
        url: &str,
        operation: RepositoryOperation,
    ) -> Result<GitAuth, String> {
        self.credentials
            .resolve(org_id, repo_id, actor_id, url, operation)
            .await
            .map_err(|error| error.to_string())
    }

    fn provider_endpoints(&self) -> Result<ProviderEndpoints, ProviderError> {
        ProviderEndpoints::new(
            g5_scm::ScmProvider::GitHub,
            &self.github_api_base_url,
            &self.github_web_base_url,
        )
    }
    fn repository_protocol(
        &self,
        token: &str,
        name: &str,
        capability: RepositoryCapability,
    ) -> Result<RepositoryClient, ProviderError> {
        let providers = self.providers.as_ref().map_err(Clone::clone)?;
        Ok(providers.repository(RepositoryGrant::new(
            self.provider_endpoints()?,
            RepositoryIdentity::Name(name.to_owned()),
            capability,
            token.to_owned(),
        )?))
    }
    async fn find_open_pull_request(
        &self,
        token: &str,
        name: &str,
        source: &str,
        target: &str,
    ) -> Result<Option<GitHubPull>, ProviderHttpError> {
        self.repository_protocol(token, name, RepositoryCapability::PullRequestRead)?
            .find_open_pull_request(source, target)
            .await
            .map_err(Into::into)
    }
    async fn update_pull_request(
        &self,
        token: &str,
        name: &str,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<GitHubPull, ProviderHttpError> {
        self.repository_protocol(token, name, RepositoryCapability::PullRequestWrite)?
            .update_pull_request(number, title, body)
            .await
            .map_err(Into::into)
    }
    async fn create_pull_request(
        &self,
        token: &str,
        candidate: &ProviderCandidate,
        payload: &ProviderPrPayload,
    ) -> Result<GitHubPull, ProviderHttpError> {
        self.repository_protocol(
            token,
            &candidate.provider_full_name,
            RepositoryCapability::PullRequestWrite,
        )?
        .create_pull_request(CreatePullRequest {
            title: &payload.title,
            head: &candidate.source_remote_branch,
            base: &candidate.target_remote_branch,
            body: &payload.body,
        })
        .await
        .map_err(Into::into)
    }

    /// Spec D11: the provider PR payload derives from the candidate repo's
    /// own snapshot commit range; a repo with no usable commits (or a failed
    /// derivation) falls back to the legacy workspace-name payload — never to
    /// the stored local title or another repo's derivation. The git work runs
    /// on a blocking task, and the complete rendered body is guarded against
    /// the D11 ceiling so the provenance line always fits.
    async fn build_provider_pr_payload(
        &self,
        context: &PullRequestProviderContext,
        candidate: &ProviderCandidate,
    ) -> ProviderPrPayload {
        let repo_storage = context.repos.clone();
        let org_id = context.org_id;
        let repo_id = candidate.repo_id;
        let target_head = candidate.target_head.clone();
        let source_head = candidate.source_head.clone();
        let derived = tokio::task::spawn_blocking(move || {
            let repo_storage = repo_storage.ok_or_else(|| G5Error::Internal {
                message: "Pull request context has no repository scope".to_string(),
            })?;
            derive_repo_pr_text(&repo_storage, org_id, repo_id, &target_head, &source_head)
        })
        .await;
        let provenance = provider_pr_provenance(context);
        let mut payload = match derived {
            Ok(derived_result) => {
                // Spec D16: the workspace PR-body Project Doc (read once per
                // run, when usable) replaces the derived body content for
                // every candidate repo; the title keeps D10/D11 semantics —
                // derived when usable commits exist, legacy otherwise,
                // including on hard derivation failure.
                let pr_body = context.pr_body_doc.clone();
                let derived_text = match derived_result {
                    Ok(text) => text,
                    Err(err) => {
                        tracing::debug!(
                            repo_id = %candidate.repo_id,
                            error = %err,
                            "provider PR payload derivation failed; using legacy title"
                        );
                        None
                    }
                };
                compose_provider_pr_payload(context, derived_text, pr_body, &provenance)
            }
            Err(err) => {
                tracing::warn!(
                    repo_id = %candidate.repo_id,
                    error = %err,
                    "provider PR payload derivation task failed; using legacy payload"
                );
                legacy_provider_pr_payload(context)
            }
        };
        payload = apply_description_override(
            payload,
            context.description_override.as_deref(),
            &provenance,
        );
        payload
    }

    /// A bounded, cancellable provider read. The automatic worker never blocks
    /// a Tokio thread in libgit2 or leaves abandoned Git subprocesses behind.
    async fn published_remote_head(
        &self,
        candidate: &ProviderCandidate,
        branch: &str,
        auth: &GitAuth,
    ) -> Result<String, G5Error> {
        let token = token_from_auth(auth).ok_or_else(|| G5Error::Conflict {
            reason: "GitHub ContentsRead credential is required to observe a published branch"
                .into(),
        })?;
        self.repository_protocol(
            &token,
            &candidate.provider_full_name,
            RepositoryCapability::ContentsRead,
        )?
        .published_branch_head(branch)
        .await
        .map_err(G5Error::from)
    }
    async fn get_pull_request(
        &self,
        token: &str,
        name: &str,
        number: i64,
    ) -> Result<GitHubPull, ProviderHttpError> {
        self.repository_protocol(token, name, RepositoryCapability::PullRequestRead)?
            .pull_request(number)
            .await
            .map_err(Into::into)
    }
    async fn merge_pull_request(
        &self,
        token: &str,
        name: &str,
        number: i64,
        expected: &str,
    ) -> Result<GitHubMergeResponse, ProviderHttpError> {
        self.repository_protocol(token, name, RepositoryCapability::PullRequestMerge)?
            .merge_pull_request(number, expected)
            .await
            .map_err(Into::into)
    }

    async fn record_provider_status(
        &self,
        db: &mut ScopedDb,
        context: &PullRequestProviderContext,
        candidate: &ProviderCandidate,
        record: ProviderStatusRecord<'_>,
    ) -> Result<(), G5Error> {
        let pull = record.pull;
        let provider_pr_number = pull.map(|p| p.number);
        let provider_pr_node_id = pull.and_then(|p| p.node_id.clone());
        let provider_pr_url = pull.map(|p| p.html_url.clone());
        let provider_head_sha = pull.map(|p| p.head.sha.clone());
        let provider_base_sha = pull.map(|p| p.base.sha.clone());
        sqlx::query(
            r#"
            INSERT INTO workspace_pull_request_provider_repos (
                org_id, pull_request_id, repo_id, provider, provider_repo_full_name,
                source_remote_branch, target_remote_branch, provider_pr_number,
                provider_pr_node_id, provider_pr_url, provider_status,
                source_head_sha, target_head_sha, provider_head_sha,
                provider_base_sha, last_error, last_synced_at, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, 'github', $4,
                $5, $6, $7,
                $8, $9, $10,
                $11, $12, $13,
                $14, $15, now(), now(), now()
            )
            ON CONFLICT (org_id, pull_request_id, repo_id, provider)
            DO UPDATE SET
                provider_repo_full_name = EXCLUDED.provider_repo_full_name,
                source_remote_branch = EXCLUDED.source_remote_branch,
                target_remote_branch = EXCLUDED.target_remote_branch,
                provider_pr_number = COALESCE(EXCLUDED.provider_pr_number, workspace_pull_request_provider_repos.provider_pr_number),
                provider_pr_node_id = COALESCE(EXCLUDED.provider_pr_node_id, workspace_pull_request_provider_repos.provider_pr_node_id),
                provider_pr_url = COALESCE(EXCLUDED.provider_pr_url, workspace_pull_request_provider_repos.provider_pr_url),
                provider_status = EXCLUDED.provider_status,
                merge_attempt_id = CASE WHEN EXCLUDED.provider_pr_number IS NOT NULL AND workspace_pull_request_provider_repos.provider_pr_number IS DISTINCT FROM EXCLUDED.provider_pr_number THEN NULL ELSE workspace_pull_request_provider_repos.merge_attempt_id END,
                merge_execution_id = CASE WHEN EXCLUDED.provider_pr_number IS NOT NULL AND workspace_pull_request_provider_repos.provider_pr_number IS DISTINCT FROM EXCLUDED.provider_pr_number THEN NULL ELSE workspace_pull_request_provider_repos.merge_execution_id END,
                merge_target_head_sha = CASE WHEN EXCLUDED.provider_pr_number IS NOT NULL AND workspace_pull_request_provider_repos.provider_pr_number IS DISTINCT FROM EXCLUDED.provider_pr_number THEN NULL ELSE workspace_pull_request_provider_repos.merge_target_head_sha END,
                merge_uncertain = CASE WHEN EXCLUDED.provider_pr_number IS NOT NULL AND workspace_pull_request_provider_repos.provider_pr_number IS DISTINCT FROM EXCLUDED.provider_pr_number THEN false ELSE workspace_pull_request_provider_repos.merge_uncertain END,
                merge_accepted = CASE WHEN EXCLUDED.provider_pr_number IS NOT NULL AND workspace_pull_request_provider_repos.provider_pr_number IS DISTINCT FROM EXCLUDED.provider_pr_number THEN false ELSE workspace_pull_request_provider_repos.merge_accepted END,
                source_head_sha = CASE WHEN EXCLUDED.provider_status IN ('failed','action_required')
                    THEN workspace_pull_request_provider_repos.source_head_sha ELSE EXCLUDED.source_head_sha END,
                target_head_sha = CASE WHEN EXCLUDED.provider_status IN ('failed','action_required')
                    THEN workspace_pull_request_provider_repos.target_head_sha ELSE EXCLUDED.target_head_sha END,
                provider_head_sha = COALESCE(EXCLUDED.provider_head_sha, workspace_pull_request_provider_repos.provider_head_sha),
                provider_base_sha = COALESCE(EXCLUDED.provider_base_sha, workspace_pull_request_provider_repos.provider_base_sha),
                last_error = EXCLUDED.last_error,
                last_synced_at = EXCLUDED.last_synced_at,
                provider_completed_pr_number = CASE WHEN EXCLUDED.provider_status = 'open'
                    THEN NULL ELSE workspace_pull_request_provider_repos.provider_completed_pr_number END,
                updated_at = now()
            "#,
        )
        .bind(context.org_id)
        .bind(context.id)
        .bind(candidate.repo_id)
        .bind(&candidate.provider_full_name)
        .bind(&candidate.source_remote_branch)
        .bind(&candidate.target_remote_branch)
        .bind(provider_pr_number)
        .bind(provider_pr_node_id)
        .bind(provider_pr_url)
        .bind(record.status.as_str())
        .bind(&candidate.source_head)
        .bind(&candidate.target_head)
        .bind(provider_head_sha)
        .bind(provider_base_sha)
        .bind(record.last_error.as_deref().map(truncate_provider_message))
        .execute(db.conn())
        .await?;
        self.insert_provider_event(
            db,
            context,
            record.event_type,
            record.actor_id,
            record.last_error,
        )
        .await
    }

    async fn update_existing_provider_status(
        &self,
        db: &mut ScopedDb,
        row: &ProviderRepoRow,
        record: ExistingProviderStatusRecord<'_>,
    ) -> Result<(), G5Error> {
        let pull = record.pull;
        let provider_head_sha = pull.map(|p| p.head.sha.clone()).or(record.merge_sha);
        let provider_base_sha = pull.map(|p| p.base.sha.clone());
        let updated = sqlx::query(
            r#"
            UPDATE workspace_pull_request_provider_repos
               SET provider_status = $1,
                   target_head_sha = CASE WHEN $1='merged' AND merge_pending THEN merge_target_head_sha ELSE target_head_sha END,
                   merge_accepted = merge_accepted OR ($1='merged' AND merge_pending),
                   merge_pending = CASE WHEN $1='merged' THEN false ELSE merge_pending END,
                   provider_pr_number = COALESCE(provider_pr_number, $2),
                   provider_pr_node_id = COALESCE($3, provider_pr_node_id),
                   provider_pr_url = COALESCE($4, provider_pr_url),
                   provider_head_sha = COALESCE($5, provider_head_sha),
                   provider_base_sha = COALESCE($6, provider_base_sha),
                   last_error = $7,
                   last_synced_at = now(),
                   updated_at = now()
             WHERE id = $8
               AND org_id = $9
               AND merge_attempt_id IS NOT DISTINCT FROM $10
               AND merge_execution_id IS NOT DISTINCT FROM $11
               AND ($1='merged' OR merge_accepted=$12)
            "#,
        )
        .bind(record.status.as_str())
        .bind(pull.map(|p| p.number))
        .bind(pull.and_then(|p| p.node_id.clone()))
        .bind(pull.map(|p| p.html_url.clone()))
        .bind(provider_head_sha)
        .bind(provider_base_sha)
        .bind(record.last_error.as_deref().map(truncate_provider_message))
        .bind(row.id)
        .bind(row.org_id)
        .bind(row.merge_attempt_id)
        .bind(row.merge_execution_id)
        .bind(row.merge_accepted)
        .execute(db.conn())
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(G5Error::Conflict {
                reason: STALE_PROVIDER_MERGE_COMPLETION.into(),
            });
        }
        self.insert_provider_event_for_row(
            db,
            row,
            record.event_type,
            record.actor_id,
            record.last_error,
        )
        .await
    }

    async fn insert_provider_event(
        &self,
        db: &mut ScopedDb,
        context: &PullRequestProviderContext,
        event_type: ProviderEventType,
        actor_id: Uuid,
        message: Option<String>,
    ) -> Result<(), G5Error> {
        sqlx::query(
            r#"
            INSERT INTO workspace_pull_request_events (
                org_id, pull_request_id, event_type, actor_id, message
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(context.org_id)
        .bind(context.id)
        .bind(event_type.as_str())
        .bind(actor_id)
        .bind(message.as_deref().map(truncate_provider_message))
        .execute(db.conn())
        .await?;
        Ok(())
    }

    async fn insert_provider_event_for_row(
        &self,
        db: &mut ScopedDb,
        row: &ProviderRepoRow,
        event_type: ProviderEventType,
        actor_id: Uuid,
        message: Option<String>,
    ) -> Result<(), G5Error> {
        sqlx::query(
            r#"
            INSERT INTO workspace_pull_request_events (
                org_id, pull_request_id, event_type, actor_id, message
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(row.org_id)
        .bind(row.pull_request_id)
        .bind(event_type.as_str())
        .bind(actor_id)
        .bind(message.as_deref().map(truncate_provider_message))
        .execute(db.conn())
        .await?;
        Ok(())
    }

    async fn recompute_aggregate_provider_status(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
    ) -> Result<(), G5Error> {
        let statuses: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT provider_status
              FROM workspace_pull_request_provider_repos
             WHERE pull_request_id = $1
            "#,
        )
        .bind(pull_request_id)
        .fetch_all(db.conn())
        .await?;
        let aggregate = aggregate_provider_status(&statuses);
        self.set_aggregate_provider_status(db, pull_request_id, aggregate)
            .await
    }

    async fn set_aggregate_provider_status(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        provider_status: &str,
    ) -> Result<(), G5Error> {
        sqlx::query(
            r#"
            UPDATE workspace_pull_requests
               SET provider_status = $1,
                   updated_at = now()
             WHERE id = $2
            "#,
        )
        .bind(provider_status)
        .bind(pull_request_id)
        .execute(db.conn())
        .await?;
        Ok(())
    }
}

/// Title/body sent to GitHub's create-PR API. `derived` marks a commit-derived
/// payload eligible for the one-shot legacy retry after a 422 rejection.
struct ProviderPrPayload {
    title: String,
    body: String,
    derived: bool,
}

fn provider_pr_provenance(context: &PullRequestProviderContext) -> String {
    let mut workspace_url = url::Url::parse(&context.external_url)
        .expect("server.external_url must be a valid absolute URL");
    workspace_url.set_path("/");
    workspace_url.set_query(None);
    workspace_url.set_fragment(None);
    workspace_url
        .path_segments_mut()
        .expect("server.external_url must be a hierarchical URL")
        .extend([
            context.source_org_slug.as_str(),
            context.source_workspace_name.as_str(),
        ]);
    format!("Created by G5 workspace {workspace_url}")
}

/// Spec D11: the one retry after GitHub rejects a derived payload with 422.
/// It re-applies the D16 body precedence (the workspace PR-body Project Doc
/// already read for this run, then the D15 description override on top)
/// without commit-derived text. The description read compares with it too.
fn legacy_retry_payload(context: &PullRequestProviderContext) -> ProviderPrPayload {
    let provenance = provider_pr_provenance(context);
    apply_description_override(
        compose_provider_pr_payload(context, None, context.pr_body_doc.clone(), &provenance),
        context.description_override.as_deref(),
        &provenance,
    )
}

/// Legacy workspace-name payload, reconstructed from the context workspace
/// names rather than the stored local title so per-repo independence holds
/// even when the local title was commit-derived.
fn legacy_provider_pr_payload(context: &PullRequestProviderContext) -> ProviderPrPayload {
    ProviderPrPayload {
        title: format!(
            "{} into {}",
            context.source_workspace_name, context.target_workspace_name
        ),
        body: provider_pr_provenance(context),
        derived: false,
    }
}

/// Spec D11/D16: compose one candidate repo's provider payload from its
/// derived commit text and the workspace PR-body Project Doc. Body precedence
/// (BEFORE the [D15] override, which the caller applies last): pr_body doc >
/// derived body > provenance-only/legacy. Title keeps D10/D11 semantics —
/// derived when usable commits exist, legacy otherwise. The ceiling guard
/// falls back to the full legacy payload rather than truncating silently.
fn compose_provider_pr_payload(
    context: &PullRequestProviderContext,
    derived_text: Option<DerivedPrText>,
    pr_body: Option<String>,
    provenance: &str,
) -> ProviderPrPayload {
    if derived_text.is_none() && pr_body.is_none() {
        return legacy_provider_pr_payload(context);
    }
    let content = pr_body
        .or_else(|| derived_text.as_ref().map(|text| text.body.clone()))
        .unwrap_or_default();
    let body = if content.is_empty() {
        provenance.to_string()
    } else {
        format!("{content}\n{provenance}")
    };
    if body.chars().count() > PR_PROVIDER_BODY_MAX_CHARS {
        tracing::warn!("provider PR body exceeded the size ceiling; using legacy payload");
        return legacy_provider_pr_payload(context);
    }
    let mut payload = legacy_provider_pr_payload(context);
    if let Some(text) = derived_text {
        payload.title = text.title;
    }
    payload.body = body;
    payload.derived = true;
    payload
}

/// Spec D11/D15: a stored user-authored description replaces the body content
/// for every candidate repo — titles stay per-repo — with the provenance line
/// still appended and the ceiling still enforced; an over-ceiling override
/// keeps the derived/legacy body instead of truncating silently.
fn apply_description_override(
    mut payload: ProviderPrPayload,
    description_override: Option<&str>,
    provenance: &str,
) -> ProviderPrPayload {
    let Some(override_text) = description_override
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        return payload;
    };
    let body = format!("{override_text}\n{provenance}");
    if body.chars().count() > PR_PROVIDER_BODY_MAX_CHARS {
        tracing::warn!(
            "stored PR description override exceeded the size ceiling; keeping derived body"
        );
    } else {
        payload.body = body;
    }
    payload
}

fn parse_preflight(value: &serde_json::Value) -> Result<MergePreflightResponse, G5Error> {
    serde_json::from_value(value.clone()).map_err(|err| G5Error::Internal {
        message: format!("failed to parse workspace PR preflight evidence: {err}"),
    })
}

fn repo_needs_provider_pr(
    repo: &crate::services::workspace_merge_service::MergePreflightRepoResult,
) -> bool {
    super::pull_request_adapters::changes_member(repo)
}

fn token_from_auth(auth: &GitAuth) -> Option<String> {
    match auth {
        GitAuth::ScmToken { token, provider } if provider.is_github() => Some(token.clone()),
        GitAuth::None | GitAuth::SshKey { .. } => None,
        _ => None,
    }
}

fn provider_error_to_g5(err: ProviderHttpError) -> G5Error {
    G5Error::Conflict {
        reason: err.message,
    }
}

/// Why GitHub cannot rebase-merge the PR, and what to do, in the Prepare to
/// Ship dialog's words. `provider_message` is GitHub's refusal when the merge
/// endpoint itself refused; it may have another cause, so it stays visible.
fn unrebaseable_reason(
    number: i64,
    branch: &str,
    target_workspace_name: &str,
    history: Option<&SourceHistory>,
    provider_message: Option<&str>,
) -> String {
    let get_latest = format!("Get the latest changes from {target_workspace_name}");
    let then = "then merge again once the PR shows the new commits (with Auto-push off, publish them first with Update PR or g5d pr update)";
    let mut reason = match provider_message {
        Some(message) => format!(
            "GitHub refused to merge PR #{number} into {branch} (GitHub: {})",
            truncate_provider_message(&message.chars().take(120).collect::<String>())
                .trim_end_matches('.')
        ),
        None => format!("GitHub can't rebase PR #{number} onto {branch}"),
    };
    match history {
        Some(history) if !history.merges.is_empty() => {
            let merge = &history.merges[0];
            let more = if history.merges.len() > 1 {
                " and other merge commits"
            } else {
                ""
            };
            reason.push_str(&format!(
                ": the branch contains merge commit {} (\"{}\"){more}, which GitHub can't rebase. ",
                &merge.sha[..merge.sha.len().min(7)],
                truncate_provider_message(&merge.summary.chars().take(60).collect::<String>()),
            ));
            if history.contains_target {
                // The workspace already has the target, so the button finds
                // nothing to replay and keeps the merge.
                reason.push_str(&format!(
                    "\"{get_latest}\" can't remove it because this workspace already has them; ask the workspace's agent to rebase its branch onto {target_workspace_name} without merge commits, {then}."
                ));
            } else {
                reason.push_str(&format!(
                    "Click \"{get_latest}\" to replay this workspace's commits onto {target_workspace_name} without the merge, {then}."
                ));
            }
        }
        Some(history) if provider_message.is_some() && history.is_linear_on_target() => {
            reason.push_str(
                ". Nothing changed on GitHub or in G5; resolve GitHub's reason, then merge again.",
            )
        }
        _ if provider_message.is_some() => reason.push_str(&format!(
            ". Nothing changed on GitHub or in G5. If GitHub can't rebase the branch, click \"{get_latest}\" and resolve any conflicts, {then}."
        )),
        _ => reason.push_str(&format!(
            ": its commits conflict when replayed onto {branch}. Click \"{get_latest}\" and resolve the conflicts, {then}."
        )),
    }
    reason
}

fn aggregate_provider_status(statuses: &[String]) -> &'static str {
    if statuses.is_empty() {
        return "local_only";
    }
    let all = |value: &str| statuses.iter().all(|status| status == value);
    let any = |value: &str| statuses.iter().any(|status| status == value);
    let has_open = any("open");
    let has_merged = any("merged");
    let has_blocking = any("failed") || any("action_required");
    if all("merged") {
        "merged"
    } else if all("open") {
        "open"
    } else if (has_merged && has_open) || (has_blocking && (has_open || has_merged)) {
        "partial"
    } else if any("action_required") {
        "action_required"
    } else if any("failed") {
        "failed"
    } else {
        "pending"
    }
}

fn truncate_provider_message(message: &str) -> String {
    const MAX: usize = 500;
    let mut sanitized = String::new();
    for token in message.split_whitespace() {
        let redacted = if token.contains("://") || token.contains('@') {
            "[redacted]"
        } else {
            token
        };
        if !sanitized.is_empty() {
            sanitized.push(' ');
        }
        sanitized.push_str(redacted);
    }
    let mut truncated = sanitized.chars().take(MAX).collect::<String>();
    if sanitized.chars().count() > MAX {
        truncated.push_str("...");
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_provider_status, apply_description_override, compose_provider_pr_payload,
        legacy_provider_pr_payload, provider_pr_provenance, truncate_provider_message,
        unrebaseable_reason, ProviderPrPayload, PullRequestProviderContext, SourceHistory,
        WorkflowRequiredGate,
    };
    use crate::services::coordinated_repo_publication::{
        DerivedPrText, PR_PROVIDER_BODY_MAX_CHARS,
    };
    use crate::services::workspace_merge_service::{
        MergePreflightRepoOutcome, MergePreflightRepoResult,
    };
    use uuid::Uuid;

    fn statuses(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn preflight_repo(
        outcome: MergePreflightRepoOutcome,
        actual_source_head: Option<&str>,
        actual_target_head: Option<&str>,
    ) -> MergePreflightRepoResult {
        MergePreflightRepoResult {
            repo_id: Uuid::nil(),
            source_workspace_id: Uuid::nil(),
            target_workspace_id: Uuid::nil(),
            source_branch_name: "source".to_string(),
            target_branch_name: "target".to_string(),
            source_head: "source-head".to_string(),
            target_head: "target-head".to_string(),
            actual_source_head: actual_source_head.map(str::to_string),
            actual_target_head: actual_target_head.map(str::to_string),
            outcome,
            conflict_paths: Vec::new(),
            message: None,
        }
    }

    #[test]
    fn classify_workflow_required_gate_ignores_non_workflow_outcomes() {
        let repo = preflight_repo(
            MergePreflightRepoOutcome::CleanMerge,
            Some("source-head"),
            Some("moved-target-head"),
        );
        assert_eq!(
            repo.workflow_required_gate(),
            WorkflowRequiredGate::NotBlocked
        );
    }

    #[test]
    fn classify_workflow_required_gate_treats_pure_target_drift_as_advisory() {
        let repo = preflight_repo(
            MergePreflightRepoOutcome::WorkflowRequired,
            Some("source-head"),
            Some("moved-target-head"),
        );
        assert_eq!(
            repo.workflow_required_gate(),
            WorkflowRequiredGate::TargetOnlyDrift
        );
    }

    #[test]
    fn classify_workflow_required_gate_blocks_on_source_head_mismatch() {
        let repo = preflight_repo(
            MergePreflightRepoOutcome::WorkflowRequired,
            Some("moved-source-head"),
            Some("moved-target-head"),
        );
        assert_eq!(
            repo.workflow_required_gate(),
            WorkflowRequiredGate::SourceBlocked
        );
    }

    #[test]
    fn classify_workflow_required_gate_blocks_when_actual_heads_are_unreadable() {
        // Unresolved refs / git-analysis failures leave the actual heads
        // unpopulated: never advisory, always source-blocked.
        let repo = preflight_repo(MergePreflightRepoOutcome::WorkflowRequired, None, None);
        assert_eq!(
            repo.workflow_required_gate(),
            WorkflowRequiredGate::SourceBlocked
        );

        let target_missing = preflight_repo(
            MergePreflightRepoOutcome::WorkflowRequired,
            Some("source-head"),
            None,
        );
        assert_eq!(
            target_missing.workflow_required_gate(),
            WorkflowRequiredGate::SourceBlocked
        );
    }

    #[test]
    fn classify_workflow_required_gate_blocks_when_target_snapshot_is_current() {
        // WorkflowRequired evidence with both heads matching the snapshots
        // has no target-drift explanation; stay conservative and block.
        let repo = preflight_repo(
            MergePreflightRepoOutcome::WorkflowRequired,
            Some("source-head"),
            Some("target-head"),
        );
        assert_eq!(
            repo.workflow_required_gate(),
            WorkflowRequiredGate::SourceBlocked
        );
    }

    #[test]
    fn aggregate_provider_status_surfaces_partial_provider_outcomes() {
        assert_eq!(
            aggregate_provider_status(&statuses(&["open", "action_required"])),
            "partial"
        );
        assert_eq!(
            aggregate_provider_status(&statuses(&["failed", "action_required"])),
            "action_required"
        );
        assert_eq!(
            aggregate_provider_status(&statuses(&["merged", "action_required"])),
            "partial"
        );
        assert_eq!(
            aggregate_provider_status(&statuses(&["failed", "open"])),
            "partial"
        );
        assert_eq!(
            aggregate_provider_status(&statuses(&["failed", "merged"])),
            "partial"
        );
        assert_eq!(
            aggregate_provider_status(&statuses(&["merged", "open"])),
            "partial"
        );
    }

    fn derived_payload() -> ProviderPrPayload {
        ProviderPrPayload {
            title: "feat: derived title".to_string(),
            body: "- `derived bullet`\nprovenance-line".to_string(),
            derived: true,
        }
    }

    fn provider_context() -> PullRequestProviderContext {
        PullRequestProviderContext {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            status: "open".to_string(),
            source_workspace_id: Uuid::nil(),
            source_workspace_name: "src-ws".to_string(),
            source_docs_repo_id: None,
            source_org_slug: "test-org".to_string(),
            target_workspace_name: "tgt-ws".to_string(),
            external_url: "http://localhost:8080".to_string(),
            preflight: serde_json::json!({}),
            description_override: None,
            title: "Example PR".into(),
            pr_body_doc: None,
            repos: None,
        }
    }

    fn derived_text() -> DerivedPrText {
        DerivedPrText {
            title: "feat: derived title".to_string(),
            body: "- `derived bullet`".to_string(),
        }
    }

    #[test]
    fn provider_pr_provenance_uses_public_workspace_url() {
        let mut context = provider_context();
        context.source_org_slug = "internal-g5-devs".to_string();
        context.source_workspace_name = "Davide PR/body".to_string();

        context.external_url = "https://dev.g5labs.ai".to_string();
        assert_eq!(
            provider_pr_provenance(&context),
            "Created by G5 workspace https://dev.g5labs.ai/internal-g5-devs/Davide%20PR%2Fbody"
        );
        context.external_url = "https://g5labs.ai/ignored/base?query=yes#fragment".to_string();
        assert_eq!(
            provider_pr_provenance(&context),
            "Created by G5 workspace https://g5labs.ai/internal-g5-devs/Davide%20PR%2Fbody"
        );
    }

    #[test]
    fn compose_payload_pr_body_replaces_derived_body_keeps_title() {
        // Spec D16: pr_body > derived body; the derived title stays.
        let payload = compose_provider_pr_payload(
            &provider_context(),
            Some(derived_text()),
            Some("Authored PR body.".to_string()),
            "provenance-line",
        );
        assert_eq!(payload.title, "feat: derived title");
        assert_eq!(payload.body, "Authored PR body.\nprovenance-line");
        assert!(payload.derived);
    }

    #[test]
    fn compose_payload_pr_body_without_derived_uses_legacy_title() {
        // Spec D16: no usable commits (or hard derivation failure) with a
        // usable pr_body → legacy title, pr_body body.
        let payload = compose_provider_pr_payload(
            &provider_context(),
            None,
            Some("Authored PR body.".to_string()),
            "provenance-line",
        );
        assert_eq!(payload.title, "src-ws into tgt-ws");
        assert_eq!(payload.body, "Authored PR body.\nprovenance-line");
        assert!(payload.derived);
    }

    #[test]
    fn compose_payload_without_sources_is_exact_legacy() {
        let payload =
            compose_provider_pr_payload(&provider_context(), None, None, "provenance-line");
        let legacy = legacy_provider_pr_payload(&provider_context());
        assert_eq!(payload.title, legacy.title);
        assert_eq!(payload.body, legacy.body);
        assert!(!payload.derived);
    }

    #[test]
    fn compose_payload_derived_only_matches_prior_behavior() {
        let payload = compose_provider_pr_payload(
            &provider_context(),
            Some(derived_text()),
            None,
            "provenance-line",
        );
        assert_eq!(payload.title, "feat: derived title");
        assert_eq!(payload.body, "- `derived bullet`\nprovenance-line");
        assert!(payload.derived);

        // Empty derived body → provenance-only body, derived title kept.
        let empty_body = compose_provider_pr_payload(
            &provider_context(),
            Some(DerivedPrText {
                title: "feat: derived title".to_string(),
                body: String::new(),
            }),
            None,
            "provenance-line",
        );
        assert_eq!(empty_body.body, "provenance-line");
    }

    #[test]
    fn compose_payload_over_ceiling_falls_back_to_legacy() {
        let payload = compose_provider_pr_payload(
            &provider_context(),
            Some(derived_text()),
            Some("x".repeat(PR_PROVIDER_BODY_MAX_CHARS)),
            "provenance-line",
        );
        assert_eq!(
            payload.body,
            legacy_provider_pr_payload(&provider_context()).body
        );
        assert!(!payload.derived);
    }

    #[test]
    fn description_override_still_wins_over_pr_body() {
        // Spec D16 precedence: override > pr_body > derived.
        let composed = compose_provider_pr_payload(
            &provider_context(),
            Some(derived_text()),
            Some("Authored PR body.".to_string()),
            "provenance-line",
        );
        let payload =
            apply_description_override(composed, Some("User override."), "provenance-line");
        assert_eq!(payload.body, "User override.\nprovenance-line");
        assert_eq!(payload.title, "feat: derived title");
    }

    #[test]
    fn apply_description_override_replaces_body_and_keeps_title() {
        let payload = apply_description_override(
            derived_payload(),
            Some("  User-authored summary.  "),
            "provenance-line",
        );
        assert_eq!(payload.title, "feat: derived title");
        assert_eq!(payload.body, "User-authored summary.\nprovenance-line");
        assert!(payload.derived);
    }

    #[test]
    fn apply_description_override_blank_or_absent_keeps_derived_body() {
        let untouched = apply_description_override(derived_payload(), None, "provenance-line");
        assert_eq!(untouched.body, derived_payload().body);

        let blank = apply_description_override(derived_payload(), Some("   "), "provenance-line");
        assert_eq!(blank.body, derived_payload().body);
    }

    #[test]
    fn apply_description_override_over_ceiling_keeps_derived_body() {
        let oversized = "x".repeat(PR_PROVIDER_BODY_MAX_CHARS);
        let payload =
            apply_description_override(derived_payload(), Some(&oversized), "provenance-line");
        assert_eq!(payload.body, derived_payload().body);
    }

    fn history(merges: &[(&str, &str)], contains_target: bool) -> SourceHistory {
        SourceHistory {
            merges: merges
                .iter()
                .map(|(sha, summary)| g5_scm::CommitSummary {
                    sha: sha.to_string(),
                    summary: summary.to_string(),
                    author_name: String::new(),
                    committed_at: 0,
                })
                .collect(),
            contains_target,
        }
    }

    #[test]
    fn unrebaseable_reason_names_the_merge_and_a_fix_that_applies() {
        let sha = "1ba8106".repeat(6);
        let merge = [(sha.as_str(), "Merge origin/main into issue-558")];
        // Behind Base: the button replays the commits and drops the merge.
        assert_eq!(
            unrebaseable_reason(564, "main", "Base", Some(&history(&merge, false)), None),
            "GitHub can't rebase PR #564 onto main: the branch contains merge commit 1ba8106 (\"Merge origin/main into issue-558\"), which GitHub can't rebase. Click \"Get the latest changes from Base\" to replay this workspace's commits onto Base without the merge, then merge again once the PR shows the new commits (with Auto-push off, publish them first with Update PR or g5d pr update)."
        );
        // Already up to date: the button would keep the merge.
        let two = [merge[0], (sha.as_str(), "Merge again")];
        let reason = unrebaseable_reason(564, "main", "Base", Some(&history(&two, true)), None);
        assert!(reason.contains("(\"Merge origin/main into issue-558\") and other merge commits"));
        assert!(reason.contains("\"Get the latest changes from Base\" can't remove it because this workspace already has them; ask the workspace's agent to rebase its branch onto Base without merge commits"));
        // A long summary stays bounded.
        let long = "x".repeat(200);
        let reason = unrebaseable_reason(
            1,
            "main",
            "Base",
            Some(&history(&[(sha.as_str(), long.as_str())], true)),
            None,
        );
        assert!(
            reason.contains(&format!("(\"{}\")", "x".repeat(60))),
            "{reason}"
        );
    }

    #[test]
    fn unrebaseable_reason_without_a_merge_commit() {
        assert_eq!(
            unrebaseable_reason(7, "main", "Base", None, None),
            "GitHub can't rebase PR #7 onto main: its commits conflict when replayed onto main. Click \"Get the latest changes from Base\" and resolve the conflicts, then merge again once the PR shows the new commits (with Auto-push off, publish them first with Update PR or g5d pr update)."
        );
        // GitHub's refusal stays visible; its cause may be something else.
        assert_eq!(
            unrebaseable_reason(7, "main", "Base", None, Some("Pull Request is not mergeable.")),
            "GitHub refused to merge PR #7 into main (GitHub: Pull Request is not mergeable). Nothing changed on GitHub or in G5. If GitHub can't rebase the branch, click \"Get the latest changes from Base\" and resolve any conflicts, then merge again once the PR shows the new commits (with Auto-push off, publish them first with Update PR or g5d pr update)."
        );
        // A linear branch on the target rebases; another cause refused it.
        assert_eq!(
            unrebaseable_reason(
                7,
                "main",
                "Base",
                Some(&history(&[], true)),
                Some("At least 1 approving review is required")
            ),
            "GitHub refused to merge PR #7 into main (GitHub: At least 1 approving review is required). Nothing changed on GitHub or in G5; resolve GitHub's reason, then merge again."
        );
    }

    #[test]
    fn truncate_provider_message_redacts_endpoints_and_truncates_on_chars() {
        let message = format!(
            "failed https://token@example.com/repo.git user@example.com {}",
            "å".repeat(600)
        );
        let truncated = truncate_provider_message(&message);

        assert!(!truncated.contains("https://"));
        assert!(!truncated.contains("user@example.com"));
        assert!(truncated.starts_with("failed [redacted] [redacted] "));
        assert!(truncated.ends_with("..."));
        assert_eq!(truncated.chars().count(), 503);
    }
}
