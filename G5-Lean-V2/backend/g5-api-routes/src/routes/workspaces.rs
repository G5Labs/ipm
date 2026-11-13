//! Workspace route handlers under multiple path prefixes.
//!
//! This module implements the HTTP route handlers for the Workspaces API
//! (API_AND_STORAGE.md §3.1) and workspace member management from §3.12.3.
//! Endpoints span 2 path prefixes:
//!
//! - `/api/v1/orgs/:orgId/workspaces` — List and create workspaces (org-scoped)
//! - `/api/v1/workspaces/:id/*` — Detail, update, delete, and all sub-resource operations
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to `WorkspaceService` or `CloneService`, and format the response
//! using `ResponseEnvelope`. No business logic resides in the handlers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Seek, SeekFrom, Write};
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::{delete, get, patch, post, put},
    Extension, Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::db::scoped::{
    ensure_workspace_repo_id, ensure_workspace_repo_merge_run_id, resolve_org_id_for_workspace,
    scoped_db_for_org, scoped_db_for_workspace,
};
use crate::errors::AppError;
use crate::middleware::{AuthContext, AuthenticatedUser};
use crate::routes::audited::AuditedRouter;
use crate::routes::chat::SubscriptionUsageDTO;
use crate::routes::workspace_merge_override::{merge_with_override, preview_merge_override};
use crate::routes::{Pagination, ResponseEnvelope};
use crate::services::external_issues::LinkRole;
use crate::services::workspace_reader::{resolve_connected_binding_view, CommittedRepoReadView};
use crate::services::workspace_vm_service::{
    ResumeOutcome, StartKind, StopOutcome, StopReason, TransferOutcome, WorkspaceVmView,
};
use crate::services::AppState;
use crate::services::{
    coordinated_repo_publication, workspace_fork_service, workspace_merge_service,
    workspace_service,
};
use g5_api_foundation::contracts::workbench::{SetJourneyMarkRequestDTO, WorkspaceJourneyDTO};
use g5_auth::Scope;
use g5_contracts::error::G5Error;
use g5_contracts::types::AuthorityTarget;
use g5_internal_api::dtos::{
    PROVIDER_QUOTA_AUTH_MODE_ANTHROPIC_OAUTH, PROVIDER_QUOTA_PROVIDER_CLAUDE_CODE,
};
use g5_scm::TreeEntryType;

// ─── Path Parameter Types ────────────────────────────────────────────────────

/// Path parameters for org-scoped workspace endpoints (`/api/v1/orgs/:orgId/workspaces`).
#[derive(Debug, Deserialize)]
pub struct OrgPathParams {
    #[serde(rename = "orgId")]
    pub org_id: Uuid,
}

/// Path parameters for workspace-scoped endpoints (`/api/v1/workspaces/:id`).
#[derive(Debug, Deserialize)]
pub struct WorkspacePathParams {
    pub id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceFilePathParams {
    pub id: Uuid,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileQuery {
    pub repo_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceFileWriteRequestDTO {
    pub content: String,
    #[serde(default = "default_file_encoding")]
    pub encoding: String,
    pub expected_hash: Option<String>,
    pub expected_vm_id: Option<Uuid>,
    #[serde(default)]
    pub commit_and_push: bool,
}

fn default_file_encoding() -> String {
    "utf-8".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceFileDeleteRequestDTO {
    pub expected_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceFileRenameRequestDTO {
    pub repo_id: Uuid,
    pub from: String,
    pub to: String,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceChangesActionRequestDTO {
    #[serde(default)]
    pub repo_id: Option<Uuid>,
    /// `commit` only: the commit message.
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceChangesActionPathParams {
    pub id: Uuid,
    pub action: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileContentDTO {
    pub path: String,
    pub content: String,
    pub encoding: String,
    pub size: u64,
    pub hash: String,
    pub source: String,
    pub repo_oid: Option<String>,
    pub snapshot_at: Option<String>,
    pub vm_id: Option<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceLifecycleRequestDTO {
    #[serde(default)]
    pub operation_id: Option<Uuid>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTransferRequestDTO {
    pub to_user_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize)]
pub struct RebaseChangeGraphPathParams {
    pub id: Uuid,
    #[serde(rename = "artifactKey")]
    pub artifact_key: String,
}

/// Canonical workspace/repository route identity.
#[derive(Debug, Deserialize)]
pub struct WorkspaceRepoPathParams {
    pub id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceRepoMergeRunPathParams {
    pub id: Uuid,
    #[serde(rename = "repoId")]
    pub repo_id: Uuid,
    #[serde(rename = "mergeRunId")]
    pub merge_run_id: Uuid,
}

/// Path parameters for workspace PR merge endpoints.
#[derive(Debug, Deserialize)]
pub struct WorkspacePullRequestPathParams {
    pub id: Uuid,
    #[serde(rename = "pullRequestId")]
    pub pull_request_id: Uuid,
}

/// Path parameters for guided-journey step marks
/// (`/api/v1/workspaces/:id/journey/steps/:stepId`).
#[derive(Debug, Deserialize)]
pub struct WorkspaceJourneyStepPathParams {
    pub id: Uuid,
    #[serde(rename = "stepId")]
    pub step_id: String,
}

// ─── Query Parameter DTOs ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphQueryDTO {
    pub target_workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphDetailsQueryDTO {
    pub target_workspace_id: Uuid,
    pub revision: String,
}

/// Bounded workspace ids for the open-question metrics read.
#[derive(Debug, Deserialize)]
pub struct WorkspaceOpenQuestionMetricsQueryDTO {
    #[serde(rename = "workspaceIds")]
    pub workspace_ids: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocOpenQuestionDTO {
    pub block_id: String,
    pub title: String,
    pub severity: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocOpenQuestionSourceDTO {
    pub doc_id: String,
    pub display_name: String,
    pub open_question_count: i64,
    pub first_open_question_block_id: Option<String>,
    pub open_questions: Vec<ProjectDocOpenQuestionDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSetOpenQuestionDTO {
    pub question_id: String,
    pub display_number: u32,
    pub title: String,
    pub severity: Option<String>,
    pub status: String,
}

/// One question set of the metrics' third family
/// (`g5.documents.question.metrics`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSetOpenSourceDTO {
    pub path: String,
    pub set_id: String,
    pub title: String,
    pub open_question_count: i64,
    pub open_questions: Vec<QuestionSetOpenQuestionDTO>,
    /// Stable doc ids of the referencing Project Docs (navigation targets).
    pub referenced_by: Vec<String>,
    pub invalid: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenQuestionMetricsDTO {
    pub workspace_id: Uuid,
    pub document_open_questions: i64,
    pub open_questions_total: i64,
    pub project_doc_sources: Vec<ProjectDocOpenQuestionSourceDTO>,
    pub question_set_sources: Vec<QuestionSetOpenSourceDTO>,
    /// A scanned set did not parse: the counts are a lower bound.
    pub question_sets_incomplete: bool,
}

impl From<workspace_service::WorkspaceOpenQuestionMetrics> for WorkspaceOpenQuestionMetricsDTO {
    fn from(metric: workspace_service::WorkspaceOpenQuestionMetrics) -> Self {
        Self {
            workspace_id: metric.workspace_id,
            document_open_questions: metric.document_open_questions,
            open_questions_total: metric.document_open_questions,
            project_doc_sources: metric
                .project_doc_sources
                .into_iter()
                .map(|source| ProjectDocOpenQuestionSourceDTO {
                    doc_id: source.doc_id,
                    display_name: source.display_name,
                    open_question_count: source.open_question_count,
                    first_open_question_block_id: source.first_open_question_block_id,
                    open_questions: source
                        .open_questions
                        .into_iter()
                        .map(|question| ProjectDocOpenQuestionDTO {
                            block_id: question.block_id,
                            title: question.title,
                            severity: question.severity,
                        })
                        .collect(),
                })
                .collect(),
            question_set_sources: metric
                .question_set_sources
                .into_iter()
                .map(|source| QuestionSetOpenSourceDTO {
                    path: source.path,
                    set_id: source.set_id,
                    title: source.title,
                    open_question_count: source.open_question_count,
                    open_questions: source
                        .open_questions
                        .into_iter()
                        .map(|question| QuestionSetOpenQuestionDTO {
                            question_id: question.question_id,
                            display_number: question.display_number,
                            title: question.title,
                            severity: question.severity,
                            status: question.status,
                        })
                        .collect(),
                    referenced_by: source.referenced_by,
                    invalid: source.invalid,
                })
                .collect(),
            question_sets_incomplete: metric.question_sets_incomplete,
        }
    }
}

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// Request body for `PATCH /api/v1/workspaces/:id`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateWorkspaceRequestDTO {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Workbench phase (`be` | `se` | `ie`; workbench `g5.workbench.phases.state`).
    /// Owner-moved forward or back; Base never has a phase to move.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

/// Request body for `POST /api/v1/workspaces/:id/rename`
/// (`g5.workspace.rename`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameWorkspaceRequestDTO {
    /// The new name: an exact Git branch name, not a reserved segment, free in
    /// the organization. The branch the workspace owns does not change.
    pub name: String,
}

/// Response of `POST /api/v1/workspaces/:id/rename`: the current name and
/// the former names the workspace reserves, newest first
/// (`g5.workspace.former-names`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRenameDTO {
    pub id: String,
    pub name: String,
    pub former_names: Vec<String>,
}

/// Request body for `POST /api/v1/workspaces/:id/mutation-policy` (spec
/// routes/workspaces [I5c]).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationPolicyTransitionRequestDTO {
    /// Target policy: `"read_only"` or `"read_write"`.
    pub policy: String,
    /// Concurrency mode, RW→RO only: `"drain"` (default) or `"require_quiescent"`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Drain timeout in seconds, RW→RO drain only (default 300).
    #[serde(default)]
    pub drain_timeout_sec: Option<u64>,
}

/// One outstanding blocker in a transition response (spec [I5c]).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionBlockerDTO {
    pub kind: String,
    pub id: String,
    pub detail: String,
}

/// Response body for `POST /api/v1/workspaces/:id/mutation-policy` (spec [I5c]).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationPolicyTransitionResponseDTO {
    /// Resulting `mutation_policy`.
    pub policy: String,
    /// Resulting transient transition marker (`"freezing"` or absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    /// Human-readable one-line summary of the outcome (committed / draining /
    /// refused-with-reason) so the client never synthesizes an explanation from a
    /// bare status code (spec [I5c]).
    pub message: String,
    /// Outstanding blockers on a `require_quiescent` refusal, an up-front
    /// precondition refusal, or a drain-timeout revert. Omitted when committed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<TransitionBlockerDTO>,
    /// Number of tasks that terminalized during the drain window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drained_tasks: Option<i64>,
}

/// Request body for promoting an existing branch repo child to remote tracking.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteRemoteTrackingRequestDTO {
    pub remote_branch: String,
    /// Optional Make-Remote Adopt-Remote target: the real remote clone URL to
    /// re-point a `g5-local://` repo onto (workspace_service D47).
    /// Re-pointing requires org-admin authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_url: Option<String>,
    /// The actor confirmed creating an absent provider branch. Base requires
    /// it before its one create-only push (`g5.workspace.remote.base-parity`);
    /// other workspaces create an absent branch without it.
    #[serde(default)]
    pub create_provider_branch: bool,
}

/// Request body for `POST /api/v1/workspaces/:id/clone`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneWorkspaceRequestDTO {
    pub name: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub source: Option<workspace_fork_service::ForkSource>,
    #[serde(default)]
    pub interrupt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional read-only creation flag for the clone; omitted/`false` →
    /// read-write. The source workspace's policy is never inherited (design
    /// clone_service D30).
    #[serde(default)]
    pub read_only: Option<bool>,
    /// Org workspace the fork intends to merge into; omitted stores NULL,
    /// meaning "merge to the fork parent" (clone_service D32).
    #[serde(default)]
    pub merge_target_workspace_id: Option<Uuid>,
    /// Selected source sessions to carry into the completed workspace fork.
    #[serde(default)]
    pub chat_session_ids: Vec<Uuid>,
    /// An existing external issue to link to the fork.
    #[serde(default)]
    pub issue: Option<CloneWorkspaceIssueDTO>,
    /// Start a chat in the new workspace with this first message.
    #[serde(default)]
    pub start_chat: Option<CloneWorkspaceStartChatDTO>,
    /// `name` is a suggestion: when it is taken, the fork takes the first
    /// free `name-2`, `name-3`, … (the response names the workspace created).
    #[serde(default)]
    pub uniquify_name: bool,
    /// Start a Base re-verification run with this fork of Base
    /// (g5.reverification.run).
    #[serde(default)]
    pub reverification: Option<CloneWorkspaceReverificationDTO>,
}

/// The Base re-verification run a fork of Base starts.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloneWorkspaceReverificationDTO {
    pub kind: crate::services::base_reverification::RunKind,
}

/// The first message of a chat a fork starts in the new workspace.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloneWorkspaceStartChatDTO {
    pub content: String,
    /// The launch choices (provider, model, account, permission mode,
    /// effort), resolved for the new workspace when the chat is queued.
    #[serde(default)]
    pub launch_preferences: Option<g5_contracts::types::AgentLaunchPreferencesV1>,
}

/// The chat a fork started. `sessionId` is set when the first message was
/// queued; `error` says why it was not. `workspaceStartError` says why the new
/// workspace was not started, leaving the message queued until it is.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneStartedChatDTO {
    pub session_id: Option<Uuid>,
    pub error: Option<String>,
    pub workspace_start_error: Option<String>,
}

/// Issue to link when a fork completes: `ref` is a GitHub issue URL or
/// `owner/name#number` inside a configured issue domain.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneWorkspaceIssueDTO {
    #[serde(rename = "ref")]
    pub reference: String,
    pub role: String,
}

/// Request body for updating a workspace, with an agent fallback for conflicts.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkspaceMergeRequestDTO {
    pub target_workspace_id: Uuid,
    #[serde(default)]
    pub agent_launch: Option<g5_contracts::types::AgentLaunchSelectionV1>,
    /// Start and resolve with agent (`g5.git.stopped-rebase.resolve-with-agent`,
    /// `g5.chat.start-and-send`): while the owner's VM is stopped, failed or
    /// starting, the same request records (or joins) their Start consent and
    /// queues the conflict-resolution brief bound to it.
    #[serde(default)]
    pub start_compute: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectCleanWorkspaceMergeRequestDTO {
    pub target_workspace_id: Uuid,
    pub preflight_fingerprint: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSyncFilesQueryDTO {
    #[serde(default)]
    pub direction: Option<String>,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// Full workspace summary with branches and stats for list endpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListItemDTO {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_base: bool,
    /// True when the workspace is read-only (`mutation_policy = 'read_only'`).
    /// Drives the UI read-only badge; server enforcement is the mutation guard.
    pub read_only: bool,
    /// True for a review workspace (a reviewer-owned copy made by Start
    /// review). Drives the Review tag wherever workspaces are listed.
    pub is_review: bool,
    pub agent_review: Option<crate::services::worker_host_service::AgentReviewInventoryLink>,
    /// Immutable registry kind: `base`, `working` or `personal`
    /// (`g5.workspace.kind`). Presentation follows it, never the name.
    pub kind: g5_contracts::types::WorkspaceKind,
    pub owner_user_id: Option<String>,
    pub created_by: Option<String>,
    pub cloned_from_id: Option<String>,
    pub branches: Vec<BranchSummaryDTO>,
    pub tasks_completed: i64,
    pub total_tasks: i64,
    pub running_time: Option<String>,
    pub cost: f64,
    pub concurrent_tasks: i64,
    pub last_updated: String,
    pub created_at: String,
    /// Active primary external issue (association only; no tracker read).
    /// Present on workspace list items; `null` when there is none.
    pub primary_issue: Option<workspace_service::WorkspacePrimaryIssue>,
    /// Workbench phase (`be` | `se` | `ie`; workbench `g5.workbench.phases.state`).
    /// Present on list reads; omitted (not `null`) on the clone response, which
    /// describes the new workspace rather than its listing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

/// Per-branch summary within a workspace list item.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummaryDTO {
    pub workspace_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    pub tracking_mode: String,
    pub head_commit: String,
    /// Commits ahead of the merge-target-else-clone-parent workspace.
    /// `None` serializes as an explicit JSON `null` = unknown; only the
    /// workspace-detail path computes real values and no surface may
    /// fabricate a `0` (workspace_service ontology D50, issue #1995).
    pub divergence: Option<i32>,
    /// Commits behind the comparison workspace; same nullable contract as
    /// `divergence`. The key is always serialized (never skipped) so clients
    /// can distinguish "unknown" from "absent".
    pub behind_base: Option<i32>,
    pub has_draft: bool,
    pub dirty_file_count: i32,
    /// Remote-side branch name. Present only for `tracking_mode == "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    /// Effective local branch name. Present only for `tracking_mode == "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_branch_name: Option<String>,
    /// Provider repository full name (`provider_full_name`, e.g. `octocat/hello-world`,
    /// or `owner/repo` parsed from `git_url`). Present only for
    /// `tracking_mode == "remote_tracking"`; lets the client render the remote
    /// identity (`remote_repo_name : remote_branch`) on the tracked repo chip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_repo_name: Option<String>,
    /// Other workspaces in the same org tracking the same `(repo_id, remote_branch)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_tracking_workspaces: Option<Vec<CoTrackingWorkspaceDTO>>,
    /// Count of peer workspaces in the same org tracking the same remote branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_tracking_workspace_count: Option<i64>,
    /// Option 1 (G5 #1198): authoritative per-child passive auto-pull setting.
    /// `Some` only for `trackingMode = "remote_tracking"` children.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_pull_enabled: Option<bool>,
    /// Option 2 (G5 #1198): `Some(true)` when a local-only branch is
    /// Publish-eligible (its fork parent had this repo remote-tracked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_eligible: Option<bool>,
}

/// Full workspace detail for get/create/update responses.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDetailDTO {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub is_base: bool,
    /// Immutable registry kind (`g5.workspace.kind`).
    pub kind: g5_contracts::types::WorkspaceKind,
    /// True when the workspace is read-only (`mutation_policy = 'read_only'`).
    pub read_only: bool,
    /// Transient `mutation_policy_transition` marker (`"freezing"` while a
    /// read-write → read-only drain is in progress, else absent). Distinct from
    /// `read_only`; lets the client show a Freezing indicator and offer
    /// "Make editable" for any not-effectively-read-write state (spec [D30]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation_policy_transition: Option<String>,
    pub description: Option<String>,
    pub cloned_from_id: Option<String>,
    pub docs_repo_id: Option<String>,
    pub created_by: Option<String>,
    pub owner_user_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Per-workspace outbound push mode (`'manual'` default | `'auto'`); arch D66.
    pub push_mode: String,
    /// Workbench phase (`be` | `se` | `ie`; workbench `g5.workbench.phases.state`).
    /// Populated on the GET path and after a phase update; omitted elsewhere
    /// so a mutation response never reports a stale default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub branches: Vec<BranchSummaryDTO>,
    /// Dedicated workspace docs folder, always `docs/<id>` (derived convention).
    pub docs_folder_path: String,
    /// Names the workspace held before and still reserves, newest first
    /// (`g5.workspace.former-names`). Populated on the GET path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub former_names: Vec<String>,
}

/// Per-task cost breakdown from the workspace usage ledger.
/// Standalone conversation costs are included only in the total.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCostBreakdownDTO {
    pub task_id: String,
    pub task_name: String,
    pub task_type: String,
    pub cost_usd: f64,
}

/// Computed workspace statistics.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatsDTO {
    pub total_cost_usd: f64,
    pub tasks_completed: i64,
    pub total_tasks: i64,
    pub concurrent_tasks: i64,
    /// Tasks in the `cancelled` terminal state (e.g. a stopped chat).
    pub tasks_stopped: i64,
    /// Top task costs; the total includes all recorded workspace usage.
    pub by_task: Vec<TaskCostBreakdownDTO>,
    pub branches: Vec<BranchDivergenceDTO>,
}

/// Per-branch divergence info.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDivergenceDTO {
    pub workspace_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    /// `None` (JSON `null`) = unknown; the stats path computes no divergence
    /// and always reports `null` (workspace_service ontology D50).
    pub ahead_of_base: Option<i32>,
    /// Same nullable contract as `ahead_of_base` (ontology D50).
    pub behind_base: Option<i32>,
}

/// Detailed branch info for list_branches and add_branch responses.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBranchDetailDTO {
    pub workspace_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    pub head_commit: String,
    pub tracking_mode: String,
    pub materialization_status: String,
    pub has_draft: bool,
    pub dirty_file_count: i32,
    pub dirty_files: Vec<String>,
    /// Remote-side branch name. Present only for `tracking_mode == "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    /// Effective local branch name. Present only for `tracking_mode == "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_branch_name: Option<String>,
    /// Other workspaces in the same org tracking the same `(repo_id, remote_branch)`.
    /// Surfaces the co-tracking advisory to the UI on workspace-create
    /// (per `views/pm_views/remote_repo_tracking.pm_view.md` D4 + D17).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_tracking_workspaces: Option<Vec<CoTrackingWorkspaceDTO>>,
}

/// Wire shape of one co-tracking workspace entry, surfaced on add-repo
/// responses when the new branch is `remote_tracking`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoTrackingWorkspaceDTO {
    pub workspace_id: String,
    pub workspace_name: String,
}

impl From<workspace_service::CoTrackingWorkspace> for CoTrackingWorkspaceDTO {
    fn from(c: workspace_service::CoTrackingWorkspace) -> Self {
        Self {
            workspace_id: c.workspace_id.to_string(),
            workspace_name: c.workspace_name,
        }
    }
}

/// Result of a workspace sync operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResultDTO {
    pub branches_updated: i32,
    pub branches_skipped: i32,
    pub details: Vec<BranchSyncDetailDTO>,
}

/// Per-branch sync detail.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSyncDetailDTO {
    pub workspace_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    pub status: String,
    pub old_commit: Option<String>,
    pub new_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_by: Option<crate::services::remote_integrations::ImportHold>,
}

/// Additive coordinated-operation evidence included by sync/push/bind routes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatedOperationResultDTO {
    pub operation: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub repos: Vec<CoordinatedRepoEvidenceDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatedRepoEvidenceDTO {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    pub operation: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteRemoteTrackingResponseDTO {
    pub branch: BranchSummaryDTO,
    pub coordinated: CoordinatedOperationResultDTO,
    /// Outcome discriminator (D46): `"bound"` (ordinary equal/remote-ahead
    /// promotion), `"published"` (Make-Remote absent-ref create-and-bind), or
    /// `"merge_required"` (Make-Remote divergent/local-ahead merge routing).
    pub outcome: String,
    /// Present only for the `"merge_required"` outcome: the synchronously opened
    /// remote-tracking merge run id the UI routes into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_run_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushToRemoteResponseDTO {
    pub enqueued: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinated: Option<CoordinatedOperationResultDTO>,
}

/// Result of a workspace merge operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResultDTO {
    pub branches_merged: i32,
    pub branches_failed: i32,
    pub details: Vec<BranchMergeDetailDTO>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum StartWorkspaceMergeResponseDTO {
    Direct {
        run: Box<workspace_merge_service::WorkspaceMergeRunSummary>,
    },
    Resolution {
        resolution: crate::services::git_conflict_resolution::ConflictResolutionTask,
        /// The Start a `startCompute: true` request recorded or joined,
        /// beside the resolution as `startCompute`.
        #[serde(rename = "startCompute", skip_serializing_if = "Option::is_none")]
        start_compute: Option<crate::routes::start_and_send::StartComputeDTO>,
    },
    /// The running owner VM rebased the source and G5 accepted the result.
    Rebased {
        rebase: workspace_merge_service::WorkspaceRebaseOutcome,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMergePreflightResponseDTO {
    pub preflight: workspace_merge_service::MergePreflightResponse,
}

pub use crate::services::workspace_pull_request_create::{
    CreatePullRequestOutcome as CreateWorkspacePullRequestResponseDTO,
    CreatePullRequestRequest as CreateWorkspacePullRequestRequestDTO,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWorkspacePullRequestTextRequestDTO {
    pub target_workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePullRequestListQueryDTO {
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeWorkspacePullRequestResponseDTO {
    pub pull_request: coordinated_repo_publication::WorkspacePullRequestSummary,
    pub run: workspace_merge_service::WorkspaceMergeRunSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePullRequestResponseDTO {
    pub pull_request: coordinated_repo_publication::WorkspacePullRequestSummary,
}

/// Per-branch merge detail.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchMergeDetailDTO {
    pub workspace_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    pub status: String,
    pub merge_commit: Option<String>,
    pub error: Option<String>,
}

/// Response from a successful workspace clone.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneWorkspaceResponseDTO {
    pub workspace: WorkspaceListItemDTO,
    pub cloned_branches: i32,
    pub cloned_drafts: i32,
    /// How many chat sessions were carried into the fork (fork-chat; D31).
    pub carried_sessions: i32,
    /// The issue association the fork created, if it named an issue.
    pub issue_link: Option<workspace_fork_service::ForkIssueLinkOutcome>,
    /// The chat the fork started, if it asked for one.
    pub started_chat: Option<CloneStartedChatDTO>,
    /// Non-blocking advisories about the source's remote-tracked members at
    /// fork time (`g5.fork.source.advisory`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_advisories: Vec<crate::services::remote_integrations::ForkSourceAdvisory>,
    /// The Base re-verification run the fork started, if it asked for one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverification_run: Option<crate::services::base_reverification::Run>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSyncFileDTO {
    pub id: String,
    pub target_kind: String,
    pub path: String,
    pub repo_id: String,
    pub repo_name: String,
    pub branch_name: String,
    pub head_commit: String,
    pub last_downlifted_commit_sha: Option<String>,
    pub pending_reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_doc_ref: Option<String>,
}

// ─── From Implementations ────────────────────────────────────────────────────

impl From<workspace_service::WorkspaceListItem> for WorkspaceListItemDTO {
    fn from(ws: workspace_service::WorkspaceListItem) -> Self {
        WorkspaceListItemDTO {
            id: ws.id.to_string(),
            name: ws.name,
            description: ws.description,
            is_base: ws.is_base,
            read_only: ws.read_only,
            is_review: ws.is_review,
            agent_review: ws.agent_review,
            kind: ws.kind,
            owner_user_id: ws.owner_user_id.map(|id| id.to_string()),
            created_by: ws.created_by.map(|id| id.to_string()),
            cloned_from_id: ws.cloned_from_id.map(|id| id.to_string()),
            branches: ws
                .branches
                .into_iter()
                .map(BranchSummaryDTO::from)
                .collect(),
            tasks_completed: ws.tasks_completed,
            total_tasks: ws.total_tasks,
            running_time: ws.running_time,
            cost: ws.cost,
            concurrent_tasks: ws.concurrent_tasks,
            last_updated: ws.last_updated,
            created_at: ws.created_at,
            primary_issue: ws.primary_issue,
            phase: ws.phase,
        }
    }
}

impl From<workspace_service::BranchSummary> for BranchSummaryDTO {
    fn from(b: workspace_service::BranchSummary) -> Self {
        BranchSummaryDTO {
            workspace_id: b.workspace_id.to_string(),
            repo_id: b.repo_id.to_string(),
            repo_name: b.repo_name,
            branch_name: b.branch_name,
            tracking_mode: b.tracking_mode,
            head_commit: b.head_commit,
            divergence: b.divergence,
            behind_base: b.behind_base,
            has_draft: b.has_draft,
            dirty_file_count: b.dirty_file_count,
            remote_branch: b.remote_branch,
            local_branch_name: b.local_branch_name,
            remote_repo_name: b.remote_repo_name,
            co_tracking_workspaces: b
                .co_tracking_workspaces
                .map(|cs| cs.into_iter().map(CoTrackingWorkspaceDTO::from).collect()),
            co_tracking_workspace_count: b.co_tracking_workspace_count,
            auto_pull_enabled: b.auto_pull_enabled,
            remote_eligible: b.remote_eligible,
        }
    }
}

impl From<workspace_service::WorkspaceDetail> for WorkspaceDetailDTO {
    fn from(ws: workspace_service::WorkspaceDetail) -> Self {
        WorkspaceDetailDTO {
            id: ws.id.to_string(),
            org_id: ws.org_id.to_string(),
            name: ws.name,
            is_base: ws.is_base,
            kind: ws.kind,
            read_only: ws.read_only,
            mutation_policy_transition: ws.mutation_policy_transition,
            description: ws.description,
            cloned_from_id: ws.cloned_from_id.map(|id| id.to_string()),
            docs_repo_id: ws.docs_repo_id.map(|id| id.to_string()),
            created_by: ws.created_by.map(|id| id.to_string()),
            owner_user_id: None,
            created_at: ws.created_at,
            updated_at: ws.updated_at,
            // Default; `get_workspace` overrides push_mode with the live value
            // via a runtime query.
            push_mode: "manual".to_string(),
            phase: None,
            branches: ws
                .branches
                .into_iter()
                .map(BranchSummaryDTO::from)
                .collect(),
            docs_folder_path: ws.docs_folder_path,
            former_names: Vec::new(),
        }
    }
}

impl From<workspace_service::TaskCostBreakdown> for TaskCostBreakdownDTO {
    fn from(j: workspace_service::TaskCostBreakdown) -> Self {
        TaskCostBreakdownDTO {
            task_id: j.task_id.to_string(),
            task_name: j.task_name,
            task_type: j.task_type,
            cost_usd: j.cost_usd,
        }
    }
}

impl From<workspace_service::WorkspaceStats> for WorkspaceStatsDTO {
    fn from(s: workspace_service::WorkspaceStats) -> Self {
        WorkspaceStatsDTO {
            total_cost_usd: s.total_cost_usd,
            tasks_completed: s.tasks_completed,
            total_tasks: s.total_tasks,
            concurrent_tasks: s.concurrent_tasks,
            tasks_stopped: s.tasks_stopped,
            by_task: s
                .by_task
                .into_iter()
                .map(TaskCostBreakdownDTO::from)
                .collect(),
            branches: s
                .branches
                .into_iter()
                .map(BranchDivergenceDTO::from)
                .collect(),
        }
    }
}

impl From<workspace_service::BranchDivergence> for BranchDivergenceDTO {
    fn from(b: workspace_service::BranchDivergence) -> Self {
        BranchDivergenceDTO {
            workspace_id: b.workspace_id.to_string(),
            repo_id: b.repo_id.to_string(),
            repo_name: b.repo_name,
            branch_name: b.branch_name,
            ahead_of_base: b.ahead_of_base,
            behind_base: b.behind_base,
        }
    }
}

impl From<workspace_service::WorkspaceBranchDetail> for WorkspaceBranchDetailDTO {
    fn from(b: workspace_service::WorkspaceBranchDetail) -> Self {
        WorkspaceBranchDetailDTO {
            workspace_id: b.workspace_id.to_string(),
            repo_id: b.repo_id.to_string(),
            repo_name: b.repo_name,
            branch_name: b.branch_name,
            head_commit: b.head_commit,
            tracking_mode: b.tracking_mode,
            materialization_status: b.materialization_status,
            has_draft: b.has_draft,
            dirty_file_count: b.dirty_file_count,
            dirty_files: b.dirty_files,
            remote_branch: b.remote_branch,
            local_branch_name: b.local_branch_name,
            co_tracking_workspaces: b
                .co_tracking_workspaces
                .map(|cs| cs.into_iter().map(CoTrackingWorkspaceDTO::from).collect()),
        }
    }
}

impl From<workspace_service::SyncResult> for SyncResultDTO {
    fn from(r: workspace_service::SyncResult) -> Self {
        SyncResultDTO {
            branches_updated: r.branches_updated,
            branches_skipped: r.branches_skipped,
            details: r
                .details
                .into_iter()
                .map(BranchSyncDetailDTO::from)
                .collect(),
        }
    }
}

impl From<workspace_service::BranchSyncDetail> for BranchSyncDetailDTO {
    fn from(d: workspace_service::BranchSyncDetail) -> Self {
        BranchSyncDetailDTO {
            workspace_id: d.workspace_id.to_string(),
            repo_id: d.repo_id.to_string(),
            repo_name: d.repo_name,
            branch_name: d.branch_name,
            status: d.status,
            old_commit: d.old_commit,
            new_commit: d.new_commit,
            held_by: d.held_by,
        }
    }
}

impl From<workspace_service::CoordinatedOperationResult> for CoordinatedOperationResultDTO {
    fn from(r: workspace_service::CoordinatedOperationResult) -> Self {
        CoordinatedOperationResultDTO {
            operation: r.operation,
            status: r.status,
            message: r.message,
            repos: r
                .repos
                .into_iter()
                .map(CoordinatedRepoEvidenceDTO::from)
                .collect(),
        }
    }
}

impl From<workspace_service::CoordinatedRepoEvidence> for CoordinatedRepoEvidenceDTO {
    fn from(r: workspace_service::CoordinatedRepoEvidence) -> Self {
        CoordinatedRepoEvidenceDTO {
            workspace_id: r.workspace_id.map(|id| id.to_string()),
            repo_id: r.repo_id.to_string(),
            repo_name: r.repo_name,
            branch_name: r.branch_name,
            remote_branch: r.remote_branch,
            operation: r.operation,
            status: r.status,
            old_commit: r.old_commit,
            new_commit: r.new_commit,
            error: r.error,
        }
    }
}

impl From<workspace_service::PromoteRemoteTrackingResult> for PromoteRemoteTrackingResponseDTO {
    fn from(r: workspace_service::PromoteRemoteTrackingResult) -> Self {
        PromoteRemoteTrackingResponseDTO {
            branch: BranchSummaryDTO::from(r.branch),
            coordinated: CoordinatedOperationResultDTO::from(r.coordinated),
            outcome: r.outcome,
            merge_run_id: r.merge_run_id.map(|id| id.to_string()),
        }
    }
}

impl From<workspace_service::PushToRemoteResult> for PushToRemoteResponseDTO {
    fn from(r: workspace_service::PushToRemoteResult) -> Self {
        PushToRemoteResponseDTO {
            enqueued: r.enqueued,
            coordinated: r.coordinated.map(CoordinatedOperationResultDTO::from),
        }
    }
}

impl From<workspace_service::ApprovedRebasePublishResult> for PushToRemoteResponseDTO {
    fn from(r: workspace_service::ApprovedRebasePublishResult) -> Self {
        PushToRemoteResponseDTO {
            enqueued: r.enqueued,
            coordinated: r.coordinated.map(CoordinatedOperationResultDTO::from),
        }
    }
}

impl From<workspace_service::MergeResult> for MergeResultDTO {
    fn from(r: workspace_service::MergeResult) -> Self {
        MergeResultDTO {
            branches_merged: r.branches_merged,
            branches_failed: r.branches_failed,
            details: r
                .details
                .into_iter()
                .map(BranchMergeDetailDTO::from)
                .collect(),
        }
    }
}

impl From<workspace_service::BranchMergeDetail> for BranchMergeDetailDTO {
    fn from(d: workspace_service::BranchMergeDetail) -> Self {
        BranchMergeDetailDTO {
            workspace_id: d.workspace_id.to_string(),
            repo_id: d.repo_id.to_string(),
            repo_name: d.repo_name,
            branch_name: d.branch_name,
            status: d.status,
            merge_commit: d.merge_commit,
            error: d.error,
        }
    }
}

impl From<workspace_fork_service::CloneWorkspaceResponse> for CloneWorkspaceResponseDTO {
    fn from(r: workspace_fork_service::CloneWorkspaceResponse) -> Self {
        CloneWorkspaceResponseDTO {
            workspace: WorkspaceListItemDTO::from_clone_detail(r.workspace),
            cloned_branches: r.cloned_branches,
            cloned_drafts: r.cloned_drafts,
            carried_sessions: r.carried_sessions,
            issue_link: r.issue_link,
            started_chat: r.started_chat.map(|chat| CloneStartedChatDTO {
                session_id: chat.session_id,
                error: chat.error,
                workspace_start_error: None,
            }),
            source_advisories: Vec::new(),
            reverification_run: None,
        }
    }
}

impl WorkspaceListItemDTO {
    /// Convert a `CloneWorkspaceDetail` into a `WorkspaceListItemDTO`.
    ///
    /// The clone service returns its own detail type that has the same logical
    /// shape as `WorkspaceListItem`. This helper handles the conversion,
    /// mapping `ClonedBranchSummary` fields to `BranchSummaryDTO`.
    fn from_clone_detail(detail: workspace_fork_service::CloneWorkspaceDetail) -> Self {
        WorkspaceListItemDTO {
            id: detail.id.to_string(),
            name: detail.name,
            description: detail.description,
            is_base: false,
            read_only: detail.read_only,
            // A clone response describes the new workspace, not its listing;
            // the list query derives `is_review` from the fork record.
            is_review: false,
            agent_review: None,
            kind: g5_contracts::types::WorkspaceKind::Working,
            owner_user_id: Some(detail.owner_user_id.to_string()),
            created_by: detail.created_by.map(|id| id.to_string()),
            cloned_from_id: detail.cloned_from_id.map(|id| id.to_string()),
            branches: detail
                .branches
                .into_iter()
                .map(|b| BranchSummaryDTO {
                    workspace_id: b.workspace_id.to_string(),
                    repo_id: b.repo_id.to_string(),
                    repo_name: b.repo_name,
                    branch_name: b.branch_name,
                    tracking_mode: b.tracking_mode,
                    head_commit: b.head_commit,
                    divergence: b.divergence,
                    behind_base: b.behind_base,
                    has_draft: b.has_draft,
                    dirty_file_count: b.dirty_file_count,
                    // Fork remote-tracking branch surfaces its remote ref name:
                    // `g5/<clone-name>` (isolate) or the shared source branch
                    // (co-track); `None` for a non-tracked repo (design §3.3).
                    remote_branch: b.remote_branch,
                    local_branch_name: None,
                    remote_repo_name: None,
                    // Fork remote-tracking advisory: `Some([])` for an isolate
                    // fork, `Some([source, …peers])` for a co-track fork, `None`
                    // for a non-tracked repo (design §3.3; test_view D9/D90).
                    co_tracking_workspaces: b
                        .co_tracking_workspaces
                        .map(|cs| cs.into_iter().map(CoTrackingWorkspaceDTO::from).collect()),
                    co_tracking_workspace_count: None,
                    auto_pull_enabled: None,
                    remote_eligible: None,
                })
                .collect(),
            tasks_completed: detail.tasks_completed,
            total_tasks: detail.total_tasks,
            running_time: detail.running_time,
            cost: detail.cost,
            concurrent_tasks: detail.concurrent_tasks,
            last_updated: detail.last_updated,
            created_at: detail.created_at,
            // The clone response reports its issue association separately
            // (`issueLink`).
            primary_issue: None,
            // The fork copied its source's phase; the list/detail reads report
            // it, the clone response does not.
            phase: None,
        }
    }
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Workspace routes nested under `/api/v1/orgs/:orgId/workspaces`.
///
/// Handles read-only workspace discovery scoped to an org. Working workspace
/// creation is performed only by `POST /workspaces/:id/clone` from an existing
/// base or working workspace.
///
/// Endpoints:
/// - `GET /` — list workspaces
pub fn org_workspaces_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/", get(list_workspaces), &[Scope::WorkspacesRead])
        .audited_route(
            "/open-question-metrics",
            get(get_open_question_metrics),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

/// Workspace routes nested under `/api/v1/workspaces`.
///
/// Handles detail, lifecycle actions, and workspace-scoped sub-resources.
///
/// Endpoints:
/// - `GET /:id` — get workspace detail
/// - `GET /:id/manifest` — get workspace materialization manifest
/// - `PATCH /:id` — update workspace
/// - `GET /:id/stats` — get workspace statistics
/// - `GET /:id/branches` — list branches
/// - `POST /:id/sync` — sync workspace
/// - `POST /:id/merge` — merge workspace
/// - `POST /:id/clone` — clone workspace
/// - `GET /:id/source-download` — download workspace code repos source archive
/// - `GET /:id/members` — list workspace members
/// - `POST /:id/members` — add workspace member
/// - `DELETE /:id/members/:userId` — remove workspace member
pub fn workspace_detail_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/:id", get(get_workspace), &[Scope::WorkspacesRead])
        .audited_route(
            "/:id/user-activity",
            get(user_activity_clock),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/user-activity",
            post(record_user_activity),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/manifest",
            get(get_workspace_manifest),
            &[Scope::WorkspacesRead],
        )
        .audited_route("/:id", patch(update_workspace), &[Scope::WorkspacesWrite])
        .audited_route(
            "/:id/rename",
            post(rename_workspace),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/journey",
            get(get_workspace_journey),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/journey/steps/:stepId",
            put(set_workspace_journey_step),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/resume",
            post(resume_workspace_vm),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/stop",
            post(stop_workspace_vm),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/delete",
            post(delete_workspace_action),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/transfer",
            post(transfer_workspace),
            &[Scope::WorkspacesWrite],
        )
        .audited_route("/:id/vm", get(get_workspace_vm), &[Scope::WorkspacesRead])
        .audited_route(
            "/:id/changes",
            get(get_workspace_changes),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/changes/:action",
            post(run_workspace_changes_action),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/files/rename",
            post(rename_workspace_file),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/files/*path",
            get(get_workspace_file),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/files/*path",
            // Project Docs and HTML Apply share this route with Source Save.
            // Preserve the docs router's 32 MiB JSON request envelope.
            put(put_workspace_file)
                .layer(DefaultBodyLimit::max(MAX_WORKSPACE_FILE_BYTES))
                .delete(delete_workspace_file),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/mutation-policy",
            post(transition_mutation_policy),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/stats",
            get(get_workspace_stats),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/provider-quota",
            get(get_workspace_provider_quota),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/branches",
            get(list_branches),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/sync/pending",
            get(list_pending_sync_files),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/repos/:repoId/sync_runs",
            get(list_branch_sync_runs),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/repos/:repoId/sync_runs/retry",
            post(retry_branch_sync_runs),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/remote_tracking",
            post(promote_branch_repo_remote_tracking),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/auto_pull",
            put(set_branch_repo_auto_pull),
            &[Scope::WorkspacesWrite],
        )
        // The preview is a POST under write scope: it is owner-only and fetches
        // from the provider into the shared mirror with the owner's
        // credentials, even though it moves no ref.
        .audited_route(
            "/:id/repos/:repoId/provider_history_preview",
            post(preview_provider_history_acceptance),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/accept_provider_history",
            post(accept_provider_history),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/publish",
            post(publish_branch_repo),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/merge_runs",
            get(list_branch_merge_runs),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/repos/:repoId/merge_runs/:mergeRunId/complete",
            post(complete_branch_merge_run),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/merge_runs/start",
            post(start_branch_resolution),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/repos/:repoId/merge_runs/:mergeRunId/start",
            post(start_branch_merge_run),
            &[Scope::WorkspacesWrite],
        )
        .audited_route("/:id/sync", post(sync_workspace), &[Scope::WorkspacesWrite])
        .audited_route(
            "/:id/push_to_remote",
            post(push_to_remote),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/remote_tracking/approved_rebase_publish",
            post(approved_rebase_publish),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/push_mode",
            put(set_workspace_push_mode),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/merge",
            post(merge_workspace),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/rebase-merge",
            post(start_workspace_merge),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/rebase-merge/change-graph",
            get(get_rebase_change_graph),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/rebase-merge/change-graph/nodes/:artifactKey/details",
            get(get_rebase_change_graph_node_details),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/rebase-merge/preflight",
            post(preflight_workspace_merge),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/rebase-merge/direct-clean",
            post(direct_clean_workspace_merge),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/targets",
            get(list_pull_request_targets),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests/info",
            get(get_pull_request_info),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests/github",
            get(list_repository_pull_requests),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests",
            get(list_workspace_pull_requests),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests",
            post(create_workspace_pull_request),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/preview",
            post(preview_workspace_pull_request_text),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/description",
            get(get_workspace_pull_request_description),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests/descriptions",
            get(get_workspace_pull_request_descriptions),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/merge",
            post(merge_workspace_pull_request),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/merge-override",
            get(preview_merge_override),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/merge-override",
            post(merge_with_override),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/refresh",
            post(refresh_workspace_pull_request),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/close",
            post(close_workspace_pull_request),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/auto-merge",
            post(enable_workspace_pull_request_auto_merge),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/auto-merge",
            delete(disable_workspace_pull_request_auto_merge),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/pull-requests/:pullRequestId/auto-merge",
            patch(update_workspace_pull_request_auto_merge_settings),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/clone",
            post(clone_workspace),
            &[Scope::WorkspacesWrite],
        )
        .audited_route(
            "/:id/fork_advisories",
            get(get_fork_advisories),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:id/source-download",
            get(download_workspace_source),
            &[Scope::WorkspacesRead],
        )
        .into_router()
}

/// A passive platform clock sample, never an activity update. This avoids
/// trusting the browser's wall clock without re-dating delayed gestures.
async fn user_activity_clock(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    db.commit().await.map_err(AppError::from)?;
    state
        .workspace_runtime
        .owner_handle(params.id, user.user_uuid())
        .await?;
    let server_time = state.workspace_runtime.activity_clock().await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ResponseEnvelope::ok(
            serde_json::json!({"serverTime": server_time}),
        )),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UserActivityRequest {
    vm_id: Uuid,
    epoch: u64,
    observed_at: chrono::DateTime<chrono::Utc>,
    reason: crate::services::workspace_runtime::WorkspaceActivityReason,
}

/// Separate from reads/mutations so refetch, recovery and downstream work
/// cannot inherit an activity side effect. The owner may interact read-only.
async fn record_user_activity(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Extension(request_id): Extension<crate::middleware::request_id::RequestId>,
    Json(body): Json<UserActivityRequest>,
) -> Result<Json<ResponseEnvelope<()>>, AppError> {
    use crate::services::workspace_runtime::WorkspaceActivityReason;
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    db.commit().await.map_err(AppError::from)?;
    if !matches!(
        body.reason,
        WorkspaceActivityReason::Keyboard
            | WorkspaceActivityReason::Paste
            | WorkspaceActivityReason::Pointer
    ) {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"reason": "expected a browser interaction"}),
        }));
    }
    let handle = state
        .workspace_runtime
        .owner_handle(params.id, user.user_uuid())
        .await?;
    if handle.vm_id == body.vm_id && handle.epoch == body.epoch {
        state
            .workspace_runtime
            .record_user_activity(&handle, body.reason, &request_id.0, body.observed_at)
            .await?;
    }
    Ok(Json(ResponseEnvelope::ok(())))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/orgs/:orgId/workspaces`
///
/// Lists every live workspace in an org with computed stats and per-branch details.
///
/// Unpaginated by design — the response carries no `meta` and the returned
/// array is the complete set. See the `list_workspaces` docs on
/// `WorkspaceService` for why paging this endpoint was incorrect rather than
/// merely limiting.
async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<Vec<WorkspaceListItemDTO>>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    tracing::debug!(org_id = %params.org_id, "Workspace endpoint called: list_workspaces");

    let result = state
        .workspace_service
        .list_workspaces(&mut db, params.org_id, user.user_uuid())
        .await?;

    let items: Vec<WorkspaceListItemDTO> =
        result.into_iter().map(WorkspaceListItemDTO::from).collect();

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(items)))
}

fn parse_workspace_metric_ids(raw: &str) -> Result<Vec<Uuid>, AppError> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for value in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let id = Uuid::parse_str(value).map_err(|_| {
            AppError::from(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "workspaceIds",
                    "message": "workspaceIds must contain comma-separated UUIDs",
                }),
            })
        })?;
        if seen.insert(id) {
            ids.push(id);
        }
    }
    if ids.is_empty() || ids.len() > 50 {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "workspaceIds",
                "message": "workspaceIds must contain between 1 and 50 unique workspace UUIDs",
            }),
        }));
    }
    Ok(ids)
}

/// `GET /api/v1/orgs/:orgId/workspaces/open-question-metrics`
async fn get_open_question_metrics(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<OrgPathParams>,
    Query(query): Query<WorkspaceOpenQuestionMetricsQueryDTO>,
) -> Result<Json<ResponseEnvelope<Vec<WorkspaceOpenQuestionMetricsDTO>>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_org(&state, &auth, params.org_id, &["workspaces:read"]).await?;
    let workspace_ids = parse_workspace_metric_ids(&query.workspace_ids)?;

    let captured = state
        .workspace_service
        .capture_open_question_metrics(
            &mut db,
            params.org_id,
            user.user_uuid(),
            &workspace_ids,
            &state.doc_service,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    // Effective Git+live workspace file Project Doc reads happen only
    // after the capture transaction closed (the capture/materialize seam).
    let metrics = state
        .workspace_service
        .open_question_metrics(captured, &state.doc_service, &state.workspace_runtime)
        .await?
        .into_iter()
        .map(WorkspaceOpenQuestionMetricsDTO::from)
        .collect();
    Ok(Json(ResponseEnvelope::ok(metrics)))
}

/// `POST /api/v1/orgs/:orgId/workspaces`
///
/// Creates a new workspace with branches, dependencies, and settings
/// Returns full workspace detail with branches and dependencies.
async fn get_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceDetailDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: get_workspace");
    let caller_user_id = if auth.is_user() {
        Some(auth.require_user_id().map_err(AppError::from)?)
    } else {
        None
    };

    let detail = state
        .workspace_service
        .get_workspace(&mut db, params.id, caller_user_id)
        .await?;
    let mut dto: WorkspaceDetailDTO = detail.into();

    // Surface the per-workspace push_mode (arch D66) for the UI toggle. A
    // runtime query keeps it out of the compile-time `WorkspaceDetail`
    // `query_as!` macro that several call sites share.
    let (push_mode, owner_user_id, phase): (String, Option<Uuid>, String) =
        sqlx::query_as("SELECT push_mode, owner_user_id, phase FROM workspaces WHERE id = $1")
            .bind(params.id)
            .fetch_one(db.conn())
            .await
            .map_err(AppError::from)?;
    dto.push_mode = push_mode;
    dto.owner_user_id = owner_user_id.map(|id| id.to_string());
    dto.phase = Some(phase);
    dto.former_names = crate::services::workspace_names::former_names(db.conn(), params.id).await?;

    // Option 1 (G5 #1198), accepted `workspace_open` decision: workspace GET is
    // read-only and no longer enqueues an implicit `workspace_open` inbound
    // cycle. Initial population is performed by explicit Pull (which reuses the
    // worker's ensure-mirror → ls-remote → fetch → integrate behavior via an
    // explicit `manual` trigger) and, when enabled, by background auto-pull;
    // adding a repo remains responsible for its own initial bootstrap. Ontology
    // `software_architecture/g5-api/models/branch_integration_queue` D8.
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/workspaces/:id/manifest`
///
/// Returns a workspace materialization manifest that `g5d clone` / `refresh`
/// can serialize directly into `.g5/workspace.yaml`.
async fn get_workspace_manifest(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<workspace_service::WorkspaceManifest>>, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(
        workspace_id = %params.id,
        "Workspace endpoint called: get_workspace_manifest"
    );

    let manifest = state
        .workspace_service
        .get_workspace_manifest(db, params.id, &state.external_url, &state.git_external_url)
        .await?;

    Ok(Json(ResponseEnvelope::ok(manifest)))
}

/// `GET /api/v1/workspaces/:id/sync/pending?direction=downlift`
///
/// Returns committed files across the workspace that are pending downlift sync.
async fn list_pending_sync_files(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<PendingSyncFilesQueryDTO>,
) -> Result<Json<ResponseEnvelope<Vec<PendingSyncFileDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let direction = query.direction.as_deref().unwrap_or("downlift");
    if direction != "downlift" {
        return Err(AppError::from(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "direction",
                "message": "pending sync files only support direction=downlift",
            }),
        }));
    }

    tracing::debug!(
        workspace_id = %params.id,
        "Workspace endpoint called: list_pending_sync_files"
    );

    let mut rows = Vec::new();
    let connected = resolve_connected_binding_view(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace {
            workspace_id: params.id,
        },
    )
    .await
    .map_err(AppError::from)?;

    for row in state
        .doc_service
        .list_pending_downlift_docs(&mut db, params.id, &connected)
        .await?
    {
        rows.push(PendingSyncFileDTO {
            id: row.id,
            target_kind: "doc".to_string(),
            path: row.path,
            repo_id: row.repo_id.to_string(),
            repo_name: row.repo_name,
            branch_name: row.branch_name,
            head_commit: row.head_commit,
            last_downlifted_commit_sha: row.last_downlifted_commit_sha,
            pending_reasons: [row
                .document_changed
                .then_some("document_change".to_string())]
            .into_iter()
            .flatten()
            .collect(),
            target_doc_ref: Some(row.target_doc_ref),
        });
    }

    rows.sort_by(|a, b| {
        a.repo_name
            .cmp(&b.repo_name)
            .then_with(|| a.branch_name.cmp(&b.branch_name))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.target_kind.cmp(&b.target_kind))
    });

    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %params.id,
        pending_count = rows.len(),
        "Workspace endpoint succeeded: list_pending_sync_files"
    );
    Ok(Json(ResponseEnvelope::ok(rows)))
}

/// `GET /api/v1/workspaces/:id/source-download`
///
/// Downloads all eligible non-doc code repository source snapshots.
async fn download_workspace_source(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Response, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let membership =
        crate::services::identity::membership_activity::MembershipActivityGuard::capture(
            &state.system_pool,
            db.primary_org_id().map_err(AppError::from)?.into_uuid(),
            auth.require_user_id()?,
        )
        .await?;
    tracing::debug!(
        workspace_id = %params.id,
        "Workspace endpoint called: download_workspace_source"
    );

    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    let scope = db.reopener();
    let source = state
        .workspace_service
        .resolve_workspace_source_download(db, params.id)
        .await?;

    let mut archive_repos = Vec::with_capacity(source.repos.len());
    for repo in &source.repos {
        let repo_view =
            CommittedRepoReadView::from_binding(&scope, org_id, repo).map_err(AppError::from)?;
        let file_paths = repo_view
            .list_tree(&state.repo_storage, "", u32::MAX)
            .await
            .map_err(AppError::from)?
            .into_iter()
            .filter(|entry| matches!(entry.entry_type, TreeEntryType::File))
            .map(|entry| entry.path)
            .filter(|path| !should_skip_source_archive_path(path))
            .map(|path| {
                validate_source_archive_path(&path)?;
                Ok(path)
            })
            .collect::<Result<Vec<_>, G5Error>>()?;
        let reads = repo_view
            .read_files(&state.repo_storage, &file_paths)
            .await
            .map_err(AppError::from)?;
        let mut files = BTreeMap::new();
        for path in file_paths {
            let bytes = reads
                .get(&path)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    AppError::from(G5Error::Internal {
                        message: format!(
                            "Workspace source archive tree member '{path}' is missing from the exact repository cut"
                        ),
                    })
                })?
                .clone();
            files.insert(path, bytes);
        }
        archive_repos.push(WorkspaceSourceZipRepo {
            repo_id: repo.repo_id,
            repo_name: repo.repo_name.clone(),
            files,
        });
    }

    let archive = tokio::task::spawn_blocking(move || -> Result<std::fs::File, G5Error> {
        let mut archive = tempfile::tempfile().map_err(source_zip_io_error)?;
        write_workspace_source_zip(&archive_repos, &mut archive)?;
        archive
            .seek(SeekFrom::Start(0))
            .map_err(source_zip_io_error)?;
        Ok(archive)
    })
    .await
    .map_err(|e| {
        AppError::from(G5Error::Internal {
            message: format!("Workspace source archive task failed: {e}"),
        })
    })??;

    let filename = format!("{}-source.zip", safe_download_stem(&source.workspace_name));

    tracing::info!(
        workspace_id = %params.id,
        repo_count = source.repos.len(),
        "Workspace endpoint succeeded: download_workspace_source"
    );

    use futures_util::StreamExt as _;
    let stream = ReaderStream::with_capacity(tokio::fs::File::from_std(archive), 256 * 1024).then(
        move |chunk| {
            let membership = membership.clone();
            async move {
                if !membership.authorize().await {
                    return Err(std::io::Error::other("Organization access changed"));
                }
                chunk
            }
        },
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from_stream(stream))
        .map_err(|e| {
            AppError::from(G5Error::Internal {
                message: format!("Failed to build workspace source download response: {e}"),
            })
        })
}

#[derive(Debug, Clone)]
struct WorkspaceSourceZipRepo {
    repo_id: Uuid,
    repo_name: String,
    files: BTreeMap<String, Vec<u8>>,
}

fn write_workspace_source_zip<W: Write + Seek>(
    repos: &[WorkspaceSourceZipRepo],
    writer: W,
) -> Result<(), G5Error> {
    let roots = source_zip_roots(repos);
    let mut zip = zip::ZipWriter::new(writer);

    for (source, root) in repos.iter().zip(roots.iter()) {
        write_workspace_source_repo_to_zip(&mut zip, &source.files, root)?;
    }

    zip.finish().map_err(source_zip_error)?;
    Ok(())
}

fn write_workspace_source_repo_to_zip<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    files: &BTreeMap<String, Vec<u8>>,
    root: &str,
) -> Result<(), G5Error> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for (path, bytes) in files {
        let archive_path = format!("{root}/{path}");
        zip.start_file(archive_path, options)
            .map_err(source_zip_error)?;
        zip.write_all(bytes).map_err(source_zip_io_error)?;
    }

    Ok(())
}

fn source_zip_roots(repos: &[WorkspaceSourceZipRepo]) -> Vec<String> {
    let stems: Vec<String> = repos
        .iter()
        .map(|source| safe_zip_segment(&source.repo_name))
        .collect();
    let mut stem_counts: HashMap<String, usize> = HashMap::new();
    for stem in &stems {
        *stem_counts.entry(stem.clone()).or_default() += 1;
    }

    let mut used_roots: HashMap<String, usize> = HashMap::new();
    repos
        .iter()
        .zip(stems)
        .map(|(source, stem)| {
            let base_root = if stem_counts.get(&stem).copied().unwrap_or(0) > 1 {
                format!("{stem}-{}", short_repo_id(source.repo_id))
            } else {
                stem
            };
            let mut root = base_root.clone();
            let mut duplicate_count = 2;
            while used_roots.contains_key(&root) {
                root = format!("{base_root}-{duplicate_count}");
                duplicate_count += 1;
            }
            used_roots.insert(root.clone(), 1);
            root
        })
        .collect()
}

fn short_repo_id(repo_id: Uuid) -> String {
    let simple = repo_id.simple().to_string();
    simple.chars().take(8).collect()
}

fn should_skip_source_archive_path(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment.eq_ignore_ascii_case(".git"))
}

fn validate_source_archive_path(path: &str) -> Result<(), G5Error> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
        return Err(source_zip_path_error(path));
    }
    for segment in path.split('/') {
        if segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.eq_ignore_ascii_case(".git")
        {
            return Err(source_zip_path_error(path));
        }
    }
    Ok(())
}

fn source_zip_path_error(path: &str) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "message": "Workspace source archive contains an unsafe repo path",
            "path": path,
        }),
    }
}

fn source_zip_error(error: zip::result::ZipError) -> G5Error {
    G5Error::Internal {
        message: format!("Failed to build workspace source archive: {error}"),
    }
}

fn source_zip_io_error(error: std::io::Error) -> G5Error {
    G5Error::Internal {
        message: format!("Failed to write workspace source archive: {error}"),
    }
}

fn safe_download_stem(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let collapsed = sanitized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let trimmed = collapsed.trim_matches(&['-', '.', '_'][..]);
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

fn safe_zip_segment(input: &str) -> String {
    let stem = safe_download_stem(input);
    if stem == "." || stem == ".." {
        "repo".to_string()
    } else {
        stem
    }
}

/// `PATCH /api/v1/workspaces/:id`
///
/// Updates owner-managed workspace metadata without changing branch identity.
async fn update_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<UpdateWorkspaceRequestDTO>,
) -> Result<Json<ResponseEnvelope<WorkspaceDetailDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    if body.description.is_some() {
        crate::services::workspace_mutation_policy::ensure_workspace_mutable(
            &mut db,
            params.id,
            "workspaces.update_description",
        )
        .await?;
    }
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: update_workspace");

    // Workbench phase (workbench g5.workbench.phases.state): owner-moved
    // explicit state, never derived. The workspace service validates the
    // literal, refuses Base and records the move durably in this transaction.
    if let Some(ref phase) = body.phase {
        let actor_user_id = if auth.is_user() {
            Some(auth.require_user_id().map_err(AppError::from)?)
        } else {
            None
        };
        state
            .workspace_service
            .set_workspace_phase(&mut db, params.id, phase, actor_user_id)
            .await?;
    }

    let req = workspace_service::UpdateWorkspaceRequest {
        description: body.description,
    };
    let caller_user_id = if auth.is_user() {
        Some(auth.require_user_id().map_err(AppError::from)?)
    } else {
        None
    };

    let detail = state
        .workspace_service
        .update_workspace(&mut db, params.id, caller_user_id, req)
        .await?;
    let mut dto: WorkspaceDetailDTO = detail.into();
    // The phase just written is the live value; report it so the client can
    // settle its optimistic chevron without a second read.
    dto.phase = body.phase;

    tracing::info!(workspace_id = %params.id, "Workspace endpoint succeeded: update_workspace");
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `POST /api/v1/workspaces/:id/rename`
///
/// Renames a working workspace (`g5.workspace.rename`): owner only, under the
/// configuration-authority gate, which also refuses while an ownership
/// transfer is pending. Base is refused, and formal agent-review workspaces
/// are not visible to human writes at all. The transaction touches only the
/// database; the branch the workspace owns never changes. The previous name
/// stays reserved and keeps resolving (`g5.workspace.former-names`).
async fn rename_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<RenameWorkspaceRequestDTO>,
) -> Result<Json<ResponseEnvelope<WorkspaceRenameDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: rename_workspace");
    let actor_user_id = if auth.is_user() {
        Some(auth.require_user_id().map_err(AppError::from)?)
    } else {
        None
    };
    let renamed = state
        .workspace_service
        .rename_workspace(&mut db, params.id, actor_user_id, &body.name)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(workspace_id = %params.id, "Workspace endpoint succeeded: rename_workspace");
    Ok(Json(ResponseEnvelope::ok(WorkspaceRenameDTO {
        id: renamed.id.to_string(),
        name: renamed.name,
        former_names: renamed.former_names,
    })))
}

/// `GET /api/v1/workspaces/:id/journey`
///
/// The latest guided-journey mark per step (workbench
/// g5.workbench.journey.marks). Readable by anyone who reads the workspace;
/// served under every layout.
async fn get_workspace_journey(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceJourneyDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: get_workspace_journey");
    let marks = state
        .workspace_service
        .list_journey_marks(&mut db, params.id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(WorkspaceJourneyDTO { marks })))
}

/// `PUT /api/v1/workspaces/:id/journey/steps/:stepId`
///
/// The owner's "Mark done" / "Not done" for one step (workbench
/// g5.workbench.journey.marks): owner-only durable authority like the phase
/// move, refused on Base, idempotent, and recorded as a timeline event.
/// Responds with the whole mark list so the client settles in one read.
async fn set_workspace_journey_step(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceJourneyStepPathParams>,
    Json(body): Json<SetJourneyMarkRequestDTO>,
) -> Result<Json<ResponseEnvelope<WorkspaceJourneyDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    // Owner-only durable write under the workspace mutation policy
    // (g5.workspace.ui.gates): a read-only or freezing workspace refuses it
    // like any other owner write.
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.journey_mark",
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.id,
        step_id = %params.step_id,
        "Workspace endpoint called: set_workspace_journey_step"
    );
    let actor_user_id = if auth.is_user() {
        Some(auth.require_user_id().map_err(AppError::from)?)
    } else {
        None
    };
    state
        .workspace_service
        .set_journey_mark(
            &mut db,
            params.id,
            &params.step_id,
            body.done,
            body.skipped,
            actor_user_id,
        )
        .await?;
    let marks = state
        .workspace_service
        .list_journey_marks(&mut db, params.id)
        .await?;
    tracing::info!(
        workspace_id = %params.id,
        step_id = %params.step_id,
        done = body.done,
        skipped = body.skipped,
        "Workspace endpoint succeeded: set_workspace_journey_step"
    );
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(WorkspaceJourneyDTO { marks })))
}

/// `POST /api/v1/workspaces/:id/mutation-policy`
///
/// Transition a live workspace between `read_write` and `read_only` (spec
/// routes/workspaces [I5c]). Current-owner gated and classified as workspace
/// administration so it is not self-blocked on an already-read-only workspace. RW→RO is a
/// two-phase freeze/drain; RO→RW is instantaneous. The orchestration is owned by
/// `workspace_service`.
async fn transition_mutation_policy(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<MutationPolicyTransitionRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<MutationPolicyTransitionResponseDTO>>,
    ),
    AppError,
> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    // AdminWorkspaceAdministration is a guard no-op, so a read-only workspace is
    // not self-blocked from flipping back to read-write; the explicit call keeps
    // the route classified for the route_auth audit (spec
    // workspace_mutation_policy [I11]).
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        params.id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::AdminWorkspaceAdministration,
        "workspaces.transition_mutation_policy",
    )
    .await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: transition_mutation_policy");

    let actor = user.user_uuid();
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();

    let target = match body.policy.as_str() {
        "read_only" => workspace_service::TargetMutationPolicy::ReadOnly,
        "read_write" => workspace_service::TargetMutationPolicy::ReadWrite,
        other => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: format!("Invalid policy '{other}'. Must be 'read_only' or 'read_write'.")
                    .into(),
            }));
        }
    };

    // `mode` / `drainTimeoutSec` are meaningful only for RW→RO.
    if matches!(target, workspace_service::TargetMutationPolicy::ReadWrite)
        && (body.mode.is_some() || body.drain_timeout_sec.is_some())
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: "mode and drainTimeoutSec apply only to a read_write → read_only transition."
                .into(),
        }));
    }

    let mode = match body.mode.as_deref() {
        None | Some("drain") => workspace_service::TransitionMode::Drain,
        Some("require_quiescent") => workspace_service::TransitionMode::RequireQuiescent,
        Some(other) => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: format!("Invalid mode '{other}'. Must be 'drain' or 'require_quiescent'.")
                    .into(),
            }));
        }
    };
    let drain_timeout_secs = body.drain_timeout_sec.unwrap_or(300);

    // Release the request-scoped connection before the (possibly long) drain;
    // the service rechecks current owner authority in every later transaction.
    db.rollback().await.map_err(AppError::from)?;

    let outcome = state
        .workspace_service
        .transition_mutation_policy(
            &state.event_bus,
            workspace_service::MutationPolicyTransitionCommand {
                org_id,
                workspace_id: params.id,
                actor,
                target,
                mode,
                drain_timeout_secs,
            },
        )
        .await?;

    // Status mapping (spec [I5c]): the endpoint returns a STRUCTURED outcome
    // (committed / blockers / message) via the success envelope, and the shared
    // client treats every non-2xx as an opaque error (reading only error.message,
    // else falling back to statusText). So structured outcomes use a 2xx status
    // and never mismatch their success body: `committed` → 200; a non-committal
    // refusal/revert carrying blockers + message (up-front precondition,
    // require_quiescent, or drain-timeout) → ALSO 200 so the client's success path
    // renders the blockers instead of collapsing to a bare "Conflict"; `freezing`
    // (async drain accepted, running in the background) → 202. Genuine transient
    // failures (advisory-lock contention, auth, validation) surface separately as
    // an AppError error envelope with a real message.
    let status = if outcome.transition.as_deref() == Some("freezing") {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    tracing::info!(
        workspace_id = %params.id,
        policy = %outcome.policy,
        committed = outcome.committed,
        transition = ?outcome.transition,
        blockers = outcome.blockers.len(),
        "Workspace endpoint succeeded: transition_mutation_policy"
    );
    let dto = MutationPolicyTransitionResponseDTO {
        policy: outcome.policy,
        transition: outcome.transition,
        message: outcome.message,
        blockers: outcome
            .blockers
            .into_iter()
            .map(|b| TransitionBlockerDTO {
                kind: b.kind,
                id: b.id,
                detail: b.detail,
            })
            .collect(),
        drained_tasks: Some(outcome.drained_tasks),
    };
    Ok((status, Json(ResponseEnvelope::ok(dto))))
}

pub(crate) fn workspace_vm_service(
    state: &Arc<AppState>,
    scope: g5_authz::ScopedDbReopener,
) -> Result<crate::services::WorkspaceVmService, AppError> {
    let minter = state.jwt_minter.clone().ok_or_else(|| {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "workspace lifecycle signing is unavailable".to_string(),
        })
    })?;
    Ok(crate::services::WorkspaceVmService::new(
        crate::services::WorkspaceVmServiceConfig {
            system_pool: state.system_pool.clone(),
            scope,
            hosts: state.preview_tunnels.clone(),
            minter,
            runtime: state.workspace_runtime.clone(),
            agent_work: state.agent_work.clone(),
            repo_storage: state.repo_storage.clone(),
            git_external_url: state.git_external_url.clone(),
            ws_hub: state.event_bus.ws_hub().clone(),
        },
    ))
}

/// Authorize an owner-only lifecycle action on `workspace_id` and hand back
/// the tenant scope the lifecycle service runs under.
pub(crate) async fn authorize_workspace_action(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
) -> Result<g5_authz::ScopedDbReopener, AppError> {
    let mut db = scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, workspace_id)
        .await?;
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from).map(|()| scope)
}

async fn get_workspace_vm(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceVmView>>, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    let vm = workspace_vm_service(&state, scope)?.view(params.id).await?;
    Ok(Json(ResponseEnvelope::ok(vm)))
}

async fn get_workspace_changes(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    let report = state
        .workspace_runtime
        .workspace_changes_report(&scope, params.id, user.user_uuid())
        .await?;
    Ok(Json(ResponseEnvelope::ok(report)))
}

async fn run_workspace_changes_action(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspaceChangesActionPathParams>,
    Json(body): Json<WorkspaceChangesActionRequestDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let operation = match params.action.as_str() {
        "push" => "push",
        "update" => "update",
        "switch-back" => "switch_back",
        "commit" => "commit",
        _ => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({"action": "must be commit, push, update, or switch-back"}),
            }))
        }
    };
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.changes_action",
    )
    .await?;
    if let Some(repo_id) = body.repo_id {
        ensure_workspace_repo_id(&mut db, params.id, repo_id, "repoId").await?;
    }
    if operation == "commit" {
        return commit_workspace_changes(&state, db, &user, params.id, body).await;
    }
    // Review updates save work before merging the immutable parent relationship.
    let mut review_fork = None;
    // Held through the save and the rebase of a review copy: the same
    // per-(source, reviewer) admission preview, publish and attach take,
    // so Update and Push review never stage and commit one checkout at
    // once.
    let mut review_admission = None;
    if operation == "update" {
        use crate::services::workspace_review_service as reviews;
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        if let Some(review) =
            reviews::find_review_by_review_workspace(&mut db, org_id, params.id).await?
        {
            review_fork = Some(review);
        }
        db.commit().await.map_err(AppError::from)?;
        if let Some(review) = &review_fork {
            review_admission = Some(
                reviews::lock_review_admission(
                    &state.system_pool,
                    review.source_workspace_id,
                    user.user_uuid(),
                )
                .await?,
            );
            // A review copy has no version-control step of its own: what the
            // reviewer has in the working tree is saved as a commit first,
            // so the rebase never refuses a "dirty" tree that is simply
            // their work in progress (the same save Push review makes).
            let saved = state
                .workspace_runtime
                .workspace_changes_commit(
                    params.id,
                    user.user_uuid(),
                    // The same members the update touches: a selected
                    // repository saves alone.
                    body.repo_id,
                    &format!("Review of {}: before update", review.source_workspace_name),
                )
                .await?;
            crate::routes::workspace_reviews::ensure_changes_succeeded(
                &saved,
                "save your changes before the update",
            )?;
        }
    } else {
        db.rollback().await.map_err(AppError::from)?;
    };
    let report = if operation == "update" {
        // A review copy's rounds reach its source as the owner's decisions:
        // where one of the copy's commits collides with what the source
        // already took, the source's side is kept and reported. An
        // ordinary workspace's update refuses a collision and keeps its
        // branch where it was.
        state
            .workspace_runtime
            .workspace_changes_update(
                params.id,
                user.user_uuid(),
                body.repo_id,
                review_fork.is_some(),
            )
            .await?
    } else {
        state
            .workspace_runtime
            .workspace_changes_action(params.id, user.user_uuid(), operation, body.repo_id)
            .await?
    };
    drop(review_admission);
    Ok(Json(ResponseEnvelope::ok(report)))
}

/// `POST /api/v1/workspaces/:id/changes/commit`: commit the owner's working
/// tree on the workspace branch with `message` and push it, so the branch
/// tip the reviews are measured against moves (the rounds decided before
/// it become outdated, and reviewers are told). The Reviews page offers it
/// after the owner accepted, edited or rejected a round's proposals. A
/// review copy has no commit of its own: Push review saves it.
async fn commit_workspace_changes(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    user: &AuthenticatedUser,
    workspace_id: Uuid,
    body: WorkspaceChangesActionRequestDTO,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    use crate::services::workspace_review_service as reviews;
    let actor = user.user_uuid();
    crate::services::workspace_mutation_policy::ensure_workspace_owner_is(
        &mut db,
        workspace_id,
        actor,
    )
    .await?;
    let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
    if reviews::find_review_by_review_workspace(&mut db, org_id, workspace_id)
        .await?
        .is_some()
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "a review copy is saved by Push review, not committed".to_string(),
        }));
    }
    db.rollback().await.map_err(AppError::from)?;
    let message = body
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .ok_or_else(|| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({"message": "a commit needs a message"}),
            })
        })?
        .to_string();
    let committed = state
        .workspace_runtime
        .workspace_changes_commit(workspace_id, actor, body.repo_id, &message)
        .await?;
    ensure_changes_action_succeeded(&committed, "commit")?;
    let mut members: Vec<serde_json::Value> =
        committed["members"].as_array().cloned().unwrap_or_default();
    // The push runs even when this call committed nothing: a commit whose
    // push failed last time sits on a clean tree, and "Commit and push"
    // pressed again must publish it rather than report success over an
    // unpublished branch. Pushing an up-to-date branch moves nothing, so a
    // truly clean workspace outdates no round.
    let pushed = state
        .workspace_runtime
        .workspace_changes_action(workspace_id, actor, "push", body.repo_id)
        .await?;
    ensure_changes_action_succeeded(&pushed, "push")?;
    let pushed_members: Vec<serde_json::Value> =
        pushed["members"].as_array().cloned().unwrap_or_default();
    for member in &mut members {
        let repo_id = member["repo_id"].clone();
        let pushed = pushed_members
            .iter()
            .find(|candidate| candidate["repo_id"] == repo_id)
            .map(|candidate| candidate["errors"].is_null() && candidate["pushed"] != false)
            .unwrap_or(false);
        member["pushed"] = serde_json::Value::Bool(pushed);
    }
    Ok(Json(ResponseEnvelope::ok(serde_json::json!({
        "workspace_id": committed["workspace_id"],
        "source": committed["source"],
        "attached": committed["attached"],
        "vm_id": committed["vm_id"],
        "epoch": committed["epoch"],
        "status": committed["status"],
        "members": members,
    }))))
}

pub(crate) use crate::services::workspace_file_save::map_file_runtime_error as map_workspace_file_runtime_error;
use crate::services::workspace_file_save::{self, ensure_changes_action_succeeded};

pub(crate) fn workspace_runtime_request(
    handle: &crate::services::workspace_runtime::WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    operation: &str,
) -> crate::services::workspace_runtime::RuntimeRequest {
    workspace_file_save::file_runtime_request(handle, actor_user_id, operation)
}

/// A write scope on the workspace for the person behind `auth`.
async fn workspace_file_write_db(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
) -> Result<g5_authz::ScopedDb, AppError> {
    scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:write"]).await
}

/// Check the file-write policy for the scope's actor and close the scope.
async fn authorize_workspace_file_mutation(
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<(), AppError> {
    let actor = db.actor_id().into_uuid();
    ensure_workspace_file_mutation_allowed(&mut db, workspace_id, actor, repo_id).await?;
    db.rollback().await.map_err(AppError::from)
}

/// The file-write policy on an open connection: `actor_user_id` owns the
/// (non-base, mutable) workspace and `repo_id` is one of its repositories.
/// The review accept path runs it on its own transaction, for the owner
/// or for the owner's agent acting through the internal API.
pub(crate) async fn ensure_workspace_file_mutation_allowed(
    db: &mut g5_authz::ScopedDb,
    workspace_id: Uuid,
    actor_user_id: Uuid,
    repo_id: Uuid,
) -> Result<(), AppError> {
    crate::services::workspace_mutation_policy::ensure_workspace_mutable_by(
        db,
        workspace_id,
        actor_user_id,
        "workspaces.write_file",
    )
    .await?;
    let is_base: bool =
        sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id = $1 AND deleted_at IS NULL")
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await?;
    if is_base {
        return Err(AppError::from_error(G5Error::WorkspaceIsBase));
    }
    ensure_workspace_repo_id(db, workspace_id, repo_id, "repoId").await?;
    Ok(())
}

async fn get_workspace_file(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspaceFilePathParams>,
    Query(query): Query<WorkspaceFileQuery>,
) -> Result<Json<ResponseEnvelope<WorkspaceFileContentDTO>>, AppError> {
    let content =
        read_workspace_file_for_actor(&state, &user, &auth, params.id, query.repo_id, &params.path)
            .await?;
    Ok(Json(ResponseEnvelope::ok(content)))
}

/// The most bytes one workspace file read materializes, committed or live
/// (`workspace_reader::MAX_FILE_READ_BYTES`): the workspace runtime refuses
/// a larger live file (`fs.read`), and a committed file past it is refused
/// from its object header before any content is loaded.
pub const MAX_WORKSPACE_FILE_BYTES: usize = crate::services::workspace_reader::MAX_FILE_READ_BYTES;

/// Read `raw_path` as the actor sees it: the attached owner's live checkout,
/// otherwise the durable committed cut, within [`MAX_WORKSPACE_FILE_BYTES`].
pub(crate) async fn read_workspace_file_for_actor(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
) -> Result<WorkspaceFileContentDTO, AppError> {
    read_workspace_file_for_actor_bounded(
        state,
        user,
        auth,
        workspace_id,
        repo_id,
        raw_path,
        MAX_WORKSPACE_FILE_BYTES,
    )
    .await
}

/// `read_workspace_file_for_actor` with the byte limit the caller applies,
/// [`MAX_WORKSPACE_FILE_BYTES`] or smaller for a purpose narrower than "the
/// file": a committed file larger than the limit is refused from its
/// metadata as [`G5Error::FileTooLarge`], before its bytes are read.
pub(crate) async fn read_workspace_file_for_actor_bounded(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
    max_bytes: usize,
) -> Result<WorkspaceFileContentDTO, AppError> {
    let path = crate::routes::files::sanitize_path(raw_path)?;
    let db = scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:read"]).await?;
    read_workspace_file_as(
        state,
        db,
        user.user_uuid(),
        workspace_id,
        repo_id,
        &path,
        max_bytes,
    )
    .await
}

/// The body of [`read_workspace_file_for_actor_bounded`] on a read scope the
/// caller opened: `actor` reads its own running checkout live and anything
/// else from the committed cut. The workspace agent reads through
/// it as its owner (`g5.security.agent-read-scope`).
pub(crate) async fn read_workspace_file_as(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
    max_bytes: usize,
) -> Result<WorkspaceFileContentDTO, AppError> {
    let max_bytes = max_bytes.min(MAX_WORKSPACE_FILE_BYTES);
    let path = crate::routes::files::sanitize_path(raw_path)?;
    ensure_workspace_repo_id(&mut db, workspace_id, repo_id, "repoId").await?;

    if let Some(handle) = state.workspace_runtime.get(workspace_id).await? {
        if handle.status == "running" && handle.owner_user_id == actor {
            db.rollback().await.map_err(AppError::from)?;
            return read_live_workspace_file(state, &handle, actor, repo_id, &path).await;
        }
    }

    let snapshot = crate::services::workspace_reader::resolve_published_snapshot(
        &mut db,
        &state.repo_storage,
        AuthorityTarget::Workspace { workspace_id },
    )
    .await?;
    let binding = snapshot.repo(repo_id).map_err(|message| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"field":"repoId", "message":message}),
        })
    })?;
    let view = CommittedRepoReadView::from_binding(&db, snapshot.org_id, binding)?;
    let repo_oid = view.head_commit().to_string();
    let metadata = view
        .file_metadata(&state.repo_storage, std::slice::from_ref(&path))
        .await?;
    let (hash, size) = metadata
        .get(&path)
        .and_then(|entry| entry.as_ref())
        .map(|(hash, size)| (hash.clone(), *size))
        .ok_or_else(|| {
            AppError::from_error(G5Error::NotFound {
                resource: format!("file '{path}'"),
            })
        })?;
    if size > max_bytes {
        return Err(AppError::from_error(G5Error::FileTooLarge {
            path,
            size: size as u64,
            max_bytes: max_bytes as u64,
        }));
    }
    let bytes = view.read_file(&state.repo_storage, &path).await?;
    db.rollback().await.map_err(AppError::from)?;
    let size = bytes.len() as u64;
    // The bytes are moved into the text, or back out of the failed
    // conversion for base64: one copy of the file is held, not two.
    let (content, encoding) = match String::from_utf8(bytes) {
        Ok(content) => (content, "utf-8".to_string()),
        Err(error) => (
            base64::engine::general_purpose::STANDARD.encode(error.into_bytes()),
            "base64".to_string(),
        ),
    };
    Ok(WorkspaceFileContentDTO {
        path,
        content,
        encoding,
        size,
        hash,
        source: snapshot.source.as_str().to_string(),
        repo_oid: Some(repo_oid),
        snapshot_at: Some(snapshot.snapshot_at.to_rfc3339()),
        vm_id: None,
    })
}

/// Read `path` from the running incarnation `handle` on behalf of
/// `actor_user_id` (its owner). `path` is already sanitized. This is the
/// live branch of [`read_workspace_file_for_actor`], usable by a caller that
/// resolved the owner's handle itself (the review accept path, whether the
/// owner or the owner's agent through the internal API decides).
pub(crate) async fn read_live_workspace_file(
    state: &Arc<AppState>,
    handle: &crate::services::workspace_runtime::WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    repo_id: Uuid,
    path: &str,
) -> Result<WorkspaceFileContentDTO, AppError> {
    let live = workspace_file_save::read_live_file(
        &state.workspace_runtime,
        handle,
        actor_user_id,
        repo_id,
        path,
    )
    .await?;
    Ok(WorkspaceFileContentDTO {
        path: live.path,
        content: live.content,
        encoding: live.encoding,
        size: live.size,
        hash: live.hash,
        source: "live".to_string(),
        repo_oid: None,
        snapshot_at: None,
        vm_id: Some(live.vm_id),
    })
}

async fn put_workspace_file(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspaceFilePathParams>,
    Query(query): Query<WorkspaceFileQuery>,
    Json(body): Json<WorkspaceFileWriteRequestDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let result = write_workspace_file_and_maybe_push(
        &state,
        &user,
        &auth,
        params.id,
        query.repo_id,
        &params.path,
        &body.content,
        &body.encoding,
        body.expected_hash.as_deref(),
        body.expected_vm_id,
        body.commit_and_push,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

/// The editor Save. With `commit_and_push` it is the Workspace owner's explicit
/// Save operation (`workspace_file_save::save_live_file`): the write, the
/// identical-bytes retry after a failed publication, and the selected-file
/// checkpoint. A landed write whose publication failed is reported as that
/// publication error; the editor retries with the same bytes.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_workspace_file_and_maybe_push(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
    expected_vm_id: Option<Uuid>,
    commit_and_push: bool,
) -> Result<serde_json::Value, AppError> {
    let db = workspace_file_write_db(state, auth, workspace_id).await?;
    write_workspace_file_and_maybe_push_as(
        state,
        db,
        user.user_uuid(),
        workspace_id,
        repo_id,
        path,
        content,
        encoding,
        expected_hash,
        expected_vm_id,
        commit_and_push,
    )
    .await
}

/// The body of [`write_workspace_file_and_maybe_push`] on a write scope the
/// caller opened, for `actor`: the person's editor, or the workspace agent
/// as the owner (`g5.security.agent-workspace-scope`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_workspace_file_and_maybe_push_as(
    state: &Arc<AppState>,
    db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
    expected_vm_id: Option<Uuid>,
    commit_and_push: bool,
) -> Result<serde_json::Value, AppError> {
    if !commit_and_push {
        return write_workspace_file_as(
            state,
            db,
            actor,
            workspace_id,
            repo_id,
            path,
            content,
            encoding,
            expected_hash,
            expected_vm_id,
        )
        .await;
    }
    let Some(expected_vm_id) = expected_vm_id else {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"field":"expectedVmId","message":"reload the live file before saving"}),
        }));
    };
    let path = crate::routes::files::sanitize_path(path)?;
    validate_file_encoding(encoding)?;
    let scope = db.reopener();
    authorize_workspace_file_mutation(db, workspace_id, repo_id).await?;
    crate::services::question_set_service::QuestionSetService::check_file_write(
        &path, content, encoding,
    )?;
    match workspace_file_save::save_live_file(
        &state.workspace_runtime,
        &scope,
        workspace_id,
        actor,
        repo_id,
        &path,
        content,
        encoding,
        expected_hash,
        expected_vm_id,
        &format!("Save {path}"),
    )
    .await?
    {
        workspace_file_save::SaveOutcome::Published { saved } => Ok(saved),
        workspace_file_save::SaveOutcome::Applied { error, .. } => Err(error),
    }
}

/// Commit only these files and publish their member branches
/// (`workspace_file_save::checkpoint_selected_files`).
pub(crate) async fn commit_and_push_files(
    state: &Arc<AppState>,
    workspace_id: Uuid,
    owner_id: Uuid,
    files: &[(Uuid, String, String)],
    message: &str,
) -> Result<(), AppError> {
    let files = files
        .iter()
        .map(|(repo_id, path, hash)| {
            Ok(workspace_file_save::SelectedFile {
                repo_id: *repo_id,
                path: crate::routes::files::sanitize_path(path)?,
                blob_hash: hash.clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    workspace_file_save::checkpoint_selected_files(
        &state.workspace_runtime,
        workspace_id,
        owner_id,
        &files,
        message,
    )
    .await
}

fn validate_file_encoding(encoding: &str) -> Result<(), AppError> {
    if matches!(encoding, "utf-8" | "base64") {
        return Ok(());
    }
    Err(AppError::from_error(G5Error::ValidationError {
        details: serde_json::json!({"field":"encoding", "message":"encoding must be utf-8 or base64"}),
    }))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_workspace_file_for_actor(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
    expected_vm_id: Option<Uuid>,
) -> Result<serde_json::Value, AppError> {
    let db = workspace_file_write_db(state, auth, workspace_id).await?;
    write_workspace_file_as(
        state,
        db,
        user.user_uuid(),
        workspace_id,
        repo_id,
        raw_path,
        content,
        encoding,
        expected_hash,
        expected_vm_id,
    )
    .await
}

/// The body of [`write_workspace_file_for_actor`] on a write scope the
/// caller opened, for `actor`.
#[allow(clippy::too_many_arguments)]
async fn write_workspace_file_as(
    state: &Arc<AppState>,
    db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
    expected_vm_id: Option<Uuid>,
) -> Result<serde_json::Value, AppError> {
    let path = crate::routes::files::sanitize_path(raw_path)?;
    validate_file_encoding(encoding)?;
    let scope = db.reopener();
    authorize_workspace_file_mutation(db, workspace_id, repo_id).await?;
    let handle = workspace_file_save::expected_owner_handle(
        &state.workspace_runtime,
        workspace_id,
        actor,
        expected_vm_id,
        "the workspace changed after this file was opened; reload before saving",
    )
    .await?;
    write_live_workspace_file(
        state,
        &scope,
        &handle,
        actor,
        repo_id,
        &path,
        content,
        encoding,
        expected_hash,
    )
    .await
}

/// Write `path` into the running incarnation `handle` on behalf of
/// `actor_user_id` (its owner), with the branch-association guard and the
/// editor's conflict contract (`workspace_file_save::write_live_file`) and
/// the question set schema check.
/// `path` is already sanitized and the caller has authorized the mutation
/// ([`ensure_workspace_file_mutation_allowed`]).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_live_workspace_file(
    state: &Arc<AppState>,
    scope: &(impl g5_api_core::services::git::repo_store::OrgScope + ?Sized),
    handle: &crate::services::workspace_runtime::WorkspaceRuntimeHandle,
    actor_user_id: Uuid,
    repo_id: Uuid,
    path: &str,
    content: &str,
    encoding: &str,
    expected_hash: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    // Every owner and review write of a set file passes the question
    // schema check (`g5.documents.question.writer`).
    crate::services::question_set_service::QuestionSetService::check_file_write(
        path, content, encoding,
    )?;
    workspace_file_save::write_live_file(
        &state.workspace_runtime,
        scope,
        handle,
        actor_user_id,
        repo_id,
        path,
        content,
        encoding,
        expected_hash,
    )
    .await
}

async fn delete_workspace_file(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspaceFilePathParams>,
    Query(query): Query<WorkspaceFileQuery>,
    Json(body): Json<WorkspaceFileDeleteRequestDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let result = delete_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        params.id,
        query.repo_id,
        &params.path,
        body.expected_hash.as_deref(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

pub(crate) async fn delete_workspace_file_for_actor(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
    expected_hash: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let db = workspace_file_write_db(state, auth, workspace_id).await?;
    delete_workspace_file_as(
        state,
        db,
        user.user_uuid(),
        workspace_id,
        repo_id,
        raw_path,
        expected_hash,
    )
    .await
}

/// The body of [`delete_workspace_file_for_actor`] on a write scope the
/// caller opened, for `actor`.
pub(crate) async fn delete_workspace_file_as(
    state: &Arc<AppState>,
    db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_path: &str,
    expected_hash: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let path = crate::routes::files::sanitize_path(raw_path)?;
    let scope = db.reopener();
    authorize_workspace_file_mutation(db, workspace_id, repo_id).await?;
    let handle = state
        .workspace_runtime
        .owner_handle(workspace_id, actor)
        .await?;
    state
        .workspace_runtime
        .workspace_changes(&scope, workspace_id, actor)
        .await?;
    let request = workspace_runtime_request(&handle, actor, "delete");
    state
        .workspace_runtime
        .file_delete(&handle, &request, repo_id, &path, expected_hash)
        .await
        .map_err(map_workspace_file_runtime_error)
}

async fn rename_workspace_file(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<WorkspaceFileRenameRequestDTO>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let result = rename_workspace_file_for_actor(
        &state,
        &user,
        &auth,
        params.id,
        body.repo_id,
        &body.from,
        &body.to,
        body.expected_hash.as_deref(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(result)))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn rename_workspace_file_for_actor(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_from: &str,
    raw_to: &str,
    expected_hash: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let db = workspace_file_write_db(state, auth, workspace_id).await?;
    rename_workspace_file_as(
        state,
        db,
        user.user_uuid(),
        workspace_id,
        repo_id,
        raw_from,
        raw_to,
        expected_hash,
    )
    .await
}

/// The body of [`rename_workspace_file_for_actor`] on a write scope the
/// caller opened, for `actor`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn rename_workspace_file_as(
    state: &Arc<AppState>,
    db: g5_authz::ScopedDb,
    actor: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    raw_from: &str,
    raw_to: &str,
    expected_hash: Option<&str>,
) -> Result<serde_json::Value, AppError> {
    let from = crate::routes::files::sanitize_path(raw_from)?;
    let to = crate::routes::files::sanitize_path(raw_to)?;
    let scope = db.reopener();
    authorize_workspace_file_mutation(db, workspace_id, repo_id).await?;
    let handle = state
        .workspace_runtime
        .owner_handle(workspace_id, actor)
        .await?;
    if g5_contracts::types::question_set::is_question_set_path(&to) {
        // A rename lands bytes at a set path like a write does: the same
        // schema check runs on the file being moved there.
        let moving = workspace_file_save::read_live_file(
            &state.workspace_runtime,
            &handle,
            actor,
            repo_id,
            &from,
        )
        .await?;
        crate::services::question_set_service::QuestionSetService::check_file_write(
            &to,
            &moving.content,
            &moving.encoding,
        )?;
    }
    state
        .workspace_runtime
        .workspace_changes(&scope, workspace_id, actor)
        .await?;
    let request = workspace_runtime_request(&handle, actor, "rename");
    state
        .workspace_runtime
        .file_rename(&handle, &request, repo_id, &from, &to, expected_hash)
        .await
        .map_err(map_workspace_file_runtime_error)
}

/// Start a forked workspace for the chat its fork queued. The Start intent is
/// bound to that chat, so a replayed fork never starts the workspace again
/// after it was stopped (`g5.lifecycle.start`).
async fn start_forked_workspace(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: &str,
    user_id: Uuid,
    started_session_id: Uuid,
) -> Result<ResumeOutcome, AppError> {
    let workspace_id = Uuid::parse_str(workspace_id).map_err(|_| {
        AppError::from_error(G5Error::Internal {
            message: "forked workspace id is not a UUID".to_string(),
        })
    })?;
    let scope = authorize_workspace_action(state, auth, workspace_id).await?;
    Ok(workspace_vm_service(state, scope)?
        .launch_forked_workspace(workspace_id, user_id, started_session_id)
        .await?)
}

async fn resume_workspace_vm(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    body: Option<Json<WorkspaceLifecycleRequestDTO>>,
) -> Result<Json<ResponseEnvelope<ResumeOutcome>>, AppError> {
    let scope = authorize_workspace_action(&state, &auth, params.id).await?;
    let operation_id = body
        .and_then(|Json(body)| body.operation_id)
        .unwrap_or_else(Uuid::new_v4);
    let outcome = workspace_vm_service(&state, scope)?
        .resume(params.id, user.user_uuid(), operation_id, StartKind::Resume)
        .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

async fn stop_workspace_vm(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    body: Option<Json<WorkspaceLifecycleRequestDTO>>,
) -> Result<Json<ResponseEnvelope<StopOutcome>>, AppError> {
    let scope = authorize_workspace_action(&state, &auth, params.id).await?;
    let operation_id = body
        .and_then(|Json(body)| body.operation_id)
        .unwrap_or_else(Uuid::new_v4);
    let outcome = workspace_vm_service(&state, scope)?
        .stop(
            params.id,
            user.user_uuid(),
            operation_id,
            StopReason::User,
            None,
        )
        .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

async fn transfer_workspace(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<WorkspaceTransferRequestDTO>,
) -> Result<Json<ResponseEnvelope<TransferOutcome>>, AppError> {
    // Read access is enough to reach the service because an org administrator
    // may transfer a workspace they do not own. The service locks and rechecks
    // the current owner, actor role and target eligibility before admission.
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut db, org_id, params.id, "Transfer",
        )
        .await?;
    }
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    let outcome = workspace_vm_service(&state, scope)?
        .transfer(
            params.id,
            user.user_uuid(),
            body.to_user_id,
            &body.idempotency_key,
        )
        .await?;
    if outcome.status == "completed" {
        state.event_bus.publish(
            &crate::ws::Channel::Workspace(params.id),
            &crate::ws::WsEvent::WorkspaceOwnershipTransferred {
                workspace_id: params.id,
                from_user_id: outcome.from_user_id,
                to_user_id: outcome.to_user_id,
                transfer_id: outcome.transfer_id,
            },
        );
    }
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

async fn delete_workspace_action(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    body: Option<Json<WorkspaceLifecycleRequestDTO>>,
) -> Result<StatusCode, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    let scope = db.reopener();
    // Refuse before stopping: a personal workspace is never owner-deleted
    // (g5.personal.refusals), so its VM must not be stopped on the way.
    let refused =
        crate::services::workspace_kind::refuse_personal(db.conn(), params.id, "owner deletion")
            .await;
    db.rollback().await.map_err(AppError::from)?;
    refused?;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let operation_id = body.operation_id.unwrap_or_else(Uuid::new_v4);
    let lifecycle = workspace_vm_service(&state, scope.clone())?;
    state
        .workspace_service
        .delete_workspace_with_stop(
            scope,
            &lifecycle,
            params.id,
            user.user_uuid(),
            operation_id,
            body.force,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/workspaces/:id/stats`
///
/// Returns computed workspace statistics: cost, task counts, per-branch divergence.
async fn get_workspace_stats(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<WorkspaceStatsDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: get_workspace_stats");

    let stats = state
        .workspace_service
        .get_workspace_stats(&mut db, params.id)
        .await?;
    let dto: WorkspaceStatsDTO = stats.into();

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// Query parameters for `GET /api/v1/workspaces/:id/provider-quota`.
///
/// The composer usage chip only ever asks for the Claude Code subscription
/// (Anthropic OAuth) key, but the parameters are explicit so the endpoint
/// mirrors the `(provider, auth_mode, account_id)` shape the snapshot table
/// is keyed by (system_invariants/external_agent_provider_contract D13).
/// Defaults match the chip's single purpose so a bare call still resolves the
/// subscription key; an absent `accountId` selects the account-less (nil)
/// row (routes/workspaces D36).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderQuotaQueryParams {
    #[serde(default = "default_quota_provider")]
    provider: String,
    #[serde(default = "default_quota_auth_mode")]
    auth_mode: String,
    /// Frozen provider account id whose row to read; `None` reads the
    /// nil-account row. A key component only — it authorizes nothing.
    #[serde(default)]
    account_id: Option<Uuid>,
}

fn default_quota_provider() -> String {
    PROVIDER_QUOTA_PROVIDER_CLAUDE_CODE.to_string()
}

fn default_quota_auth_mode() -> String {
    PROVIDER_QUOTA_AUTH_MODE_ANTHROPIC_OAUTH.to_string()
}

/// `GET /api/v1/workspaces/:id/provider-quota`
///
/// Returns the display-only subscription-usage snapshot for the workspace's
/// `(provider, auth_mode, account_id)` key — the nil account row unless
/// `accountId` is given (routes/workspaces D36) — or `null` when no snapshot
/// has been captured for exactly that key, never a fallback to another one
/// (agent_interaction_ux D8; provider quota snapshot D13). Backs the composer
/// usage chip's PRE-session render, where no chat session is bound yet and the
/// per-session enrich path (`enrich_session_subscription_usage`) has no session
/// to read from.
///
/// Authorization is workspace-scoped and member-readable
/// (security_policy/role_based_access_control D10): `scoped_db_for_workspace`
/// projects a cross-org or invisible workspace to the tenant 404 and admits any
/// same-workspace member with 200. The snapshot is provider-owned budget
/// evidence for display only — it never gates dispatch and carries no
/// credential material.
async fn get_workspace_provider_quota(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<ProviderQuotaQueryParams>,
) -> Result<Json<ResponseEnvelope<Option<SubscriptionUsageDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: get_workspace_provider_quota");

    let org_id = resolve_org_id_for_workspace(&mut db, params.id).await?;
    let snapshot = state
        .provider_quota_service
        .fetch_snapshot(
            &mut db,
            org_id,
            params.id,
            &query.provider,
            &query.auth_mode,
            query.account_id.unwrap_or_else(Uuid::nil),
        )
        .await?;
    let dto = snapshot.map(SubscriptionUsageDTO::from);

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/workspaces/:id/branches`
///
/// Lists all branches for a workspace with per-branch details.
async fn list_branches(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<Vec<WorkspaceBranchDetailDTO>>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: list_branches");

    let branches = state
        .workspace_service
        .list_branches(&mut db, params.id)
        .await?;
    let dtos: Vec<WorkspaceBranchDetailDTO> = branches
        .into_iter()
        .map(WorkspaceBranchDetailDTO::from)
        .collect();

    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(dtos)))
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/remote_tracking`
///
/// Promotes an existing clean/materialized branch repo child to remote tracking
/// using the persisted `repos.git_url`. The request only supplies the remote
/// branch name; changing remote URLs remains create/add-time behavior.
async fn promote_branch_repo_remote_tracking(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
    Json(req): Json<PromoteRemoteTrackingRequestDTO>,
) -> Result<Json<ResponseEnvelope<PromoteRemoteTrackingResponseDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    // The workspace owner, or an administrator of Base's organization on Base
    // (`g5.workspace.remote.base-admin-pull`). On an ordinary workspace this
    // admits the owner alone, so a non-owner administrator is already blocked;
    // the service then requires that owner to also be an organization
    // administrator, so Make Remote needs owner-and-administrator there and
    // administrator on Base. The administrator-only rule is enforced before any
    // provider I/O (`g5.git.make-remote`, `g5.git.remote-binding`). Publish of an
    // inherited remote is the separate `publish_branch_repo` operation and stays
    // the owner's.
    let mut db = scoped_db_for_owner_or_base_admin(
        &state,
        &auth,
        params.id,
        BaseAdminAction::RemoteTracking,
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        params.id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::DirectWorkspaceWrite,
        "workspaces.promote_remote_tracking",
    )
    .await?;
    {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut db,
            org_id,
            params.id,
            "Publish branch to a remote",
        )
        .await?;
    }
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    tracing::debug!(
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        "Workspace endpoint called: promote_branch_repo_remote_tracking"
    );

    let result = state
        .workspace_service
        .promote_branch_repo_to_remote_tracking(
            db,
            params.id,
            params.repo_id,
            &req.remote_branch,
            req.git_url.as_deref(),
            state.allow_local_git_remotes,
            req.create_provider_branch,
            user.user_uuid(),
            &state.event_bus,
        )
        .await?;
    tracing::info!(
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        remote_branch = %req.remote_branch,
        "Workspace endpoint succeeded: promote_branch_repo_remote_tracking"
    );
    Ok(Json(ResponseEnvelope::ok(
        PromoteRemoteTrackingResponseDTO::from(result),
    )))
}

/// `POST /api/v1/workspaces/:id/sync`
///
/// Queues a remote pull for every remote-tracked repository, Base included.
/// This is a GitOperation, so the read-only content guard does not block it;
/// the route's normal `workspaces:write` authorization still applies.
async fn sync_workspace(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<SyncResultDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_owner_or_base_admin(
        &state,
        &auth,
        params.id,
        BaseAdminAction::RemoteTracking,
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        params.id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::GitOperation,
        "workspaces.sync",
    )
    .await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: sync_workspace");

    let (db, result) = state
        .workspace_service
        .sync_workspace(
            db,
            params.id,
            crate::models::branch_sync_run::trigger_kind::MANUAL,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(workspace_id = %params.id, "Workspace endpoint succeeded: sync_workspace");

    Ok(Json(ResponseEnvelope::ok(result.into())))
}

/// `POST /api/v1/workspaces/:id/push_to_remote`
///
/// The explicit PushToRemote action (design §6.2 Trigger C, arch D67). Enqueues
/// a `trigger_kind = 'push'` integration cycle for every unpaused remote-tracked
/// branch in the workspace; the worker runs each through `classify_and_act`
/// (push if workspace-ahead, merge run if diverged, `noop` if already synced).
/// This is the only publication path in `manual` mode, and is always available.
async fn push_to_remote(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<PushToRemoteResponseDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: push_to_remote");

    let result = state
        .workspace_service
        .push_to_remote(&mut db, params.id)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %params.id,
        enqueued = result.enqueued,
        "Workspace endpoint succeeded: push_to_remote"
    );
    Ok(Json(ResponseEnvelope::ok(PushToRemoteResponseDTO::from(
        result,
    ))))
}

/// `POST /api/v1/workspaces/:id/remote_tracking/approved_rebase_publish`
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApprovedRebasePublishRequest {
    expected_merge_run_id: Option<Uuid>,
    expected_merge_run_ids: Option<Vec<Uuid>>,
}

async fn approved_rebase_publish(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    body: Option<Json<ApprovedRebasePublishRequest>>,
) -> Result<Json<ResponseEnvelope<PushToRemoteResponseDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    tracing::debug!(
        workspace_id = %params.id,
        "Workspace endpoint called: approved_rebase_publish"
    );

    let expected_merge_run_ids = body.and_then(|Json(body)| {
        let mut ids = body.expected_merge_run_ids.unwrap_or_default();
        if let Some(id) = body.expected_merge_run_id {
            ids.push(id);
        }
        (!ids.is_empty()).then_some(ids)
    });
    let (db, result) = state
        .workspace_service
        .approved_rebase_publish(db, params.id, user.user_uuid(), expected_merge_run_ids)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %params.id,
        enqueued = result.enqueued,
        "Workspace endpoint succeeded: approved_rebase_publish"
    );
    Ok(Json(ResponseEnvelope::ok(PushToRemoteResponseDTO::from(
        result,
    ))))
}

/// Request body for `PUT /api/v1/workspaces/:id/push_mode`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetPushModeRequest {
    push_mode: String,
}

/// `PUT /api/v1/workspaces/:id/push_mode`
///
/// Sets the per-workspace `push_mode` (`'manual'` | `'auto'`, arch D66). In
/// `manual` (default) nothing publishes implicitly; in `auto` commit-coupled
/// advances and the background loop publish workspace-ahead commits. Switching
/// to `auto` does not itself push — the next cycle publishes.
async fn set_workspace_push_mode(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<SetPushModeRequest>,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    if body.push_mode != "manual" && body.push_mode != "auto" {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: format!(
                "Invalid push_mode '{}'. Must be 'manual' or 'auto'.",
                body.push_mode
            )
            .into(),
        }));
    }
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    // Base never publishes to its provider, so its push mode stays manual
    // (`g5.workspace.remote.base-parity`).
    let is_base: bool = sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id = $1")
        .bind(params.id)
        .fetch_one(db.conn())
        .await
        .map_err(AppError::from)?;
    if is_base && body.push_mode == "auto" {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: "Base never pushes to its provider; its push mode stays manual.".into(),
        }));
    }
    tracing::debug!(
        workspace_id = %params.id,
        push_mode = %body.push_mode,
        "Workspace endpoint called: set_workspace_push_mode"
    );

    sqlx::query("UPDATE workspaces SET push_mode = $2, updated_at = now() WHERE id = $1")
        .bind(params.id)
        .bind(&body.push_mode)
        .execute(db.conn())
        .await
        .map_err(AppError::from)?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(
        serde_json::json!({ "pushMode": body.push_mode }),
    )))
}

/// `GET /api/v1/workspaces/:id/fork_advisories`
///
/// Non-blocking advisories about the workspace's remote-tracked members that a
/// fork would capture: held by the merged-PR hold, paused, or behind their
/// provider (`g5.fork.source.advisory`). Forking is never refused for them.
async fn get_fork_advisories(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspacePathParams>,
) -> Result<
    Json<ResponseEnvelope<Vec<crate::services::remote_integrations::ForkSourceAdvisory>>>,
    AppError,
> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let repos = state.repo_storage.scoped(&db);
    let advisories =
        crate::services::remote_integrations::fork_source_advisories(db.conn(), &repos, params.id)
            .await
            .map_err(AppError::from)?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(advisories)))
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/provider_history_preview`
///
/// Show the provider tip a repository would adopt and how many of its
/// commits would be discarded (`g5.workspace.remote.base-provider-rewrite`,
/// `g5.workspace.remote.adopt-provider-history`). Fetches provider objects
/// into the mirror but moves no ref.
async fn preview_provider_history_acceptance(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
) -> Result<Json<ResponseEnvelope<workspace_service::ProviderHistoryPreview>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_owner_or_base_admin(
        &state,
        &auth,
        params.id,
        BaseAdminAction::RemoteTracking,
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        params.id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::GitOperation,
        "workspaces.preview_provider_history_acceptance",
    )
    .await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    let preview = state
        .workspace_service
        .preview_provider_history_acceptance(db, params.id, params.repo_id, user.user_uuid())
        .await?;
    Ok(Json(ResponseEnvelope::ok(preview)))
}

/// Request body for
/// `POST /api/v1/workspaces/:id/repos/:repoId/accept_provider_history`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptProviderHistoryRequest {
    expected_provider_tip: String,
    expected_head: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptProviderHistoryResponse {
    workspace_id: Uuid,
    repo_id: Uuid,
    head: String,
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/accept_provider_history`
///
/// Adopt the reviewed provider history for one repository, discarding its
/// local commits there and pushing nothing to the provider. Base lands it
/// without a VM for its owner or an organization administrator
/// (`g5.workspace.remote.base-provider-rewrite`); any other workspace resets
/// in its owner's running VM (`g5.workspace.remote.adopt-provider-history`).
async fn accept_provider_history(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
    Json(body): Json<AcceptProviderHistoryRequest>,
) -> Result<Json<ResponseEnvelope<AcceptProviderHistoryResponse>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_owner_or_base_admin(
        &state,
        &auth,
        params.id,
        BaseAdminAction::RemoteTracking,
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        params.id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::GitOperation,
        "workspaces.accept_provider_history",
    )
    .await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    let is_base = state
        .workspace_service
        .workspace_is_base(&mut db, params.id)
        .await?;
    if !is_base {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        let scope = db.reopener();
        db.rollback().await.map_err(AppError::from)?;
        crate::services::remote_tracking_merge::adopt_remote_tracking_provider_history(
            &state,
            &scope,
            g5_contracts::types::WorkspaceRepoKey {
                org_id,
                workspace_id: params.id,
                repo_id: params.repo_id,
            },
            user.user_uuid(),
            &body.expected_provider_tip,
            &body.expected_head,
        )
        .await?;
        return Ok(Json(ResponseEnvelope::ok(AcceptProviderHistoryResponse {
            workspace_id: params.id,
            repo_id: params.repo_id,
            head: body.expected_provider_tip,
        })));
    }
    let (db, publication) = state
        .workspace_service
        .accept_provider_history(
            db,
            params.id,
            params.repo_id,
            &body.expected_provider_tip,
            &body.expected_head,
            user.user_uuid(),
        )
        .await?;
    match publication
        .land_interactive_bounded(db, &state.event_bus, &state.repo_storage)
        .await
    {
        Ok(landed) => {
            crate::routes::workspace_reviews::notify_reviewers_after_landing(
                &state,
                &landed,
                user.user_uuid(),
            )
            .await;
        }
        Err(crate::services::branch_commit::BranchCommitError::FenceBusy { .. }) => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "WORKSPACE_COMMIT_FENCE_BUSY",
                    "operation": "accept_provider_history",
                    "message": "Another operation is committing this workspace; retry shortly.",
                }),
            }));
        }
        Err(error) => {
            tracing::warn!(
                workspace_id = %params.id,
                repo_id = %params.repo_id,
                %error,
                "Accepting provider history rolled back"
            );
            return Err(AppError::from_error(G5Error::Conflict {
                reason: format!(
                    "Base or its provider history changed while accepting; review again ({error})"
                ),
            }));
        }
    }
    tracing::info!(
        target: "g5_audit",
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        actor_user_id = %user.user_uuid(),
        provider_tip = %body.expected_provider_tip,
        replaced_head = %body.expected_head,
        "Base accepted provider history"
    );
    Ok(Json(ResponseEnvelope::ok(AcceptProviderHistoryResponse {
        workspace_id: params.id,
        repo_id: params.repo_id,
        head: body.expected_provider_tip,
    })))
}

/// Request body for
/// `PUT /api/v1/workspaces/:id/repos/:repoId/auto_pull`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetBranchRepoAutoPullRequestDTO {
    enabled: bool,
}

/// Response body for the per-child auto-pull toggle. Echoes the authoritative
/// value and branch/repo identity so the client can round-trip without a full
/// workspace refetch.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SetBranchRepoAutoPullResponseDTO {
    workspace_id: Uuid,
    repo_id: Uuid,
    auto_pull_enabled: bool,
}

/// `PUT /api/v1/workspaces/:id/repos/:repoId/auto_pull`
///
/// Option 1 (G5 #1198): toggle passive background auto-pull for one
/// remote-tracking repository branch child. Available to workspace editors
/// (`workspaces:write`). Idempotent; rejects local/pinned children. Base uses
/// it like any workspace. This is a GitOperation, so read-only workspaces may configure it;
/// the normal scope and branch/repository checks still apply. Ontology
/// `software_architecture/g5-api/routes/workspaces` I30.
async fn set_branch_repo_auto_pull(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
    Json(body): Json<SetBranchRepoAutoPullRequestDTO>,
) -> Result<Json<ResponseEnvelope<SetBranchRepoAutoPullResponseDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_owner_or_base_admin(
        &state,
        &auth,
        params.id,
        BaseAdminAction::RemoteTracking,
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_allows_mutation(
        &mut db,
        params.id,
        crate::services::workspace_mutation_policy::WorkspaceMutationKind::GitOperation,
        "workspaces.set_branch_repo_auto_pull",
    )
    .await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    tracing::debug!(
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        enabled = body.enabled,
        "Workspace endpoint called: set_branch_repo_auto_pull"
    );

    let auto_pull_enabled = state
        .workspace_service
        .set_branch_repo_auto_pull(
            &mut db,
            params.id,
            params.repo_id,
            body.enabled,
            user.user_uuid(),
        )
        .await?;

    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        enabled = auto_pull_enabled,
        "Workspace endpoint succeeded: set_branch_repo_auto_pull"
    );
    Ok(Json(ResponseEnvelope::ok(
        SetBranchRepoAutoPullResponseDTO {
            workspace_id: params.id,
            repo_id: params.repo_id,
            auto_pull_enabled,
        },
    )))
}

/// Response body for the explicit Publish of a local-only branch child.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishBranchRepoResponseDTO {
    workspace_id: Uuid,
    repo_id: Uuid,
    remote_branch: String,
    head_commit: String,
}

impl From<workspace_service::PublishBranchRepoResult> for PublishBranchRepoResponseDTO {
    fn from(r: workspace_service::PublishBranchRepoResult) -> Self {
        Self {
            workspace_id: r.workspace_id,
            repo_id: r.repo_id,
            remote_branch: r.remote_branch,
            head_commit: r.head_commit,
        }
    }
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/publish`
///
/// Option 2 (G5 #1198): explicitly publishes a local-only/pinned branch child to
/// a new remote ref (create-only, confirmed), then transitions it to
/// remote-tracking. Available to workspace editors (`workspaces:write`). Ontology
/// `software_architecture/g5-api/services/workspace_service` D43.
async fn publish_branch_repo(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
) -> Result<Json<ResponseEnvelope<PublishBranchRepoResponseDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.publish_branch_repo",
    )
    .await?;
    {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut db,
            org_id,
            params.id,
            "Publish branch",
        )
        .await?;
    }
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    tracing::debug!(
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        "Workspace endpoint called: publish_branch_repo"
    );

    let result = state
        .workspace_service
        .publish_branch_repo(&mut db, params.id, params.repo_id, user.user_uuid())
        .await?;

    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        workspace_id = %params.id,
        repo_id = %params.repo_id,
        remote_branch = %result.remote_branch,
        "Workspace endpoint succeeded: publish_branch_repo"
    );
    Ok(Json(ResponseEnvelope::ok(
        PublishBranchRepoResponseDTO::from(result),
    )))
}

/// `POST /api/v1/workspaces/:id/merge`
///
/// Merges workspace branches into their base branches.
async fn merge_workspace(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<Json<ResponseEnvelope<MergeResultDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut db, org_id, params.id, "Merge",
        )
        .await?;
    }
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: merge_workspace");

    // User callers are re-authorized against an explicit merge target inside
    // the service (workspace_service merge row); non-user actors pass None and
    // skip the audience predicate like enforce_workspace_access_for_user.
    let caller_user_id = if auth.is_user() {
        Some(auth.require_user_id().map_err(AppError::from)?)
    } else {
        None
    };
    let (db, mut result) = state
        .workspace_service
        .merge_workspace(db, params.id, caller_user_id, &state.event_bus)
        .await?;
    let authority_advance = result.authority_advance.take();
    let dto: MergeResultDTO = result.into();

    tracing::info!(workspace_id = %params.id, "Workspace endpoint succeeded: merge_workspace");
    if let Some(advance) = authority_advance {
        let actor_user_id = db.actor_id().into_uuid();
        let finalized = advance
            .land(db, &state.event_bus, &state.repo_storage)
            .await
            .map_err(AppError::from)?;
        crate::routes::workspace_reviews::notify_reviewers_after_landing(
            &state,
            &finalized,
            actor_user_id,
        )
        .await;
        for warning in finalized.projection_warnings {
            tracing::warn!(
                advance_id = %finalized.advance_id,
                %warning,
                "Workspace merge committed while an authority projection remains pending"
            );
        }
    } else {
        db.commit().await.map_err(AppError::from)?;
    }

    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/workspaces/:id/rebase-merge/change-graph`
///
/// Projects read-only source and target changes from the source workspace's
/// exact divergence vector. Merge execution still performs its own preflight.
async fn get_rebase_change_graph(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<RebaseChangeGraphQueryDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::RebaseChangeGraphResponse>>, AppError> {
    let source_access_db =
        scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    source_access_db.rollback().await.map_err(AppError::from)?;
    let target_access_db = scoped_db_for_workspace(
        &state,
        &auth,
        query.target_workspace_id,
        &["workspaces:read"],
    )
    .await
    .map_err(mask_rebase_graph_target_access_error)?;
    target_access_db.rollback().await.map_err(AppError::from)?;

    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let graph = state
        .source_control_change_graph_service
        .project_rebase_merge(&mut db, params.id, query.target_workspace_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(graph)))
}

/// `GET /api/v1/workspaces/:id/rebase-merge/change-graph/nodes/:artifactKey/details`
async fn get_rebase_change_graph_node_details(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<RebaseChangeGraphPathParams>,
    Query(query): Query<RebaseChangeGraphDetailsQueryDTO>,
) -> Result<Json<ResponseEnvelope<crate::services::RebaseChangeGraphNodeDetailsResponse>>, AppError>
{
    let source_access_db =
        scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    source_access_db.rollback().await.map_err(AppError::from)?;
    let target_access_db = scoped_db_for_workspace(
        &state,
        &auth,
        query.target_workspace_id,
        &["workspaces:read"],
    )
    .await
    .map_err(mask_rebase_graph_target_access_error)?;
    target_access_db.rollback().await.map_err(AppError::from)?;

    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let details = state
        .source_control_change_graph_service
        .rebase_merge_node_details(
            &mut db,
            params.id,
            query.target_workspace_id,
            &params.artifact_key,
            &query.revision,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(details)))
}

fn mask_rebase_graph_target_access_error(error: AppError) -> AppError {
    if matches!(
        &error.inner,
        G5Error::NotFound { .. } | G5Error::Forbidden { .. }
    ) {
        return AppError::from_error(G5Error::NotFound {
            resource: "workspace".to_string(),
        });
    }
    error
}

/// `POST /api/v1/workspaces/:id/rebase-merge/preflight`
///
/// Non-mutating preflight for source workspace rebase / PR merge readiness.
async fn preflight_workspace_merge(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<StartWorkspaceMergeRequestDTO>,
) -> Result<Json<ResponseEnvelope<WorkspaceMergePreflightResponseDTO>>, AppError> {
    let target_access_db = scoped_db_for_workspace(
        &state,
        &auth,
        body.target_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    target_access_db.rollback().await.map_err(AppError::from)?;
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;

    tracing::debug!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        "Workspace endpoint called: preflight_workspace_merge"
    );

    let preflight = state
        .workspace_merge_service
        .prepare_transaction_free_workspace_merge_preflight(
            db,
            params.id,
            body.target_workspace_id,
            workspace_merge_service::WorkspaceMergePreflightMode::SourceRebaseExecution,
        )
        .await?
        .into_response();

    tracing::info!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        decision = ?preflight.decision,
        "Workspace endpoint succeeded: preflight_workspace_merge"
    );
    Ok(Json(ResponseEnvelope::ok(
        WorkspaceMergePreflightResponseDTO { preflight },
    )))
}

/// `POST /api/v1/workspaces/:id/rebase-merge/direct-clean`
///
/// Publishes a no-Git-conflict workspace merge after explicit user choice.
/// The existing `/rebase-merge` route remains the full workflow entrypoint.
async fn direct_clean_workspace_merge(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<DirectCleanWorkspaceMergeRequestDTO>,
) -> Result<Json<ResponseEnvelope<StartWorkspaceMergeResponseDTO>>, AppError> {
    {
        let mut source_db =
            scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
        let org_id = source_db
            .primary_org_id()
            .map_err(AppError::from)?
            .into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut source_db,
            org_id,
            params.id,
            "Merge",
        )
        .await?;
        source_db.rollback().await.map_err(AppError::from)?;
    }
    let mut target_access_db = scoped_db_for_workspace(
        &state,
        &auth,
        body.target_workspace_id,
        &["workspaces:write"],
    )
    .await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut target_access_db,
        body.target_workspace_id,
    )
    .await?;
    target_access_db.rollback().await.map_err(AppError::from)?;
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;

    tracing::debug!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        "Workspace endpoint called: direct_clean_workspace_merge"
    );

    // Source must be mutable: merging FROM a read-only source is blocked (it
    // writes merge-run state on the source). The target may be read-only — that
    // side is a GitOperation. Design workspace_mutation_policy [I6].
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.direct_clean_merge",
    )
    .await?;

    let prepared = state
        .workspace_merge_service
        .prepare_direct_clean_workspace_merge(
            db,
            params.id,
            user.user_uuid(),
            workspace_merge_service::DirectCleanWorkspaceMergeRequest {
                target_workspace_id: body.target_workspace_id,
                preflight_fingerprint: body.preflight_fingerprint,
            },
        )
        .await?;

    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.direct_clean_merge",
    )
    .await?;
    let completed = state
        .workspace_merge_service
        .apply_prepared_workspace_merge(&mut db, prepared)
        .await?;
    // Re-authorize the target at the application boundary. The owner check
    // above ran in a transaction that was rolled back before the Git work,
    // so a transfer of the target that completed during preparation would
    // otherwise let the former owner advance a workspace that is no longer
    // theirs (G5Labs/G5-Lean#463). The application just took the merge
    // fences and both workspaces' row locks in this transaction, so this
    // shared lock on the target row waits for nothing and is held, like the
    // clone-merge landing's, until the landing commits.
    crate::services::workspace_mutation_policy::ensure_workspace_owner(
        &mut db,
        body.target_workspace_id,
    )
    .await?;
    let run = completed.run;
    let authority_advance = completed.authority_advance;

    if let Some(advance) = authority_advance {
        // The application transaction already holds the workspace commit
        // fences, so the cut lands in it: the merge lifecycle, its settlement
        // and the refs commit together, and a refused landing or settlement
        // leaves no merge run behind to block the retry.
        let landed = advance
            .land(db, &state.event_bus, &state.repo_storage)
            .await
            .map_err(AppError::from)?;
        crate::routes::workspace_reviews::notify_reviewers_after_landing(
            &state,
            &landed,
            user.user_uuid(),
        )
        .await;
    } else {
        db.commit().await.map_err(AppError::from)?;
    }
    tracing::info!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        merge_run_id = %run.id,
        "Workspace endpoint succeeded: direct_clean_workspace_merge"
    );

    Ok(Json(ResponseEnvelope::ok(
        StartWorkspaceMergeResponseDTO::Direct { run: Box::new(run) },
    )))
}

async fn start_workspace_merge(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<StartWorkspaceMergeRequestDTO>,
) -> Result<Json<ResponseEnvelope<StartWorkspaceMergeResponseDTO>>, AppError> {
    // Phase 1 source-rebase semantics: the target is read-only context. This
    // route must not advance the target workspace.
    let target_access_db = scoped_db_for_workspace(
        &state,
        &auth,
        body.target_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    target_access_db.rollback().await.map_err(AppError::from)?;
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;

    tracing::debug!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        "Workspace endpoint called: start_workspace_merge"
    );
    {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut db,
            org_id,
            params.id,
            "Rebase & Merge",
        )
        .await?;
    }

    // Source must be mutable: rebasing/merging FROM a read-only source is
    // blocked. The target may be read-only (GitOperation side). Design
    // workspace_mutation_policy [I6].
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.merge_start",
    )
    .await?;

    if let Some(resolution) = state
        .workspace_merge_service
        .active_source_resolution(&mut db, params.id, body.target_workspace_id)
        .await?
    {
        db.commit().await?;
        return Ok(Json(ResponseEnvelope::ok(
            StartWorkspaceMergeResponseDTO::Resolution {
                resolution,
                start_compute: None,
            },
        )));
    }

    // A running owner VM executes every source rebase
    // (`g5.architecture.workspace_rebase`); only a stopped workspace whose
    // restore would adopt the new heads rebases centrally below. A Start of
    // the owner's under way admits nothing but a resolution bound to it
    // (`g5.git.stopped-rebase.resolve-with-agent`), so only Start and
    // resolve with agent reads that state instead of a refusal.
    let execution = if body.start_compute {
        crate::services::workspace_vm_service::source_rebase_state(db.conn(), params.id).await?
    } else {
        crate::services::workspace_vm_service::source_rebase_execution(db.conn(), params.id).await?
    };
    if execution == crate::services::workspace_vm_service::SourceRebaseExecution::Vm {
        let scope = db.reopener();
        db.rollback().await.map_err(AppError::from)?;
        let outcome = state
            .workspace_merge_service
            .rebase_source_in_vm(
                &state,
                &scope,
                params.id,
                body.target_workspace_id,
                user.user_uuid(),
                body.agent_launch.as_ref(),
            )
            .await?;
        return Ok(Json(ResponseEnvelope::ok(match outcome {
            workspace_merge_service::SourceRebaseResult::Rebased(rebase) => {
                tracing::info!(
                    source_workspace_id = %params.id,
                    target_workspace_id = %body.target_workspace_id,
                    vm_id = %rebase.vm_id,
                    "Workspace endpoint succeeded: start_workspace_merge (in VM)"
                );
                StartWorkspaceMergeResponseDTO::Rebased { rebase }
            }
            workspace_merge_service::SourceRebaseResult::Resolution(resolution) => {
                StartWorkspaceMergeResponseDTO::Resolution {
                    resolution,
                    start_compute: None,
                }
            }
        })));
    }

    let merge_request = workspace_merge_service::StartWorkspaceMergeRequest {
        target_workspace_id: body.target_workspace_id,
    };
    let scope = db.reopener();
    let prepared = state
        .workspace_merge_service
        .prepare_source_workspace_merge(db, params.id, user.user_uuid(), &merge_request)
        .await?;

    // Start and resolve with agent: only a conflict the agent can resolve
    // records or joins the owner's Start. A clean rebase lands centrally as
    // before, and Git the agent cannot fix (unreadable or stale inputs)
    // refuses here, before any VM starts.
    let prepared = match prepared {
        workspace_merge_service::PreparedSourceWorkspaceMerge::WorkflowRequired(prepared)
            if body.start_compute =>
        {
            if let Some(repo) = prepared.response().repo_results.iter().find(|repo| {
                repo.outcome == workspace_merge_service::MergePreflightRepoOutcome::WorkflowRequired
            }) {
                return Err(G5Error::Conflict {
                    reason: repo.message.clone().unwrap_or_else(|| {
                        "Workspace merge inputs are unavailable; refresh and retry".into()
                    }),
                }
                .into());
            }
            return start_and_resolve_with_agent(
                &state,
                &auth,
                &scope,
                params.id,
                body,
                *prepared,
                user.user_uuid(),
            )
            .await;
        }
        prepared => prepared,
    };

    let mut db = scope.open().await?;
    // Reopen the authorized scope without the route factory's ownership row
    // lock, then recheck ownership below the fence. Simultaneous starts cannot
    // deadlock while upgrading their ownership checks to task admission locks.
    crate::services::branch_commit::acquire_workspace_commit_fence(db.conn(), params.id).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        params.id,
        "workspaces.merge_start",
    )
    .await?;
    // Start takes this fence before capturing boot heads, so a workspace
    // still stopped and clean here restores the landed cut.
    if crate::services::workspace_vm_service::source_rebase_execution(db.conn(), params.id).await?
        != crate::services::workspace_vm_service::SourceRebaseExecution::Central
    {
        return Err(G5Error::Conflict {
            reason: "The workspace started during the rebase; retry so it runs in the workspace"
                .into(),
        }
        .into());
    }
    let completed = match prepared {
        workspace_merge_service::PreparedSourceWorkspaceMerge::Direct(prepared) => {
            state
                .workspace_merge_service
                .apply_prepared_workspace_merge(&mut db, *prepared)
                .await?
        }
        workspace_merge_service::PreparedSourceWorkspaceMerge::WorkflowRequired(prepared) => {
            let resolution = state
                .workspace_merge_service
                .enqueue_source_resolution(
                    &state,
                    &mut db,
                    *prepared,
                    user.user_uuid(),
                    body.agent_launch.as_ref(),
                )
                .await?;
            db.commit().await?;
            return Ok(Json(ResponseEnvelope::ok(
                StartWorkspaceMergeResponseDTO::Resolution {
                    resolution,
                    start_compute: None,
                },
            )));
        }
    };
    let run = completed.run;
    let authority_advance = completed.authority_advance;

    if let Some(advance) = authority_advance {
        // The application transaction already holds the workspace commit
        // fences, so the cut lands in it: the merge lifecycle, its settlement
        // and the refs commit together, and a refused landing or settlement
        // leaves no merge run behind to block the retry.
        let landed = advance
            .land(db, &state.event_bus, &state.repo_storage)
            .await
            .map_err(AppError::from)?;
        crate::routes::workspace_reviews::notify_reviewers_after_landing(
            &state,
            &landed,
            user.user_uuid(),
        )
        .await;
    } else {
        db.commit().await.map_err(AppError::from)?;
    }
    tracing::info!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        merge_run_id = %run.id,
        "Workspace endpoint succeeded: start_workspace_merge"
    );

    Ok(Json(ResponseEnvelope::ok(
        StartWorkspaceMergeResponseDTO::Direct { run: Box::new(run) },
    )))
}

/// Start and resolve with agent (`g5.git.stopped-rebase.resolve-with-agent`,
/// `g5.chat.start-and-send`): record the owner's Start consent for the source
/// workspace, or join the one under way, before any transaction (Resume locks
/// the workspace row); then queue the resolution brief under the workspace
/// commit fence, bound to that consent in the same transaction, so a Start
/// that ends before compute is ready withdraws it. A VM found running by then
/// takes the ordinary in-VM path. A request that fails after recording a
/// consent abandons it, as the chat routes do.
async fn start_and_resolve_with_agent(
    state: &Arc<AppState>,
    auth: &AuthContext,
    scope: &g5_authz::ScopedDbReopener,
    source_workspace_id: Uuid,
    body: StartWorkspaceMergeRequestDTO,
    prepared: workspace_merge_service::PreparedWorkspaceMergePreflight,
    user_id: Uuid,
) -> Result<Json<ResponseEnvelope<StartWorkspaceMergeResponseDTO>>, AppError> {
    let start = crate::routes::start_and_send::begin(state, auth, source_workspace_id).await?;
    let result = async {
        if !start.bind {
            // Attached: the VM runs, so the rebase runs there as for any
            // running workspace, conflicts included (`g5.git.live-rebase`).
            let outcome = state
                .workspace_merge_service
                .rebase_source_in_vm(
                    state,
                    scope,
                    source_workspace_id,
                    body.target_workspace_id,
                    user_id,
                    body.agent_launch.as_ref(),
                )
                .await?;
            return Ok(Json(ResponseEnvelope::ok(match outcome {
                workspace_merge_service::SourceRebaseResult::Rebased(rebase) => {
                    StartWorkspaceMergeResponseDTO::Rebased { rebase }
                }
                workspace_merge_service::SourceRebaseResult::Resolution(resolution) => {
                    StartWorkspaceMergeResponseDTO::Resolution {
                        resolution,
                        start_compute: Some((&start).into()),
                    }
                }
            })));
        }
        let mut db = scope.open().await?;
        crate::services::branch_commit::acquire_workspace_commit_fence(
            db.conn(),
            source_workspace_id,
        )
        .await?;
        crate::services::workspace_mutation_policy::ensure_workspace_mutable(
            &mut db,
            source_workspace_id,
            "workspaces.merge_start",
        )
        .await?;
        // Nothing lands centrally and the VM is not running: the only state
        // in which a brief bound to the Start is the right admission.
        if crate::services::workspace_vm_service::source_rebase_state(
            db.conn(),
            source_workspace_id,
        )
        .await?
            != crate::services::workspace_vm_service::SourceRebaseExecution::Starting
        {
            return Err(G5Error::Conflict {
                reason:
                    "The workspace started during the rebase; retry so it runs in the workspace"
                        .into(),
            }
            .into());
        }
        let resolution = state
            .workspace_merge_service
            .enqueue_source_resolution_with(
                state,
                &mut db,
                prepared,
                user_id,
                body.agent_launch.as_ref(),
                workspace_merge_service::ResolutionAdmission::PendingStart {
                    intent_id: start.intent_id,
                },
            )
            .await?;
        // A task found already active (a racing click) is returned as is; only
        // the brief this request queued is bound to its Start.
        if let Some(input_id) = resolution.input_id {
            crate::routes::start_and_send::bind(&mut db, &start, input_id).await?;
        }
        db.commit().await.map_err(AppError::from)?;
        tracing::info!(
            source_workspace_id = %source_workspace_id,
            target_workspace_id = %body.target_workspace_id,
            intent_id = %start.intent_id,
            "Workspace endpoint succeeded: start_workspace_merge (start and resolve with agent)"
        );
        Ok(Json(ResponseEnvelope::ok(
            StartWorkspaceMergeResponseDTO::Resolution {
                resolution,
                start_compute: Some((&start).into()),
            },
        )))
    }
    .await;
    if result.is_err() {
        crate::routes::start_and_send::abandon(state, auth, &start).await;
    }
    result
}

async fn list_pull_request_targets(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
) -> Result<
    Json<ResponseEnvelope<Vec<crate::services::workspace_pull_request_create::PullRequestTarget>>>,
    AppError,
> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let targets = crate::services::workspace_pull_request_create::targets(
        &state,
        &mut db,
        params.id,
        user.user_uuid(),
    )
    .await?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(targets)))
}

async fn create_workspace_pull_request(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CreateWorkspacePullRequestRequestDTO>,
) -> Result<Json<ResponseEnvelope<CreateWorkspacePullRequestResponseDTO>>, AppError> {
    let outcome = crate::services::workspace_pull_request_create::create_or_update(
        &state,
        crate::services::workspace_pull_request_create::PullRequestCaller::User(&auth),
        params.id,
        body,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(outcome)))
}

/// Spec routes I31 / coordinated_repo_publication D14: create-shaped preview
/// of the commit-derived PR text. Same guards as create; writes nothing.
async fn preview_workspace_pull_request_text(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<PreviewWorkspacePullRequestTextRequestDTO>,
) -> Result<
    Json<ResponseEnvelope<coordinated_repo_publication::WorkspacePullRequestTextPreview>>,
    AppError,
> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:write"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    let target_access_db = scoped_db_for_workspace(
        &state,
        &auth,
        body.target_workspace_id,
        &["workspaces:read"],
    )
    .await?;
    target_access_db.rollback().await.map_err(AppError::from)?;

    tracing::debug!(
        source_workspace_id = %params.id,
        target_workspace_id = %body.target_workspace_id,
        "Workspace endpoint called: preview_workspace_pull_request_text"
    );

    // The preview delegates through the transaction-free preflight, which
    // consumes the scoped capture and closes it before Git classification —
    // nothing to roll back here and no rows are written.
    let preview = state
        .coordinated_repo_publication_service
        .preview_workspace_pull_request_text(
            db,
            &state.workspace_merge_service,
            state.repo_storage.clone(),
            params.id,
            body.target_workspace_id,
        )
        .await?;

    Ok(Json(ResponseEnvelope::ok(preview)))
}

async fn list_workspace_pull_requests(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspacePathParams>,
    Query(query): Query<WorkspacePullRequestListQueryDTO>,
) -> Result<
    Json<ResponseEnvelope<Vec<coordinated_repo_publication::WorkspacePullRequestSummary>>>,
    AppError,
> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let role = parse_workspace_pull_request_role(query.role.as_deref())?;
    let status = parse_workspace_pull_request_status(query.status.as_deref())?;

    let pull_requests = state
        .coordinated_repo_publication_service
        .list_workspace_pull_requests(&mut db, params.id, role, status)
        .await?;

    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(pull_requests)))
}

// LEAN-TODO(P6.4): workspace pull requests retire with the governance legacy.
/// What an administrator of Base's organization may do on Base besides its
/// owner.
#[derive(Debug, Clone, Copy)]
enum BaseAdminAction {
    /// Pull Base or turn its auto-pull on or off
    /// (`g5.workspace.remote.base-admin-pull`).
    RemoteTracking,
    /// Refresh, close or merge this pull request, whose target must be Base
    /// (`g5.governance.pr.base-admin`).
    PullRequest(Uuid),
}

/// Opens a transaction for an owner action at path workspace `workspace_id`
/// that an administrator of Base's organization may also take on Base. The
/// workspace owner is admitted as for any owner mutation; otherwise
/// [`scoped_db_for_base_admin`] decides, and anyone it refuses gets the owner
/// refusal.
async fn scoped_db_for_owner_or_base_admin(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    action: BaseAdminAction,
) -> Result<g5_authz::ScopedDb, AppError> {
    let owner_refusal =
        match scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:write"]).await {
            Ok(mut db) => {
                crate::services::workspace_mutation_policy::ensure_workspace_owner(
                    &mut db,
                    workspace_id,
                )
                .await?;
                return Ok(db);
            }
            Err(error) if matches!(error.inner, G5Error::Forbidden { .. }) => error,
            Err(error) => return Err(error),
        };
    match scoped_db_for_base_admin(state, auth, workspace_id, action).await {
        Err(error) if matches!(error.inner, G5Error::Forbidden { .. }) => Err(owner_refusal),
        result => result,
    }
}

/// The organization-administrator branch of
/// [`scoped_db_for_owner_or_base_admin`]: the path workspace must be live
/// Base (for a pull request, its target rather than its source), and the
/// caller must hold `org:admin` in the token and at organization level, as
/// approval-requirement administration does. A workspace-scoped grant does
/// not count.
async fn scoped_db_for_base_admin(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    action: BaseAdminAction,
) -> Result<g5_authz::ScopedDb, AppError> {
    let refused = || {
        AppError::from(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        })
    };
    if !auth.is_user() || !auth.scopes().contains(&Scope::OrgAdmin) {
        return Err(refused());
    }
    let pull_request_id = match action {
        BaseAdminAction::RemoteTracking => None,
        BaseAdminAction::PullRequest(pull_request_id) => Some(pull_request_id),
    };
    // `org:admin` alone: a `workspaces:write` scope admits the owner only.
    let db = scoped_db_for_workspace(state, auth, workspace_id, &["org:admin"]).await?;
    admit_base_admin(db, workspace_id, pull_request_id).await
}

/// Admit the scope's actor as an organization administrator acting on the
/// live Base `workspace_id` (and on `pull_request_id` targeting it), at
/// organization level.
async fn admit_base_admin(
    mut db: g5_authz::ScopedDb,
    workspace_id: Uuid,
    pull_request_id: Option<Uuid>,
) -> Result<g5_authz::ScopedDb, AppError> {
    let org_id = crate::services::workspace_mutation_policy::base_admin_subject_org(
        &mut db,
        workspace_id,
        pull_request_id,
    )
    .await?
    .ok_or_else(|| {
        AppError::from(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        })
    })?;
    let actor_id = db.actor_id().into_uuid();
    crate::services::PermissionService::require_org_admin_on_conn(db.conn(), actor_id, org_id)
        .await?;
    Ok(db)
}

/// Merge authority over a pull request's target for a person session: the
/// target owner or Base administrator ([`scoped_db_for_owner_or_base_admin`]),
/// or — when `require_target_requirement` holds for the full check — the pull
/// request's [source owner](g5.governance.pr.source-owner-merge). The merge
/// surfaces require the target requirement (full authority); turning merge
/// when ready on does not, since the background merger rechecks it at merge
/// time (decision 3). Refresh and close do not call this: the source side is
/// granted merge only.
async fn scoped_db_for_source_owner_merge(
    state: &Arc<AppState>,
    auth: &AuthContext,
    target_workspace_id: Uuid,
    pull_request_id: Uuid,
    require_target_requirement: bool,
) -> Result<g5_authz::ScopedDb, AppError> {
    let owner_refusal = match scoped_db_for_owner_or_base_admin(
        state,
        auth,
        target_workspace_id,
        BaseAdminAction::PullRequest(pull_request_id),
    )
    .await
    {
        Ok(db) => return Ok(db),
        Err(error) if matches!(error.inner, G5Error::Forbidden { .. }) => error,
        Err(error) => return Err(error),
    };
    // A read scope confirms the member can see the target and resolves its org
    // without the write scope's single-writer ownership fence (which only the
    // owner clears). Someone who cannot even see the target gets the owner
    // refusal.
    let (org_id, actor) = {
        let read_db =
            match scoped_db_for_workspace(state, auth, target_workspace_id, &["workspaces:read"])
                .await
            {
                Ok(db) => db,
                Err(error)
                    if matches!(
                        error.inner,
                        G5Error::Forbidden { .. } | G5Error::NotFound { .. }
                    ) =>
                {
                    return Err(owner_refusal)
                }
                Err(error) => return Err(error),
            };
        let org_id = read_db.primary_org_id()?.into_uuid();
        let actor = read_db.actor_id().into_uuid();
        read_db.rollback().await.map_err(AppError::from)?;
        (org_id, actor)
    };
    // Admit the source owner on an org-scoped write capability, not the
    // target-owner fence, exactly as the Base administrator and the background
    // merger write a target they do not own (g5.workspace.mutation-authority).
    let mut db = crate::db::scoped::scoped_db_for_user_actor_in_org(
        state,
        actor,
        org_id,
        &["workspaces:write"],
    )
    .await?;
    let admitted = if require_target_requirement {
        crate::services::workspace_mutation_policy::source_owner_may_merge(
            &mut db,
            org_id,
            target_workspace_id,
            pull_request_id,
            actor,
        )
        .await?
    } else {
        crate::services::workspace_mutation_policy::is_source_owner_of_pull_request(
            &mut db,
            target_workspace_id,
            pull_request_id,
            actor,
        )
        .await?
    };
    if admitted {
        Ok(db)
    } else {
        db.rollback().await.map_err(AppError::from)?;
        Err(owner_refusal)
    }
}

/// Enforce the organization external-check merge policy
/// (governance g5.governance.merge.external-policy). When the org requires
/// passing external checks, re-read the PR's provider check/status snapshot at
/// the current provider head and refuse the merge unless every provider repo is
/// passing. A PR with no provider repos (local-only) is exempt. The reason is
/// surfaced to the Ship/Merge control.
async fn ensure_external_checks_merge_policy(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    db: &mut g5_authz::ScopedDb,
    params: &WorkspacePullRequestPathParams,
    actor: Uuid,
) -> Result<(), AppError> {
    let org = db.primary_org_id()?.into_uuid();
    if !merger.requires_external_checks()
        && !crate::services::config_service::org_external_checks_block_merge(db, org).await?
    {
        return Ok(());
    }
    let summary = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(db, params.pull_request_id)
        .await?;
    let verdict = match policy_checks_snapshot(state, merger, &summary, actor).await? {
        // No external PR left to evaluate: local-only, or every repository
        // already merged on GitHub (g5.git.provider-merged-outside).
        None => return Ok(()),
        Some(Ok(snapshot)) => {
            crate::services::workspace_pull_request_provider_lifecycle::external_checks_verdict(
                &snapshot,
            )
        }
        // A read failure cannot confirm passing checks; the policy waits rather
        // than admit the merge.
        Some(Err(())) => {
            crate::services::workspace_pull_request_provider_lifecycle::ExternalChecksVerdict::Pending
        }
    };
    use crate::services::workspace_pull_request_provider_lifecycle::ExternalChecksVerdict;
    let reason = match verdict {
        ExternalChecksVerdict::Passing => return Ok(()),
        ExternalChecksVerdict::Failing => {
            "Merge blocked: external checks are failing on this pull request. Your organization requires passing external checks before merge."
        }
        ExternalChecksVerdict::Pending => {
            "Merge blocked: external checks have not finished (still pending or unavailable). Your organization requires passing external checks before merge."
        }
    };
    Err(G5Error::Conflict {
        reason: reason.to_string(),
    }
    .into())
}

/// The checks the external-checks policy evaluates for a pull request: those
/// of its provider repositories G5 has yet to merge, read fresh against the
/// source workspace, where they are attached, so the verdict matches what
/// Prepare to Ship shows. `None` when no such repository is left; `Err` when
/// the checks could not be read. `read_checks` consumes its own scope, so the
/// caller's merge transaction is untouched.
pub(crate) async fn policy_checks_snapshot(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    summary: &coordinated_repo_publication::WorkspacePullRequestSummary,
    actor: Uuid,
) -> Result<Option<Result<serde_json::Value, ()>>, AppError> {
    let merged: Vec<&str> = summary
        .provider_repos
        .iter()
        .filter(|repo| repo.provider_status == "merged")
        .map(|repo| repo.provider_repo_full_name.as_str())
        .collect();
    if summary.provider_repos.len() == merged.len() {
        return Ok(None);
    }
    let checks_db = merger
        .workspace_read_db(state, summary.source_workspace_id)
        .await?;
    let query = crate::services::workspace_pull_request_provider_lifecycle::PullRequestReadQuery {
        selector: Some(summary.id.to_string()),
        repo: None,
        comments: false,
        reviews: false,
        checks: true,
        details: false,
        head: None,
        // The merge decision uses the current required set, not the display cache.
        fresh_requirements: true,
        ..Default::default()
    };
    Ok(Some(
        match crate::services::workspace_pull_request_provider_lifecycle::read_checks(
            state,
            checks_db,
            summary.source_workspace_id,
            actor,
            &query,
        )
        .await
        {
            Ok(mut snapshot) => {
                // A repository GitHub already merged is outside the policy.
                if let Some(repositories) = snapshot
                    .get_mut("repositories")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    repositories.retain(|repository| {
                        !repository["repository"].as_str().is_some_and(|name| {
                            merged
                                .iter()
                                .any(|merged| merged.eq_ignore_ascii_case(name))
                        })
                    });
                }
                Ok(snapshot)
            }
            Err(_) => Err(()),
        },
    ))
}

/// Who acts on a workspace pull request from the path workspace (merge,
/// close, refresh): a person through the UI, or that workspace's own agent
/// (`g5.story.agent-merge-child`), which acts for its current owner and never
/// as a person session.
pub(crate) enum PullRequestMerger<'a> {
    Person {
        auth: &'a AuthContext,
        user_id: Uuid,
    },
    WorkspaceAgent(&'a g5_auth::WorkspaceIncarnationClaims),
    /// G5 merging for the person who turned on merge when ready
    /// (`g5.git.pr-auto-merge`), re-admitted as that person at every step.
    WhenReady {
        user_id: Uuid,
        org_id: Uuid,
    },
}

impl PullRequestMerger<'_> {
    pub(crate) fn actor(&self) -> Uuid {
        match self {
            Self::Person { user_id, .. } | Self::WhenReady { user_id, .. } => *user_id,
            Self::WorkspaceAgent(claims) => claims.owner_user_id,
        }
    }

    /// Merge when ready waits for passing external checks whatever the
    /// organization requires (`g5.git.pr-auto-merge.readiness`).
    fn requires_external_checks(&self) -> bool {
        matches!(self, Self::WhenReady { .. })
    }

    /// A scope holding merge authority over the target.
    pub(crate) async fn target_db(
        &self,
        state: &Arc<AppState>,
        target_workspace_id: Uuid,
        pull_request_id: Uuid,
    ) -> Result<g5_authz::ScopedDb, AppError> {
        match self {
            Self::Person { auth, .. } => {
                scoped_db_for_source_owner_merge(
                    state,
                    auth,
                    target_workspace_id,
                    pull_request_id,
                    true,
                )
                .await
            }
            Self::WorkspaceAgent(claims) => {
                if target_workspace_id != claims.workspace_id {
                    return Err(G5Error::Forbidden {
                        missing_scopes: Vec::new(),
                    }
                    .into());
                }
                let mut db = crate::db::workspace_agent_scope::agent_owner_db(
                    state,
                    claims,
                    &["workspaces:write"],
                )
                .await?;
                crate::services::workspace_mutation_policy::ensure_workspace_owner(
                    &mut db,
                    target_workspace_id,
                )
                .await?;
                Ok(db)
            }
            Self::WhenReady { user_id, org_id } => {
                let mut db = crate::db::scoped::scoped_db_for_user_actor_in_org(
                    state,
                    *user_id,
                    *org_id,
                    &["workspaces:write"],
                )
                .await?;
                match crate::services::workspace_mutation_policy::ensure_workspace_owner(
                    &mut db,
                    target_workspace_id,
                )
                .await
                {
                    Ok(()) => return Ok(db),
                    Err(G5Error::Forbidden { .. }) => {}
                    Err(error) => return Err(error.into()),
                }
                // Not the target owner: the pull request's source owner may
                // merge when the target has an attached requirement, rechecked
                // here every pass so a lapsed authority turns the decision off
                // (g5.governance.pr.source-owner-merge, g5.git.pr-auto-merge).
                if crate::services::workspace_mutation_policy::source_owner_may_merge(
                    &mut db,
                    *org_id,
                    target_workspace_id,
                    pull_request_id,
                    *user_id,
                )
                .await?
                {
                    return Ok(db);
                }
                db.rollback().await.map_err(AppError::from)?;
                // Not the owner: an organization administrator of a Base
                // target, as for a person session. Minting `org:admin`
                // already requires holding it.
                let db = crate::db::scoped::scoped_db_for_user_actor_in_org(
                    state,
                    *user_id,
                    *org_id,
                    &["org:admin"],
                )
                .await
                .map_err(|error| match error.inner {
                    G5Error::Forbidden { .. } | G5Error::Unauthorized => {
                        AppError::from(G5Error::Forbidden {
                            missing_scopes: Vec::new(),
                        })
                    }
                    _ => error,
                })?;
                admit_base_admin(db, target_workspace_id, Some(pull_request_id)).await
            }
        }
    }

    /// Read a workspace for external checks or a committed merge response.
    /// A completed mutation does not need fresh write authority just to read
    /// its result when a transfer begins after the commit.
    pub(crate) async fn workspace_read_db(
        &self,
        state: &Arc<AppState>,
        workspace_id: Uuid,
    ) -> Result<g5_authz::ScopedDb, AppError> {
        match self {
            Self::Person { auth, .. } => {
                scoped_db_for_workspace(state, auth, workspace_id, &["workspaces:read"]).await
            }
            Self::WorkspaceAgent(claims) => {
                let mut db = crate::db::workspace_agent_scope::agent_owner_db(
                    state,
                    claims,
                    &["workspaces:read"],
                )
                .await?;
                crate::db::workspace_agent_scope::agent_read_target(
                    state,
                    &mut db,
                    claims,
                    workspace_id,
                )
                .await?;
                Ok(db)
            }
            Self::WhenReady { user_id, org_id } => {
                let mut db = crate::db::scoped::scoped_db_for_user_actor_in_org(
                    state,
                    *user_id,
                    *org_id,
                    &["workspaces:read"],
                )
                .await?;
                crate::db::scoped::ensure_user_workspace_access(
                    &state.system_pool,
                    &mut db,
                    *user_id,
                    *org_id,
                    workspace_id,
                )
                .await?;
                Ok(db)
            }
        }
    }
}

async fn merge_workspace_pull_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
) -> Result<Json<ResponseEnvelope<MergeWorkspacePullRequestResponseDTO>>, AppError> {
    let merger = PullRequestMerger::Person {
        auth: &auth,
        user_id: user.user_uuid(),
    };
    let merged = merge_pull_request_as(&state, &merger, &params, None).await?;
    Ok(Json(ResponseEnvelope::ok(merged)))
}

/// Merge a workspace pull request into its target (`params.id`) for `merger`,
/// with the override a person confirmed, if any
/// (g5.workspace.merge.admin-override.formal).
pub(crate) async fn merge_pull_request_as(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    params: &WorkspacePullRequestPathParams,
    merge_override: Option<&super::workspace_merge_override::MergeOverrideRequest>,
) -> Result<MergeWorkspacePullRequestResponseDTO, AppError> {
    // Provider outcomes the database never acknowledged are reconciled
    // before the merge is admitted again (g5.governance.merge): a provider
    // PR already merged keeps the admission recorded for its cut. Its own
    // committed transaction, closed before the capture opens. Reconciliation
    // resolves credentials in that transaction, including when refresh holds
    // publication locks and another request waits for the PR.
    {
        let mut reconcile_db = merger
            .target_db(state, params.id, params.pull_request_id)
            .await?;
        state
            .workspace_pull_request_provider_lifecycle_service
            .reconcile_provider_acceptance(
                &mut reconcile_db,
                params.pull_request_id,
                merger.actor(),
            )
            .await?;
        reconcile_db.commit().await.map_err(AppError::from)?;
    }
    let mut db = merger
        .target_db(state, params.id, params.pull_request_id)
        .await?;

    tracing::debug!(
        target_workspace_id = %params.id,
        pull_request_id = %params.pull_request_id,
        "Workspace endpoint called: merge_workspace_pull_request"
    );

    // External-check merge policy (governance g5.governance.merge.external-policy):
    // when the organization requires passing external checks, re-read the PR's
    // provider status at the pinned head and refuse unless every provider repo is
    // passing. A PR with no provider repos (local-only) has no external status to
    // gate. This runs before the merge so no ref advances on a block.
    // An override reads the same checks fresh and waives exactly the
    // blockers it was shown instead of refusing on them.
    let merge_override = match merge_override {
        None => {
            ensure_external_checks_merge_policy(state, merger, &mut db, params, merger.actor())
                .await?;
            None
        }
        Some(request) => Some(
            super::workspace_merge_override::override_intent(
                state, merger, &mut db, params, request,
            )
            .await?,
        ),
    };

    let outcome = state
        .coordinated_repo_publication_service
        .merge_workspace_pull_request(
            db,
            &state.workspace_merge_service,
            params.id,
            params.pull_request_id,
            merger.actor(),
            merge_override.as_ref(),
        )
        .await?;

    let outcome = match outcome {
        coordinated_repo_publication::MergeWorkspacePullRequestOutcome::ProviderFirstMerge(req) => {
            let provider_db = merger
                .target_db(state, params.id, params.pull_request_id)
                .await?;
            let mut provider_db = state
                .workspace_pull_request_provider_lifecycle_service
                .merge_provider_pull_requests(
                    provider_db,
                    req.pull_request_id,
                    &req.target_heads,
                    req.actor_id,
                    || async {
                        merger
                            .target_db(state, params.id, params.pull_request_id)
                            .await
                            .map_err(|error| error.inner)
                    },
                )
                .await?;
            let summary = state
                .coordinated_repo_publication_service
                .load_workspace_pull_request_summary(&mut provider_db, req.pull_request_id)
                .await?;
            // Persist provider acceptance before import. A failed fetch/landing
            // leaves the local PR open; retry verifies the already-merged PR.
            provider_db.commit().await.map_err(AppError::from)?;
            if !(summary.provider_status == "merged"
                || (summary.provider_status == "local_only" && summary.provider_repos.is_empty()))
            {
                // Name each unmerged repository's recorded reason; otherwise
                // it is visible only in the ship modal (issue #551).
                let reasons = summary
                    .provider_repos
                    .iter()
                    .filter(|repo| repo.provider_status != "merged")
                    .filter_map(|repo| {
                        repo.last_error.as_deref().map(|error| {
                            // Keep a truncated error's "..." marker.
                            let error = error.trim_end();
                            let stop = if error.ends_with('.') { "" } else { "." };
                            format!(" {}: {error}{stop}", repo.provider_repo_full_name)
                        })
                    })
                    .collect::<String>();
                return Err(G5Error::Conflict {
                    reason: format!("GitHub merge did not complete for all repos (provider status: {}).{reasons} The target was not advanced; resolve the GitHub PRs and retry.", summary.provider_status),
                }.into());
            }
            // The target's own auto-pull may land the accepted provider commit
            // between capture and preparation. One fresh capture then sees the
            // target already converged; any other change still refuses.
            let mut recaptured = false;
            let prepared = loop {
                let capture_db = merger
                    .target_db(state, params.id, params.pull_request_id)
                    .await?;
                let imports = state
                    .workspace_pull_request_provider_lifecycle_service
                    .prepare_merged_target_import(
                        capture_db,
                        req.pull_request_id,
                        req.target_workspace_id,
                        req.actor_id,
                    )
                    .await?;
                let prepare_db = merger
                    .target_db(state, params.id, params.pull_request_id)
                    .await?;
                match state
                    .coordinated_repo_publication_service
                    .prepare_provider_workspace_pull_request_merge(
                        prepare_db,
                        &state.workspace_merge_service,
                        (*req).clone(),
                        imports,
                    )
                    .await
                {
                    Err(error)
                        if !recaptured
                            && workspace_merge_service::is_provider_import_cut_changed(&error) =>
                    {
                        recaptured = true;
                    }
                    result => break result?,
                }
            };
            coordinated_repo_publication::MergeWorkspacePullRequestOutcome::PreparedLocal(Box::new(
                prepared,
            ))
        }
        other => other,
    };

    let (db, outcome) = match outcome {
        coordinated_repo_publication::MergeWorkspacePullRequestOutcome::PreparedLocal(prepared) => {
            let mut application_db = merger
                .target_db(state, params.id, params.pull_request_id)
                .await?;
            let result = state
                .coordinated_repo_publication_service
                .apply_prepared_workspace_pull_request_merge(
                    &mut application_db,
                    &state.workspace_merge_service,
                    *prepared,
                )
                .await?;
            (
                Some(application_db),
                coordinated_repo_publication::MergeWorkspacePullRequestOutcome::Merged(Box::new(
                    result,
                )),
            )
        }
        other => (None, other),
    };

    match outcome {
        coordinated_repo_publication::MergeWorkspacePullRequestOutcome::Merged(result) => {
            let db = db.ok_or_else(|| {
                AppError::from(G5Error::Internal {
                    message: "Merged workspace PR outcome is missing its application transaction"
                        .to_string(),
                })
            })?;
            let result = *result;
            let run = result.run;
            if let Some(advance) = result.authority_advance {
                // The application transaction holds the workspace commit
                // fences: the target cut lands in it with the merge run and
                // pull request settlements, so a refused landing or settlement
                // leaves the pull request open and no merge run behind.
                let landed = state
                    .coordinated_repo_publication_service
                    .land_workspace_pull_request_merge(
                        &state.workspace_merge_service,
                        advance,
                        db,
                        &state.event_bus,
                        &state.repo_storage,
                    )
                    .await?;
                // The target's branch moved: its reviewers' rounds are
                // outdated and their copies want an Update.
                crate::routes::workspace_reviews::notify_reviewers_after_landing(
                    state,
                    &landed,
                    merger.actor(),
                )
                .await;
            } else {
                db.commit().await.map_err(AppError::from)?;
            }
            tracing::info!(
                target_workspace_id = %params.id,
                pull_request_id = %params.pull_request_id,
                merge_run_id = %run.id,
                "Workspace endpoint succeeded: merge_workspace_pull_request"
            );

            let mut summary_db = merger.workspace_read_db(state, params.id).await?;
            let pull_request = state
                .coordinated_repo_publication_service
                .load_workspace_pull_request_summary(&mut summary_db, params.pull_request_id)
                .await?;
            let org_id = summary_db.primary_org_id()?.into_uuid();
            summary_db.commit().await.map_err(AppError::from)?;
            // Readers, the people who decided it and the source workspace's
            // agents learn of the merge (g5.git.pr-merge-announcement).
            crate::auto_merge::announce(
                state,
                org_id,
                pull_request.id,
                pull_request
                    .auto_merge_request
                    .as_ref()
                    .map(|request| request.enabled_by),
                match merger {
                    PullRequestMerger::Person { user_id, .. } => Some(*user_id),
                    PullRequestMerger::WorkspaceAgent(_) | PullRequestMerger::WhenReady { .. } => {
                        None
                    }
                },
                crate::auto_merge::Announced::Merged,
            )
            .await;
            crate::auto_merge::stop_after_merge(state, org_id, &pull_request);

            Ok(MergeWorkspacePullRequestResponseDTO { pull_request, run })
        }
        coordinated_repo_publication::MergeWorkspacePullRequestOutcome::PreparedLocal(_) => {
            unreachable!("prepared local workspace PR merges are applied before outcome dispatch")
        }
        coordinated_repo_publication::MergeWorkspacePullRequestOutcome::Blocked { reason } => {
            tracing::info!(
                target_workspace_id = %params.id,
                pull_request_id = %params.pull_request_id,
                reason = %reason,
                "Workspace endpoint blocked: merge_workspace_pull_request"
            );
            Err(G5Error::Conflict { reason }.into())
        }
        coordinated_repo_publication::MergeWorkspacePullRequestOutcome::ProviderFirstMerge(_) => {
            unreachable!("provider merges are prepared before local publication")
        }
    }
}

async fn refresh_workspace_pull_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspacePullRequestResponseDTO>>, AppError> {
    let merger = PullRequestMerger::Person {
        auth: &auth,
        user_id: user.user_uuid(),
    };
    let pull_request = refresh_pull_request_as(&state, &merger, &params).await?;
    Ok(Json(ResponseEnvelope::ok(
        WorkspacePullRequestResponseDTO { pull_request },
    )))
}

/// Recompute a workspace pull request's stored merge evidence for `merger`,
/// who holds merge authority over the path workspace.
pub(crate) async fn refresh_pull_request_as(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    params: &WorkspacePullRequestPathParams,
) -> Result<coordinated_repo_publication::WorkspacePullRequestSummary, AppError> {
    let db = merger
        .target_db(state, params.id, params.pull_request_id)
        .await?;
    tracing::debug!(
        workspace_id = %params.id,
        pull_request_id = %params.pull_request_id,
        "Workspace endpoint called: refresh_workspace_pull_request"
    );
    let prepared = state
        .coordinated_repo_publication_service
        .prepare_workspace_pull_request_refresh(
            db,
            &state.workspace_merge_service,
            params.id,
            params.pull_request_id,
            merger.actor(),
        )
        .await?;
    let mut db = merger
        .target_db(state, params.id, params.pull_request_id)
        .await?;
    let pull_request = state
        .coordinated_repo_publication_service
        .apply_prepared_workspace_pull_request_refresh(
            &mut db,
            &state.workspace_merge_service,
            prepared,
        )
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(pull_request)
}

async fn close_workspace_pull_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspacePullRequestResponseDTO>>, AppError> {
    let merger = PullRequestMerger::Person {
        auth: &auth,
        user_id: user.user_uuid(),
    };
    let pull_request = close_pull_request_as(&state, &merger, &params).await?;
    Ok(Json(ResponseEnvelope::ok(
        WorkspacePullRequestResponseDTO { pull_request },
    )))
}

/// Body of turning on merge when ready (`{}` without an agent).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableAutoMergeRequestDTO {
    /// Merge with agent (`g5.git.pr-auto-merge.with-agent`): also start a
    /// chat in the source workspace whose agent keeps the pull request green
    /// and rebase-mergeable until it merges.
    #[serde(default)]
    pub agent: Option<AutoMergeAgentDTO>,
    /// Omitted callers retain the source owner's existing shutdown choice.
    #[serde(default)]
    pub stop_vm_after_merge: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoMergeSettingsDTO {
    pub stop_vm_after_merge: bool,
}

/// Change the source owner's shutdown choice without enabling merge or
/// starting an agent. The PR owner validates the side and holds authority.
async fn update_workspace_pull_request_auto_merge_settings(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
    Json(body): Json<AutoMergeSettingsDTO>,
) -> Result<Json<ResponseEnvelope<WorkspacePullRequestResponseDTO>>, AppError> {
    let source = pull_request_source(&state, &auth, &params).await?;
    let mut db = scoped_db_for_workspace(&state, &auth, source, &["workspaces:write"]).await?;
    let changed = crate::services::workspace_pull_request_auto_merge::set_stop_vm_after_merge(
        &mut db,
        params.id,
        params.pull_request_id,
        user.user_uuid(),
        body.stop_vm_after_merge,
    )
    .await?;
    auto_merge_response(&state, db, params.pull_request_id, changed).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoMergeAgentDTO {
    /// The person's agent choice; `None` uses the configured defaults.
    #[serde(default)]
    pub agent_launch: Option<g5_contracts::types::AgentLaunchSelectionV1>,
    /// Names the chat start so a retry returns the first chat.
    #[serde(default)]
    pub request_key: Option<String>,
    /// Start and ship with agent (`g5.chat.start-and-send`): the source
    /// workspace's owner also records Start consent, and the agent's chat is
    /// queued bound to it, in this one request.
    #[serde(default)]
    pub start_compute: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableAutoMergeResponseDTO {
    pub pull_request: coordinated_repo_publication::WorkspacePullRequestSummary,
    /// The chat Merge with agent started.
    pub chat_session_id: Option<Uuid>,
    /// The Start a `startCompute: true` request recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_compute: Option<crate::routes::start_and_send::StartComputeDTO>,
}

/// Turn on merge when ready (`g5.git.pr-auto-merge`) for the signed-in
/// person, who must be able to merge the pull request into the path (target)
/// workspace. Workspace incarnation tokens never reach this route. With an
/// agent, the person must also own the source workspace; its chat is queued
/// before the decision commits, so a refusal of either changes nothing.
/// Start and ship with agent (`g5.git.pr-auto-merge.with-agent`,
/// `g5.chat.start-and-send`) records the owner's Start consent for the source
/// workspace first, as the chat routes do, and binds the queued chat to it;
/// a request that fails afterwards abandons a consent still only waiting.
async fn enable_workspace_pull_request_auto_merge(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
    Json(body): Json<EnableAutoMergeRequestDTO>,
) -> Result<Json<ResponseEnvelope<EnableAutoMergeResponseDTO>>, AppError> {
    let start = match &body.agent {
        Some(agent) if agent.start_compute => {
            begin_merge_agent_start(&state, &auth, &params, agent).await?
        }
        _ => None,
    };
    let mut chat_committed = false;
    let result = enable_auto_merge_with(
        &state,
        &user,
        &auth,
        &params,
        body,
        start.as_ref(),
        &mut chat_committed,
    )
    .await;
    // A chat already committed with its binding stands with its Start: the
    // owner's retry replays that chat and only the decision is still to be
    // made. Only a request that queued nothing abandons the consent.
    if result.is_err() && !chat_committed {
        if let Some(start) = &start {
            crate::routes::start_and_send::abandon(&state, &auth, start).await;
        }
    }
    result
}

/// Record the source owner's Start consent for Start and ship with agent.
/// Runs before any transaction of the request, because Resume locks the
/// workspace row; the source owner's Resume admission is the ownership
/// check, so a non-owner is refused here with nothing written. The
/// decision's own admission and the pull request's open state are checked
/// first, so a request the decision would refuse starts no VM. A retry of a
/// request whose response was lost replays the claimed chat
/// (`g5.chat.request-key`) and records no second Start.
async fn begin_merge_agent_start(
    state: &Arc<AppState>,
    auth: &AuthContext,
    params: &WorkspacePullRequestPathParams,
    agent: &AutoMergeAgentDTO,
) -> Result<Option<crate::routes::start_and_send::CompoundStart>, AppError> {
    let source = {
        let mut db =
            scoped_db_for_source_owner_merge(state, auth, params.id, params.pull_request_id, false)
                .await?;
        let summary = state
            .coordinated_repo_publication_service
            .load_workspace_pull_request_summary(&mut db, params.pull_request_id)
            .await?;
        db.rollback().await.map_err(AppError::from)?;
        // The decision would refuse a pull request that does not target the
        // path workspace or is no longer open; refuse here, before any VM
        // starts for it.
        if summary.target_workspace_id != params.id {
            return Err(AppError::from_error(G5Error::NotFound {
                resource: format!(
                    "workspace pull request {} into workspace {}",
                    params.pull_request_id, params.id
                ),
            }));
        }
        if summary.status != "open" {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: format!(
                    "Workspace pull request is {}; merge when ready applies to open pull requests",
                    summary.status
                ),
            }));
        }
        summary.source_workspace_id
    };
    let replay = match agent.request_key.as_deref() {
        Some(key) => {
            let mut db =
                scoped_db_for_workspace(state, auth, source, &["workspaces:write"]).await?;
            let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
            let user_id = auth.require_user_id()?;
            let claimed = state
                .workspace_messenger
                .request_key_claimed(&mut db, org_id, user_id, key)
                .await?;
            db.rollback().await.map_err(AppError::from)?;
            claimed
        }
        None => false,
    };
    if replay {
        return Ok(None);
    }
    Ok(Some(
        crate::routes::start_and_send::begin(state, auth, source).await?,
    ))
}

async fn enable_auto_merge_with(
    state: &Arc<AppState>,
    user: &AuthenticatedUser,
    auth: &AuthContext,
    params: &WorkspacePullRequestPathParams,
    body: EnableAutoMergeRequestDTO,
    start: Option<&crate::routes::start_and_send::CompoundStart>,
    chat_committed: &mut bool,
) -> Result<Json<ResponseEnvelope<EnableAutoMergeResponseDTO>>, AppError> {
    // Turning merge when ready on does not check target merge authority; the
    // source owner or a target merger may arm it and the background merger
    // rechecks authority at merge time (g5.git.pr-auto-merge, decision 3).
    let mut db =
        scoped_db_for_source_owner_merge(state, auth, params.id, params.pull_request_id, false)
            .await?;
    let settings_changed = match body.stop_vm_after_merge {
        Some(selected) => {
            crate::services::workspace_pull_request_auto_merge::set_stop_vm_after_merge(
                &mut db,
                params.id,
                params.pull_request_id,
                user.user_uuid(),
                selected,
            )
            .await?
        }
        None => false,
    };
    let changed = crate::services::workspace_pull_request_auto_merge::enable(
        &mut db,
        params.id,
        params.pull_request_id,
        user.user_uuid(),
        body.agent.is_some(),
    )
    .await?;
    let chat_session_id = match body.agent {
        Some(agent) => {
            let session =
                start_merge_agent(state, auth, &mut db, user.user_uuid(), params, agent, start)
                    .await?;
            // The chat's transaction committed with its binding.
            *chat_committed = true;
            Some(session)
        }
        None => None,
    };
    let pull_request = commit_auto_merge_change(
        state,
        db,
        params.pull_request_id,
        changed || settings_changed,
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(EnableAutoMergeResponseDTO {
        pull_request,
        chat_session_id,
        start_compute: start.map(Into::into),
    })))
}

/// Queue Merge with agent's chat in the source workspace, as the person
/// starting a chat there does; commits only the chat. With a Start consent,
/// the queued input is bound to it in the same transaction.
async fn start_merge_agent(
    state: &Arc<AppState>,
    auth: &AuthContext,
    target_db: &mut g5_authz::ScopedDb,
    user_id: Uuid,
    params: &WorkspacePullRequestPathParams,
    agent: AutoMergeAgentDTO,
    start: Option<&crate::routes::start_and_send::CompoundStart>,
) -> Result<Uuid, AppError> {
    let summary = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(target_db, params.pull_request_id)
        .await?;
    let source = summary.source_workspace_id;
    let mut db = scoped_db_for_workspace(state, auth, source, &["workspaces:write"]).await?;
    // Only the source owner's agent runs their chats.
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, source).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_mutable(
        &mut db,
        source,
        "pull_request.merge_with_agent",
    )
    .await?;
    let org_id = db.primary_org_id()?.into_uuid();
    let start_compute = agent.start_compute;
    let message = crate::services::workspace_messages::WorkspaceMessage {
        org_id,
        workspace_id: source,
        actor_user_id: user_id,
        target: crate::services::workspace_messages::MessageTarget::NewSession {
            title: Some(format!("Merge into {}", summary.target_workspace_name)),
            scope_type: None,
            scope_ref: None,
        },
        content: crate::auto_merge::merge_with_agent_prompt(
            summary.id,
            &summary.target_workspace_name,
            // GitHub as the checks read expects it (`g5.git.pr-no-github`).
            summary.repository_pull_requests.iter().any(|repository| {
                crate::services::git::pull_request_adapters::expects_github(
                    repository,
                    &summary.status,
                )
            }),
            summary.stop_vm_after_merge,
        ),
        agent_launch: agent.agent_launch,
        legacy_preferences: crate::services::config_service::legacy_agent_launch_preferences(
            None, None, None, None,
        ),
        attachment_digest: None,
        request_key: agent.request_key,
        workbench: None,
    };
    let sent = state.workspace_messenger.send(&mut db, message).await?;
    if sent.replayed && start_compute && sent.queued.delivery == "withdrawn" {
        // The replayed chat was withdrawn when the earlier Start ended; a
        // retry cannot revive it, under the old consent or a new one
        // (`g5.lifecycle.start`), so the request fails, a consent it just
        // recorded is abandoned with it, and the decision already made stays.
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "The earlier Start ended before the agent's message was sent; press Ship with agent again".to_string(),
        }));
    }
    if let Some(start) = start {
        if !sent.replayed {
            crate::routes::start_and_send::bind(&mut db, start, sent.queued.input_id).await?;
        }
    }
    db.commit().await.map_err(AppError::from)?;
    Ok(sent.session.id)
}

/// Turn off merge when ready for the signed-in person: someone who may merge
/// the pull request into the path (target) workspace, or the owner of its
/// source workspace.
async fn disable_workspace_pull_request_auto_merge(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
) -> Result<Json<ResponseEnvelope<WorkspacePullRequestResponseDTO>>, AppError> {
    use crate::services::workspace_pull_request_auto_merge::Side;
    let (mut db, side) = match scoped_db_for_owner_or_base_admin(
        &state,
        &auth,
        params.id,
        BaseAdminAction::PullRequest(params.pull_request_id),
    )
    .await
    {
        Ok(db) => (db, Side::Target(params.id)),
        Err(error) if matches!(error.inner, G5Error::Forbidden { .. }) => {
            let source = pull_request_source(&state, &auth, &params).await?;
            let mut db =
                scoped_db_for_workspace(&state, &auth, source, &["workspaces:write"]).await?;
            crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, source)
                .await?;
            (db, Side::Source(source))
        }
        Err(error) => return Err(error),
    };
    let org_id = db.primary_org_id()?.into_uuid();
    let decided_by = crate::services::workspace_pull_request_auto_merge::disable(
        &mut db,
        side,
        params.pull_request_id,
        user.user_uuid(),
        None,
    )
    .await?;
    let response = auto_merge_response(&state, db, params.pull_request_id, false).await?;
    if decided_by.is_some() {
        crate::auto_merge::announce(
            &state,
            org_id,
            params.pull_request_id,
            decided_by,
            Some(user.user_uuid()),
            crate::auto_merge::Announced::TurnedOff { reason: None },
        )
        .await;
    }
    Ok(response)
}

/// The source workspace of a pull request into the path workspace, read with
/// the caller's access to that workspace.
async fn pull_request_source(
    state: &Arc<AppState>,
    auth: &AuthContext,
    params: &WorkspacePullRequestPathParams,
) -> Result<Uuid, AppError> {
    let mut db = scoped_db_for_workspace(state, auth, params.id, &["workspaces:read"]).await?;
    let source = crate::services::workspace_pull_request_auto_merge::source_into(
        &mut db,
        params.pull_request_id,
        params.id,
    )
    .await?
    .ok_or_else(|| G5Error::NotFound {
        resource: format!("workspace pull request {}", params.pull_request_id),
    })?;
    db.rollback().await.map_err(AppError::from)?;
    Ok(source)
}

/// Commit a merge-when-ready change and return the pull request as readers
/// now see it, publishing the change to both workspaces when `publish`.
pub(crate) async fn commit_auto_merge_change(
    state: &Arc<AppState>,
    mut db: g5_authz::ScopedDb,
    pull_request_id: Uuid,
    publish: bool,
) -> Result<coordinated_repo_publication::WorkspacePullRequestSummary, AppError> {
    let pull_request = state
        .coordinated_repo_publication_service
        .load_workspace_pull_request_summary(&mut db, pull_request_id)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    if publish {
        crate::services::workspace_pull_request_auto_merge::publish_updated(
            state,
            pull_request.source_workspace_id,
            pull_request.target_workspace_id,
            pull_request.id,
        );
    }
    Ok(pull_request)
}

/// [`commit_auto_merge_change`] as a route response.
pub(crate) async fn auto_merge_response(
    state: &Arc<AppState>,
    db: g5_authz::ScopedDb,
    pull_request_id: Uuid,
    publish: bool,
) -> Result<Json<ResponseEnvelope<WorkspacePullRequestResponseDTO>>, AppError> {
    let pull_request = commit_auto_merge_change(state, db, pull_request_id, publish).await?;
    Ok(Json(ResponseEnvelope::ok(
        WorkspacePullRequestResponseDTO { pull_request },
    )))
}

/// Close a workspace pull request in G5 for `merger`, who holds merge
/// authority over the path workspace (its source or target).
pub(crate) async fn close_pull_request_as(
    state: &Arc<AppState>,
    merger: &PullRequestMerger<'_>,
    params: &WorkspacePullRequestPathParams,
) -> Result<coordinated_repo_publication::WorkspacePullRequestSummary, AppError> {
    let mut db = merger
        .target_db(state, params.id, params.pull_request_id)
        .await?;
    tracing::debug!(
        workspace_id = %params.id,
        pull_request_id = %params.pull_request_id,
        "Workspace endpoint called: close_workspace_pull_request"
    );
    let pull_request = state
        .coordinated_repo_publication_service
        .close_workspace_pull_request(&mut db, params.id, params.pull_request_id, merger.actor())
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(pull_request)
}

pub(crate) fn parse_workspace_pull_request_role(
    role: Option<&str>,
) -> Result<coordinated_repo_publication::WorkspacePullRequestRole, AppError> {
    match role.unwrap_or("target") {
        "source" => Ok(coordinated_repo_publication::WorkspacePullRequestRole::Source),
        "target" => Ok(coordinated_repo_publication::WorkspacePullRequestRole::Target),
        "all" => Ok(coordinated_repo_publication::WorkspacePullRequestRole::All),
        other => Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Invalid workspace pull request role",
                "role": other,
            }),
        }
        .into()),
    }
}

pub(crate) fn parse_workspace_pull_request_status(
    status: Option<&str>,
) -> Result<Option<&str>, AppError> {
    match status.unwrap_or("open") {
        "open" => Ok(Some("open")),
        "merged" => Ok(Some("merged")),
        "closed" => Ok(Some("closed")),
        "all" => Ok(None),
        other => Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Invalid workspace pull request status",
                "status": other,
            }),
        }
        .into()),
    }
}

/// `POST /api/v1/workspaces/:id/clone`
///
/// Deep-copies a workspace including branches, ontology nodes, dirty drafts,
/// docs, and settings. Returns 201 Created.
async fn clone_workspace(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspacePathParams>,
    Json(body): Json<CloneWorkspaceRequestDTO>,
) -> Result<
    (
        StatusCode,
        Json<ResponseEnvelope<CloneWorkspaceResponseDTO>>,
    ),
    AppError,
> {
    // The review-fork key family belongs to the review routes (Start
    // review): a caller-supplied key in it would make an ordinary clone
    // pass for a review workspace.
    if body
        .idempotency_key
        .starts_with(crate::services::workspace_review_service::REVIEW_FORK_KEY_PREFIX)
    {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "idempotencyKey": "keys starting with review-workspace: are reserved for review forks"
            }),
        }));
    }
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    tracing::debug!(workspace_id = %params.id, "Workspace endpoint called: clone_workspace");
    {
        let org_id = db.primary_org_id().map_err(AppError::from)?.into_uuid();
        crate::services::workspace_review_service::ensure_not_review_workspace(
            &mut db, org_id, params.id, "Fork",
        )
        .await?;
    }
    let user_id = user.user_uuid();
    let org_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT org_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(params.id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| {
        AppError::from_error(g5_contracts::error::G5Error::NotFound {
            resource: format!("workspace {}", params.id),
        })
    })?;
    // Advisory only (`g5.fork.source.advisory`): a failure to read it never
    // blocks the fork. The read runs under a savepoint, so a failed statement
    // (the hint rows live in Postgres) cannot leave the fork's transaction
    // aborted and fail the commit below.
    let repos = state.repo_storage.scoped(&db);
    let source_advisories = match sqlx::query("SAVEPOINT g5_fork_source_advisories")
        .execute(db.conn())
        .await
    {
        Ok(_) => match crate::services::remote_integrations::fork_source_advisories(
            db.conn(),
            &repos,
            params.id,
        )
        .await
        {
            Ok(advisories) => {
                if let Err(error) = sqlx::query("RELEASE SAVEPOINT g5_fork_source_advisories")
                    .execute(db.conn())
                    .await
                {
                    tracing::warn!(workspace_id = %params.id, %error, "Fork source advisory savepoint release failed");
                }
                advisories
            }
            Err(error) => {
                tracing::warn!(workspace_id = %params.id, %error, "Fork source advisories unavailable");
                match sqlx::query("ROLLBACK TO SAVEPOINT g5_fork_source_advisories")
                    .execute(db.conn())
                    .await
                {
                    Ok(_) => {
                        if let Err(error) =
                            sqlx::query("RELEASE SAVEPOINT g5_fork_source_advisories")
                                .execute(db.conn())
                                .await
                        {
                            tracing::warn!(workspace_id = %params.id, %error, "Fork source advisory savepoint release failed");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(workspace_id = %params.id, %error, "Fork source advisory savepoint rollback failed");
                    }
                }
                Vec::new()
            }
        },
        Err(error) => {
            tracing::warn!(workspace_id = %params.id, %error, "Fork source advisory savepoint unavailable");
            Vec::new()
        }
    };
    // Base re-verification is offered only while the organization's
    // verification switch is on (g5.workbench.review-prompts.switch).
    let reverification = body
        .reverification
        .map(|reverification| reverification.kind);
    if reverification.is_some()
        && !crate::services::config_service::org_boolean_setting(
            &mut db,
            org_id,
            crate::services::config_service::VERIFICATION_ACTION_ENABLED_SETTING,
        )
        .await
        .map_err(AppError::from_error)?
    {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "Base re-verification is turned off for this organization".into(),
        }));
    }
    let fork_scope = db.reopener();
    let run_scope = db.reopener();
    db.commit().await.map_err(AppError::from)?;

    let issue = body
        .issue
        .map(|issue| {
            let role = LinkRole::parse(issue.role.trim()).ok_or_else(|| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "issue.role",
                        "error": "must be primary or related",
                    }),
                })
            })?;
            Ok::<_, AppError>(workspace_fork_service::ForkIssueRequest {
                reference: issue.reference,
                role,
            })
        })
        .transpose()?;
    let req = workspace_fork_service::ForkWorkspaceRequest {
        name: body.name,
        description: body.description,
        idempotency_key: body.idempotency_key,
        read_only: body.read_only.unwrap_or(false),
        merge_target_workspace_id: body.merge_target_workspace_id,
        chat_session_ids: body.chat_session_ids,
        source: body
            .source
            .unwrap_or(workspace_fork_service::ForkSource::Durable),
        interrupt: body.interrupt,
        issue,
        start_chat: body
            .start_chat
            .map(|chat| workspace_fork_service::ForkStartChatRequest {
                content: chat.content,
                launch_preferences: chat.launch_preferences,
            }),
        uniquify_name: body.uniquify_name,
        reverification,
    };
    let result = state
        .workspace_fork_service
        .scoped(fork_scope)
        .fork_workspace(org_id, params.id, user_id, req)
        .await?;

    let workspace_id = result.workspace.id;
    let mut dto: CloneWorkspaceResponseDTO = result.into();
    dto.source_advisories = source_advisories;
    if reverification.is_some() {
        let mut run_db = run_scope.open().await.map_err(AppError::from)?;
        dto.reverification_run =
            crate::services::base_reverification::run_by_id(run_db.conn(), org_id, workspace_id)
                .await?;
        run_db.commit().await.map_err(AppError::from)?;
    }
    // A queued chat runs once the new workspace's VM is up, so start it now,
    // as a review fork does. The message stays queued if this fails; the
    // owner can start the workspace.
    if let Some((chat, session_id)) = dto
        .started_chat
        .as_mut()
        .and_then(|chat| chat.session_id.map(|session_id| (chat, session_id)))
    {
        if let Err(error) =
            start_forked_workspace(&state, &auth, &dto.workspace.id, user_id, session_id).await
        {
            tracing::warn!(
                workspace_id = %dto.workspace.id,
                error = %error.inner,
                "Forked workspace was not started for its chat"
            );
            chat.workspace_start_error = Some(error.inner.to_string());
        }
    }

    tracing::info!(
        source_workspace_id = %params.id,
        clone_workspace_id = %dto.workspace.id,
        "Workspace endpoint succeeded: clone_workspace"
    );

    Ok((StatusCode::CREATED, Json(ResponseEnvelope::ok(dto))))
}

// ─── Sync runs API (Phase 4.B remote-repo-tracking) ─────────────────────────

#[derive(Debug, serde::Serialize)]
struct BranchSyncRunDTO {
    id: Uuid,
    #[serde(rename = "workspaceId")]
    workspace_id: Uuid,
    #[serde(rename = "repoId")]
    repo_id: Uuid,
    #[serde(rename = "actingUserId")]
    acting_user_id: Uuid,
    kind: String,
    #[serde(rename = "triggerKind")]
    trigger_kind: String,
    status: String,
    #[serde(rename = "oldWorkspaceTip", skip_serializing_if = "Option::is_none")]
    old_workspace_tip: Option<String>,
    #[serde(rename = "newWorkspaceTip", skip_serializing_if = "Option::is_none")]
    new_workspace_tip: Option<String>,
    #[serde(rename = "oldMirrorTip", skip_serializing_if = "Option::is_none")]
    old_mirror_tip: Option<String>,
    #[serde(rename = "newMirrorTip", skip_serializing_if = "Option::is_none")]
    new_mirror_tip: Option<String>,
    #[serde(rename = "newBaseline", skip_serializing_if = "Option::is_none")]
    new_baseline: Option<String>,
    #[serde(rename = "errorDetail", skip_serializing_if = "Option::is_none")]
    error_detail: Option<String>,
    #[serde(rename = "mergeRunId", skip_serializing_if = "Option::is_none")]
    merge_run_id: Option<Uuid>,
    #[serde(rename = "startedAt")]
    started_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<crate::models::branch_sync_run::BranchSyncRun> for BranchSyncRunDTO {
    fn from(r: crate::models::branch_sync_run::BranchSyncRun) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            repo_id: r.repo_id,
            acting_user_id: r.acting_user_id,
            kind: r.kind,
            trigger_kind: r.trigger_kind,
            status: r.status,
            old_workspace_tip: r.old_workspace_tip,
            new_workspace_tip: r.new_workspace_tip,
            old_mirror_tip: r.old_mirror_tip,
            new_mirror_tip: r.new_mirror_tip,
            new_baseline: r.new_baseline,
            error_detail: r.error_detail,
            merge_run_id: r.merge_run_id,
            started_at: r.started_at,
            completed_at: r.completed_at,
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct SyncRunsRetryResponseDTO {
    enqueued: usize,
    outcome: &'static str,
}

/// Git state and integration policy for one workspace/repository membership,
/// surfaced alongside the paginated audit feed.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchTrackingRepoDTO {
    workspace_id: Uuid,
    repo_id: Uuid,
    repo_name: String,
    /// G5-side local branch name on the per-repo entry.
    repo_branch_name: String,
    tracking_mode: String,
    head_commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync_paused_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_tip_advisory_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_tip_advisory_observed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_run_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    import_held_by: Option<crate::services::remote_integrations::ImportHold>,
    /// An accepted workspace rewrite awaits publication to the provider.
    rewrite_pending: bool,
}

impl From<crate::services::branch_sync_runs_service::BranchTrackingRepo> for BranchTrackingRepoDTO {
    fn from(r: crate::services::branch_sync_runs_service::BranchTrackingRepo) -> Self {
        Self {
            workspace_id: r.workspace_id,
            repo_id: r.repo_id,
            repo_name: r.repo_name,
            repo_branch_name: r.repo_branch_name,
            tracking_mode: r.tracking_mode,
            head_commit: r.head_commit,
            remote_branch: r.remote_branch,
            baseline_commit: r.baseline_commit,
            sync_paused_reason: r.sync_paused_reason,
            last_sync_at: r.last_sync_at,
            remote_tip_advisory_sha: r.remote_tip_advisory_sha,
            remote_tip_advisory_observed_at: r.remote_tip_advisory_observed_at,
            latest_run_status: r.latest_run_status,
            import_held_by: r.import_held_by,
            rewrite_pending: r.rewrite_pending,
        }
    }
}

/// Branch-scoped tracking summary returned alongside the runs feed so
/// the operator panel renders without a second round-trip.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchTrackingSummaryDTO {
    workspace_id: Uuid,
    repos: Vec<BranchTrackingRepoDTO>,
}

/// Response shape for `GET .../sync_runs`. Carries the branch tracking
/// summary alongside the paginated audit-row feed.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ListBranchSyncRunsResponseDTO {
    branch: BranchTrackingSummaryDTO,
    runs: crate::services::PaginatedResult<BranchSyncRunDTO>,
}

/// `GET /api/v1/workspaces/:id/repos/:repoId/sync_runs`
///
/// Returns the branch's per-repo tracking summary (`sync_paused_reason`,
/// `tracking_mode`, advisory SHA, latest run status) and a paged list of
/// `remote_sync_runs` rows for the selected workspace/repository membership,
/// newest-first.
async fn list_branch_sync_runs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceRepoPathParams>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ResponseEnvelope<ListBranchSyncRunsResponseDTO>>, AppError> {
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    let key = g5_contracts::types::WorkspaceRepoKey {
        org_id: resolve_org_id_for_workspace(&mut db, params.id).await?,
        workspace_id: params.id,
        repo_id: params.repo_id,
    };
    let page = pagination_params(&pagination);

    let repos = state.repo_storage.scoped(&db);
    let summary = crate::services::branch_sync_runs_service::load_branch_tracking_summary(
        db.conn(),
        &repos,
        key,
    )
    .await?;

    let result =
        crate::services::branch_sync_runs_service::list_sync_runs_for_branch(db.conn(), key, &page)
            .await?;

    db.commit().await.map_err(AppError::from)?;

    let items: Vec<BranchSyncRunDTO> = result
        .items
        .into_iter()
        .map(BranchSyncRunDTO::from)
        .collect();
    let runs = crate::services::PaginatedResult::new(items, result.total, &page);

    let branch = BranchTrackingSummaryDTO {
        workspace_id: summary.workspace_id,
        repos: summary
            .repos
            .into_iter()
            .map(BranchTrackingRepoDTO::from)
            .collect(),
    };

    Ok(Json(ResponseEnvelope::ok(ListBranchSyncRunsResponseDTO {
        branch,
        runs,
    })))
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/sync_runs/retry`
///
/// Re-enqueue every remote-tracking child whose last cycle ended in a
/// retryable status (`push_failed` / `fetch_failed` / `auth_required`).
/// Returns 200 with `{ enqueued, outcome: "reenqueued" }`, 200 with
/// `outcome: "nothing"` if there are no remote-tracking children, and
/// 409 if a child exists but its last status is non-retryable.
async fn retry_branch_sync_runs(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
) -> Result<(StatusCode, Json<ResponseEnvelope<SyncRunsRetryResponseDTO>>), AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    let key = g5_contracts::types::WorkspaceRepoKey {
        org_id: resolve_org_id_for_workspace(&mut db, params.id).await?,
        workspace_id: params.id,
        repo_id: params.repo_id,
    };

    let outcome =
        crate::services::branch_sync_runs_service::retry_sync_runs_for_branch(db.conn(), key)
            .await?;

    db.commit().await.map_err(AppError::from)?;

    use crate::services::branch_sync_runs_service::RetryOutcome;
    let (code, dto) = match outcome {
        RetryOutcome::Reenqueued { branches } => (
            StatusCode::OK,
            SyncRunsRetryResponseDTO {
                enqueued: branches,
                outcome: "reenqueued",
            },
        ),
        RetryOutcome::Nothing => (
            StatusCode::OK,
            SyncRunsRetryResponseDTO {
                enqueued: 0,
                outcome: "nothing",
            },
        ),
        RetryOutcome::NonRetryable => (
            StatusCode::CONFLICT,
            SyncRunsRetryResponseDTO {
                enqueued: 0,
                outcome: "non_retryable",
            },
        ),
    };

    Ok((code, Json(ResponseEnvelope::ok(dto))))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchMergeRunDTO {
    id: Uuid,
    workspace_id: Uuid,
    source_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution_task_id: Option<Uuid>,
    resolution_session_id: Option<Uuid>,
    resolution_task_status: Option<String>,
    source_branch_name: String,
    status: String,
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocking_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_heads: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_heads: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<crate::services::workspace_merge_service::WorkspaceMergeRunSummary>
    for BranchMergeRunDTO
{
    fn from(s: crate::services::workspace_merge_service::WorkspaceMergeRunSummary) -> Self {
        Self {
            id: s.id,
            workspace_id: s.workspace_id,
            source_kind: s.source_kind,
            source_workspace_id: s.source_workspace_id,
            target_workspace_id: s.target_workspace_id,
            resolution_task_id: s.resolution_task_id,
            resolution_session_id: s.resolution_session_id,
            resolution_task_status: s.resolution_task_status,
            source_branch_name: s.source_branch_name,
            status: s.status,
            phase: s.phase,
            blocking_reason: s.blocking_reason,
            repo_id: s.repo_id,
            source_heads: s.source_heads,
            target_heads: s.target_heads,
            started_at: s.started_at,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ListBranchMergeRunsResponseDTO {
    items: Vec<BranchMergeRunDTO>,
}

#[derive(Debug, serde::Deserialize)]
struct ListBranchMergeRunsQuery {
    /// `active` (default) returns waiting/running/pending/rolling_back rows;
    /// `all` adds terminal states for audit.
    status: Option<String>,
    /// Include active reset-review siblings from the authorized workspace so
    /// its coordinated publication action can bind the complete checkpoint set.
    #[serde(default)]
    include_coordinated_reset: bool,
}

/// `GET /api/v1/workspaces/:id/repos/:repoId/merge_runs?status=active|all`
///
/// Lists `workspace_merge_runs` with `source_kind = 'remote_tracking'` bound
/// to the parent branch. Drives the merge-required popover (G2).
async fn list_branch_merge_runs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<WorkspaceRepoPathParams>,
    Query(query): Query<ListBranchMergeRunsQuery>,
) -> Result<Json<ResponseEnvelope<ListBranchMergeRunsResponseDTO>>, AppError> {
    use crate::services::remote_tracking_merge::MergeRunStatusFilter;

    let filter = match query.status.as_deref() {
        Some("all") => MergeRunStatusFilter::All,
        Some("active") | None => MergeRunStatusFilter::Active,
        Some(other) => {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: format!("unknown status filter '{other}'; expected 'active' or 'all'")
                    .into(),
            }));
        }
    };

    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    let items = crate::services::remote_tracking_merge::list_remote_tracking_merge_runs_for_branch(
        db.conn(),
        params.id,
        params.repo_id,
        filter,
        query.include_coordinated_reset,
    )
    .await?;
    db.commit().await.map_err(AppError::from)?;

    Ok(Json(ResponseEnvelope::ok(ListBranchMergeRunsResponseDTO {
        items: items.into_iter().map(BranchMergeRunDTO::from).collect(),
    })))
}

#[derive(Debug, serde::Serialize)]
struct CompleteMergeRunResponseDTO {
    completed: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StartMergeRunResponseDTO {
    run: BranchMergeRunDTO,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartMergeRunRequestDTO {
    agent_launch: Option<g5_contracts::types::AgentLaunchSelectionV1>,
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/merge_runs/:mergeRunId/start`
///
/// Launch a workspace agent with the owner's confirmed settings to rebase a
/// provider divergence. Legacy callers may use configured launch settings.
async fn start_branch_merge_run(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoMergeRunPathParams>,
    body: Option<Json<StartMergeRunRequestDTO>>,
) -> Result<Json<ResponseEnvelope<StartMergeRunResponseDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    ensure_workspace_repo_merge_run_id(
        &mut db,
        params.id,
        params.repo_id,
        params.merge_run_id,
        "mergeRunId",
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.id,
        merge_run_id = %params.merge_run_id,
        "Workspace endpoint called: start_branch_merge_run"
    );

    let start_scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let summary = crate::services::remote_tracking_merge::start_remote_tracking_merge(
        &state,
        &start_scope,
        params.merge_run_id,
        user.user_uuid(),
        body.agent_launch.as_ref(),
    )
    .await?;

    tracing::info!(
        workspace_id = %params.id,
        merge_run_id = %params.merge_run_id,
        resolution_task_id = ?summary.resolution_task_id,
        "Workspace endpoint succeeded: start_branch_merge_run"
    );

    Ok(Json(ResponseEnvelope::ok(StartMergeRunResponseDTO {
        run: BranchMergeRunDTO::from(summary),
    })))
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/merge_runs/start`
/// Resolve a pause-only divergence without requiring a preexisting workflow.
async fn start_branch_resolution(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoPathParams>,
    body: Option<Json<StartMergeRunRequestDTO>>,
) -> Result<Json<ResponseEnvelope<StartMergeRunResponseDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    let key = g5_contracts::types::WorkspaceRepoKey {
        org_id: resolve_org_id_for_workspace(&mut db, params.id).await?,
        workspace_id: params.id,
        repo_id: params.repo_id,
    };
    let scope = db.reopener();
    db.rollback().await.map_err(AppError::from)?;
    let body = body.map(|Json(body)| body).unwrap_or_default();
    let summary = crate::services::remote_tracking_merge::start_remote_tracking_resolution(
        &state,
        &scope,
        key,
        user.user_uuid(),
        body.agent_launch.as_ref(),
    )
    .await?;
    Ok(Json(ResponseEnvelope::ok(StartMergeRunResponseDTO {
        run: BranchMergeRunDTO::from(summary),
    })))
}

/// `POST /api/v1/workspaces/:id/repos/:repoId/merge_runs/:mergeRunId/complete`
///
/// Operator endpoint to mark a remote-tracking merge run as completed
/// after they have externally resolved the divergence. Clears the
/// paused flag on the child row and re-enqueues a commit-coupled
/// integration cycle so the worker pushes the resolved tip.
async fn complete_branch_merge_run(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(params): Path<WorkspaceRepoMergeRunPathParams>,
) -> Result<Json<ResponseEnvelope<CompleteMergeRunResponseDTO>>, AppError> {
    let auth = AuthContext::User(user.claims().clone());
    let mut db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    crate::services::workspace_mutation_policy::ensure_workspace_owner(&mut db, params.id).await?;
    ensure_workspace_repo_id(&mut db, params.id, params.repo_id, "repoId").await?;
    ensure_workspace_repo_merge_run_id(
        &mut db,
        params.id,
        params.repo_id,
        params.merge_run_id,
        "mergeRunId",
    )
    .await?;
    tracing::debug!(
        workspace_id = %params.id,
        merge_run_id = %params.merge_run_id,
        "Workspace endpoint called: complete_branch_merge_run"
    );

    let repos = state.repo_storage.scoped(&db);
    let prepared =
        crate::services::remote_tracking_merge::prepare_external_remote_tracking_merge_completion(
            db.conn(),
            params.merge_run_id,
            &repos,
            crate::services::remote_tracking_merge::CompletionRequester::Owner(user.user_uuid()),
        )
        .await?;
    db.rollback().await.map_err(AppError::from)?;

    let application_db =
        scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let outcome =
        crate::services::remote_tracking_merge::apply_external_remote_tracking_merge_completion(
            application_db,
            prepared,
            &state.event_bus,
            &state.repo_storage,
        )
        .await?;
    for warning in outcome.projection_warnings {
        tracing::warn!(
            workspace_id = %params.id,
                merge_run_id = %params.merge_run_id,
            %warning,
            "Remote-tracking external completion committed while notification remains pending"
        );
    }

    tracing::info!(
        workspace_id = %params.id,
        merge_run_id = %params.merge_run_id,
        completed = outcome.completed,
        "Workspace endpoint succeeded: complete_branch_merge_run"
    );

    Ok(Json(ResponseEnvelope::ok(CompleteMergeRunResponseDTO {
        completed: outcome.completed,
    })))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extracts the `user_id` from `AuthContext::User`. Returns `Forbidden`
/// error if the auth context is a service token (service tokens cannot
/// perform workspace operations).
/// Converts the shared `Pagination` extractor into the service-layer
/// `PaginationParams` type.
fn pagination_params(pagination: &Pagination) -> crate::services::PaginationParams {
    crate::services::PaginationParams {
        page: pagination.page(),
        per_page: pagination.per_page(),
    }
}

/// The PR's canonical G5 description and each provider publication's
/// lifecycle and comparison (`g5.git.pr-description`); read-only.
async fn get_workspace_pull_request_description(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(params): Path<WorkspacePullRequestPathParams>,
) -> Result<
    Json<
        ResponseEnvelope<
            crate::services::workspace_pull_request_provider_lifecycle::PullRequestDescriptionView,
        >,
    >,
    AppError,
> {
    let db = scoped_db_for_workspace(&state, &auth, params.id, &["workspaces:read"]).await?;
    let view = state
        .workspace_pull_request_provider_lifecycle_service
        .read_pull_request_description(db, params.id, params.pull_request_id, user.user_uuid())
        .await?;
    Ok(Json(ResponseEnvelope::ok(view)))
}

/// `?ids=a,b,c` for the batch description read.
#[derive(Debug, Deserialize)]
pub(crate) struct PullRequestDescriptionsQuery {
    #[serde(default)]
    ids: String,
}

impl PullRequestDescriptionsQuery {
    pub(crate) fn ids(&self) -> Result<Vec<Uuid>, G5Error> {
        self.ids
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| {
                Uuid::parse_str(id).map_err(|_| G5Error::ValidationError {
                    details: serde_json::json!({"message": format!("Invalid pull request id {id:?}")}),
                })
            })
            .collect()
    }
}

/// `GET /api/v1/workspaces/:id/pull-requests/descriptions?ids=…`: several
/// PRs' canonical descriptions without provider calls, for a listing's `body`
/// (`g5.git.pr-description`).
async fn get_workspace_pull_request_descriptions(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(workspace): Path<Uuid>,
    Query(query): Query<PullRequestDescriptionsQuery>,
) -> Result<
    Json<
        ResponseEnvelope<
            Vec<crate::services::workspace_pull_request_provider_lifecycle::PullRequestDescriptionText>,
        >,
    >,
    AppError,
>{
    let ids = query.ids()?;
    let db = scoped_db_for_workspace(&state, &auth, workspace, &["workspaces:read"]).await?;
    let read = state
        .workspace_pull_request_provider_lifecycle_service
        .read_pull_request_descriptions(db, workspace, &ids, user.user_uuid())
        .await?;
    Ok(Json(ResponseEnvelope::ok(read)))
}

async fn get_pull_request_info(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(workspace): Path<Uuid>,
    Query(query): Query<
        crate::services::workspace_pull_request_provider_lifecycle::PullRequestReadQuery,
    >,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, workspace, &["workspaces:read"]).await?;
    read_pull_request_info(&state, db, workspace, user.user_uuid(), query).await
}

/// `gh pr list`-style listing of a repository the workspace reads PRs from
/// (`g5.git.pr-list`). People's searches are not limited by G5.
async fn list_repository_pull_requests(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    auth: AuthContext,
    Path(workspace): Path<Uuid>,
    Query(query): Query<
        crate::services::workspace_pull_request_provider_lifecycle::PullRequestListQuery,
    >,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    let db = scoped_db_for_workspace(&state, &auth, workspace, &["workspaces:read"]).await?;
    let data = state
        .workspace_pull_request_provider_lifecycle_service
        .list_repository_pull_requests(db, workspace, user.user_uuid(), &query, false)
        .await?;
    Ok(Json(ResponseEnvelope::ok(data)))
}

pub(crate) async fn read_pull_request_info(
    state: &AppState,
    mut db: g5_authz::ScopedDb,
    workspace: Uuid,
    actor: Uuid,
    query: crate::services::workspace_pull_request_provider_lifecycle::PullRequestReadQuery,
) -> Result<Json<ResponseEnvelope<serde_json::Value>>, AppError> {
    if query.checks {
        return crate::services::workspace_pull_request_provider_lifecycle::read_checks(
            state, db, workspace, actor, &query,
        )
        .await
        .map(|data| Json(ResponseEnvelope::ok(data)));
    }
    if query.details || query.head.is_some() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({"message": "Details and head require checks=true"}),
        }
        .into());
    }
    let listing = query.all || query.workspace.is_some();
    let filtered = query.state.is_some()
        || query.author.is_some()
        || query.search.is_some()
        || query.limit.is_some();
    if (listing && query.selector.is_some()) || (filtered && !listing) {
        return Err(G5Error::ValidationError { details: serde_json::json!({"message": "State, author, search and limit filters apply to an --all or --workspace listing"}) }.into());
    }
    // A G5 PR ID or name (`PR######`) selects a G5 PR; a number or URL names
    // GitHub's (`g5.git.pr-adapter`, `g5.git.pr-local-number`).
    let g5_selector = match query.selector.as_deref() {
        Some(selector) => {
            state
                .workspace_pull_request_provider_lifecycle_service
                .g5_selection(&mut db, actor, selector)
                .await?
        }
        None => None,
    };
    if query.selector.is_some() && g5_selector.is_none() {
        let pull = state
            .workspace_pull_request_provider_lifecycle_service
            .read_repository_pull_request(db, workspace, actor, &query)
            .await?;
        return Ok(Json(ResponseEnvelope::ok(
            serde_json::json!({"source": "github", "pullRequest": pull}),
        )));
    }
    let data = {
        if query.repo.is_some() || query.comments || query.reviews {
            return Err(G5Error::ValidationError { details: serde_json::json!({"message": "Repository, comments and reviews options require a GitHub PR number or URL"}) }.into());
        }
        let service = &state.workspace_pull_request_provider_lifecycle_service;
        // Any readable PR by ID, and the readable listings (`g5.git.pr-reads`);
        // otherwise this workspace's own PRs.
        let mut has_more = None;
        let pulls = if let Some(id) = g5_selector {
            let reader = service.pull_request_reader(&db, actor).await?;
            service
                .ensure_readable_pull_request(&mut db, reader, id)
                .await?;
            state
                .coordinated_repo_publication_service
                .list_workspace_pull_requests_by_ids(&mut db, &[id])
                .await?
        } else if listing {
            let (ids, more) = service
                .list_readable_pull_requests(
                    &mut db,
                    actor,
                    &crate::services::workspace_pull_request_provider_lifecycle::ReadablePullRequestListQuery {
                        workspace: query.workspace.clone().filter(|_| !query.all),
                        state: query.state.clone(),
                        author: query.author.clone(),
                        search: query.search.clone(),
                        limit: query.limit,
                    },
                )
                .await?;
            has_more = Some(more);
            state
                .coordinated_repo_publication_service
                .list_workspace_pull_requests_by_ids(&mut db, &ids)
                .await?
        } else {
            state
                .coordinated_repo_publication_service
                .list_workspace_pull_requests(
                    &mut db,
                    workspace,
                    coordinated_repo_publication::WorkspacePullRequestRole::All,
                    None,
                )
                .await?
        };
        let repos = state.repo_storage.scoped(&db);
        db.rollback().await.map_err(AppError::from)?;
        let mut projected = Vec::with_capacity(pulls.len());
        for pull in &pulls {
            let evidence = crate::services::workspace_pull_request_freshness::merge_evidence(
                repos.clone(),
                pull,
            )
            .await;
            let mut value = serde_json::to_value(pull).map_err(|err| G5Error::Internal {
                message: format!("failed to serialize workspace PR: {err}"),
            })?;
            value["mergeEvidence"] = evidence.unwrap_or(serde_json::Value::Null);
            projected.push(value);
        }
        match (g5_selector, has_more) {
            (Some(_), _) => serde_json::json!({
                "source": "g5",
                "pullRequest": projected.into_iter().next().unwrap_or(serde_json::Value::Null),
            }),
            (None, Some(more)) => serde_json::json!({
                "source": "g5", "pullRequests": projected, "hasMore": more, "listing": true,
            }),
            (None, None) => serde_json::json!({"source": "g5", "pullRequests": projected}),
        }
    };
    Ok(Json(ResponseEnvelope::ok(data)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    #[test]
    fn provider_quota_query_defaults_to_claude_oauth_key() {
        // A bare `GET /provider-quota` (no query params) resolves the exact
        // subscription key the composer usage chip always asks for
        // (agent_interaction_ux D8; provider quota snapshot D13). Deserialized
        // through `serde_urlencoded` — the same format `axum::extract::Query`
        // uses — so this exercises the real query-string wire path, not JSON.
        let defaulted: ProviderQuotaQueryParams = serde_urlencoded::from_str("").expect("defaults");
        assert_eq!(defaulted.provider, PROVIDER_QUOTA_PROVIDER_CLAUDE_CODE);
        assert_eq!(
            defaulted.auth_mode,
            PROVIDER_QUOTA_AUTH_MODE_ANTHROPIC_OAUTH
        );

        // Explicit camelCase params (`authMode`) are honored verbatim.
        let explicit: ProviderQuotaQueryParams =
            serde_urlencoded::from_str("provider=claude_code&authMode=anthropic_oauth")
                .expect("explicit");
        assert_eq!(explicit.provider, "claude_code");
        assert_eq!(explicit.auth_mode, "anthropic_oauth");
    }

    #[test]
    fn open_question_metric_ids_are_bounded_and_deduplicated() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let ids =
            parse_workspace_metric_ids(&format!("{first},{second},{first}")).expect("valid ids");
        assert_eq!(ids, vec![first, second]);

        assert!(parse_workspace_metric_ids("").is_err());
        assert!(parse_workspace_metric_ids("not-a-uuid").is_err());
        let too_many = (0..51)
            .map(|_| Uuid::new_v4().to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse_workspace_metric_ids(&too_many).is_err());
    }

    #[test]
    fn open_question_metrics_dto_serializes_derived_total() {
        let workspace_id = Uuid::new_v4();
        let dto = WorkspaceOpenQuestionMetricsDTO::from(
            workspace_service::WorkspaceOpenQuestionMetrics {
                workspace_id,
                document_open_questions: 1,
                project_doc_sources: vec![
                    crate::services::doc_service::ProjectDocOpenQuestionSource {
                        doc_id: "doc-1".to_string(),
                        display_name: "Plan".to_string(),
                        open_question_count: 1,
                        first_open_question_block_id: Some("Q4".to_string()),
                        open_questions: vec![
                            crate::services::doc_service::ProjectDocOpenQuestion {
                                block_id: "Q4".to_string(),
                                title: "Choose rollout".to_string(),
                                severity: Some("high".to_string()),
                            },
                        ],
                        actionable_at: chrono::Utc::now(),
                    },
                ],
                question_set_sources: vec![
                    crate::services::question_set_service::QuestionSetOpenSource {
                        path: "docs/parent/questions/plan.g5q.json".to_string(),
                        set_id: "plan".to_string(),
                        title: "Plan".to_string(),
                        open_question_count: 1,
                        open_questions: vec![
                            crate::services::question_set_service::QuestionSetOpenQuestion {
                                question_id: "pick-0123abcd".to_string(),
                                display_number: 1,
                                title: "Pick one".to_string(),
                                severity: None,
                                status: "proposed".to_string(),
                            },
                        ],
                        referenced_by: vec!["doc-1".to_string()],
                        invalid: false,
                    },
                ],
                question_sets_incomplete: true,
            },
        );

        let value = serde_json::to_value(dto).expect("DTO should serialize");
        assert_eq!(value["workspaceId"], workspace_id.to_string());
        assert_eq!(value["questionSetsIncomplete"], true);
        assert_eq!(
            value["questionSetSources"][0]["openQuestions"][0]["status"],
            "proposed"
        );
        assert_eq!(value["questionSetSources"][0]["referencedBy"][0], "doc-1");
        assert_eq!(value["documentOpenQuestions"], 1);
        assert_eq!(value["openQuestionsTotal"], 1);
        assert_eq!(
            value["projectDocSources"][0]["openQuestions"][0]["blockId"],
            "Q4"
        );
        assert_eq!(
            value["projectDocSources"][0]["openQuestions"][0]["severity"],
            "high"
        );
    }

    #[test]
    fn source_download_zip_contains_multi_repo_layout() {
        let api_id = Uuid::new_v4();
        let worker_id = Uuid::new_v4();
        let sources = vec![
            WorkspaceSourceZipRepo {
                repo_id: api_id,
                repo_name: "api".to_string(),
                files: BTreeMap::from([
                    ("README.md".to_string(), b"api readme".to_vec()),
                    ("empty.txt".to_string(), Vec::new()),
                    ("assets/data.bin".to_string(), vec![0, 255, 128, 10]),
                ]),
            },
            WorkspaceSourceZipRepo {
                repo_id: worker_id,
                repo_name: "worker".to_string(),
                files: BTreeMap::from([("src/lib.rs".to_string(), b"pub fn worker() {}".to_vec())]),
            },
        ];
        let mut archive = Cursor::new(Vec::new());

        write_workspace_source_zip(&sources, &mut archive).expect("source zip should build");
        archive.set_position(0);
        let mut zip = zip::ZipArchive::new(archive).expect("zip should be readable");

        let names: Vec<String> = zip
            .file_names()
            .map(|name| name.expect("archive filename should be valid").into_owned())
            .collect();
        assert!(names.contains(&"api/README.md".to_string()));
        assert!(names.contains(&"worker/src/lib.rs".to_string()));
        assert!(!names.iter().any(|name| name.starts_with("docs/")));

        for (name, expected) in [
            ("api/empty.txt", Vec::new()),
            ("api/assets/data.bin", vec![0, 255, 128, 10]),
        ] {
            let mut file = zip.by_name(name).expect("source file should be archived");
            assert_eq!(file.compression(), zip::CompressionMethod::Deflated);
            assert_eq!(file.unix_mode().map(|mode| mode & 0o777), Some(0o644));
            let mut actual = Vec::new();
            file.read_to_end(&mut actual)
                .expect("source bytes should round-trip with a valid checksum");
            assert_eq!(actual, expected);
        }

        let mut api_readme = String::new();
        zip.by_name("api/README.md")
            .expect("api readme should exist")
            .read_to_string(&mut api_readme)
            .expect("api readme should be utf-8");
        assert_eq!(api_readme, "api readme");
    }

    #[test]
    fn source_download_zip_roots_are_collision_safe() {
        let repo_a = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let repo_b = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();
        let sources = vec![
            WorkspaceSourceZipRepo {
                repo_id: repo_a,
                repo_name: "API".to_string(),
                files: BTreeMap::new(),
            },
            WorkspaceSourceZipRepo {
                repo_id: repo_b,
                repo_name: "api".to_string(),
                files: BTreeMap::new(),
            },
        ];

        let roots = source_zip_roots(&sources);

        assert_eq!(roots, vec!["api-aaaaaaaa", "api-bbbbbbbb"]);
    }

    #[test]
    fn source_download_rejects_unsafe_archive_paths() {
        for path in [
            "",
            "/absolute/path.rs",
            "\\absolute\\path.rs",
            "src\\lib.rs",
            "src//lib.rs",
            "src/./lib.rs",
            "src/../lib.rs",
            "src/.git/config",
        ] {
            assert!(
                validate_source_archive_path(path).is_err(),
                "path should be unsafe: {path}"
            );
        }
        validate_source_archive_path("src/lib.rs").expect("normal repo path should be safe");
    }

    #[test]
    fn source_download_skips_git_archive_paths() {
        assert!(should_skip_source_archive_path(".git/config"));
        assert!(should_skip_source_archive_path("nested/.GIT/config"));
        assert!(!should_skip_source_archive_path(".gitignore"));
        assert!(!should_skip_source_archive_path("src/git/mod.rs"));
    }
}
