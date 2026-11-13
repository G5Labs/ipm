//! Approval config service — append-only `approval_configs` attach/retire
//! events and human `approval_statuses` decisions (migrations 186/217, B6
//! governance UI PR-2), plus the combined human/agent history and evaluation
//! projection.
//!
//! Agent status insertion remains owned by `ApprovalReviewService`. This
//! service owns explicit human request admission, human settlement, and the
//! combined request/status read projection. Immutable governance evidence is
//! insert-only; only the guarded request row receives its one terminal update.

use crate::services::git::repo_store::TenantRepoStorage;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{
    is_approval_phase, ApprovalPolicy, ApprovalRequestKeyV2, ApprovalRequestSettlementV2,
    MergeSourceHead, SourceCommitVectorPolicy, APPROVAL_REQUEST_KEY_SCHEMA_VERSION,
    APPROVAL_REQUEST_SETTLEMENT_SCHEMA_VERSION, WORKSPACE_APPROVAL_COMPAT_PHASE,
};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::services::git::workspace_merge_service::{
    MergeAdmissionEvidence, MergeAdmissionGate, MergeAdmissionRequirement,
    MERGE_ADMISSION_SCHEMA_VERSION,
};
use crate::services::workspace::approval_definition_service::{
    ApprovalDefinitionRecord, ApprovalDefinitionService,
};
use crate::services::workspace::home_service::{HomeAttentionCursor, HomeAttentionOrder};
use crate::services::PermissionService;

mod requirement_status;
use requirement_status::{counted_decision, HumanCandidate, RequestFact, RequirementInput};
pub use requirement_status::{
    AgentEscalation, AgentEscalationState, ApprovalPath, Currentness, CurrentnessState,
    RecipientNeeds, RequestNeeds, RequirementEvaluation, ReviewerRef,
};

#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeApprovalQueryObservation {
    /// The organization read, so a test can tell its reads from those of
    /// tests running beside it.
    pub org_id: Uuid,
    pub batch_id: Uuid,
    pub visible_workspace_count: usize,
    pub query_id: &'static str,
}

#[cfg(debug_assertions)]
static HOME_APPROVAL_QUERY_OBSERVATIONS: std::sync::LazyLock<
    std::sync::Mutex<Vec<HomeApprovalQueryObservation>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

#[cfg(debug_assertions)]
fn observe_home_approval_query(
    org_id: Uuid,
    batch_id: Uuid,
    visible_workspace_count: usize,
    query_id: &'static str,
) {
    HOME_APPROVAL_QUERY_OBSERVATIONS
        .lock()
        .expect("Home approval query observer lock")
        .push(HomeApprovalQueryObservation {
            org_id,
            batch_id,
            visible_workspace_count,
            query_id,
        });
}

#[cfg(not(debug_assertions))]
fn observe_home_approval_query(
    _org_id: Uuid,
    _batch_id: Uuid,
    _visible_workspace_count: usize,
    _query_id: &'static str,
) {
}

/// Drain one organization's Home approval query observations recorded by
/// production call sites in debug builds, leaving those of reads running
/// beside it (parallel tests share this record).
#[cfg(debug_assertions)]
pub fn take_home_approval_query_observations(org_id: Uuid) -> Vec<HomeApprovalQueryObservation> {
    let mut observations = HOME_APPROVAL_QUERY_OBSERVATIONS
        .lock()
        .expect("Home approval query observer lock");
    let (taken, kept) = std::mem::take(&mut *observations)
        .into_iter()
        .partition(|observation| observation.org_id == org_id);
    *observations = kept;
    taken
}

/// One attach/retire event on a target workspace.
#[derive(Debug, Clone)]
pub struct AttachmentEventRecord {
    pub definition_id: Uuid,
    pub definition_name: String,
    pub retired: bool,
    pub actor_user_id: Uuid,
    pub actor_display_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// An active requirement of a target workspace (latest event not retired).
#[derive(Debug, Clone)]
pub struct ActiveRequirementRecord {
    pub definition: ApprovalDefinitionRecord,
    pub attached_by: Uuid,
    pub attached_by_display_name: String,
    pub attached_at: chrono::DateTime<chrono::Utc>,
}

/// One immutable decision row, flagged when superseded by invalidation.
#[derive(Debug, Clone)]
pub struct DecisionRecord {
    pub agent_review_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    pub outcome: String,
    pub comment: Option<String>,
    pub summary: Option<String>,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub actor_display_name: Option<String>,
    pub definition_version: i32,
    pub superseded: bool,
    pub reviewed_git_heads: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Per-approver current state on one outgoing requirement.
#[derive(Debug, Clone)]
pub struct ApproverStateRecord {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub still_member: bool,
    pub deactivated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Latest eligible decision, `pending`, or `not_requested` when no
    /// actor-slot request exists for this definition.
    pub state: String,
    pub comment: Option<String>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub request_id: Option<Uuid>,
    pub request_state: Option<String>,
    pub requested_at: Option<chrono::DateTime<chrono::Utc>>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct ApprovalRequestRecord {
    pub request_id: Uuid,
    pub actor_type: String,
    pub actor_user_id: Option<Uuid>,
    pub actor_display_name: Option<String>,
    pub requested_by: Option<Uuid>,
    pub requested_by_display_name: Option<String>,
    pub state: String,
    pub definition_version: i32,
    pub definition_name: String,
    pub note: Option<String>,
    /// Legacy storage phase of the request chain; `None` for a request admitted under the
    /// stage model before migration 067 (terminal since the cutover).
    pub phase: Option<String>,
    pub requested_at: chrono::DateTime<chrono::Utc>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub agent_reviews: Vec<AgentApprovalReviewRecord>,
}

/// Sanitized observational state for one agent-review execution. Approval
/// authority remains exclusively in `approval_statuses`.
#[derive(Debug, Clone)]
pub struct AgentApprovalReviewRecord {
    pub review_id: Option<Uuid>,
    pub stage: Option<String>,
    pub cleanup_pending: bool,
    pub cleanup_error: Option<String>,
    pub cleanup_overdue: bool,
    pub report_available: bool,
    pub retry_available: bool,
    pub content_sha256: Option<String>,

    pub task_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub state: String,
    pub terminal_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One requirement of the workspace's merge target, with decision state.
#[derive(Debug, Clone)]
pub struct OutgoingRequirementRecord {
    pub definition: ApprovalDefinitionRecord,
    /// Compatibility storage vocabulary; one entry per workspace-wide type.
    /// Definition phase associations only recommend when to request review.
    pub phase: String,
    /// Overall status (g5.governance.approval.status) shared with merge
    /// admission: `approved`, `rejected` or `pending`.
    pub state: String,
    /// Status, currentness, path and request needs from the one evaluator.
    pub evaluation: RequirementEvaluation,
    pub approvers: Vec<ApproverStateRecord>,
    /// True when the current organization member has no eligible blocking
    /// verdict and may write either a direct or request-addressed decision;
    /// merge evaluation still considers only the definition's required actors.
    pub caller_can_decide: bool,
    pub requests: Vec<ApprovalRequestRecord>,
    /// Timestamp of the latest event that can make this requirement actionable
    /// for the caller at the current source cut.
    pub actionable_at: chrono::DateTime<chrono::Utc>,
    pub decision_log: Vec<DecisionRecord>,
}

/// Request-complete outgoing history, independent of active merge requirements.
#[derive(Debug, Clone)]
pub struct OutgoingHistoryRecord {
    pub definition_id: Uuid,
    pub definition_name: String,
    pub requests: Vec<ApprovalRequestRecord>,
    pub decision_log: Vec<DecisionRecord>,
}

/// Compact organization-Home projection of one approval the caller may decide.
#[derive(Debug, Clone)]
pub struct ActionableApprovalRecord {
    pub id: String,
    pub source_workspace_id: Uuid,
    pub source_workspace_name: String,
    pub definition_id: Uuid,
    pub definition_name: String,
    pub actionable_at: chrono::DateTime<chrono::Utc>,
}

impl ActionableApprovalRecord {
    fn key(&self) -> (chrono::DateTime<chrono::Utc>, Uuid, &str) {
        (self.actionable_at, self.source_workspace_id, &self.id)
    }
}

/// A Home approval candidate ranked by its place in the requested page
/// order, so the bounded max-heap evicts whichever would be listed last.
struct PageOrderedApproval {
    order: HomeAttentionOrder,
    record: ActionableApprovalRecord,
}

impl PartialEq for PageOrderedApproval {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for PageOrderedApproval {}

impl PartialOrd for PageOrderedApproval {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PageOrderedApproval {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.compare(&self.record.key(), &other.record.key())
    }
}

/// The workspace-as-source section, absent without a merge target.
#[derive(Debug, Clone)]
pub struct OutgoingSection {
    pub target_workspace_id: Uuid,
    pub target_workspace_name: String,
    pub requirements: Vec<OutgoingRequirementRecord>,
    pub history: Vec<OutgoingHistoryRecord>,
    /// Merge admission against the current source head vector: the same
    /// evaluation the merge paths pin, so every shipping surface shows one
    /// state (g5.governance.approval.requirements).
    pub merge_admission: MergeAdmissionEvidence,
    /// The request and source-commit policy this projection evaluated.
    pub policy: ApprovalPolicy,
}

/// Combined tab state.
#[derive(Debug, Clone)]
pub struct WorkspaceApprovalsState {
    pub required_into: Vec<ActiveRequirementRecord>,
    pub attachment_history: Vec<AttachmentEventRecord>,
    pub outgoing: Option<OutgoingSection>,
}

/// Human decision request (route DTOs stay in the route file).
#[derive(Debug, Clone)]
pub struct DecisionRequest {
    /// Complete pushed cut explicitly captured by the approver.
    pub reviewed_git_heads: Vec<MergeSourceHead>,
    pub definition_id: Uuid,
    pub request_id: Option<Uuid>,
    /// `approved`, `rejected`, or `deferred` (the durable Undo/withdrawal).
    pub outcome: String,
    pub comment: Option<String>,
    /// Phase this decision settles (`be` | `se` | `ie`). Validated against
    /// the legacy phase vocabulary; request timing recommendations confer no authority.
    pub phase: String,
}

#[derive(Debug, Clone)]
pub struct CreateHumanApprovalRequests {
    pub client_request_id: Option<Uuid>,
    pub definition_id: Uuid,
    pub definition_version: i32,
    /// Phase of the request chain; must be bound by the exact version.
    pub phase: String,
    pub actor_user_ids: Option<Vec<Uuid>>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HumanApprovalRequestAdmission {
    pub request_id: Uuid,
    pub actor_user_id: Uuid,
    pub reused: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct ConfigEventRow {
    approval_definition_id: Uuid,
    retired: bool,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct StatusRow {
    agent_review_id: Option<Uuid>,
    /// Human rows: counted evidence under `HUMAN_EVIDENCE_IN_SCOPE`.
    in_scope: bool,
    request_id: Option<Uuid>,
    approval_definition_id: Uuid,
    approval_definition_version: i32,
    outcome: String,
    comment: Option<String>,
    actor_type: String,
    actor_user_id: Option<Uuid>,
    reviewed_git_heads: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    /// An agent decision whose report carries an authored summary (schema 5+).
    #[sqlx(default)]
    summary_authored: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspaceTargetRow {
    merge_target_workspace_id: Option<Uuid>,
    cloned_from_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchSourceTargetRow {
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchActiveConfigRow {
    target_workspace_id: Uuid,
    approval_definition_id: Uuid,
    attached_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchStatusRow {
    source_workspace_id: Uuid,
    approval_definition_id: Uuid,
    approval_definition_version: i32,
    outcome: String,
    actor_type: String,
    actor_user_id: Option<Uuid>,
    reviewed_git_heads: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct BatchInvalidationRow {
    approval_definition_id: Uuid,
    version: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ApprovalRequestRow {
    /// Pending-capable under the request PR rule: bound to the open PR, or
    /// made without a PR while none is required.
    active_pr: bool,
    id: Uuid,
    approval_definition_id: Uuid,
    approval_definition_version: i32,
    actor_type: String,
    actor_user_id: Option<Uuid>,
    requested_by: Option<Uuid>,
    state: String,
    definition_name: String,
    allow_self_approval: bool,
    note: Option<String>,
    phase: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A settled human decision counts when its request was bound to a PR, open
/// or not, or was made without a PR (g5.governance.approval.request-pr): PR
/// lifecycle and `PR_REQUIRED` never retire it. Decisions without a request,
/// or on a legacy request recording neither, remain historical. Agent rows
/// ignore it.
const HUMAN_EVIDENCE_IN_SCOPE: &str = "approval_statuses.request_id IS NOT NULL AND (approval_statuses.pull_request_id IS NOT NULL \
     OR EXISTS (SELECT 1 FROM approval_requests q WHERE q.id = approval_statuses.request_id AND q.without_pull_request))";

/// An agent decision whose report carries an agent-authored summary
/// (assignment schema 5+); earlier stored text is a report clip.
const AGENT_SUMMARY_AUTHORED: &str = "approval_statuses.actor_type = 'agent' AND (SELECT (r.invocation->'resolution'->>'schemaVersion')::int FROM agent_approval_reviews r WHERE r.id = approval_statuses.agent_review_id) >= 5";

pub(crate) trait ApprovalDecisionLike {
    fn definition_version(&self) -> i32;
    fn outcome(&self) -> &str;
    fn actor_type(&self) -> &str;
    fn actor_user_id(&self) -> Option<Uuid>;
    fn reviewed_git_heads(&self) -> &serde_json::Value;
    fn created_at(&self) -> chrono::DateTime<chrono::Utc>;
    /// The formal review behind an agent decision, when recorded.
    fn agent_review_id(&self) -> Option<Uuid> {
        None
    }
}

impl ApprovalDecisionLike for StatusRow {
    fn definition_version(&self) -> i32 {
        self.approval_definition_version
    }

    fn outcome(&self) -> &str {
        &self.outcome
    }

    fn actor_type(&self) -> &str {
        &self.actor_type
    }

    fn actor_user_id(&self) -> Option<Uuid> {
        self.actor_user_id
    }

    fn reviewed_git_heads(&self) -> &serde_json::Value {
        &self.reviewed_git_heads
    }

    fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
    }

    fn agent_review_id(&self) -> Option<Uuid> {
        self.agent_review_id
    }
}

impl ApprovalDecisionLike for BatchStatusRow {
    fn definition_version(&self) -> i32 {
        self.approval_definition_version
    }

    fn outcome(&self) -> &str {
        &self.outcome
    }

    fn actor_type(&self) -> &str {
        &self.actor_type
    }

    fn actor_user_id(&self) -> Option<Uuid> {
        self.actor_user_id
    }

    fn reviewed_git_heads(&self) -> &serde_json::Value {
        &self.reviewed_git_heads
    }

    fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.created_at
    }
}

fn caller_deferred_at<'a, T: ApprovalDecisionLike + 'a>(
    statuses: impl IntoIterator<Item = &'a T>,
    caller_user_id: Uuid,
    invalidation_boundary: i32,
    current_heads: &[MergeSourceHead],
    policy: SourceCommitVectorPolicy,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let current = counted_decision(
        statuses,
        "human",
        Some(caller_user_id),
        invalidation_boundary,
        Some(current_heads),
        policy,
    );
    current
        .filter(|status| status.outcome() == "deferred")
        .map(ApprovalDecisionLike::created_at)
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AgentApprovalReviewRow {
    review_id: Option<Uuid>,
    stage: Option<String>,
    cleanup_pending: bool,
    cleanup_error: Option<String>,
    cleanup_overdue: bool,
    report_available: bool,
    retry_available: bool,
    content_sha256: Option<String>,

    approval_request_id: Uuid,
    task_id: Option<Uuid>,
    session_id: Option<Uuid>,
    state: String,
    terminal_reason: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    ended_at: Option<chrono::DateTime<chrono::Utc>>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// Approval requests, decisions and their projection under one
/// [`ApprovalPolicy`] (g5.governance.approval.request-pr,
/// g5.governance.approval.source-commit-policy).
#[derive(Clone)]
pub struct ApprovalConfigService {
    repo_storage: TenantRepoStorage,
    policy: ApprovalPolicy,
}

impl ApprovalConfigService {
    pub fn new(repo_storage: TenantRepoStorage, policy: ApprovalPolicy) -> Self {
        Self {
            repo_storage,
            policy,
        }
    }

    pub fn policy(&self) -> ApprovalPolicy {
        self.policy
    }

    /// Reuse the exact outgoing requirement derivation for a visible
    /// organization workspace set and retain only caller-decidable rows.
    // One argument per authority/collaborator plus the explicit page bound;
    // grouping them would hide rather than reduce the service boundary.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn actionable_for_org(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        caller_user_id: Uuid,
        visible_workspaces: &[(Uuid, String)],
        cursor: Option<&HomeAttentionCursor>,
        order: HomeAttentionOrder,
        candidate_limit: usize,
    ) -> Result<Vec<ActionableApprovalRecord>, G5Error> {
        if visible_workspaces.is_empty() || candidate_limit == 0 {
            return Ok(Vec::new());
        }

        let batch_id = Uuid::new_v4();
        let visible_workspace_count = visible_workspaces.len();
        let visible_workspace_ids = visible_workspaces
            .iter()
            .map(|(workspace_id, _)| *workspace_id)
            .collect::<Vec<_>>();
        let workspace_names = visible_workspaces
            .iter()
            .cloned()
            .collect::<HashMap<_, _>>();

        observe_home_approval_query(
            org_id,
            batch_id,
            visible_workspace_count,
            "caller-membership",
        );
        let caller_is_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL)",
        )
        .bind(org_id)
        .bind(caller_user_id)
        .fetch_one(system_pool)
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to validate Home approval caller membership: {error}"),
        })?;
        if !caller_is_member {
            return Ok(Vec::new());
        }

        observe_home_approval_query(org_id, batch_id, visible_workspace_count, "source-targets");
        let source_targets: Vec<BatchSourceTargetRow> = sqlx::query_as(
            r#"
            SELECT id AS source_workspace_id,
                   COALESCE(merge_target_workspace_id, cloned_from_id)
                       AS target_workspace_id
            FROM workspaces
            WHERE org_id = $1
              AND id = ANY($2)
              AND deleted_at IS NULL
              AND COALESCE(merge_target_workspace_id, cloned_from_id) IS NOT NULL
            ORDER BY id
            "#,
        )
        .bind(org_id)
        .bind(&visible_workspace_ids)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to batch approval source targets: {error}"),
        })?;
        let source_workspace_ids = source_targets
            .iter()
            .map(|row| row.source_workspace_id)
            .collect::<Vec<_>>();
        let target_workspace_ids = source_targets
            .iter()
            .map(|row| row.target_workspace_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        observe_home_approval_query(org_id, batch_id, visible_workspace_count, "active-configs");
        let active_configs: Vec<BatchActiveConfigRow> = sqlx::query_as(
            r#"
            WITH latest AS (
                SELECT DISTINCT ON (target_workspace_id, approval_definition_id)
                       target_workspace_id, approval_definition_id, retired, created_at
                FROM approval_configs
                WHERE org_id = $1
                  AND target_workspace_id = ANY($2)
                ORDER BY target_workspace_id, approval_definition_id, seq DESC
            )
            SELECT target_workspace_id, approval_definition_id,
                   created_at AS attached_at
            FROM latest
            WHERE retired = false
            ORDER BY target_workspace_id, approval_definition_id
            "#,
        )
        .bind(org_id)
        .bind(&target_workspace_ids)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to batch active approval requirements: {error}"),
        })?;
        let relevant_definition_ids = active_configs
            .iter()
            .map(|row| row.approval_definition_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        observe_home_approval_query(
            org_id,
            batch_id,
            visible_workspace_count,
            "latest-definitions",
        );
        observe_home_approval_query(
            org_id,
            batch_id,
            visible_workspace_count,
            "definition-profiles",
        );
        let definitions = definition_service
            .list_latest(db, system_pool, org_id)
            .await?
            .into_iter()
            .filter(|definition| relevant_definition_ids.contains(&definition.id))
            .map(|definition| (definition.id, definition))
            .collect::<HashMap<_, _>>();

        // Home offers actual addressed work only. An unchanged head or an
        // earlier decision does not hide an explicitly renewed request.
        let pending: Vec<(Uuid, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT r.source_workspace_id,r.approval_definition_id,r.created_at FROM approval_requests r JOIN workspaces w ON w.id=r.source_workspace_id JOIN approval_definitions d ON d.org_id=r.org_id AND d.id=r.approval_definition_id AND d.version=r.approval_definition_version WHERE r.org_id=$1 AND r.source_workspace_id=ANY($2) AND r.actor_type='human' AND r.actor_user_id=$3 AND r.state='pending' AND ((r.without_pull_request AND NOT $4) OR r.pull_request_id IN (SELECT p.id FROM workspace_pull_requests p WHERE p.source_workspace_id=r.source_workspace_id AND p.status='open')) AND (w.owner_user_id IS DISTINCT FROM $3 OR (d.allow_self_approval AND (SELECT current.allow_self_approval FROM approval_definitions current WHERE current.org_id=r.org_id AND current.id=r.approval_definition_id ORDER BY current.version DESC LIMIT 1))) ORDER BY r.seq"
        ).bind(org_id).bind(&source_workspace_ids).bind(caller_user_id).bind(self.policy.pr_required).fetch_all(db.conn()).await?;
        let pending = pending
            .into_iter()
            .map(|(w, d, at)| ((w, d), at))
            .collect::<HashMap<_, _>>();
        observe_home_approval_query(
            org_id,
            batch_id,
            visible_workspace_count,
            "caller-decisions",
        );
        let caller_statuses: Vec<BatchStatusRow> = sqlx::query_as(
            r#"
            SELECT source_workspace_id, approval_definition_id,
                   approval_definition_version, outcome, actor_type,
                   actor_user_id, reviewed_git_heads, created_at
            FROM approval_statuses
            WHERE org_id = $1
              AND source_workspace_id = ANY($2)
              AND approval_definition_id = ANY($3)
              AND actor_type = 'human'
              AND actor_user_id = $4
            ORDER BY source_workspace_id, approval_definition_id, seq DESC
            "#,
        )
        .bind(org_id)
        .bind(&source_workspace_ids)
        .bind(&relevant_definition_ids)
        .bind(caller_user_id)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to batch caller approval decisions: {error}"),
        })?;
        let mut statuses_by_source_definition =
            HashMap::<(Uuid, Uuid), Vec<&BatchStatusRow>>::new();
        for status in &caller_statuses {
            statuses_by_source_definition
                .entry((status.source_workspace_id, status.approval_definition_id))
                .or_default()
                .push(status);
        }

        observe_home_approval_query(org_id, batch_id, visible_workspace_count, "source-heads");
        let versions = self
            .capture_source_versions(db, &source_workspace_ids)
            .await?;
        let mut heads_by_workspace = HashMap::<Uuid, Vec<MergeSourceHead>>::new();
        let mut head_activity_by_workspace =
            HashMap::<Uuid, Option<chrono::DateTime<chrono::Utc>>>::new();
        for version in versions {
            let workspace_id = version.member.workspace_id;
            heads_by_workspace
                .entry(workspace_id)
                .or_default()
                .push(MergeSourceHead {
                    repo_id: version.member.repo_id.to_string(),
                    branch_name: version.member.branch_name,
                    workspace_id: workspace_id.to_string(),
                    head_commit: version.head_commit,
                });
            let activity = head_activity_by_workspace.entry(workspace_id).or_default();
            *activity = Some(activity.map_or(version.committed_at, |current| {
                current.max(version.committed_at)
            }));
        }

        observe_home_approval_query(
            org_id,
            batch_id,
            visible_workspace_count,
            "invalidation-boundaries",
        );
        let invalidations: Vec<BatchInvalidationRow> = sqlx::query_as(
            r#"
            SELECT DISTINCT ON (id)
                   id AS approval_definition_id, version, created_at
            FROM approval_definitions
            WHERE org_id = $1
              AND id = ANY($2)
              AND invalidates_previous = true
            ORDER BY id, version DESC
            "#,
        )
        .bind(org_id)
        .bind(&relevant_definition_ids)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to batch approval invalidation boundaries: {error}"),
        })?;
        let invalidations = invalidations
            .into_iter()
            .map(|row| (row.approval_definition_id, (row.version, row.created_at)))
            .collect::<HashMap<_, _>>();
        let configs_by_target = active_configs.into_iter().fold(
            HashMap::<Uuid, Vec<BatchActiveConfigRow>>::new(),
            |mut grouped, config| {
                grouped
                    .entry(config.target_workspace_id)
                    .or_default()
                    .push(config);
                grouped
            },
        );

        let mut first_in_order = BinaryHeap::with_capacity(candidate_limit + 1);
        for source_target in source_targets {
            let Some(configs) = configs_by_target.get(&source_target.target_workspace_id) else {
                continue;
            };
            let Some(source_heads) = heads_by_workspace
                .get(&source_target.source_workspace_id)
                .filter(|heads| !heads.is_empty())
            else {
                continue;
            };
            let current_heads_value =
                serde_json::to_value(source_heads).map_err(|error| G5Error::Internal {
                    message: format!("Failed to serialize batched source head vector: {error}"),
                })?;
            let current_heads = normalized_head_vector(&current_heads_value)?;
            for config in configs {
                let Some(definition) = definitions.get(&config.approval_definition_id) else {
                    continue;
                };
                let (boundary, invalidated_at) = invalidations
                    .get(&definition.id)
                    .map_or((1, None), |(version, created_at)| {
                        (*version, Some(*created_at))
                    });
                let statuses = statuses_by_source_definition
                    .get(&(source_target.source_workspace_id, definition.id))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                // One addressed workspace-wide task per approval type.
                {
                    let phase_statuses = statuses.iter().copied();
                    let deferred_at = caller_deferred_at(
                        phase_statuses,
                        caller_user_id,
                        boundary,
                        &current_heads,
                        self.policy.on_source_commit_vector_change,
                    );
                    let Some(requested_at) =
                        pending.get(&(source_target.source_workspace_id, definition.id))
                    else {
                        continue;
                    };
                    let mut actionable_at = config.attached_at.max(*requested_at);
                    if let Some(invalidated_at) = invalidated_at {
                        actionable_at = actionable_at.max(invalidated_at);
                    }
                    if let Some(Some(head_changed_at)) =
                        head_activity_by_workspace.get(&source_target.source_workspace_id)
                    {
                        actionable_at = actionable_at.max(*head_changed_at);
                    }
                    if let Some(deferred_at) = deferred_at {
                        actionable_at = actionable_at.max(deferred_at);
                    }
                    let id = format!(
                        "approval:{}:{}",
                        source_target.source_workspace_id, definition.id
                    );
                    if cursor.is_some_and(|cursor| {
                        !cursor.is_after(actionable_at, 0, source_target.source_workspace_id, &id)
                    }) {
                        continue;
                    }
                    let candidate = ActionableApprovalRecord {
                        id,
                        source_workspace_id: source_target.source_workspace_id,
                        source_workspace_name: workspace_names
                            .get(&source_target.source_workspace_id)
                            .cloned()
                            .unwrap_or_else(|| "Unknown workspace".to_string()),
                        definition_id: definition.id,
                        definition_name: definition.name.clone(),
                        actionable_at,
                    };
                    first_in_order.push(PageOrderedApproval {
                        order,
                        record: candidate,
                    });
                    if first_in_order.len() > candidate_limit {
                        first_in_order.pop();
                    }
                }
            }
        }

        Ok(first_in_order
            .into_sorted_vec()
            .into_iter()
            .map(|ranked| ranked.record)
            .collect())
    }

    /// Combined tab state for one workspace (both sections).
    pub async fn workspace_state(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        workspace_id: Uuid,
        caller_user_id: Uuid,
    ) -> Result<WorkspaceApprovalsState, G5Error> {
        let (required_into, attachment_history) = self
            .target_side_state(db, system_pool, definition_service, org_id, workspace_id)
            .await?;
        let outgoing = self
            .outgoing_state(
                db,
                system_pool,
                definition_service,
                org_id,
                workspace_id,
                caller_user_id,
            )
            .await?;
        Ok(WorkspaceApprovalsState {
            required_into,
            attachment_history,
            outgoing,
        })
    }

    /// Resolve the owning org on the ALREADY-SCOPED connection — `workspaces`
    /// carries forced RLS, so a system-pool read without org context sees
    /// nothing. 404 when the workspace is invisible to the caller's scope.
    pub async fn org_of_workspace(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Uuid, G5Error> {
        let org_id: Option<Uuid> =
            sqlx::query_scalar("SELECT org_id FROM workspaces WHERE id = $1")
                .bind(workspace_id)
                .fetch_optional(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve workspace org: {e}"),
                })?;
        org_id.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })
    }

    /// Serialize requirement-lifecycle writes for one `(org, target, definition)`.
    ///
    /// `attach`/`retire` decide their conditional insert from the latest-event
    /// read; without serialization two concurrent requests can both observe
    /// the same state and append duplicate lifecycle events (the table is
    /// append-only, so no uniqueness constraint can catch this). The advisory
    /// lock is transaction-scoped: `ScopedDb` always holds an open
    /// transaction, so the lock releases at commit/rollback.
    ///
    /// Protocol steps 1 and 2 (see the lock protocol at
    /// `lock_target_requirement_set`): the target's requirement-set lock
    /// first, then the definition lock.
    async fn lock_requirement_lifecycle(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        target_workspace_id: Uuid,
        definition_id: Uuid,
    ) -> Result<(), G5Error> {
        lock_target_requirement_set(db, org_id, target_workspace_id).await?;
        lock_approval_definition(db.conn(), org_id, definition_id).await
    }

    /// Attach event (idempotent when already active).
    #[allow(clippy::too_many_arguments)] // route-surface identity params, repo precedent
    pub async fn attach(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        target_workspace_id: Uuid,
        actor_user_id: Uuid,
        definition_id: Uuid,
    ) -> Result<WorkspaceApprovalsState, G5Error> {
        Self::require_requirement_admin(db, org_id, actor_user_id).await?;
        // Latest version doubles as the existence check (404 on missing).
        let definition = definition_service
            .get_latest(db, system_pool, org_id, definition_id)
            .await?;

        self.lock_requirement_lifecycle(db, org_id, target_workspace_id, definition_id)
            .await?;
        let active = self
            .latest_event_is_active(db, org_id, target_workspace_id, definition_id)
            .await?;
        if active != Some(true) {
            sqlx::query(
                r#"
                INSERT INTO approval_configs
                    (org_id, target_workspace_id, approval_definition_id,
                     definition_version_at_write, retired, created_by)
                VALUES ($1, $2, $3, $4, false, $5)
                "#,
            )
            .bind(org_id)
            .bind(target_workspace_id)
            .bind(definition_id)
            .bind(definition.latest_version)
            .bind(actor_user_id)
            .execute(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to attach approval requirement: {e}"),
            })?;
        }

        self.workspace_state(
            db,
            system_pool,
            definition_service,
            org_id,
            target_workspace_id,
            actor_user_id,
        )
        .await
    }

    /// Requirement administration belongs to an administrator of the
    /// workspace's organization (`g5.governance.approval.requirements`):
    /// org-level authority, never a workspace-scoped grant or ownership.
    async fn require_requirement_admin(
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<(), G5Error> {
        PermissionService::require_org_admin_on_conn(db.conn(), actor_user_id, org_id).await
    }

    /// Retire event (404 when never attached; idempotent when already retired).
    #[allow(clippy::too_many_arguments)] // route-surface identity params, repo precedent
    pub async fn retire(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        target_workspace_id: Uuid,
        actor_user_id: Uuid,
        definition_id: Uuid,
    ) -> Result<WorkspaceApprovalsState, G5Error> {
        Self::require_requirement_admin(db, org_id, actor_user_id).await?;
        self.lock_requirement_lifecycle(db, org_id, target_workspace_id, definition_id)
            .await?;
        let active = self
            .latest_event_is_active(db, org_id, target_workspace_id, definition_id)
            .await?;
        match active {
            None => {
                return Err(G5Error::NotFound {
                    resource: format!(
                        "approval requirement {definition_id} on workspace {target_workspace_id}"
                    ),
                });
            }
            Some(true) => {
                let definition = definition_service
                    .get_latest(db, system_pool, org_id, definition_id)
                    .await?;
                sqlx::query(
                    r#"
                    INSERT INTO approval_configs
                        (org_id, target_workspace_id, approval_definition_id,
                         definition_version_at_write, retired, created_by)
                    VALUES ($1, $2, $3, $4, true, $5)
                    "#,
                )
                .bind(org_id)
                .bind(target_workspace_id)
                .bind(definition_id)
                .bind(definition.latest_version)
                .bind(actor_user_id)
                .execute(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to retire approval requirement: {e}"),
                })?;
            }
            Some(false) => { /* already retired — idempotent no-op */ }
        }

        self.workspace_state(
            db,
            system_pool,
            definition_service,
            org_id,
            target_workspace_id,
            actor_user_id,
        )
        .await
    }

    /// Explicitly create or reuse one pending request for every selected human
    /// actor against an exact existing definition version.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_human_requests(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        source_workspace_id: Uuid,
        requested_by: Uuid,
        req: CreateHumanApprovalRequests,
    ) -> Result<Vec<HumanApprovalRequestAdmission>, G5Error> {
        let req = CreateHumanApprovalRequests {
            phase: workspace_approval_phase(&req.phase)?,
            note: normalized_request_note(req.note)?,
            ..req
        };
        crate::services::branch_commit::acquire_workspace_commit_fence(
            db.conn(),
            source_workspace_id,
        )
        .await?;
        let pr_id = request_pull_request(db, source_workspace_id, self.policy.pr_required).await?;
        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1 FOR SHARE")
                .bind(source_workspace_id)
                .fetch_one(db.conn())
                .await?;
        if owner != Some(requested_by) {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["workspace_owner".into()],
            });
        }
        if req.definition_version < 1 {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "definitionVersion": "must be positive"
                }),
            });
        }
        validate_phase_value(&req.phase)?;
        // Protocol step 2 (see `lock_target_requirement_set`).
        lock_approval_definition(db.conn(), org_id, req.definition_id).await?;
        let (definition_actor_ids, requested_self_approval): (Vec<Uuid>, bool) = sqlx::query_as(
            r#"
            SELECT humans_list, allow_self_approval
            FROM approval_definitions
            WHERE org_id = $1 AND id = $2 AND version = $3
            "#,
        )
        .bind(org_id)
        .bind(req.definition_id)
        .bind(req.definition_version)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to resolve exact approval definition version: {error}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: "approval definition version".to_string(),
        })?;
        // Phase associations recommend request timing; they never restrict eligibility.

        let current_definition =
            current_self_approval_allowed(db, org_id, req.definition_id).await?;
        let allow_self_approval = requested_self_approval && current_definition;
        let actor_ids = match req.actor_user_ids {
            Some(actor_ids) => {
                if actor_ids.is_empty() {
                    return Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "actorUserIds": "must contain at least one user id when present"
                        }),
                    });
                }
                let mut normalized = actor_ids;
                normalized.sort();
                if normalized.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "actorUserIds": "must not contain duplicate user ids"
                        }),
                    });
                }
                normalized
            }
            None => {
                let mut derived = definition_actor_ids.clone();
                derived.retain(|id| allow_self_approval || Some(*id) != owner);
                derived.sort();
                derived.dedup();
                derived
            }
        };
        if actor_ids.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "actorUserIds": "no human recipients were selected"
                }),
            });
        }
        if actor_ids.iter().any(|id| {
            (!allow_self_approval && Some(*id) == owner) || !definition_actor_ids.contains(id)
        }) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({"actorUserIds":"select configured reviewers; owner review requires Allow self approval"}),
            });
        }
        let active_ids = super::approval_definition_service::lock_active_approval_recipients(
            db, org_id, &actor_ids,
        )
        .await?;
        if active_ids.len() != actor_ids.len() {
            return Err(G5Error::NotFound {
                resource: "one or more active approval request recipients".to_string(),
            });
        }

        let mut admissions = Vec::with_capacity(actor_ids.len());
        for actor_user_id in actor_ids {
            // A request made without a PR has its own chain identity.
            let request_key = format!(
                "{}:{}",
                pr_id.map_or_else(|| "without-pull-request".to_string(), |id| id.to_string()),
                approval_request_key(
                    org_id,
                    source_workspace_id,
                    req.definition_id,
                    req.definition_version,
                    &req.phase,
                    &format!("human:{actor_user_id}"),
                )?
            );
            let request_key = hex::encode(Sha256::digest(request_key.as_bytes()));
            let proposed_id = req
                .client_request_id
                .map_or_else(Uuid::new_v4, |operation| {
                    Uuid::new_v5(
                        &source_workspace_id,
                        format!("review-request:{requested_by}:{operation}:{actor_user_id}")
                            .as_bytes(),
                    )
                });
            if req.client_request_id.is_some() {
                // The workspace fence serializes creation/settlement. Replays
                // must find a settled original too, not open another chain.
                let matches: Option<bool> = sqlx::query_scalar(
                    "SELECT approval_definition_id=$4 AND approval_definition_version=$5 \
                     AND requested_by=$6 AND pull_request_id IS NOT DISTINCT FROM $7 \
                     AND actor_type='human' AND actor_user_id=$8 \
                     AND note IS NOT DISTINCT FROM $9 \
                     FROM approval_requests WHERE org_id=$1 AND source_workspace_id=$2 AND id=$3",
                )
                .bind(org_id)
                .bind(source_workspace_id)
                .bind(proposed_id)
                .bind(req.definition_id)
                .bind(req.definition_version)
                .bind(requested_by)
                .bind(pr_id)
                .bind(actor_user_id)
                .bind(&req.note)
                .fetch_optional(db.conn())
                .await?;
                if let Some(matches) = matches {
                    if !matches {
                        return Err(G5Error::Conflict {
                            reason: "Review request retry does not match the original submission"
                                .into(),
                        });
                    }
                    admissions.push(HumanApprovalRequestAdmission {
                        request_id: proposed_id,
                        actor_user_id,
                        reused: true,
                    });
                    continue;
                }
            }
            // Legacy pending rows can carry any stored phase. Reuse only the
            // same immutable version/recipient/note, under the workspace fence.
            // Any active request for the recipient is a duplicate, whether
            // bound to the open PR or made without one.
            let pending: Option<(Uuid, Option<String>)> = sqlx::query_as(
                "SELECT id,note FROM approval_requests WHERE org_id=$1 AND source_workspace_id=$2 AND approval_definition_id=$3 AND approval_definition_version=$4 AND actor_type='human' AND actor_user_id=$5 AND (without_pull_request OR ($6::uuid IS NOT NULL AND pull_request_id=$6)) AND state='pending' FOR UPDATE",
            ).bind(org_id).bind(source_workspace_id).bind(req.definition_id)
                .bind(req.definition_version).bind(actor_user_id).bind(pr_id)
                .fetch_optional(db.conn()).await?;
            if let Some((request_id, note)) = pending {
                if req.client_request_id.is_some() || note != req.note {
                    return Err(G5Error::Conflict {
                        reason: "This reviewer already has a pending request for this approval. Refresh before requesting again.".into(),
                    });
                }
                admissions.push(HumanApprovalRequestAdmission {
                    request_id,
                    actor_user_id,
                    reused: true,
                });
                continue;
            }
            if pr_id.is_none() {
                // A pending request recorded before PR association was never
                // decidable; it gives way rather than blocking the new chain's
                // pending slot.
                sqlx::query(
                    "UPDATE approval_requests SET state='superseded', closed_at=transaction_timestamp(), updated_at=transaction_timestamp() WHERE org_id=$1 AND source_workspace_id=$2 AND approval_definition_id=$3 AND approval_definition_version=$4 AND actor_type='human' AND actor_user_id=$5 AND pull_request_id IS NULL AND NOT without_pull_request AND state='pending'",
                ).bind(org_id).bind(source_workspace_id).bind(req.definition_id)
                    .bind(req.definition_version).bind(actor_user_id)
                    .execute(db.conn()).await?;
            }
            let inserted = sqlx::query(
                r#"
                INSERT INTO approval_requests (
                    id, org_id, source_workspace_id,
                    approval_definition_id, approval_definition_version,
                    request_key, actor_type, actor_user_id, requested_by, phase, pull_request_id, note,
                    without_pull_request
                ) VALUES ($1, $2, $3, $4, $5, $6, 'human', $7, $8, $9, $10, $11, $10::uuid IS NULL)
                ON CONFLICT (org_id, source_workspace_id, request_key)
                    WHERE state = 'pending'
                DO NOTHING
                "#,
            )
            .bind(proposed_id)
            .bind(org_id)
            .bind(source_workspace_id)
            .bind(req.definition_id)
            .bind(req.definition_version)
            .bind(&request_key)
            .bind(actor_user_id)
            .bind(requested_by)
            .bind(&req.phase)
            .bind(pr_id)
            .bind(&req.note)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to create human approval request: {error}"),
            })?;
            let reused = inserted.rows_affected() == 0;
            if reused && req.client_request_id.is_some() {
                return Err(G5Error::Conflict {
                    reason: "This reviewer already has a pending request for this approval. Refresh before requesting again.".into(),
                });
            }
            let request_id = if reused {
                let existing: (Uuid, Uuid, i32, String, Option<Uuid>, Option<String>) =
                    sqlx::query_as(
                        r#"
                    SELECT id, approval_definition_id,
                           approval_definition_version, actor_type, actor_user_id,
                           phase
                    FROM approval_requests
                    WHERE org_id = $1 AND source_workspace_id = $2
                      AND request_key = $3 AND state = 'pending'
                    "#,
                    )
                    .bind(org_id)
                    .bind(source_workspace_id)
                    .bind(&request_key)
                    .fetch_optional(db.conn())
                    .await?
                    .ok_or_else(|| G5Error::Conflict {
                        reason: "identical human approval request did not converge".to_string(),
                    })?;
                if existing.1 != req.definition_id
                    || existing.2 != req.definition_version
                    || existing.3 != "human"
                    || existing.4 != Some(actor_user_id)
                    || existing.5.as_deref() != Some(req.phase.as_str())
                {
                    return Err(G5Error::Conflict {
                        reason: "approval request key collides with unequal pending state"
                            .to_string(),
                    });
                }
                existing.0
            } else {
                proposed_id
            };
            admissions.push(HumanApprovalRequestAdmission {
                request_id,
                actor_user_id,
                reused,
            });
        }
        Ok(admissions)
    }

    /// Cancel one pending human approval request. Only the authenticated user
    /// recorded as `requested_by` may close it; existing decisions are not
    /// touched and no successor request is created.
    #[allow(clippy::too_many_arguments)]
    pub async fn cancel_human_request(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        source_workspace_id: Uuid,
        requested_by: Uuid,
        request_id: Uuid,
    ) -> Result<WorkspaceApprovalsState, G5Error> {
        let is_current_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL)",
        )
        .bind(org_id)
        .bind(requested_by)
        .fetch_one(system_pool)
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to validate approval request requester: {error}"),
        })?;
        if !is_current_member {
            return Err(G5Error::NotFound {
                resource: "approval request".to_string(),
            });
        }

        // Protocol step 2 (see `lock_target_requirement_set`): the chain's
        // definition is locked before the request row itself.
        let definition_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT approval_definition_id FROM approval_requests
             WHERE org_id = $1 AND source_workspace_id = $2 AND id = $3",
        )
        .bind(org_id)
        .bind(source_workspace_id)
        .bind(request_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to resolve approval request definition: {error}"),
        })?;
        if let Some(definition_id) = definition_id {
            lock_approval_definition(db.conn(), org_id, definition_id).await?;
        }
        let state: String = sqlx::query_scalar(
            r#"
            SELECT state
            FROM approval_requests
            WHERE org_id = $1
              AND source_workspace_id = $2
              AND id = $3
              AND requested_by = $4
            FOR UPDATE
            "#,
        )
        .bind(org_id)
        .bind(source_workspace_id)
        .bind(request_id)
        .bind(requested_by)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to resolve approval request cancellation: {error}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: "approval request".to_string(),
        })?;

        match state.as_str() {
            "pending" => {
                let cancelled = sqlx::query(
                    r#"
                    UPDATE approval_requests
                    SET state = 'cancelled',
                        closed_at = transaction_timestamp(),
                        updated_at = transaction_timestamp()
                    WHERE org_id = $1
                      AND source_workspace_id = $2
                      AND id = $3
                      AND requested_by = $4
                      AND state = 'pending'
                    "#,
                )
                .bind(org_id)
                .bind(source_workspace_id)
                .bind(request_id)
                .bind(requested_by)
                .execute(db.conn())
                .await
                .map_err(|error| G5Error::Internal {
                    message: format!("Failed to cancel approval request: {error}"),
                })?;
                if cancelled.rows_affected() != 1 {
                    return Err(G5Error::Conflict {
                        reason: format!("approval request {request_id} is no longer pending"),
                    });
                }
            }
            "cancelled" => { /* exact requester replay after a lost acknowledgement */ }
            state => {
                return Err(G5Error::Conflict {
                    reason: format!("approval request {request_id} is already {state}"),
                });
            }
        }

        self.workspace_state(
            db,
            system_pool,
            definition_service,
            org_id,
            source_workspace_id,
            requested_by,
        )
        .await
    }

    /// Record a human decision for this SOURCE workspace: approve, reject,
    /// or `deferred` — the durable Undo that withdraws the caller's current
    /// decision (state derives back to pending; the ledger keeps every row).
    #[allow(clippy::too_many_arguments)] // route-surface identity params, repo precedent
    pub async fn record_human_decision(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        source_workspace_id: Uuid,
        actor_user_id: Uuid,
        req: DecisionRequest,
    ) -> Result<WorkspaceApprovalsState, G5Error> {
        crate::services::branch_commit::acquire_workspace_commit_fence(
            db.conn(),
            source_workspace_id,
        )
        .await?;
        let pr_id = request_pull_request(db, source_workspace_id, self.policy.pr_required).await?;
        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1 FOR SHARE")
                .bind(source_workspace_id)
                .fetch_one(db.conn())
                .await?;
        if req.request_id.is_none() {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["addressed_review_request".into()],
            });
        }
        if req.outcome == "rejected" && req.comment.as_ref().is_none_or(|c| c.trim().is_empty()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({"comment":"Explain the requested changes"}),
            });
        }
        let captured = normalized_head_vector(&serde_json::json!(&req.reviewed_git_heads))
            .map_err(|_| G5Error::ValidationError {
                details: serde_json::json!({"reviewedGitHeads":"Provide a complete, valid pushed repository vector"}),
            })?;
        if req.outcome != "approved" && req.outcome != "rejected" && req.outcome != "deferred" {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "outcome": "must be 'approved', 'rejected', or 'deferred'"
                }),
            });
        }
        let comment = match req.comment.as_deref().map(str::trim) {
            Some("") => {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({ "comment": "must not be blank when present" }),
                });
            }
            other => other.map(str::to_string),
        };

        let is_current_member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL)",
        )
        .bind(org_id)
        .bind(actor_user_id)
        .fetch_one(system_pool)
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to validate approval decision actor: {error}"),
        })?;
        if !is_current_member {
            return Err(G5Error::NotFound {
                resource: "approval request".to_string(),
            });
        }

        // The settled phase is explicit (g5.workbench.gates). Request-addressed
        // settlement validates against the request's exact immutable
        // definition version and the chain's own phase. Requestless decisions
        // are refused above.
        let mut phase = req.phase.clone();
        validate_phase_value(&phase)?;
        // Protocol step 2: a decision or withdrawal serializes with the
        // admissions and publications that read or extend this definition.
        lock_approval_definition(db.conn(), org_id, req.definition_id).await?;

        if owner == Some(actor_user_id)
            && !current_self_approval_allowed(db, org_id, req.definition_id).await?
        {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["self_approval_disabled".into()],
            });
        }
        // The decision belongs to its request's PR, or to none.
        let mut decision_pr_id = pr_id;
        let (request_settlement, request_definition_version) = if let Some(request_id) =
            req.request_id
        {
            let request = sqlx::query_as::<
                _,
                (
                    i32,
                    String,
                    Option<String>,
                    Vec<String>,
                    Option<String>,
                    bool,
                    Option<Uuid>,
                ),
            >(
                r#"
                    SELECT r.approval_definition_version, r.state,
                           r.settlement_fingerprint, d.phases, r.phase, d.allow_self_approval,
                           r.pull_request_id
                    FROM approval_requests r
                    JOIN approval_definitions d
                      ON d.org_id = r.org_id
                     AND d.id = r.approval_definition_id
                     AND d.version = r.approval_definition_version
                    WHERE r.org_id = $1
                      AND r.source_workspace_id = $2
                      AND r.approval_definition_id = $3
                      AND r.id = $4
                      AND r.actor_type = 'human'
                      AND r.actor_user_id = $5
                      AND (($6::uuid IS NOT NULL AND r.pull_request_id = $6)
                           OR (r.without_pull_request AND NOT $7))
                    FOR UPDATE OF r
                    "#,
            )
            .bind(org_id)
            .bind(source_workspace_id)
            .bind(req.definition_id)
            .bind(request_id)
            .bind(actor_user_id)
            .bind(pr_id)
            .bind(self.policy.pr_required)
            .fetch_optional(db.conn())
            .await?
            .ok_or_else(|| G5Error::Conflict {
                reason: "no matching approval request exists for this actor".to_string(),
            })?;
            if owner == Some(actor_user_id) && !request.5 {
                return Err(G5Error::Forbidden {
                    missing_scopes: vec!["self_approval_disabled".into()],
                });
            }
            decision_pr_id = request.6;
            // An old request keeps its historical phase in immutable replay
            // evidence; the caller's workbench phase confers no authority.
            phase = request
                .4
                .clone()
                .unwrap_or_else(|| WORKSPACE_APPROVAL_COMPAT_PHASE.to_string());
            let settlement = ApprovalRequestSettlementV2 {
                schema_version: APPROVAL_REQUEST_SETTLEMENT_SCHEMA_VERSION,
                request_id,
                org_id,
                source_workspace_id,
                approval_definition_id: req.definition_id,
                approval_definition_version: request.0,
                phase: phase.clone(),
                actor_slot: format!("human:{actor_user_id}"),
                outcome: req.outcome.clone(),
                comment: comment.clone(),
            };
            let fingerprint = settlement
                .fingerprint()
                .map_err(|message| G5Error::Internal {
                    message: format!("Human approval settlement is invalid: {message}"),
                })?;
            // The vector is part of replay identity; retries cannot retarget a
            // settled decision merely by keeping the same text and outcome.
            let fingerprint = hex::encode(Sha256::digest(
                format!("{fingerprint}:{}", serde_json::json!(captured)).as_bytes(),
            ));
            match request.1.as_str() {
                "pending" => (Some((request_id, fingerprint)), request.0),
                "settled" if request.2.as_deref() == Some(fingerprint.as_str()) => {
                    return self
                        .workspace_state(
                            db,
                            system_pool,
                            definition_service,
                            org_id,
                            source_workspace_id,
                            actor_user_id,
                        )
                        .await;
                }
                "settled" => {
                    return Err(G5Error::Conflict {
                        reason: format!(
                            "approval request {request_id} settlement replay conflicts"
                        ),
                    });
                }
                state => {
                    return Err(G5Error::Conflict {
                        reason: format!("approval request {request_id} is already {state}"),
                    });
                }
            }
        } else {
            let latest_definition = definition_service
                .get_latest(db, system_pool, org_id, req.definition_id)
                .await?;
            validate_phase_value(&phase)?;
            (None, latest_definition.latest_version)
        };

        // Bind the review cut only when this action is admitted. Request
        // replay is checked first so an equal lost-ACK retry does not depend
        // on mutable workspace topology or branch heads.
        let Some(target_id) = self.merge_target_of(db, source_workspace_id).await? else {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "workspaceId": "this workspace has no merge target; there is nothing to approve"
                }),
            });
        };
        let (current, _) = self.source_head_state(db, source_workspace_id).await?;
        let current = normalized_head_vector(&current)?;
        let selected = captured;
        let canonical = serde_json::json!(selected);
        if selected != current {
            let recorded: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM workspace_revision_events WHERE workspace_id=$1 AND heads=$2) OR EXISTS(SELECT 1 FROM workspace_pull_requests WHERE id=$3 AND source_heads=$2) OR EXISTS(SELECT 1 FROM workspace_pull_request_events WHERE pull_request_id=$3 AND source_heads=$2)"
            ).bind(source_workspace_id).bind(&canonical).bind(pr_id).fetch_one(db.conn()).await?;
            if !recorded {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({"reviewedGitHeads":"Select a recorded pushed workspace revision"}),
                });
            }
        }
        let heads = canonical;
        let (base_heads, _) = self.source_head_state(db, target_id).await?;
        let heads = serde_json::to_value(normalized_head_vector(&heads)?).map_err(|error| {
            G5Error::Internal {
                message: format!("Failed to serialize normalized approval heads: {error}"),
            }
        })?;
        let _base_heads = normalized_head_vector(&base_heads)?;
        let status_id = Uuid::new_v4();
        let default_comment = match req.outcome.as_str() {
            "approved" => "Approved",
            "rejected" => "Changes requested",
            _ => "Decision withdrawn", // Only deferred reaches this validated path.
        };
        sqlx::query("INSERT INTO comment_index(id,org_id,workspace_id,author_id,scope,content,origin_branch,phase) SELECT $1,$2,$3,$4,'workspace',$5,name,$6 FROM workspaces WHERE id=$3")
            .bind(status_id).bind(org_id).bind(source_workspace_id).bind(actor_user_id)
            .bind(comment.as_deref().unwrap_or(default_comment)).bind(&phase).execute(db.conn()).await?;

        sqlx::query(
            r#"
            INSERT INTO approval_statuses
                (id, org_id, source_workspace_id, approval_definition_id,
                 approval_definition_version, outcome, comment,
                 actor_type, actor_user_id, reviewed_git_heads, phase, pull_request_id, request_id, comment_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'human', $8, $9, $10, $11, $12, $1)
            "#,
        )
        .bind(status_id)
        .bind(org_id)
        .bind(source_workspace_id)
        .bind(req.definition_id)
        .bind(request_definition_version)
        .bind(&req.outcome)
        .bind(comment.as_deref())
        .bind(actor_user_id)
        .bind(&heads)
        .bind(&phase)
        .bind(decision_pr_id)
        .bind(req.request_id)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to record approval decision: {e}"),
        })?;
        if let Some((request_id, settlement_fingerprint)) = request_settlement {
            let settled = sqlx::query(
                r#"
                UPDATE approval_requests
                SET state = 'settled', settlement_fingerprint = $1,
                    closed_at = transaction_timestamp(),
                    updated_at = transaction_timestamp()
                WHERE org_id = $2 AND id = $3 AND state = 'pending'
                "#,
            )
            .bind(&settlement_fingerprint)
            .bind(org_id)
            .bind(request_id)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to settle approval request: {error}"),
            })?;
            if settled.rows_affected() != 1 {
                return Err(G5Error::Conflict {
                    reason: format!("approval request {request_id} is no longer pending"),
                });
            }
        }

        self.workspace_state(
            db,
            system_pool,
            definition_service,
            org_id,
            source_workspace_id,
            actor_user_id,
        )
        .await
    }

    // ── internals ──────────────────────────────────────────────────────────

    /// Latest-event activity for (target, definition): None = never attached.
    async fn latest_event_is_active(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        target_workspace_id: Uuid,
        definition_id: Uuid,
    ) -> Result<Option<bool>, G5Error> {
        let retired: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT retired FROM approval_configs
            WHERE org_id = $1 AND target_workspace_id = $2
              AND approval_definition_id = $3
            ORDER BY seq DESC
            LIMIT 1
            "#,
        )
        .bind(org_id)
        .bind(target_workspace_id)
        .bind(definition_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to resolve approval requirement state: {e}"),
        })?;
        Ok(retired.map(|r| !r))
    }

    pub(crate) async fn merge_target_of(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Option<Uuid>, G5Error> {
        let row: Option<WorkspaceTargetRow> = sqlx::query_as(
            "SELECT merge_target_workspace_id, cloned_from_id FROM workspaces WHERE id = $1",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to resolve workspace merge target: {e}"),
        })?;
        let row = row.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        // Model convention [D19]: explicit merge target wins over lineage.
        Ok(row.merge_target_workspace_id.or(row.cloned_from_id))
    }

    async fn capture_source_versions(
        &self,
        db: &mut ScopedDb,
        workspace_ids: &[Uuid],
    ) -> Result<
        Vec<crate::services::git::workspace_repositories::WorkspaceRepositoryVersion>,
        G5Error,
    > {
        use crate::services::git::workspace_repositories::{
            read_repository_versions, WorkspaceRepository,
        };
        let members: Vec<WorkspaceRepository> = sqlx::query_as(
            "SELECT m.org_id, m.workspace_id, m.repo_id,
                    r.name AS repo_name, r.slug AS repo_slug, m.root_path, m.role,
                    CASE WHEN w.is_base THEN r.default_branch ELSE w.branch_name END AS branch_name,
                    w.is_base
             FROM workspace_repos m
             JOIN workspaces w ON w.id = m.workspace_id AND w.org_id = m.org_id
             JOIN repos r ON r.id = m.repo_id AND r.org_id = m.org_id
             WHERE w.id = ANY($1) AND w.deleted_at IS NULL
             ORDER BY w.id, m.repo_id",
        )
        .bind(workspace_ids)
        .fetch_all(db.conn())
        .await?;
        let represented = members
            .iter()
            .map(|member| member.workspace_id)
            .collect::<std::collections::HashSet<_>>();
        if workspace_ids
            .iter()
            .any(|workspace_id| !represented.contains(workspace_id))
        {
            return Err(G5Error::Conflict {
                reason: "Approval requires a complete workspace repository vector".into(),
            });
        }
        read_repository_versions(&self.repo_storage.scoped(&*db), members).await
    }

    pub(crate) async fn capture_formal_review_heads(
        &self,
        db: &mut ScopedDb,
        workspace: Uuid,
    ) -> Result<Vec<MergeSourceHead>, G5Error> {
        let (heads, _) = self.source_head_state(db, workspace).await?;
        let heads = normalized_head_vector(&heads)?;
        let checks = heads
            .iter()
            .map(|head| {
                Ok((
                    Uuid::parse_str(&head.repo_id).map_err(|_| G5Error::Internal {
                        message: "invalid review repository".into(),
                    })?,
                    head.branch_name.clone(),
                    head.head_commit.clone(),
                ))
            })
            .collect::<Result<Vec<_>, G5Error>>()?;
        crate::services::workspace_reader::verify_committed_heads(
            db,
            &self.repo_storage,
            workspace,
            &checks,
            "formal_review_capture",
        )
        .await?;
        Ok(heads)
    }

    /// Approval freshness and Home ordering come from the same captured vector.
    async fn source_head_state(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<(serde_json::Value, Option<chrono::DateTime<chrono::Utc>>), G5Error> {
        let versions = self.capture_source_versions(db, &[workspace_id]).await?;
        let actionable_at = versions.iter().map(|version| version.committed_at).max();
        let heads = versions
            .into_iter()
            .map(|version| MergeSourceHead {
                repo_id: version.member.repo_id.to_string(),
                workspace_id: version.member.workspace_id.to_string(),
                branch_name: version.member.branch_name,
                head_commit: version.head_commit,
            })
            .collect::<Vec<_>>();
        Ok((
            serde_json::to_value(heads).map_err(|error| G5Error::Internal {
                message: format!("Failed to serialize approval head vector: {error}"),
            })?,
            actionable_at,
        ))
    }

    async fn display_names(
        &self,
        system_pool: &PgPool,
        ids: &[Uuid],
    ) -> Result<HashMap<Uuid, String>, G5Error> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, display_name FROM users WHERE id = ANY($1)")
                .bind(ids)
                .fetch_all(system_pool)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve user display names: {e}"),
                })?;
        Ok(rows.into_iter().collect())
    }

    async fn target_side_state(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(Vec<ActiveRequirementRecord>, Vec<AttachmentEventRecord>), G5Error> {
        let events: Vec<ConfigEventRow> = sqlx::query_as(
            r#"
            SELECT approval_definition_id, retired, created_by, created_at
            FROM approval_configs
            WHERE org_id = $1 AND target_workspace_id = $2
            ORDER BY seq DESC
            "#,
        )
        .bind(org_id)
        .bind(target_workspace_id)
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list approval config events: {e}"),
        })?;

        // Latest event per definition decides activity; first occurrence in
        // the DESC stream is the latest.
        let mut latest: HashMap<Uuid, &ConfigEventRow> = HashMap::new();
        for ev in &events {
            latest.entry(ev.approval_definition_id).or_insert(ev);
        }

        let mut actor_ids: Vec<Uuid> = events.iter().map(|e| e.created_by).collect();
        actor_ids.sort();
        actor_ids.dedup();
        let names = self.display_names(system_pool, &actor_ids).await?;

        let mut required_into = Vec::new();
        for (definition_id, ev) in latest.iter().filter(|(_, ev)| !ev.retired) {
            let definition = definition_service
                .get_latest(db, system_pool, org_id, *definition_id)
                .await?;
            required_into.push(ActiveRequirementRecord {
                definition,
                attached_by: ev.created_by,
                attached_by_display_name: names
                    .get(&ev.created_by)
                    .cloned()
                    .unwrap_or_else(|| "Unknown user".to_string()),
                attached_at: ev.created_at,
            });
        }
        required_into.sort_by(|a, b| a.definition.name.cmp(&b.definition.name));

        // History needs definition names; resolve one snapshot per distinct id.
        let mut def_names: HashMap<Uuid, String> = HashMap::new();
        for id in latest.keys() {
            if let std::collections::hash_map::Entry::Vacant(entry) = def_names.entry(*id) {
                let definition = definition_service
                    .get_latest(db, system_pool, org_id, *id)
                    .await?;
                entry.insert(definition.name);
            }
        }

        let attachment_history = events
            .iter()
            .map(|ev| AttachmentEventRecord {
                definition_id: ev.approval_definition_id,
                definition_name: def_names
                    .get(&ev.approval_definition_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown definition".to_string()),
                retired: ev.retired,
                actor_user_id: ev.created_by,
                actor_display_name: names
                    .get(&ev.created_by)
                    .cloned()
                    .unwrap_or_else(|| "Unknown user".to_string()),
                created_at: ev.created_at,
            })
            .collect();

        Ok((required_into, attachment_history))
    }

    async fn outgoing_state(
        &self,
        db: &mut ScopedDb,
        system_pool: &PgPool,
        definition_service: &ApprovalDefinitionService,
        org_id: Uuid,
        workspace_id: Uuid,
        caller_user_id: Uuid,
    ) -> Result<Option<OutgoingSection>, G5Error> {
        let Some(target_id) = self.merge_target_of(db, workspace_id).await? else {
            return Ok(None);
        };
        let target_name: Option<String> =
            sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1")
                .bind(target_id)
                .fetch_optional(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve target workspace name: {e}"),
                })?;
        let target_name = target_name.unwrap_or_else(|| "Unknown workspace".to_string());

        let (target_requirements, _) = self
            .target_side_state(db, system_pool, definition_service, org_id, target_id)
            .await?;

        // All durable decisions from THIS workspace, newest first. Tasks,
        // tasks, logs, and artifacts never enter this projection directly.
        let statuses: Vec<StatusRow> = sqlx::query_as(&format!(
            r#"
            SELECT ({HUMAN_EVIDENCE_IN_SCOPE}) IS TRUE AS in_scope,
                   approval_definition_id, approval_definition_version, outcome,
                   comment, actor_type, actor_user_id, reviewed_git_heads, COALESCE(request_id, (SELECT r.approval_request_id FROM agent_approval_reviews r WHERE r.id=approval_statuses.agent_review_id)) AS request_id, agent_review_id,
                   created_at, ({AGENT_SUMMARY_AUTHORED}) IS TRUE AS summary_authored
            FROM approval_statuses
            WHERE org_id = $1 AND source_workspace_id = $2
            ORDER BY seq DESC
            "#
        ))
        .bind(org_id)
        .bind(workspace_id)
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list approval decisions: {e}"),
        })?;
        let requests: Vec<ApprovalRequestRow> = sqlx::query_as(
            r#"
            SELECT ((request.without_pull_request AND NOT $3)
                    OR request.pull_request_id IN (SELECT id FROM workspace_pull_requests WHERE source_workspace_id=$2 AND status='open')) IS TRUE AS active_pr,
                   request.id, request.approval_definition_id,
                   request.approval_definition_version, request.actor_type,
                   request.actor_user_id, request.requested_by, request.state,
                   definition.name AS definition_name, definition.allow_self_approval, request.note, request.phase,
                   request.created_at, request.closed_at
            FROM approval_requests request
            JOIN approval_definitions definition
              ON definition.org_id = request.org_id
             AND definition.id = request.approval_definition_id
             AND definition.version = request.approval_definition_version
            WHERE request.org_id = $1 AND request.source_workspace_id = $2
            ORDER BY request.seq DESC
            "#,
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(self.policy.pr_required)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to list approval requests: {error}"),
        })?;
        let review_runs: Vec<AgentApprovalReviewRow> = sqlx::query_as(
            r#"
            SELECT link.approval_request_id, task.id AS task_id, task.chat_session_id AS session_id,
                   task.status AS state,
                   CASE WHEN task.status='failed' THEN 'execution_failed' WHEN task.status='cancelled' THEN 'cancelled' END AS terminal_reason,
                   task.created_at,task.started_at,NULL::timestamptz AS ended_at,task.updated_at,
                   NULL::uuid AS review_id,NULL::text AS stage,false AS cleanup_pending,NULL::text AS cleanup_error,false AS cleanup_overdue,false AS report_available,false AS retry_available,NULL::text AS content_sha256
            FROM approval_request_tasks link JOIN tasks task ON task.org_id=link.org_id AND task.id=link.task_id
            WHERE link.org_id=$1 AND task.workspace_id=$2
              AND NOT EXISTS(SELECT 1 FROM agent_approval_reviews review WHERE review.task_id=task.id)
            UNION ALL
            SELECT review.approval_request_id,review.task_id,NULL::uuid,
                   CASE WHEN review.stage='effective' THEN 'succeeded' WHEN review.stage IN ('failed','cancelled') THEN review.stage WHEN review.stage IN ('capture','start') THEN 'queued' ELSE 'running' END,
                   CASE WHEN review.failure_cause IS NOT NULL THEN review.failure_stage || ': ' || review.failure_cause END,
                   review.created_at,review.claimed_at,review.ended_at,review.updated_at,
                   review.id,review.stage,review.cleanup_completed_at IS NULL,review.cleanup_error,public.formal_review_cleanup_overdue(review.ended_at,review.cleanup_completed_at,review.cleanup_error),review.artifact_id IS NOT NULL,review.stage='failed' AND review.invocation IS NOT NULL,artifact.content_sha256
            FROM agent_approval_reviews review LEFT JOIN agent_approval_artifacts artifact ON artifact.id=review.artifact_id AND artifact.org_id=review.org_id
            WHERE review.org_id=$1 AND review.source_workspace_id=$2
            ORDER BY approval_request_id,created_at,task_id
            "#,
        )
        .bind(org_id)
        .bind(workspace_id)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to list approval-review runs: {error}"),
        })?;
        let mut review_runs_by_request: HashMap<Uuid, Vec<AgentApprovalReviewRecord>> =
            HashMap::new();
        for run in review_runs {
            review_runs_by_request
                .entry(run.approval_request_id)
                .or_default()
                .push(agent_approval_review_record(run));
        }

        let mut actor_ids: Vec<Uuid> = statuses.iter().filter_map(|s| s.actor_user_id).collect();
        actor_ids.extend(requests.iter().filter_map(|request| request.requested_by));
        actor_ids.extend(requests.iter().filter_map(|request| request.actor_user_id));
        actor_ids.sort();
        actor_ids.dedup();
        let names = self.display_names(system_pool, &actor_ids).await?;
        let (current_heads, source_head_actionable_at) =
            self.source_head_state(db, workspace_id).await?;
        let current_heads = normalized_head_vector(&current_heads)?;

        let owner: Option<Uuid> =
            sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
                .bind(workspace_id)
                .fetch_one(db.conn())
                .await?;

        // History is request/status complete and deliberately independent from
        // the target's active configuration. This union therefore includes
        // discretionary and retired-definition records that must remain
        // visible without becoming merge requirements.
        let mut history_definition_ids = statuses
            .iter()
            .map(|status| status.approval_definition_id)
            .chain(
                requests
                    .iter()
                    .map(|request| request.approval_definition_id),
            )
            .collect::<Vec<_>>();
        history_definition_ids.sort();
        history_definition_ids.dedup();
        let history_definition_rows: Vec<(Uuid, String, i32)> = if history_definition_ids.is_empty()
        {
            Vec::new()
        } else {
            sqlx::query_as(
                r#"
                SELECT id,
                       (array_agg(name ORDER BY version DESC))[1] AS latest_name,
                       COALESCE(
                           MAX(version) FILTER (WHERE invalidates_previous = true),
                           1
                       ) AS invalidation_boundary
                FROM approval_definitions
                WHERE org_id = $1 AND id = ANY($2)
                GROUP BY id
                "#,
            )
            .bind(org_id)
            .bind(&history_definition_ids)
            .fetch_all(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to resolve approval history definitions: {error}"),
            })?
        };
        let mut history_definition_metadata = history_definition_rows
            .into_iter()
            .map(|(definition_id, name, boundary)| (definition_id, (name, boundary)))
            .collect::<HashMap<_, _>>();
        let mut history = Vec::with_capacity(history_definition_ids.len());
        for definition_id in history_definition_ids {
            let (definition_name, boundary) = history_definition_metadata
                .remove(&definition_id)
                .unwrap_or_else(|| ("Unknown definition".to_string(), 1));
            let definition_requests = requests
                .iter()
                .filter(|request| request.approval_definition_id == definition_id)
                .collect::<Vec<_>>();
            let request_records = definition_requests
                .iter()
                .map(|request| approval_request_record(request, &names, &review_runs_by_request))
                .collect();
            let decision_log = statuses
                .iter()
                .filter(|status| status.approval_definition_id == definition_id)
                .map(|status| {
                    decision_record(
                        status,
                        boundary,
                        &current_heads,
                        self.policy.on_source_commit_vector_change,
                        &names,
                        &definition_requests,
                    )
                })
                .collect();
            history.push(OutgoingHistoryRecord {
                definition_id,
                definition_name,
                requests: request_records,
                decision_log,
            });
        }

        let mut requirements = Vec::new();
        for requirement in target_requirements {
            let attached_at = requirement.attached_at;
            let definition = requirement.definition;
            // Schema D13 eligibility: a decision at version v is usable at
            // latest r iff no version in (v, r] invalidates previous.
            let invalidation_boundary: Option<(i32, chrono::DateTime<chrono::Utc>)> =
                sqlx::query_as(
                    r#"
                SELECT version, created_at
                FROM approval_definitions
                WHERE org_id = $1 AND id = $2 AND invalidates_previous = true
                ORDER BY version DESC
                LIMIT 1
                "#,
                )
                .bind(org_id)
                .bind(definition.id)
                .fetch_optional(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve invalidation boundary: {e}"),
                })?;
            let boundary = invalidation_boundary.map_or(1, |(version, _)| version);
            let mut base_actionable_at = attached_at;
            if let Some((_, invalidated_at)) = invalidation_boundary {
                base_actionable_at = base_actionable_at.max(invalidated_at);
            }
            if let Some(head_changed_at) = source_head_actionable_at {
                base_actionable_at = base_actionable_at.max(head_changed_at);
            }

            let def_statuses: Vec<&StatusRow> = statuses
                .iter()
                .filter(|s| s.approval_definition_id == definition.id)
                .collect();
            let def_requests: Vec<&ApprovalRequestRow> = requests
                .iter()
                .filter(|request| request.approval_definition_id == definition.id)
                .collect();

            // One entry per attached type. The phase field is retained for
            // transport compatibility, never to select approval policy.
            let phase = WORKSPACE_APPROVAL_COMPAT_PHASE.to_string();
            let policy = self.policy.on_source_commit_vector_change;
            let request_records = def_requests
                .iter()
                .map(|request| approval_request_record(request, &names, &review_runs_by_request))
                .collect::<Vec<_>>();
            let decision_log: Vec<DecisionRecord> = def_statuses
                .iter()
                .map(|status| {
                    decision_record(
                        status,
                        boundary,
                        &current_heads,
                        policy,
                        &names,
                        &def_requests,
                    )
                })
                .collect();

            // Full history remains above; only in-scope human evidence and
            // active requests participate in status, tasks and eligibility.
            let counted_statuses: Vec<&StatusRow> = def_statuses
                .iter()
                .copied()
                .filter(|s| s.actor_type != "human" || s.in_scope)
                .collect();
            let active_requests: Vec<&ApprovalRequestRow> = def_requests
                .iter()
                .copied()
                .filter(|r| r.actor_type != "human" || r.active_pr)
                .collect();
            let deferred_at = caller_deferred_at(
                counted_statuses.iter().copied(),
                caller_user_id,
                boundary,
                &current_heads,
                policy,
            );
            let caller_can_decide = (owner != Some(caller_user_id)
                || definition.allow_self_approval)
                && active_requests.iter().any(|r| {
                    r.actor_type == "human"
                        && r.actor_user_id == Some(caller_user_id)
                        && r.state == "pending"
                        && (owner != Some(caller_user_id) || r.allow_self_approval)
                });
            let mut actionable_at = base_actionable_at;
            if let Some(deferred_at) = deferred_at {
                actionable_at = actionable_at.max(deferred_at);
            }
            let mut approvers = Vec::new();
            for human in &definition.humans {
                let pending_request = active_requests.iter().find(|request| {
                    request.actor_type == "human"
                        && request.actor_user_id == Some(human.user_id)
                        && request.state == "pending"
                        && request.approval_definition_version >= boundary
                });
                let current = counted_decision(
                    counted_statuses.iter().copied(),
                    "human",
                    Some(human.user_id),
                    boundary,
                    Some(&current_heads),
                    policy,
                );
                // A latest counted `deferred` row is the durable Undo: it
                // never satisfies and carries no verdict, so it derives to
                // pending (schema D13) while staying in the log.
                let (state, comment, decided_at) = match current {
                    Some(s) if s.outcome != "deferred" => {
                        (s.outcome.clone(), s.comment.clone(), Some(s.created_at))
                    }
                    Some(_) => ("pending".to_string(), None, None),
                    None if pending_request.is_some() => ("pending".to_string(), None, None),
                    None => ("not_requested".to_string(), None, None),
                };
                let actor_request = pending_request.copied();
                approvers.push(ApproverStateRecord {
                    user_id: human.user_id,
                    display_name: human.display_name.clone(),
                    email: human.email.clone(),
                    still_member: human.still_member,
                    deactivated_at: human.deactivated_at,
                    state,
                    comment,
                    decided_at,
                    request_id: actor_request.map(|request| request.id),
                    request_state: actor_request.map(|request| request.state.clone()),
                    requested_at: actor_request.map(|request| request.created_at),
                    closed_at: actor_request.and_then(|request| request.closed_at),
                });
            }

            let disallowed_owner = (!definition.allow_self_approval).then_some(owner).flatten();
            let evaluation = requirement_status::evaluate(&RequirementInput {
                require_all: definition.require_all_reviewers,
                human_override: definition.human_approvals_override_agent,
                agent_configured: definition.agent_type.is_some(),
                agent_name: agent_display_name(&definition.name),
                humans: definition
                    .humans
                    .iter()
                    .map(|human| {
                        let eligible = Some(human.user_id) != disallowed_owner;
                        HumanCandidate {
                            user_id: human.user_id,
                            display_name: human.display_name.clone(),
                            eligible,
                            requestable: eligible
                                && human.still_member
                                && human.deactivated_at.is_none(),
                        }
                    })
                    .collect(),
                statuses: counted_statuses,
                requests: def_requests
                    .iter()
                    .map(|request| RequestFact {
                        actor_type_agent: request.actor_type == "agent",
                        actor_user_id: request.actor_user_id,
                        active: request.state == "pending"
                            && (request.actor_type != "human" || request.active_pr)
                            && request.approval_definition_version >= boundary,
                    })
                    .collect(),
                boundary,
                current_heads: Some(&current_heads),
                policy,
            });
            let state = evaluation.status.as_str().to_string();

            tracing::debug!(
                source_workspace_id = %workspace_id,
                target_workspace_id = %target_id,
                definition_id = %definition.id,
                definition_version = definition.latest_version,
                phase,
                human_approver_count = approvers.len(),
                agent_configured = definition.agent_type.is_some(),
                decision_count = decision_log.len(),
                outcome = %state,
                "Evaluated durable workspace approval statuses"
            );

            requirements.push(OutgoingRequirementRecord {
                definition: definition.clone(),
                phase,
                state,
                evaluation,
                approvers,
                caller_can_decide,
                requests: request_records,
                actionable_at,
                decision_log,
            });
        }

        // The admission every shipping surface shows: all attached target
        // Engineering entries just projected, against the current cut.
        let merge_admission = merge_admission_from_requirements(
            workspace_id,
            target_id,
            current_heads,
            requirements.iter().map(|entry| MergeAdmissionRequirement {
                definition_id: entry.definition.id,
                definition_name: entry.definition.name.clone(),
                definition_version: entry.definition.latest_version,
                state: entry.state.clone(),
            }),
        );

        Ok(Some(OutgoingSection {
            target_workspace_id: target_id,
            target_workspace_name: target_name,
            requirements,
            history,
            merge_admission,
            policy: self.policy,
        }))
    }
}

// Approval policy lock protocol (g5.governance.approval.requirements,
// g5.governance.merge). Every writer of the policy a merge admission reads
// — attached requirements, definition versions, decisions and withdrawals,
// request chains, agent review settlement — and the admission itself take
// transaction-scoped advisory locks in ONE order, so an admission never
// combines snapshots from different writers:
//
//   1. `lock_target_requirement_set(org, target)` — attach, retire and
//      admission (admission must also see any definition attached while it
//      runs, which the per-definition locks alone could not guarantee);
//   2. `lock_approval_definition(org, definition)`, by definition id
//      ascending when several — attach/retire (theirs), definition
//      publishing (`ApprovalDefinitionService::publish_version`), human
//      decisions and withdrawals (`record_human_decision`), request creation
//      and cancellation, agent review settlement
//      (`ApprovalReviewService::settle_terminal`), and admission for every
//      attached definition.
//
// A writer that needs only step 2 takes it without step 1; nobody takes a
// definition lock before the set lock in the same transaction.

/// Transaction-scoped lock on a target workspace's whole requirement set
/// (protocol step 1).
async fn lock_target_requirement_set(
    db: &mut ScopedDb,
    org_id: Uuid,
    target_workspace_id: Uuid,
) -> Result<(), G5Error> {
    sqlx::query(
        r#"
        SELECT pg_advisory_xact_lock(hashtextextended(
            'g5:approval-config:' || $1::text || ':' || $2::text, 0))
        "#,
    )
    .bind(org_id)
    .bind(target_workspace_id)
    .execute(db.conn())
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to lock approval requirement set: {e}"),
    })?;
    Ok(())
}

/// Transaction-scoped lock on one approval definition (protocol step 2):
/// its versions, its decisions, its request chains and its attachments.
pub async fn lock_approval_definition(
    conn: &mut sqlx::PgConnection,
    org_id: Uuid,
    definition_id: Uuid,
) -> Result<(), G5Error> {
    sqlx::query(
        r#"
        SELECT pg_advisory_xact_lock(hashtextextended(
            'g5:approval-definition:' || $1::text || ':' || $2::text, 0))
        "#,
    )
    .bind(org_id)
    .bind(definition_id)
    .execute(conn)
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to lock approval definition: {e}"),
    })?;
    Ok(())
}

/// Take the policy locks for a merge admission (protocol steps 1 and 2):
/// the requirement set first, then every attached definition by id. The
/// locks are transaction-scoped and must be held by the transaction that
/// pins the admission evidence.
pub async fn lock_merge_admission(
    db: &mut ScopedDb,
    org_id: Uuid,
    target_workspace_id: Uuid,
) -> Result<(), G5Error> {
    lock_target_requirement_set(db, org_id, target_workspace_id).await?;
    let attached: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT approval_definition_id
        FROM approval_configs
        WHERE org_id = $1 AND target_workspace_id = $2
        ORDER BY approval_definition_id
        "#,
    )
    .bind(org_id)
    .bind(target_workspace_id)
    .fetch_all(db.conn())
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to list attached approval requirements: {e}"),
    })?;
    for definition_id in attached {
        lock_approval_definition(db.conn(), org_id, definition_id).await?;
    }
    Ok(())
}

/// The approvals context as Git's [`MergeAdmissionGate`], evaluating one
/// [`ApprovalPolicy`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ApprovalMergeAdmission {
    pub policy: ApprovalPolicy,
}

#[async_trait::async_trait]
impl MergeAdmissionGate for ApprovalMergeAdmission {
    async fn lock_requirement_set(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        lock_target_requirement_set(db, org_id, target_workspace_id).await
    }

    async fn lock(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        lock_merge_admission(db, org_id, target_workspace_id).await
    }

    async fn evaluate(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        pinned_source_heads: &[MergeSourceHead],
    ) -> Result<MergeAdmissionEvidence, G5Error> {
        evaluate_merge_admission_with_policy(
            db,
            org_id,
            source_workspace_id,
            target_workspace_id,
            pinned_source_heads,
            self.policy.on_source_commit_vector_change,
        )
        .await
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AdmissionDefinitionRow {
    require_all_reviewers: bool,
    allow_self_approval: bool,
    human_approvals_override_agent: bool,
    id: Uuid,
    version: i32,
    name: String,
    humans_list: Vec<Uuid>,
    agent_type: Option<String>,
    invalidation_boundary: i32,
}

/// Evaluate merge admission for `source_workspace_id` into
/// `target_workspace_id` against the pinned source head vector
/// (g5.governance.approval.requirements, g5.governance.merge). Target scope
/// V1: only the workspace's configured target (`merge_target_workspace_id`,
/// else `cloned_from_id`) is evaluated; another destination is refused
/// rather than evaluated against requirements the approval page cannot show.
/// Satisfied when every requirement attached to the
/// target has its configured combined human/agent quorum
/// through the one requirement evaluator under the selected source-commit
/// policy. Takes no locks: the merge paths call [`lock_merge_admission`]
/// first, the projection evaluates lock-free.
pub async fn evaluate_merge_admission(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    pinned_source_heads: &[MergeSourceHead],
) -> Result<MergeAdmissionEvidence, G5Error> {
    evaluate_merge_admission_with_policy(
        db,
        org_id,
        source_workspace_id,
        target_workspace_id,
        pinned_source_heads,
        ApprovalPolicy::default().on_source_commit_vector_change,
    )
    .await
}

/// [`evaluate_merge_admission`] under an explicit source-commit policy.
pub async fn evaluate_merge_admission_with_policy(
    db: &mut ScopedDb,
    org_id: Uuid,
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    pinned_source_heads: &[MergeSourceHead],
    policy: SourceCommitVectorPolicy,
) -> Result<MergeAdmissionEvidence, G5Error> {
    let source: WorkspaceTargetRow = sqlx::query_as(
        "SELECT merge_target_workspace_id, cloned_from_id FROM workspaces WHERE id = $1",
    )
    .bind(source_workspace_id)
    .fetch_optional(db.conn())
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to resolve workspace merge target: {e}"),
    })?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace {source_workspace_id}"),
    })?;
    // Model convention [D19]: explicit merge target wins over lineage.
    let configured = source.merge_target_workspace_id.or(source.cloned_from_id);
    let current_heads =
        serde_json::to_value(pinned_source_heads).map_err(|error| G5Error::Internal {
            message: format!("Failed to serialize pinned source head vector: {error}"),
        })?;
    let current_heads = normalized_head_vector(&current_heads)?;
    if configured != Some(target_workspace_id) {
        return Ok(MergeAdmissionEvidence {
            schema_version: MERGE_ADMISSION_SCHEMA_VERSION,
            evaluated_at: chrono::Utc::now(),
            source_workspace_id,
            target_workspace_id,
            source_heads: current_heads,
            admitted: false,
            reason: Some(format!(
                "Merge admission evaluates the workspace's configured merge target{}; this merge targets {target_workspace_id}. Change the merge target first.",
                configured.map(|id| format!(" ({id})")).unwrap_or_default()
            )),
            requirements: Vec::new(),
            merge_override: None,
        });
    }

    // Active workspace-wide requirements of the target, at their
    // latest version, with the eligibility boundary (schema D13).
    let definitions: Vec<AdmissionDefinitionRow> = sqlx::query_as(
        r#"
        WITH active AS (
            SELECT DISTINCT ON (approval_definition_id)
                   approval_definition_id, retired
            FROM approval_configs
            WHERE org_id = $1 AND target_workspace_id = $2
            ORDER BY approval_definition_id, seq DESC
        ),
        latest AS (
            SELECT DISTINCT ON (id) id, version, name, humans_list, agent_type, phases, require_all_reviewers, allow_self_approval, human_approvals_override_agent
            FROM approval_definitions
            WHERE org_id = $1
            ORDER BY id, version DESC
        )
        SELECT latest.id, latest.version, latest.name, latest.humans_list,
               latest.agent_type, latest.require_all_reviewers, latest.allow_self_approval, latest.human_approvals_override_agent,
               COALESCE((
                   SELECT MAX(version) FROM approval_definitions boundary
                   WHERE boundary.org_id = $1 AND boundary.id = latest.id
                     AND boundary.invalidates_previous = true
               ), 1) AS invalidation_boundary
        FROM active
        JOIN latest ON latest.id = active.approval_definition_id
        WHERE active.retired = false
        ORDER BY latest.name, latest.id
        "#,
    )
    .bind(org_id)
    .bind(target_workspace_id)
    .fetch_all(db.conn())
    .await
    .map_err(|e| G5Error::Internal {
        message: format!("Failed to list merge admission requirements: {e}"),
    })?;
    let statuses: Vec<StatusRow> = if definitions.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(&format!(
            r#"
            SELECT true AS in_scope, approval_definition_id, approval_definition_version, outcome,
                   comment, actor_type, actor_user_id, reviewed_git_heads, COALESCE(request_id, (SELECT r.approval_request_id FROM agent_approval_reviews r WHERE r.id=approval_statuses.agent_review_id)) AS request_id, agent_review_id,
                   created_at
            FROM approval_statuses
            WHERE org_id = $1 AND source_workspace_id = $2
              AND (actor_type <> 'human' OR ({HUMAN_EVIDENCE_IN_SCOPE}))
              AND approval_definition_id = ANY($3)
            ORDER BY seq DESC
            "#
        ))
        .bind(org_id)
        .bind(source_workspace_id)
        .bind(
            definitions
                .iter()
                .map(|definition| definition.id)
                .collect::<Vec<_>>(),
        )
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list merge admission decisions: {e}"),
        })?
    };

    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_user_id FROM workspaces WHERE id=$1")
            .bind(source_workspace_id)
            .fetch_one(db.conn())
            .await?;
    let requirements = definitions
        .iter()
        .map(|definition| {
            let disallowed_owner = (!definition.allow_self_approval).then_some(owner).flatten();
            let evaluation = requirement_status::evaluate(&RequirementInput {
                require_all: definition.require_all_reviewers,
                human_override: definition.human_approvals_override_agent,
                agent_configured: definition.agent_type.is_some(),
                agent_name: agent_display_name(&definition.name),
                humans: definition
                    .humans_list
                    .iter()
                    .map(|user_id| HumanCandidate {
                        user_id: *user_id,
                        display_name: String::new(),
                        eligible: Some(*user_id) != disallowed_owner,
                        requestable: false,
                    })
                    .collect(),
                statuses: statuses
                    .iter()
                    .filter(|status| status.approval_definition_id == definition.id)
                    .collect(),
                requests: Vec::new(),
                boundary: definition.invalidation_boundary,
                current_heads: Some(&current_heads),
                policy,
            });
            MergeAdmissionRequirement {
                definition_id: definition.id,
                definition_name: definition.name.clone(),
                definition_version: definition.version,
                state: evaluation.status.as_str().to_string(),
            }
        })
        .collect::<Vec<_>>();
    Ok(merge_admission_from_requirements(
        source_workspace_id,
        target_workspace_id,
        current_heads,
        requirements,
    ))
}

/// The one approval-specific merge blocker.
pub const APPROVAL_BLOCKER: &str = "Approval(s) required before merging";

/// The agent approver's display name for a requirement.
fn agent_display_name(definition_name: &str) -> String {
    format!("{definition_name} agent")
}

fn merge_admission_from_requirements(
    source_workspace_id: Uuid,
    target_workspace_id: Uuid,
    source_heads: Vec<MergeSourceHead>,
    requirements: impl IntoIterator<Item = MergeAdmissionRequirement>,
) -> MergeAdmissionEvidence {
    let requirements: Vec<MergeAdmissionRequirement> = requirements.into_iter().collect();
    let admitted = requirements
        .iter()
        .all(|requirement| requirement.state == "approved");
    MergeAdmissionEvidence {
        schema_version: MERGE_ADMISSION_SCHEMA_VERSION,
        evaluated_at: chrono::Utc::now(),
        source_workspace_id,
        target_workspace_id,
        source_heads,
        admitted,
        // The ship blocker names no requirements (g5.workbench.ship-to-parent);
        // they remain in `requirements`.
        reason: (!admitted).then(|| APPROVAL_BLOCKER.to_string()),
        requirements,
        merge_override: None,
    }
}

/// The canonical request key of one request chain (g5.workbench.gates):
/// tenant, source, exact definition version, phase and actor slot.
pub(crate) fn approval_request_key(
    org_id: Uuid,
    source_workspace_id: Uuid,
    definition_id: Uuid,
    definition_version: i32,
    phase: &str,
    actor_slot: &str,
) -> Result<String, G5Error> {
    ApprovalRequestKeyV2 {
        schema_version: APPROVAL_REQUEST_KEY_SCHEMA_VERSION,
        org_id,
        source_workspace_id,
        approval_definition_id: definition_id,
        approval_definition_version: definition_version,
        phase: phase.to_string(),
        actor_slot: actor_slot.to_string(),
    }
    .digest()
    .map_err(|message| G5Error::Internal {
        message: format!("Approval request key is invalid: {message}"),
    })
}

fn validate_phase_value(phase: &str) -> Result<(), G5Error> {
    if is_approval_phase(phase) {
        return Ok(());
    }
    Err(G5Error::ValidationError {
        details: serde_json::json!({
            "phase": format!("'{phase}' is not a workbench phase (be, se, ie)")
        }),
    })
}

/// Validate legacy wire vocabulary, then use one workspace-wide storage slot.
pub(crate) fn workspace_approval_phase(phase: &str) -> Result<String, G5Error> {
    validate_phase_value(phase)?;
    Ok(WORKSPACE_APPROVAL_COMPAT_PHASE.to_string())
}

fn normalized_request_note(note: Option<String>) -> Result<Option<String>, G5Error> {
    let note = note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    if note.is_some_and(|note| note.len() > 8192) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({"note":"must be at most 8192 UTF-8 bytes"}),
        });
    }
    Ok(note.map(str::to_owned))
}

fn approval_request_record(
    request: &ApprovalRequestRow,
    names: &HashMap<Uuid, String>,
    review_runs_by_request: &HashMap<Uuid, Vec<AgentApprovalReviewRecord>>,
) -> ApprovalRequestRecord {
    ApprovalRequestRecord {
        request_id: request.id,
        actor_type: request.actor_type.clone(),
        actor_user_id: request.actor_user_id,
        actor_display_name: request.actor_user_id.and_then(|id| names.get(&id).cloned()),
        requested_by: request.requested_by,
        requested_by_display_name: request.requested_by.and_then(|id| names.get(&id).cloned()),
        state: request.state.clone(),
        definition_version: request.approval_definition_version,
        definition_name: request.definition_name.clone(),
        note: request.note.clone(),
        phase: request.phase.clone(),
        requested_at: request.created_at,
        closed_at: request.closed_at,
        agent_reviews: review_runs_by_request
            .get(&request.id)
            .cloned()
            .unwrap_or_default(),
    }
}

fn agent_approval_review_record(run: AgentApprovalReviewRow) -> AgentApprovalReviewRecord {
    AgentApprovalReviewRecord {
        review_id: run.review_id,
        stage: run.stage,
        cleanup_pending: run.cleanup_pending,
        cleanup_error: run.cleanup_error,
        cleanup_overdue: run.cleanup_overdue,
        report_available: run.report_available,
        retry_available: run.retry_available,
        content_sha256: run.content_sha256,
        task_id: run.task_id,
        session_id: run.session_id,
        state: run.state,
        terminal_reason: run.terminal_reason,
        created_at: run.created_at,
        started_at: run.started_at,
        ended_at: run.ended_at,
        updated_at: run.updated_at,
    }
}

fn decision_record(
    status: &StatusRow,
    invalidation_boundary: i32,
    current_heads: &[MergeSourceHead],
    policy: SourceCommitVectorPolicy,
    names: &HashMap<Uuid, String>,
    requests: &[&ApprovalRequestRow],
) -> DecisionRecord {
    // Only MAKE_PENDING retires evidence on another source vector; currentness
    // itself is the separate comparison every reader shows.
    let head_eligible = policy == SourceCommitVectorPolicy::Ignore
        || status_evaluation_heads(status).is_some_and(|heads| heads == current_heads);
    let is_agent = status.actor_type == "agent";
    DecisionRecord {
        agent_review_id: status.agent_review_id,
        request_id: human_request_id_for_status(status, requests),
        outcome: status.outcome.clone(),
        comment: (!is_agent).then(|| status.comment.clone()).flatten(),
        // Earlier agent reports stored a body clip, never a summary.
        summary: (is_agent && status.summary_authored)
            .then(|| status.comment.clone())
            .flatten(),
        actor_type: status.actor_type.clone(),
        actor_user_id: status.actor_user_id,
        actor_display_name: status.actor_user_id.and_then(|id| names.get(&id).cloned()),
        definition_version: status.approval_definition_version,
        superseded: status.approval_definition_version < invalidation_boundary || !head_eligible,
        reviewed_git_heads: status.reviewed_git_heads.clone(),
        created_at: status.created_at,
    }
}

/// Request-addressed human settlement writes the status and closes its request
/// in one transaction, so their server timestamps are equal. Older status rows
/// have no request-id column; reconstruct only a unique exact match and leave
/// direct or ambiguous decisions unassociated.
fn human_request_id_for_status(
    status: &StatusRow,
    requests: &[&ApprovalRequestRow],
) -> Option<Uuid> {
    if let Some(id) = status.request_id {
        return Some(id);
    }
    if status.actor_type != "human" {
        return None;
    }
    let mut matches = requests.iter().filter(|request| {
        request.actor_type == "human"
            && request.state == "settled"
            && request.approval_definition_id == status.approval_definition_id
            && request.approval_definition_version == status.approval_definition_version
            && request.actor_user_id == status.actor_user_id
            && request.closed_at.as_ref() == Some(&status.created_at)
    });
    let request_id = matches.next().map(|request| request.id)?;
    matches.next().is_none().then_some(request_id)
}

fn status_evaluation_heads(status: &StatusRow) -> Option<Vec<MergeSourceHead>> {
    normalized_head_vector(&status.reviewed_git_heads).ok()
}

/// Parse and canonicalize the shared `MergeSourceHead` vector used by both
/// human and agent eligibility. Input order is irrelevant; malformed,
/// duplicate, partial, or non-canonical identities fail closed.
fn normalized_head_vector(value: &serde_json::Value) -> Result<Vec<MergeSourceHead>, G5Error> {
    let mut heads: Vec<MergeSourceHead> =
        serde_json::from_value(value.clone()).map_err(|error| G5Error::Internal {
            message: format!("Stored approval head vector is malformed: {error}"),
        })?;
    if heads.is_empty() {
        return Err(G5Error::Internal {
            message: "Stored approval head vector must not be empty".to_string(),
        });
    }
    let mut repo_ids = HashSet::with_capacity(heads.len());
    for head in &heads {
        if Uuid::parse_str(&head.repo_id).is_err()
            || Uuid::parse_str(&head.workspace_id).is_err()
            || head.branch_name.trim().is_empty()
            || !matches!(head.head_commit.len(), 40 | 64)
            || !head
                .head_commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !repo_ids.insert(head.repo_id.as_str())
        {
            return Err(G5Error::Internal {
                message: "Stored approval head vector violates the MergeSourceHead contract"
                    .to_string(),
            });
        }
    }
    heads.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
    Ok(heads)
}

/// Read the current self-approval policy while the caller holds the definition lock.
async fn current_self_approval_allowed(
    db: &mut ScopedDb,
    org_id: Uuid,
    definition_id: Uuid,
) -> Result<bool, G5Error> {
    sqlx::query_scalar("SELECT allow_self_approval FROM approval_definitions WHERE org_id=$1 AND id=$2 ORDER BY version DESC LIMIT 1")
        .bind(org_id).bind(definition_id).fetch_optional(db.conn()).await?
        .ok_or_else(|| G5Error::NotFound { resource: "approval definition".into() })
}

/// The open PR a new request or decision belongs to, locked so closing it
/// cannot race the request (g5.governance.approval.request-pr). Without one,
/// refuse only when the request rule requires a PR.
pub(crate) async fn request_pull_request(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    pr_required: bool,
) -> Result<Option<Uuid>, G5Error> {
    let open: Option<Uuid> = sqlx::query_scalar("SELECT id FROM workspace_pull_requests WHERE source_workspace_id=$1 AND status='open' ORDER BY created_at DESC LIMIT 1 FOR SHARE")
        .bind(workspace_id).fetch_optional(db.conn()).await?;
    if open.is_none() && pr_required {
        return Err(G5Error::Conflict {
            reason: "Submit a PR before requesting or submitting a formal review".into(),
        });
    }
    Ok(open)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reviewer_quorum_requires_a_positive_vote_and_no_denial() {
        let owner = Uuid::new_v4();
        let reviewer = Uuid::new_v4();
        let reviewed_heads = json!([{
            "repoId": Uuid::new_v4(), "workspaceId": Uuid::new_v4(),
            "branchName": "review", "headCommit": "a".repeat(40)
        }]);
        let current_heads = normalized_head_vector(&reviewed_heads).unwrap();
        let decision = |actor: Option<Uuid>, outcome: &str| StatusRow {
            agent_review_id: None,
            in_scope: true,
            approval_definition_id: Uuid::nil(),
            approval_definition_version: 1,
            outcome: outcome.into(),
            comment: None,
            actor_type: if actor.is_some() { "human" } else { "agent" }.into(),
            actor_user_id: actor,
            reviewed_git_heads: reviewed_heads.clone(),
            request_id: None,
            created_at: chrono::Utc::now(),
            summary_authored: false,
        };
        let evaluate =
            |people: &[(Uuid, &str)], all: bool, excluded: Option<Uuid>, agent: Option<&str>| {
                let rows: Vec<StatusRow> = people
                    .iter()
                    .filter(|(_, state)| *state != "pending")
                    .map(|(user, state)| decision(Some(*user), state))
                    .chain(agent.map(|outcome| decision(None, outcome)))
                    .collect();
                requirement_status::evaluate(&RequirementInput {
                    require_all: all,
                    human_override: true,
                    agent_configured: agent.is_some(),
                    agent_name: "agent".into(),
                    humans: people
                        .iter()
                        .map(|(user, _)| HumanCandidate {
                            user_id: *user,
                            display_name: String::new(),
                            eligible: Some(*user) != excluded,
                            requestable: true,
                        })
                        .collect(),
                    statuses: rows.iter().collect(),
                    requests: Vec::new(),
                    boundary: 1,
                    current_heads: Some(&current_heads),
                    policy: SourceCommitVectorPolicy::Ignore,
                })
                .status
                .as_str()
            };
        let mut people = vec![(owner, "approved"), (reviewer, "pending")];
        assert_eq!(evaluate(&people, false, None, None), "approved");
        assert_eq!(evaluate(&people, true, None, None), "pending");
        assert_eq!(evaluate(&people, false, Some(owner), None), "pending");
        assert_eq!(evaluate(&people[..1], false, Some(owner), None), "pending");
        assert_eq!(evaluate(&people, false, None, Some("deferred")), "approved");
        people[1].1 = "rejected";
        assert_eq!(evaluate(&people, false, None, None), "rejected");
        assert_eq!(evaluate(&people, true, None, None), "rejected");
        people[1].1 = "approved";
        people[0].1 = "rejected";
        assert_eq!(evaluate(&people, true, Some(owner), None), "approved");
        assert_eq!(evaluate(&people, false, None, None), "rejected");
        assert_eq!(evaluate(&[], false, None, Some("deferred")), "pending");
        assert_eq!(evaluate(&[], false, None, None), "pending");
        people[0].1 = "approved";
        // A disallowed self approval cannot supply the human override.
        for (excluded, expected) in [(Some(owner), "rejected"), (None, "approved")] {
            assert_eq!(
                evaluate(&people[..1], false, excluded, Some("rejected")),
                expected
            );
        }
    }

    #[test]
    fn human_status_request_link_requires_one_exact_settlement_match() {
        let definition_id = Uuid::new_v4();
        let actor_user_id = Uuid::new_v4();
        let settled_at = chrono::Utc::now();
        let status = StatusRow {
            agent_review_id: None,
            in_scope: true,
            request_id: None,
            approval_definition_id: definition_id,
            approval_definition_version: 2,
            outcome: "approved".to_string(),
            comment: None,
            actor_type: "human".to_string(),
            actor_user_id: Some(actor_user_id),
            reviewed_git_heads: json!([]),
            created_at: settled_at,
            summary_authored: false,
        };
        let request = ApprovalRequestRow {
            active_pr: true,
            id: Uuid::new_v4(),
            approval_definition_id: definition_id,
            approval_definition_version: 2,
            actor_type: "human".to_string(),
            actor_user_id: Some(actor_user_id),
            requested_by: Some(actor_user_id),
            state: "settled".to_string(),
            definition_name: "Human review".to_string(),
            allow_self_approval: false,
            note: None,
            phase: Some("ie".to_string()),
            created_at: settled_at - chrono::Duration::minutes(1),
            closed_at: Some(settled_at),
        };

        assert_eq!(
            human_request_id_for_status(&status, &[&request]),
            Some(request.id)
        );
        assert_eq!(human_request_id_for_status(&status, &[]), None);

        let mut timestamp_mismatch = request.clone();
        timestamp_mismatch.closed_at = Some(settled_at + chrono::Duration::seconds(1));
        assert_eq!(
            human_request_id_for_status(&status, &[&timestamp_mismatch]),
            None
        );

        let mut duplicate = request.clone();
        duplicate.id = Uuid::new_v4();
        assert_eq!(
            human_request_id_for_status(&status, &[&request, &duplicate]),
            None
        );
    }

    fn head_vector(commit: char) -> (serde_json::Value, Vec<MergeSourceHead>) {
        let heads = vec![MergeSourceHead {
            repo_id: Uuid::new_v4().to_string(),
            branch_name: "main".to_string(),
            workspace_id: Uuid::new_v4().to_string(),
            head_commit: commit.to_string().repeat(40),
        }];
        (serde_json::to_value(&heads).unwrap(), heads)
    }

    #[test]
    fn head_vector_normalization_rejects_empty_vectors() {
        let error = normalized_head_vector(&serde_json::json!([]))
            .expect_err("empty source head vectors must fail closed");
        let G5Error::Internal { message } = error else {
            panic!("empty source head vectors must return an internal projection error");
        };
        assert_eq!(message, "Stored approval head vector must not be empty");
    }

    #[test]
    fn rich_and_batch_deferred_timestamps_match_eligibility_boundaries() {
        let caller_user_id = Uuid::new_v4();
        let source_workspace_id = Uuid::new_v4();
        let definition_id = Uuid::new_v4();
        let created_at = chrono::Utc::now();
        let (reviewed_heads, current_heads) = head_vector('a');
        let (stale_heads, _) = head_vector('b');

        for (outcome, version, heads, boundary, deferred) in [
            ("approved", 1, reviewed_heads.clone(), 1, false),
            ("deferred", 1, reviewed_heads.clone(), 1, true),
            ("deferred", 1, reviewed_heads.clone(), 2, false),
            ("deferred", 2, stale_heads, 2, false),
            ("deferred", 2, reviewed_heads.clone(), 2, true),
        ] {
            let rich = StatusRow {
                agent_review_id: None,
                in_scope: true,
                request_id: None,
                approval_definition_id: definition_id,
                approval_definition_version: version,
                outcome: outcome.to_string(),
                comment: None,
                actor_type: "human".to_string(),
                actor_user_id: Some(caller_user_id),
                reviewed_git_heads: heads.clone(),
                created_at,
                summary_authored: false,
            };
            let batch = BatchStatusRow {
                source_workspace_id,
                approval_definition_id: definition_id,
                approval_definition_version: version,
                outcome: outcome.to_string(),
                actor_type: "human".to_string(),
                actor_user_id: Some(caller_user_id),
                reviewed_git_heads: heads,
                created_at,
            };
            let policy = SourceCommitVectorPolicy::MakePending;
            let rich_state =
                caller_deferred_at([&rich], caller_user_id, boundary, &current_heads, policy);
            let batch_state =
                caller_deferred_at([&batch], caller_user_id, boundary, &current_heads, policy);
            assert_eq!(rich_state, deferred.then_some(created_at));
            assert_eq!(batch_state, deferred.then_some(created_at));
        }
    }
}
