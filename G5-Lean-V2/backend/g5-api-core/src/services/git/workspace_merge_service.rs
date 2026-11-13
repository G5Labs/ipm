use super::merge_override::{apply_override, MergeOverrideIntent};
use crate::errors::G5Error;
use crate::services::branch_commit::{
    acquire_workspace_commit_fence, map_branch_commit_error, prepare_branch_commit,
    PreparedBranchCommit,
};
use crate::services::git::repo_store::{ScopedRepoStorage, TenantRepoStorage};
use crate::services::workspace_ops::terminalize_active_merge_runs_with_tombstoned_sides;
use crate::services::workspace_reader::{resolve_committed_snapshot, resolve_published_snapshot};
use chrono::Utc;
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::types::CommittedBindingSnapshot;
use g5_contracts::types::{AuthorityTarget, MergeSourceHead, WorkspaceHeadRepoInput};
use g5_scm::{BranchMergeAnalysisOutcome, GitAuthor, GitRepo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;
const REJECT_ACTIVE_WORKSPACE_MERGE_PAIR_SQL: &str = r#"
            SELECT jmr.id
            FROM workspace_merge_runs jmr
            WHERE jmr.status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
              AND jmr.source_kind = 'workspace'
              AND (
                    jmr.workspace_id = $1
                 OR jmr.source_workspace_id = $1
                 OR jmr.target_workspace_id = $1
                 OR jmr.workspace_id = $2
                 OR jmr.source_workspace_id = $2
                 OR (
                        jmr.advances_target = true
                    AND jmr.target_workspace_id = $2
                 )
              )
            LIMIT 1
            "#;
const REJECT_LIVE_ACTIVE_WORKSPACE_MERGE_EXECUTION_SQL: &str = r#"
            SELECT jmr.id
            FROM workspace_merge_runs jmr
            WHERE jmr.status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
              AND jmr.source_kind = 'workspace'
              AND (
                    jmr.workspace_id = $1
                 OR jmr.source_workspace_id = $1
                 OR jmr.target_workspace_id = $1
                 OR jmr.workspace_id = $2
                 OR jmr.source_workspace_id = $2
                 OR (
                        jmr.advances_target = true
                    AND jmr.target_workspace_id = $2
                 )
              )
              AND NOT EXISTS (
                    SELECT 1
                    FROM workspaces w
                    WHERE w.deleted_at IS NOT NULL
                      AND w.id IN (
                            jmr.workspace_id,
                            jmr.source_workspace_id,
                            jmr.target_workspace_id
                      )
              )
            LIMIT 1
            "#;
fn map_active_workspace_merge_lock_error(
    err: sqlx::Error,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
) -> G5Error {
    match &err {
        sqlx::Error::Database(
            db_err,
        ) if matches!(db_err.code().as_deref(), Some("23505")) => {
            G5Error::Conflict {
                reason: format!(
                    "Another merge is already active for source workspace {source_workspace_id} or target workspace {target_workspace_id}"
                ),
            }
        }
        _ => G5Error::from(err),
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMergeRunSummary {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source_kind: String,
    pub source_workspace_id: Option<Uuid>,
    pub target_workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_task_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_session_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_task_status: Option<String>,

    pub source_branch_name: String,
    pub status: String,
    pub phase: String,
    pub blocking_reason: Option<String>,
    /// Per-repo child row this run is bound to. Populated for
    /// `source_kind = 'remote_tracking'`; `None` for workspace
    /// merges that don't carry a per-repo handle.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repo_id: Option<Uuid>,
    /// `{ workspace_tip, mirror_tip }` snapshot captured under the
    /// per-branch lock at divergence time. Populated only for
    /// `source_kind = 'remote_tracking'`; the merge-required UI surfaces
    /// the two tips alongside the baseline.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_heads: Option<serde_json::Value>,
    /// `{ baseline }` for `source_kind = 'remote_tracking'`. The baseline
    /// is the synchronization watermark — the last commit both sides
    /// agreed on.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub target_heads: Option<serde_json::Value>,
    /// Wall-clock at run open. Newest-first ordering uses this column.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The merge admission this target-advancing run landed under
    /// (g5.governance.approval.requirements): the Integration Engineering
    /// requirement states against the pinned source revision, as recorded
    /// before any provider mutation. Absent for source-only rebases.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub approval_admission: Option<MergeAdmissionEvidence>,
}
#[must_use = "transfer authority_advance to a canonical finalizer or commit the unchanged transaction"]
pub struct CompletedWorkspaceHeadAdvance {
    pub run: WorkspaceMergeRunSummary,
    pub authority_advance: Option<PreparedBranchCommit>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparedWorkspaceMergeKind {
    TargetAdvance,
    SourceRebase,
}
/// Immutable Git and operation preparation for one direct workspace merge.
///
/// This value is produced before the route opens its application transaction.
/// Applying it takes the workspace commit fences, revalidates both exact cuts,
/// records the completed merge run. After
/// [`WorkspaceMergeService::apply_prepared_workspace_merge`], the caller lands the
/// cut in that same transaction, so the run and the refs commit together and
/// a refused landing leaves no merge run behind to block a retry.
pub struct PreparedWorkspaceMergeApplication {
    context: WorkspaceMergePreflightContext,
    preflight: MergePreflightResponse,
    confirmed_by: Uuid,
    run_id: Uuid,
    source_branch_name: String,
    authority_advance: PreparedBranchCommit,
    kind: PreparedWorkspaceMergeKind,
    remote_branches: std::collections::BTreeMap<Uuid, String>,
    /// Pinned admission evidence of a target-advancing merge; `None` for a
    /// source-only rebase, which approvals never gate.
    approval_admission: Option<MergeAdmissionEvidence>,
}
impl PreparedWorkspaceMergeApplication {
    pub fn preflight(&self) -> &MergePreflightResponse {
        &self.preflight
    }

    pub fn approval_admission(&self) -> Option<&MergeAdmissionEvidence> {
        self.approval_admission.as_ref()
    }
}
/// How a source conflict resolution is admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionAdmission {
    /// The actor's running owner attachment, frozen while the task queues
    /// (`g5.git.conflicts-rebase`).
    RunningOwner,
    /// The actor's Start consent under way, recorded or joined by this
    /// request; the queued brief is bound to it by the caller
    /// (`g5.git.stopped-rebase.resolve-with-agent`).
    PendingStart { intent_id: Uuid },
}
pub enum PreparedSourceWorkspaceMerge {
    Direct(Box<PreparedWorkspaceMergeApplication>),
    WorkflowRequired(Box<PreparedWorkspaceMergePreflight>),
}
pub enum PreparedLocalWorkspaceMerge {
    Ready(Box<PreparedWorkspaceMergeApplication>),
    Blocked(MergePreflightResponse),
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkspaceMergeRequest {
    pub target_workspace_id: Uuid,
}
fn is_rebase_replay_conflict(err: &G5Error) -> bool {
    matches!(err, G5Error::MergeConflict { .. })
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MergePreflightDecision {
    FastForwardOrNoOp,
    CleanMergeNoGitConflicts,
    GitConflicts,
    WorkflowRequired,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MergePreflightRepoOutcome {
    Identical,
    SourceAlreadyMerged,
    FastForward,
    CleanMerge,
    GitConflicts,
    WorkflowRequired,
}
/// How a stored `WorkflowRequired` preflight outcome gates coordinated Create-PR
/// work — both source publication
/// (`@ref software_architecture/g5-api/services/workspace_service#D44 `) and
/// provider PR create/update
/// (`@ref software_architecture/g5-api/services/coordinated_repo_publication#D13 `)
/// — for one repo. Single source of truth so the two halves of Create PR cannot
/// diverge: target-side-only drift is advisory, source-side staleness or
/// unreadability blocks (`@ref core_workflow/merge_workflow#D31 `, issue #869).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRequiredGate {
    /// The stored outcome is not `WorkflowRequired`; no workflow-derived gate
    /// applies.
    NotBlocked,
    /// The stored source snapshot still matches the actual source head and only
    /// the target branch moved. Advisory (merge_workflow D31 /
    /// coordinated_repo_publication D13, issue #869): source publication
    /// proceeds and the provider PR proceeds against the current remote target
    /// branch, with the stale local target snapshot surfaced as advisory
    /// evidence rather than an `action_required` skip.
    TargetOnlyDrift,
    /// The source side is stale or unreadable — a source-head mismatch, or
    /// missing actual-head evidence (unresolved refs / git analysis failure).
    /// Keeps blocking semantics for both halves of Create PR.
    SourceBlocked,
}
/// How a workspace-merge preflight is being used.
/// Evidence capture (PR create/refresh) takes no execution gates. Execution
/// callers share dirty-head gates and the advancement-aware active-run guard:
/// sources and target writers remain exclusive, while unrelated source-only
/// target readers do not block one another or target publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMergePreflightMode {
    EvidenceCapture,
    /// PR continuation over pushed refs, excluding retained dirty snapshots.
    PublishedEvidenceCapture,
    SourceRebaseExecution,
    /// Source rebase whose cuts are the *published* branch heads of both
    /// workspaces rather than their retained flush receipts: the review
    /// workspace reopen sync, which must bring in what the source pushed
    /// even while the source VM runs (its receipt lags) and must never
    /// replay a stopped source's unpushed snapshot.
    PublishedSourceRebaseExecution,
    TargetMergeExecution,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WorkspaceMergeAdmissionResource {
    Participation,
    TargetWriter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceMergeAdmissionMode {
    Shared,
    Exclusive,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceMergeAdmissionRequest {
    workspace_id: Uuid,
    resource: WorkspaceMergeAdmissionResource,
    mode: WorkspaceMergeAdmissionMode,
}

/// One workspace's captured member heads: repository, branch and head.
pub type PreflightCut = (Uuid, Vec<(Uuid, String, String)>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreflightRepoResult {
    pub repo_id: Uuid,
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Uuid,
    pub source_branch_name: String,
    pub target_branch_name: String,
    pub source_head: String,
    pub target_head: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_source_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_target_head: Option<String>,
    pub outcome: MergePreflightRepoOutcome,
    pub conflict_paths: Vec<String>,
    pub message: Option<String>,
}
impl MergePreflightRepoResult {
    /// Classify how this repo's stored `WorkflowRequired` evidence gates
    /// coordinated Create-PR work. Blocks only when the *source* side is stale
    /// or unreadable — a source-head mismatch, or missing actual-head evidence
    /// (unresolved refs / git analysis failure). Drift confined to the target
    /// branch is advisory (`@ref core_workflow/merge_workflow#D31 `, issue
    /// #869), so the same target-only drift that lets the provider PR open also
    /// lets the source branch publish. A non-`WorkflowRequired` outcome is
    /// `NotBlocked`.
    pub fn workflow_required_gate(&self) -> WorkflowRequiredGate {
        if !matches!(self.outcome, MergePreflightRepoOutcome::WorkflowRequired) {
            return WorkflowRequiredGate::NotBlocked;
        }
        let target_only_drift = self
            .actual_source_head
            .as_deref()
            .is_some_and(|actual| actual == self.source_head)
            && self
                .actual_target_head
                .as_deref()
                .is_some_and(|actual| actual != self.target_head);
        if target_only_drift {
            WorkflowRequiredGate::TargetOnlyDrift
        } else {
            WorkflowRequiredGate::SourceBlocked
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreflightResponse {
    pub source_kind: String,
    pub source_workspace_id: Option<Uuid>,
    pub target_workspace_id: Option<Uuid>,
    pub decision: MergePreflightDecision,
    pub can_direct_merge: bool,
    pub preflight_fingerprint: String,
    pub repo_results: Vec<MergePreflightRepoResult>,
    pub message: String,
}
/// A provider-merge import whose captured source, target or bindings moved
/// before preparation. The target's own auto-pull can land the accepted
/// provider commit between the two captures; a fresh capture then proves it.
const PROVIDER_IMPORT_CUT_CHANGED: &str =
    "Workspace source, target, or remote bindings changed after provider acceptance; retry import";

/// Whether `error` only reports a provider-merge import cut that moved, so a
/// fresh capture may succeed.
#[must_use]
pub fn is_provider_import_cut_changed(error: &G5Error) -> bool {
    matches!(error, G5Error::Conflict { reason } if reason == PROVIDER_IMPORT_CUT_CHANGED)
}
/// One member of an accepted in-VM source rebase.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRebaseMember {
    pub repo_id: Uuid,
    pub previous_head: String,
    pub head: String,
    /// False when the member already contained the target (a no-op).
    pub published: bool,
}
/// A source rebase the owner VM executed and G5 accepted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRebaseOutcome {
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Uuid,
    pub vm_id: Uuid,
    pub members: Vec<WorkspaceRebaseMember>,
}
pub enum SourceRebaseResult {
    Rebased(WorkspaceRebaseOutcome),
    Resolution(crate::services::git::git_conflict_resolution::ConflictResolutionTask),
}
/// The owner's next step for a member the VM refused to rebase.
fn source_rebase_refusal(code: &str, repository: &str, accepted: usize) -> String {
    let action = match code {
        "DIRTY_WORKING_TREE" | "STATUS_UNAVAILABLE" => {
            "has uncommitted changes in the workspace; save or discard them, then retry"
        }
        "LOCAL_HISTORY_DIVERGED" => {
            "has workspace commits that conflict with newer G5 commits; resolve them in the workspace, then retry"
        }
        "INTERRUPTED_SEQUENCER" => {
            "has a Git merge or rebase in progress in the workspace; finish or abort it, then retry"
        }
        "DETACHED_HEAD" | "BRANCH_ASSOCIATION_MISMATCH" => {
            "is not on the workspace branch in the workspace; switch back to it, then retry"
        }
        "INTERNAL_BRANCH_MOVED" | "TARGET_MOVED" => "changed during the rebase; retry",
        "PUSH_FAILED" => {
            "was rebased in the workspace but G5 did not accept the result; retry to publish it"
        }
        _ => "could not be rebased in the workspace; retry",
    };
    let kept = if accepted > 0 {
        format!(
            " {accepted} other repositor{} already rebased and kept.",
            if accepted == 1 { "y was" } else { "ies were" }
        )
    } else {
        String::new()
    };
    format!("Rebase stopped: {repository} {action} ({code}).{kept}")
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectCleanWorkspaceMergeRequest {
    pub target_workspace_id: Uuid,
    pub preflight_fingerprint: String,
}
/// One Integration Engineering requirement as the merge admission saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAdmissionRequirement {
    pub definition_id: Uuid,
    pub definition_name: String,
    pub definition_version: i32,
    /// `approved` (every required actor holds an eligible approval against
    /// the pinned revision), `rejected`, or `pending` (missing, withdrawn or
    /// stale against the pinned revision).
    pub state: String,
}

/// Durable result of one merge admission evaluation
/// (g5.governance.approval.requirements, g5.governance.merge): only the
/// Integration Engineering requirements attached to the configured target
/// gate the merge, evaluated against the pinned source head vector. Recorded
/// on the pull request before any provider mutation and copied onto the
/// merge run at local application, which re-reads it instead of evaluating
/// again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAdmissionEvidence {
    pub schema_version: u8,
    pub evaluated_at: chrono::DateTime<chrono::Utc>,
    pub source_workspace_id: Uuid,
    pub target_workspace_id: Uuid,
    /// The pinned source head vector the requirements were evaluated against.
    pub source_heads: Vec<MergeSourceHead>,
    pub admitted: bool,
    /// Why the merge is refused, when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub requirements: Vec<MergeAdmissionRequirement>,
    /// The override that admitted this cut despite its blockers
    /// (g5.workspace.merge.admin-override.formal); absent for an ordinary
    /// admission and in schema version 1 evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_override: Option<super::merge_override::MergeOverrideRecord>,
}

/// Version 2 adds [`MergeAdmissionEvidence::merge_override`]; version 1
/// evidence reads as having none.
pub const MERGE_ADMISSION_SCHEMA_VERSION: u8 = 2;

/// Approval policy's answer to whether a pinned cut may merge into its
/// target, taken in the merging transaction. Git records the evidence on the
/// pull request and the merge run; the approvals context owns the
/// evaluation and is installed here at composition.
#[async_trait::async_trait]
pub trait MergeAdmissionGate: Send + Sync {
    /// Retain the attached requirement set when it grants source-owner merge
    /// authority. Does not lock decisions or re-evaluate a recorded admission.
    async fn lock_requirement_set(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error>;

    /// Take the policy locks for the target's requirement set and every
    /// attached definition. Transaction-scoped: the transaction that pins the
    /// admission evidence must hold them.
    async fn lock(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error>;

    /// Evaluate the pinned source head vector against the target's
    /// requirements. Takes no locks.
    async fn evaluate(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        pinned_source_heads: &[MergeSourceHead],
    ) -> Result<MergeAdmissionEvidence, G5Error>;
}

#[derive(Clone)]
pub struct WorkspaceMergeService {
    repo_storage: TenantRepoStorage,
    merge_admission: Arc<dyn MergeAdmissionGate>,
}
struct SourceBranchRow {
    workspace_id: Uuid,
    repo_id: Uuid,
    branch_name: String,
    head_commit: String,
}
#[derive(Debug, Clone, sqlx::FromRow)]
struct TargetBranchRow {
    workspace_id: Uuid,
    repo_id: Uuid,
    branch_name: String,
    head_commit: String,
}
#[derive(sqlx::FromRow)]
struct WorkspaceMergeWorkspaceRow {
    id: Uuid,
    org_id: Uuid,
    /// The branch the workspace owns; a rename never changes it, so it is the
    /// identity a merge revalidates (`g5.workspace.branch`). Base has none.
    branch_name: Option<String>,
}
#[derive(Clone)]
struct DirectMergePlan {
    source_workspace_id: Uuid,
    repo_id: Uuid,
    target_workspace_id: Uuid,
    target_branch_name: String,
    source_branch_name: String,
    source_head: String,
    target_head: String,
}
struct WorkspaceMergePreflightContext {
    /// The store narrowed to the capturing transaction's scope
    /// (`g5.repo.storage-tenancy`); both workspaces are in its organization.
    repos: ScopedRepoStorage,
    source_workspace: WorkspaceMergeWorkspaceRow,
    target_workspace: WorkspaceMergeWorkspaceRow,
    source_branches: Vec<SourceBranchRow>,
    source_heads: serde_json::Value,
    target_heads: serde_json::Value,
    branch_pairs: Vec<DirectMergePlan>,
    /// Cuts were resolved from published branch heads (see
    /// `WorkspaceMergePreflightMode::PublishedSourceRebaseExecution`);
    /// revalidation must resolve them the same way.
    published_heads: bool,
}
/// One complete workspace merge cut captured before Git classification.
///
/// Callers may inspect or consume the classification, but only this service can
/// exact-revalidate the private captured context before database application.
pub struct PreparedWorkspaceMergePreflight {
    context: WorkspaceMergePreflightContext,
    mode: WorkspaceMergePreflightMode,
    response: MergePreflightResponse,
}
impl PreparedWorkspaceMergePreflight {
    pub fn response(&self) -> &MergePreflightResponse {
        &self.response
    }
    pub fn into_response(self) -> MergePreflightResponse {
        self.response
    }
}
struct DirectCleanPublicationPlan {
    repo_id: Uuid,
    expected_old_head: String,
    new_head: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceRebasePlan {
    repo_id: Uuid,
    source_branch_name: String,
    expected_old_head: String,
    new_head: String,
}
enum SourceRebasePlanOutcome {
    Direct(Vec<SourceRebasePlan>),
    NeedsWorkflow,
}
impl WorkspaceMergeService {
    pub fn new(
        repo_storage: TenantRepoStorage,
        merge_admission: Arc<dyn MergeAdmissionGate>,
    ) -> Self {
        Self {
            repo_storage,
            merge_admission,
        }
    }

    pub async fn active_source_resolution(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<
        Option<crate::services::git::git_conflict_resolution::ConflictResolutionTask>,
        crate::errors::AppError,
    > {
        crate::services::git::git_conflict_resolution::active_workspace_resolution(
            db,
            source_workspace_id,
            target_workspace_id,
        )
        .await
    }

    /// Queue the resolution task for the actor's running owner attachment
    /// (`g5.git.conflicts-rebase`).
    pub async fn enqueue_source_resolution(
        &self,
        state: &crate::services::AppState,
        db: &mut ScopedDb,
        prepared: PreparedWorkspaceMergePreflight,
        actor_user_id: Uuid,
        agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
    ) -> Result<
        crate::services::git::git_conflict_resolution::ConflictResolutionTask,
        crate::errors::AppError,
    > {
        self.enqueue_source_resolution_with(
            state,
            db,
            prepared,
            actor_user_id,
            agent_launch,
            ResolutionAdmission::RunningOwner,
        )
        .await
    }

    /// Queue the resolution task under the given admission. With a pending
    /// Start (`g5.git.stopped-rebase.resolve-with-agent`) the caller binds the
    /// returned `input_id` to that consent in the same transaction, so the
    /// brief is withdrawn with the Start if it ends before compute is ready.
    pub async fn enqueue_source_resolution_with(
        &self,
        state: &crate::services::AppState,
        db: &mut ScopedDb,
        prepared: PreparedWorkspaceMergePreflight,
        actor_user_id: Uuid,
        agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
        admission: ResolutionAdmission,
    ) -> Result<
        crate::services::git::git_conflict_resolution::ConflictResolutionTask,
        crate::errors::AppError,
    > {
        use crate::services::git::git_conflict_resolution::{
            self, ResolutionPurpose, ResolutionRepository, ResolutionRequest,
        };
        let source_id = prepared.context.source_workspace.id;
        let target_id = prepared.context.target_workspace.id;
        acquire_workspace_commit_fence(db.conn(), source_id).await?;
        // A racing/retried request must return the original task before
        // checking newly changed Git inputs or creating a second session.
        if let Some(task) = self
            .active_source_resolution(db, source_id, target_id)
            .await?
        {
            return Ok(task);
        }
        match admission {
            ResolutionAdmission::RunningOwner => {
                let handle = state
                    .workspace_runtime
                    .owner_handle(source_id, actor_user_id)
                    .await?;
                state
                    .workspace_runtime
                    .lock_owner_handle(db, &handle)
                    .await?;
            }
            ResolutionAdmission::PendingStart { intent_id } => {
                state
                    .workspace_runtime
                    .lock_owner_pending_start(db, source_id, actor_user_id, intent_id)
                    .await?;
            }
        }
        crate::services::identity::workspace_mutation_policy::ensure_workspace_mutable_by(
            db,
            source_id,
            actor_user_id,
            "workspaces.merge_resolution",
        )
        .await?;
        self.revalidate_transaction_free_workspace_merge_preflight(db, &prepared)
            .await?;
        // Unreadable or stale Git is not a content conflict an agent can fix.
        if let Some(repo) = prepared
            .response
            .repo_results
            .iter()
            .find(|repo| repo.outcome == MergePreflightRepoOutcome::WorkflowRequired)
        {
            return Err(G5Error::Conflict {
                reason: repo.message.clone().unwrap_or_else(|| {
                    "Workspace merge inputs are unavailable; refresh and retry".into()
                }),
            }
            .into());
        }
        let snapshot = self
            .resolve_merge_snapshot(db, source_id, prepared.context.published_heads)
            .await?;
        let repositories = prepared
            .context
            .branch_pairs
            .iter()
            .map(|pair| {
                let member = snapshot
                    .repo(pair.repo_id)
                    .map_err(|reason| G5Error::Conflict {
                        reason: reason.to_string(),
                    })?;
                Ok(ResolutionRepository {
                    repo_id: pair.repo_id,
                    root_path: member.root_path.clone(),
                    local_branch: pair.source_branch_name.clone(),
                    local_tip: pair.source_head.clone(),
                    remote_tip: pair.target_head.clone(),
                    expected_pins: None,
                })
            })
            .collect::<Result<Vec<_>, G5Error>>()?;
        git_conflict_resolution::enqueue(
            state,
            db,
            ResolutionRequest {
                id: Uuid::new_v4(),
                org_id: prepared.context.source_workspace.org_id,
                workspace_id: source_id,
                actor_user_id,
                repositories,
                purpose: ResolutionPurpose::Workspace {
                    target_workspace_id: target_id,
                },
                agent_launch,
            },
        )
        .await
    }
    /// Rebase a source workspace in its running owner VM
    /// (`g5.architecture.workspace_rebase`). Clean replay, fast-forward and
    /// no-op all execute there, never centrally behind the checkout; Git
    /// conflicts hand off to the existing resolution task in the same VM.
    /// Each member publishes with its captured G5 head as the lease, so a
    /// completed rebase is one G5 accepted. A refusal reports the members
    /// already accepted, which a retry treats as no-ops.
    pub async fn rebase_source_in_vm(
        &self,
        state: &crate::services::AppState,
        scope: &ScopedDbReopener,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        actor_user_id: Uuid,
        agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
    ) -> Result<SourceRebaseResult, crate::errors::AppError> {
        let handle = state
            .workspace_runtime
            .owner_handle(source_workspace_id, actor_user_id)
            .await?;
        let mode = WorkspaceMergePreflightMode::SourceRebaseExecution;
        // Base is never a rebase source, whichever owner would execute it.
        let mut db = scope.open().await?;
        self.reject_base_source_workspace(&mut db, source_workspace_id)
            .await?;
        let prepared = self
            .prepare_transaction_free_workspace_merge_preflight(
                db,
                source_workspace_id,
                target_workspace_id,
                mode,
            )
            .await?;
        if matches!(
            prepared.response.decision,
            MergePreflightDecision::GitConflicts | MergePreflightDecision::WorkflowRequired
        ) {
            return self
                .resolve_source_in_vm(state, scope, prepared, actor_user_id, agent_launch)
                .await;
        }
        let mut members = Vec::new();
        let mut conflicted = false;
        for pair in &prepared.context.branch_pairs {
            let report = state
                .workspace_runtime
                .workspace_changes_rebase(
                    &handle,
                    actor_user_id,
                    pair.repo_id,
                    &pair.source_head,
                    &pair.target_branch_name,
                    &pair.target_head,
                )
                .await?;
            let member = &report["members"][0];
            if member["rebased"] == true {
                members.push(WorkspaceRebaseMember {
                    repo_id: pair.repo_id,
                    previous_head: pair.source_head.clone(),
                    head: member["head"].as_str().unwrap_or_default().to_owned(),
                    published: member["published"] == true,
                });
                continue;
            }
            let code = member["errors"][0].as_str().unwrap_or("REBASE_FAILED");
            if code == "REBASE_CONFLICT" {
                conflicted = true;
                continue;
            }
            let mut db = scope.open().await?;
            let root_path: Option<String> = sqlx::query_scalar(
                "SELECT root_path FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2",
            )
            .bind(source_workspace_id)
            .bind(pair.repo_id)
            .fetch_optional(db.conn())
            .await
            .map_err(G5Error::from)?;
            db.rollback().await?;
            return Err(G5Error::Conflict {
                reason: source_rebase_refusal(
                    code,
                    root_path.as_deref().unwrap_or("a repository"),
                    members.len(),
                ),
            }
            .into());
        }
        if conflicted {
            // Accepted members stay accepted; a fresh cut hands the rest to
            // the resolution task in this VM.
            let fresh = self
                .prepare_transaction_free_workspace_merge_preflight(
                    scope.open().await?,
                    source_workspace_id,
                    target_workspace_id,
                    mode,
                )
                .await?;
            return self
                .resolve_source_in_vm(state, scope, fresh, actor_user_id, agent_launch)
                .await;
        }
        Ok(SourceRebaseResult::Rebased(WorkspaceRebaseOutcome {
            source_workspace_id,
            target_workspace_id,
            vm_id: handle.vm_id,
            members,
        }))
    }

    async fn resolve_source_in_vm(
        &self,
        state: &crate::services::AppState,
        scope: &ScopedDbReopener,
        prepared: PreparedWorkspaceMergePreflight,
        actor_user_id: Uuid,
        agent_launch: Option<&g5_contracts::types::AgentLaunchSelectionV1>,
    ) -> Result<SourceRebaseResult, crate::errors::AppError> {
        let mut db = scope.open().await?;
        let task = self
            .enqueue_source_resolution(state, &mut db, prepared, actor_user_id, agent_launch)
            .await?;
        db.commit().await?;
        Ok(SourceRebaseResult::Resolution(task))
    }
    pub async fn prepare_direct_clean_workspace_merge(
        &self,
        mut db: ScopedDb,
        source_workspace_id: Uuid,
        confirmed_by: Uuid,
        req: DirectCleanWorkspaceMergeRequest,
    ) -> Result<PreparedWorkspaceMergeApplication, G5Error> {
        let context = self
            .load_workspace_merge_preflight_context(
                &mut db,
                source_workspace_id,
                req.target_workspace_id,
                WorkspaceMergePreflightMode::TargetMergeExecution,
            )
            .await?;
        self.reject_remote_direct_clean_target(&mut db, req.target_workspace_id)
            .await?;
        // Direct target-advancing merges are admitted by the same evaluator
        // as pull requests (g5.governance.approval.requirements); the
        // evidence rides the prepared application to the merge run.
        let admission = self.admit_target_merge(&mut db, &context, None).await?;
        let scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to close direct-clean workspace merge capture transaction: {error}"
            ),
        })?;
        let preflight = self.build_workspace_merge_preflight_response(&context);
        self.validate_direct_clean_preflight(&preflight, &req.preflight_fingerprint)?;
        let prepared = self
            .prepare_target_workspace_merge_application(
                &scope,
                context,
                preflight,
                confirmed_by,
                admission,
            )
            .await?;
        // Test-only rendezvous between the Git preparation and the landing,
        // keyed by the target: the window in which the target's owner can
        // change before the route re-authorizes it under the landing's locks.
        #[cfg(feature = "test-helpers")]
        crate::services::git_test_barrier::clone_merge_test_barrier::wait(req.target_workspace_id)
            .await;
        Ok(prepared)
    }

    /// The pinned source head vector of a captured merge cut, in the shape
    /// approval evidence binds to (g5.governance.approval.version).
    fn pinned_source_heads(context: &WorkspaceMergePreflightContext) -> Vec<MergeSourceHead> {
        context
            .source_branches
            .iter()
            .map(|branch| MergeSourceHead {
                repo_id: branch.repo_id.to_string(),
                branch_name: branch.branch_name.clone(),
                workspace_id: branch.workspace_id.to_string(),
                head_commit: branch.head_commit.clone(),
            })
            .collect()
    }

    /// Lock the target's requirement set and evaluate merge admission for
    /// the captured cut, refusing the merge while any Integration
    /// Engineering requirement is unsatisfied or stale. An override waives
    /// exactly the blockers it was shown, under the same locks
    /// (g5.workspace.merge.admin-override.formal).
    async fn admit_target_merge(
        &self,
        db: &mut ScopedDb,
        context: &WorkspaceMergePreflightContext,
        merge_override: Option<&MergeOverrideIntent>,
    ) -> Result<MergeAdmissionEvidence, G5Error> {
        let org_id = context.source_workspace.org_id;
        self.merge_admission
            .lock(db, org_id, context.target_workspace.id)
            .await?;
        let mut admission = self
            .merge_admission
            .evaluate(
                db,
                org_id,
                context.source_workspace.id,
                context.target_workspace.id,
                &Self::pinned_source_heads(context),
            )
            .await?;
        if let Some(intent) = merge_override {
            apply_override(&mut admission, intent)?;
            if let Some(record) = admission.merge_override.as_ref() {
                self.require_current_override_authority(
                    db,
                    context.source_workspace.id,
                    context.target_workspace.id,
                    record,
                )
                .await?;
            }
        }
        Self::require_admitted(&admission)?;
        Ok(admission)
    }

    /// Re-admit a confirmed waiver while retaining its requirement-set facts
    /// through the same admission owner used for ordinary merging.
    pub(super) async fn require_current_override_authority(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        record: &super::merge_override::MergeOverrideRecord,
    ) -> Result<(), G5Error> {
        super::merge_override::require_current_override_authority(
            self.merge_admission.as_ref(),
            db,
            source_workspace_id,
            target_workspace_id,
            record,
        )
        .await
    }

    /// Evaluate merge admission for a source cut without taking the
    /// requirement locks: what a merge pinning that cut would see now. The
    /// override preview shows it (g5.workspace.merge.admin-override.formal);
    /// the merge itself re-evaluates under the locks.
    pub async fn evaluate_merge_admission(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        source_heads: &[MergeSourceHead],
    ) -> Result<MergeAdmissionEvidence, G5Error> {
        self.merge_admission
            .evaluate(
                db,
                org_id,
                source_workspace_id,
                target_workspace_id,
                source_heads,
            )
            .await
    }

    /// Admit a pull request merge for the pinned cut of an already captured
    /// preflight, taking the requirement locks in the caller's transaction so
    /// the evidence it records is consistent with the requirement set.
    pub async fn admit_prepared_pull_request_merge(
        &self,
        db: &mut ScopedDb,
        prepared: &PreparedWorkspaceMergePreflight,
        merge_override: Option<&MergeOverrideIntent>,
    ) -> Result<MergeAdmissionEvidence, G5Error> {
        self.admit_target_merge(db, &prepared.context, merge_override)
            .await
    }

    fn require_admitted(admission: &MergeAdmissionEvidence) -> Result<(), G5Error> {
        if admission.admitted {
            return Ok(());
        }
        Err(G5Error::Conflict {
            reason: admission
                .reason
                .clone()
                .unwrap_or_else(|| "Merge admission refused".to_string()),
        })
    }

    /// Recorded admission evidence is reused at local application only for
    /// the exact source revision it was evaluated against.
    fn require_admission_for_cut(
        admission: &MergeAdmissionEvidence,
        context: &WorkspaceMergePreflightContext,
    ) -> Result<(), G5Error> {
        Self::require_admitted(admission)?;
        let mut pinned = Self::pinned_source_heads(context);
        pinned.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
        let mut recorded = admission.source_heads.clone();
        recorded.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
        if admission.source_workspace_id != context.source_workspace.id
            || admission.target_workspace_id != context.target_workspace.id
            || pinned != recorded
        {
            return Err(G5Error::Conflict {
                reason: "Merge admission evidence is for another source revision; retry the merge"
                    .to_string(),
            });
        }
        Ok(())
    }
    /// Prepare a source update without advancing the target workspace.
    pub async fn prepare_source_workspace_merge(
        &self,
        db: ScopedDb,
        source_workspace_id: Uuid,
        confirmed_by: Uuid,
        req: &StartWorkspaceMergeRequest,
    ) -> Result<PreparedSourceWorkspaceMerge, G5Error> {
        self.prepare_source_workspace_merge_with_mode(
            db,
            source_workspace_id,
            confirmed_by,
            req,
            WorkspaceMergePreflightMode::SourceRebaseExecution,
        )
        .await
    }
    /// `prepare_source_workspace_merge` with an explicit source-rebase mode
    /// (`SourceRebaseExecution` or `PublishedSourceRebaseExecution`).
    pub async fn prepare_source_workspace_merge_with_mode(
        &self,
        mut db: ScopedDb,
        source_workspace_id: Uuid,
        confirmed_by: Uuid,
        req: &StartWorkspaceMergeRequest,
        mode: WorkspaceMergePreflightMode,
    ) -> Result<PreparedSourceWorkspaceMerge, G5Error> {
        if !matches!(
            mode,
            WorkspaceMergePreflightMode::SourceRebaseExecution
                | WorkspaceMergePreflightMode::PublishedSourceRebaseExecution
        ) {
            return Err(G5Error::Internal {
                message: "source workspace merge requires a source-rebase preflight mode"
                    .to_string(),
            });
        }
        self.reject_base_source_workspace(&mut db, source_workspace_id)
            .await?;
        let context = self
            .load_workspace_merge_preflight_context(
                &mut db,
                source_workspace_id,
                req.target_workspace_id,
                mode,
            )
            .await?;
        let scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close source workspace merge capture transaction: {error}"),
        })?;
        let preflight = self.build_workspace_merge_preflight_response(&context);
        match preflight.decision {
            MergePreflightDecision::FastForwardOrNoOp
            | MergePreflightDecision::CleanMergeNoGitConflicts => {
                match self.prepare_source_rebase_plans(&context, &preflight)? {
                    SourceRebasePlanOutcome::Direct(plans) => {
                        let prepared = self
                            .prepare_source_workspace_merge_application(
                                &scope,
                                context,
                                preflight,
                                confirmed_by,
                                plans,
                            )
                            .await?;
                        #[cfg(feature = "test-helpers")]
                        crate::services::git_test_barrier::clone_merge_test_barrier::wait(
                            source_workspace_id,
                        )
                        .await;
                        Ok(PreparedSourceWorkspaceMerge::Direct(Box::new(prepared)))
                    }
                    SourceRebasePlanOutcome::NeedsWorkflow => {
                        Ok(PreparedSourceWorkspaceMerge::WorkflowRequired(Box::new(
                            PreparedWorkspaceMergePreflight {
                                context,
                                mode,
                                response: preflight,
                            },
                        )))
                    }
                }
            }
            MergePreflightDecision::GitConflicts | MergePreflightDecision::WorkflowRequired => {
                Ok(PreparedSourceWorkspaceMerge::WorkflowRequired(Box::new(
                    PreparedWorkspaceMergePreflight {
                        context,
                        mode,
                        response: preflight,
                    },
                )))
            }
        }
    }
    pub async fn prepare_transaction_free_workspace_merge_preflight(
        &self,
        mut db: ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        mode: WorkspaceMergePreflightMode,
    ) -> Result<PreparedWorkspaceMergePreflight, G5Error> {
        let context = self
            .load_workspace_merge_preflight_context(
                &mut db,
                source_workspace_id,
                target_workspace_id,
                mode,
            )
            .await?;
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to close workspace merge preflight capture transaction: {error}"
            ),
        })?;
        let response = self.build_workspace_merge_preflight_response(&context);
        Ok(PreparedWorkspaceMergePreflight {
            context,
            mode,
            response,
        })
    }
    pub async fn revalidate_transaction_free_workspace_merge_preflight(
        &self,
        db: &mut ScopedDb,
        prepared: &PreparedWorkspaceMergePreflight,
    ) -> Result<(), G5Error> {
        let current = self
            .load_workspace_merge_preflight_context(
                db,
                prepared.context.source_workspace.id,
                prepared.context.target_workspace.id,
                prepared.mode,
            )
            .await?;
        if current.source_workspace.id != prepared.context.source_workspace.id
            || current.source_workspace.org_id != prepared.context.source_workspace.org_id
            || current.source_workspace.branch_name != prepared.context.source_workspace.branch_name
            || current.target_workspace.id != prepared.context.target_workspace.id
            || current.target_workspace.org_id != prepared.context.target_workspace.org_id
            || current.target_workspace.branch_name != prepared.context.target_workspace.branch_name
            || current.source_heads != prepared.context.source_heads
            || current.target_heads != prepared.context.target_heads
        {
            return Err(G5Error::Conflict {
                reason: "Workspace merge cut changed after preflight; retry".to_string(),
            });
        }
        Ok(())
    }
    /// The preflight's source and target vectors, which are read without the
    /// commit fences and persisted as PR evidence, in workspace order.
    #[must_use]
    pub fn preflight_cuts(prepared: &PreparedWorkspaceMergePreflight) -> Vec<PreflightCut> {
        let results = &prepared.response().repo_results;
        let mut cuts = vec![
            (
                prepared.context.source_workspace.id,
                results
                    .iter()
                    .map(|repo| {
                        (
                            repo.repo_id,
                            repo.source_branch_name.clone(),
                            repo.source_head.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            (
                prepared.context.target_workspace.id,
                results
                    .iter()
                    .map(|repo| {
                        (
                            repo.repo_id,
                            repo.target_branch_name.clone(),
                            repo.target_head.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        ];
        cuts.sort_by_key(|(workspace_id, _)| *workspace_id);
        cuts
    }

    /// Check that each captured vector is one committed cut
    /// (`g5.fork.source.cut`). Both fences are taken in the caller's
    /// transaction, in workspace order like every two-fence path, as its last
    /// locks (after the PR row), and held until it commits.
    pub async fn verify_committed_cuts(
        &self,
        db: &mut ScopedDb,
        cuts: &[PreflightCut],
        operation: &str,
    ) -> Result<(), G5Error> {
        for (workspace_id, heads) in cuts {
            crate::services::workspace_reader::verify_committed_heads(
                db,
                &self.repo_storage,
                *workspace_id,
                heads,
                operation,
            )
            .await?;
        }
        Ok(())
    }

    pub async fn prepare_local_workspace_pull_request_merge(
        &self,
        mut db: ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        confirmed_by: Uuid,
        merge_override: Option<&MergeOverrideIntent>,
    ) -> Result<PreparedLocalWorkspaceMerge, G5Error> {
        self.reject_remote_direct_clean_target(&mut db, target_workspace_id)
            .await?;
        let context = self
            .load_workspace_merge_preflight_context(
                &mut db,
                source_workspace_id,
                target_workspace_id,
                WorkspaceMergePreflightMode::TargetMergeExecution,
            )
            .await?;
        let preflight = self.build_workspace_merge_preflight_response(&context);
        if matches!(
            preflight.decision,
            MergePreflightDecision::GitConflicts | MergePreflightDecision::WorkflowRequired
        ) {
            db.rollback().await.map_err(|error| G5Error::Internal {
                message: format!(
                    "Failed to close local workspace PR merge capture transaction: {error}"
                ),
            })?;
            return Ok(PreparedLocalWorkspaceMerge::Blocked(preflight));
        }
        let admission = self
            .admit_target_merge(&mut db, &context, merge_override)
            .await?;
        let scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to close local workspace PR merge capture transaction: {error}"
            ),
        })?;
        Ok(PreparedLocalWorkspaceMerge::Ready(Box::new(
            self.prepare_target_workspace_merge_application(
                &scope,
                context,
                preflight,
                confirmed_by,
                admission,
            )
            .await?,
        )))
    }
    pub async fn apply_prepared_workspace_merge(
        &self,
        db: &mut ScopedDb,
        prepared: PreparedWorkspaceMergeApplication,
    ) -> Result<CompletedWorkspaceHeadAdvance, G5Error> {
        self.apply_prepared_workspace_merge_application(db, prepared)
            .await
    }

    /// Provider-backed members use the accepted remote commit, even when
    /// GitHub rebased or squashed the source. Local-only members still merge
    /// normally, and all members land in one fenced, compare-and-swap cut.
    pub async fn prepare_provider_workspace_pull_request_merge(
        &self,
        mut db: ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        confirmed_by: Uuid,
        imports: Vec<
            crate::services::git::workspace_pull_request_provider_lifecycle::AcceptedProviderMerge,
        >,
        admission: Option<MergeAdmissionEvidence>,
    ) -> Result<PreparedWorkspaceMergeApplication, G5Error> {
        let context = self
            .load_workspace_merge_preflight_context(
                &mut db,
                source_workspace_id,
                target_workspace_id,
                WorkspaceMergePreflightMode::TargetMergeExecution,
            )
            .await?;
        let bindings = crate::services::git::remote_integrations::remote_branches(
            db.conn(),
            target_workspace_id,
            false,
        )
        .await?;
        // The route pins the admission before the provider merge and hands
        // it here; a caller that reaches this preparation without one (no
        // pull request route in front of it) is admitted now, still before
        // any local application.
        let admission = match admission {
            Some(admission) => admission,
            None => self.admit_target_merge(&mut db, &context, None).await?,
        };
        let scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close provider merge capture: {error}"),
        })?;
        if imports.len() != bindings.len()
            || imports.iter().any(|import| {
                import.workspace_id != target_workspace_id
                    || bindings.get(&import.repo_id) != Some(&import.remote_branch)
                    || !context.branch_pairs.iter().any(|pair| {
                        pair.repo_id == import.repo_id
                            && pair.source_head == import.source_head
                            && pair.target_head == import.expected_target_head
                    })
            })
        {
            return Err(G5Error::Conflict {
                reason: PROVIDER_IMPORT_CUT_CHANGED.into(),
            });
        }
        let preflight = self.build_workspace_merge_preflight_response(&context);
        let local_pairs = context
            .branch_pairs
            .iter()
            .filter(|pair| !bindings.contains_key(&pair.repo_id))
            .cloned()
            .collect::<Vec<_>>();
        let mut plans = self
            .prepare_direct_clean_publication_plans(
                &context.repos,
                context.source_workspace.org_id,
                &local_pairs,
                &preflight.repo_results,
            )
            .await?;
        for import in &imports {
            plans.push(DirectCleanPublicationPlan {
                repo_id: import.repo_id,
                expected_old_head: import.expected_target_head.clone(),
                new_head: import.convergence.as_ref().map_or_else(
                    || import.expected_target_head.clone(),
                    |convergence| convergence.proposed_new_head.clone(),
                ),
            });
        }
        if plans.len() != context.branch_pairs.len() {
            return Err(G5Error::Conflict {
                reason: "Provider merge omitted a repository".into(),
            });
        }
        let repos = plans
            .into_iter()
            .map(|plan| WorkspaceHeadRepoInput {
                repo_id: plan.repo_id,
                expected_old_head: plan.expected_old_head,
                proposed_new_head: plan.new_head,
            })
            .collect();
        let advance = prepare_branch_commit(
            &scope,
            &self.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: target_workspace_id,
            },
            repos,
            "workspace_merge",
        )
        .await
        .map_err(map_branch_commit_error)?;
        let convergence = imports
            .into_iter()
            .filter_map(|import| import.convergence)
            .collect::<Vec<_>>();
        let authority_advance = crate::services::git::remote_integrations::stage_merge_convergence(
            &scope,
            &self.repo_storage,
            advance,
            &convergence,
        )
        .await?;
        let source_branch_name = Self::require_uniform_source_branch_name(
            &context.source_branches,
            "source workspace",
            source_workspace_id,
        )?;
        // The admission recorded before the provider merge is reused for
        // the same pinned cut; it is not evaluated again here.
        Self::require_admission_for_cut(&admission, &context)?;
        Ok(PreparedWorkspaceMergeApplication {
            context,
            preflight,
            confirmed_by,
            run_id: Uuid::new_v4(),
            source_branch_name,
            authority_advance,
            kind: PreparedWorkspaceMergeKind::TargetAdvance,
            remote_branches: bindings,
            approval_admission: Some(admission),
        })
    }

    /// Require the completed target merge associated with this publication.
    /// Direct workspace merges record completion in the landing transaction;
    /// publication uses the merge run settlement and receipt.
    /// Keep the lifecycle representation here so PR finalization depends on
    /// the merge service's guarantee, not its tables or status conventions.
    pub(crate) async fn require_completed_target_merge(
        &self,
        db: &mut ScopedDb,
        advance_id: Uuid,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        let run = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM workspace_merge_runs
             WHERE completion_advance_id = $1
               AND source_workspace_id = $2 AND target_workspace_id = $3
               AND source_kind = 'workspace' AND advances_target
               AND status = 'completed' AND completed_at IS NOT NULL
             FOR SHARE",
        )
        .bind(advance_id)
        .bind(source_workspace_id)
        .bind(target_workspace_id)
        .fetch_optional(db.conn())
        .await?;
        if run.is_none() {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Workspace merge publication {advance_id} has no matching completed target merge"
                ),
            });
        }
        Ok(())
    }

    fn validate_direct_clean_preflight(
        &self,
        preflight: &MergePreflightResponse,
        fingerprint: &str,
    ) -> Result<(), G5Error> {
        if preflight.preflight_fingerprint != fingerprint {
            return Err(G5Error::Conflict {
                reason:
                    "Merge preflight changed before direct-clean publication; run preflight again"
                        .to_string(),
            });
        }
        if preflight.decision != MergePreflightDecision::CleanMergeNoGitConflicts {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Direct clean merge requires cleanMergeNoGitConflicts preflight, got {:?}",
                    preflight.decision
                ),
            });
        }
        Ok(())
    }
    /// The workspace cut a merge reasons about: the retained durable cut by
    /// default, the published branch heads for a published source rebase.
    async fn resolve_merge_snapshot(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        published_heads: bool,
    ) -> Result<CommittedBindingSnapshot, G5Error> {
        let target = AuthorityTarget::Workspace { workspace_id };
        if published_heads {
            resolve_published_snapshot(db, &self.repo_storage, target).await
        } else {
            resolve_committed_snapshot(db, &self.repo_storage, target).await
        }
    }
    async fn load_workspace_merge_preflight_context(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        mode: WorkspaceMergePreflightMode,
    ) -> Result<WorkspaceMergePreflightContext, G5Error> {
        if source_workspace_id == target_workspace_id {
            return Err(G5Error::ValidationError {
                details: serde_json::json!(
                    { "message" : "Source and target workspaces must be different",
                    "source_workspace_id" : source_workspace_id, "target_workspace_id" :
                    target_workspace_id, }
                ),
            });
        }
        let (source_workspace, target_workspace) = self
            .load_workspace_pair_for_merge(db, source_workspace_id, target_workspace_id)
            .await?;
        if source_workspace.org_id != target_workspace.org_id {
            return Err(G5Error::Conflict {
                reason: "Source and target workspaces must belong to the same org".to_string(),
            });
        }
        match mode {
            WorkspaceMergePreflightMode::EvidenceCapture => {}
            WorkspaceMergePreflightMode::PublishedEvidenceCapture => {
                self.reject_active_workspace_merge_execution(
                    db,
                    source_workspace_id,
                    target_workspace_id,
                )
                .await?;
            }
            WorkspaceMergePreflightMode::SourceRebaseExecution
            | WorkspaceMergePreflightMode::PublishedSourceRebaseExecution => {
                self.reject_dirty_workspace_head(db, source_workspace_id, Some("source workspace"))
                    .await?;
                self.reject_active_workspace_merge_execution(
                    db,
                    source_workspace_id,
                    target_workspace_id,
                )
                .await?;
            }
            WorkspaceMergePreflightMode::TargetMergeExecution => {
                self.reject_dirty_workspace_head(db, source_workspace_id, Some("source workspace"))
                    .await?;
                self.reject_dirty_workspace_head(db, target_workspace_id, Some("target workspace"))
                    .await?;
                self.reject_active_workspace_merge_execution(
                    db,
                    source_workspace_id,
                    target_workspace_id,
                )
                .await?;
            }
        }
        let published_heads = matches!(
            mode,
            WorkspaceMergePreflightMode::PublishedSourceRebaseExecution
                | WorkspaceMergePreflightMode::PublishedEvidenceCapture
        );
        let source_snapshot = self
            .resolve_merge_snapshot(db, source_workspace_id, published_heads)
            .await?;
        let target_snapshot = self
            .resolve_merge_snapshot(db, target_workspace_id, published_heads)
            .await?;
        let source_repo_ids = source_snapshot
            .repos
            .iter()
            .map(|repo| repo.repo_id)
            .collect::<std::collections::BTreeSet<_>>();
        let target_repo_ids = target_snapshot
            .repos
            .iter()
            .map(|repo| repo.repo_id)
            .collect::<std::collections::BTreeSet<_>>();
        if source_repo_ids != target_repo_ids {
            return Err(G5Error::ValidationError {
                details: serde_json::json!(
                    { "message" :
                    "Source and target workspaces do not contain the same complete repository vector",
                    "source_workspace_id" : source_workspace_id, "target_workspace_id" :
                    target_workspace_id, "source_repo_ids" : source_repo_ids,
                    "target_repo_ids" : target_repo_ids, }
                ),
            });
        }
        let source_branches = source_snapshot
            .repos
            .iter()
            .map(|repo| SourceBranchRow {
                workspace_id: repo.workspace_id,
                repo_id: repo.repo_id,
                branch_name: repo.branch_name.clone(),
                head_commit: repo.head_commit.clone(),
            })
            .collect::<Vec<_>>();
        let mut target_rows = Vec::with_capacity(source_branches.len());
        let mut branch_pairs = Vec::with_capacity(source_branches.len());
        for source_branch in &source_branches {
            let target_member = target_snapshot
                .repo(source_branch.repo_id)
                .map_err(|_| {
                    G5Error::ValidationError {
                        details: serde_json::json!(
                            { "message" :
                            "Source and target workspace authority vectors do not contain the same repositories",
                            "source_workspace_id" : source_workspace_id,
                            "target_workspace_id" : target_workspace_id, "repo_id" :
                            source_branch.repo_id, }
                        ),
                    }
                })?;
            let target = TargetBranchRow {
                workspace_id: target_member.workspace_id,
                repo_id: target_member.repo_id,
                branch_name: target_member.branch_name.clone(),
                head_commit: target_member.head_commit.clone(),
            };
            branch_pairs.push(DirectMergePlan {
                source_workspace_id: source_branch.workspace_id,
                repo_id: source_branch.repo_id,
                target_workspace_id: target.workspace_id,
                target_branch_name: target.branch_name.clone(),
                source_branch_name: source_branch.branch_name.clone(),
                source_head: source_branch.head_commit.clone(),
                target_head: target.head_commit.clone(),
            });
            target_rows.push(target);
        }
        let source_heads = serde_json::Value::Array(
            source_branches
                .iter()
                .map(|row| {
                    serde_json::json!(
                        { "workspaceId" : row.workspace_id, "repoId" : row.repo_id,
                        "branchName" : row.branch_name,
                        "headCommit" : row.head_commit, }
                    )
                })
                .collect(),
        );
        let target_heads = serde_json::Value::Array(
            target_rows
                .iter()
                .map(|row| {
                    serde_json::json!(
                        { "sourceWorkspaceId" : source_workspace_id,
                        "workspaceId" : row.workspace_id, "repoId" : row.repo_id,
                        "branchName" : row.branch_name, "headCommit" : row.head_commit, }
                    )
                })
                .collect(),
        );
        Ok(WorkspaceMergePreflightContext {
            repos: self.repo_storage.scoped(&*db),
            published_heads,
            source_workspace,
            target_workspace,
            source_branches,
            source_heads,
            target_heads,
            branch_pairs,
        })
    }
    fn build_workspace_merge_preflight_response(
        &self,
        context: &WorkspaceMergePreflightContext,
    ) -> MergePreflightResponse {
        let (decision, repo_results) = self.analyze_merge_branch_pairs(
            &context.repos,
            context.source_workspace.org_id,
            &context.branch_pairs,
        );
        Self::finalize_preflight_response(
            "workspace",
            Some(context.source_workspace.id),
            Some(context.target_workspace.id),
            decision,
            repo_results,
        )
    }
    async fn reject_base_source_workspace(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        let is_base: Option<bool> =
            sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id = $1")
                .bind(source_workspace_id)
                .fetch_optional(db.conn())
                .await?;
        match is_base {
            Some(true) => Err(G5Error::ValidationError {
                details: serde_json::json!(
                    { "message" :
                    "Base workspace cannot be rebased as a source workspace",
                    "source_workspace_id" : source_workspace_id, }
                ),
            }),
            Some(false) => Ok(()),
            None => Err(G5Error::NotFound {
                resource: format!("workspace {source_workspace_id}"),
            }),
        }
    }
    /// Direct target merges have no provider PR acceptance to import.
    async fn reject_remote_direct_clean_target(
        &self,
        db: &mut ScopedDb,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        if crate::services::git::remote_integrations::has_bindings(db.conn(), target_workspace_id)
            .await?
        {
            return Err(G5Error::Conflict {
                reason: "This target tracks a remote branch. Open and merge a pull request so the provider accepts the change before G5 imports it.".into(),
            });
        }
        Ok(())
    }
    fn prepare_source_rebase_plans(
        &self,
        context: &WorkspaceMergePreflightContext,
        preflight: &MergePreflightResponse,
    ) -> Result<SourceRebasePlanOutcome, G5Error> {
        let mut plans = Vec::with_capacity(context.branch_pairs.len());
        for pair in &context.branch_pairs {
            let repo_result = preflight
                .repo_results
                .iter()
                .find(|result| {
                    result.repo_id == pair.repo_id
                        && result.source_workspace_id == pair.source_workspace_id
                        && result.target_workspace_id == pair.target_workspace_id
                })
                .ok_or_else(|| G5Error::Internal {
                    message: format!(
                        "missing preflight result for repo {} source branch {}",
                        pair.repo_id, pair.source_workspace_id
                    ),
                })?;
            let new_head = match repo_result.outcome {
                MergePreflightRepoOutcome::Identical | MergePreflightRepoOutcome::FastForward => {
                    pair.source_head.clone()
                }
                MergePreflightRepoOutcome::SourceAlreadyMerged => pair.target_head.clone(),
                MergePreflightRepoOutcome::CleanMerge => {
                    let repo_path = context
                        .repos
                        .repo_path(context.source_workspace.org_id, pair.repo_id)?;
                    let repo = GitRepo::open(&repo_path)?;
                    match repo.rebase_onto(
                        &pair.source_head,
                        &pair.target_head,
                        &pair.source_branch_name,
                        &pair.target_branch_name,
                    ) {
                        Ok(new_head) => new_head,
                        Err(err) if is_rebase_replay_conflict(&err) => {
                            return Ok(SourceRebasePlanOutcome::NeedsWorkflow);
                        }
                        Err(err) => return Err(err),
                    }
                }
                MergePreflightRepoOutcome::GitConflicts => {
                    return Err(G5Error::Conflict {
                        reason: format!(
                            "Source workspace branch {} for repo {} has Git conflicts with target {}",
                            pair.source_branch_name, pair.repo_id, pair
                            .target_branch_name
                        ),
                    });
                }
                MergePreflightRepoOutcome::WorkflowRequired => {
                    return Err(G5Error::Conflict {
                        reason: repo_result
                            .message
                            .clone()
                            .unwrap_or_else(|| {
                                format!(
                                    "Source workspace branch {} for repo {} requires workflow resolution before rebase",
                                    pair.source_branch_name, pair.repo_id
                                )
                            }),
                    });
                }
            };
            plans.push(SourceRebasePlan {
                repo_id: pair.repo_id,
                source_branch_name: pair.source_branch_name.clone(),
                expected_old_head: pair.source_head.clone(),
                new_head,
            });
        }
        Ok(SourceRebasePlanOutcome::Direct(plans))
    }
    fn analyze_merge_branch_pairs(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        branch_pairs: &[DirectMergePlan],
    ) -> (MergePreflightDecision, Vec<MergePreflightRepoResult>) {
        let mut repo_results = Vec::with_capacity(branch_pairs.len());
        let mut has_clean_merge = false;
        let mut has_conflict = false;
        let mut has_workflow_required = false;
        for pair in branch_pairs {
            let mut result = MergePreflightRepoResult {
                repo_id: pair.repo_id,
                source_workspace_id: pair.source_workspace_id,
                target_workspace_id: pair.target_workspace_id,
                source_branch_name: pair.source_branch_name.clone(),
                target_branch_name: pair.target_branch_name.clone(),
                source_head: pair.source_head.clone(),
                target_head: pair.target_head.clone(),
                actual_source_head: None,
                actual_target_head: None,
                outcome: MergePreflightRepoOutcome::WorkflowRequired,
                conflict_paths: Vec::new(),
                message: None,
            };
            let Ok(repo_path) = repos.repo_path(org_id, pair.repo_id) else {
                has_workflow_required = true;
                result.message = Some("local repo path is unavailable".to_string());
                repo_results.push(result);
                continue;
            };
            let repo = match GitRepo::open(&repo_path) {
                Ok(repo) => repo,
                Err(err) => {
                    has_workflow_required = true;
                    result.message = Some(format!("local repo cannot be opened: {err}"));
                    repo_results.push(result);
                    continue;
                }
            };
            let actual_source_head = match repo.resolve_branch_head(&pair.source_branch_name) {
                Ok(head) => head,
                Err(err) => {
                    has_workflow_required = true;
                    result.message = Some(format!("source branch head cannot be resolved: {err}"));
                    repo_results.push(result);
                    continue;
                }
            };
            let actual_target_head = match repo.resolve_branch_head(&pair.target_branch_name) {
                Ok(head) => head,
                Err(err) => {
                    has_workflow_required = true;
                    result.message = Some(format!("target branch head cannot be resolved: {err}"));
                    repo_results.push(result);
                    continue;
                }
            };
            if actual_source_head != pair.source_head || actual_target_head != pair.target_head {
                has_workflow_required = true;
                result.message =
                    Some("database branch snapshots no longer match local git refs".to_string());
                result.actual_source_head = Some(actual_source_head);
                result.actual_target_head = Some(actual_target_head);
                repo_results.push(result);
                continue;
            }
            match repo.analyze_head_merge_no_checkout(&pair.source_head, &pair.target_head) {
                Ok(analysis) => {
                    result.source_head = analysis.source_head;
                    result.target_head = analysis.target_head;
                    match analysis.outcome {
                        BranchMergeAnalysisOutcome::Identical => {
                            result.outcome = MergePreflightRepoOutcome::Identical;
                        }
                        BranchMergeAnalysisOutcome::SourceAlreadyMerged => {
                            result.outcome = MergePreflightRepoOutcome::SourceAlreadyMerged;
                        }
                        BranchMergeAnalysisOutcome::FastForward => {
                            result.outcome = MergePreflightRepoOutcome::FastForward;
                        }
                        BranchMergeAnalysisOutcome::CleanMerge => {
                            has_clean_merge = true;
                            result.outcome = MergePreflightRepoOutcome::CleanMerge;
                        }
                        BranchMergeAnalysisOutcome::Conflicts { paths } => {
                            has_conflict = true;
                            result.outcome = MergePreflightRepoOutcome::GitConflicts;
                            result.conflict_paths = paths;
                        }
                    }
                }
                Err(err) => {
                    has_workflow_required = true;
                    result.message = Some(format!("git merge analysis failed: {err}"));
                }
            }
            repo_results.push(result);
        }
        let decision = if has_conflict {
            MergePreflightDecision::GitConflicts
        } else if has_workflow_required {
            MergePreflightDecision::WorkflowRequired
        } else if has_clean_merge {
            MergePreflightDecision::CleanMergeNoGitConflicts
        } else {
            MergePreflightDecision::FastForwardOrNoOp
        };
        (decision, repo_results)
    }
    fn finalize_preflight_response(
        source_kind: &str,
        source_workspace_id: Option<Uuid>,
        target_workspace_id: Option<Uuid>,
        decision: MergePreflightDecision,
        repo_results: Vec<MergePreflightRepoResult>,
    ) -> MergePreflightResponse {
        let can_direct_merge = decision == MergePreflightDecision::CleanMergeNoGitConflicts;
        let message = match decision {
            MergePreflightDecision::FastForwardOrNoOp => {
                "All repos are no-op, already merged, or fast-forward eligible; existing direct publication can be used."
                    .to_string()
            }
            MergePreflightDecision::CleanMergeNoGitConflicts => {
                "No Git merge conflicts were detected, but semantic risk may remain."
                    .to_string()
            }
            MergePreflightDecision::GitConflicts => {
                "Git merge conflicts were detected; use the full rebase workflow."
                    .to_string()
            }
            MergePreflightDecision::WorkflowRequired => {
                "Preflight could not prove a direct clean merge; use the full rebase workflow."
                    .to_string()
            }
        };
        let preflight_fingerprint = Self::preflight_fingerprint(
            source_kind,
            source_workspace_id,
            target_workspace_id,
            decision,
            &repo_results,
        );
        MergePreflightResponse {
            source_kind: source_kind.to_string(),
            source_workspace_id,
            target_workspace_id,
            decision,
            can_direct_merge,
            preflight_fingerprint,
            repo_results,
            message,
        }
    }
    fn preflight_fingerprint(
        source_kind: &str,
        source_workspace_id: Option<Uuid>,
        target_workspace_id: Option<Uuid>,
        decision: MergePreflightDecision,
        repo_results: &[MergePreflightRepoResult],
    ) -> String {
        let payload = serde_json::json!(
            { "sourceKind" : source_kind,
            "sourceWorkspaceId" : source_workspace_id, "targetWorkspaceId" :
            target_workspace_id, "decision" : decision, "repoResults" : repo_results, }
        );
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        let digest = Sha256::digest(bytes);
        format!("sha256:{}", hex::encode(digest))
    }
    fn direct_clean_merge_author() -> GitAuthor {
        GitAuthor {
            name: "G5 Direct Merge".to_string(),
            email: "g5-direct-merge@g5.local".to_string(),
        }
    }
    fn require_uniform_source_branch_name(
        source_branches: &[SourceBranchRow],
        source_kind: &str,
        source_id: Uuid,
    ) -> Result<String, G5Error> {
        let mut names = source_branches
            .iter()
            .map(|branch| branch.branch_name.as_str());
        let Some(branch_name) = names.next() else {
            return Err(G5Error::ValidationError {
                details: serde_json::json!(
                    { "message" :
                    format!("{source_kind} has no repository authority members"),
                    "source_id" : source_id, }
                ),
            });
        };
        if names.any(|candidate| candidate != branch_name) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!(
                    { "message" :
                    format!("{source_kind} does not have one uniform logical branch name"),
                    "source_id" : source_id, "branch_names" : source_branches.iter()
                    .map(| branch | serde_json::json!({ "repoId" : branch.repo_id,
                    "branchName" : branch.branch_name, })).collect::< Vec < _ >> (), }
                ),
            });
        }
        Ok(branch_name.to_string())
    }
    async fn prepare_direct_clean_publication_plans(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        branch_pairs: &[DirectMergePlan],
        repo_results: &[MergePreflightRepoResult],
    ) -> Result<Vec<DirectCleanPublicationPlan>, G5Error> {
        let author = Self::direct_clean_merge_author();
        let mut publication_plans = Vec::with_capacity(branch_pairs.len());
        for pair in branch_pairs {
            let preflight = repo_results
                .iter()
                .find(|result| {
                    result.repo_id == pair.repo_id
                        && result.source_workspace_id == pair.source_workspace_id
                        && result.target_workspace_id == pair.target_workspace_id
                })
                .ok_or_else(|| G5Error::Conflict {
                    reason: format!(
                        "Missing preflight result for repo {} source branch {}",
                        pair.repo_id, pair.source_branch_name
                    ),
                })?;
            let repo_path = repos.repo_path(org_id, pair.repo_id)?;
            let repo = GitRepo::open(&repo_path)?;
            let new_head = match preflight.outcome {
                MergePreflightRepoOutcome::CleanMerge => repo
                    .create_clean_merge_commit_from_heads_no_checkout(
                        &pair.source_head,
                        &pair.target_head,
                        &pair.source_branch_name,
                        &pair.target_branch_name,
                        &author,
                    )?,
                MergePreflightRepoOutcome::FastForward => {
                    if !repo.is_ancestor_of(&pair.target_head, &pair.source_head)? {
                        return Err(G5Error::Conflict {
                            reason: format!(
                                "Source branch {} for repo {} no longer fast-forwards target {}",
                                pair.source_branch_name, pair.repo_id, pair.target_branch_name
                            ),
                        });
                    }
                    pair.source_head.clone()
                }
                MergePreflightRepoOutcome::Identical => {
                    if pair.source_head != pair.target_head {
                        return Err(G5Error::Conflict {
                            reason: format!(
                                "Preflight identity for repo {} is stale",
                                pair.repo_id
                            ),
                        });
                    }
                    pair.target_head.clone()
                }
                MergePreflightRepoOutcome::SourceAlreadyMerged => {
                    if !repo.is_ancestor_of(&pair.source_head, &pair.target_head)? {
                        return Err(G5Error::Conflict {
                            reason: format!(
                                "Source branch {} for repo {} is no longer already contained in target {}",
                                pair.source_branch_name, pair.repo_id, pair
                                .target_branch_name
                            ),
                        });
                    }
                    pair.target_head.clone()
                }
                MergePreflightRepoOutcome::GitConflicts
                | MergePreflightRepoOutcome::WorkflowRequired => {
                    return Err(G5Error::Conflict {
                        reason: format!(
                            "Repo {} is not eligible for direct clean merge",
                            pair.repo_id
                        ),
                    });
                }
            };
            publication_plans.push(DirectCleanPublicationPlan {
                repo_id: pair.repo_id,
                expected_old_head: pair.target_head.clone(),
                new_head,
            });
        }
        Ok(publication_plans)
    }
    async fn prepare_target_workspace_merge_application(
        &self,
        scope: &ScopedDbReopener,
        context: WorkspaceMergePreflightContext,
        preflight: MergePreflightResponse,
        confirmed_by: Uuid,
        admission: MergeAdmissionEvidence,
    ) -> Result<PreparedWorkspaceMergeApplication, G5Error> {
        Self::require_admission_for_cut(&admission, &context)?;
        let run_id = Uuid::new_v4();
        let source_branch_name = Self::require_uniform_source_branch_name(
            &context.source_branches,
            "source workspace",
            context.source_workspace.id,
        )?;
        let publication_plans = self
            .prepare_direct_clean_publication_plans(
                &context.repos,
                context.source_workspace.org_id,
                &context.branch_pairs,
                &preflight.repo_results,
            )
            .await?;
        if publication_plans.len() != context.source_branches.len() {
            return Err(G5Error::Conflict {
                reason: "direct workspace merge omitted a repository".to_string(),
            });
        }
        let exact_repos = publication_plans
            .iter()
            .map(|plan| WorkspaceHeadRepoInput {
                repo_id: plan.repo_id,
                expected_old_head: plan.expected_old_head.clone(),
                proposed_new_head: plan.new_head.clone(),
            })
            .collect();
        let authority_advance = prepare_branch_commit(
            scope,
            &self.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: context.target_workspace.id,
            },
            exact_repos,
            "workspace_merge",
        )
        .await
        .map_err(map_branch_commit_error)?;
        Ok(PreparedWorkspaceMergeApplication {
            context,
            preflight,
            confirmed_by,
            run_id,
            source_branch_name,
            authority_advance,
            kind: PreparedWorkspaceMergeKind::TargetAdvance,
            remote_branches: std::collections::BTreeMap::new(),
            approval_admission: Some(admission),
        })
    }
    async fn prepare_source_workspace_merge_application(
        &self,
        scope: &ScopedDbReopener,
        context: WorkspaceMergePreflightContext,
        preflight: MergePreflightResponse,
        confirmed_by: Uuid,
        plans: Vec<SourceRebasePlan>,
    ) -> Result<PreparedWorkspaceMergeApplication, G5Error> {
        if plans.len() != context.source_branches.len() {
            return Err(G5Error::Conflict {
                reason: "source workspace rebase omitted a repository".to_string(),
            });
        }
        let source_branch_name = Self::require_uniform_source_branch_name(
            &context.source_branches,
            "source workspace",
            context.source_workspace.id,
        )?;
        let exact_repos = plans
            .iter()
            .map(|plan| WorkspaceHeadRepoInput {
                repo_id: plan.repo_id,
                expected_old_head: plan.expected_old_head.clone(),
                proposed_new_head: plan.new_head.clone(),
            })
            .collect();
        let authority_advance = prepare_branch_commit(
            scope,
            &self.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: context.source_workspace.id,
            },
            exact_repos,
            "workspace_rebase",
        )
        .await
        .map_err(map_branch_commit_error)?;
        Ok(PreparedWorkspaceMergeApplication {
            context,
            preflight,
            confirmed_by,
            run_id: Uuid::new_v4(),
            source_branch_name,
            authority_advance,
            kind: PreparedWorkspaceMergeKind::SourceRebase,
            remote_branches: std::collections::BTreeMap::new(),
            approval_admission: None,
        })
    }
    async fn apply_prepared_workspace_merge_application(
        &self,
        db: &mut ScopedDb,
        prepared: PreparedWorkspaceMergeApplication,
    ) -> Result<CompletedWorkspaceHeadAdvance, G5Error> {
        let PreparedWorkspaceMergeApplication {
            context,
            preflight: _,
            confirmed_by,
            run_id,
            source_branch_name,
            authority_advance,
            kind,
            remote_branches,
            approval_admission,
        } = prepared;
        let advances_target = matches!(kind, PreparedWorkspaceMergeKind::TargetAdvance);
        let publication_workspace_id = if advances_target {
            context.target_workspace.id
        } else {
            context.source_workspace.id
        };
        if authority_advance.workspace_id() != publication_workspace_id {
            return Err(G5Error::Internal {
                message: "prepared workspace merge targets the wrong workspace".to_string(),
            });
        }
        if advances_target {
            let (first, second) = if context.source_workspace.id <= context.target_workspace.id {
                (context.source_workspace.id, context.target_workspace.id)
            } else {
                (context.target_workspace.id, context.source_workspace.id)
            };
            acquire_workspace_commit_fence(db.conn(), first).await?;
            acquire_workspace_commit_fence(db.conn(), second).await?;
            self.revalidate_prepared_workspace_merge_context(db, &context)
                .await?;
            // Under the fences the recorded admission is re-read, never
            // re-evaluated: an approval change after provider acceptance
            // must not strand a merged provider PR (g5.governance.merge).
            let admission = approval_admission
                .as_ref()
                .ok_or_else(|| G5Error::Internal {
                    message: "target-advancing workspace merge carries no admission evidence"
                        .to_string(),
                })?;
            Self::require_admission_for_cut(admission, &context)?;
            if crate::services::git::remote_integrations::remote_branches(
                db.conn(),
                context.target_workspace.id,
                true,
            )
            .await?
                != remote_branches
            {
                return Err(G5Error::Conflict {
                    reason: "Merge target remote bindings changed; use the provider-first pull request merge".into(),
                });
            }
        } else {
            acquire_workspace_commit_fence(db.conn(), context.source_workspace.id).await?;
            self.revalidate_prepared_source_rebase_context(db, &context)
                .await?;
        }
        self.admit_workspace_merge_run(
            db,
            context.source_workspace.id,
            context.target_workspace.id,
            advances_target,
        )
        .await?;
        let now = Utc::now();
        let phase = if advances_target {
            "workspace_merged"
        } else {
            "source_rebased"
        };
        let approval_admission_json = approval_admission
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to serialize merge admission evidence: {error}"),
            })?;
        let final_heads = serde_json::Value::Array(
            authority_advance
                .members()
                .iter()
                .map(|member| {
                    serde_json::json!(
                        { "workspaceId" : member.workspace_id, "repoId" :
                        member.repo_id, "branchName" : member.branch_name, "headCommit" :
                        member.proposed_new_head, "previousHeadCommit" : member
                        .expected_old_head, }
                    )
                })
                .collect(),
        );
        sqlx::query(
            r#"
            INSERT INTO workspace_merge_runs (
                id, org_id, workspace_id, source_kind,
                source_workspace_id, target_workspace_id,
                source_branch_name, status, phase, source_heads, target_heads,
                pre_start_source_heads, target_checkpoints, advances_target,
                confirmed_by, started_at, completed_at, completion_advance_id,
                created_at, updated_at, approval_admission
            ) VALUES (
                $1, $2, $3, 'workspace',
                $3, $4,
                $5, 'completed', $6, $7, $8,
                $7, $8, $9,
                $10, $11, $11, $12,
                $11, $11, $13
            )
            "#,
        )
        .bind(run_id)
        .bind(context.source_workspace.org_id)
        .bind(context.source_workspace.id)
        .bind(context.target_workspace.id)
        .bind(&source_branch_name)
        .bind(phase)
        .bind(&final_heads)
        .bind(&context.target_heads)
        .bind(advances_target)
        .bind(confirmed_by)
        .bind(now)
        .bind(authority_advance.advance_id())
        .bind(&approval_admission_json)
        .execute(db.conn())
        .await
        .map_err(|error| {
            map_active_workspace_merge_lock_error(
                error,
                context.source_workspace.id,
                context.target_workspace.id,
            )
        })?;
        Ok(CompletedWorkspaceHeadAdvance {
            run: WorkspaceMergeRunSummary {
                id: run_id,
                workspace_id: context.source_workspace.id,
                source_kind: "workspace".to_string(),
                source_workspace_id: Some(context.source_workspace.id),
                target_workspace_id: Some(context.target_workspace.id),
                resolution_task_id: None,
                resolution_session_id: None,
                resolution_task_status: None,
                source_branch_name,
                status: "completed".to_string(),
                phase: phase.to_string(),
                blocking_reason: None,
                repo_id: None,
                source_heads: Some(final_heads),
                target_heads: Some(context.target_heads),
                started_at: Some(now),
                approval_admission,
            },
            authority_advance: Some(authority_advance),
        })
    }
    async fn acquire_workspace_merge_admission_locks(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        advances_target: bool,
    ) -> Result<(), G5Error> {
        let mut requests = vec![
            WorkspaceMergeAdmissionRequest {
                workspace_id: source_workspace_id,
                resource: WorkspaceMergeAdmissionResource::Participation,
                mode: WorkspaceMergeAdmissionMode::Exclusive,
            },
            WorkspaceMergeAdmissionRequest {
                workspace_id: target_workspace_id,
                resource: WorkspaceMergeAdmissionResource::Participation,
                mode: WorkspaceMergeAdmissionMode::Shared,
            },
        ];
        if advances_target {
            requests.push(WorkspaceMergeAdmissionRequest {
                workspace_id: target_workspace_id,
                resource: WorkspaceMergeAdmissionResource::TargetWriter,
                mode: WorkspaceMergeAdmissionMode::Exclusive,
            });
        }
        let mut coalesced: Vec<WorkspaceMergeAdmissionRequest> = Vec::with_capacity(requests.len());
        for request in requests {
            if let Some(existing) = coalesced.iter_mut().find(|existing| {
                existing.workspace_id == request.workspace_id
                    && existing.resource == request.resource
            }) {
                if request.mode == WorkspaceMergeAdmissionMode::Exclusive {
                    existing.mode = WorkspaceMergeAdmissionMode::Exclusive;
                }
            } else {
                coalesced.push(request);
            }
        }
        coalesced.sort_by_key(|request| (request.workspace_id, request.resource));
        for request in coalesced {
            let acquired: bool = match (request.resource, request.mode) {
                (
                    WorkspaceMergeAdmissionResource::Participation,
                    WorkspaceMergeAdmissionMode::Shared,
                ) => {
                    sqlx::query_scalar(
                        r#"
                        SELECT pg_try_advisory_xact_lock_shared(
                            hashtextextended(
                                'g5:merge-run:workspace-participation:' || $1::text,
                                0
                            )
                        )
                        "#,
                    )
                    .bind(request.workspace_id)
                    .fetch_one(db.conn())
                    .await?
                }
                (
                    WorkspaceMergeAdmissionResource::Participation,
                    WorkspaceMergeAdmissionMode::Exclusive,
                ) => {
                    sqlx::query_scalar(
                        r#"
                        SELECT pg_try_advisory_xact_lock(
                            hashtextextended(
                                'g5:merge-run:workspace-participation:' || $1::text,
                                0
                            )
                        )
                        "#,
                    )
                    .bind(request.workspace_id)
                    .fetch_one(db.conn())
                    .await?
                }
                (
                    WorkspaceMergeAdmissionResource::TargetWriter,
                    WorkspaceMergeAdmissionMode::Exclusive,
                ) => {
                    sqlx::query_scalar(
                        r#"
                        SELECT pg_try_advisory_xact_lock(
                            hashtextextended(
                                'g5:merge-run:target-writer:' || $1::text,
                                0
                            )
                        )
                        "#,
                    )
                    .bind(request.workspace_id)
                    .fetch_one(db.conn())
                    .await?
                }
                (
                    WorkspaceMergeAdmissionResource::TargetWriter,
                    WorkspaceMergeAdmissionMode::Shared,
                ) => unreachable!("target-writer admission locks are always exclusive"),
            };
            if !acquired {
                let resource = match request.resource {
                    WorkspaceMergeAdmissionResource::Participation => "workspace participation",
                    WorkspaceMergeAdmissionResource::TargetWriter => "target writer",
                };
                return Err(G5Error::Conflict {
                    reason: format!(
                        "Merge admission is already in progress for workspace {} ({resource})",
                        request.workspace_id
                    ),
                });
            }
        }
        Ok(())
    }
    async fn admit_workspace_merge_run(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        advances_target: bool,
    ) -> Result<(), G5Error> {
        let mut workspace_ids = vec![source_workspace_id];
        if advances_target {
            workspace_ids.push(target_workspace_id);
        }
        workspace_ids.sort_unstable();
        workspace_ids.dedup();
        for workspace_id in workspace_ids {
            crate::services::workspace_mutation_policy::ensure_workspace_not_deleting(
                db.conn(),
                workspace_id,
            )
            .await?;
        }
        if !advances_target {
            // Source rebases keep the target as an optimistic proof input,
            // matching revalidate_prepared_source_rebase_context. Taking its
            // SHARE lock after the source UPDATE lock can deadlock opposite
            // rebases before the nonblocking admission locks reject overlap.
            let deleting: bool = sqlx::query_scalar(
                "SELECT deletion_operation_id IS NOT NULL FROM workspaces WHERE id=$1 AND deleted_at IS NULL",
            )
            .bind(target_workspace_id)
            .fetch_optional(db.conn())
            .await?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace {target_workspace_id}"),
            })?;
            if deleting {
                return Err(
                    crate::services::workspace_mutation_policy::workspace_deletion_pending(),
                );
            }
        }
        self.acquire_workspace_merge_admission_locks(
            db,
            source_workspace_id,
            target_workspace_id,
            advances_target,
        )
        .await?;
        self.reject_active_workspace_merge_execution_after_cleanup(
            db,
            source_workspace_id,
            target_workspace_id,
        )
        .await
    }
    async fn load_workspace_for_merge(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkspaceMergeWorkspaceRow, G5Error> {
        sqlx::query_as::<_, WorkspaceMergeWorkspaceRow>(
            r#"
            SELECT id, org_id, branch_name
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {}", workspace_id),
        })
    }
    async fn load_workspace_pair_for_merge(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(WorkspaceMergeWorkspaceRow, WorkspaceMergeWorkspaceRow), G5Error> {
        if source_workspace_id.as_bytes() <= target_workspace_id.as_bytes() {
            let source = self
                .load_workspace_for_merge(db, source_workspace_id)
                .await?;
            let target = self
                .load_workspace_for_merge(db, target_workspace_id)
                .await?;
            Ok((source, target))
        } else {
            let target = self
                .load_workspace_for_merge(db, target_workspace_id)
                .await?;
            let source = self
                .load_workspace_for_merge(db, source_workspace_id)
                .await?;
            Ok((source, target))
        }
    }
    async fn reject_active_workspace_merge_execution(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        let existing: Option<Uuid> =
            sqlx::query_scalar(REJECT_LIVE_ACTIVE_WORKSPACE_MERGE_EXECUTION_SQL)
                .bind(source_workspace_id)
                .bind(target_workspace_id)
                .fetch_optional(db.conn())
                .await?;
        if let Some(run_id) = existing {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Merge run {run_id} is already active for the source workspace, a source run mutating the target workspace, or a target-advancing run on the target workspace"
                ),
            });
        }
        Ok(())
    }
    async fn reject_active_workspace_merge_execution_after_cleanup(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        terminalize_active_merge_runs_with_tombstoned_sides(
            db,
            &[source_workspace_id, target_workspace_id],
        )
        .await?;
        let existing: Option<Uuid> = sqlx::query_scalar(REJECT_ACTIVE_WORKSPACE_MERGE_PAIR_SQL)
            .bind(source_workspace_id)
            .bind(target_workspace_id)
            .fetch_optional(db.conn())
            .await?;
        if let Some(run_id) = existing {
            return Err(G5Error::Conflict {
                reason: format!(
                    "Merge run {run_id} is already active for the source workspace, a source run mutating the target workspace, or a target-advancing run on the target workspace"
                ),
            });
        }
        Ok(())
    }
    async fn reject_dirty_workspace_head(
        &self,
        _db: &mut ScopedDb,
        _workspace_id: Uuid,
        _workspace_role: Option<&str>,
    ) -> Result<(), G5Error> {
        Ok(())
    }
    /// Revalidate a source-only rebase without serializing the read-only target.
    ///
    /// The source is fenced and row-locked because its authoritative cut will be
    /// rewritten. The target remains an optimistic proof input: resolve its full
    /// committed vector in this final application transaction and reject drift,
    /// but do not take target workspace/branch rows or the target finalization
    /// fence away from concurrent writers.
    async fn revalidate_prepared_source_rebase_context(
        &self,
        db: &mut ScopedDb,
        context: &WorkspaceMergePreflightContext,
    ) -> Result<(), G5Error> {
        self.lock_workspace_for_merge_application(db, context.source_workspace.id)
            .await?;
        self.revalidate_workspace_vectors(db, context, false).await
    }

    async fn revalidate_prepared_workspace_merge_context(
        &self,
        db: &mut ScopedDb,
        context: &WorkspaceMergePreflightContext,
    ) -> Result<(), G5Error> {
        let mut workspaces = [context.source_workspace.id, context.target_workspace.id];
        workspaces.sort();
        for workspace_id in workspaces {
            self.lock_workspace_for_merge_application(db, workspace_id)
                .await?;
        }
        self.revalidate_workspace_vectors(db, context, true).await
    }

    async fn revalidate_workspace_vectors(
        &self,
        db: &mut ScopedDb,
        context: &WorkspaceMergePreflightContext,
        lock_target: bool,
    ) -> Result<(), G5Error> {
        for (workspace, is_source) in [
            (&context.source_workspace, true),
            (&context.target_workspace, false),
        ] {
            let current = self.load_workspace_for_merge(db, workspace.id).await?;
            if current.org_id != workspace.org_id || current.branch_name != workspace.branch_name {
                return Err(G5Error::Conflict {
                    reason: "Workspace identity changed before merge publication".into(),
                });
            }
            crate::services::workspace_repositories::load_workspace_repositories(
                db.conn(),
                workspace.id,
                is_source || lock_target,
            )
            .await?;
            let snapshot = self
                .resolve_merge_snapshot(db, workspace.id, context.published_heads)
                .await?;
            if snapshot.repos.len() != context.branch_pairs.len()
                || context.branch_pairs.iter().any(|pair| {
                    snapshot.repo(pair.repo_id).map_or(true, |member| {
                        member.workspace_id != workspace.id
                            || member.branch_name
                                != if is_source {
                                    &pair.source_branch_name
                                } else {
                                    &pair.target_branch_name
                                }
                                .as_str()
                            || member.head_commit
                                != if is_source {
                                    &pair.source_head
                                } else {
                                    &pair.target_head
                                }
                                .as_str()
                    })
                })
            {
                return Err(G5Error::Conflict {
                    reason: "Complete workspace Git cut changed before merge publication; retry"
                        .into(),
                });
            }
        }
        Ok(())
    }
    async fn lock_workspace_for_merge_application(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkspaceMergeWorkspaceRow, G5Error> {
        let workspace = sqlx::query_as::<_, WorkspaceMergeWorkspaceRow>(
            r#"
            SELECT id, org_id, branch_name
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            FOR UPDATE
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        sqlx::query("SELECT g5_require_workspace_not_deleting($1)")
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        Ok(workspace)
    }
}
