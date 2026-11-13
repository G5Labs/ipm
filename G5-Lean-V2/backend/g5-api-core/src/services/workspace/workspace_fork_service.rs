//! Durable exact-name workspace fork operation.
//!
//! `workspace_forks` is both the idempotency record and the active name/member
//! claim. The service freezes the complete source vector before touching Git,
//! creates an ordinary branch together with a server-only proof ref in each
//! repository, and publishes the stopped child only after every proof verifies.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};

use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use chrono::{DateTime, Utc};
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::types::{approval_review_workspace_branch, ApprovalReviewIdentity};
use g5_internal_api::workspace_flush::{FlushCompleteResponse, FlushRepoReceipt, SequencerState};
use g5_scm::{ForkCopyRefs, ForkRefsCreateOutcome, GitRepo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, PgPool};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, Semaphore};
use uuid::Uuid;

use crate::auth::RepositoryOperation;
use crate::errors::{AppError, G5Error};
use crate::services::external_issues::{self, IssueLinkTarget, LinkRole};
use crate::services::github_app::GithubAppClient;
use crate::services::org_admin_settings;
use crate::services::workspace::{base_reverification, workspace_owner_limit};
use crate::services::workspace_reader::read_committed_branch_heads;
use crate::services::workspace_runtime::{RuntimeRequest, RuntimeRpcError, WorkspaceRuntime};
use crate::services::workspace_service::CoTrackingWorkspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForkSource {
    /// The source's last complete flush: its committed heads and its
    /// dirty working tree (else the member branches' refs).
    Durable,
    /// A fresh flush of the source's running VM.
    Live,
    /// The member branches' refs in the mirror: the source's last push,
    /// nothing from a flush (no dirty tree, no unpushed commit). A review
    /// copy starts here: the reviewer reviews what reached the branch.
    Branch,
    /// What persisted fork rows and retried requests from before the
    /// agent-only audit was retired still say. Captured like `Branch`: the
    /// committed state it named is the branch's.
    #[serde(rename = "committed")]
    LegacyCommitted,
}

impl ForkSource {
    /// Whether the source's last complete flush receipt is the capture.
    fn captures_flush_receipt(self) -> bool {
        matches!(self, Self::Durable)
    }
}

#[derive(Debug, Clone)]
pub struct ForkWorkspaceRequest {
    pub name: String,
    pub description: Option<String>,
    pub idempotency_key: String,
    pub read_only: bool,
    pub merge_target_workspace_id: Option<Uuid>,
    pub chat_session_ids: Vec<Uuid>,
    pub source: ForkSource,
    pub interrupt: bool,
    /// An existing external issue to link to the new workspace.
    pub issue: Option<ForkIssueRequest>,
    /// Start a chat in the new workspace with this first message.
    pub start_chat: Option<ForkStartChatRequest>,
    /// `name` is a suggestion, not an exact choice: when it is taken, the
    /// fork takes the first free `name-2`, `name-3`, ….
    pub uniquify_name: bool,
    /// Start a Base re-verification run with this fork
    /// (g5.reverification.run). Only a fork of Base may carry one; the run is
    /// admitted in the fork's own admission transaction.
    pub reverification: Option<base_reverification::RunKind>,
}

/// The first message of a chat the fork starts in the new workspace.
#[derive(Debug, Clone)]
pub struct ForkStartChatRequest {
    pub content: String,
    /// The launch choices the requester made (provider, model, account,
    /// permission mode, effort), resolved for the new workspace when the chat
    /// is queued; `None` inherits the configured defaults.
    pub launch_preferences: Option<g5_contracts::types::AgentLaunchPreferencesV1>,
}

/// The chat a completed fork started: the new session, or why it did not
/// start (the workspace is ready either way).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkStartedChatOutcome {
    pub session_id: Option<Uuid>,
    pub error: Option<String>,
}

/// An existing issue named by URL or `owner/name#n`, resolved inside the
/// organization's issue domains when the fork is admitted.
#[derive(Debug, Clone)]
pub struct ForkIssueRequest {
    pub reference: String,
    pub role: LinkRole,
}

/// The association a completed fork created.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkIssueLinkOutcome {
    pub link_id: Uuid,
    pub display_key: String,
    pub canonical_url: String,
    /// The role the request asked for.
    pub requested_role: &'static str,
    /// The role the link has: `related` when another workspace claimed the
    /// issue as primary while the fork ran.
    pub role: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneWorkspaceResponse {
    pub workspace: CloneWorkspaceDetail,
    pub cloned_branches: i32,
    pub cloned_drafts: i32,
    pub carried_sessions: i32,
    pub issue_link: Option<ForkIssueLinkOutcome>,
    pub started_chat: Option<ForkStartedChatOutcome>,
}

/// The fork's frozen issue request.
#[derive(Debug, Clone, sqlx::FromRow)]
struct ForkIssueRow {
    issue_tracker_id: Uuid,
    issue_external_id: String,
    issue_display_key: String,
    issue_canonical_url: String,
    issue_locator: serde_json::Value,
    issue_role: String,
}

impl ForkIssueRow {
    fn target(&self) -> IssueLinkTarget {
        IssueLinkTarget {
            tracker_id: self.issue_tracker_id,
            external_issue_id: self.issue_external_id.clone(),
            display_key: self.issue_display_key.clone(),
            canonical_url: self.issue_canonical_url.clone(),
            locator: self.issue_locator.clone(),
        }
    }

    fn role(&self) -> LinkRole {
        LinkRole::parse(&self.issue_role).unwrap_or(LinkRole::Related)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneWorkspaceDetail {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_user_id: Uuid,
    pub created_by: Option<Uuid>,
    pub cloned_from_id: Option<Uuid>,
    pub read_only: bool,
    pub branches: Vec<ClonedBranchSummary>,
    pub tasks_completed: i64,
    pub total_tasks: i64,
    pub running_time: Option<String>,
    pub cost: f64,
    pub concurrent_tasks: i64,
    pub dependencies: Vec<Uuid>,
    pub last_updated: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonedBranchSummary {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub tracking_mode: String,
    pub head_commit: String,
    pub materialization_status: String,
    pub divergence: Option<i32>,
    pub behind_base: Option<i32>,
    pub has_draft: bool,
    pub dirty_file_count: i32,
    pub remote_branch: Option<String>,
    pub co_tracking_workspaces: Option<Vec<CoTrackingWorkspace>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ForkCopyManifest {
    source_flush_id: Uuid,
    target_workspace_id: Uuid,
    target_flush_id: Uuid,
    receipt: FlushRepoReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ForkRepoManifest {
    repo_id: Uuid,
    repo_name: String,
    git_url: String,
    provider: Option<String>,
    root_path: String,
    role: String,
    source_branch: String,
    source_oid: String,
    target_branch: String,
    target_description: Option<String>,
    target_read_only: bool,
    target_merge_target_workspace_id: Option<Uuid>,
    /// Workbench phase the fork starts in (g5.workbench.phases.state), resolved
    /// ONCE at admission — `be` for a Base source, else the source's phase at
    /// that moment — and frozen in the manifest so a resumed or replayed
    /// operation (g5.fork.replay) never re-reads the source. Derived
    /// server-side, so it is not part of the request digest. Manifests written
    /// before the field existed decode to `be`.
    #[serde(default = "default_target_phase")]
    target_phase: String,
    #[serde(default)]
    carry_session_ids: Vec<Uuid>,
    source_kind: ForkSource,
    live_interrupt: bool,
    fork_copy: Option<ForkCopyManifest>,
}

fn default_target_phase() -> String {
    "be".to_string()
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ForkOperationRow {
    id: Uuid,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Option<Uuid>,
    requested_by: Uuid,
    owner_user_id: Uuid,
    idempotency_key: String,
    request_digest: String,
    proposed_name: String,
    status: String,
    repo_manifest: serde_json::Value,
    source_flush_id: Option<Uuid>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SourceWorkspaceRow {
    id: Uuid,
    owner_user_id: Option<Uuid>,
    description: Option<String>,
    docs_repo_id: Option<Uuid>,
    push_mode: String,
    is_base: bool,
    /// Workbench phase a fork copies from its source
    /// (workbench `g5.workbench.phases.state`).
    phase: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SourceRepoRow {
    repo_id: Uuid,
    repo_name: String,
    git_url: String,
    provider: Option<String>,
    root_path: String,
    role: String,
    source_branch: Option<String>,
    source_oid: Option<String>,
}

#[derive(Clone)]
pub struct WorkspaceForkService {
    system_pool: PgPool,
    local_resume_gates: Arc<Mutex<HashMap<Uuid, Weak<LocalForkResumeGate>>>>,
    resume_slots: Arc<Semaphore>,
    resume_wait: Duration,
    repo_storage: TenantRepoStorage,
    repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    runtime: Arc<WorkspaceRuntime>,
    github_app: Option<Arc<GithubAppClient>>,
    /// The deployment's public address, for native issue references.
    issue_site_base_url: String,
    messenger: Arc<dyn ForkMessenger>,
    scope: Option<ScopedDbReopener>,
    approval_review_id: Option<Uuid>,
    operation_deadline: Option<DateTime<Utc>>,
}

/// The first chat a fork starts in the workspace it created.
#[derive(Debug, Clone)]
pub struct ForkChatStart {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub actor_user_id: Uuid,
    pub content: String,
    pub legacy_preferences: g5_contracts::types::AgentLaunchPreferencesV1,
}

/// What a completed fork asks of Agents in its publishing transaction: carry
/// the chosen chat sessions over, and start the requested chat. Agents
/// implements it and is installed at composition.
#[async_trait::async_trait]
pub trait ForkMessenger: Send + Sync {
    /// Copy `session_ids` from the source into the fork; returns how many.
    async fn copy_sessions(
        &self,
        conn: &mut PgConnection,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        session_ids: &[Uuid],
        created_by: Uuid,
    ) -> Result<i32, AppError>;

    /// Queue the chat's first message; returns the new session's id.
    async fn start_chat(&self, db: &mut ScopedDb, chat: ForkChatStart) -> Result<Uuid, AppError>;
}

const MAX_ACTIVE_FORK_RESUME_SESSIONS: usize = 4;
const FORK_RESUME_LOCK_WAIT: Duration = Duration::from_secs(180);
const FORK_PROVIDER_PREFLIGHT_WAIT: Duration = Duration::from_secs(30);

struct LocalForkResumeGate {
    lock: Arc<AsyncMutex<()>>,
    waiters: AtomicUsize,
}

struct LocalForkResumeWaiter(Arc<LocalForkResumeGate>);

impl Drop for LocalForkResumeWaiter {
    fn drop(&mut self) {
        self.0.waiters.fetch_sub(1, Ordering::AcqRel);
    }
}

impl LocalForkResumeGate {
    async fn acquire(self: &Arc<Self>) -> OwnedMutexGuard<()> {
        self.waiters.fetch_add(1, Ordering::AcqRel);
        let waiting = LocalForkResumeWaiter(Arc::clone(self));
        let guard = Arc::clone(&self.lock).lock_owned().await;
        drop(waiting);
        guard
    }
}

impl WorkspaceForkService {
    #[must_use]
    pub fn new(
        system_pool: PgPool,
        repo_storage: TenantRepoStorage,
        repository_credentials: Arc<crate::auth::RepositoryCredentials>,
        runtime: Arc<WorkspaceRuntime>,
        messenger: Arc<dyn ForkMessenger>,
    ) -> Self {
        Self {
            system_pool,
            local_resume_gates: Arc::new(Mutex::new(HashMap::new())),
            resume_slots: Arc::new(Semaphore::new(MAX_ACTIVE_FORK_RESUME_SESSIONS)),
            resume_wait: FORK_RESUME_LOCK_WAIT,
            repo_storage,
            repository_credentials,
            runtime,
            github_app: None,
            issue_site_base_url: String::new(),
            messenger,
            scope: None,
            approval_review_id: None,
            operation_deadline: None,
        }
    }

    /// Resolve fork issue references with the GitHub App. Without it, a
    /// fork that names an issue is refused as unavailable.
    #[must_use]
    pub fn with_github_app(mut self, github_app: Option<Arc<GithubAppClient>>) -> Self {
        self.github_app = github_app;
        self
    }

    /// The deployment's public address, from which native issue page URLs
    /// (a reference form a fork accepts) are built.
    #[must_use]
    pub fn with_issue_site(mut self, external_url: &str) -> Self {
        self.issue_site_base_url = external_url.to_string();
        self
    }

    /// Bind this service to the caller's already-authorized tenant scope.
    ///
    /// Forking deliberately spans short database phases separated by Git,
    /// provider, and live-runtime work. The reopener preserves the caller's
    /// RLS authority without retaining an idle transaction across those
    /// external effects.
    #[must_use]
    pub fn scoped(&self, scope: ScopedDbReopener) -> Self {
        let mut service = self.clone();
        service.scope = Some(scope);
        service
    }

    /// Keep a composing operation's query budget across reopened scopes and
    /// detached fork resumption. Ordinary fork callers set no such deadline.
    pub(crate) fn with_operation_deadline(mut self, deadline: DateTime<Utc>) -> Self {
        self.operation_deadline = Some(deadline);
        self.resume_wait = self.resume_wait.min(
            deadline
                .signed_duration_since(Utc::now())
                .to_std()
                .unwrap_or(Duration::from_millis(1)),
        );
        self
    }

    /// Platform-only specialization: the durable approval row is the authority
    /// for this exact source, requester, target and child identity. Public fork
    /// requests cannot select this mode or inherit its access exception.
    pub(crate) async fn fork_for_approval(
        &self,
        review_id: Uuid,
    ) -> Result<CloneWorkspaceResponse, AppError> {
        let mut db = self.open_db().await?;
        let (org_id, source, requester, target, child): (Uuid,Uuid,Uuid,Uuid,Uuid) = sqlx::query_as("SELECT org_id,source_workspace_id,requested_by,target_workspace_id,review_workspace_id FROM agent_approval_reviews WHERE id=$1 AND stage='capture'")
            .bind(review_id).fetch_optional(db.conn()).await?.ok_or_else(|| not_found("active formal review"))?;
        rollback_db(db).await?;
        self.pin_approval_base(review_id, false).await?;
        let mut service = self.clone();
        service.approval_review_id = Some(review_id);
        service
            .fork_workspace(
                org_id,
                source,
                requester,
                ForkWorkspaceRequest {
                    name: approval_review_workspace_branch(child),
                    description: Some("Temporary formal agent review".into()),
                    idempotency_key: ApprovalReviewIdentity::new(review_id).operation_key(),
                    read_only: false,
                    merge_target_workspace_id: Some(target),
                    chat_session_ids: vec![],
                    source: ForkSource::Branch,
                    interrupt: false,
                    issue: None,
                    start_chat: None,
                    uniquify_name: false,
                    reverification: None,
                },
            )
            .await
    }

    /// Base pins use checked fork proofs too: equality alone never authorizes
    /// adoption/deletion. The deterministic identities survive ambiguous ACKs.
    async fn pin_approval_base(&self, review_id: Uuid, remove: bool) -> Result<(), AppError> {
        let mut db = self.open_db().await?;
        let row: Option<(Uuid, Option<serde_json::Value>)> =
            sqlx::query_as("SELECT org_id,target_heads FROM agent_approval_reviews WHERE id=$1")
                .bind(review_id)
                .fetch_optional(db.conn())
                .await?;
        let Some((org, Some(heads))) = row else {
            return Ok(());
        };
        let heads: Vec<g5_contracts::types::MergeSourceHead> =
            serde_json::from_value(heads).map_err(internal_json)?;
        let repo_ids = heads
            .iter()
            .map(|h| {
                Uuid::parse_str(&h.repo_id)
                    .map_err(|_| validation("repoId", "Invalid pinned review repo"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        lock_repo_rows(db.conn(), org, &repo_ids).await?;
        for (head, repo_id) in heads.into_iter().zip(repo_ids) {
            let path = self.member_path(org, repo_id)?;
            let branch = ApprovalReviewIdentity::new(review_id).base_branch();
            let proof = ApprovalReviewIdentity::new(review_id).base_proof_id();
            tokio::task::spawn_blocking(move || {
                let repo = GitRepo::open(&path)?;
                if remove {
                    repo.delete_workspace_fork_refs_if_matches(&branch, proof, &head.head_commit)
                        .map(|_| ())
                } else {
                    repo.create_workspace_fork_refs(&branch, proof, &head.head_commit)
                        .map(|_| ())
                }
            })
            .await
            .map_err(|e| G5Error::Internal {
                message: e.to_string(),
            })??;
        }
        commit_db(db).await?;
        Ok(())
    }

    /// Permanent, resolution-scoped retention. Temporary fork cleanup never
    /// removes these private refs; frozen retries verify and reuse them.
    pub(crate) async fn pin_approval_evidence(
        &self,
        review_id: Uuid,
        resolution: &g5_contracts::types::ApprovalReviewResolutionV2,
    ) -> Result<(), AppError> {
        use g5_contracts::types::{approval_evidence_ref, ApprovalEvidenceSide};
        resolution
            .validate()
            .map_err(|error| validation("evidence", error.to_string()))?;
        let mut db = self.open_db().await?;
        let org: Uuid = sqlx::query_scalar(
            r#"
            SELECT org_id FROM agent_approval_reviews
            WHERE id = $1 AND stage = 'capture'
            "#,
        )
        .bind(review_id)
        .fetch_one(db.conn())
        .await?;
        let ids = resolution
            .source_heads
            .iter()
            .map(|head| {
                Uuid::parse_str(&head.repo_id)
                    .map_err(|error| validation("evidence", error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        lock_repo_rows(db.conn(), org, &ids).await?;
        for (repo, source) in ids.into_iter().zip(&resolution.source_heads) {
            let base = resolution
                .comparison_heads()
                .iter()
                .find(|head| head.repo_id == source.repo_id)
                .ok_or_else(|| validation("targetHeads", "Assigned repository vectors differ"))?;
            let path = self.member_path(org, repo)?;
            let mut refs = vec![
                (
                    approval_evidence_ref(
                        &resolution.resolution_id,
                        repo,
                        ApprovalEvidenceSide::Source,
                    )
                    .map_err(|error| validation("evidence", error.to_string()))?,
                    source.head_commit.clone(),
                ),
                (
                    approval_evidence_ref(
                        &resolution.resolution_id,
                        repo,
                        ApprovalEvidenceSide::Base,
                    )
                    .map_err(|error| validation("evidence", error.to_string()))?,
                    base.head_commit.clone(),
                ),
            ];
            if resolution.workspace_base_heads.is_some() {
                let parent = resolution
                    .target_heads
                    .iter()
                    .find(|head| head.repo_id == source.repo_id)
                    .ok_or_else(|| {
                        validation("targetHeads", "Assigned repository vectors differ")
                    })?;
                refs.push((
                    approval_evidence_ref(
                        &resolution.resolution_id,
                        repo,
                        ApprovalEvidenceSide::Parent,
                    )
                    .map_err(|error| validation("evidence", error.to_string()))?,
                    parent.head_commit.clone(),
                ));
            }
            tokio::task::spawn_blocking(move || {
                let repository = GitRepo::open(&path)?;
                for (name, oid) in refs {
                    repository.create_approval_evidence_ref_if_absent_or_equal(&name, &oid)?;
                }
                Ok::<(), G5Error>(())
            })
            .await
            .map_err(|error| G5Error::Internal {
                message: error.to_string(),
            })??;
        }
        commit_db(db).await
    }

    pub(crate) async fn retire_approval_fork(&self, review_id: Uuid) -> Result<(), AppError> {
        let mut db = self.open_db().await?;
        let (org,requester): (Uuid,Uuid)=sqlx::query_as("SELECT org_id,requested_by FROM agent_approval_reviews WHERE id=$1 AND stage IN ('effective','failed','cancelled')")
            .bind(review_id).fetch_optional(db.conn()).await?.ok_or_else(||not_found("retiring formal review"))?;
        rollback_db(db).await?;
        if let Some(operation) = self
            .load_operation(
                org,
                requester,
                &ApprovalReviewIdentity::new(review_id).operation_key(),
            )
            .await?
        {
            if operation.status == "pending" {
                self.begin_abort(
                    &operation,
                    &G5Error::Conflict {
                        reason: "formal review ended".into(),
                    }
                    .into(),
                )
                .await?;
            }
            if ["pending", "aborting"].contains(&operation.status.as_str()) {
                let operation = self
                    .load_operation(org, requester, &operation.idempotency_key)
                    .await?
                    .ok_or_else(|| not_found("formal fork"))?;
                self.cleanup_operation(&operation).await?;
            }
            if operation.status == "completed" {
                let mut db = self.open_db().await?;
                let deleted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id=$1 AND org_id=$2 AND agent_approval_review_id=$3 AND deleted_at IS NOT NULL)").bind(operation.id).bind(org).bind(review_id).fetch_one(db.conn()).await?;
                if !deleted {
                    return Err(validation(
                        "cleanup",
                        "formal review workspace is not deleted",
                    ));
                }
                let manifest = parse_manifest(&operation)?;
                lock_repo_rows(
                    db.conn(),
                    org,
                    &manifest.iter().map(|m| m.repo_id).collect::<Vec<_>>(),
                )
                .await?;
                for member in manifest {
                    let path = self.member_path(org, member.repo_id)?;
                    tokio::task::spawn_blocking(move || {
                        GitRepo::open(&path)?.retire_workspace_fork_proof(
                            &member.target_branch,
                            operation.id,
                            &member.source_oid,
                        )
                    })
                    .await
                    .map_err(|e| G5Error::Internal {
                        message: e.to_string(),
                    })??;
                }
                commit_db(db).await?;
            }
        }
        self.pin_approval_base(review_id, true).await
    }

    /// The source vector is derived from the checked fork manifest, including
    /// original member identities. Never sample source refs a second time.
    pub(crate) async fn approval_source_heads(
        &self,
        review_id: Uuid,
    ) -> Result<
        (
            Vec<g5_contracts::types::MergeSourceHead>,
            std::collections::BTreeMap<String, String>,
        ),
        AppError,
    > {
        let mut db = self.open_db().await?;
        let (source, manifest): (Uuid,serde_json::Value) = sqlx::query_as("SELECT r.source_workspace_id,f.repo_manifest FROM agent_approval_reviews r JOIN workspace_forks f ON f.id=r.review_workspace_id AND f.org_id=r.org_id AND f.source_workspace_id=r.source_workspace_id WHERE r.id=$1 AND f.status='completed'")
            .bind(review_id).fetch_optional(db.conn()).await?.ok_or_else(||not_found("checked formal review fork"))?;
        let manifest: Vec<ForkRepoManifest> =
            serde_json::from_value(manifest).map_err(internal_json)?;
        rollback_db(db).await?;
        let names = manifest
            .iter()
            .map(|m| (m.repo_id.to_string(), m.repo_name.clone()))
            .collect();
        Ok((
            manifest
                .into_iter()
                .map(|m| g5_contracts::types::MergeSourceHead {
                    repo_id: m.repo_id.to_string(),
                    workspace_id: source.to_string(),
                    branch_name: m.source_branch,
                    head_commit: m.source_oid,
                })
                .collect(),
            names,
        ))
    }

    fn local_resume_gate(&self, operation_id: Uuid) -> Arc<LocalForkResumeGate> {
        let mut gates = self
            .local_resume_gates
            .lock()
            .expect("workspace fork resume gates remain available");
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(&operation_id).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(LocalForkResumeGate {
            lock: Arc::new(AsyncMutex::new(())),
            waiters: AtomicUsize::new(0),
        });
        gates.insert(operation_id, Arc::downgrade(&gate));
        gate
    }

    #[cfg(feature = "test-helpers")]
    pub fn local_resume_waiters(&self, operation_id: Uuid) -> usize {
        self.local_resume_gates
            .lock()
            .expect("workspace fork resume gates remain available")
            .get(&operation_id)
            .and_then(Weak::upgrade)
            .map_or(0, |gate| gate.waiters.load(Ordering::Acquire))
    }

    #[cfg(feature = "test-helpers")]
    pub fn with_resume_limits_for_test(&self, slots: usize, wait: Duration) -> Self {
        assert!(slots > 0);
        let mut service = self.clone();
        service.resume_slots = Arc::new(Semaphore::new(slots));
        service.resume_wait = wait;
        service
    }

    #[cfg(feature = "test-helpers")]
    pub fn with_resume_wait_for_test(&self, wait: Duration) -> Self {
        let mut service = self.clone();
        service.resume_wait = wait;
        service
    }

    /// This service's repository store, narrowed to its tenant scope.
    fn repos(&self) -> Result<ScopedRepoStorage, G5Error> {
        let scope = self.scope.as_ref().ok_or_else(|| G5Error::Internal {
            message: "Workspace fork service requires an authorized tenant scope".to_string(),
        })?;
        Ok(self.repo_storage.scoped(scope))
    }

    /// A member repository's directory, for an organization in this
    /// service's tenant scope.
    fn member_path(&self, org_id: Uuid, repo_id: Uuid) -> Result<std::path::PathBuf, G5Error> {
        self.repos()?.repo_path(org_id, repo_id)
    }

    async fn open_db(&self) -> Result<ScopedDb, AppError> {
        let mut db = self
            .scope
            .as_ref()
            .ok_or_else(|| G5Error::Internal {
                message: "Workspace fork service requires an authorized tenant scope".to_string(),
            })?
            .open()
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to open workspace fork database scope: {error}"),
            })?;
        crate::db::operation_deadline::apply(&mut db, self.operation_deadline).await?;
        Ok(db)
    }

    /// Admit or resume one durable fork operation and drive it to a verified
    /// completed child. Provider verification failures leave the same pending
    /// operation available for an idempotent retry.
    pub async fn fork_workspace(
        &self,
        org_id: Uuid,
        source_workspace_id: Uuid,
        requested_by: Uuid,
        request: ForkWorkspaceRequest,
    ) -> Result<CloneWorkspaceResponse, AppError> {
        validate_request(&request)?;
        let digests = request_digests(source_workspace_id, &request)?;

        // A Base re-verification fork still unfinished after the bound has
        // failed (g5.reverification.run): every start, a replay or a retry
        // included, aborts such forks first, so none of them finishes after
        // its run read as abandoned, and a new start is admitted afresh.
        if request.reverification.is_some() {
            self.abort_stale_run_forks(org_id, source_workspace_id)
                .await?;
        }

        if let Some(existing) = self
            .load_operation(org_id, requested_by, &request.idempotency_key)
            .await?
        {
            return self
                .resume_operation(existing, source_workspace_id, &digests, request.interrupt)
                .await;
        }

        // A browser may lose its generated idempotency key after a transient
        // provider or storage failure. The durable pending operation remains
        // the authority for the same requester and exact request, so let an
        // identical retry rediscover it instead of permanently hiding its
        // active name claim behind the lost key.
        if let Some(existing) = self
            .load_matching_active_operation(org_id, source_workspace_id, requested_by, &digests)
            .await?
        {
            return self
                .resume_operation(existing, source_workspace_id, &digests, request.interrupt)
                .await;
        }

        // Only a new admission resolves the issue (GitHub, no transaction
        // open); retries and replays use the identity frozen on the fork.
        let issue = match &request.issue {
            Some(issue) => Some(self.resolve_issue(org_id, issue).await?),
            None => None,
        };
        let operation = self
            .admit_operation(
                org_id,
                source_workspace_id,
                requested_by,
                &request,
                &digests,
                issue.as_ref(),
            )
            .await?;
        self.resume_operation(operation, source_workspace_id, &digests, request.interrupt)
            .await
    }

    async fn resolve_issue(
        &self,
        org_id: Uuid,
        issue: &ForkIssueRequest,
    ) -> Result<(IssueLinkTarget, LinkRole), AppError> {
        let mut db = self.open_db().await?;
        external_issues::ensure_external_issues_enabled(&mut db, org_id).await?;
        let domains = external_issues::list_domain_access(&mut db, org_id).await?;
        rollback_db(db).await?;
        let opener = ForkTenantDb(self);
        // Forking reads no attachment.
        let no_attachments = crate::services::git::native_issues::IssueAttachmentStore::default();
        let native = if domains
            .iter()
            .any(external_issues::IssueDomainAccess::is_native)
        {
            Some(external_issues::NativeBackend {
                opener: &opener,
                system_pool: &self.system_pool,
                site: crate::services::git::native_issues::native_site(
                    &self.system_pool,
                    &self.issue_site_base_url,
                    org_id,
                )
                .await?,
                attachments: &no_attachments,
            })
        } else {
            None
        };
        let backends = external_issues::IssueBackends {
            github: self.github_app.as_deref(),
            native,
        };
        let resolved = external_issues::resolve_reference(&backends, &domains, &issue.reference)
            .await
            .map_err(G5Error::from)?;
        Ok((IssueLinkTarget::from_summary(&resolved.summary), issue.role))
    }

    async fn load_operation(
        &self,
        org_id: Uuid,
        requested_by: Uuid,
        idempotency_key: &str,
    ) -> Result<Option<ForkOperationRow>, AppError> {
        let mut db = self.open_db().await?;
        let operation = sqlx::query_as::<_, ForkOperationRow>(
            r#"
            SELECT id, org_id, source_workspace_id, target_workspace_id,
                   requested_by, owner_user_id, idempotency_key, request_digest,
                   proposed_name, status, repo_manifest, source_flush_id, created_at,
                   completed_at, error
            FROM workspace_forks
            WHERE org_id = $1 AND requested_by = $2 AND idempotency_key = $3
            "#,
        )
        .bind(org_id)
        .bind(requested_by)
        .bind(idempotency_key)
        .fetch_optional(db.conn())
        .await?;
        rollback_db(db).await?;
        Ok(operation)
    }

    async fn load_matching_active_operation(
        &self,
        org_id: Uuid,
        source_workspace_id: Uuid,
        requested_by: Uuid,
        digests: &ForkRequestDigests,
    ) -> Result<Option<ForkOperationRow>, AppError> {
        let mut db = self.open_db().await?;
        let operation = load_matching_active_operation_in_tx(
            db.conn(),
            org_id,
            source_workspace_id,
            requested_by,
            digests,
        )
        .await?;
        rollback_db(db).await?;
        Ok(operation)
    }

    async fn admit_operation(
        &self,
        org_id: Uuid,
        source_workspace_id: Uuid,
        requested_by: Uuid,
        request: &ForkWorkspaceRequest,
        digests: &ForkRequestDigests,
        issue: Option<&(IssueLinkTarget, LinkRole)>,
    ) -> Result<ForkOperationRow, AppError> {
        let mut capture_db = self.open_db().await?;
        let source = sqlx::query_as::<_, SourceWorkspaceRow>(
            r#"
            SELECT id, org_id, owner_user_id, description, docs_repo_id, push_mode, is_base, phase
            FROM workspaces
            WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(source_workspace_id)
        .bind(org_id)
        .fetch_optional(capture_db.conn())
        .await?
        .ok_or_else(|| not_found("source workspace"))?;
        // Forking would copy the private repository and its history to the
        // fork's readers (`g5.personal.refusals`); reviews fork too.
        crate::services::workspace::workspace_kind::refuse_personal(
            capture_db.conn(),
            source_workspace_id,
            "forking",
        )
        .await?;

        if request.source == ForkSource::Live && source.owner_user_id != Some(requested_by) {
            return Err(forbidden());
        }
        if request.reverification.is_some()
            && (!source.is_base || request.read_only || self.approval_review_id.is_some())
        {
            return Err(validation(
                "reverification",
                "Only a writable fork of Base can start a Base re-verification",
            ));
        }
        // A run's workspace merges into Base and lands on an unsent prompt
        // (g5.reverification.workspace), so it takes no other fork options.
        if request.reverification.is_some()
            && (request.merge_target_workspace_id.is_some()
                || request.start_chat.is_some()
                || !request.chat_session_ids.is_empty()
                || request.issue.is_some())
        {
            return Err(validation(
                "reverification",
                "A Base re-verification fork takes no merge target, chat or issue",
            ));
        }

        if !fork_owner_is_eligible(&self.system_pool, org_id, requested_by).await? {
            return Err(forbidden());
        }
        if let Some(target_id) = request.merge_target_workspace_id {
            let target_visible = if self.approval_review_id.is_some() {
                crate::db::scoped::ensure_user_workspace_access_until_commit(
                    &self.system_pool,
                    &mut capture_db,
                    requested_by,
                    org_id,
                    target_id,
                )
                .await?;
                true
            } else {
                sqlx::query_scalar::<_, bool>(
                    r#"
                SELECT EXISTS (
                    SELECT 1 FROM workspaces AS target
                    WHERE target.id = $1 AND target.org_id = $2
                      AND target.deleted_at IS NULL
                      AND authz.workspace_audience_allows(target.personal_user_id, target.owner_user_id,
                          target.agent_approval_review_id, $3, false)
                )
                "#,
                )
                .bind(target_id)
                .bind(org_id)
                .bind(requested_by)
                .fetch_one(capture_db.conn())
                .await?
            };
            if !target_visible {
                return Err(not_found("merge target workspace"));
            }
            crate::services::workspace::workspace_kind::refuse_personal(
                capture_db.conn(),
                target_id,
                "being a merge target",
            )
            .await?;
        }

        let mut source_repos = sqlx::query_as::<_, SourceRepoRow>(
            r#"
            SELECT mapping.repo_id,
                   repo.name AS repo_name,
                   repo.git_url,
                   repo.provider,
                   mapping.root_path,
                   mapping.role,
                   CASE WHEN workspace.is_base THEN repo.default_branch
                        ELSE workspace.branch_name END AS source_branch,
                   NULL::text AS source_oid
            FROM workspace_repos AS mapping
            JOIN repos AS repo
              ON repo.id = mapping.repo_id AND repo.org_id = mapping.org_id
            JOIN workspaces AS workspace
              ON workspace.id = mapping.workspace_id AND workspace.org_id = mapping.org_id
             AND workspace.deleted_at IS NULL
            WHERE mapping.workspace_id = $1 AND mapping.org_id = $2
            ORDER BY mapping.repo_id
            "#,
        )
        .bind(source_workspace_id)
        .bind(org_id)
        .fetch_all(capture_db.conn())
        .await?;
        if source_repos.is_empty() {
            return Err(validation("source", "Workspace has no repository members"));
        }
        // A suggested name becomes the first free one before anything is
        // derived from it; the claim below re-checks it under the locks.
        let uniquified;
        let request = if request.uniquify_name {
            let repo_ids = source_repos
                .iter()
                .map(|repo| repo.repo_id)
                .collect::<Vec<_>>();
            uniquified = ForkWorkspaceRequest {
                name: choose_available_name(capture_db.conn(), org_id, &request.name, &repo_ids)
                    .await?,
                ..request.clone()
            };
            &uniquified
        } else {
            request
        };

        let durable_receipt = if request.source.captures_flush_receipt() {
            sqlx::query_as::<_, (Uuid, serde_json::Value)>(
                r#"
                SELECT flush.flush_id, flush.repos
                FROM workspace_vms AS vm
                JOIN workspace_flushes AS flush
                  ON flush.org_id = vm.org_id AND flush.flush_id = vm.last_flush_id
                WHERE vm.workspace_id = $1 AND flush.workspace_id = $1
                  AND flush.complete_at IS NOT NULL AND flush.pruned_at IS NULL
                  AND flush.error IS NULL
                "#,
            )
            .bind(source_workspace_id)
            .fetch_optional(capture_db.conn())
            .await?
        } else {
            None
        };
        // Frozen formal retries use retained assigned objects. Reading a
        // mutable source ref first would fail after force-push/ref retirement,
        // even while the source's current access and membership remain valid.
        let frozen_source = if let Some(review_id) = self.approval_review_id {
            sqlx::query_scalar::<_, Option<serde_json::Value>>(
                r#"
                SELECT invocation FROM agent_approval_reviews
                WHERE id = $1 AND org_id = $2 AND source_workspace_id = $3
                  AND requested_by = $4 AND stage = 'capture' AND retry_of IS NOT NULL
                "#,
            )
            .bind(review_id)
            .bind(org_id)
            .bind(source_workspace_id)
            .bind(requested_by)
            .fetch_optional(capture_db.conn())
            .await?
            .flatten()
            .map(serde_json::from_value::<g5_contracts::types::FormalApprovalInvocation>)
            .transpose()
            .map_err(internal_json)?
        } else {
            None
        };
        if let Some(frozen) = &frozen_source {
            frozen
                .validate()
                .map_err(|error| validation("source", &error))?;
            if source_repos.len() != frozen.resolution.source_heads.len() {
                return Err(validation(
                    "source",
                    "retry source repository membership changed",
                ));
            }
            for repo in &mut source_repos {
                let pinned = frozen
                    .resolution
                    .source_heads
                    .iter()
                    .find(|head| head.repo_id == repo.repo_id.to_string())
                    .ok_or_else(|| validation("source", "retry source member absent"))?;
                repo.source_branch = Some(pinned.branch_name.clone());
                repo.source_oid = Some(pinned.head_commit.clone());
            }
        }
        let members = source_repos
            .iter()
            .map(|repo| {
                repo.source_branch
                    .clone()
                    .map(|branch| (repo.repo_id, branch))
                    .ok_or_else(|| validation("source", "Source workspace branch is incomplete"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        rollback_db(capture_db).await?;

        // Freeze one committed source cut (`g5.fork.source.cut`): read every
        // member head under the source's commit fence in a short transaction
        // that takes no row lock, then release it. A busy fence refuses before
        // any fork row exists, so an identical retry admits normally.
        let mut current_heads = if frozen_source.is_some() {
            source_repos
                .iter()
                .map(|repo| {
                    (
                        (
                            repo.repo_id,
                            repo.source_branch.clone().expect("frozen branch"),
                        ),
                        repo.source_oid.clone(),
                    )
                })
                .collect()
        } else {
            let mut fence_db = self.open_db().await?;
            let heads = read_committed_branch_heads(
                &mut fence_db,
                source_workspace_id,
                &self.repo_storage,
                &members,
                "fork",
            )
            .await?;
            rollback_db(fence_db).await?;
            heads
        };

        // A previous stop receipt is a valid cut only while its recorded
        // remote branch vector is still current. Once any member is pushed,
        // the immutable receipt describes an older Git cut; use the current
        // remote vector for the whole durable fork instead of silently
        // copying the old snapshot over newly published commits.
        let durable_receipt = if let Some((flush_id, receipt_value)) = durable_receipt {
            let receipts = serde_json::from_value::<Vec<FlushRepoReceipt>>(receipt_value.clone())
                .map_err(internal_json)?;
            let receipt_heads = receipts
                .into_iter()
                .map(|receipt| (receipt.repo_id, receipt.branch_oid))
                .collect::<std::collections::HashMap<_, _>>();
            if receipt_heads.len() != source_repos.len() {
                return Err(validation(
                    "source",
                    "Durable workspace receipt has an incomplete repository vector",
                ));
            }
            if source_repos
                .iter()
                .any(|repo| !receipt_heads.contains_key(&repo.repo_id))
            {
                return Err(validation(
                    "source",
                    "Durable workspace receipt has a different repository vector",
                ));
            }
            let receipt_is_current = members.iter().all(|(repo_id, branch)| {
                current_heads
                    .get(&(*repo_id, branch.clone()))
                    .and_then(Option::as_ref)
                    == receipt_heads.get(repo_id)
            });
            receipt_is_current.then_some((flush_id, receipt_value))
        } else {
            None
        };
        if let Some((_, receipt_value)) = &durable_receipt {
            let receipts = serde_json::from_value::<Vec<FlushRepoReceipt>>(receipt_value.clone())
                .map_err(internal_json)?;
            // Per member: the flushed local head and the branch ref the flush
            // observed (`branch_oid`, an `ls-remote` at flush time).
            let receipt_heads = receipts
                .into_iter()
                .map(|receipt| {
                    (
                        receipt.repo_id,
                        (receipt.local_branch_oid, receipt.branch_oid),
                    )
                })
                .collect::<std::collections::HashMap<_, _>>();
            if receipt_heads.len() != source_repos.len() {
                return Err(validation(
                    "source",
                    "Durable workspace receipt has an incomplete repository vector",
                ));
            }
            for repo in &mut source_repos {
                repo.source_oid = Some(
                    receipt_heads
                        .get(&repo.repo_id)
                        .map(|(local, _)| local.clone())
                        .ok_or_else(|| {
                            validation(
                                "source",
                                "Durable workspace receipt has a different repository vector",
                            )
                        })?,
                );
            }
        } else {
            // Every admission needs a real Git source head, including Live.
            // Live capture replaces this initial cut with the fresh flush
            // before materializing any child refs.
            for repo in &mut source_repos {
                let branch = repo
                    .source_branch
                    .as_ref()
                    .expect("source branch validated above");
                repo.source_oid = Some(
                    current_heads
                        .remove(&(repo.repo_id, branch.clone()))
                        .flatten()
                        .ok_or_else(|| {
                            validation(
                                "source",
                                format!(
                                    "Source branch refs/heads/{branch} is absent from repository {}",
                                    repo.repo_id
                                ),
                            )
                        })?,
                );
            }
        }

        let operation_id = if let Some(review_id) = self.approval_review_id {
            if request.source != ForkSource::Branch
                || !request.chat_session_ids.is_empty()
                || request.start_chat.is_some()
            {
                return Err(forbidden());
            }
            let mut review_db = self.open_db().await?;
            let (child,target): (Uuid,Uuid)=sqlx::query_as("SELECT review_workspace_id,target_workspace_id FROM agent_approval_reviews WHERE id=$1 AND org_id=$2 AND source_workspace_id=$3 AND requested_by=$4 AND stage='capture'")
                .bind(review_id).bind(org_id).bind(source_workspace_id).bind(requested_by).fetch_optional(review_db.conn()).await?.ok_or_else(forbidden)?;
            rollback_db(review_db).await?;
            if request.merge_target_workspace_id != Some(target) {
                return Err(forbidden());
            }
            child
        } else {
            Uuid::new_v4()
        };
        // The fork's starting phase is fixed here, at admission
        // (g5.workbench.phases.state): Base is always `be`, an ordinary source
        // hands over the phase it is in right now. Finalization and every
        // resume read it from the manifest (g5.fork.replay), never from the
        // source again.
        let target_phase = if source.is_base {
            "be"
        } else {
            source.phase.as_str()
        };
        let mut manifest = source_repos
            .into_iter()
            .map(|repo| source_repo_to_manifest(request, target_phase, repo))
            .collect::<Result<Vec<_>, _>>()?;
        let mut source_flush_id = None;
        if let Some((flush_id, receipt_value)) = durable_receipt {
            let receipts = serde_json::from_value::<Vec<FlushRepoReceipt>>(receipt_value)
                .map_err(internal_json)?;
            let mut receipts = receipts
                .into_iter()
                .map(|receipt| (receipt.repo_id, receipt))
                .collect::<std::collections::HashMap<_, _>>();
            if receipts.len() != manifest.len() {
                return Err(validation(
                    "source",
                    "Durable workspace receipt has an incomplete repository vector",
                ));
            }
            let target_flush_id = Uuid::new_v5(&operation_id, b"workspace-fork-copy-flush");
            for member in &mut manifest {
                let receipt = receipts.remove(&member.repo_id).ok_or_else(|| {
                    validation(
                        "source",
                        "Durable workspace receipt has a different repository vector",
                    )
                })?;
                let receipt =
                    normalize_fork_receipt(&self.repos()?, org_id, &member.target_branch, receipt)
                        .await?;
                member.source_oid = receipt.local_branch_oid.clone();
                member.fork_copy = Some(ForkCopyManifest {
                    source_flush_id: flush_id,
                    target_workspace_id: operation_id,
                    target_flush_id,
                    receipt,
                });
            }
            source_flush_id = Some(flush_id);
        }
        let manifest_json = serde_json::to_value(&manifest).map_err(internal_json)?;
        let repo_ids = manifest
            .iter()
            .map(|member| member.repo_id)
            .collect::<Vec<_>>();

        let mut db = self.open_db().await?;
        lock_repo_rows(db.conn(), org_id, &repo_ids).await?;
        // The name claim below is checked under the organization's name lock,
        // which a rename takes too (`g5.workspace.former-names`).
        crate::services::workspace_names::lock_org_workspace_names(db.conn(), org_id).await?;

        crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(
            db.conn(),
            source_workspace_id,
        )
        .await?;

        // READ COMMITTED gives this statement a fresh snapshot after every
        // canonical repository lock has been acquired.
        if let Some(existing) =
            load_operation_in_tx(db.conn(), org_id, requested_by, &request.idempotency_key).await?
        {
            commit_db(db).await?;
            return Ok(existing);
        }

        // Close the race with another identical admission that committed while
        // this caller waited for the canonical repository locks.
        if let Some(existing) = load_matching_active_operation_in_tx(
            db.conn(),
            org_id,
            source_workspace_id,
            requested_by,
            digests,
        )
        .await?
        {
            commit_db(db).await?;
            return Ok(existing);
        }

        // A run in progress refuses a Base re-verification fork before its
        // name is claimed (g5.reverification.run).
        if request.reverification.is_some() {
            base_reverification::refuse_while_another_runs(db.conn(), org_id, source.id).await?;
        }
        if !name_is_available(db.conn(), org_id, &request.name, &repo_ids).await? {
            return Err(name_unavailable());
        }
        if let Some((target, LinkRole::Primary)) = issue {
            external_issues::ensure_primary_available(
                &mut db,
                &self.system_pool,
                Some(requested_by),
                org_id,
                target,
            )
            .await?;
        }
        // The owner's workspace limit (`g5.workspace.owner-limit`) is decided
        // only here: the pending fork counts from now on, and finalizing it
        // or retrying it is never refused, so no admitted fork is stranded.
        workspace_owner_limit::lock_owner(db.conn(), requested_by).await?;
        workspace_owner_limit::ensure_below_limit(db.conn(), org_id, requested_by).await?;

        let start_chat_agent_launch = request
            .start_chat
            .as_ref()
            .and_then(|chat| chat.launch_preferences.as_ref())
            .map(serde_json::to_value)
            .transpose()
            .map_err(internal_json)?;
        let inserted = sqlx::query_as::<_, ForkOperationRow>(
            r#"
            INSERT INTO workspace_forks (
                id, org_id, source_workspace_id, requested_by, owner_user_id,
                idempotency_key, request_digest, proposed_name, repo_manifest, source_flush_id,
                issue_tracker_id, issue_external_id, issue_display_key, issue_canonical_url,
                issue_locator, issue_role, start_chat_content, start_chat_agent_launch
            ) VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                      $17)
            RETURNING id, org_id, source_workspace_id, target_workspace_id,
                      requested_by, owner_user_id, idempotency_key, request_digest,
                      proposed_name, status, repo_manifest, source_flush_id, created_at,
                      completed_at, error
            "#,
        )
        .bind(operation_id)
        .bind(org_id)
        .bind(source.id)
        .bind(requested_by)
        .bind(&request.idempotency_key)
        .bind(&digests.current)
        .bind(&request.name)
        .bind(&manifest_json)
        .bind(source_flush_id)
        .bind(issue.map(|(target, _)| target.tracker_id))
        .bind(issue.map(|(target, _)| target.external_issue_id.as_str()))
        .bind(issue.map(|(target, _)| target.display_key.as_str()))
        .bind(issue.map(|(target, _)| target.canonical_url.as_str()))
        .bind(issue.map(|(target, _)| &target.locator))
        .bind(issue.map(|(_, role)| role.as_str()))
        .bind(
            request
                .start_chat
                .as_ref()
                .map(|chat| chat.content.as_str()),
        )
        .bind(start_chat_agent_launch)
        .fetch_one(db.conn())
        .await;

        let inserted = match inserted {
            Ok(row) => row,
            Err(error) if is_unique_violation(&error) => {
                rollback_db(db).await?;
                let replay = self
                    .load_operation(org_id, requested_by, &request.idempotency_key)
                    .await?;
                return replay.ok_or_else(name_unavailable);
            }
            Err(error) => return Err(error.into()),
        };
        // A Base re-verification run is admitted with its fork or not at all
        // (g5.reverification.run); a refusal rolls the fork back with it.
        if let Some(kind) = request.reverification {
            let base_heads = manifest
                .iter()
                .filter(|member| member.role == "code")
                .map(|member| base_reverification::RunHead {
                    repo_id: member.repo_id,
                    root_path: member.root_path.clone(),
                    commit: member.source_oid.clone(),
                })
                .collect::<Vec<_>>();
            base_reverification::admit(
                db.conn(),
                base_reverification::RunAdmission {
                    org_id,
                    base_workspace_id: source.id,
                    run_id: operation_id,
                    started_by: requested_by,
                    kind,
                    base_heads: &base_heads,
                },
            )
            .await?;
        }
        commit_db(db).await?;
        Ok(inserted)
    }

    async fn resume_operation(
        &self,
        operation: ForkOperationRow,
        expected_source_workspace_id: Uuid,
        expected: &ForkRequestDigests,
        requested_interrupt: bool,
    ) -> Result<CloneWorkspaceResponse, AppError> {
        // Only one caller per operation on this replica reaches a database
        // session. Other retries queue here so they cannot consume the whole
        // global fork capacity while one slow live capture is in progress.
        let local_gate = self.local_resume_gate(operation.id);
        let local_guard = tokio::time::timeout(self.resume_wait, local_gate.acquire())
            .await
            .map_err(|_| fork_resume_busy())?;
        let current = self
            .load_operation(
                operation.org_id,
                operation.requested_by,
                &operation.idempotency_key,
            )
            .await?
            .ok_or_else(|| not_found("workspace fork operation"))?;
        if current.source_workspace_id != expected_source_workspace_id
            || !expected.accepts(&current.request_digest)
        {
            return Err(validation(
                "idempotencyKey",
                "Idempotency key was already used for a different fork request",
            ));
        }
        match current.status.as_str() {
            "completed" => return self.completed_response(&current).await,
            "aborted" => return Err(aborted_fork_error(&current)),
            _ => {}
        }

        // A dropped HTTP future must not release capacity while Postgres still
        // has a backend queued for its advisory lock. The spawned task owns
        // the permit, connection, and durable attempt through completion, even
        // when the caller disconnects before the lock is granted.
        let service = self.clone();
        let expected = expected.clone();
        let operation_id = operation.id;
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _local_gate = local_gate;
            let _local_guard = local_guard;
            let result = async {
                // Bound detached sessions, including lock waiters whose
                // clients have disconnected but whose Postgres backends have
                // not yet noticed the closed socket.
                let _resume_slot = tokio::time::timeout(
                    service.resume_wait,
                    Arc::clone(&service.resume_slots).acquire_owned(),
                )
                .await
                .map_err(|_| fork_resume_busy())?
                .expect("fork resume semaphore remains open");
                let mut resume_connection = service.system_pool.acquire().await?.detach();
                sqlx::query("SELECT set_config('lock_timeout', $1, false)")
                    .bind(format!("{}ms", service.resume_wait.as_millis()))
                    .execute(&mut resume_connection)
                    .await?;
                sqlx::query(
                    "SELECT pg_advisory_lock(\
                         hashtextextended('g5:workspace-fork-resume:' || $1::text, 0)\
                     )",
                )
                .bind(operation.id)
                .execute(&mut resume_connection)
                .await
                .map_err(fork_resume_lock_error)?;
                let result = async {
                    let operation = service
                        .load_operation(
                            operation.org_id,
                            operation.requested_by,
                            &operation.idempotency_key,
                        )
                        .await?
                        .ok_or_else(|| not_found("workspace fork operation"))?;
                    service
                        .resume_operation_locked(
                            operation,
                            expected_source_workspace_id,
                            &expected,
                            requested_interrupt,
                            &mut resume_connection,
                        )
                        .await
                }
                .await;
                if let Err(error) = resume_connection.close().await {
                    tracing::warn!(%error, "Failed to close workspace fork resume lock session");
                }
                result
            }
            .await;
            if let Err(result) = reply_tx.send(result) {
                match result {
                    Ok(_) => {
                        tracing::info!(%operation_id, "Workspace fork completed after caller disconnected")
                    }
                    Err(error) => tracing::warn!(
                        %operation_id,
                        code = error.inner.error_code(),
                        %error,
                        "Workspace fork resumer failed after caller disconnected"
                    ),
                }
            }
        });
        reply_rx.await.map_err(|error| G5Error::Internal {
            message: format!("Workspace fork resumer task ended without a result: {error}"),
        })?
    }

    async fn resume_operation_locked(
        &self,
        mut operation: ForkOperationRow,
        expected_source_workspace_id: Uuid,
        expected: &ForkRequestDigests,
        requested_interrupt: bool,
        lock_connection: &mut PgConnection,
    ) -> Result<CloneWorkspaceResponse, AppError> {
        if operation.source_workspace_id != expected_source_workspace_id
            || !expected.accepts(&operation.request_digest)
        {
            return Err(validation(
                "idempotencyKey",
                "Idempotency key was already used for a different fork request",
            ));
        }
        match operation.status.as_str() {
            "completed" => return self.completed_response(&operation).await,
            "aborted" => return Err(aborted_fork_error(&operation)),
            "aborting" => {
                self.cleanup_operation(&operation).await?;
                return Err(G5Error::Conflict {
                    reason:
                        "Fork cleanup completed; submit the desired fork with a new idempotency key"
                            .to_string(),
                }
                .into());
            }
            "pending" => {}
            status => {
                return Err(G5Error::Internal {
                    message: format!("Unknown workspace fork status '{status}'"),
                }
                .into())
            }
        }

        if operation.status == "pending"
            && operation.source_flush_id.is_none()
            && requested_interrupt
        {
            let mut manifest = parse_manifest(&operation)?;
            let mut changed = false;
            for member in &mut manifest {
                if member.source_kind == ForkSource::Live && !member.live_interrupt {
                    member.live_interrupt = true;
                    changed = true;
                }
            }
            if changed {
                let manifest = serde_json::to_value(manifest).map_err(internal_json)?;
                let mut db = self.open_db().await?;
                let updated = sqlx::query(
                    "UPDATE workspace_forks SET repo_manifest = $2
                     WHERE id = $1 AND status = 'pending' AND source_flush_id IS NULL",
                )
                .bind(operation.id)
                .bind(&manifest)
                .execute(db.conn())
                .await?;
                if updated.rows_affected() != 1 {
                    return Err(G5Error::Conflict {
                        reason: "Fork source capture changed before interrupt authorization"
                            .to_string(),
                    }
                    .into());
                }
                commit_db(db).await?;
                operation.repo_manifest = manifest;
            }
        }
        let mut manifest = parse_manifest(&operation)?;
        if let Err(error) = self.preflight_refs(&operation, &manifest).await {
            if terminal_fork_error(&error) {
                self.abort_without_refs(&operation, &error.inner.to_string())
                    .await?;
            }
            return Err(error);
        }
        if manifest
            .first()
            .is_some_and(|member| member.source_kind == ForkSource::Live)
            && operation.source_flush_id.is_none()
        {
            sqlx::query(
                "SELECT pg_advisory_lock(\
                     hashtextextended('g5:workspace-live-fork-source:' || $1::text, 0)\
                 )",
            )
            .bind(operation.source_workspace_id)
            .execute(&mut *lock_connection)
            .await
            .map_err(fork_resume_lock_error)?;
            let capture = self.capture_live_source(&operation, &manifest).await;
            let released: bool = sqlx::query_scalar(
                "SELECT pg_advisory_unlock(\
                     hashtextextended('g5:workspace-live-fork-source:' || $1::text, 0)\
                 )",
            )
            .bind(operation.source_workspace_id)
            .fetch_one(&mut *lock_connection)
            .await?;
            if !released {
                return Err(G5Error::Internal {
                    message: "Live fork source capture lock was lost".to_string(),
                }
                .into());
            }
            operation = match capture {
                Ok(operation) => operation,
                Err(error) => {
                    if terminal_fork_error(&error) {
                        self.abort_without_refs(&operation, &error.inner.to_string())
                            .await?;
                    }
                    return Err(error);
                }
            };
            manifest = parse_manifest(&operation)?;
        }

        for member in &manifest {
            let current = self
                .load_operation(
                    operation.org_id,
                    operation.requested_by,
                    &operation.idempotency_key,
                )
                .await?
                .ok_or_else(|| not_found("workspace fork operation"))?;
            if current.status == "completed" {
                return self.completed_response(&current).await;
            }
            if current.status != "pending" {
                return self.resume_operation_terminal(current).await;
            }
            if let Err(error) = self.create_member_refs(&current, member).await {
                if terminal_fork_error(&error) {
                    self.begin_abort(&current, &error).await?;
                    self.cleanup_operation(&current).await?;
                }
                return Err(error);
            }
        }

        match self.finalize_operation(&operation, &manifest).await {
            Ok(response) => Ok(response),
            Err(error) => {
                if terminal_fork_error(&error) {
                    self.begin_abort(&operation, &error).await?;
                    self.cleanup_operation(&operation).await?;
                }
                Err(error)
            }
        }
    }

    async fn capture_live_source(
        &self,
        operation: &ForkOperationRow,
        manifest: &[ForkRepoManifest],
    ) -> Result<ForkOperationRow, AppError> {
        let mut capture_db = self.open_db().await?;
        let source_owner = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT owner_user_id FROM workspaces
            WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(operation.source_workspace_id)
        .bind(operation.org_id)
        .fetch_optional(capture_db.conn())
        .await?
        .ok_or_else(|| not_found("source workspace"))?;
        if source_owner != operation.requested_by {
            self.abort_without_refs(
                operation,
                "Live source capture is no longer authorized for this operation",
            )
            .await?;
            return Err(forbidden());
        }

        let interrupt = manifest.first().is_some_and(|member| member.live_interrupt);
        if !interrupt {
            let active_work = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM tasks
                    WHERE workspace_id = $1
                      AND status IN ('queued', 'waiting', 'running')
                )
                "#,
            )
            .bind(operation.source_workspace_id)
            .fetch_one(capture_db.conn())
            .await?;
            if active_work {
                return Err(G5Error::Conflict {
                    reason: "WORKSPACE_BUSY".to_string(),
                }
                .into());
            }
        }
        rollback_db(capture_db).await?;

        let handle = self
            .runtime
            .owner_handle(operation.source_workspace_id, operation.requested_by)
            .await?;
        let deadline_ms = unix_time_ms().saturating_add(30_000);
        let close_request = RuntimeRequest::lifecycle(
            handle.workspace_id,
            handle.vm_id,
            handle.epoch,
            deadline_ms,
            format!("fork:{}:admission", operation.id),
        );
        self.runtime
            .call(
                &handle,
                &close_request,
                "admission.close",
                serde_json::json!({"mode":"fork", "interrupt":interrupt, "operation_id":operation.id}),
            )
            .await
            .map_err(runtime_app_error)?;

        let flush_request = RuntimeRequest::lifecycle(
            handle.workspace_id,
            handle.vm_id,
            handle.epoch,
            unix_time_ms().saturating_add(120_000),
            format!("fork:{}:flush", operation.id),
        );
        let flush_result = self
            .runtime
            .call(
                &handle,
                &flush_request,
                "workspace.flush",
                serde_json::json!({"reason":"fork"}),
            )
            .await;
        let reopen_request = RuntimeRequest::lifecycle(
            handle.workspace_id,
            handle.vm_id,
            handle.epoch,
            unix_time_ms().saturating_add(10_000),
            format!("fork:{}:reopen", operation.id),
        );
        let reopen_result = self
            .runtime
            .call(
                &handle,
                &reopen_request,
                "admission.open",
                serde_json::json!({"operation_id":operation.id}),
            )
            .await;
        let flush_value = flush_result.map_err(runtime_app_error)?;
        reopen_result.map_err(runtime_app_error)?;
        let flush: FlushCompleteResponse =
            serde_json::from_value(flush_value).map_err(|error| G5Error::Internal {
                message: format!("Invalid live fork flush reply: {error}"),
            })?;

        let mut receipts = flush
            .repos
            .into_iter()
            .map(|receipt| (receipt.repo_id, receipt))
            .collect::<std::collections::HashMap<_, _>>();
        if receipts.len() != manifest.len() {
            return Err(G5Error::Conflict {
                reason: "Live fork returned an incomplete repository vector".to_string(),
            }
            .into());
        }
        let target_flush_id = Uuid::new_v5(&operation.id, b"workspace-fork-copy-flush");
        let mut updated = manifest.to_vec();
        for member in &mut updated {
            let receipt = receipts
                .remove(&member.repo_id)
                .ok_or_else(|| G5Error::Conflict {
                    reason: "Live fork returned a different repository vector".to_string(),
                })?;
            let receipt = normalize_fork_receipt(
                &self.repos()?,
                operation.org_id,
                &member.target_branch,
                receipt,
            )
            .await?;
            member.source_oid = receipt.local_branch_oid.clone();
            member.fork_copy = Some(ForkCopyManifest {
                source_flush_id: flush.flush_id,
                target_workspace_id: operation.id,
                target_flush_id,
                receipt,
            });
        }
        if !receipts.is_empty() {
            return Err(G5Error::Conflict {
                reason: "Live fork returned an unexpected repository vector".to_string(),
            }
            .into());
        }

        let updated_json = serde_json::to_value(updated).map_err(internal_json)?;
        let repo_ids = manifest
            .iter()
            .map(|member| member.repo_id)
            .collect::<Vec<_>>();
        let mut db = self.open_db().await?;
        lock_repo_rows(db.conn(), operation.org_id, &repo_ids).await?;
        let current = lock_operation_status(db.conn(), operation.id).await?;
        if current != "pending" {
            return Err(G5Error::Conflict {
                reason: format!("Fork operation is {current}"),
            }
            .into());
        }
        sqlx::query(
            r#"
            UPDATE workspace_forks
            SET repo_manifest = $2, source_flush_id = $3
            WHERE id = $1 AND status = 'pending' AND source_flush_id IS NULL
            "#,
        )
        .bind(operation.id)
        .bind(updated_json)
        .bind(flush.flush_id)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        self.load_operation(
            operation.org_id,
            operation.requested_by,
            &operation.idempotency_key,
        )
        .await?
        .ok_or_else(|| not_found("workspace fork operation"))
    }

    async fn resume_operation_terminal(
        &self,
        operation: ForkOperationRow,
    ) -> Result<CloneWorkspaceResponse, AppError> {
        match operation.status.as_str() {
            "completed" => self.completed_response(&operation).await,
            "aborting" => {
                self.cleanup_operation(&operation).await?;
                Err(G5Error::Conflict {
                    reason: "Fork operation was aborted while it was being resumed".to_string(),
                }
                .into())
            }
            "aborted" => Err(G5Error::Conflict {
                reason: "Fork operation was aborted".to_string(),
            }
            .into()),
            _ => Err(G5Error::Conflict {
                reason: "Fork operation changed while it was being resumed".to_string(),
            }
            .into()),
        }
    }

    async fn preflight_refs(
        &self,
        operation: &ForkOperationRow,
        manifest: &[ForkRepoManifest],
    ) -> Result<(), AppError> {
        for member in manifest {
            let path = self.member_path(operation.org_id, member.repo_id)?;
            let target = member.target_branch.clone();
            let source_oid = member.source_oid.clone();
            let fork_id = operation.id;
            let member_copy = fork_copy_refs(member);
            let local = tokio::task::spawn_blocking(move || {
                let repo = GitRepo::open(&path)?;
                if !repo.has_commit(&source_oid)? {
                    return Err(G5Error::GitError {
                        message: format!("Pinned source commit {source_oid} is unavailable"),
                    });
                }
                let branches = repo.local_branch_names()?;
                let collision = if branches.iter().any(|name| name == &target) {
                    let proof_valid = match &member_copy {
                        Some(copy) => repo
                            .verify_workspace_fork_copy_refs(&target, fork_id, &source_oid, copy)
                            .is_ok(),
                        None => repo
                            .verify_workspace_fork_refs(&target, fork_id, &source_oid)
                            .is_ok(),
                    };
                    !proof_valid
                        || branches
                            .iter()
                            .any(|name| name != &target && branch_names_conflict(name, &target))
                } else {
                    branches
                        .iter()
                        .any(|name| branch_names_conflict(name, &target))
                };
                Ok::<_, G5Error>(collision)
            })
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Fork local preflight task failed: {error}"),
            })??;
            if local {
                self.abort_without_refs(operation, "Fork name is unavailable")
                    .await?;
                return Err(name_unavailable());
            }

            // A stored remote identity still requires collision verification
            // when its URL is ineligible for credentials. Probe without a token.
            if crate::services::git::git_policy::is_local_repo_identity(
                member.provider.as_deref(),
                &member.git_url,
            ) {
                continue;
            }
            let auth = self
                .repository_credentials
                .resolve(
                    operation.org_id,
                    member.repo_id,
                    operation.requested_by,
                    &member.git_url,
                    RepositoryOperation::ReadGit,
                )
                .await?;
            let target = member.target_branch.clone();
            let heads = crate::git_runtime::remote_branch_names(
                &member.git_url,
                &auth,
                FORK_PROVIDER_PREFLIGHT_WAIT,
            )
            .await
            .map_err(|error| G5Error::ServiceUnavailable {
                message: format!("Provider branch verification is unavailable: {error}"),
            })?;
            if heads
                .iter()
                .any(|name| branch_names_conflict(name, &target))
            {
                self.abort_without_refs(operation, "Fork name is unavailable")
                    .await?;
                return Err(name_unavailable());
            }
        }
        Ok(())
    }

    async fn create_member_refs(
        &self,
        operation: &ForkOperationRow,
        member: &ForkRepoManifest,
    ) -> Result<ForkRefsCreateOutcome, AppError> {
        let manifest = parse_manifest(operation)?;
        let repo_ids = manifest.iter().map(|item| item.repo_id).collect::<Vec<_>>();
        let mut db = self.open_db().await?;
        lock_repo_rows(db.conn(), operation.org_id, &repo_ids).await?;
        let status = lock_operation_status(db.conn(), operation.id).await?;
        if status == "completed" {
            // Another retry of this same durable operation may have completed
            // while this caller was in provider preflight or waiting for the
            // canonical repository locks. Treat its committed refs as the
            // idempotent result; finalize_operation will return that completed
            // operation's response.
            commit_db(db).await?;
            return Ok(ForkRefsCreateOutcome::AlreadyApplied);
        }
        if status != "pending" {
            return Err(G5Error::Conflict {
                reason: format!("Fork operation is {status}"),
            }
            .into());
        }
        let path = self.member_path(operation.org_id, member.repo_id)?;
        let branch = member.target_branch.clone();
        let oid = member.source_oid.clone();
        let fork_id = operation.id;
        let copy = fork_copy_refs(member);
        let outcome = tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&path)?;
            match copy {
                Some(copy) => repo.create_workspace_fork_copy_refs(&branch, fork_id, &oid, &copy),
                None => repo.create_workspace_fork_refs(&branch, fork_id, &oid),
            }
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Fork ref creation task failed: {error}"),
        })??;
        commit_db(db).await?;
        Ok(outcome)
    }

    /// Exercise the completed-operation branch after canonical repository
    /// locking without allowing a second public resumer past the operation
    /// serializer. The branch remains a mixed-version rollout fallback when an
    /// older replica completes the row after this version takes repo locks.
    /// This seam is present only in API integration test builds.
    #[cfg(feature = "test-helpers")]
    pub async fn probe_completed_member_refs_after_repo_lock(
        &self,
        org_id: Uuid,
        requested_by: Uuid,
        idempotency_key: &str,
    ) -> Result<bool, AppError> {
        let operation = self
            .load_operation(org_id, requested_by, idempotency_key)
            .await?
            .ok_or_else(|| not_found("workspace fork operation"))?;
        let manifest = parse_manifest(&operation)?;
        let member = manifest
            .first()
            .ok_or_else(|| validation("source", "Fork manifest has no repository members"))?;
        Ok(matches!(
            self.create_member_refs(&operation, member).await?,
            ForkRefsCreateOutcome::AlreadyApplied
        ))
    }

    async fn begin_abort(
        &self,
        operation: &ForkOperationRow,
        cause: &AppError,
    ) -> Result<(), AppError> {
        let manifest = parse_manifest(operation)?;
        let repo_ids = manifest.iter().map(|item| item.repo_id).collect::<Vec<_>>();
        let mut db = self.open_db().await?;
        lock_repo_rows(db.conn(), operation.org_id, &repo_ids).await?;
        sqlx::query(
            r#"
            UPDATE workspace_forks
            SET status = 'aborting', error = $2
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(operation.id)
        .bind(serde_json::json!({
            "code": cause.inner.error_code(),
            "message": cause.inner.to_string(),
        }))
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    /// Abort the Base re-verification forks still pending after
    /// [`base_reverification::STALE_FORK_MINUTES`], whose runs read as
    /// abandoned, so they can never finish afterwards.
    async fn abort_stale_run_forks(
        &self,
        org_id: Uuid,
        base_workspace_id: Uuid,
    ) -> Result<(), AppError> {
        let mut db = self.open_db().await?;
        let stale =
            base_reverification::stale_run_forks(db.conn(), org_id, base_workspace_id).await?;
        let mut operations = Vec::new();
        for id in stale {
            if let Some(operation) = sqlx::query_as::<_, ForkOperationRow>(
                r#"
                SELECT id, org_id, source_workspace_id, target_workspace_id,
                       requested_by, owner_user_id, idempotency_key, request_digest,
                       proposed_name, status, repo_manifest, source_flush_id, created_at,
                       completed_at, error
                FROM workspace_forks
                WHERE id = $1 AND org_id = $2 AND status = 'pending'
                "#,
            )
            .bind(id)
            .bind(org_id)
            .fetch_optional(db.conn())
            .await?
            {
                operations.push(operation);
            }
        }
        rollback_db(db).await?;
        for operation in operations {
            self.abort_without_refs(
                &operation,
                "This Base re-verification fork did not finish and was abandoned",
            )
            .await?;
        }
        Ok(())
    }

    async fn abort_without_refs(
        &self,
        operation: &ForkOperationRow,
        message: &str,
    ) -> Result<(), AppError> {
        let error = AppError::from_error(G5Error::Conflict {
            reason: message.to_string(),
        });
        self.begin_abort(operation, &error).await?;
        self.cleanup_operation(operation).await
    }

    async fn cleanup_operation(&self, operation: &ForkOperationRow) -> Result<(), AppError> {
        let manifest = parse_manifest(operation)?;
        let repo_ids = manifest.iter().map(|item| item.repo_id).collect::<Vec<_>>();
        let mut db = self.open_db().await?;
        lock_repo_rows(db.conn(), operation.org_id, &repo_ids).await?;
        let status = lock_operation_status(db.conn(), operation.id).await?;
        if status == "aborted" {
            commit_db(db).await?;
            return Ok(());
        }
        if status != "aborting" {
            return Err(G5Error::Conflict {
                reason: format!("Cannot clean up fork operation in state {status}"),
            }
            .into());
        }

        for member in &manifest {
            let path = self.member_path(operation.org_id, member.repo_id)?;
            let branch = member.target_branch.clone();
            let oid = member.source_oid.clone();
            let fork_id = operation.id;
            let copy = fork_copy_refs(member);
            tokio::task::spawn_blocking(move || {
                let repo = GitRepo::open(&path)?;
                match copy {
                    Some(copy) => repo
                        .delete_workspace_fork_copy_refs_if_matches(&branch, fork_id, &oid, &copy),
                    None => repo.delete_workspace_fork_refs_if_matches(&branch, fork_id, &oid),
                }
            })
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Fork cleanup task failed: {error}"),
            })??;
        }

        sqlx::query(
            "UPDATE workspace_forks SET status = 'aborted' WHERE id = $1 AND status = 'aborting'",
        )
        .bind(operation.id)
        .execute(db.conn())
        .await?;
        commit_db(db).await?;
        Ok(())
    }

    async fn finalize_operation(
        &self,
        operation: &ForkOperationRow,
        manifest: &[ForkRepoManifest],
    ) -> Result<CloneWorkspaceResponse, AppError> {
        let repo_ids = manifest.iter().map(|item| item.repo_id).collect::<Vec<_>>();
        let mut db = self.open_db().await?;
        lock_repo_rows(db.conn(), operation.org_id, &repo_ids).await?;
        let status = lock_operation_status(db.conn(), operation.id).await?;
        if status == "completed" {
            commit_db(db).await?;
            let current = self
                .load_operation(
                    operation.org_id,
                    operation.requested_by,
                    &operation.idempotency_key,
                )
                .await?
                .ok_or_else(|| not_found("workspace fork operation"))?;
            return self.completed_response(&current).await;
        }
        if status != "pending" {
            return Err(G5Error::Conflict {
                reason: format!("Cannot complete fork operation in state {status}"),
            }
            .into());
        }

        // A same-name operation on a disjoint repository vector can finalize
        // between admission's workspace-name read and active-claim read. The
        // active-claim index serializes pending operations, while this fresh
        // check prevents a just-completed sibling from publishing a duplicate
        // workspace name after its claim becomes terminal.
        let name_taken = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM workspaces
                 WHERE org_id = $1 AND deleted_at IS NULL
                   AND lower(name) = lower($2))",
        )
        .bind(operation.org_id)
        .bind(&operation.proposed_name)
        .fetch_one(db.conn())
        .await?;
        let repo_ids = manifest
            .iter()
            .map(|member| member.repo_id)
            .collect::<Vec<_>>();
        if name_taken || branch_name_owned(db.conn(), &repo_ids, &operation.proposed_name).await? {
            return Err(name_unavailable());
        }

        for member in manifest {
            let path = self.member_path(operation.org_id, member.repo_id)?;
            let branch = member.target_branch.clone();
            let oid = member.source_oid.clone();
            let fork_id = operation.id;
            let copy = fork_copy_refs(member);
            tokio::task::spawn_blocking(move || {
                let repo = GitRepo::open(&path)?;
                match copy {
                    Some(copy) => {
                        repo.verify_workspace_fork_copy_refs(&branch, fork_id, &oid, &copy)
                    }
                    None => repo.verify_workspace_fork_refs(&branch, fork_id, &oid),
                }
            })
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Fork verification task failed: {error}"),
            })??;
        }

        let source = sqlx::query_as::<_, SourceWorkspaceRow>(
            r#"
            SELECT id, org_id, owner_user_id, description, docs_repo_id, push_mode, is_base, phase
            FROM workspaces
            WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL
            FOR SHARE
            "#,
        )
        .bind(operation.source_workspace_id)
        .bind(operation.org_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| not_found("source workspace"))?;
        crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(
            db.conn(),
            operation.source_workspace_id,
        )
        .await?;
        let target_id = operation.id;
        let target = manifest
            .first()
            .ok_or_else(|| validation("source", "Fork manifest has no repository members"))?;
        let description = target
            .target_description
            .clone()
            .or_else(|| source.description.clone());
        // A detached resumer can outlive the review's capture deadline. Never
        // turn that late fork into an ordinary, human-visible workspace. Hold
        // the review through publication so terminalization sees the child.
        let review: Option<(Uuid,Uuid,String)> = sqlx::query_as("SELECT id,target_workspace_id,stage FROM agent_approval_reviews WHERE review_workspace_id=$1 AND org_id=$2 AND source_workspace_id=$3 AND requested_by=$4 FOR SHARE")
            .bind(operation.id).bind(operation.org_id).bind(operation.source_workspace_id).bind(operation.requested_by).fetch_optional(db.conn()).await?;
        if review
            .as_ref()
            .is_some_and(|(_, _, stage)| stage != "capture")
            || (self.approval_review_id.is_some() && review.is_none())
        {
            return Err(G5Error::Conflict {
                reason: "formal review ended before its fork completed".into(),
            }
            .into());
        }
        let formal = review.map(|(id, target, _)| (id, target));
        let mutation_policy = if target.target_read_only {
            "read_only"
        } else {
            "read_write"
        };

        sqlx::query(
            r#"
            INSERT INTO workspaces (
                id, org_id, name, branch_name, is_base, description, cloned_from_id,
                docs_repo_id,
                created_by, push_mode, mutation_policy,
                merge_target_workspace_id, owner_user_id, phase, agent_approval_review_id
            ) VALUES (
                $1, $2, $3, $3, false, $4, $5,
                $6, $7, $8, $9, $10, $7, $11, $12
            )
            "#,
        )
        .bind(target_id)
        .bind(operation.org_id)
        .bind(&operation.proposed_name)
        .bind(description)
        .bind(formal.map_or(operation.source_workspace_id, |(_, target)| target))
        .bind(source.docs_repo_id)
        .bind(operation.owner_user_id)
        // Base's push mode is always manual because Base never publishes
        // (`g5.workspace.remote.base-parity`); a fork of Base starts in auto,
        // the default for new workspaces (`g5.governance.remote.push`).
        .bind(if formal.is_some() {
            "manual"
        } else if source.is_base {
            "auto"
        } else {
            source.push_mode.as_str()
        })
        .bind(mutation_policy)
        .bind(target.target_merge_target_workspace_id)
        // The phase admitted into the manifest (g5.workbench.phases.state);
        // a resumed operation does not re-read the source (g5.fork.replay).
        .bind(&target.target_phase)
        .bind(formal.map(|(review, _)| review))
        .execute(db.conn())
        .await?;

        // A new workspace starts with its organization's current default idle
        // timeout, never its source's (`g5.lifecycle.idle.settings`). Whether
        // its agent may change other workspaces is copied only into a fork its
        // source's owner owns; a fork for anyone else starts with the default,
        // on, and a formal review workspace with it off
        // (`g5.security.agent-other-workspaces`). Every other setting is copied
        // from the source.
        let idle_timeout_secs =
            org_admin_settings::default_idle_timeout_secs(db.conn(), operation.org_id).await?;
        let agent_other_workspaces: Option<bool> = if formal.is_some() {
            Some(false)
        } else if source.owner_user_id == Some(operation.owner_user_id) {
            None
        } else {
            Some(true)
        };
        sqlx::query(
            r#"
            INSERT INTO workspace_settings (
                workspace_id, org_id, max_spend_usd,
                spend_period, spend_period_start, max_input_tokens,
                max_output_tokens, updated_by,
                idle_timeout_secs,
                response_deadline_secs, bounded_execution_slots,
                permission_policy, agent_may_modify_other_workspaces
            )
            SELECT $1, org_id, max_spend_usd,
                   spend_period, spend_period_start, max_input_tokens,
                   max_output_tokens, $2,
                   $4,
                   response_deadline_secs, bounded_execution_slots,
                   permission_policy, COALESCE($5, agent_may_modify_other_workspaces)
            FROM workspace_settings
            WHERE workspace_id = $3
            "#,
        )
        .bind(target_id)
        .bind(operation.owner_user_id)
        .bind(operation.source_workspace_id)
        .bind(idle_timeout_secs)
        .bind(agent_other_workspaces)
        .execute(db.conn())
        .await?;

        for member in manifest {
            sqlx::query(
                "INSERT INTO workspace_repos (org_id, workspace_id, repo_id, root_path, role)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(operation.org_id)
            .bind(target_id)
            .bind(member.repo_id)
            .bind(&member.root_path)
            .bind(&member.role)
            .execute(db.conn())
            .await?;
        }

        let fork_copy_flush_id = manifest
            .first()
            .and_then(|member| member.fork_copy.as_ref())
            .map(|copy| copy.target_flush_id);
        if let Some(target_flush_id) = fork_copy_flush_id {
            let source_flush_id = operation.source_flush_id.ok_or_else(|| G5Error::Internal {
                message: "Live fork manifest has no source flush".to_string(),
            })?;
            let receipts = manifest
                .iter()
                .map(|member| {
                    member
                        .fork_copy
                        .as_ref()
                        .filter(|copy| {
                            copy.source_flush_id == source_flush_id
                                && copy.target_flush_id == target_flush_id
                                && copy.target_workspace_id == target_id
                        })
                        .map(|copy| copy.receipt.clone())
                        .ok_or_else(|| G5Error::Internal {
                            message: "Live fork manifest has inconsistent receipt metadata"
                                .to_string(),
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            sqlx::query(
                r#"
                INSERT INTO workspace_flushes (
                    flush_id, org_id, workspace_id, origin, source_flush_id,
                    reason, required_repo_ids, repos, complete_at
                ) VALUES ($1, $2, $3, 'fork_copy', $4, 'fork', $5, $6, now())
                "#,
            )
            .bind(target_flush_id)
            .bind(operation.org_id)
            .bind(target_id)
            .bind(source_flush_id)
            .bind(&repo_ids)
            .bind(serde_json::to_value(receipts).map_err(internal_json)?)
            .execute(db.conn())
            .await?;
        }

        sqlx::query(
            r#"
            INSERT INTO workspace_vms (org_id, workspace_id, status, last_flush_id)
            VALUES ($1, $2, 'stopped', $3)
            "#,
        )
        .bind(operation.org_id)
        .bind(target_id)
        .bind(fork_copy_flush_id)
        .execute(db.conn())
        .await?;
        if let Some(ids) = manifest
            .first()
            .map(|member| member.carry_session_ids.as_slice())
            .filter(|ids| !ids.is_empty())
        {
            self.messenger
                .copy_sessions(
                    db.conn(),
                    operation.source_workspace_id,
                    target_id,
                    ids,
                    operation.owner_user_id,
                )
                .await?;
        }
        // The requested issue is linked in the same transaction that
        // publishes the workspace, while the organization still has external
        // issues on. Retries resume without resolving the issue again, so an
        // admin who turns them off after admission is honored here: the fork
        // completes without the link rather than failing or staying pending.
        let issue = match load_issue_row(&mut db, operation.id).await? {
            Some(issue)
                if external_issues::org_external_issues_enabled(&mut db, operation.org_id)
                    .await? =>
            {
                Some(issue)
            }
            _ => None,
        };
        let issue_link_id = match issue {
            Some(issue) => Some(
                external_issues::link_forked_workspace(
                    &mut db,
                    operation.org_id,
                    target_id,
                    operation.requested_by,
                    &issue.target(),
                    issue.role(),
                )
                .await?
                .id,
            ),
            None => None,
        };
        // A requested chat is queued in the same transaction. It never
        // undoes the fork: if it cannot start, the reason is recorded.
        let (started_session_id, start_chat_error) =
            match load_start_chat(&mut db, operation.id).await? {
                Some((content, launch_preferences)) => {
                    self.start_chat(&mut db, operation, target_id, content, launch_preferences)
                        .await?
                }
                None => (None, None),
            };
        sqlx::query(
            r#"
            UPDATE workspace_forks
            SET status = 'completed', target_workspace_id = $2,
                completed_at = now(), error = NULL, issue_link_id = $3,
                started_session_id = $4, start_chat_error = $5
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(operation.id)
        .bind(target_id)
        .bind(issue_link_id)
        .bind(started_session_id)
        .bind(start_chat_error)
        .execute(db.conn())
        .await?;
        // Capture the initial complete cut. These are fork observations, not
        // new pushes; unknown inherited push times remain explicitly unknown.
        let initial = manifest
            .iter()
            .map(|m| g5_contracts::types::MergeSourceHead {
                repo_id: m.repo_id.to_string(),
                workspace_id: target_id.to_string(),
                branch_name: m.target_branch.clone(),
                head_commit: m.source_oid.clone(),
            })
            .collect();
        let repos = self.repo_storage.scoped(&db);
        crate::services::git::workspace_revision_history::observe(
            db.conn(),
            &repos,
            operation.org_id,
            target_id,
            initial,
            chrono::Utc::now(),
            &[],
        )
        .await;
        commit_db(db).await?;

        let current = self
            .load_operation(
                operation.org_id,
                operation.requested_by,
                &operation.idempotency_key,
            )
            .await?
            .ok_or_else(|| not_found("workspace fork operation"))?;
        self.completed_response(&current).await
    }

    async fn completed_response(
        &self,
        operation: &ForkOperationRow,
    ) -> Result<CloneWorkspaceResponse, AppError> {
        let target_id = operation
            .target_workspace_id
            .ok_or_else(|| G5Error::Internal {
                message: "Completed fork has no target workspace".to_string(),
            })?;
        let manifest = parse_manifest(operation)?;
        let mut db = self.open_db().await?;
        let workspace = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<Uuid>,
                bool,
                DateTime<Utc>,
                DateTime<Utc>,
            ),
        >(
            r#"
            SELECT name, description, cloned_from_id,
                   mutation_policy = 'read_only' AS read_only,
                   created_at, updated_at
            FROM workspaces
            WHERE id = $1 AND org_id = $2
            "#,
        )
        .bind(target_id)
        .bind(operation.org_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| not_found("completed fork workspace"))?;

        let branches = manifest
            .iter()
            .map(|member| ClonedBranchSummary {
                workspace_id: target_id,
                repo_id: member.repo_id,
                repo_name: member.repo_name.clone(),
                branch_name: member.target_branch.clone(),
                tracking_mode: "pinned".to_string(),
                head_commit: member.source_oid.clone(),
                materialization_status: "materialized".to_string(),
                divergence: None,
                behind_base: None,
                has_draft: false,
                dirty_file_count: 0,
                remote_branch: None,
                co_tracking_workspaces: None,
            })
            .collect::<Vec<_>>();
        let carry_session_ids = manifest
            .first()
            .map(|member| member.carry_session_ids.as_slice())
            .unwrap_or_default();
        let carried_sessions = if carry_session_ids.is_empty() {
            0
        } else {
            sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM chat_sessions
                WHERE workspace_id = $1 AND forked_from_id = ANY($2)
                "#,
            )
            .bind(target_id)
            .bind(carry_session_ids)
            .fetch_one(db.conn())
            .await? as i32
        };
        let issue_link = sqlx::query_as::<_, (Uuid, String, String, String, bool)>(
            r#"
            SELECT link.id, link.display_key, link.canonical_url, fork.issue_role,
                   link.is_primary
            FROM workspace_forks AS fork
            JOIN workspace_issue_links AS link ON link.id = fork.issue_link_id
            WHERE fork.id = $1
            "#,
        )
        .bind(operation.id)
        .fetch_optional(db.conn())
        .await?
        .map(
            |(link_id, display_key, canonical_url, requested_role, is_primary)| {
                ForkIssueLinkOutcome {
                    link_id,
                    display_key,
                    canonical_url,
                    requested_role: LinkRole::parse(&requested_role)
                        .unwrap_or(LinkRole::Related)
                        .as_str(),
                    role: if is_primary {
                        LinkRole::Primary.as_str()
                    } else {
                        LinkRole::Related.as_str()
                    },
                }
            },
        );
        let started_chat = sqlx::query_as::<_, (Option<Uuid>, Option<String>)>(
            r#"
            SELECT started_session_id, start_chat_error
            FROM workspace_forks
            WHERE id = $1 AND start_chat_content IS NOT NULL
            "#,
        )
        .bind(operation.id)
        .fetch_optional(db.conn())
        .await?
        .map(|(session_id, error)| ForkStartedChatOutcome { session_id, error });
        rollback_db(db).await?;
        Ok(CloneWorkspaceResponse {
            workspace: CloneWorkspaceDetail {
                id: target_id,
                name: workspace.0,
                description: workspace.1,
                owner_user_id: operation.owner_user_id,
                created_by: Some(operation.owner_user_id),
                cloned_from_id: workspace.2,
                read_only: workspace.3,
                branches,
                tasks_completed: 0,
                total_tasks: 0,
                running_time: None,
                cost: 0.0,
                concurrent_tasks: 0,
                dependencies: Vec::new(),
                last_updated: workspace.5.to_rfc3339(),
                created_at: workspace.4.to_rfc3339(),
            },
            cloned_branches: manifest.len() as i32,
            cloned_drafts: 0,
            carried_sessions,
            issue_link,
            started_chat,
        })
    }

    /// Queue the fork's first message in the new workspace, inside the
    /// completion transaction. Any failure is rolled back to a savepoint and
    /// returned as the recorded reason, so the fork still completes.
    async fn start_chat(
        &self,
        db: &mut ScopedDb,
        operation: &ForkOperationRow,
        target_id: Uuid,
        content: String,
        launch_preferences: Option<serde_json::Value>,
    ) -> Result<(Option<Uuid>, Option<String>), AppError> {
        let preferences: g5_contracts::types::AgentLaunchPreferencesV1 =
            match launch_preferences.map(serde_json::from_value).transpose() {
                Ok(preferences) => preferences.unwrap_or_default(),
                Err(error) => {
                    return Ok((
                        None,
                        Some(format!(
                            "the recorded launch preferences are not valid: {error}"
                        )),
                    ))
                }
            };
        sqlx::query("SAVEPOINT fork_start_chat")
            .execute(db.conn())
            .await?;
        let sent = self
            .messenger
            .start_chat(
                db,
                ForkChatStart {
                    org_id: operation.org_id,
                    workspace_id: target_id,
                    actor_user_id: operation.owner_user_id,
                    content,
                    legacy_preferences: preferences,
                },
            )
            .await;
        match sent {
            Ok(session_id) => {
                sqlx::query("RELEASE SAVEPOINT fork_start_chat")
                    .execute(db.conn())
                    .await?;
                Ok((Some(session_id), None))
            }
            Err(error) => {
                sqlx::query("ROLLBACK TO SAVEPOINT fork_start_chat")
                    .execute(db.conn())
                    .await?;
                tracing::warn!(
                    fork_id = %operation.id,
                    workspace_id = %target_id,
                    error = %error.inner,
                    "Fork completed without starting its chat"
                );
                Ok((None, Some(error.inner.to_string())))
            }
        }
    }
}

/// The fork's frozen first message and launch preferences, if it asked to
/// start a chat.
async fn load_start_chat(
    db: &mut ScopedDb,
    operation_id: Uuid,
) -> Result<Option<(String, Option<serde_json::Value>)>, AppError> {
    Ok(sqlx::query_as::<_, (String, Option<serde_json::Value>)>(
        "SELECT start_chat_content, start_chat_agent_launch FROM workspace_forks \
         WHERE id = $1 AND start_chat_content IS NOT NULL",
    )
    .bind(operation_id)
    .fetch_optional(db.conn())
    .await?)
}

/// The fork's frozen issue request, if it named one.
async fn load_issue_row(
    db: &mut ScopedDb,
    operation_id: Uuid,
) -> Result<Option<ForkIssueRow>, AppError> {
    Ok(sqlx::query_as::<_, ForkIssueRow>(
        r#"
        SELECT issue_tracker_id, issue_external_id, issue_display_key, issue_canonical_url,
               issue_locator, issue_role
        FROM workspace_forks
        WHERE id = $1 AND issue_tracker_id IS NOT NULL
        "#,
    )
    .bind(operation_id)
    .fetch_optional(db.conn())
    .await?)
}

const MAX_ISSUE_REFERENCE_CHARS: usize = 2048;
/// The agent prompt limit (`agent_dispatch`).
const MAX_START_CHAT_BYTES: usize = 512 * 1024;

/// Whether `user_id` may own a fork in `org_id`: an active member whose role
/// owns workspaces. Base re-verification offers its start on the same terms
/// (g5.reverification.run), so Home asks this too.
pub async fn fork_owner_is_eligible(
    system_pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM org_memberships
            WHERE org_id = $1 AND user_id = $2
              AND role IN ('admin', 'member') AND deactivated_at IS NULL
        )
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(system_pool)
    .await?)
}

/// A workspace name is the exact Git branch name the new workspace will own
/// and must not be reserved (`workspace_names::check_workspace_name`).
fn check_workspace_name(name: &str) -> Result<(), AppError> {
    crate::services::workspace_names::check_workspace_name(name).map_err(Into::into)
}

fn validate_request(request: &ForkWorkspaceRequest) -> Result<(), AppError> {
    if request.idempotency_key.is_empty()
        || request.idempotency_key.trim() != request.idempotency_key
        || request.idempotency_key.len() > 200
    {
        return Err(validation(
            "idempotencyKey",
            "Idempotency key must be 1-200 characters without surrounding whitespace",
        ));
    }
    check_workspace_name(&request.name)?;
    if let Some(chat) = &request.start_chat {
        if chat.content.trim().is_empty() || chat.content.len() > MAX_START_CHAT_BYTES {
            return Err(validation(
                "startChat.content",
                "The first message must be non-empty and at most 512 KiB",
            ));
        }
        if let Some(preferences) = &chat.launch_preferences {
            preferences.validate().map_err(AppError::from_error)?;
        }
    }
    if let Some(issue) = &request.issue {
        let reference = issue.reference.trim();
        if reference.is_empty() || reference.chars().count() > MAX_ISSUE_REFERENCE_CHARS {
            return Err(validation(
                "issue.ref",
                "Issue reference must be a GitHub issue URL or owner/name#number",
            ));
        }
    }
    Ok(())
}

/// The request identity a fork retry must reproduce (g5.fork.replay). The
/// current digest is v2; the v1 digests encoded the removed `stages` field,
/// so a record admitted before the phase cutover — pending, completed or
/// aborted — keeps matching through both v1 forms (one and two stages) and a
/// lost-response retry keeps its terminal outcome (g5.fork.replay.terminal).
#[derive(Debug, Clone)]
struct ForkRequestDigests {
    current: String,
    accepted: Vec<String>,
}

impl ForkRequestDigests {
    fn accepts(&self, digest: &str) -> bool {
        self.accepted.iter().any(|candidate| candidate == digest)
    }
}

fn request_digests(
    source_workspace_id: Uuid,
    request: &ForkWorkspaceRequest,
) -> Result<ForkRequestDigests, AppError> {
    let current = request_digest(source_workspace_id, request, None)?;
    let accepted = vec![
        current.clone(),
        request_digest(source_workspace_id, request, Some(1))?,
        request_digest(source_workspace_id, request, Some(2))?,
    ];
    Ok(ForkRequestDigests { current, accepted })
}

/// `legacy_stages` selects the v1 encoding of a record admitted before the
/// phase cutover; `None` is the current v2 digest without `stages`.
fn request_digest(
    source_workspace_id: Uuid,
    request: &ForkWorkspaceRequest,
    legacy_stages: Option<i32>,
) -> Result<String, AppError> {
    let mut payload = match legacy_stages {
        // The exact v1 shape, `stages` included, so pre-cutover records match.
        Some(stages) => serde_json::json!({
            "version": 1,
            "source_workspace_id": source_workspace_id,
            "name": request.name,
            "description": request.description,
            "read_only": request.read_only,
            "stages": stages,
            "merge_target_workspace_id": request.merge_target_workspace_id,
            "chat_session_ids": request.chat_session_ids,
            "source": request.source,
        }),
        None => serde_json::json!({
            "version": 2,
            "source_workspace_id": source_workspace_id,
            "name": request.name,
            "description": request.description,
            "read_only": request.read_only,
            "merge_target_workspace_id": request.merge_target_workspace_id,
            "chat_session_ids": request.chat_session_ids,
            "source": request.source,
        }),
    };
    // The accepted issue input, not the fetched issue: a retry needs no
    // GitHub call to match, and a changed issue or role is a different
    // request. Omitted when absent so issue-less digests are unchanged.
    if let Some(issue) = &request.issue {
        payload["issue"] = serde_json::json!({
            "reference": issue.reference.trim(),
            "role": issue.role.as_str(),
        });
    }
    // Omitted unless set so digests of exact-name forks are unchanged.
    if request.uniquify_name {
        payload["uniquify_name"] = serde_json::json!(true);
    }
    // Omitted when absent so digests of ordinary forks are unchanged.
    if let Some(kind) = request.reverification {
        payload["reverification"] = serde_json::json!(kind.as_str());
    }
    // Omitted when absent so digests of forks without a chat are unchanged.
    if let Some(chat) = &request.start_chat {
        payload["start_chat"] = serde_json::json!({ "content": chat.content });
        // Omitted when absent so digests of earlier chat forks are unchanged.
        if let Some(preferences) = &chat.launch_preferences {
            payload["start_chat"]["launch_preferences"] =
                serde_json::to_value(preferences).map_err(internal_json)?;
        }
    }
    let canonical = serde_json::to_vec(&payload).map_err(internal_json)?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

fn source_repo_to_manifest(
    request: &ForkWorkspaceRequest,
    target_phase: &str,
    source: SourceRepoRow,
) -> Result<ForkRepoManifest, AppError> {
    let source_branch = source.source_branch.ok_or_else(|| {
        validation(
            "source",
            format!("Repository {} has no source branch", source.repo_id),
        )
    })?;
    let source_oid = source.source_oid.ok_or_else(|| {
        validation(
            "source",
            format!("Repository {} has no committed source OID", source.repo_id),
        )
    })?;
    if source_oid.len() != 40 || !source_oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(validation(
            "source",
            format!("Repository {} has an invalid source OID", source.repo_id),
        ));
    }
    Ok(ForkRepoManifest {
        repo_id: source.repo_id,
        repo_name: source.repo_name,
        git_url: source.git_url,
        provider: source.provider,
        root_path: source.root_path,
        role: source.role,
        source_branch,
        source_oid: source_oid.to_ascii_lowercase(),
        target_branch: request.name.clone(),
        target_description: request.description.clone(),
        target_read_only: request.read_only,
        target_merge_target_workspace_id: request.merge_target_workspace_id,
        target_phase: target_phase.to_string(),
        carry_session_ids: request.chat_session_ids.clone(),
        source_kind: request.source,
        live_interrupt: request.interrupt,
        fork_copy: None,
    })
}

fn parse_manifest(operation: &ForkOperationRow) -> Result<Vec<ForkRepoManifest>, AppError> {
    serde_json::from_value(operation.repo_manifest.clone()).map_err(internal_json)
}

async fn lock_repo_rows(
    conn: &mut PgConnection,
    org_id: Uuid,
    repo_ids: &[Uuid],
) -> Result<(), AppError> {
    let mut expected = repo_ids.to_vec();
    expected.sort_unstable();
    expected.dedup();
    let locked = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM repos
        WHERE org_id = $1 AND id = ANY($2)
        ORDER BY id
        FOR UPDATE
        "#,
    )
    .bind(org_id)
    .bind(&expected)
    .fetch_all(conn)
    .await?;
    if locked != expected {
        return Err(validation(
            "source",
            "Fork repository membership changed while the operation was admitted",
        ));
    }
    Ok(())
}

async fn load_operation_in_tx(
    conn: &mut PgConnection,
    org_id: Uuid,
    requested_by: Uuid,
    idempotency_key: &str,
) -> Result<Option<ForkOperationRow>, AppError> {
    Ok(sqlx::query_as::<_, ForkOperationRow>(
        r#"
        SELECT id, org_id, source_workspace_id, target_workspace_id,
               requested_by, owner_user_id, idempotency_key, request_digest,
               proposed_name, status, repo_manifest, source_flush_id, created_at,
               completed_at, error
        FROM workspace_forks
        WHERE org_id = $1 AND requested_by = $2 AND idempotency_key = $3
        "#,
    )
    .bind(org_id)
    .bind(requested_by)
    .bind(idempotency_key)
    .fetch_optional(conn)
    .await?)
}

async fn load_matching_active_operation_in_tx(
    conn: &mut PgConnection,
    org_id: Uuid,
    source_workspace_id: Uuid,
    requested_by: Uuid,
    digests: &ForkRequestDigests,
) -> Result<Option<ForkOperationRow>, AppError> {
    Ok(sqlx::query_as::<_, ForkOperationRow>(
        r#"
        SELECT id, org_id, source_workspace_id, target_workspace_id,
               requested_by, owner_user_id, idempotency_key, request_digest,
               proposed_name, status, repo_manifest, source_flush_id, created_at,
               completed_at, error
        FROM workspace_forks
        WHERE org_id = $1 AND source_workspace_id = $2 AND requested_by = $3
          AND request_digest = ANY($4) AND status IN ('pending', 'aborting')
        ORDER BY created_at, id
        LIMIT 1
        "#,
    )
    .bind(org_id)
    .bind(source_workspace_id)
    .bind(requested_by)
    .bind(&digests.accepted)
    .fetch_optional(conn)
    .await?)
}

async fn lock_operation_status(
    conn: &mut PgConnection,
    operation_id: Uuid,
) -> Result<String, AppError> {
    sqlx::query_scalar("SELECT status FROM workspace_forks WHERE id = $1 FOR UPDATE")
        .bind(operation_id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| not_found("workspace fork operation"))
}

fn branch_names_conflict(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

async fn normalize_fork_receipt(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    target_branch: &str,
    receipt: FlushRepoReceipt,
) -> Result<FlushRepoReceipt, AppError> {
    let path = repo_storage.repo_path(org_id, receipt.repo_id)?;
    let local_branch_oid = receipt.local_branch_oid.clone();
    let snapshot_oid = receipt.snapshot_oid.clone();
    let dirty_paths = tokio::task::spawn_blocking(move || {
        GitRepo::open(&path)?.diff_name_only_between_commits(&local_branch_oid, &snapshot_oid)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Fork receipt normalization task failed: {error}"),
    })??;
    Ok(normalize_fork_receipt_metadata(
        receipt,
        target_branch,
        dirty_paths,
    ))
}

fn normalize_fork_receipt_metadata(
    mut receipt: FlushRepoReceipt,
    target_branch: &str,
    dirty_paths: Vec<String>,
) -> FlushRepoReceipt {
    receipt.branch_oid = receipt.local_branch_oid.clone();
    receipt.local_head_oid = receipt.local_branch_oid.clone();
    receipt.head_ref = Some(format!("refs/heads/{target_branch}"));
    receipt.sequencer = SequencerState::None;
    receipt.dirty = receipt.snapshot_oid != receipt.local_branch_oid;
    receipt.dirty_paths = dirty_paths;
    receipt
}

fn fork_copy_refs(member: &ForkRepoManifest) -> Option<ForkCopyRefs> {
    member.fork_copy.as_ref().map(|copy| ForkCopyRefs {
        workspace_id: copy.target_workspace_id,
        flush_id: copy.target_flush_id,
        snapshot_oid: copy.receipt.snapshot_oid.clone(),
    })
}

fn runtime_app_error(error: RuntimeRpcError) -> AppError {
    match error.message.as_str() {
        "WORKSPACE_NOT_RUNNING" => G5Error::WorkspaceNotRunning.into(),
        "WORKSPACE_STOPPING" | "WORKSPACE_BUSY" => G5Error::Conflict {
            reason: error.message,
        }
        .into(),
        _ if error.code == 503 => G5Error::ServiceUnavailable {
            message: error.message,
        }
        .into(),
        _ => G5Error::Conflict {
            reason: error.message,
        }
        .into(),
    }
}

fn terminal_fork_error(error: &AppError) -> bool {
    match &error.inner {
        G5Error::Conflict { reason } => {
            reason != "WORKSPACE_BUSY" && reason != "WORKSPACE_STOPPING"
        }
        G5Error::ValidationError { .. } | G5Error::GitError { .. } | G5Error::NotFound { .. } => {
            true
        }
        _ => false,
    }
}

fn fork_resume_busy() -> AppError {
    G5Error::ServiceUnavailable {
        message: "Workspace fork is still in progress; retry the same request".to_string(),
    }
    .into()
}

fn aborted_fork_error(operation: &ForkOperationRow) -> AppError {
    G5Error::Conflict {
        reason: operation
            .error
            .as_ref()
            .and_then(|error| error.get("message"))
            .and_then(|message| message.as_str())
            .unwrap_or("The previous fork attempt was aborted")
            .to_string(),
    }
    .into()
}

fn fork_resume_lock_error(error: sqlx::Error) -> AppError {
    if error
        .as_database_error()
        .and_then(|database| database.code())
        .as_deref()
        == Some("55P03")
    {
        fork_resume_busy()
    } else {
        error.into()
    }
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
        == Some("23505")
}

/// Whether a new fork may take `name` in these repositories: nobody holds it
/// in the organization (`workspace_names::name_holder`: a current name, a
/// reserved former name or a pending fork claim), no pending fork in a shared
/// repository claims a name one is a path prefix of, and no workspace,
/// including a deleted one, recorded the branch in one of the repositories.
async fn name_is_available(
    conn: &mut PgConnection,
    org_id: Uuid,
    name: &str,
    repo_ids: &[Uuid],
) -> Result<bool, AppError> {
    if crate::services::workspace_names::name_holder(&mut *conn, org_id, name, None)
        .await?
        .is_some()
    {
        return Ok(false);
    }
    let prefix_claimed = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM workspace_forks AS fork
            WHERE fork.org_id = $1
              AND fork.status IN ('pending', 'aborting')
              AND EXISTS (
                  SELECT 1
                  FROM jsonb_array_elements(fork.repo_manifest) AS claimed
                  WHERE (claimed ->> 'repo_id')::uuid = ANY($3)
              )
              AND (
                  starts_with(lower(fork.proposed_name), lower($2) || '/')
                  OR starts_with(lower($2), lower(fork.proposed_name) || '/')
              )
        )
        "#,
    )
    .bind(org_id)
    .bind(name)
    .bind(repo_ids)
    .fetch_one(&mut *conn)
    .await?;
    Ok(!prefix_claimed && !branch_name_owned(conn, repo_ids, name).await?)
}

/// Suggested names get at most this many numbered alternatives.
const MAX_NAME_SUFFIX: u32 = 50;

/// `base`, or the first free `base-2`, `base-3`, … that is a valid name.
async fn choose_available_name(
    conn: &mut PgConnection,
    org_id: Uuid,
    base: &str,
    repo_ids: &[Uuid],
) -> Result<String, AppError> {
    for n in 1..=MAX_NAME_SUFFIX {
        let candidate = if n == 1 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        if check_workspace_name(&candidate).is_err() {
            break;
        }
        if name_is_available(conn, org_id, &candidate, repo_ids).await? {
            return Ok(candidate);
        }
    }
    Err(name_unavailable())
}

/// Whether a workspace member owns this branch name in one of the repositories.
/// Completed deletion removes both membership and Git refs, releasing the name.
/// Base owns the repository's default branch even when its workspace name differs.
async fn branch_name_owned(
    conn: &mut sqlx::PgConnection,
    repo_ids: &[Uuid],
    name: &str,
) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM workspace_repos m
             JOIN workspaces w ON w.id=m.workspace_id AND w.org_id=m.org_id
             JOIN repos r ON r.id=m.repo_id AND r.org_id=m.org_id
             WHERE m.repo_id = ANY($1)
               AND lower(CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END) = lower($2))",
    )
    .bind(repo_ids)
    .bind(name)
    .fetch_one(conn)
    .await?)
}

fn name_unavailable() -> AppError {
    validation(
        "name",
        "Workspace name is unavailable in one or more repositories",
    )
}

fn validation(field: &str, message: impl Into<String>) -> AppError {
    G5Error::ValidationError {
        details: serde_json::json!({
            "field": field,
            "message": message.into(),
        }),
    }
    .into()
}

fn forbidden() -> AppError {
    G5Error::Forbidden {
        missing_scopes: Vec::new(),
    }
    .into()
}

fn not_found(resource: impl Into<String>) -> AppError {
    G5Error::NotFound {
        resource: resource.into(),
    }
    .into()
}

fn internal_json(error: serde_json::Error) -> AppError {
    G5Error::Internal {
        message: format!("Failed to encode workspace fork state: {error}"),
    }
    .into()
}

async fn commit_db(db: ScopedDb) -> Result<(), AppError> {
    crate::db::durable_commit::commit_durable_scope(db)
        .await
        .map_err(Into::into)
}

async fn rollback_db(db: ScopedDb) -> Result<(), AppError> {
    db.rollback().await.map_err(|error| {
        G5Error::Internal {
            message: format!("Failed to close workspace fork database scope: {error}"),
        }
        .into()
    })
}

/// The fork's tenant scope, for native issue reads during admission.
struct ForkTenantDb<'a>(&'a WorkspaceForkService);

#[async_trait::async_trait]
impl external_issues::TenantDbOpener for ForkTenantDb<'_> {
    async fn open(&self) -> Result<ScopedDb, G5Error> {
        self.0.open_db().await.map_err(|error| error.inner)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_committed_fork_rows_still_load() {
        let source: super::ForkSource = serde_json::from_str("\"committed\"").unwrap();
        assert_eq!(source, super::ForkSource::LegacyCommitted);
        assert!(!source.captures_flush_receipt());
        assert_eq!(serde_json::to_string(&source).unwrap(), "\"committed\"");
    }

    use super::*;

    #[test]
    fn branch_conflicts_are_literal_and_path_aware() {
        assert!(branch_names_conflict("feature", "feature"));
        assert!(branch_names_conflict("feature", "feature/x"));
        assert!(branch_names_conflict("feature/x", "feature"));
        assert!(!branch_names_conflict("feature", "feature-x"));
        assert!(!branch_names_conflict("literal_%", "literal_ax"));
    }

    #[test]
    fn exact_branch_validation_does_not_trim_or_rewrite() {
        let valid = ForkWorkspaceRequest {
            name: "feature/exact_name".to_string(),
            description: None,
            idempotency_key: "request-1".to_string(),
            read_only: false,
            merge_target_workspace_id: None,
            chat_session_ids: Vec::new(),
            source: ForkSource::Durable,
            interrupt: false,
            issue: None,
            start_chat: None,
            uniquify_name: false,
            reverification: None,
        };
        validate_request(&valid).unwrap();
        for invalid in [" feature", "feature ", "feature..x", "feature.lock", "@"] {
            let mut request = valid.clone();
            request.name = invalid.to_string();
            assert!(validate_request(&request).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn manifest_phase_is_frozen_at_admission_and_defaults_for_older_manifests() {
        // g5.workbench.phases.state / g5.fork.replay: the admitted phase rides
        // the manifest; a manifest persisted before the field existed decodes
        // to `be` instead of failing a resume.
        let mut json = serde_json::json!({
            "repo_id": Uuid::new_v4(),
            "repo_name": "g5",
            "git_url": "https://example.test/g5.git",
            "provider": null,
            "root_path": "",
            "role": "code",
            "source_branch": "main",
            "source_oid": "0123456789abcdef0123456789abcdef01234567",
            "target_branch": "feature",
            "target_description": null,
            "target_read_only": false,
            "target_merge_target_workspace_id": null,
            "source_kind": "durable",
            "live_interrupt": false,
            "fork_copy": null,
        });
        let legacy: ForkRepoManifest = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(legacy.target_phase, "be");

        json["target_phase"] = serde_json::json!("se");
        let admitted: ForkRepoManifest = serde_json::from_value(json).unwrap();
        assert_eq!(admitted.target_phase, "se");
        let round_trip = serde_json::to_value(&admitted).unwrap();
        assert_eq!(round_trip["target_phase"], serde_json::json!("se"));
    }

    #[test]
    fn digest_changes_with_client_input_but_not_source_refresh() {
        let source = Uuid::new_v4();
        let request = ForkWorkspaceRequest {
            name: "feature/exact".to_string(),
            description: Some("one".to_string()),
            idempotency_key: "request-1".to_string(),
            read_only: false,
            merge_target_workspace_id: None,
            chat_session_ids: Vec::new(),
            source: ForkSource::Durable,
            interrupt: false,
            issue: None,
            start_chat: None,
            uniquify_name: false,
            reverification: None,
        };
        let first = request_digest(source, &request, None).unwrap();
        assert_eq!(first, request_digest(source, &request, None).unwrap());
        let mut interrupt_retry = request.clone();
        interrupt_retry.interrupt = true;
        assert_eq!(
            first,
            request_digest(source, &interrupt_retry, None).unwrap()
        );
        let mut changed = request;
        changed.name = "feature/other".to_string();
        assert_ne!(first, request_digest(source, &changed, None).unwrap());
    }

    #[test]
    fn digest_covers_the_requested_issue_input() {
        let source = Uuid::new_v4();
        let without = ForkWorkspaceRequest {
            name: "fix/issue".to_string(),
            description: None,
            idempotency_key: "request-1".to_string(),
            read_only: false,
            merge_target_workspace_id: None,
            chat_session_ids: Vec::new(),
            source: ForkSource::Durable,
            interrupt: false,
            issue: None,
            start_chat: None,
            uniquify_name: false,
            reverification: None,
        };
        let with_issue = |reference: &str, role| ForkWorkspaceRequest {
            issue: Some(ForkIssueRequest {
                reference: reference.to_string(),
                role,
            }),
            ..without.clone()
        };
        let base = request_digest(source, &without, None).unwrap();
        // A suggested name is a different request from the exact name.
        let suggested = ForkWorkspaceRequest {
            uniquify_name: true,
            ..without.clone()
        };
        assert_ne!(base, request_digest(source, &suggested, None).unwrap());
        // A first message is part of the request; its text changes identity.
        let with_chat = |content: &str| ForkWorkspaceRequest {
            start_chat: Some(ForkStartChatRequest {
                content: content.to_string(),
                launch_preferences: None,
            }),
            ..without.clone()
        };
        let chat = request_digest(source, &with_chat("Fix the login bug."), None).unwrap();
        assert_ne!(base, chat);
        assert_ne!(
            chat,
            request_digest(source, &with_chat("Fix the signup bug."), None).unwrap()
        );
        assert!(validate_request(&with_chat("  ")).is_err());
        assert!(validate_request(&with_chat("Fix the login bug.")).is_ok());
        let primary =
            request_digest(source, &with_issue("acme/api#12", LinkRole::Primary), None).unwrap();
        assert_ne!(base, primary);
        assert_eq!(
            primary,
            request_digest(
                source,
                &with_issue("  acme/api#12 ", LinkRole::Primary),
                None
            )
            .unwrap()
        );
        assert_ne!(
            primary,
            request_digest(source, &with_issue("acme/api#12", LinkRole::Related), None).unwrap()
        );
        assert_ne!(
            primary,
            request_digest(source, &with_issue("acme/api#13", LinkRole::Primary), None).unwrap()
        );

        // Issue-less requests keep the v2 digest shape without `stages`.
        let current = hex::encode(Sha256::digest(
            serde_json::to_vec(&serde_json::json!({
                "version": 2,
                "source_workspace_id": source,
                "name": without.name,
                "description": without.description,
                "read_only": without.read_only,
                "merge_target_workspace_id": without.merge_target_workspace_id,
                "chat_session_ids": without.chat_session_ids,
                "source": without.source,
            }))
            .unwrap(),
        ));
        assert_eq!(base, current);

        validate_request(&with_issue("acme/api#12", LinkRole::Primary)).unwrap();
        for reference in ["", "   ", &"x".repeat(MAX_ISSUE_REFERENCE_CHARS + 1)] {
            assert!(
                validate_request(&with_issue(reference, LinkRole::Primary)).is_err(),
                "accepted {reference:?}"
            );
        }
    }

    #[test]
    fn replay_accepts_the_v1_digests_of_records_admitted_before_the_phase_cutover() {
        // g5.fork.replay.terminal: a fork admitted with the stage-model digest
        // (one or two stages) keeps matching its lost-response retry, while
        // the current identity is the v2 digest without `stages`.
        let source = Uuid::new_v4();
        let request = ForkWorkspaceRequest {
            name: "feature/replay".to_string(),
            description: None,
            idempotency_key: "request-1".to_string(),
            read_only: false,
            merge_target_workspace_id: None,
            chat_session_ids: Vec::new(),
            source: ForkSource::Durable,
            interrupt: false,
            issue: None,
            start_chat: None,
            uniquify_name: false,
            reverification: None,
        };
        let digests = request_digests(source, &request).unwrap();
        let v1 = |stages: i32| {
            hex::encode(Sha256::digest(
                serde_json::to_vec(&serde_json::json!({
                    "version": 1,
                    "source_workspace_id": source,
                    "name": request.name,
                    "description": request.description,
                    "read_only": request.read_only,
                    "stages": stages,
                    "merge_target_workspace_id": request.merge_target_workspace_id,
                    "chat_session_ids": request.chat_session_ids,
                    "source": request.source,
                }))
                .unwrap(),
            ))
        };
        assert!(digests.accepts(&digests.current));
        assert!(digests.accepts(&v1(1)));
        assert!(digests.accepts(&v1(2)));
        assert_ne!(digests.current, v1(1));
        assert!(!digests.accepts(&"0".repeat(64)));
        let mut other = request.clone();
        other.name = "feature/other".to_string();
        assert!(!digests.accepts(&request_digests(source, &other).unwrap().current));
    }

    #[test]
    fn retryable_member_ref_failures_keep_the_operation_claimed() {
        let unavailable = AppError::from_error(G5Error::ServiceUnavailable {
            message: "repository temporarily unavailable".to_string(),
        });
        assert!(!terminal_fork_error(&unavailable));

        for reason in ["WORKSPACE_BUSY", "WORKSPACE_STOPPING"] {
            let busy = AppError::from_error(G5Error::Conflict {
                reason: reason.to_string(),
            });
            assert!(!terminal_fork_error(&busy));
        }

        let invalid = validation("name", "invalid branch name");
        assert!(terminal_fork_error(&invalid));
    }

    #[test]
    fn fork_receipt_attaches_child_branch_and_clears_source_sequencer() {
        let branch_oid = "1".repeat(40);
        let head_oid = "2".repeat(40);
        let snapshot_oid = "3".repeat(40);
        let receipt = FlushRepoReceipt {
            repo_id: Uuid::new_v4(),
            branch_oid: "4".repeat(40),
            local_branch_oid: branch_oid.clone(),
            local_head_oid: head_oid,
            head_ref: None,
            sequencer: SequencerState::Rebase,
            snapshot_oid: snapshot_oid.clone(),
            dirty: true,
            dirty_paths: vec!["old-only.txt".to_string()],
        };

        let normalized = normalize_fork_receipt_metadata(
            receipt,
            "exact-child",
            vec!["all-child-state.txt".to_string()],
        );
        assert_eq!(normalized.branch_oid, branch_oid);
        assert_eq!(normalized.local_head_oid, normalized.local_branch_oid);
        assert_eq!(
            normalized.head_ref.as_deref(),
            Some("refs/heads/exact-child")
        );
        assert_eq!(normalized.sequencer, SequencerState::None);
        assert!(normalized.dirty);
        assert_eq!(normalized.snapshot_oid, snapshot_oid);
        assert_eq!(normalized.dirty_paths, ["all-child-state.txt"]);
    }
}
