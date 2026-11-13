//! Workspace service — business logic for workspace lifecycle management.
//!
//! Implements workspace CRUD, branch management, sync, merge, dependency
//! management, and member management per `API_AND_STORAGE.md` §3.1.
//!
//! The `WorkspaceService` struct owns a `PgPool` and exposes async methods
//! that sit between route handlers and the database layer. Transaction
//! boundaries are owned by this service — route handlers never manage
//! transactions directly.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use g5_api_foundation::contracts::explainer_templates::{
    CommitExplainerTemplateResult, ExplainerTemplateInput, SavedExplainerTemplate,
};
use g5_api_foundation::contracts::files::{
    CommitDiffDTO, CommitHistoryPageDTO, CommitSummaryDTO, FileTreeEntry, FileTreeListing,
};
use g5_api_foundation::contracts::preview_profiles::{
    CommitPreviewProfileResult, PreviewApplicationDTO, PreviewApplicationSource, PreviewProfileDTO,
    PreviewProfileInput,
};
use g5_api_foundation::contracts::settings::UpdateWorkspaceSettingsRequest;
use g5_api_foundation::contracts::workbench::JourneyMarkDTO;
use g5_authz::{AuthzService, ScopedDb, ScopedDbReopener};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info};
use uuid::Uuid;

use crate::errors::AppError;
use crate::git_runtime::{remote_branch_tip, NETWORK_DEADLINE};
use crate::models::Workspace;
use crate::services::branch_commit::{
    map_branch_commit_error, prepare_branch_commit, BranchCommitError, PreparedBranchCommit,
};
use crate::services::git::remote_integrations::PreparedMakeRemoteApplication;
use crate::services::git::repo_store::{OrgScope, RepoKey, ScopedRepoStorage, TenantRepoStorage};
use crate::services::git_policy::{is_local_git_remote, is_local_repo_identity};
use crate::services::home_service::{HomeAttentionCursor, HomeAttentionKind, HomeAttentionOrder};
use crate::services::preview_config::read_preview_application;
use crate::services::workspace_names;
use crate::services::workspace_ops::{
    branch_materialization_status, terminalize_active_merge_runs_with_tombstoned_sides,
};
use crate::services::workspace_reader::{
    expected_unborn_workspace_authority_cuts, read_branch_head_oids, resolve_committed_snapshot,
    CommittedRepoReadView, RepoReadView, FILE_TREE_LIMITS, MAX_FILE_READ_BYTES,
};
use crate::services::{DocService, EventBus, WorkspaceRuntime};
use crate::ws::{Channel, WsEvent};
use g5_contracts::error::G5Error;
use g5_contracts::types::ids::{OrgId, UserId};
use g5_contracts::types::{
    AuthorityTarget, CommittedBindingSnapshot, HistoricalGitVersion, HistoricalReadReason,
    RepoBindingVersion, WorkspaceHeadRepoInput, WorkspaceRepoKey,
};
use g5_scm::{GitAuth, GitAuthor, GitRepo};

// Only independently owned tasks block workspace quiescence.
const NONTERMINAL_WORK_SQL: &str = r#"
    SELECT 'task' AS kind, id, kind AS label, status, created_at FROM tasks
    WHERE workspace_id = $1 AND kind <> 'preview' AND status IN ('queued', 'running', 'waiting')
"#;

const ZERO_GIT_SHA: &str = "0000000000000000000000000000000000000000";

#[cfg(feature = "test-helpers")]
pub(crate) use crate::services::git_test_barrier::clone_merge_test_barrier;

#[cfg(feature = "test-helpers")]
pub use clone_merge_test_barrier::install as install_clone_merge_preparation_barrier;
#[cfg(feature = "test-helpers")]
pub use clone_merge_test_barrier::install as install_prepared_landing_barrier;
#[cfg(feature = "test-helpers")]
pub use clone_merge_test_barrier::install_refs_applied as install_refs_applied_landing_barrier;

/// A head participates in divergence computation only when it is a real
/// commit SHA: unborn shells carry `ZERO_GIT_SHA` and must degrade to the
/// honest unknown (`None`) instead of erroring (ontology D50, issue #1995).
fn is_computable_git_head(sha: &str) -> bool {
    !sha.trim().is_empty() && sha != ZERO_GIT_SHA
}
#[derive(Debug, Clone)]
pub struct WorkspaceOpenQuestionMetrics {
    pub workspace_id: Uuid,
    /// Embedded open fences plus question-set entries that are `open` or
    /// `proposed`, each set entry counted once however many documents
    /// reference it (`g5.documents.question.metrics`).
    pub document_open_questions: i64,
    pub project_doc_sources: Vec<crate::services::doc_service::ProjectDocOpenQuestionSource>,
    /// The question-set family: the workspace's own sets plus the inherited
    /// sets its documents reference.
    pub question_set_sources: Vec<crate::services::question_set_service::QuestionSetOpenSource>,
    /// True when a scanned set did not parse or could not be read: the count
    /// is a lower bound.
    pub question_sets_incomplete: bool,
}

/// Per-workspace inputs captured inside the capture transaction (validated
/// Project Doc read identities).
struct CapturedWorkspaceOpenQuestionInputs {
    workspace_id: Uuid,
    project_docs: Option<crate::services::doc_service::CapturedDocsList>,
}

/// Everything `open_question_metrics` needs after the capture transaction has
/// committed. Live workspace file overlays are deliberately NOT read yet:
/// only validated identities ride along, so effective document I/O never
/// holds tenant DB resources (the capture/materialize seam, enforced by
/// `tests/current_reader_contract.rs`).
pub struct CapturedWorkspaceOpenQuestionMetrics {
    actor_user_id: Uuid,
    workspaces: Vec<CapturedWorkspaceOpenQuestionInputs>,
}

struct CapturedHomeProjectDocs {
    workspace_id: Uuid,
    captured: crate::services::doc_service::CapturedDocsList,
}

pub(crate) struct CapturedHomeQuestionCandidates {
    actor_user_id: Uuid,
    project_docs: Vec<CapturedHomeProjectDocs>,
    cursor: Option<HomeAttentionCursor>,
    order: HomeAttentionOrder,
    candidate_limit: usize,
}

pub(crate) struct HomeProjectDocQuestionSource {
    pub workspace_id: Uuid,
    pub source: crate::services::doc_service::ProjectDocOpenQuestionSource,
}

/// One question set with open entries for the Home `open_question` kind
/// (`g5.documents.question.metrics`), linked to its first referencing doc.
pub(crate) struct HomeQuestionSetSource {
    pub workspace_id: Uuid,
    pub source: crate::services::question_set_service::QuestionSetOpenSource,
    pub referencing_doc_id: String,
    pub actionable_at: DateTime<Utc>,
}

/// The two open-question sources of one workspace, read in page order.
#[derive(Clone, Copy)]
enum HomeQuestionSource {
    Docs,
    Sets,
}

pub(crate) struct HomeQuestionCandidateSets {
    pub project_docs: Vec<HomeProjectDocQuestionSource>,
    /// Shares the `open_question` stream budget with `project_docs`.
    pub question_sets: Vec<HomeQuestionSetSource>,
}

fn repo_name_for(repo_names: &HashMap<Uuid, String>, repo_id: Uuid) -> String {
    repo_names
        .get(&repo_id)
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Render the canonical `.g5/preview_profiles/<name>.yaml` body from a typed
/// profile input (top-level block shape — `g5/g5-common/types` D18).
/// Deterministic field order; optional fields are omitted when empty.
fn render_preview_profile_yaml(input: &PreviewProfileInput) -> Result<String, G5Error> {
    #[derive(serde::Serialize)]
    struct PreviewProfileFile<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<&'a str>,
        command: &'a str,
        port: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        working_dir: Option<&'a str>,
        #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        env: &'a std::collections::BTreeMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cleanup_command: Option<&'a str>,
    }
    let file = PreviewProfileFile {
        description: input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        command: input.command.trim(),
        port: input.port,
        working_dir: input
            .working_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        env: &input.env,
        cleanup_command: input
            .cleanup_command
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    };
    serde_yaml::to_string(&file).map_err(|e| G5Error::Internal {
        message: format!("failed to render preview profile YAML: {e}"),
    })
}

/// Render the canonical saved explainer template YAML (`{ name, prompt }` —
/// `g5/g5-api/routes/explainer_templates` I4).
fn render_explainer_template_yaml(input: &ExplainerTemplateInput) -> Result<String, G5Error> {
    #[derive(serde::Serialize)]
    struct ExplainerTemplateFile<'a> {
        name: &'a str,
        prompt: &'a str,
    }
    let file = ExplainerTemplateFile {
        name: input.name.trim(),
        prompt: &input.prompt,
    };
    serde_yaml::to_string(&file).map_err(|e| G5Error::Internal {
        message: format!("failed to render explainer template YAML: {e}"),
    })
}

/// The provider repository name for a clone URL. A configured GitHub origin
/// uses the canonical configured-instance parser, so an Enterprise path prefix
/// or port never enters stored identity or the chip; other origins keep their
/// full path (GitLab subgroups).
fn provider_repository_name(policy: &g5_scm::ScmOriginPolicy, git_url: &str) -> Option<String> {
    match policy.provider_for_url(git_url) {
        Some(provider) if provider.is_github() => provider.github_repository_name(git_url),
        _ => repo_full_name_from_git_url(git_url),
    }
}

/// Generic non-GitHub fallback: parse the path after the host from a clone
/// URL. Handles `https://host/owner/repo(.git)` and scp-like
/// `git@host:owner/repo(.git)` forms; GitLab subgroups keep their full path.
/// Returns `None` when no `owner/repo` path segment can be extracted.
fn repo_full_name_from_git_url(git_url: &str) -> Option<String> {
    let trimmed = git_url.trim();
    let path = if let Some(idx) = trimmed.find("://") {
        // scheme://host/owner/repo — drop scheme + host.
        trimmed[idx + 3..].split_once('/').map(|(_, rest)| rest)?
    } else if let Some((_, after_user)) = trimmed.split_once('@') {
        // user@host:owner/repo — drop user + host.
        after_user.split_once(':').map(|(_, rest)| rest)?
    } else if let Some((_, rest)) = trimmed.split_once(':') {
        // host:owner/repo (no user) — drop host.
        rest
    } else {
        trimmed
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
    if path.is_empty() || !path.contains('/') {
        None
    } else {
        Some(path.to_string())
    }
}

/// Whether `url` uses an allow-listed remote transport scheme for Make-Remote
/// Adopt-Remote accepts only the HTTPS origins of supported SCM providers.
#[cfg(test)]
fn is_allowed_remote_git_scheme(url: &str) -> bool {
    g5_scm::ScmProvider::from_git_url(url).is_some()
}

/// Chip-display metadata for a repo, used to render the remote-tracking repo
/// chip's remote identity.
struct RepoChipMeta {
    name: String,
    remote_repo_name: Option<String>,
}

impl RepoChipMeta {
    /// The provider repository full name for the chip: `provider_full_name`
    /// when present, else [`provider_repository_name`] of `git_url`.
    fn new(
        name: String,
        provider_full_name: Option<String>,
        git_url: &str,
        policy: &g5_scm::ScmOriginPolicy,
    ) -> Self {
        Self {
            name,
            remote_repo_name: provider_full_name
                .or_else(|| provider_repository_name(policy, git_url)),
        }
    }

    fn remote_repo_name(&self) -> Option<String> {
        self.remote_repo_name.clone()
    }
}

/// Compute the `(remote_branch, remote_repo_name)` pair surfaced on a
/// `BranchSummary`. Both are `Some` only for remote-tracking branches; the
/// remote repo name derives from the repo's `provider_full_name` / `git_url`.
/// Shared by the workspace-detail (`fetch_branch_summaries`) and the
/// workspace-list (`list_workspaces`) branch builders so the two never drift.
fn remote_identity_for(
    is_remote_tracking: bool,
    remote_branch: Option<String>,
    meta: Option<&RepoChipMeta>,
) -> (Option<String>, Option<String>) {
    if is_remote_tracking {
        (remote_branch, meta.and_then(RepoChipMeta::remote_repo_name))
    } else {
        (None, None)
    }
}

// Membership is the complete canonical inventory, including unborn Git refs.
fn canonical_workspace_summaries(branches: Vec<BranchSummary>) -> Vec<BranchSummary> {
    branches
}

fn workspace_source_download_from_snapshot(
    workspace_name: String,
    docs_repo_id: Option<Uuid>,
    snapshot: CommittedBindingSnapshot,
) -> Result<WorkspaceSourceDownloadArchive, G5Error> {
    let mut repos = snapshot
        .repos
        .into_iter()
        .filter(|repo| Some(repo.repo_id) != docs_repo_id)
        .collect::<Vec<_>>();
    repos.sort_by(|left, right| {
        left.repo_name
            .cmp(&right.repo_name)
            .then_with(|| left.branch_name.cmp(&right.branch_name))
            .then_with(|| left.repo_id.cmp(&right.repo_id))
    });

    if repos.is_empty() {
        Err(G5Error::NotFound {
            resource: "workspace source code repository branches".to_string(),
        })
    } else {
        Ok(WorkspaceSourceDownloadArchive {
            workspace_name,
            repos,
        })
    }
}

// ---------------------------------------------------------------------------
// DTO structs
// ---------------------------------------------------------------------------

/// Request to update a workspace.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub description: Option<String>,
}

/// The three workbench phases a workspace can be in (workbench
/// `g5.workbench.phases`): Business, System and Integration Engineering.
pub const WORKSPACE_PHASES: &[&str] = &["be", "se", "ie"];

/// A workspace list item with computed stats and branch info.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceListItem {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_base: bool,
    /// Derived from `workspaces.mutation_policy = 'read_only'`. When true the
    /// workspace blocks direct workspace-owned content mutations (design
    /// [I2b]); enforcement lives in the mutation guard, not this list item.
    pub read_only: bool,
    /// True when this workspace is a review workspace: a reviewer-owned copy
    /// created through the review flow (a completed `workspace_forks` row
    /// keyed with `workspace_review_service::REVIEW_FORK_KEY_PREFIX`).
    pub is_review: bool,
    pub agent_review: Option<crate::services::worker_host_service::AgentReviewInventoryLink>,
    /// Immutable registry kind (`g5.workspace.kind`).
    pub kind: g5_contracts::types::WorkspaceKind,
    /// Current exclusive write authority. `created_by` is provenance only.
    pub owner_user_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub cloned_from_id: Option<Uuid>,
    pub branches: Vec<BranchSummary>,
    pub tasks_completed: i64,
    pub total_tasks: i64,
    pub running_time: Option<String>,
    pub cost: f64,
    pub concurrent_tasks: i64,
    pub last_updated: String,
    pub created_at: String,
    /// The workspace's active primary external issue, from the association
    /// alone (no tracker read).
    pub primary_issue: Option<WorkspacePrimaryIssue>,
    /// Workbench phase (`be` | `se` | `ie`; workbench `g5.workbench.phases.state`).
    pub phase: Option<String>,
}

/// Identity and display data of a workspace's primary external issue.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrimaryIssue {
    #[serde(skip)]
    pub workspace_id: Uuid,
    pub link_id: Uuid,
    pub display_key: String,
    pub canonical_url: String,
    pub locator: serde_json::Value,
}

/// What accepting a Base repository's provider history would do
/// (`g5.workspace.remote.base-provider-rewrite`). The owner reviews these exact
/// tips before confirming.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHistoryPreview {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub remote_branch: String,
    /// The workspace's current head, which adopting replaces.
    pub head: String,
    /// The provider tip the workspace would adopt.
    pub provider_tip: String,
    /// Commits on the head that the provider tip does not contain.
    pub discarded_commits: usize,
    /// The discarded history is only the repository's initial commit, so
    /// adopting is the recommended reconciliation.
    pub initial_only: bool,
    pub sync_paused_reason: Option<String>,
}

/// Captured inputs shared by preview and acceptance.
struct ProviderHistoryCapture {
    row: CoordinatedWorkspaceRepoRow,
    authority_inventory: g5_contracts::types::CommittedBindingSnapshot,
    preview: ProviderHistoryPreview,
}

/// Result of an explicit Publish of a local-only branch child to a new remote
/// ref (Option 2, G5 #1198). Ontology
/// `software_architecture/g5-api/services/workspace_service` D40.
#[derive(Debug, Clone, Serialize)]
pub struct PublishBranchRepoResult {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub remote_branch: String,
    pub head_commit: String,
}

/// Per-branch summary within a workspace list item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummary {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub tracking_mode: String,
    pub head_commit: String,
    pub materialization_status: String,
    /// Commits this branch is ahead of the comparison workspace's branch
    /// (merge target first, clone parent fallback). `None` = unknown,
    /// serialized as JSON `null`; only the workspace-detail path computes a
    /// real value and no surface may fabricate a `0` (ontology D50, issue #1995).
    pub divergence: Option<i32>,
    /// Commits this branch is behind the comparison workspace's branch. Same
    /// nullable contract as `divergence`; behind and ahead are independent
    /// axes and are never collapsed into one boolean (ontology D50).
    pub behind_base: Option<i32>,
    /// True when this branch has outstanding draft edits. Clean active live workspace file
    /// draft sessions do not count as user-visible draft work.
    pub has_draft: bool,
    pub dirty_file_count: i32,
    /// Remote-side branch name. `Some` only for `tracking_mode = "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    /// User-supplied local branch name (or defaulted from `remote_branch`).
    /// `Some` only for `tracking_mode = "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_branch_name: Option<String>,
    /// Provider repository full name surfaced for the tracked repo chip:
    /// `repos.provider_full_name` (e.g. `octocat/hello-world`) when present,
    /// else `owner/repo` parsed from the repo `git_url`. `Some` only for
    /// `tracking_mode = "remote_tracking"`; lets the client render the remote
    /// identity (`remote_repo_name : remote_branch`) instead of the G5-internal
    /// `org/repo_name : canonical_local_name`. Per `remote_repo_tracking.pm_view`
    /// I1/I5/D1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_repo_name: Option<String>,
    /// Other workspaces in the same org tracking the same
    /// `(repo_id, remote_branch)` pair. `Some` (possibly empty) only for
    /// `tracking_mode = "remote_tracking"`. Per `designs/REMOTE_REPO_TRACKING.md`
    /// §3.1.5 and PM view D17.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_tracking_workspaces: Option<Vec<CoTrackingWorkspace>>,
    /// Number of peer workspaces in the same org tracking the same
    /// `(repo_id, remote_branch)` pair. `Some` only for workspace-detail
    /// remote-tracking branches; create-time responses use
    /// `co_tracking_workspaces` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_tracking_workspace_count: Option<i64>,
    /// Option 1 (G5 #1198): authoritative per-child passive auto-pull setting.
    /// `Some` only for `tracking_mode = "remote_tracking"` children — the only
    /// children for which the flag is meaningful. The client treats this server
    /// value as authoritative after refresh so an optimistic toggle never
    /// masquerades as confirmed state. Ontology
    /// `software_architecture/g5-api/services/workspace_service` D26.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_pull_enabled: Option<bool>,
    /// Option 2 (G5 #1198): `Some(true)` when this is a local-only/pinned branch
    /// whose fork parent (`cloned_from_branch_id`) had this repo `remote_tracking`
    /// — i.e. the branch can be Published to a new remote ref. `None`/absent
    /// otherwise, so the client offers Publish only for provenance-eligible
    /// branches. Ontology `software_architecture/g5-api/services/workspace_service` D26.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_eligible: Option<bool>,
}

/// One entry in the `co_tracking_workspaces` advisory for a remote-tracked
/// branch. Surfaced at workspace create to make multi-workspace tracking
/// visible to the operator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoTrackingWorkspace {
    pub workspace_id: Uuid,
    pub workspace_name: String,
}

/// Repo metadata and commit context used by grounding trace lookups.
#[derive(Debug, Clone)]
pub struct WorkspaceRepoContext {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub repo_slug: String,
    pub branch_name: String,
    pub commit: String,
}

/// The outcome of a rename: the current name and the names the workspace
/// still reserves, newest first (`g5.workspace.former-names`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRename {
    pub id: Uuid,
    pub name: String,
    pub former_names: Vec<String>,
}

/// Detailed workspace view (used for get/create/update responses).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDetail {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub is_base: bool,
    /// Immutable registry kind (`g5.workspace.kind`).
    pub kind: g5_contracts::types::WorkspaceKind,
    /// Derived from `workspaces.mutation_policy = 'read_only'` (design [I2b]).
    pub read_only: bool,
    /// Raw `workspaces.mutation_policy_transition` — `Some("freezing")` while a
    /// read-write → read-only drain is in progress, else `None`. Distinct from
    /// `read_only` (a freezing workspace is still `read_only:false`). Lets the
    /// client show a Freezing indicator and offer "Make editable" for any
    /// not-effectively-read-write state (routes/workspaces [D30]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation_policy_transition: Option<String>,
    pub description: Option<String>,
    pub cloned_from_id: Option<Uuid>,
    pub docs_repo_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
    pub branches: Vec<BranchSummary>,
    /// Dedicated workspace docs folder, always `docs/<id>` (derived convention,
    /// never stored — flat under docs/ and immune to workspace rename).
    pub docs_folder_path: String,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspaceDeleteFlushRow {
    flush_id: Uuid,
    required_repo_ids: Vec<Uuid>,
}

/// Single source of the dedicated workspace docs folder convention:
/// `docs/<workspace_id>` (lowercase hyphenated UUID). The path is derived from
/// the immutable workspace id, so it needs no persistence, cannot collide, and
/// never changes on rename. Git folders are implicit, so the folder
/// materializes when the first doc is written into it.
pub fn workspace_docs_folder_path(workspace_id: Uuid) -> String {
    format!("docs/{workspace_id}")
}

/// Minimal docs repository ownership metadata for a workspace.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceDocsOwner {
    pub docs_repo_id: Option<Uuid>,
}

/// Immutable source snapshots selected for workspace source download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSourceDownloadArchive {
    pub workspace_name: String,
    pub repos: Vec<RepoBindingVersion>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCostBreakdown {
    pub task_id: Uuid,
    pub task_name: String,
    pub task_type: String,
    pub cost_usd: f64,
}

/// Workspace statistics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStats {
    pub total_cost_usd: f64,
    pub tasks_completed: i64,
    pub total_tasks: i64,
    pub concurrent_tasks: i64,
    /// Tasks in the `cancelled` terminal state — the status a chat/task lands in
    /// when a user stops it. Surfaced so the Activity card can account for
    /// stopped work (it is neither `completed` nor `running`).
    pub tasks_stopped: i64,
    /// Up to 100 task costs from the usage ledger, highest spend first.
    pub by_task: Vec<TaskCostBreakdown>,
    pub branches: Vec<BranchDivergence>,
}

/// Per-branch divergence info.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDivergence {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    /// `None` = unknown (JSON `null`). The stats path computes no divergence
    /// and always reports `None` — never a fabricated `0` (ontology D50).
    pub ahead_of_base: Option<i32>,
    /// Same nullable contract as `ahead_of_base` (ontology D50).
    pub behind_base: Option<i32>,
}

/// Detailed branch info (for list_branches and add_branch responses).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBranchDetail {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub head_commit: String,
    pub tracking_mode: String,
    pub materialization_status: String,
    /// True when this branch has outstanding draft edits.
    pub has_draft: bool,
    pub dirty_file_count: i32,
    pub dirty_files: Vec<String>,
    /// Remote-side branch name. `Some` only when `tracking_mode == "remote_tracking"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    /// Effective local branch name. `Some` only when `tracking_mode == "remote_tracking"`.
    /// Mirrors `branch_name` but surfaced separately so the UI can distinguish
    /// the user-chosen local name from the remote ref name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_branch_name: Option<String>,
    /// Other workspaces in the same org tracking the same `(repo_id, remote_branch)`
    /// pair. Empty (or `Some(vec![])`) when there is no co-tracking peer; `Some`
    /// only for remote-tracking branches. UI surfaces this as the co-tracking
    /// advisory at create time (per
    /// [`remote_repo_tracking.pm_view.md` D4 + D17](../../../../views/pm_views/remote_repo_tracking.pm_view.md)).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co_tracking_workspaces: Option<Vec<CoTrackingWorkspace>>,
}

/// Result of a workspace sync operation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub branches_updated: i32,
    pub branches_skipped: i32,
    pub details: Vec<BranchSyncDetail>,
}

/// Per-branch sync detail.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSyncDetail {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub status: String,
    pub old_commit: Option<String>,
    pub new_commit: Option<String>,
    /// The half-merged workspace PR holding this member's auto-pull. An
    /// explicit pull imports the member anyway, alone
    /// (`g5.governance.remote.pull`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_by: Option<crate::services::remote_integrations::ImportHold>,
}

/// Workspace-level coordinated git operation result.
///
/// This is additive over legacy sync/push DTOs: older clients keep reading the
/// existing fields, while coordinated repo sync clients get per-repo evidence.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatedOperationResult {
    pub operation: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub repos: Vec<CoordinatedRepoEvidence>,
}

/// Per-repo evidence for a coordinated workspace operation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatedRepoEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    pub repo_id: Uuid,
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

/// Result of promoting an existing branch repo child to remote tracking.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteRemoteTrackingResult {
    pub branch: BranchSummary,
    pub coordinated: CoordinatedOperationResult,
    /// Outcome discriminator (D46). `"bound"` is the ordinary [D28]
    /// equal/remote-ahead promotion; `"published"` is the Make-Remote
    /// absent-ref create-and-bind (D46 item 2); `"merge_required"` is the
    /// Make-Remote divergent/local-ahead merge routing (D46 item 3).
    pub outcome: String,
    /// Present only for the `"merge_required"` outcome: the synchronously
    /// opened remote-tracking merge run the UI routes into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_run_id: Option<Uuid>,
}

/// Result of a workspace-level push enqueue.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushToRemoteResult {
    pub enqueued: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinated: Option<CoordinatedOperationResult>,
}

/// Result of a workspace-level approved reset-below-baseline publication enqueue.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedRebasePublishResult {
    pub enqueued: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinated: Option<CoordinatedOperationResult>,
}

/// Result of a workspace merge operation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub branches_merged: i32,
    pub branches_failed: i32,
    pub details: Vec<BranchMergeDetail>,
    #[serde(skip)]
    pub authority_advance: Option<PreparedBranchCommit>,
}

/// Per-branch merge detail.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchMergeDetail {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub branch_name: String,
    pub status: String,
    pub merge_commit: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MergeSourceIntoTargetOutcome {
    Updated(String),
    SkippedUnmaterialized,
}

/// Workspace manifest used to materialize a local `.g5/workspace.yaml`.
///
/// Field names intentionally follow the on-disk YAML schema rather than the
/// public camelCase API DTO style.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceManifest {
    pub workspace_id: String,
    pub remote: WorkspaceManifestRemote,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<WorkspaceManifestDocs>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<WorkspaceManifestRepo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceManifestRemote {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceManifestDocs {
    pub repo_url: String,
    pub repo_id: Uuid,
    pub branch: String,
    pub tracking_mode: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceManifestRepo {
    pub name: String,
    pub url: String,
    pub repo_id: Uuid,
    pub branch: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Upstream remote ref name (e.g. `main`) for `remote_tracking` repos.
    /// `branch` above stays the canonical local name; this is the remote-side
    /// label a developer clone may opt to check out (`g5d clone
    /// --checkout-remote-names`). `None` for non-remote-tracking repos. (D15)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    /// Provider repository full name (e.g. `octocat/hello-world`) for
    /// `remote_tracking` repos, for display of the remote identity. `None`
    /// for non-remote-tracking repos. (D15)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_repo_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal row types for sqlx queries
// ---------------------------------------------------------------------------

/// Row type for the workspace list stats lateral join query.
#[derive(Debug, Clone, sqlx::FromRow)]
struct WorkspaceListRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    is_base: bool,
    /// Present in the list SELECTs as `(w.mutation_policy = 'read_only')`.
    /// `#[sqlx(default)]` keeps any query that omits the column decoding to
    /// `false` (read-write) rather than erroring at runtime.
    #[sqlx(default)]
    read_only: bool,
    /// Present in the list SELECTs as an EXISTS over `workspace_forks` rows
    /// whose idempotency key carries the review-workspace prefix
    /// (`workspace_review_service::REVIEW_FORK_KEY_PREFIX`), i.e. this
    /// workspace is a reviewer-owned copy of another workspace.
    #[sqlx(default)]
    is_review: bool,
    owner_user_id: Option<Uuid>,
    created_by: Option<Uuid>,
    cloned_from_id: Option<Uuid>,
    #[sqlx(default)]
    personal_user_id: Option<Uuid>,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    cost: f64,
    total_tasks: i64,
    tasks_completed: i64,
    concurrent_tasks: i64,
    /// Workbench phase (`be` | `se` | `ie`); `#[sqlx(default)]` keeps a query
    /// that omits the column decoding to `None` rather than erroring.
    #[sqlx(default)]
    phase: Option<String>,
}

/// Structural repository membership plus a request-scoped Git observation.
use crate::services::git::remote_integrations::WorkspaceRepositoryRow;

/// Resolved merge destination for a workspace: the live explicit
/// `merge_target_workspace_id` override when present, else the clone parent.
#[derive(Debug, Clone, Copy)]
struct MergeDestination {
    destination_id: Option<Uuid>,
    org_id: Uuid,
}

/// Clone branch pairing derived by explicit clone lineage, with a narrow
/// legacy branch-name fallback for older rows.
#[derive(Debug, Clone, sqlx::FromRow)]
struct CloneBranchPairRow {
    repo_id: Uuid,
    repo_name: String,
    child_workspace_id: Uuid,
    child_branch_name: String,
    child_head_commit: String,
    parent_workspace_id: Option<Uuid>,
    parent_branch_name: Option<String>,
    parent_head_commit: Option<String>,
}

fn exact_workspace_operation_repos(
    snapshot: &CommittedBindingSnapshot,
    prepared_heads: &BTreeMap<Uuid, String>,
) -> Result<Vec<WorkspaceHeadRepoInput>, G5Error> {
    let mut unmatched_repos = prepared_heads.keys().copied().collect::<BTreeSet<_>>();
    let repos = snapshot
        .repos
        .iter()
        .map(|repo| {
            let proposed_new_head = prepared_heads
                .get(&repo.repo_id)
                .map(|head| {
                    unmatched_repos.remove(&repo.repo_id);
                    head.clone()
                })
                .unwrap_or_else(|| repo.head_commit.clone());
            WorkspaceHeadRepoInput {
                repo_id: repo.repo_id,
                expected_old_head: repo.head_commit.clone(),
                proposed_new_head,
            }
        })
        .collect::<Vec<_>>();

    if !unmatched_repos.is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Prepared heads are not members of the captured authority cut",
                "target": snapshot.target,
                "repo_ids": unmatched_repos,
            }),
        });
    }
    Ok(repos)
}

use crate::services::git::remote_integrations::CoordinatedWorkspaceRepoRow;

use crate::services::git::remote_integrations::EligiblePrSourceRow;

struct TrackedPrSourcePushInput {
    source_workspace_id: Uuid,
    repo_id: Uuid,
    git_url: String,
    local_branch: String,
    remote_branch: String,
    head: String,
    actor_user_id: Uuid,
}

use crate::services::branch_integration_service::TipRelation as CoordinatedTipRelation;

/// Result from reading file content.
pub struct FileContentResult {
    pub content: Vec<u8>,
    pub size: i64,
    pub draft_dirty: bool,
}

// ---------------------------------------------------------------------------
// Mutation-policy transition (live read-write <-> read-only)
// ---------------------------------------------------------------------------
//
// Orchestration for `POST /workspaces/:id/mutation-policy`
// (spec `software_architecture/g5-api/services/workspace_service` [I5]/[I6]/[I7],
// `software_architecture/g5-api/routes/workspaces` [I5c],
// design `read-only-workspace-transition-design.md`).

/// The requested target mutation policy for a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMutationPolicy {
    /// Transition the workspace to read-only (`read_write` → `read_only`,
    /// two-phase freeze/drain/commit).
    ReadOnly,
    /// Transition the workspace to read-write (`read_only` → `read_write`,
    /// instantaneous; also reverts an in-progress `freezing` drain).
    ReadWrite,
}

/// Concurrency mode for a read-write → read-only transition (RW→RO only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMode {
    /// Freeze admission, drain already-running work up to the timeout, then
    /// commit; fail-safe revert on timeout (spec [I5], resolved O3). Default.
    Drain,
    /// Refuse up front with a structured blocker list if the workspace is not
    /// already quiescent; never enters `freezing` (host-rotation "refuse if
    /// busy" posture).
    RequireQuiescent,
}

/// Service command for a live workspace mutation-policy transition.
#[derive(Debug, Clone, Copy)]
pub struct MutationPolicyTransitionCommand {
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub actor: Uuid,
    pub target: TargetMutationPolicy,
    pub mode: TransitionMode,
    pub drain_timeout_secs: u64,
}

/// One outstanding non-terminal execution preventing a transition.
/// Returned on a `require_quiescent` refusal or a drain-timeout revert.
/// Tasks can block; an attached editor alone does not.
#[derive(Debug, Clone, Serialize)]
pub struct TransitionBlocker {
    /// Blocker class: `"task"`.
    pub kind: String,
    /// Identifier of the blocking task.
    pub id: String,
    /// Human-readable detail for the operator, including what to do about it.
    pub detail: String,
}

/// Result of a mutation-policy transition attempt.
#[derive(Debug, Clone)]
pub struct TransitionOutcome {
    /// Resulting `workspaces.mutation_policy` (`"read_write"` / `"read_only"`).
    pub policy: String,
    /// Resulting `workspaces.mutation_policy_transition` (`None` in every steady
    /// state, `Some("freezing")` while a read-write → read-only drain proceeds in
    /// the background).
    pub transition: Option<String>,
    /// Whether the requested transition completed (committed / idempotent no-op).
    /// `false` means the workspace was left unchanged and `blockers` explains why
    /// (require_quiescent refusal, up-front precondition reject, or drain-timeout
    /// revert). A `false` outcome with `transition == Some("freezing")` is NOT a
    /// refusal — it means the async drain was accepted and is running.
    pub committed: bool,
    /// Outstanding blockers when `committed == false`.
    pub blockers: Vec<TransitionBlocker>,
    /// Count of tasks that terminalized during the drain window.
    pub drained_tasks: i64,
    /// Human-readable one-line summary of the outcome (committed / draining /
    /// refused-with-reason) so the UI never synthesizes an explanation from a
    /// bare status code (spec [I5c]).
    pub message: String,
}

struct ReadOnlyCommitContext<'a> {
    authz: &'a Arc<dyn AuthzService>,
    tenant_pool: &'a PgPool,
    event_bus: &'a EventBus,
    command: MutationPolicyTransitionCommand,
}

struct DrainContext {
    authz: Arc<dyn AuthzService>,
    tenant_pool: PgPool,
    event_bus: EventBus,
    command: MutationPolicyTransitionCommand,
    baseline: i64,
    transition_id: Uuid,
}

// ---------------------------------------------------------------------------
// WorkspaceService
// ---------------------------------------------------------------------------

/// Stateless service for workspace lifecycle management.
///
/// All methods receive a `&PgPool` parameter. The `repo_storage` backend
/// is kept on the struct since it is environment-scoped, not per-request.
pub struct WorkspaceService {
    repo_storage: TenantRepoStorage,
    system_pool: PgPool,
    tenant_pool: PgPool,
    authz: Arc<dyn AuthzService>,
    /// Repository-scoped credentials shared by explicit and background
    /// provider operations. Credential source selection stays behind it.
    repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    /// The approval policy direct merges admit under, as pull requests do.
    approval_policy: g5_contracts::types::ApprovalPolicy,
}

impl WorkspaceService {
    /// Create a new `WorkspaceService` with the given storage backend.
    pub fn new(
        repo_storage: TenantRepoStorage,
        system_pool: PgPool,
        tenant_pool: PgPool,
        authz: Arc<dyn AuthzService>,
        repository_credentials: Arc<crate::auth::RepositoryCredentials>,
    ) -> Self {
        Self {
            repo_storage,
            system_pool,
            tenant_pool,
            authz,
            repository_credentials,
            approval_policy: g5_contracts::types::ApprovalPolicy::default(),
        }
    }

    /// Admit direct merges under the deployment's approval policy.
    pub fn with_approval_policy(mut self, policy: g5_contracts::types::ApprovalPolicy) -> Self {
        self.approval_policy = policy;
        self
    }

    fn transition_internal_error(context: &str, error: impl std::fmt::Display) -> G5Error {
        G5Error::Internal {
            message: format!("{context}: {error}"),
        }
    }

    async fn scoped_transition_db_for(
        authz: &Arc<dyn AuthzService>,
        tenant_pool: &PgPool,
        org_id: Uuid,
        actor: Uuid,
    ) -> Result<ScopedDb, G5Error> {
        let scopes = ["workspaces:write".to_string()];
        let capability = authz
            .mint_service_db_capability(UserId::from_uuid(actor), OrgId::from_uuid(org_id), &scopes)
            .await
            .map_err(|e| {
                Self::transition_internal_error("Failed to mint transition DB scope", e)
            })?;

        ScopedDb::from_capability(tenant_pool, capability)
            .await
            .map_err(|e| Self::transition_internal_error("Failed to open transition DB scope", e))
    }

    async fn scoped_transition_db(&self, org_id: Uuid, actor: Uuid) -> Result<ScopedDb, G5Error> {
        Self::scoped_transition_db_for(&self.authz, &self.tenant_pool, org_id, actor).await
    }

    /// Ownership and transfer must be read after obtaining the write lock, not
    /// from the snapshot of a statement that may wait behind a transfer.
    async fn lock_policy_authority(db: &mut ScopedDb, workspace_id: Uuid) -> Result<(), G5Error> {
        let row: Option<(bool, Option<Uuid>)> = sqlx::query_as(
            "SELECT is_base, agent_approval_review_id FROM workspaces
             WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?;
        let (is_base, review_id) = row.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        crate::services::workspace_mutation_policy::ensure_workspace_owner(db, workspace_id)
            .await?;
        let actor = db.actor_id().into_uuid();
        let org = db
            .primary_org_id()
            .map_err(|e| Self::transition_internal_error("Policy organization", e))?
            .into_uuid();
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            actor,
            org,
            false,
        )
        .await?;
        sqlx::query("SELECT g5_require_workspace_not_deleting($1)")
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        if is_base || review_id.is_some() {
            return Err(G5Error::WorkspaceReadOnly {
                operation: "workspaces.transition_mutation_policy",
                workspace_id: workspace_id.to_string(),
            });
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Workspace CRUD
    // -----------------------------------------------------------------------

    /// Capture phase for workspace open-question metrics (workspace_service
    /// D46): every tenant-DB read — access checks and Project Doc sources —
    /// happens here, inside the caller's transaction. The route commits
    /// before handing the captured value to `open_question_metrics`, so
    /// Git document I/O never holds tenant DB resources.
    pub async fn capture_open_question_metrics(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_user_id: Uuid,
        workspace_ids: &[Uuid],
        doc_service: &DocService,
    ) -> Result<CapturedWorkspaceOpenQuestionMetrics, AppError> {
        for workspace_id in workspace_ids {
            crate::db::scoped::ensure_user_workspace_access(
                &self.system_pool,
                db,
                actor_user_id,
                org_id,
                *workspace_id,
            )
            .await?;
        }

        let expected_unborn_workspace_ids =
            expected_unborn_workspace_authority_cuts(db, &self.repo_storage, org_id, workspace_ids)
                .await
                .map_err(AppError::from_error)?;

        let mut workspaces = Vec::with_capacity(workspace_ids.len());
        for workspace_id in workspace_ids {
            let project_docs = if expected_unborn_workspace_ids.contains(workspace_id) {
                tracing::debug!(
                    event = "organization_projection_source_omitted",
                    workspace_id = %workspace_id,
                    reason = "unborn_authority_cut",
                    source = "project_docs",
                    projection = "open_question_metrics",
                );
                None
            } else {
                Some(doc_service.capture_docs_list(db, *workspace_id).await?)
            };
            workspaces.push(CapturedWorkspaceOpenQuestionInputs {
                workspace_id: *workspace_id,
                project_docs,
            });
        }
        Ok(CapturedWorkspaceOpenQuestionMetrics {
            actor_user_id,
            workspaces,
        })
    }

    /// Capture the caller-owned inputs for Home. Document identities retain
    /// their exact captured authority cuts for post-transaction scanning.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn capture_home_question_candidates(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_user_id: Uuid,
        workspace_ids: &[Uuid],
        cursor: Option<&HomeAttentionCursor>,
        order: HomeAttentionOrder,
        candidate_limit: usize,
        kind: Option<HomeAttentionKind>,
        doc_service: &DocService,
    ) -> Result<CapturedHomeQuestionCandidates, AppError> {
        debug_assert_ne!(kind, Some(HomeAttentionKind::Approval));
        let valid_workspace_ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM workspaces
            WHERE org_id = $1
              AND id = ANY($2)
              AND owner_user_id = $3
              AND deleted_at IS NULL
            ORDER BY id
            "#,
        )
        .bind(org_id)
        .bind(workspace_ids)
        .bind(actor_user_id)
        .fetch_all(db.conn())
        .await?;

        let mut project_docs = Vec::with_capacity(valid_workspace_ids.len());
        {
            let expected_unborn_workspace_ids = expected_unborn_workspace_authority_cuts(
                db,
                &self.repo_storage,
                org_id,
                &valid_workspace_ids,
            )
            .await
            .map_err(AppError::from_error)?;
            for workspace_id in valid_workspace_ids {
                if expected_unborn_workspace_ids.contains(&workspace_id) {
                    tracing::debug!(
                        event = "organization_projection_source_omitted",
                        workspace_id = %workspace_id,
                        reason = "unborn_authority_cut",
                        source = "project_docs",
                        projection = "home_attention",
                    );
                    continue;
                }
                project_docs.push(CapturedHomeProjectDocs {
                    workspace_id,
                    captured: doc_service.capture_docs_list(db, workspace_id).await?,
                });
            }
        }

        Ok(CapturedHomeQuestionCandidates {
            project_docs,
            actor_user_id,
            cursor: cursor.cloned(),
            order,
            candidate_limit,
        })
    }

    /// Materialize at most one Home page budget for Project Docs, walking
    /// workspaces and their sources in the requested page order.
    pub(crate) async fn materialize_home_question_candidates(
        &self,
        mut captured: CapturedHomeQuestionCandidates,
        doc_service: &DocService,
        runtime: &WorkspaceRuntime,
    ) -> Result<HomeQuestionCandidateSets, AppError> {
        let order = captured.order;
        captured.project_docs.sort_by(|left, right| {
            order.compare(
                &(left.captured.docs_cut_at(), left.workspace_id),
                &(right.captured.docs_cut_at(), right.workspace_id),
            )
        });
        // Within one workspace every doc identity (`open_question:`) orders
        // before every set identity (`open_question_set:`), so reading the
        // sources in this sequence keeps the stream a prefix of the Home
        // order: docs first when oldest first, sets first when newest first.
        let source_sequence = match order {
            HomeAttentionOrder::Oldest => [HomeQuestionSource::Docs, HomeQuestionSource::Sets],
            HomeAttentionOrder::Newest => [HomeQuestionSource::Sets, HomeQuestionSource::Docs],
        };
        let mut project_docs = Vec::with_capacity(captured.candidate_limit);
        let mut question_sets = Vec::new();
        for input in captured.project_docs {
            if project_docs.len() + question_sets.len() == captured.candidate_limit {
                break;
            }
            if captured.cursor.as_ref().is_some_and(|cursor| {
                cursor.passes_prefix(input.captured.docs_cut_at(), 1, input.workspace_id)
            }) {
                continue;
            }
            // Design v9 §9.1: the attached owner's own counts read the live
            // docs checkout; Home lists only the caller's workspaces.
            let live = doc_service
                .live_docs_overlay(
                    runtime,
                    captured.actor_user_id,
                    input.workspace_id,
                    &input.captured,
                )
                .await;
            let docs_cut_at = input.captured.docs_cut_at();
            for source in source_sequence {
                let remaining = captured.candidate_limit - project_docs.len() - question_sets.len();
                if remaining == 0 {
                    break;
                }
                match source {
                    HomeQuestionSource::Docs => {
                        let sources = doc_service
                            .open_project_doc_question_sources_bounded(
                                input.captured.clone(),
                                live.clone(),
                                captured.cursor.as_ref(),
                                order,
                                remaining,
                            )
                            .await?;
                        project_docs.extend(sources.into_iter().map(|source| {
                            HomeProjectDocQuestionSource {
                                workspace_id: input.workspace_id,
                                source,
                            }
                        }));
                    }
                    // Question-set items: one per set with open entries and a
                    // referencing Project Doc to open.
                    HomeQuestionSource::Sets => {
                        let families = doc_service
                            .open_project_doc_question_families(
                                input.captured.clone(),
                                live.clone(),
                            )
                            .await?;
                        let mut sets =
                            crate::services::question_set_service::QuestionSetService::new(
                                doc_service,
                                runtime,
                            )
                            .open_question_set_sources(
                                &input.captured,
                                input.workspace_id,
                                Some(captured.actor_user_id),
                                &families.references,
                            )
                            .await?
                            .into_iter()
                            .filter(|source| source.open_question_count > 0)
                            .filter_map(|source| {
                                let referencing_doc_id = source.referenced_by.first()?.clone();
                                let item_id = format!(
                                    "open_question_set:{}:{}",
                                    input.workspace_id, source.path
                                );
                                captured
                                    .cursor
                                    .as_ref()
                                    .is_none_or(|cursor| {
                                        cursor.is_after(
                                            docs_cut_at,
                                            1,
                                            input.workspace_id,
                                            &item_id,
                                        )
                                    })
                                    .then_some(HomeQuestionSetSource {
                                        workspace_id: input.workspace_id,
                                        source,
                                        referencing_doc_id,
                                        actionable_at: docs_cut_at,
                                    })
                            })
                            .collect::<Vec<_>>();
                        sets.sort_by(|left, right| {
                            order.compare(&left.source.path, &right.source.path)
                        });
                        sets.truncate(remaining);
                        question_sets.extend(sets);
                    }
                }
            }
        }

        Ok(HomeQuestionCandidateSets {
            project_docs,
            question_sets,
        })
    }

    /// Materialize workspace metrics only after the capture transaction has
    /// closed, so Git document reads cannot hold tenant DB resources (the
    /// capture/materialize seam).
    pub async fn open_question_metrics(
        &self,
        captured: CapturedWorkspaceOpenQuestionMetrics,
        doc_service: &DocService,
        runtime: &WorkspaceRuntime,
    ) -> Result<Vec<WorkspaceOpenQuestionMetrics>, AppError> {
        let mut metrics = Vec::with_capacity(captured.workspaces.len());
        for workspace in captured.workspaces {
            let (project_doc_sources, question_set_sources) = match workspace.project_docs {
                Some(project_docs) => {
                    // Design v9 §9.1: an attached owner counts the questions
                    // in the live docs checkout; every other caller counts
                    // the committed cut.
                    let live = doc_service
                        .live_docs_overlay(
                            runtime,
                            captured.actor_user_id,
                            workspace.workspace_id,
                            &project_docs,
                        )
                        .await;
                    let families = doc_service
                        .open_project_doc_question_families(project_docs.clone(), live)
                        .await?;
                    let sets = crate::services::question_set_service::QuestionSetService::new(
                        doc_service,
                        runtime,
                    )
                    .open_question_set_sources(
                        &project_docs,
                        workspace.workspace_id,
                        Some(captured.actor_user_id),
                        &families.references,
                    )
                    .await?;
                    (families.project_docs, sets)
                }
                None => (Vec::new(), Vec::new()),
            };
            metrics.push(WorkspaceOpenQuestionMetrics {
                workspace_id: workspace.workspace_id,
                document_open_questions: project_doc_sources
                    .iter()
                    .map(|source| source.open_question_count)
                    .sum::<i64>()
                    + question_set_sources
                        .iter()
                        .map(|source| source.open_question_count)
                        .sum::<i64>(),
                question_sets_incomplete: question_set_sources.iter().any(|source| source.invalid),
                project_doc_sources,
                question_set_sources,
            });
        }
        Ok(metrics)
    }

    /// List all workspaces in an org with computed stats and branch info.
    ///
    /// Uses lateral joins to compute per-workspace aggregated stats from `tasks`
    /// and `tasks` tables.
    ///
    /// Unpaginated by design: the org's ordinary live-workspace set is returned in
    /// one statement — one MVCC snapshot — because `updated_at` is mutable, so
    /// offset paging over it can silently skip or duplicate rows and no page
    /// sequence reconstructs the true set. The `id DESC` tie-breaker on every
    /// branch keeps equal-`updated_at` rows in a stable order across refetches.
    /// Formal-review inventory rows are appended from a separate read and are
    /// not covered by that snapshot.
    pub async fn list_workspaces(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Vec<WorkspaceListItem>, G5Error> {
        let org_role = sqlx::query_scalar!(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
            org_id,
            actor_user_id
        )
        .fetch_optional(&self.system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load org role for workspace listing: {e}"),
        })?;
        let org_wide_access = matches!(org_role.as_deref(), Some("admin"));

        // Fetch workspaces with lateral join stats
        let workspace_rows: Vec<WorkspaceListRow> = sqlx::query_as::<_, WorkspaceListRow>(
            r#"
                SELECT
                    w.id,
                    w.name,
                    w.description,
                    w.is_base,
                    (w.mutation_policy = 'read_only') AS read_only,
                    EXISTS (
                        SELECT 1 FROM workspace_forks f
                        WHERE f.org_id = w.org_id
                          AND f.target_workspace_id = w.id
                          AND f.status = 'completed'
                          AND left(f.idempotency_key, 17) = 'review-workspace:'
                    ) AS is_review,
                    w.owner_user_id,
                    w.created_by,
                    w.cloned_from_id,
                    w.personal_user_id,
                    w.phase,
                    w.updated_at,
                    w.created_at,
                    COALESCE(j.cost_sum, 0.0) as cost,
                    COALESCE(t.total, 0) as total_tasks,
                    COALESCE(t.completed, 0) as tasks_completed,
                    COALESCE(t.active, 0) as concurrent_tasks
                FROM workspaces w
                LEFT JOIN LATERAL (
                    SELECT SUM(CAST(cost_usd AS DOUBLE PRECISION)) AS cost_sum
                    FROM usage_records WHERE workspace_id = w.id
                ) j ON TRUE
                LEFT JOIN LATERAL (
                    SELECT
                        COUNT(*) AS total,
                        COUNT(*) FILTER (WHERE t2.status = 'succeeded') AS completed,
                        COUNT(*) FILTER (WHERE t2.status = 'running') AS active
                    FROM tasks t2
                    WHERE t2.workspace_id = w.id
                ) t ON TRUE
                WHERE w.org_id = $1
                  AND w.deleted_at IS NULL
                  AND w.agent_approval_review_id IS NULL
                  AND authz.workspace_audience_allows(w.personal_user_id, w.owner_user_id,
                      w.agent_approval_review_id, $2, $3)
                ORDER BY w.updated_at DESC, w.id DESC
                "#,
        )
        .bind(org_id)
        .bind(actor_user_id)
        .bind(org_wide_access)
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to list workspaces: {e}"),
        })?;

        // Collect workspace IDs for batch branch + dependency fetching
        let workspace_ids: Vec<Uuid> = workspace_rows.iter().map(|w| w.id).collect();

        // Fetch materialized workspace branches in one batch.
        let branch_rows = self
            .load_workspace_repository_rows(db, &workspace_ids)
            .await?;
        let repo_meta = self
            .load_repo_chip_meta(db, branch_rows.iter().map(|row| row.repo_id))
            .await?;

        // Assemble WorkspaceListItem for each workspace
        // Active primary issues of the listed (already access-filtered)
        // workspaces.
        let mut primary_issues: HashMap<Uuid, WorkspacePrimaryIssue> = if workspace_ids.is_empty() {
            HashMap::new()
        } else {
            sqlx::query_as::<_, WorkspacePrimaryIssue>(
                r#"
                SELECT workspace_id, id AS link_id, display_key, canonical_url, locator
                FROM workspace_issue_links
                WHERE workspace_id = ANY($1) AND is_primary AND unlinked_at IS NULL
                "#,
            )
            .bind(&workspace_ids)
            .fetch_all(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load workspace primary issues: {e}"),
            })?
            .into_iter()
            .map(|issue| (issue.workspace_id, issue))
            .collect()
        };

        let items: Vec<WorkspaceListItem> = workspace_rows
            .into_iter()
            .map(|row| {
                let mut branches: Vec<BranchSummary> = branch_rows
                    .iter()
                    .filter(|b| b.workspace_id == row.id)
                    .map(|b| {
                        let meta = repo_meta.get(&b.repo_id);
                        // Surface the remote identity (remote_branch + remote_repo_name)
                        // for remote-tracking branches so the workspace selector
                        // dropdown can show it (per workspace_service D14). The
                        // co-tracking advisory count stays detail-only.
                        let is_remote_tracking = b.tracking_mode == "remote_tracking";
                        let (remote_branch, remote_repo_name) =
                            remote_identity_for(is_remote_tracking, b.remote_branch.clone(), meta);
                        BranchSummary {
                            workspace_id: b.workspace_id,
                            repo_id: b.repo_id,
                            repo_name: meta
                                .map(|m| m.name.clone())
                                .unwrap_or_else(|| "unknown".to_string()),
                            branch_name: b.branch_name.clone(),

                            tracking_mode: b.tracking_mode.clone(),
                            head_commit: b.head_commit.clone(),
                            materialization_status: b.materialization_status.clone(),
                            // The list path computes no divergence (N×M git
                            // calls per render); unknown is None, never a
                            // fabricated 0 (ontology D50, issue #1995).
                            divergence: None,
                            behind_base: None,
                            has_draft: b.has_draft,
                            dirty_file_count: b.dirty_file_count,
                            remote_branch,
                            local_branch_name: None,
                            remote_repo_name,
                            co_tracking_workspaces: None,
                            co_tracking_workspace_count: None,
                            auto_pull_enabled: None,
                            remote_eligible: None,
                        }
                    })
                    .collect();
                branches.sort_by(|left, right| {
                    left.repo_name
                        .cmp(&right.repo_name)
                        .then_with(|| left.branch_name.cmp(&right.branch_name))
                        .then_with(|| left.repo_id.cmp(&right.repo_id))
                });

                WorkspaceListItem {
                    id: row.id,
                    name: row.name,
                    description: row.description,
                    is_base: row.is_base,
                    read_only: row.read_only,
                    is_review: row.is_review,
                    agent_review: None,
                    kind: g5_contracts::types::WorkspaceKind::from_registry(
                        row.is_base,
                        row.personal_user_id,
                    ),
                    owner_user_id: row.owner_user_id,
                    created_by: row.created_by,
                    cloned_from_id: row.cloned_from_id,
                    branches,
                    tasks_completed: row.tasks_completed,
                    total_tasks: row.total_tasks,
                    running_time: None, // Not computed — placeholder for future
                    cost: row.cost,
                    concurrent_tasks: row.concurrent_tasks,
                    primary_issue: primary_issues.remove(&row.id),
                    phase: row.phase,
                    last_updated: row.updated_at.to_rfc3339(),
                    created_at: row.created_at.to_rfc3339(),
                }
            })
            .collect();

        let mut items: Vec<WorkspaceListItem> = items;
        let visibility = if org_wide_access {
            crate::services::worker_host_service::VmInventoryVisibility::OrgWide
        } else {
            crate::services::worker_host_service::VmInventoryVisibility::ReadableBy(actor_user_id)
        };
        for review in crate::services::worker_host_service::list_formal_review_workspaces(
            self.authz.as_ref(),
            &self.system_pool,
            org_id,
            visibility,
        )
        .await?
        {
            items.push(WorkspaceListItem {
                id: review.workspace_id,
                name: review.workspace_name,
                description: Some("Formal agent review".into()),
                is_base: false,
                read_only: review.read_only,
                is_review: true,
                agent_review: Some(review.agent_review),
                // A formal review workspace is never Base or personal.
                kind: g5_contracts::types::WorkspaceKind::Working,
                owner_user_id: review.owner_user_id,
                created_by: review.created_by,
                cloned_from_id: None,
                branches: vec![],
                tasks_completed: review.tasks_completed,
                total_tasks: review.total_tasks,
                concurrent_tasks: review.concurrent_tasks,
                running_time: None,
                cost: review.cost,
                primary_issue: None,
                phase: review.phase,
                last_updated: review.updated_at.to_rfc3339(),
                created_at: review.created_at.to_rfc3339(),
            });
        }
        items.sort_by(|a, b| b.last_updated.cmp(&a.last_updated).then(b.id.cmp(&a.id)));
        Ok(items)
    }

    /// Fetch a single workspace with branches, draft status, and dependencies.
    pub async fn get_workspace(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
    ) -> Result<WorkspaceDetail, G5Error> {
        // Fetch workspace
        let workspace = sqlx::query_as::<_, Workspace>(
            r#"
            SELECT id, org_id, name, is_base, description,
                   cloned_from_id, docs_repo_id,
                   owner_user_id, lifecycle_generation, push_mode,
                   mutation_policy,
                   mutation_policy_transition,
                   mutation_policy_updated_by, mutation_policy_updated_at,
                   merge_target_workspace_id, personal_user_id,
                   created_by, created_at, updated_at
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to get workspace: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        // Fetch branches with draft info
        let branches = canonical_workspace_summaries(
            self.fetch_branch_summaries(db, workspace_id, caller_user_id, None)
                .await?,
        );

        Ok(WorkspaceDetail {
            id: workspace.id,
            org_id: workspace.org_id,
            kind: workspace.kind(),
            name: workspace.name,
            is_base: workspace.is_base,
            read_only: workspace.mutation_policy == "read_only",
            mutation_policy_transition: workspace.mutation_policy_transition.clone(),
            description: workspace.description,
            cloned_from_id: workspace.cloned_from_id,
            docs_repo_id: workspace.docs_repo_id,
            created_by: workspace.created_by,
            created_at: workspace.created_at.to_rfc3339(),
            updated_at: workspace.updated_at.to_rfc3339(),
            branches,
            docs_folder_path: workspace_docs_folder_path(workspace.id),
        })
    }

    /// Fetch only docs repository ownership fields for a workspace.
    pub async fn get_workspace_docs_owner(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkspaceDocsOwner, G5Error> {
        sqlx::query_as::<_, WorkspaceDocsOwner>(
            "SELECT docs_repo_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to get workspace docs owner: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })
    }

    /// Resolve the non-doc code repo workspace snapshots for source download.
    pub async fn resolve_workspace_source_download(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkspaceSourceDownloadArchive, G5Error> {
        let (workspace_name, docs_repo_id) = sqlx::query_as::<_, (String, Option<Uuid>)>(
            "SELECT name, docs_repo_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to resolve workspace source download metadata: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        // The archive is one committed cut (`g5.fork.source.cut`).
        let snapshot = crate::services::workspace_reader::read_committed_cut(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
            "source_download",
        )
        .await?;

        workspace_source_download_from_snapshot(workspace_name, docs_repo_id, snapshot)
    }

    /// This is the contract consumed by `g5d clone` / `g5d refresh` to produce
    /// a local `.g5/workspace.yaml` and pinned repo checkouts.
    pub async fn get_workspace_manifest(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        external_url: &str,
        git_external_url: &str,
    ) -> Result<WorkspaceManifest, G5Error> {
        let workspace = sqlx::query_as::<_, Workspace>(
            r#"
            SELECT id, org_id, name, is_base, description,
                   cloned_from_id, docs_repo_id,
                   owner_user_id, lifecycle_generation, push_mode,
                   mutation_policy,
                   mutation_policy_transition,
                   mutation_policy_updated_by, mutation_policy_updated_at,
                   merge_target_workspace_id, personal_user_id,
                   created_by, created_at, updated_at
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to get workspace manifest: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        let git_base = git_external_url.trim_end_matches('/');
        let api_base = external_url.trim_end_matches('/');
        // Clients pin these commits as one checkout (`g5.fork.source.cut`).
        let scope = db.reopener();
        let snapshot = crate::services::workspace_reader::read_committed_cut(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
            "workspace_manifest",
        )
        .await?;
        let mut db = scope.open().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to open manifest metadata read: {error}"),
        })?;
        let repo_meta = self
            .load_repo_chip_meta(&mut db, snapshot.repos.iter().map(|repo| repo.repo_id))
            .await?;
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close manifest metadata read: {error}"),
        })?;
        if repo_meta.len() != snapshot.repos.len() {
            return Err(G5Error::Internal {
                message: format!(
                    "Manifest repository metadata is incomplete for workspace {workspace_id}"
                ),
            });
        }

        let manifest_repo = |repo: &RepoBindingVersion| -> Result<WorkspaceManifestRepo, G5Error> {
            let chip_meta = repo_meta
                .get(&repo.repo_id)
                .ok_or_else(|| G5Error::Internal {
                    message: format!(
                        "Manifest repository metadata is missing for repository {} in workspace {workspace_id}",
                        repo.repo_id
                    ),
                })?;
            let (remote_branch, remote_repo_name) = remote_identity_for(
                repo.tracking_mode == "remote_tracking",
                repo.remote_branch.clone(),
                Some(chip_meta),
            );
            Ok(WorkspaceManifestRepo {
                name: repo.repo_name.clone(),
                url: format!("{git_base}/git/{}", repo.repo_id),
                repo_id: repo.repo_id,
                branch: repo.branch_name.clone(),
                path: repo.root_path.clone(),
                commit: normalize_head_commit(&repo.head_commit),
                remote_branch,
                remote_repo_name,
            })
        };

        let mut repos = snapshot
            .repos
            .iter()
            .filter(|repo| Some(repo.repo_id) != workspace.docs_repo_id)
            .map(manifest_repo)
            .collect::<Result<Vec<_>, _>>()?;
        repos.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.path.cmp(&right.path))
        });

        let docs = if let Some(repo_id) = workspace.docs_repo_id {
            let repo = snapshot
                .repo(repo_id)
                .map_err(|_| G5Error::Internal {
                    message: format!(
                        "Workspace {workspace_id} docs repository {repo_id} is absent from its authoritative snapshot"
                    ),
                })?;
            Some(WorkspaceManifestDocs {
                repo_url: format!("{git_base}/git/{repo_id}"),
                repo_id: repo.repo_id,
                branch: repo.branch_name.clone(),
                tracking_mode: repo.tracking_mode.clone(),
                path: repo.root_path.clone(),
                commit: normalize_head_commit(&repo.head_commit),
            })
        } else {
            None
        };

        Ok(WorkspaceManifest {
            workspace_id: workspace.id.to_string(),
            remote: WorkspaceManifestRemote {
                enabled: true,
                api_url: Some(api_base.to_string()),
                project_id: None,
            },
            docs,
            repos,
        })
    }

    /// Read the explicit Save checkpoint in the caller's workspace capability.
    pub async fn source_control_generation(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<i64, G5Error> {
        Ok(
            sqlx::query_scalar("SELECT source_control_save_generation FROM workspaces WHERE id=$1")
                .bind(workspace_id)
                .fetch_one(db.conn())
                .await?,
        )
    }

    /// Serialize checkpoint writers before the incarnation's workspace/VM
    /// share fence. The caller proves the live incarnation before advancing.
    pub async fn lock_source_control_checkpoint(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<(), G5Error> {
        sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await?;
        Ok(())
    }

    pub async fn advance_source_control_generation(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<i64, G5Error> {
        Ok(sqlx::query_scalar("UPDATE workspaces SET source_control_save_generation=source_control_save_generation+1 WHERE id=$1 RETURNING source_control_save_generation")
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await?)
    }

    /// Move the workbench phase (workbench `g5.workbench.phases.state`): the
    /// owner's explicit forward or back move. Validates the literal, refuses
    /// Base (always `be`, like the other Base content mutations) and, when the
    /// phase actually changes, writes the new value together with its durable
    /// `workspace_phase_events` row in the caller's transaction. Runtime
    /// queries keep the column out of the compile-time `Workspace` row that
    /// the other paths share. Ownership is the route's concern.
    pub async fn set_workspace_phase(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        phase: &str,
        actor_user_id: Option<Uuid>,
    ) -> Result<(), G5Error> {
        if !WORKSPACE_PHASES.contains(&phase) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "phase",
                    "message": format!("Invalid phase '{phase}'. Must be one of: be, se, ie"),
                }),
            });
        }
        // Lock the row so two concurrent moves cannot both read the same
        // `from_phase` and record contradictory events for one transition.
        // Note: the route already holds `FOR SHARE` from `ensure_workspace_owner`,
        // so this upgrade can deadlock (40P01) under two concurrent PATCHes —
        // the same pre-existing pattern as every other `update_workspace`
        // field (push_mode); out of scope here.
        let (is_base, org_id, from_phase): (bool, Uuid, String) = sqlx::query_as(
            "SELECT is_base, org_id, phase FROM workspaces \
             WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        if is_base {
            return Err(G5Error::WorkspaceIsBase);
        }
        crate::services::workspace::workspace_kind::refuse_personal(
            db.conn(),
            workspace_id,
            "phases",
        )
        .await?;
        if from_phase == phase {
            return Ok(());
        }
        sqlx::query("UPDATE workspaces SET phase = $2, updated_at = now() WHERE id = $1")
            .bind(workspace_id)
            .bind(phase)
            .execute(db.conn())
            .await?;
        sqlx::query(
            "INSERT INTO workspace_phase_events \
             (org_id, workspace_id, from_phase, to_phase, actor_user_id) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(&from_phase)
        .bind(phase)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        Ok(())
    }

    /// The latest guided-journey mark of every step that was ever marked
    /// (workbench g5.workbench.journey.marks): the state is the newest
    /// event, never a second copy.
    pub async fn list_journey_marks(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Vec<JourneyMarkDTO>, G5Error> {
        // step_id, done, skipped, created_at, actor_user_id
        type MarkRow = (String, bool, bool, DateTime<Utc>, Option<Uuid>);
        let rows: Vec<MarkRow> = sqlx::query_as(
            "SELECT DISTINCT ON (step_id) step_id, done, skipped, created_at, actor_user_id \
             FROM workspace_journey_events \
             WHERE workspace_id = $1 \
             ORDER BY step_id, seq DESC",
        )
        .bind(workspace_id)
        .fetch_all(db.conn())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(step_id, done, skipped, marked_at, actor_user_id)| JourneyMarkDTO {
                    step_id,
                    done,
                    skipped,
                    marked_at,
                    actor_user_id,
                },
            )
            .collect())
    }

    /// Record the owner's "Mark done" / "Not done" for one guided-journey
    /// step (workbench g5.workbench.journey.marks). The route has already
    /// checked ownership and the mutation policy; this refuses Base,
    /// validates the step id's shape (the catalog lives in the client) and
    /// records nothing when the step already has that mark, so a retried
    /// request adds no event. Writes for one workspace are serialized on a
    /// transaction-scoped advisory lock rather than a row lock, because the
    /// route already holds the workspace row `FOR SHARE` and an upgrade to
    /// `FOR UPDATE` under two concurrent requests deadlocks.
    pub async fn set_journey_mark(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        step_id: &str,
        done: bool,
        skipped: bool,
        actor_user_id: Option<Uuid>,
    ) -> Result<(), G5Error> {
        if done && skipped {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "skipped",
                    "message": "A step is done or skipped, not both",
                }),
            });
        }
        if !g5_contracts::types::question_set::is_slug_segment(step_id.as_bytes(), 64) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "stepId",
                    "message": "A step id is 1–64 lowercase letters, digits and dashes",
                }),
            });
        }
        let (is_base, org_id): (bool, Uuid) = sqlx::query_as(
            "SELECT is_base, org_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        if is_base {
            return Err(G5Error::WorkspaceIsBase);
        }
        crate::services::workspace::workspace_kind::refuse_personal(
            db.conn(),
            workspace_id,
            "guided steps",
        )
        .await?;
        // Serialize this workspace's journey writes: two concurrent marks of
        // one step cannot both read "no mark yet", and the seq allocated
        // under the lock is the mark order (the index reads `seq DESC`).
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('journey:' || $1::text, 0))")
            .bind(workspace_id)
            .execute(db.conn())
            .await?;
        let current: Option<(bool, bool)> = sqlx::query_as(
            "SELECT done, skipped FROM workspace_journey_events \
             WHERE workspace_id = $1 AND step_id = $2 \
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(step_id)
        .fetch_optional(db.conn())
        .await?;
        // An unmarked step reads as to do: clearing it records nothing.
        if current.unwrap_or((false, false)) == (done, skipped) {
            return Ok(());
        }
        // `clock_timestamp()` under the advisory lock: the timeline orders by
        // `created_at`, so the stamp must follow the serialized write order,
        // not the transaction start (`now()`).
        sqlx::query(
            "INSERT INTO workspace_journey_events \
             (org_id, workspace_id, step_id, done, skipped, actor_user_id, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, clock_timestamp())",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(step_id)
        .bind(done)
        .bind(skipped)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        Ok(())
    }

    /// Partial update of workspace fields.
    pub async fn update_workspace(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
        req: UpdateWorkspaceRequest,
    ) -> Result<WorkspaceDetail, G5Error> {
        // Fetch existing workspace
        let existing = sqlx::query_as::<_, Workspace>(
            r#"
            SELECT id, org_id, name, is_base, description,
                   cloned_from_id, docs_repo_id,
                   owner_user_id, lifecycle_generation, push_mode,
                   mutation_policy,
                   mutation_policy_transition,
                   mutation_policy_updated_by, mutation_policy_updated_at,
                   merge_target_workspace_id, personal_user_id,
                   created_by, created_at, updated_at
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to get workspace for update: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        let new_description = req.description.or(existing.description);
        let now = Utc::now();

        let workspace = sqlx::query_as::<_, Workspace>(
            r#"
            UPDATE workspaces
            SET description = $2, updated_at = $3
            WHERE id = $1
              AND deleted_at IS NULL
            RETURNING id, org_id, name, is_base, description,
                      cloned_from_id, docs_repo_id,
                      owner_user_id, lifecycle_generation, push_mode,
                      mutation_policy,
                      mutation_policy_transition,
                      mutation_policy_updated_by, mutation_policy_updated_at,
                      merge_target_workspace_id, personal_user_id,
                      created_by, created_at, updated_at
            "#,
        )
        .bind(workspace_id)
        .bind(new_description.as_deref())
        .bind(now)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to update workspace: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        let branches = canonical_workspace_summaries(
            self.fetch_branch_summaries(db, workspace_id, caller_user_id, None)
                .await?,
        );
        info!(workspace_id = %workspace_id, "Updated workspace");

        Ok(WorkspaceDetail {
            id: workspace.id,
            org_id: workspace.org_id,
            kind: workspace.kind(),
            name: workspace.name,
            is_base: workspace.is_base,
            read_only: workspace.mutation_policy == "read_only",
            mutation_policy_transition: workspace.mutation_policy_transition.clone(),
            description: workspace.description,
            cloned_from_id: workspace.cloned_from_id,
            docs_repo_id: workspace.docs_repo_id,
            created_by: workspace.created_by,
            created_at: workspace.created_at.to_rfc3339(),
            updated_at: workspace.updated_at.to_rfc3339(),
            branches,
            docs_folder_path: workspace_docs_folder_path(workspace.id),
        })
    }

    /// Rename a working workspace (`g5.workspace.rename`): a database-only
    /// transaction that changes the name, reserves the previous name for the
    /// workspace (`g5.workspace.former-names`) and records the rename. The
    /// branch the workspace owns, its VM, docs and links follow the id. The
    /// caller has already applied the owner gate.
    pub async fn rename_workspace(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Option<Uuid>,
        new_name: &str,
    ) -> Result<WorkspaceRename, G5Error> {
        workspace_names::check_workspace_name(new_name)?;
        let org_id: Uuid = sqlx::query_scalar(
            "SELECT org_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        // Every name claim in the organization (renames, fork claims) is
        // serialized here, before the row lock, in the same order as a fork.
        workspace_names::lock_org_workspace_names(db.conn(), org_id).await?;
        let (current_name, is_base): (String, bool) = sqlx::query_as(
            "SELECT name, is_base FROM workspaces
             WHERE id = $1 AND deleted_at IS NULL
             FOR UPDATE",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        if is_base {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "name",
                    "message": "The Base workspace keeps its name",
                }),
            });
        }
        // The system names a personal workspace once (`g5.personal.names`).
        crate::services::workspace::workspace_kind::refuse_personal(
            db.conn(),
            workspace_id,
            "renames",
        )
        .await?;
        if new_name == current_name {
            // A stale or repeated submission: nothing to change.
            let former_names = workspace_names::former_names(db.conn(), workspace_id).await?;
            return Ok(WorkspaceRename {
                id: workspace_id,
                name: current_name,
                former_names,
            });
        }
        if let Some(holder) =
            workspace_names::name_holder(db.conn(), org_id, new_name, Some(workspace_id)).await?
        {
            return Err(holder.conflict(new_name));
        }
        let case_only = new_name.eq_ignore_ascii_case(&current_name);
        if !case_only {
            // Taking an own former name back moves it out of the reserved set;
            // the name being left becomes a former name. A case-only change
            // keeps the same reserved set because resolution ignores case.
            sqlx::query(
                "DELETE FROM workspace_former_names
                 WHERE workspace_id = $1 AND lower(name) = lower($2)",
            )
            .bind(workspace_id)
            .bind(new_name)
            .execute(db.conn())
            .await?;
            sqlx::query(
                "INSERT INTO workspace_former_names (org_id, workspace_id, name)
                 VALUES ($1, $2, $3)",
            )
            .bind(org_id)
            .bind(workspace_id)
            .bind(&current_name)
            .execute(db.conn())
            .await?;
        }
        sqlx::query(
            "INSERT INTO workspace_renames (org_id, workspace_id, previous_name, name, actor_user_id)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(&current_name)
        .bind(new_name)
        .bind(actor_user_id)
        .execute(db.conn())
        .await?;
        sqlx::query(
            "UPDATE workspaces SET name = $2, updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(new_name)
        .execute(db.conn())
        .await?;
        let former_names = workspace_names::former_names(db.conn(), workspace_id).await?;
        info!(workspace_id = %workspace_id, previous_name = %current_name, name = %new_name, "Renamed workspace");
        Ok(WorkspaceRename {
            id: workspace_id,
            name: new_name.to_string(),
            former_names,
        })
    }

    // -----------------------------------------------------------------------
    // Mutation-policy transition (live read-write <-> read-only)
    // -----------------------------------------------------------------------

    /// Transition a live workspace between `read_write` and `read_only`
    /// (spec [I5]/[I6]/[I7]).
    ///
    /// - **RO → RW** is instantaneous (also reverts an in-progress `freezing`
    ///   drain).
    /// - **RW → RO** is a two-phase freeze-then-drain: enter `freezing` (CAS
    ///   under an advisory lock), then drain already-admitted work until the
    ///   workspace is quiescent (commit `freezing → read_only`) or the timeout
    ///   elapses (fail-safe revert with blockers, resolved O3).
    ///
    /// `mode`/`drain_timeout_secs` apply to RW→RO only. Every writing
    /// transaction independently holds current owner authority through commit.
    pub async fn transition_mutation_policy(
        &self,
        event_bus: &EventBus,
        command: MutationPolicyTransitionCommand,
    ) -> Result<TransitionOutcome, G5Error> {
        let mut db = self
            .scoped_transition_db(command.org_id, command.actor)
            .await?;
        Self::lock_policy_authority(&mut db, command.workspace_id).await?;
        let (policy, transition) =
            Self::load_mutation_policy_state_exec(db.conn(), command.workspace_id).await?;
        db.rollback().await.map_err(|e| {
            Self::transition_internal_error("Failed to close transition state read", e)
        })?;
        match command.target {
            TargetMutationPolicy::ReadWrite => {
                self.transition_to_read_write(event_bus, command).await
            }
            TargetMutationPolicy::ReadOnly => {
                self.transition_to_read_only(event_bus, command, &policy, transition.as_deref())
                    .await
            }
        }
    }

    /// Load `(mutation_policy, mutation_policy_transition)`; `NotFound` if the
    /// workspace is missing.
    async fn load_mutation_policy_state_exec<'e, E>(
        exec: E,
        workspace_id: Uuid,
    ) -> Result<(String, Option<String>), G5Error>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let row: Option<(String, Option<String>)> = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT mutation_policy, mutation_policy_transition FROM workspaces WHERE id = $1",
        )
        .bind(workspace_id)
        .fetch_optional(exec)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load workspace mutation policy: {e}"),
        })?;
        row.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })
    }

    /// RO → RW (instantaneous), or revert an in-progress `freezing` drain.
    async fn transition_to_read_write(
        &self,
        event_bus: &EventBus,
        command: MutationPolicyTransitionCommand,
    ) -> Result<TransitionOutcome, G5Error> {
        let MutationPolicyTransitionCommand {
            org_id,
            workspace_id,
            actor,
            ..
        } = command;
        let mut db = self.scoped_transition_db(org_id, actor).await?;
        Self::lock_policy_authority(&mut db, workspace_id).await?;
        let (policy, transition) =
            Self::load_mutation_policy_state_exec(db.conn(), workspace_id).await?;
        if policy == "read_write" && transition.is_none() {
            // Already read-write and not transitioning: idempotent no-op.
            return Ok(TransitionOutcome {
                policy: "read_write".to_string(),
                transition: None,
                committed: true,
                blockers: Vec::new(),
                drained_tasks: 0,
                message: "Workspace is already read-write.".to_string(),
            });
        }
        let was_freezing = transition.as_deref() == Some("freezing");
        // CAS read_only OR freezing -> read_write (the RO→RW flip and the
        // `freezing → read_write` revert share the same conditional write). The
        // freezing revert is the operator escape hatch out of a stuck drain
        // (spec workspace_service [I6]); the background drain's own later CAS to
        // read_only then simply no-ops.
        let result = sqlx::query(
            "UPDATE workspaces \
             SET mutation_policy = 'read_write', mutation_policy_transition = NULL, \
                 mutation_policy_transition_id = NULL, \
                 mutation_policy_updated_by = CASE \
                     WHEN mutation_policy = 'read_only' THEN $2 \
                     ELSE mutation_policy_updated_by \
                 END, \
                 mutation_policy_updated_at = CASE \
                     WHEN mutation_policy = 'read_only' THEN now() \
                     ELSE mutation_policy_updated_at \
                 END \
             WHERE id = $1 \
               AND (mutation_policy = 'read_only' OR mutation_policy_transition = 'freezing')",
        )
        .bind(workspace_id)
        .bind(actor)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to transition workspace to read-write: {e}"),
        })?;
        if result.rows_affected() == 0 {
            // Lost the race / state changed underneath: report the current state.
            let (p, t) = Self::load_mutation_policy_state_exec(db.conn(), workspace_id).await?;
            db.rollback().await.map_err(|e| {
                Self::transition_internal_error("Failed to close read-write race read", e)
            })?;
            let message = format!("Workspace is already {p}.");
            return Ok(TransitionOutcome {
                policy: p,
                transition: t,
                committed: true,
                blockers: Vec::new(),
                drained_tasks: 0,
                message,
            });
        }
        db.commit().await.map_err(|e| {
            Self::transition_internal_error("Failed to commit read-write transition", e)
        })?;
        info!(%workspace_id, %actor, "workspace mutation policy → read_write");
        Self::emit_policy_changed(event_bus, workspace_id, false, None);
        Ok(TransitionOutcome {
            policy: "read_write".to_string(),
            transition: None,
            committed: true,
            blockers: Vec::new(),
            drained_tasks: 0,
            message: if was_freezing {
                "Cancelled the in-progress read-only transition; the workspace stays read-write."
                    .to_string()
            } else {
                "Workspace is now read-write. Preserved drafts are committable again.".to_string()
            },
        })
    }

    /// RW → RO: up-front preconditions (O2 dirty-draft + non-drainable live
    /// work), then freeze + **asynchronous** drain with `require_quiescent` and
    /// drain-timeout (O3) handling. Entering `freezing` performs no destructive
    /// teardown, so a revert is always side-effect-clean (spec [I5]/[I6]).
    async fn transition_to_read_only(
        &self,
        event_bus: &EventBus,
        command: MutationPolicyTransitionCommand,
        policy: &str,
        transition: Option<&str>,
    ) -> Result<TransitionOutcome, G5Error> {
        let MutationPolicyTransitionCommand {
            org_id,
            workspace_id,
            actor,
            mode,
            ..
        } = command;
        if policy == "read_only" {
            // Idempotent no-op.
            return Ok(TransitionOutcome {
                policy: "read_only".to_string(),
                transition: None,
                committed: true,
                blockers: Vec::new(),
                drained_tasks: 0,
                message: "Workspace is already read-only.".to_string(),
            });
        }
        if transition == Some("freezing") {
            // Already draining to read-only: not a bare conflict — report the
            // current draining state and what is still outstanding so the UI can
            // explain it rather than surface an opaque error.
            let (_, blockers) = self
                .quiescence_blockers(org_id, actor, workspace_id)
                .await?;
            return Ok(TransitionOutcome {
                policy: "read_write".to_string(),
                transition: Some("freezing".to_string()),
                committed: false,
                blockers,
                drained_tasks: 0,
                message:
                    "Workspace is already transitioning to read-only (draining in-flight work)."
                        .to_string(),
            });
        }

        if mode == TransitionMode::RequireQuiescent {
            let (quiescent, blockers) = self
                .quiescence_blockers(org_id, actor, workspace_id)
                .await?;
            if !quiescent {
                return Ok(TransitionOutcome {
                    policy: "read_write".to_string(),
                    transition: None,
                    committed: false,
                    blockers,
                    drained_tasks: 0,
                    message: "Workspace is not quiescent; stop the outstanding work and retry."
                        .to_string(),
                });
            }
            // Already quiescent: commit directly (read_write → read_only) with a
            // final re-verify under the advisory lock.
            return Self::commit_read_only(
                ReadOnlyCommitContext {
                    authz: &self.authz,
                    tenant_pool: &self.tenant_pool,
                    event_bus,
                    command,
                },
                None,
            )
            .await;
        }

        // mode == Drain.
        let baseline = self
            .count_nonterminal_work(org_id, actor, workspace_id)
            .await?;

        // Phase 1 — enter `freezing` (CAS under advisory lock). No teardown /
        // eviction: admission-block (guard + scheduler) plus the up-front
        // precondition guarantee no live chat/editor is present, so the drain
        // population is exactly the already-admitted, self-terminalizing tasks and
        // the revert edge stays side-effect-clean.
        let transition_id = Uuid::new_v4();
        if !self
            .enter_freezing(org_id, workspace_id, actor, transition_id)
            .await?
        {
            // Raced: another caller won, or state moved. Report current state.
            let mut db = self.scoped_transition_db(org_id, actor).await?;
            let (p, t) = Self::load_mutation_policy_state_exec(db.conn(), workspace_id).await?;
            db.rollback().await.map_err(|e| {
                Self::transition_internal_error("Failed to close race state read", e)
            })?;
            if p == "read_only" {
                return Ok(TransitionOutcome {
                    policy: p,
                    transition: None,
                    committed: true,
                    blockers: Vec::new(),
                    drained_tasks: 0,
                    message: "Workspace is now read-only.".to_string(),
                });
            }
            let (_, blockers) = self
                .quiescence_blockers(org_id, actor, workspace_id)
                .await?;
            return Ok(TransitionOutcome {
                policy: "read_write".to_string(),
                transition: t,
                committed: false,
                blockers,
                drained_tasks: 0,
                message: "Transition state changed; reporting the current workspace state."
                    .to_string(),
            });
        }

        // Fast path: already quiescent → commit inline so the common
        // no-in-flight-work case returns `read_only` immediately.
        let (quiescent, _) = self
            .quiescence_blockers(org_id, actor, workspace_id)
            .await?;
        if quiescent {
            let committed = Self::commit_read_only(
                ReadOnlyCommitContext {
                    authz: &self.authz,
                    tenant_pool: &self.tenant_pool,
                    event_bus,
                    command,
                },
                Some(transition_id),
            )
            .await?;
            if committed.committed {
                // The committed transition proved that no nonterminal work remains.
                // Do not turn a completed write into an error through a later read.
                return Ok(TransitionOutcome {
                    policy: "read_only".to_string(),
                    transition: None,
                    committed: true,
                    blockers: Vec::new(),
                    drained_tasks: baseline,
                    message: "Workspace is now read-only.".to_string(),
                });
            }
            if committed.transition.is_none() {
                return Ok(committed);
            }
            // Re-verify failed (became non-quiescent between check and lock):
            // fall through to the background drain.
        }

        // Slow path (spec [I5] step 2): drain **asynchronously**. Return
        // `freezing` immediately (the endpoint maps this to 202) and let a
        // detached driver run the drain to commit or fail-safe revert, so the
        // request never blocks on the (up to `drain_timeout_secs`) drain.
        let (_, current_blockers) = self
            .quiescence_blockers(org_id, actor, workspace_id)
            .await?;
        let drain_context = DrainContext {
            tenant_pool: self.tenant_pool.clone(),
            authz: self.authz.clone(),
            event_bus: event_bus.clone(),
            command,
            baseline,
            transition_id,
        };
        tokio::spawn(async move {
            Self::drive_drain(drain_context).await;
        });
        info!(
            %workspace_id, %actor,
            "workspace mutation policy → freezing (draining in background)"
        );
        Ok(TransitionOutcome {
            policy: "read_write".to_string(),
            transition: Some("freezing".to_string()),
            committed: false,
            blockers: current_blockers,
            drained_tasks: 0,
            message: "Freezing the workspace and draining in-flight work in the background. It \
                      becomes read-only when the drain finishes; you can close this dialog."
                .to_string(),
        })
    }

    /// Background drain driver (spec [I5] step 2/3, [I6] timeout). Runs detached
    /// from the request on cloned `system_pool`/`event_bus` handles: polls
    /// quiescence, commits `freezing → read_only` under the advisory lock when
    /// quiescent, or fail-safe reverts on timeout. The revert is side-effect-clean
    /// because entering `freezing` performed no teardown.
    async fn drive_drain(ctx: DrainContext) {
        let command = ctx.command;
        let MutationPolicyTransitionCommand {
            org_id,
            workspace_id,
            actor,
            drain_timeout_secs,
            ..
        } = command;
        let baseline = ctx.baseline;
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(drain_timeout_secs);
        loop {
            match Self::transition_is_current(&ctx).await {
                Ok(true) => {}
                Ok(false) => return,
                Err(e) => {
                    error!(%workspace_id, error = %e, "background drain identity check failed");
                    return;
                }
            }
            match Self::quiescence_blockers_scoped(
                &ctx.authz,
                &ctx.tenant_pool,
                org_id,
                actor,
                workspace_id,
            )
            .await
            {
                Ok((true, _)) => {
                    match Self::commit_read_only(
                        ReadOnlyCommitContext {
                            authz: &ctx.authz,
                            tenant_pool: &ctx.tenant_pool,
                            event_bus: &ctx.event_bus,
                            command,
                        },
                        Some(ctx.transition_id),
                    )
                    .await
                    {
                        Ok(outcome) if outcome.committed => {
                            let drained = Self::count_nonterminal_work_scoped(
                                &ctx.authz,
                                &ctx.tenant_pool,
                                org_id,
                                actor,
                                workspace_id,
                            )
                            .await
                            .map(|remaining| (baseline - remaining).max(0))
                            .unwrap_or(0);
                            info!(
                                %workspace_id, %actor, drained_tasks = drained,
                                "workspace mutation policy → read_only (committed, background drain)"
                            );
                            return;
                        }
                        // Re-verify found it non-quiescent again: keep draining.
                        Ok(_) => {}
                        Err(e) => {
                            error!(%workspace_id, error = %e, "background drain commit failed");
                            return;
                        }
                    }
                }
                Ok((false, _)) => {}
                Err(e) => {
                    error!(%workspace_id, error = %e, "background drain quiescence check failed");
                    return;
                }
            }
            if std::time::Instant::now() >= deadline {
                // Fail-safe revert (O3): clear `freezing`, re-emit policy_changed.
                // No teardown was performed on freeze, so this leaves no partial
                // read-only state.
                if let Err(e) = Self::revert_freezing(
                    &ctx.authz,
                    &ctx.tenant_pool,
                    &ctx.event_bus,
                    org_id,
                    workspace_id,
                    actor,
                    ctx.transition_id,
                )
                .await
                {
                    error!(%workspace_id, error = %e, "background drain revert failed");
                } else {
                    info!(
                        %workspace_id, %actor,
                        "workspace mutation policy transition reverted (drain timeout)"
                    );
                }
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
    }

    /// Enter `freezing`: acquire the transaction-scoped advisory workspace lock,
    /// then CAS `read_write → freezing`. Returns `true` on success, `false` if
    /// the CAS matched no row (already read-only / freezing). `Conflict` if the
    /// advisory lock is held by a concurrent transition.
    async fn enter_freezing(
        &self,
        org_id: Uuid,
        workspace_id: Uuid,
        actor: Uuid,
        transition_id: Uuid,
    ) -> Result<bool, G5Error> {
        let mut db = self.scoped_transition_db(org_id, actor).await?;
        Self::lock_policy_authority(&mut db, workspace_id).await?;
        let locked: bool = sqlx::query_scalar::<_, bool>(
            "SELECT pg_try_advisory_xact_lock(hashtext($1::text), hashtext($2::text))",
        )
        .bind("workspace_mutation_policy_transition")
        .bind(workspace_id.to_string())
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to acquire transition advisory lock: {e}"),
        })?;
        if !locked {
            return Err(G5Error::Conflict {
                reason: "Another mutation-policy transition is in progress for this workspace"
                    .to_string(),
            });
        }
        let result = sqlx::query(
            "UPDATE workspaces \
             SET mutation_policy_transition = 'freezing', mutation_policy_transition_id = $2 \
             WHERE id = $1 AND mutation_policy = 'read_write' \
               AND mutation_policy_transition IS NULL",
        )
        .bind(workspace_id)
        .bind(transition_id)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to enter freezing state: {e}"),
        })?;
        db.commit()
            .await
            .map_err(|e| Self::transition_internal_error("Failed to commit freezing state", e))?;
        Ok(result.rows_affected() == 1)
    }

    /// Commit to `read_only` under the advisory lock, re-verifying quiescence so a
    /// task admitted between the drain check and the commit cannot
    /// slip a write past the flip. `from_freezing` selects the CAS guard
    /// (`freezing → read_only` during a drain, `read_write → read_only` for a
    /// `require_quiescent` immediate flip). Returns a `committed = false` outcome
    /// (with blockers) if it could not commit.
    async fn commit_read_only(
        ctx: ReadOnlyCommitContext<'_>,
        transition_id: Option<Uuid>,
    ) -> Result<TransitionOutcome, G5Error> {
        #[cfg(feature = "test-helpers")]
        let _test_completion = match transition_id {
            Some(id) => {
                operation_test_barrier::wait(id, operation_test_barrier::Stage::PolicyCommit).await
            }
            None => None,
        };
        let MutationPolicyTransitionCommand {
            org_id,
            workspace_id,
            actor,
            ..
        } = ctx.command;
        let mut db =
            Self::scoped_transition_db_for(ctx.authz, ctx.tenant_pool, org_id, actor).await?;
        Self::lock_policy_authority(&mut db, workspace_id).await?;
        let from_freezing = transition_id.is_some();
        if let Some(expected) = transition_id {
            let current: Option<Uuid> = sqlx::query_scalar(
                "SELECT mutation_policy_transition_id FROM workspaces WHERE id = $1",
            )
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await?;
            if current != Some(expected) {
                let (policy, transition) =
                    Self::load_mutation_policy_state_exec(db.conn(), workspace_id).await?;
                return Ok(TransitionOutcome {
                    policy,
                    transition,
                    committed: false,
                    blockers: Vec::new(),
                    drained_tasks: 0,
                    message: "This transition has been superseded.".to_string(),
                });
            }
        }
        let locked: bool = sqlx::query_scalar::<_, bool>(
            "SELECT pg_try_advisory_xact_lock(hashtext($1::text), hashtext($2::text))",
        )
        .bind("workspace_mutation_policy_transition")
        .bind(workspace_id.to_string())
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to acquire transition advisory lock: {e}"),
        })?;
        if !locked {
            if from_freezing {
                // Someone else holds the lock; report not-committed so the drain
                // loop retries on its next tick (still `freezing`).
                return Ok(TransitionOutcome {
                    policy: "read_write".to_string(),
                    transition: Some("freezing".to_string()),
                    committed: false,
                    blockers: Vec::new(),
                    drained_tasks: 0,
                    message: "Still draining in-flight work.".to_string(),
                });
            }
            return Err(G5Error::Conflict {
                reason: "Another mutation-policy transition is in progress for this workspace"
                    .to_string(),
            });
        }
        // Admission holds a workspace SHARE lock through task creation/commit.
        // Take UPDATE before the final check so an admission that was already
        // in progress is visible, and later admissions observe read-only.
        sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(|e| {
                Self::transition_internal_error("Failed to lock workspace transition", e)
            })?;
        // Re-verify quiescence inside the locked transaction.
        let task_blockers = Self::nonterminal_work_blockers_exec(db.conn(), workspace_id).await?;
        if !task_blockers.is_empty() {
            return Ok(TransitionOutcome {
                policy: "read_write".to_string(),
                transition: if from_freezing {
                    Some("freezing".to_string())
                } else {
                    None
                },
                committed: false,
                blockers: task_blockers,
                drained_tasks: 0,
                message: "Workspace is not quiescent; outstanding work must finish first."
                    .to_string(),
            });
        }
        let guard = if from_freezing {
            "mutation_policy_transition = 'freezing' AND mutation_policy_transition_id = $3"
        } else {
            "mutation_policy = 'read_write' AND mutation_policy_transition IS NULL AND $3::uuid IS NULL"
        };
        let sql = format!(
            "UPDATE workspaces \
             SET mutation_policy = 'read_only', mutation_policy_transition = NULL, \
                 mutation_policy_transition_id = NULL, \
                 mutation_policy_updated_by = $2, mutation_policy_updated_at = now() \
             WHERE id = $1 AND {guard}"
        );
        let result = sqlx::query(&sql)
            .bind(workspace_id)
            .bind(actor)
            .bind(transition_id)
            .execute(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to commit read-only transition: {e}"),
            })?;
        let unchanged = if result.rows_affected() == 0 {
            Some(Self::load_mutation_policy_state_exec(db.conn(), workspace_id).await?)
        } else {
            None
        };
        db.commit().await.map_err(|e| {
            Self::transition_internal_error("Failed to commit read-only transaction", e)
        })?;
        if result.rows_affected() == 1 {
            info!(%workspace_id, %actor, "workspace mutation policy → read_only (committed)");
            Self::emit_policy_changed(ctx.event_bus, workspace_id, true, None);
            Ok(TransitionOutcome {
                policy: "read_only".to_string(),
                transition: None,
                committed: true,
                blockers: Vec::new(),
                drained_tasks: 0,
                message: "Workspace is now read-only.".to_string(),
            })
        } else {
            let (policy, transition) = unchanged.expect("unmatched transition has current state");
            Ok(TransitionOutcome {
                policy,
                transition,
                committed: false,
                blockers: Vec::new(),
                drained_tasks: 0,
                message: "Transition state changed; reporting the current workspace state."
                    .to_string(),
            })
        }
    }

    /// Revert `freezing → read_write` (clear the transient marker). The enforced
    /// `mutation_policy` stayed `read_write` throughout `freezing`, so clearing
    /// the marker returns the workspace to its steady read-write state. Re-emits
    /// `workspace.policy_changed{read_only:false}` so read-once consumers observe
    /// read-write again; because entering `freezing`
    /// performed no teardown there is no partial state to restore (spec [I6]).
    async fn revert_freezing(
        authz: &Arc<dyn AuthzService>,
        tenant_pool: &PgPool,
        event_bus: &EventBus,
        org_id: Uuid,
        workspace_id: Uuid,
        actor: Uuid,
        transition_id: Uuid,
    ) -> Result<(), G5Error> {
        #[cfg(feature = "test-helpers")]
        let _test_completion = operation_test_barrier::wait(
            transition_id,
            operation_test_barrier::Stage::PolicyTimeout,
        )
        .await;
        let mut db = Self::scoped_transition_db_for(authz, tenant_pool, org_id, actor).await?;
        Self::lock_policy_authority(&mut db, workspace_id).await?;
        let result = sqlx::query(
            "UPDATE workspaces \
             SET mutation_policy_transition = NULL, mutation_policy_transition_id = NULL \
             WHERE id = $1 AND mutation_policy_transition = 'freezing' \
               AND mutation_policy_transition_id = $2",
        )
        .bind(workspace_id)
        .bind(transition_id)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to revert freezing state: {e}"),
        })?;
        db.commit()
            .await
            .map_err(|e| Self::transition_internal_error("Failed to commit freezing revert", e))?;
        if result.rows_affected() == 1 {
            Self::emit_policy_changed(event_bus, workspace_id, false, None);
        }
        Ok(())
    }

    async fn transition_is_current(ctx: &DrainContext) -> Result<bool, G5Error> {
        let mut db = Self::scoped_transition_db_for(
            &ctx.authz,
            &ctx.tenant_pool,
            ctx.command.org_id,
            ctx.command.actor,
        )
        .await?;
        let current = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND deleted_at IS NULL
             AND mutation_policy_transition = 'freezing' AND mutation_policy_transition_id = $2)",
        )
        .bind(ctx.command.workspace_id)
        .bind(ctx.transition_id)
        .fetch_one(db.conn())
        .await?;
        Ok(current)
    }

    /// Count non-terminal, non-preview work for the workspace (drain population).
    async fn count_nonterminal_work_scoped(
        authz: &Arc<dyn AuthzService>,
        tenant_pool: &PgPool,
        org_id: Uuid,
        actor: Uuid,
        workspace_id: Uuid,
    ) -> Result<i64, G5Error> {
        let mut db = Self::scoped_transition_db_for(authz, tenant_pool, org_id, actor).await?;
        let sql = format!("SELECT COUNT(*) FROM ({NONTERMINAL_WORK_SQL}) AS work");
        let count: i64 = sqlx::query_scalar::<_, i64>(&sql)
            .bind(workspace_id)
            .fetch_one(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to count non-terminal work: {e}"),
            })?;
        db.rollback()
            .await
            .map_err(|e| Self::transition_internal_error("Failed to close work count read", e))?;
        Ok(count)
    }

    async fn count_nonterminal_work(
        &self,
        org_id: Uuid,
        actor: Uuid,
        workspace_id: Uuid,
    ) -> Result<i64, G5Error> {
        Self::count_nonterminal_work_scoped(
            &self.authz,
            &self.tenant_pool,
            org_id,
            actor,
            workspace_id,
        )
        .await
    }

    /// Quiescence check: no non-terminal non-preview tasks.
    /// Preview tasks and Git-inflow work never block quiescence (spec [I5]).
    async fn quiescence_blockers_scoped(
        authz: &Arc<dyn AuthzService>,
        tenant_pool: &PgPool,
        org_id: Uuid,
        actor: Uuid,
        workspace_id: Uuid,
    ) -> Result<(bool, Vec<TransitionBlocker>), G5Error> {
        let mut db = Self::scoped_transition_db_for(authz, tenant_pool, org_id, actor).await?;
        let blockers = Self::nonterminal_work_blockers_exec(db.conn(), workspace_id).await?;
        db.rollback()
            .await
            .map_err(|e| Self::transition_internal_error("Failed to close quiescence read", e))?;
        Ok((blockers.is_empty(), blockers))
    }

    async fn quiescence_blockers(
        &self,
        org_id: Uuid,
        actor: Uuid,
        workspace_id: Uuid,
    ) -> Result<(bool, Vec<TransitionBlocker>), G5Error> {
        Self::quiescence_blockers_scoped(
            &self.authz,
            &self.tenant_pool,
            org_id,
            actor,
            workspace_id,
        )
        .await
    }

    async fn nonterminal_work_blockers_exec<'e, E>(
        exec: E,
        workspace_id: Uuid,
    ) -> Result<Vec<TransitionBlocker>, G5Error>
    where
        E: sqlx::Executor<'e, Database = sqlx::Postgres>,
    {
        let sql = format!(
            "SELECT kind, id, label, status FROM ({NONTERMINAL_WORK_SQL}) AS work ORDER BY created_at ASC, id ASC LIMIT 50"
        );
        let rows: Vec<(String, Uuid, String, String)> = sqlx::query_as(&sql)
            .bind(workspace_id)
            .fetch_all(exec)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load non-terminal work: {e}"),
            })?;
        Ok(rows
            .into_iter()
            .map(|(kind, id, label, status)| TransitionBlocker {
                detail: format!("{label} {kind} is {status}"),
                kind,
                id: id.to_string(),
            })
            .collect())
    }

    /// Emit `workspace.policy_changed` after the DB transaction commits (spec
    /// [I5], per the branch-updated event discipline).
    fn emit_policy_changed(
        event_bus: &EventBus,
        workspace_id: Uuid,
        read_only: bool,
        transition: Option<String>,
    ) {
        event_bus.publish(
            &Channel::Workspace(workspace_id),
            &WsEvent::WorkspacePolicyChanged {
                workspace_id,
                read_only,
                transition,
            },
        );
    }

    /// Seal one completed formal review. Its tool surface already denied all
    /// non-output writes and all humans from creation; terminal task custody
    /// proves the writer ended. Under the workspace lock, close any future
    /// admission before settlement. Dirty review output is intentionally discarded.
    pub(crate) async fn seal_agent_review(
        &self,
        db: &mut ScopedDb,
        review_id: Uuid,
    ) -> Result<(), G5Error> {
        let workspace: Uuid=sqlx::query_scalar("SELECT w.id FROM workspaces w JOIN agent_approval_reviews r ON r.review_workspace_id=w.id AND r.org_id=w.org_id AND w.agent_approval_review_id=r.id JOIN tasks t ON t.id=r.task_id AND t.workspace_id=w.id AND t.org_id=r.org_id WHERE r.id=$1 AND r.stage='seal' AND r.artifact_id IS NOT NULL AND t.status='succeeded' AND w.deleted_at IS NULL FOR UPDATE OF w")
            .bind(review_id).fetch_optional(db.conn()).await?.ok_or_else(||G5Error::Conflict{reason:"formal review writer is not terminal or evidence is absent".into()})?;
        if !Self::nonterminal_work_blockers_exec(db.conn(), workspace)
            .await?
            .is_empty()
        {
            return Err(G5Error::Conflict {
                reason: "formal review still has active writers".into(),
            });
        }
        sqlx::query("UPDATE workspaces SET mutation_policy='read_only',mutation_policy_transition=NULL,mutation_policy_transition_id=NULL WHERE id=$1 AND agent_approval_review_id=$2")
            .bind(workspace).bind(review_id).execute(db.conn()).await?;
        Ok(())
    }

    pub(crate) async fn delete_agent_review(
        &self,
        db: &mut ScopedDb,
        review_id: Uuid,
    ) -> Result<(), G5Error> {
        let resource: Option<(Uuid,Uuid)> = sqlx::query_as("SELECT w.id,r.requested_by FROM agent_approval_reviews r JOIN workspaces w ON w.id=r.review_workspace_id AND w.org_id=r.org_id AND w.agent_approval_review_id=r.id WHERE r.id=$1 AND r.stage IN ('effective','failed','cancelled') AND w.deleted_at IS NULL")
            .bind(review_id).fetch_optional(db.conn()).await?;
        if let Some((workspace, requester)) = resource {
            self.delete_workspace(db, workspace, requester, review_id)
                .await?;
        }
        Ok(())
    }

    /// Admit deletion before requesting any runtime effects. The durable
    /// reservation excludes eligibility writers while stop runs in its own
    /// transactions, and survives a dropped request or process restart.
    pub async fn delete_workspace_with_stop(
        &self,
        scope: ScopedDbReopener,
        lifecycle: &crate::services::WorkspaceVmService,
        workspace_id: Uuid,
        actor: Uuid,
        requested_operation_id: Uuid,
        force: bool,
    ) -> Result<(), G5Error> {
        let mut db = scope
            .open()
            .await
            .map_err(|e| Self::transition_internal_error("Open deletion scope", e))?;
        let (workspace, repo_ids) = self
            .prepare_workspace_removal(&mut db, workspace_id, actor)
            .await?;
        crate::services::PermissionService::require_active_member_on_conn(
            db.conn(),
            actor,
            workspace.org_id,
            false,
        )
        .await?;
        // Refuse missing repositories before runtime teardown too.
        for repo_id in repo_ids {
            let path = self
                .repo_storage
                .repo_path(&db, workspace.org_id, repo_id)?;
            tokio::task::spawn_blocking(move || GitRepo::open(&path).map(|_| ()))
                .await
                .map_err(|e| Self::transition_internal_error("Deletion Git preflight", e))??;
        }
        let existing: Option<Uuid> =
            sqlx::query_scalar("SELECT deletion_operation_id FROM workspaces WHERE id = $1")
                .bind(workspace_id)
                .fetch_one(db.conn())
                .await?;
        let operation_id = if force {
            requested_operation_id
        } else {
            existing.unwrap_or(requested_operation_id)
        };
        sqlx::query("UPDATE workspaces SET deletion_operation_id = $2 WHERE id = $1")
            .bind(workspace_id)
            .bind(operation_id)
            .execute(db.conn())
            .await?;
        db.commit()
            .await
            .map_err(|e| Self::transition_internal_error("Commit deletion admission", e))?;
        self.close_pull_requests_for_deletion(&scope, workspace_id, actor)
            .await?;

        #[cfg(feature = "test-helpers")]
        let _test_completion = operation_test_barrier::wait(
            workspace_id,
            operation_test_barrier::Stage::DeleteAdmitted,
        )
        .await;

        let stopped = lifecycle
            .stop_for_deletion(workspace_id, actor, operation_id, force)
            .await?;
        if stopped.status != "stopped" {
            return Err(G5Error::WorkspaceStopping);
        }
        let mut db = scope
            .open()
            .await
            .map_err(|e| Self::transition_internal_error("Reopen deletion scope", e))?;
        self.delete_workspace(&mut db, workspace_id, actor, operation_id)
            .await?;
        db.commit()
            .await
            .map_err(|e| Self::transition_internal_error("Commit deletion", e))?;
        Ok(())
    }

    async fn prepare_workspace_removal(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<(Workspace, Vec<Uuid>), G5Error> {
        let snapshot = self
            .load_workspace_for_removal_snapshot(db, workspace_id, actor_user_id)
            .await?;
        let repo_ids = self
            .workspace_delete_repo_ids(db, snapshot.org_id, workspace_id)
            .await?;
        self.lock_workspace_delete_repos(db, snapshot.org_id, &repo_ids)
            .await?;

        let workspace = self
            .load_workspace_for_removal(db, workspace_id, actor_user_id)
            .await?;
        if workspace.org_id != snapshot.org_id
            || self
                .workspace_delete_repo_ids(db, workspace.org_id, workspace_id)
                .await?
                != repo_ids
        {
            return Err(G5Error::Conflict {
                reason: "Workspace repository membership changed during deletion".to_string(),
            });
        }
        if self.workspace_has_live_children(db, workspace_id).await? {
            return Err(Self::workspace_with_children_removal_error());
        }
        terminalize_active_merge_runs_with_tombstoned_sides(db, &[workspace_id]).await?;
        if self
            .workspace_has_active_merge_runs(db, workspace_id)
            .await?
        {
            return Err(Self::workspace_active_merge_removal_error());
        }
        let unfinished_fork = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM workspace_forks
                 WHERE org_id = $1 AND source_workspace_id = $2
                   AND status IN ('pending','aborting'))",
        )
        .bind(workspace.org_id)
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to check workspace fork pins: {error}"),
        })?;
        if unfinished_fork {
            return Err(G5Error::Conflict {
                reason: "Workspace has an unfinished fork operation".to_string(),
            });
        }
        crate::services::workspace_mutation_policy::ensure_no_pending_transfer(
            db.conn(),
            workspace_id,
        )
        .await?;
        let pending_provider_merge: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_pull_request_provider_repos p
             JOIN workspace_pull_requests pr ON pr.id=p.pull_request_id
             WHERE (pr.source_workspace_id=$1 OR pr.target_workspace_id=$1) AND p.merge_pending)",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await?;
        if pending_provider_merge {
            return Err(G5Error::Conflict {
                reason: "Reconcile the pending provider merge before deleting this workspace"
                    .into(),
            });
        }
        Ok((workspace, repo_ids))
    }

    /// Close a workspace's open pull requests before deleting it, in their
    /// own committed transaction (`g5.workspace.delete.pull-requests`).
    /// Deletion's checks run first and are rolled back, so the close holds no
    /// repository or workspace lock while it waits on a pull request, and a
    /// deletion that fails afterwards is simply retried.
    pub async fn close_pull_requests_for_deletion(
        &self,
        scope: &ScopedDbReopener,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<(), G5Error> {
        let mut db = scope
            .open()
            .await
            .map_err(|e| Self::transition_internal_error("Open deletion check", e))?;
        self.prepare_workspace_removal(&mut db, workspace_id, actor_user_id)
            .await?;
        db.rollback()
            .await
            .map_err(|e| Self::transition_internal_error("End deletion check", e))?;
        let mut db = scope
            .open()
            .await
            .map_err(|e| Self::transition_internal_error("Open pull request close", e))?;
        crate::services::git::coordinated_repo_publication::CoordinatedRepoPublicationService::close_deleted_workspace_pull_requests(
            &mut db,
            workspace_id,
            actor_user_id,
        )
        .await?;
        db.commit()
            .await
            .map_err(|e| Self::transition_internal_error("Commit pull request close", e))
    }

    /// Delete the mutable state owned by a workspace and retain its audit
    /// tombstone.
    ///
    /// Cannot remove Base. Only the current workspace owner may remove a
    /// workspace; organization administrators must transfer ownership first.
    /// Repository locks are acquired before the workspace lock to match fork
    /// finalization. The workspace and mapping are then reread under those
    /// locks before any Git ref is changed.
    pub async fn delete_workspace(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        operation_id: Uuid,
    ) -> Result<(), G5Error> {
        let (workspace, repo_ids) = self
            .prepare_workspace_removal(db, workspace_id, actor_user_id)
            .await?;
        let reserved: Option<Uuid> =
            sqlx::query_scalar("SELECT deletion_operation_id FROM workspaces WHERE id = $1")
                .bind(workspace_id)
                .fetch_one(db.conn())
                .await?;
        if reserved.is_some() {
            crate::services::PermissionService::require_active_member_on_conn(
                db.conn(),
                actor_user_id,
                workspace.org_id,
                false,
            )
            .await?;
        }
        if reserved.is_some_and(|reserved| reserved != operation_id) {
            return Err(G5Error::Conflict {
                reason: "Deletion was superseded".into(),
            });
        }
        let vm_stopped = sqlx::query_scalar::<_, bool>(
            "SELECT NOT EXISTS(
                 SELECT 1 FROM workspace_vms
                 WHERE workspace_id = $1
                   AND status IN ('starting','running','stopping'))",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to verify workspace stop before deletion: {error}"),
        })?;
        if !vm_stopped {
            return Err(G5Error::WorkspaceStopping);
        }

        let flushes = sqlx::query_as::<_, WorkspaceDeleteFlushRow>(
            "SELECT flush_id, required_repo_ids
             FROM workspace_flushes
             WHERE org_id = $1 AND workspace_id = $2
             ORDER BY flush_id
             FOR UPDATE",
        )
        .bind(workspace.org_id)
        .bind(workspace_id)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to load workspace flush receipts for deletion: {error}"),
        })?;
        // The branches to delete are the ones the workspace owns, which a rename
        // never changed (`g5.workspace.branch`); Base is never deleted.
        let branch_name: String = sqlx::query_scalar::<_, Option<String>>(
            "SELECT branch_name FROM workspaces WHERE id = $1",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to load the workspace branch for deletion: {error}"),
        })?
        .ok_or_else(|| G5Error::Conflict {
            reason: "The Base workspace cannot be deleted".to_string(),
        })?;
        let branch_name = branch_name.as_str();

        let mut repositories = Vec::with_capacity(repo_ids.len());
        for repo_id in &repo_ids {
            let path = self
                .repo_storage
                .repo_path(&*db, workspace.org_id, *repo_id)?;
            let path_for_check = path.clone();
            tokio::task::spawn_blocking(move || GitRepo::open(&path_for_check).map(|_| ()))
                .await
                .map_err(|error| G5Error::Internal {
                    message: format!("Workspace deletion Git preflight task failed: {error}"),
                })??;
            let flush_ids = flushes
                .iter()
                .filter(|flush| flush.required_repo_ids.contains(repo_id))
                .map(|flush| flush.flush_id)
                .collect::<Vec<_>>();
            repositories.push((path, flush_ids));
        }

        if !self
            .attempt_workspace_tombstone(db, workspace_id, actor_user_id, operation_id)
            .await?
        {
            return Err(G5Error::Conflict {
                reason: "Workspace changed during deletion".to_string(),
            });
        }
        // Release the workspace's external issue associations in the
        // transaction that commits the tombstone, so their primary claims end
        // exactly when deletion does. Link creation locks the live workspace
        // row, so no link can be added after this point.
        crate::services::git::external_issues::release_workspace_issue_links(
            db,
            workspace_id,
            actor_user_id,
        )
        .await?;
        // Callers close its open pull requests first
        // (`close_pull_requests_for_deletion`); any opened since close here,
        // in the tombstone's transaction. Their records stay readable
        // (`g5.workspace.delete.pull-requests`).
        crate::services::git::coordinated_repo_publication::CoordinatedRepoPublicationService::close_deleted_workspace_pull_requests(
            db,
            workspace_id,
            actor_user_id,
        )
        .await?;

        for (path, flush_ids) in repositories {
            let branch_name = branch_name.to_string();
            tokio::task::spawn_blocking(move || {
                GitRepo::open(&path)?.delete_workspace_refs(&branch_name, workspace_id, &flush_ids)
            })
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Workspace deletion Git task failed: {error}"),
            })??;
        }

        sqlx::query("DELETE FROM worker_vm_events WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to remove workspace VM events: {error}"),
            })?;
        sqlx::query("DELETE FROM workspace_vms WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to remove workspace VM state: {error}"),
            })?;
        sqlx::query(
            "UPDATE workspace_forks
             SET source_flush_id = NULL
             WHERE source_flush_id = ANY($1) AND status IN ('completed','aborted')",
        )
        .bind(
            flushes
                .iter()
                .map(|flush| flush.flush_id)
                .collect::<Vec<_>>(),
        )
        .execute(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to detach retained fork audit from deleted receipts: {error}"),
        })?;
        sqlx::query("DELETE FROM workspace_flushes WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to remove workspace flush receipts: {error}"),
            })?;
        sqlx::query("DELETE FROM workspace_repos WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to remove workspace repository mappings: {error}"),
            })?;
        // Completed deletion releases the names the workspace reserved
        // (`g5.workspace.former-names`), together with its branches.
        sqlx::query("DELETE FROM workspace_former_names WHERE workspace_id = $1")
            .bind(workspace_id)
            .execute(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to release the workspace's former names: {error}"),
            })?;

        info!(workspace_id = %workspace_id, actor_user_id = %actor_user_id, "Deleted workspace mutable state");
        Ok(())
    }

    async fn attempt_workspace_tombstone(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        operation_id: Uuid,
    ) -> Result<bool, G5Error> {
        sqlx::query(
            r#"
            UPDATE workspaces
            SET deleted_at = now(), deleted_by = $2,
                deletion_operation_id = $3,
                deletion_loss = (
                    SELECT CASE
                        WHEN vm.unflushed_loss_at IS NULL
                         AND vm.session_harvest_loss_at IS NULL THEN NULL
                        ELSE jsonb_strip_nulls(jsonb_build_object(
                            'unflushedLossAt', vm.unflushed_loss_at,
                            'sessionHarvestLossAt', vm.session_harvest_loss_at,
                            'lastError', vm.last_error
                        ))
                    END
                    FROM workspace_vms AS vm
                    WHERE vm.workspace_id = workspaces.id
                ),
                updated_at = now()
            WHERE id = $1
              AND is_base = false
              AND deleted_at IS NULL
              AND NOT EXISTS (
                  SELECT 1
                  FROM workspaces child
                  WHERE child.cloned_from_id = workspaces.id
                    AND child.deleted_at IS NULL
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM workspace_merge_runs jmr
                  WHERE jmr.status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
                    AND (
                        jmr.workspace_id = workspaces.id
                        OR jmr.source_workspace_id = workspaces.id
                        OR jmr.target_workspace_id = workspaces.id
                    )
              )
            "#,
        )
        .bind(workspace_id)
        .bind(actor_user_id)
        .bind(operation_id)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to remove workspace: {e}"),
        })
        .map(|result| result.rows_affected() == 1)
    }

    async fn load_workspace_for_removal(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Workspace, G5Error> {
        let workspace = sqlx::query_as::<_, Workspace>(
            r#"
            SELECT id, org_id, name, is_base, description,
                   cloned_from_id, docs_repo_id,
                   push_mode,
                   mutation_policy,
                   mutation_policy_transition,
                   mutation_policy_updated_by, mutation_policy_updated_at,
                   merge_target_workspace_id, personal_user_id,
                   owner_user_id, lifecycle_generation,
                   created_by, created_at, updated_at
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            FOR UPDATE
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check workspace for removal: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        if workspace.is_base {
            return Err(G5Error::WorkspaceIsBase);
        }
        if workspace.kind().is_personal() {
            return Err(
                crate::services::workspace::workspace_kind::personal_refusal("owner deletion"),
            );
        }

        if workspace.owner_user_id != Some(actor_user_id) {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["workspace_owner".to_string()],
            });
        }

        Ok(workspace)
    }

    async fn load_workspace_for_removal_snapshot(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Workspace, G5Error> {
        let workspace = sqlx::query_as::<_, Workspace>(
            r#"
            SELECT id, org_id, name, is_base, description,
                   cloned_from_id, docs_repo_id,
                   push_mode,
                   mutation_policy,
                   mutation_policy_transition,
                   mutation_policy_updated_by, mutation_policy_updated_at,
                   merge_target_workspace_id, personal_user_id,
                   owner_user_id, lifecycle_generation,
                   created_by, created_at, updated_at
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to inspect workspace for deletion: {error}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        if workspace.is_base {
            return Err(G5Error::WorkspaceIsBase);
        }
        if workspace.kind().is_personal() {
            return Err(
                crate::services::workspace::workspace_kind::personal_refusal("owner deletion"),
            );
        }
        if workspace.owner_user_id != Some(actor_user_id) {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["workspace_owner".to_string()],
            });
        }
        Ok(workspace)
    }

    async fn workspace_delete_repo_ids(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<Vec<Uuid>, G5Error> {
        sqlx::query_scalar(
            "SELECT repo_id FROM workspace_repos
             WHERE org_id = $1 AND workspace_id = $2
             ORDER BY repo_id",
        )
        .bind(org_id)
        .bind(workspace_id)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to load workspace repository mapping: {error}"),
        })
    }

    async fn lock_workspace_delete_repos(
        &self,
        db: &mut ScopedDb,
        org_id: Uuid,
        repo_ids: &[Uuid],
    ) -> Result<(), G5Error> {
        let locked = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM repos
             WHERE org_id = $1 AND id = ANY($2)
             ORDER BY id
             FOR UPDATE",
        )
        .bind(org_id)
        .bind(repo_ids)
        .fetch_all(db.conn())
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Failed to lock workspace repositories for deletion: {error}"),
        })?;
        if locked != repo_ids {
            return Err(G5Error::Conflict {
                reason: "Workspace repository membership changed during deletion".to_string(),
            });
        }
        Ok(())
    }

    async fn workspace_has_live_children(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<bool, G5Error> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workspaces
                WHERE cloned_from_id = $1
                  AND deleted_at IS NULL
            )
            "#,
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check child workspaces for removal: {e}"),
        })
    }

    async fn workspace_has_active_merge_runs(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<bool, G5Error> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workspace_merge_runs jmr
                WHERE jmr.status IN ('pending', 'running', 'waiting_for_human', 'rolling_back')
                  AND (
                      jmr.workspace_id = $1
                      OR jmr.source_workspace_id = $1
                      OR jmr.target_workspace_id = $1
                  )
            )
            "#,
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check active merge workflows for removal: {e}"),
        })
    }

    fn workspace_with_children_removal_error() -> G5Error {
        G5Error::Conflict {
            reason: "Workspace has child workspaces and cannot be removed".to_string(),
        }
    }

    fn workspace_active_merge_removal_error() -> G5Error {
        G5Error::Conflict {
            reason: "Workspace has active merge workflows and cannot be removed".to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Workspace Stats
    // -----------------------------------------------------------------------

    /// Compute workspace statistics: total cost, task counts, and per-branch
    /// divergence.
    pub async fn get_workspace_stats(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<WorkspaceStats, G5Error> {
        // Verify workspace exists
        let _workspace: Uuid = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM workspaces WHERE id = $1 AND deleted_at IS NULL"#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check workspace: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        // One ledger snapshot supplies the bounded task breakdown and remainder.
        #[derive(sqlx::FromRow)]
        struct TaskCostRow {
            id: Option<Uuid>,
            name: String,
            task_type: String,
            cost: f64,
        }
        let cost_rows = sqlx::query_as::<_, TaskCostRow>(
            "WITH task_costs AS (
                 SELECT t.id, t.name, t.kind AS task_type,
                        SUM(u.cost_usd)::double precision AS cost
                 FROM usage_records u JOIN tasks t
                   ON t.id = u.task_id AND t.org_id = u.org_id AND t.workspace_id = u.workspace_id
                 WHERE u.workspace_id = $1
                 GROUP BY t.id, t.name, t.kind
                 ORDER BY cost DESC, t.id LIMIT 100
             )
             SELECT id, name, task_type, cost FROM task_costs
             UNION ALL
             SELECT NULL, '', '',
                 COALESCE((SELECT SUM(cost_usd)::double precision FROM usage_records WHERE workspace_id = $1), 0.0)
                 - COALESCE((SELECT SUM(cost) FROM task_costs), 0.0)
             ORDER BY cost DESC, id",
        ).bind(workspace_id).fetch_all(db.conn()).await.map_err(|e| G5Error::Internal {
            message: format!("Failed to compute workspace cost breakdown: {e}"),
        })?;
        let total_cost_usd = cost_rows.iter().map(|row| row.cost).sum();
        let by_task = cost_rows
            .into_iter()
            .filter(|row| row.cost > 0.0)
            .filter_map(|row| {
                Some(TaskCostBreakdown {
                    task_id: row.id?,
                    task_name: row.name,
                    task_type: row.task_type,
                    cost_usd: row.cost,
                })
            })
            .collect();

        // Compute task counts
        #[derive(sqlx::FromRow)]
        struct TaskCounts {
            total: i64,
            completed: i64,
            active: i64,
            stopped: i64,
        }

        // Runtime query — avoids SQLX offline-cache regeneration for the added
        // `stopped` count (same pattern as the branch-rows query below).
        let task_counts = sqlx::query_as::<_, TaskCounts>(
            r#"
            SELECT
                COUNT(*) AS total,
                COUNT(*) FILTER (WHERE t.status = 'succeeded') AS completed,
                COUNT(*) FILTER (WHERE t.status = 'running') AS active,
                COUNT(*) FILTER (WHERE t.status = 'cancelled') AS stopped
            FROM tasks t
            WHERE t.workspace_id = $1
            "#,
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to compute task counts: {e}"),
        })?;

        // Resolve each repository member; this stats path leaves divergence
        // unknown because it does not capture a Git comparison.
        let branch_rows = self
            .load_workspace_repository_rows(db, &[workspace_id])
            .await?;
        let repo_names = self
            .load_repo_names(db, branch_rows.iter().map(|row| row.repo_id))
            .await?;

        let branches: Vec<BranchDivergence> = branch_rows
            .iter()
            .map(|b| {
                // The stats path computes no divergence; unknown is None,
                // never a fabricated 0 (ontology D50, issue #1995). Real
                // values come from the workspace-detail path only.
                BranchDivergence {
                    workspace_id: b.workspace_id,
                    repo_id: b.repo_id,
                    repo_name: repo_name_for(&repo_names, b.repo_id),
                    branch_name: b.branch_name.clone(),
                    ahead_of_base: None,
                    behind_base: None,
                }
            })
            .collect();

        Ok(WorkspaceStats {
            total_cost_usd,
            tasks_completed: task_counts.completed,
            total_tasks: task_counts.total,
            concurrent_tasks: task_counts.active,
            tasks_stopped: task_counts.stopped,
            by_task,
            branches,
        })
    }

    // -----------------------------------------------------------------------
    // Branch Management
    // -----------------------------------------------------------------------

    /// List all branches for a workspace with per-branch details.
    pub async fn list_branches(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceBranchDetail>, G5Error> {
        let rows = self
            .load_workspace_repository_rows(db, &[workspace_id])
            .await?;
        let repo_names = self
            .load_repo_names(db, rows.iter().map(|row| row.repo_id))
            .await?;

        let mut details: Vec<WorkspaceBranchDetail> = rows
            .into_iter()
            .map(|row| {
                let is_remote_tracking = row.tracking_mode == "remote_tracking";
                WorkspaceBranchDetail {
                    workspace_id: row.workspace_id,
                    repo_id: row.repo_id,
                    repo_name: repo_name_for(&repo_names, row.repo_id),
                    branch_name: row.branch_name.clone(),

                    head_commit: row.head_commit,
                    tracking_mode: row.tracking_mode,
                    materialization_status: row.materialization_status,
                    has_draft: row.has_draft,
                    dirty_file_count: row.dirty_file_count,
                    dirty_files: Vec::new(),
                    remote_branch: is_remote_tracking.then_some(row.remote_branch).flatten(),
                    local_branch_name: is_remote_tracking.then_some(row.branch_name),
                    co_tracking_workspaces: None,
                }
            })
            .collect();
        details.sort_by(|left, right| {
            left.repo_name
                .cmp(&right.repo_name)
                .then_with(|| left.branch_name.cmp(&right.branch_name))
                .then_with(|| left.repo_id.cmp(&right.repo_id))
        });

        Ok(details)
    }

    pub async fn initialize_local_workspace_repo(
        &self,
        key: &RepoKey,
        repo_name: &str,
        default_branch: &str,
    ) -> Result<String, G5Error> {
        let repo_id = key.repo_id();
        // Validate the branch name before any filesystem side effect so an
        // unsafe `default_branch` (spaces, `..`, control chars) returns a clean
        // 4xx instead of an internal git failure mid-initialization. Per Codex
        // review (PR #450).
        validate_git_branch_name(default_branch)?;
        let repo_path = self
            .repo_storage
            .init_repo(key)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to initialize repo storage for {repo_id}: {e}"),
            })?;
        let repo_name = repo_name.to_string();
        let default_branch = default_branch.to_string();

        tokio::task::spawn_blocking(move || -> Result<String, G5Error> {
            let repo = GitRepo::open_or_init_with_default_branch(&repo_path, &default_branch)?;

            let readme =
                format!("# {repo_name}\n\nThis is a G5-managed local project repository.\n");
            let files = vec![("README.md".to_string(), readme.into_bytes())];
            let author = GitAuthor {
                name: "G5".to_string(),
                email: "g5@local".to_string(),
            };
            g5_scm::prepare_initial_files_commit(
                &repo,
                g5_scm::InitialFilesCommit {
                    files: &files,
                    message: "Initialize local workspace repository",
                    author: &author,
                },
            )
        })
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Local repo initialization task failed: {e}"),
        })?
    }

    /// Promote an existing per-repo branch child to `remote_tracking` using the
    /// already persisted `repos.git_url`. This is the narrow "Make this branch
    /// remote" path; it deliberately does not accept or mutate a remote URL.
    #[allow(clippy::too_many_arguments)] // Coordinated remote-bind entry point: distinct request + gate inputs stay explicit.
    pub async fn promote_branch_repo_to_remote_tracking(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        remote_branch: &str,
        git_url: Option<&str>,
        allow_local_git_remotes: bool,
        create_provider_branch: bool,
        actor_user_id: Uuid,
        event_bus: &crate::services::EventBus,
    ) -> Result<PromoteRemoteTrackingResult, G5Error> {
        Self::validate_ref_name(remote_branch, "remote_branch")?;

        let mut row = self
            .load_branch_repo_for_remote_promotion(&mut db, workspace_id, repo_id)
            .await?;
        let repos = self.repo_storage.scoped(&db);
        let authority_inventory = resolve_committed_snapshot(
            &mut db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
        )
        .await?;
        let authority_member = authority_inventory
            .repos
            .iter()
            .find(|member| member.repo_id == row.repo_id);
        if !authority_member.is_some_and(|member| {
            member.workspace_id == row.workspace_id
                && member.repo_id == row.repo_id
                && member.head_commit == row.head_commit
        }) {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "REMOTE_BIND_HEAD_RACE",
                "The branch no longer matches its complete authoritative cut.",
                vec![Self::repo_evidence(
                    &row,
                    "remote_bind",
                    "failed",
                    Some("branch member differs from the captured authority inventory".to_string()),
                    None,
                )],
            ));
        }

        if row.is_base && remote_branch != row.default_branch {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "BASE_REMOTE_BIND_REQUIRES_DEFAULT_BRANCH",
                "Base workspace repo branches must bind to the repository default branch.",
                vec![Self::repo_evidence(
                    &row,
                    "remote_bind",
                    "blocked",
                    Some("Base tracks remote_branch = the repository default branch".to_string()),
                    None,
                )],
            ));
        }

        if row.tracking_mode == "remote_tracking" {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "BRANCH_ALREADY_REMOTE_TRACKED",
                "This branch repo is already remote-tracked.",
                vec![Self::repo_evidence(
                    &row,
                    "remote_bind",
                    "blocked",
                    Some("branch repo is already remote-tracked".to_string()),
                    None,
                )],
            ));
        }
        self.preflight_branch_repo_clean_and_materialized(&row, "remote_bind")?;
        if let Some(reason) = row.sync_paused_reason.as_deref() {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "REMOTE_BIND_PAUSED",
                "The branch repo must not be paused while remote tracking is being enabled.",
                vec![Self::repo_evidence(
                    &row,
                    "remote_bind",
                    "blocked",
                    Some(format!("remote tracking is paused: {reason}")),
                    None,
                )],
            ));
        }

        // Make Remote is an organization-administrator action (`g5.git.make-remote`):
        // this operation establishes or re-points a repository's external binding,
        // a provider-affecting change to the shared binding. (Publish of an
        // inherited remote is the separate `publish_branch_repo` operation and
        // stays the owner's.) The route already requires the workspace owner, or
        // an administrator on Base; require the actor to be an organization
        // administrator here too, so an ordinary workspace needs an owner who is
        // an administrator and Base needs an administrator. Refused with a typed
        // `Forbidden` before any provider I/O — no remote-ref lookup, create-push
        // or mirror preparation (`g5.git.remote-binding`) — covering the
        // existing-URL bind and the `git_url` re-point alike, and every caller of
        // the operation.
        if !self.actor_is_org_admin(row.org_id, actor_user_id).await? {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["org_admin_make_remote".to_string()],
            });
        }

        let captured_row = row.clone();
        let application_scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!(
                "Failed to close remote-tracking promotion capture transaction: {error}"
            ),
        })?;

        // Make-Remote Adopt-Remote git-URL re-point (D46 item 1). Runs at the
        // start of promotion — after ownership/materialization preflight, before
        // remote-tip resolution — so the rest of the ordinary [D28] promotion
        // (ls-remote, classify, bind) targets the newly-adopted remote URL. The
        // re-point is a strict, additive extension: it activates ONLY when the
        // a `git_url` that
        // differs from the current identity is supplied. A `git_url` of
        // None/equal leaves `row` and `repos.git_url` byte-for-byte as [D28].
        self.maybe_adopt_remote_repoint(
            &application_scope,
            &mut row,
            git_url,
            allow_local_git_remotes,
            actor_user_id,
        )
        .await?;
        if row.git_url != captured_row.git_url {
            self.refresh_mirror_origin(&repos, row.org_id, row.repo_id, &row.git_url)
                .await?;
        }

        // Resolve the remote tip. A genuinely-absent ref (`ls_remote_ref` ->
        // NotFound) is the ONLY error that the promotion path re-routes; every
        // other resolution failure (auth, unreachable) still fails through the
        // `REMOTE_BIND_REF_LOOKUP_FAILED` mapping.
        let resolve_result = self
            .resolve_remote_tip_for_row(
                &row,
                remote_branch,
                Some(actor_user_id),
                (row.git_url != captured_row.git_url).then_some(captured_row.git_url.as_str()),
            )
            .await;

        // An absent remote ref is published with create-only semantics, then
        // bound. Base publishes to its provider only here, and only once the
        // actor has confirmed creating the branch (`g5.workspace.remote.base-parity`).
        {
            if let (Err(G5Error::NotFound { .. }), true, false) =
                (&resolve_result, row.is_base, create_provider_branch)
            {
                return Err(Self::coordinated_validation_error(
                    "remote_bind",
                    "PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION",
                    "The provider branch does not exist. Confirm to create it from Base's current history.",
                    vec![Self::repo_evidence(
                        &row,
                        "remote_bind",
                        "blocked",
                        Some(format!("provider branch '{remote_branch}' does not exist")),
                        None,
                    )],
                ));
            }
            if let Err(G5Error::NotFound { .. }) = &resolve_result {
                let head = row.head_commit.clone();
                self.create_only_publish_head(
                    &repos,
                    &row,
                    remote_branch,
                    &head,
                    actor_user_id,
                    (row.git_url != captured_row.git_url).then_some(captured_row.git_url.as_str()),
                )
                .await?;
                self.finalize_make_remote_application(
                    &application_scope,
                    &authority_inventory,
                    &captured_row,
                    &row,
                    &head,
                    remote_branch,
                    &head,
                    &head,
                    None,
                    crate::models::branch_sync_run::status::NOOP,
                    "published",
                    None,
                    actor_user_id,
                    event_bus,
                )
                .await?;
                return Ok(self.build_promote_remote_tracking_result(
                    row,
                    remote_branch,
                    &head,
                    head.clone(),
                    head.clone(),
                    "succeeded",
                    "remote_tracked",
                    "published",
                    None,
                ));
            }
        }

        let (remote_tip, auth) = resolve_result.map_err(|err| {
            Self::coordinated_validation_error(
                "remote_bind",
                "REMOTE_BIND_REF_LOOKUP_FAILED",
                "The selected remote branch could not be resolved for this repo.",
                vec![Self::repo_evidence(
                    &row,
                    "remote_bind",
                    "failed",
                    Some(err.to_string()),
                    None,
                )],
            )
        })?;
        let relation = self
            .prepare_mirror_and_classify(&repos, &row, remote_branch, &remote_tip, &auth)
            .await
            .map_err(|err| {
                Self::coordinated_validation_error(
                    "remote_bind",
                    "REMOTE_BIND_CLASSIFICATION_FAILED",
                    "The selected remote branch could not be classified against the local branch.",
                    vec![Self::repo_evidence(
                        &row,
                        "remote_bind",
                        "failed",
                        Some(err.to_string()),
                        Some(remote_tip.clone()),
                    )],
                )
            })?;

        // Divergent or local-ahead lineage enters the merge workflow. Base,
        // which has no VM, reconciles it by accepting the provider history
        // (g5.workspace.remote.base-provider-rewrite).
        if matches!(
            relation,
            CoordinatedTipRelation::WorkspaceAhead
                | CoordinatedTipRelation::WorkspaceRewritten
                | CoordinatedTipRelation::Diverged
                | CoordinatedTipRelation::WatermarkViolation
        ) {
            let local_head = row.head_commit.clone();
            let merge_run_id = Uuid::now_v7();
            self.finalize_make_remote_application(
                &application_scope,
                &authority_inventory,
                &captured_row,
                &row,
                &local_head,
                remote_branch,
                ZERO_GIT_SHA,
                &remote_tip,
                Some("diverged"),
                crate::models::branch_sync_run::status::DIVERGED,
                "merge_required",
                Some(merge_run_id),
                actor_user_id,
                event_bus,
            )
            .await?;
            return Ok(self.build_promote_remote_tracking_result(
                row,
                remote_branch,
                &local_head,
                local_head.clone(),
                local_head.clone(),
                "merge_required",
                "merge_required",
                "merge_required",
                Some(merge_run_id),
            ));
        }

        if !matches!(
            relation,
            CoordinatedTipRelation::Equal | CoordinatedTipRelation::MirrorAhead
        ) {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "INCOMPATIBLE_REMOTE_LINEAGE",
                "The current branch tip cannot be fast-forwarded to the selected remote branch.",
                vec![Self::repo_evidence(
                    &row,
                    "remote_bind",
                    "failed",
                    Some(format!("incompatible lineage: {relation:?}")),
                    Some(remote_tip.clone()),
                )],
            ));
        }

        {
            let proposed_head = remote_tip.clone();
            let sync_status = if relation == CoordinatedTipRelation::MirrorAhead {
                crate::models::branch_sync_run::status::IMPORTED
            } else {
                crate::models::branch_sync_run::status::NOOP
            };
            self.finalize_make_remote_application(
                &application_scope,
                &authority_inventory,
                &captured_row,
                &row,
                &proposed_head,
                remote_branch,
                &remote_tip,
                &remote_tip,
                None,
                sync_status,
                "bound",
                None,
                actor_user_id,
                event_bus,
            )
            .await?;
            Ok(self.build_promote_remote_tracking_result(
                row,
                remote_branch,
                &proposed_head,
                captured_row.head_commit.clone(),
                proposed_head.clone(),
                "succeeded",
                "remote_tracked",
                "bound",
                None,
            ))
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn finalize_make_remote_application(
        &self,
        application_scope: &ScopedDbReopener,
        authority_inventory: &CommittedBindingSnapshot,
        captured_row: &CoordinatedWorkspaceRepoRow,
        proposed_row: &CoordinatedWorkspaceRepoRow,
        proposed_head: &str,
        remote_branch: &str,
        proposed_baseline: &str,
        proposed_advisory: &str,
        proposed_pause: Option<&str>,
        sync_status: &str,
        outcome: &str,
        merge_run_id: Option<Uuid>,
        actor_user_id: Uuid,
        event_bus: &crate::services::EventBus,
    ) -> Result<(), G5Error> {
        let exact_repos = exact_workspace_operation_repos(
            authority_inventory,
            &BTreeMap::from([(proposed_row.repo_id, proposed_head.to_string())]),
        )?;
        let advance = prepare_branch_commit(
            application_scope,
            &self.repo_storage,
            authority_inventory.target.clone(),
            exact_repos,
            "make_remote",
        )
        .await
        .map_err(|error| {
            Self::coordinated_validation_error(
                "remote_bind",
                "REMOTE_BIND_IMPORT_FAILED",
                "The branch could not prepare its complete authoritative cut during binding.",
                vec![Self::repo_evidence(
                    proposed_row,
                    "remote_bind",
                    "failed",
                    Some(error.to_string()),
                    Some(proposed_head.to_string()),
                )],
            )
        })?;
        let member = advance.member(proposed_row.repo_id);
        if !member.is_some_and(|member| {
            member.workspace_id == proposed_row.workspace_id
                && member.repo_id == proposed_row.repo_id
                && member.expected_old_head == captured_row.head_commit
                && member.proposed_new_head == proposed_head
        }) {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "REMOTE_BIND_HEAD_RACE",
                "The branch head changed while binding it to the remote branch.",
                vec![Self::repo_evidence(
                    proposed_row,
                    "remote_bind",
                    "failed",
                    Some("branch head race during operation preparation".to_string()),
                    Some(proposed_head.to_string()),
                )],
            ));
        }

        let proposal = PreparedMakeRemoteApplication {
            org_id: captured_row.org_id,
            workspace_id: authority_inventory.workspace_id,
            repo_id: captured_row.repo_id,
            expected_git_url: captured_row.git_url.clone(),
            expected_provider: captured_row.provider.clone(),
            expected_provider_repo_id: captured_row.provider_repo_id.clone(),
            expected_provider_full_name: captured_row.provider_full_name.clone(),
            proposed_git_url: proposed_row.git_url.clone(),
            proposed_provider: proposed_row.provider.clone(),
            proposed_provider_repo_id: proposed_row.provider_repo_id.clone(),
            proposed_provider_full_name: proposed_row.provider_full_name.clone(),
            expected_tracking_mode: captured_row.tracking_mode.clone(),
            expected_remote_branch: captured_row.remote_branch.clone(),
            expected_baseline_commit: captured_row.baseline_commit.clone(),
            expected_remote_tip_advisory_sha: captured_row.remote_tip_advisory_sha.clone(),
            expected_sync_paused_reason: captured_row.sync_paused_reason.clone(),
            expected_head: captured_row.head_commit.clone(),
            proposed_head: proposed_head.to_string(),
            proposed_tracking_mode: "remote_tracking".to_string(),
            proposed_remote_branch: remote_branch.to_string(),
            proposed_baseline_commit: proposed_baseline.to_string(),
            proposed_remote_tip_advisory_sha: proposed_advisory.to_string(),
            proposed_sync_paused_reason: proposed_pause.map(str::to_string),
            acting_user_id: actor_user_id,
            trigger_kind: crate::models::branch_sync_run::trigger_kind::MANUAL.to_string(),
            sync_status: sync_status.to_string(),
            outcome: outcome.to_string(),
            merge_run_id,
        };

        let application_db = application_scope
            .open()
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to open Make-Remote application transaction: {error}"),
            })?;
        let publication = crate::services::remote_integrations::prepare_binding_publication(
            advance,
            &self.repo_storage.scoped(application_scope),
            proposal,
        )?;
        let (landed, ()) = publication
            .land_interactive_bounded(
                application_db,
                event_bus,
                &self.repo_storage,
            )
            .await
            .map_err(|error| match error {
                BranchCommitError::FenceBusy { .. } => {
                    Self::coordinated_validation_error(
                        "remote_bind",
                        "WORKSPACE_COMMIT_FENCE_BUSY",
                        "Another operation is committing this workspace; retry shortly.",
                        vec![Self::repo_evidence(
                            proposed_row,
                            "remote_bind",
                            "blocked",
                            Some("workspace commit fence is busy".to_string()),
                            None,
                        )],
                    )
                }
                error => Self::coordinated_validation_error(
                    "remote_bind",
                    "REMOTE_BIND_ROLLED_BACK",
                    "The repository identity, branch tracking tuple, or complete cut changed while Make-Remote was being finalized; no partial state was committed.",
                    vec![Self::repo_evidence(
                        proposed_row,
                        "remote_bind",
                        "failed_rolled_back",
                        Some(error.to_string()),
                        Some(proposed_head.to_string()),
                    )],
                ),
            })?;
        for warning in &landed.projection_warnings {
            tracing::warn!(
                advance_id = %landed.advance_id,
                %warning,
                "Make-Remote landed while a follow-up remains pending"
            );
        }
        // Binding to a remote that is ahead imports its tip: the branch
        // moved, so the workspace's reviewers are told (the notifier skips
        // a cut whose heads did not change, as with publish and diverged).
        crate::services::review_source_notifications::ReviewSourceNotifier::new(
            self.system_pool.clone(),
            self.tenant_pool.clone(),
            self.authz.clone(),
            event_bus.clone(),
        )
        .notify_reviewers_after_landing(&landed, actor_user_id)
        .await;
        Ok(())
    }

    /// Shared constructor for the `promote_branch_repo_to_remote_tracking`
    /// response across all three D46 outcomes ("bound", "published",
    /// "merge_required"). `summary_head` is the head shown on the branch
    /// summary; `old_commit`/`new_commit` are the coordinated-evidence commits.
    #[allow(clippy::too_many_arguments)]
    fn build_promote_remote_tracking_result(
        &self,
        row: CoordinatedWorkspaceRepoRow,
        remote_branch: &str,
        summary_head: &str,
        old_commit: String,
        new_commit: String,
        coordinated_status: &str,
        evidence_status: &str,
        outcome: &str,
        merge_run_id: Option<Uuid>,
    ) -> PromoteRemoteTrackingResult {
        let meta = RepoChipMeta::new(
            row.repo_name.clone(),
            row.provider_full_name.clone(),
            &row.git_url,
            self.repository_credentials.origin_policy(),
        );
        let branch = BranchSummary {
            workspace_id: row.workspace_id,
            repo_id: row.repo_id,
            repo_name: row.repo_name.clone(),
            branch_name: row.branch_name.clone(),

            tracking_mode: "remote_tracking".to_string(),
            head_commit: summary_head.to_string(),
            materialization_status: row.materialization_status.clone(),
            divergence: None,
            behind_base: None,
            has_draft: false,
            dirty_file_count: 0,
            remote_branch: Some(remote_branch.to_string()),
            local_branch_name: Some(row.branch_name.clone()),
            remote_repo_name: meta.remote_repo_name(),
            co_tracking_workspaces: None,
            co_tracking_workspace_count: None,
            auto_pull_enabled: None,
            remote_eligible: None,
        };
        let evidence = CoordinatedRepoEvidence {
            workspace_id: Some(row.workspace_id),
            repo_id: row.repo_id,
            repo_name: row.repo_name,
            branch_name: row.branch_name,
            remote_branch: Some(remote_branch.to_string()),
            operation: "remote_bind".to_string(),
            status: evidence_status.to_string(),
            old_commit: Some(old_commit),
            new_commit: Some(new_commit),
            error: None,
        };
        PromoteRemoteTrackingResult {
            branch,
            coordinated: CoordinatedOperationResult {
                operation: "remote_bind".to_string(),
                status: coordinated_status.to_string(),
                message: None,
                repos: vec![evidence],
            },
            outcome: outcome.to_string(),
            merge_run_id,
        }
    }

    /// Create-only publication of `head` to a new remote ref `remote_branch`,
    /// reusing the [D43] create-only refspec shape (`GitRepo::push_create_ref`).
    /// Resolves SCM auth for the acting user, probes the ref (an existing ref at
    /// the SAME head is an idempotent attach; a DIFFERENT head is an explicit
    /// conflict, never an overwrite), pushes when free, and confirms the ref
    /// landed at `head` before returning `Ok`. On any failure the child is left
    /// local-only.
    ///
    /// NOTE: this mirrors the create-and-confirm publish primitive inside
    /// `publish_branch_repo` ([D43]); the shared-publish-primitive extraction is
    /// intentionally deferred for now to keep this change surgical.
    async fn create_only_publish_head(
        &self,
        repos: &ScopedRepoStorage,
        row: &CoordinatedWorkspaceRepoRow,
        remote_branch: &str,
        head: &str,
        actor_user_id: Uuid,
        adopted_from: Option<&str>,
    ) -> Result<(), G5Error> {
        let git_url = row.git_url.clone();
        let actor = actor_user_id;
        let auth = match adopted_from {
            Some(expected) => {
                self.repository_credentials
                    .resolve_adoption(
                        row.org_id,
                        row.repo_id,
                        actor,
                        expected,
                        &row.git_url,
                        crate::auth::RepositoryOperation::WriteGit,
                    )
                    .await?
            }
            None => {
                self.repository_credentials
                    .resolve(
                        row.org_id,
                        row.repo_id,
                        actor,
                        &row.git_url,
                        crate::auth::RepositoryOperation::WriteGit,
                    )
                    .await?
            }
        };

        // Probe: same head → attach (idempotent); different head → conflict;
        // absent → create.
        let needs_create = {
            let probe = remote_branch_tip(&git_url, remote_branch, &auth, NETWORK_DEADLINE).await;
            match probe {
                Ok(existing) if existing == head => false,
                Ok(_existing) => {
                    return Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "code": "PUBLISH_TARGET_REF_EXISTS",
                            "operation": "remote_bind",
                            "repoId": row.repo_id,
                            "repoName": row.repo_name,
                            "remoteBranch": remote_branch,
                            "message": format!("remote branch '{remote_branch}' already exists on '{git_url}' at a different commit; binding would not overwrite it"),
                        }),
                    });
                }
                Err(G5Error::NotFound { .. }) => true,
                Err(other) => return Err(other),
            }
        };

        if needs_create {
            let local_path = repos.repo_path(row.org_id, row.repo_id)?;
            publish_create_ref(&local_path, &git_url, head, remote_branch, &auth).await?;
        }

        // Confirm the remote ref landed at the exact head before the DB bind.
        let confirmed_tip =
            remote_branch_tip(&git_url, remote_branch, &auth, NETWORK_DEADLINE).await?;
        if confirmed_tip != head {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "PUBLISH_UNCONFIRMED",
                    "operation": "remote_bind",
                    "repoId": row.repo_id,
                    "repoName": row.repo_name,
                    "remoteBranch": remote_branch,
                    "message": format!("remote branch '{remote_branch}' did not confirm at the published head; leaving the branch local-only"),
                }),
            });
        }
        Ok(())
    }

    /// Preflight the expected code+docs set and enqueue one existing push cycle
    /// per eligible remote-tracked repo child.
    pub async fn push_to_remote(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<PushToRemoteResult, G5Error> {
        let actor = db.actor_id().into_uuid();
        let push_authority = crate::models::branch_integration_queue::PushAuthority::capture(
            db.conn(),
            workspace_id,
            actor,
        )
        .await?;
        let is_base: bool = sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_optional(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load workspace base flag: {e}"),
            })?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace {workspace_id}"),
            })?;
        // Base never publishes to its provider (`g5.workspace.remote.base-parity`).
        if is_base {
            return Err(Self::coordinated_validation_error(
                "push",
                "COORDINATED_PUSH_REQUIRES_NON_BASE",
                "Base never pushes to its provider; it follows the provider through Pull and auto-pull.",
                Vec::new(),
            ));
        }
        let rows = self
            .load_expected_coordinated_repo_rows(db, workspace_id, true)
            .await?;
        self.acquire_remote_tracking_locks(db, &rows, "push")
            .await?;

        // Lazy publication (#1198, D30 / external_git_integration#D7): a
        // pinned/local-only child has no remote to push to, so it is SKIPPED
        // (recorded as `skipped` evidence) rather than blocking the whole push.
        // The all-or-none entry boundary still applies to the remote-tracking
        // children: a genuine problem on a tracked repo (unmaterialized, missing
        // remote branch, or paused) still blocks the operation.
        let mut skipped = Vec::new();
        let mut blocked = Vec::new();
        let mut materialization_blocked = Vec::new();
        let mut eligible: Vec<&CoordinatedWorkspaceRepoRow> = Vec::new();
        for row in &rows {
            if row.tracking_mode != "remote_tracking" {
                skipped.push(Self::repo_evidence(
                    row,
                    "push",
                    "skipped",
                    Some("repo is not remote-tracked; skipped".to_string()),
                    None,
                ));
                continue;
            }
            if let Err(err) = self.preflight_branch_repo_clean_and_materialized(row, "push") {
                materialization_blocked.push(Self::repo_evidence(
                    row,
                    "push",
                    "blocked",
                    Some(err.to_string()),
                    None,
                ));
            } else if row.remote_branch.is_none() {
                blocked.push(Self::repo_evidence(
                    row,
                    "push",
                    "blocked",
                    Some("remote branch is not configured".to_string()),
                    None,
                ));
            } else if row
                .sync_paused_reason
                .as_deref()
                .is_some_and(|reason| reason != "watermark_violation")
            {
                // The worker rechecks a provider-rewrite pause before pushing:
                // a workspace rewrite publishes, a provider rewrite stays paused.
                let reason = row.sync_paused_reason.as_deref().unwrap_or("unknown");
                let message = format!("remote tracking is paused: {reason}");
                blocked.push(Self::repo_evidence(
                    row,
                    "push",
                    "blocked",
                    Some(message),
                    None,
                ));
            } else {
                eligible.push(row);
            }
        }
        if !materialization_blocked.is_empty() && blocked.is_empty() {
            materialization_blocked.extend(skipped);
            return Err(Self::coordinated_validation_error(
                "push",
                "BRANCH_NOT_MATERIALIZED",
                "The branch repo must be materialized before coordinated sync.",
                materialization_blocked,
            ));
        }
        blocked.extend(materialization_blocked);
        if !blocked.is_empty() {
            blocked.extend(skipped);
            return Err(Self::coordinated_validation_error(
                "push",
                "COORDINATED_PUSH_TRACKED_REPO_BLOCKED",
                "A remote-tracked repo is not ready to push (unmaterialized, paused, or missing its remote branch).",
                blocked,
            ));
        }

        // No genuine blocks. If every child was skipped (none remote-tracking),
        // there is nothing to push — a soft no-op, not an error.
        if eligible.is_empty() {
            return Ok(PushToRemoteResult {
                enqueued: 0,
                coordinated: Some(CoordinatedOperationResult {
                    operation: "push".to_string(),
                    status: "noop".to_string(),
                    message: Some("No remote-tracking repos to push.".to_string()),
                    repos: skipped,
                }),
            });
        }

        for row in &eligible {
            crate::models::branch_integration_queue::enqueue_authorized_branch_integration(
                db.conn(),
                row.key(),
                crate::models::branch_integration_queue::trigger_kind::PUSH,
                Utc::now(),
                push_authority,
            )
            .await?;
        }
        let enqueued = eligible.len() as u64;

        let skipped_count = skipped.len();
        let eligible_count = eligible.len();
        let mut repos: Vec<_> = eligible
            .into_iter()
            .map(|row| {
                let mut evidence = Self::repo_evidence(row, "push", "queued", None, None);
                evidence.new_commit = Some(row.head_commit.clone());
                evidence
            })
            .collect();
        repos.extend(skipped);

        let status = if skipped_count == 0 && enqueued == eligible_count as u64 {
            "queued"
        } else {
            "queued_partial"
        };
        let message = Some(format!("Queued push for {enqueued} repo(s)."));

        Ok(PushToRemoteResult {
            enqueued,
            coordinated: Some(CoordinatedOperationResult {
                operation: "push".to_string(),
                status: status.to_string(),
                message,
                repos,
            }),
        })
    }

    pub async fn approved_rebase_publish(
        &self,
        mut capture_db: ScopedDb,
        workspace_id: Uuid,
        actor_user_id: Uuid,
        expected_merge_run_ids: Option<Vec<Uuid>>,
    ) -> Result<(ScopedDb, ApprovedRebasePublishResult), G5Error> {
        let push_authority = crate::models::branch_integration_queue::PushAuthority::capture(
            capture_db.conn(),
            workspace_id,
            actor_user_id,
        )
        .await?;
        let is_base: bool = sqlx::query_scalar("SELECT is_base FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_optional(capture_db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to load workspace base flag: {e}"),
            })?
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace {workspace_id}"),
            })?;
        if is_base {
            return Err(Self::coordinated_validation_error(
                "approved_rebase_publish",
                "APPROVED_REBASE_PUBLISH_REQUIRES_NON_BASE",
                "Base cannot publish a rebased candidate over a provider rewrite; it has no VM to prepare one.",
                Vec::new(),
            ));
        }

        let rows = self
            .load_expected_coordinated_repo_rows(&mut capture_db, workspace_id, false)
            .await?;

        // Classify each expected repo into a publication lane (spec D36 /
        // external_git_integration#D7). Ineligible repos — non-tracked,
        // dirty/unmaterialized, missing remote branch, or paused for a reason
        // approved rebase publication cannot resolve (e.g. `diverged`) — block
        // the whole operation before any queue mutation, preserving the
        // all-or-none entry boundary. Eligible repos split into a reset lane
        // (`watermark_violation` -> reviewed candidate linearization) and a normal
        // lane (unpaused -> fast-forward push). A workspace may legitimately
        // have rebased only one repo child, so uniform pause state is NOT
        // required — only that at least one repo needs the reviewed reset.
        let mut blocked = Vec::new();
        let mut reset_lane_count = 0_usize;
        for row in &rows {
            if row.tracking_mode != "remote_tracking" {
                blocked.push(Self::repo_evidence(
                    row,
                    "approved_rebase_publish",
                    "blocked",
                    Some("repo is not remote-tracked".to_string()),
                    None,
                ));
                continue;
            }
            if let Err(err) =
                self.preflight_branch_repo_clean_and_materialized(row, "approved_rebase_publish")
            {
                blocked.push(Self::repo_evidence(
                    row,
                    "approved_rebase_publish",
                    "blocked",
                    Some(err.to_string()),
                    None,
                ));
                continue;
            }
            if row.remote_branch.is_none() {
                blocked.push(Self::repo_evidence(
                    row,
                    "approved_rebase_publish",
                    "blocked",
                    Some("remote branch is not configured".to_string()),
                    None,
                ));
                continue;
            }
            match row.sync_paused_reason.as_deref() {
                Some("watermark_violation") => {
                    reset_lane_count += 1;
                }
                None => {
                    // Normal lane: unpaused and otherwise eligible. It publishes
                    // by the same fast-forward-only push `push_to_remote` uses.
                }
                Some(other) => {
                    blocked.push(Self::repo_evidence(
                        row,
                        "approved_rebase_publish",
                        "blocked",
                        Some(format!(
                            "remote tracking is paused for '{other}', which approved rebase publication cannot resolve"
                        )),
                        None,
                    ));
                }
            }
        }
        if !blocked.is_empty() {
            return Err(Self::coordinated_validation_error(
                "approved_rebase_publish",
                "APPROVED_REBASE_PUBLISH_REQUIRES_RESET_REVIEW",
                "Every expected repo must be clean, remote-tracked, and either paused for reset-below-baseline publication approval or ready for a fast-forward push.",
                blocked,
            ));
        }
        if reset_lane_count == 0 {
            return Err(Self::coordinated_validation_error(
                "approved_rebase_publish",
                "APPROVED_REBASE_PUBLISH_REQUIRES_RESET_REVIEW",
                "No expected repo is paused for reset-below-baseline publication approval; use push_to_remote for a fast-forward publish.",
                rows.iter()
                    .map(|row| {
                        Self::repo_evidence(
                            row,
                            "approved_rebase_publish",
                            "blocked",
                            Some("no repo requires reset publication approval".to_string()),
                            None,
                        )
                    })
                .collect(),
            ));
        }

        struct PreparedResetLane {
            checkpoint:
                crate::services::remote_tracking_merge::RemoteTrackingRebasePublishCheckpoint,
            local_tip: String,
            live_remote_tip: String,
            provider_advanced: bool,
        }

        let mut captured_checkpoints = HashMap::new();
        for row in rows
            .iter()
            .filter(|row| row.sync_paused_reason.as_deref() == Some("watermark_violation"))
        {
            let checkpoint = crate::services::remote_tracking_merge::load_remote_tracking_rebase_publish_checkpoint(
                capture_db.conn(),
                row.key(),
            )
            .await
            .map_err(|error| {
                Self::coordinated_validation_error(
                    "approved_rebase_publish",
                    "APPROVED_REBASE_PROOF_REJECTED",
                    "The active rebase-publication approval checkpoint could not be loaded.",
                    vec![Self::repo_evidence(
                        row,
                        "approved_rebase_publish",
                        "blocked",
                        Some(error.to_string()),
                        None,
                    )],
                )
            })?;
            captured_checkpoints.insert(row.repo_id, checkpoint);
        }
        let mut captured_checkpoint_ids: Vec<Uuid> = captured_checkpoints
            .values()
            .map(|checkpoint| checkpoint.id)
            .collect();
        captured_checkpoint_ids.sort_unstable();
        let refreshed_checkpoint_present = captured_checkpoints.values().any(|checkpoint| {
            checkpoint
                .target_heads
                .get("refresh_reason")
                .and_then(|value| value.as_str())
                == Some("provider_descendant_advanced")
        });
        if let Some(mut expected_merge_run_ids) = expected_merge_run_ids {
            expected_merge_run_ids.sort_unstable();
            let has_duplicate = expected_merge_run_ids
                .windows(2)
                .any(|ids| ids[0] == ids[1]);
            if has_duplicate || expected_merge_run_ids.len() != captured_checkpoint_ids.len() {
                return Err(Self::coordinated_validation_error(
                    "approved_rebase_publish",
                    "APPROVED_REBASE_CHECKPOINT_SELECTION_REQUIRED",
                    "Reload and approve the complete set of rebase-publication checkpoints displayed for this workspace.",
                    rows.iter()
                        .filter(|row| {
                            row.sync_paused_reason.as_deref() == Some("watermark_violation")
                        })
                        .map(|row| {
                            Self::repo_evidence(
                                row,
                                "approved_rebase_publish",
                                "blocked",
                                Some("the approval selection must contain every active reset checkpoint exactly once".to_string()),
                                None,
                            )
                        })
                        .collect(),
                ));
            }
            if expected_merge_run_ids != captured_checkpoint_ids {
                return Err(Self::coordinated_validation_error(
                    "approved_rebase_publish",
                    "APPROVED_REBASE_CHECKPOINT_CHANGED",
                    "The rebase-publication approval checkpoints shown to the user are no longer the active complete set; reload before approving.",
                    rows.iter()
                        .filter(|row| {
                            row.sync_paused_reason.as_deref() == Some("watermark_violation")
                        })
                        .map(|row| {
                            Self::repo_evidence(
                                row,
                                "approved_rebase_publish",
                                "blocked",
                                Some("the displayed approval checkpoint set is no longer active".to_string()),
                                None,
                            )
                        })
                        .collect(),
                ));
            }
        } else if captured_checkpoint_ids.len() != 1 || refreshed_checkpoint_present {
            return Err(Self::coordinated_validation_error(
                "approved_rebase_publish",
                "APPROVED_REBASE_CHECKPOINT_SELECTION_REQUIRED",
                "Reload and explicitly approve the complete set of rebase-publication checkpoints.",
                rows.iter()
                    .filter(|row| row.sync_paused_reason.as_deref() == Some("watermark_violation"))
                    .map(|row| {
                        Self::repo_evidence(
                            row,
                            "approved_rebase_publish",
                            "blocked",
                            Some(
                                "every active approval checkpoint must be selected explicitly"
                                    .to_string(),
                            ),
                            None,
                        )
                    })
                    .collect(),
            ));
        }

        // Provider and local Git inspection must not retain the RLS transaction
        // or the transaction-scoped remote-tracking locks. Application reopens
        // the same authorization scope and revalidates every captured row.
        let application_scope = capture_db.reopener();
        capture_db
            .rollback()
            .await
            .map_err(|error| G5Error::Internal {
                message: format!(
                    "Failed to close approved rebase publication capture transaction: {error}"
                ),
            })?;

        let mut prepared_reset_lanes = HashMap::new();
        for row in rows
            .iter()
            .filter(|row| row.sync_paused_reason.as_deref() == Some("watermark_violation"))
        {
            let checkpoint = captured_checkpoints
                .remove(&row.repo_id)
                .expect("every reset lane has a captured approval checkpoint");
            let remote_branch = row
                .remote_branch
                .as_deref()
                .expect("validated remote branch");
            let local_tip = checkpoint
                .source_heads
                .get("workspace_tip")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    Self::coordinated_validation_error(
                        "approved_rebase_publish",
                        "APPROVED_REBASE_PROOF_REJECTED",
                        "The active rebase-publication approval checkpoint is missing its reviewed workspace candidate.",
                        vec![Self::repo_evidence(
                            row,
                            "approved_rebase_publish",
                            "blocked",
                            Some("reviewed workspace candidate is missing".to_string()),
                            None,
                        )],
                    )
                })?;
            let (live_remote_tip, auth) = self
                .resolve_remote_tip_for_row(row, remote_branch, Some(actor_user_id), None)
                .await
                .map_err(|_| {
                    Self::coordinated_validation_error(
                        "approved_rebase_publish",
                        "REMOTE_TIP_LOOKUP_FAILED",
                        "The remote branch tip could not be verified before approved rebase publication.",
                        vec![Self::repo_evidence(
                            row,
                            "approved_rebase_publish",
                            "blocked",
                            Some(
                                "remote branch tip verification failed; check repository access and retry"
                                    .to_string(),
                            ),
                            None,
                        )],
                    )
                })?;
            let reviewed_remote_tip = checkpoint.reviewed_remote_tip().map_err(|error| {
                Self::coordinated_validation_error(
                    "approved_rebase_publish",
                    "APPROVED_REBASE_PROOF_REJECTED",
                    "The active rebase-publication approval checkpoint is incomplete.",
                    vec![Self::repo_evidence(
                        row,
                        "approved_rebase_publish",
                        "blocked",
                        Some(error.to_string()),
                        None,
                    )],
                )
            })?;

            let provider_advanced = if reviewed_remote_tip == live_remote_tip {
                false
            } else {
                let mut ancestry_row = row.clone();
                ancestry_row.head_commit = reviewed_remote_tip.to_string();
                let relation = self
                    .prepare_mirror_and_classify(
                        &self.repo_storage.scoped(&application_scope),
                        &ancestry_row,
                        remote_branch,
                        &live_remote_tip,
                        &auth,
                    )
                    .await
                    .map_err(|error| {
                        Self::coordinated_validation_error(
                            "approved_rebase_publish",
                            "APPROVED_REBASE_REMOTE_ANCESTRY_UNVERIFIABLE",
                            "The provider tip relationship to the approval checkpoint could not be proven.",
                            vec![Self::repo_evidence(
                                row,
                                "approved_rebase_publish",
                                "blocked",
                                Some(error.to_string()),
                                Some(live_remote_tip.clone()),
                            )],
                        )
                    })?;
                if relation != CoordinatedTipRelation::MirrorAhead {
                    return Err(Self::coordinated_validation_error(
                        "approved_rebase_publish",
                        "APPROVED_REBASE_REMOTE_HISTORY_CHANGED",
                        "The provider branch no longer descends from the approved checkpoint; rebase against the current provider history before publishing.",
                        vec![Self::repo_evidence(
                            row,
                            "approved_rebase_publish",
                            "blocked",
                            Some("provider history is not a strict descendant of the approved checkpoint".to_string()),
                            Some(live_remote_tip.clone()),
                        )],
                    ));
                }
                true
            };

            prepared_reset_lanes.insert(
                row.repo_id,
                PreparedResetLane {
                    checkpoint,
                    local_tip,
                    live_remote_tip,
                    provider_advanced,
                },
            );
        }

        let mut application_db =
            application_scope
                .open()
                .await
                .map_err(|error| G5Error::Internal {
                    message: format!(
                "Failed to open approved rebase publication application transaction: {error}"
            ),
                })?;
        if crate::models::branch_integration_queue::PushAuthority::capture(
            application_db.conn(),
            workspace_id,
            actor_user_id,
        )
        .await?
            != push_authority
        {
            return Err(G5Error::Conflict {
                reason: "Workspace lifecycle changed during publication approval; approve again"
                    .into(),
            });
        }
        let current_rows = self
            .load_expected_coordinated_repo_rows(&mut application_db, workspace_id, true)
            .await?;
        if current_rows != rows {
            return Err(Self::coordinated_validation_error(
                "approved_rebase_publish",
                "APPROVED_REBASE_STATE_CHANGED",
                "Workspace authority or remote-tracking state changed while provider history was being verified; retry from the latest state.",
                rows.iter()
                    .map(|row| {
                        Self::repo_evidence(
                            row,
                            "approved_rebase_publish",
                            "blocked",
                            Some("captured workspace or tracking state changed".to_string()),
                            None,
                        )
                    })
                    .collect(),
            ));
        }
        self.acquire_remote_tracking_locks(
            &mut application_db,
            &current_rows,
            "approved_rebase_publish",
        )
        .await?;

        if prepared_reset_lanes
            .values()
            .any(|prepared| prepared.provider_advanced)
        {
            let mut evidence = Vec::with_capacity(rows.len());
            for row in &rows {
                if let Some(prepared) = prepared_reset_lanes.get(&row.repo_id) {
                    if prepared.provider_advanced {
                        let successor_merge_run_id = crate::services::remote_tracking_merge::refresh_remote_tracking_rebase_publish_checkpoint(
                            application_db.conn(),
                            &prepared.checkpoint,
                            &prepared.live_remote_tip,
                            actor_user_id,
                        )
                        .await
                        .map_err(|error| {
                            Self::coordinated_validation_error(
                                "approved_rebase_publish",
                                "APPROVED_REBASE_STATE_CHANGED",
                                "The approval checkpoint changed while its successor was being created; retry from the latest state.",
                                vec![Self::repo_evidence(
                                    row,
                                    "approved_rebase_publish",
                                    "blocked",
                                    Some(error.to_string()),
                                    None,
                                )],
                            )
                        })?;
                        info!(
                            workspace_id = %workspace_id,
                            repo_id = %row.repo_id,
                            superseded_merge_run_id = %prepared.checkpoint.id,
                            successor_merge_run_id = %successor_merge_run_id,
                            reviewed_remote_tip = %prepared.checkpoint.reviewed_remote_tip().unwrap_or("missing"),
                            live_remote_tip = %prepared.live_remote_tip,
                            relation = "strict_descendant",
                            outcome = "approval_checkpoint_refreshed",
                            enqueued = 0_u64,
                            "Refreshed stale rebase-publication approval checkpoint"
                        );
                    }
                }
                let status = if prepared_reset_lanes
                    .get(&row.repo_id)
                    .is_some_and(|prepared| prepared.provider_advanced)
                {
                    "approval_checkpoint_refreshed"
                } else {
                    "not_enqueued"
                };
                evidence.push(Self::repo_evidence(
                    row,
                    "approved_rebase_publish",
                    status,
                    None,
                    prepared_reset_lanes
                        .get(&row.repo_id)
                        .map(|prepared| prepared.live_remote_tip.clone()),
                ));
            }
            return Ok((
                application_db,
                ApprovedRebasePublishResult {
                    enqueued: 0,
                    coordinated: Some(CoordinatedOperationResult {
                        operation: "approved_rebase_publish".to_string(),
                        status: "approval_checkpoint_refreshed".to_string(),
                        message: Some(
                            "The provider branch advanced after the previous approval checkpoint. G5 preserved that checkpoint for audit and opened a successor at the current provider tip; check the updated provider tip and approve again to publish."
                                .to_string(),
                        ),
                        repos: evidence,
                    }),
                },
            ));
        }

        let queue_requested_at = Utc::now();
        let mut publish_rows: Vec<&CoordinatedWorkspaceRepoRow> = current_rows.iter().collect();
        publish_rows.sort_by(|left, right| {
            let left_is_docs = left.docs_repo_id == Some(left.repo_id);
            let right_is_docs = right.docs_repo_id == Some(right.repo_id);
            left_is_docs
                .cmp(&right_is_docs)
                .reverse()
                .then_with(|| left.repo_id.cmp(&right.repo_id))
        });

        let mut evidence = Vec::with_capacity(rows.len());
        let mut enqueued = 0_u64;
        for (queue_index, row) in publish_rows.into_iter().enumerate() {
            let requested_at = queue_requested_at + Duration::microseconds(queue_index as i64);
            if row.sync_paused_reason.as_deref() != Some("watermark_violation") {
                // Normal lane: an unpaused, otherwise-eligible repo publishes by
                // the same fast-forward-only push `push_to_remote` enqueues. It
                // has no reset-below-baseline merge run, so there is no lease
                // proof to record — the worker's `push` path fast-forwards or
                // no-ops and never rewrites history (spec D36).
                crate::models::branch_integration_queue::enqueue_authorized_branch_integration(
                    application_db.conn(),
                    row.key(),
                    crate::models::branch_integration_queue::trigger_kind::PUSH,
                    requested_at,
                    push_authority,
                )
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to enqueue coordinated push: {e}"),
                })?;
                enqueued += 1;

                let mut repo_evidence = Self::repo_evidence(
                    row,
                    "approved_rebase_publish",
                    "queued",
                    None,
                    Some(row.head_commit.clone()),
                );
                repo_evidence.old_commit = row.remote_tip_advisory_sha.clone();
                evidence.push(repo_evidence);
                continue;
            }

            // Reset lane: record the operator-reviewed candidate, exact current
            // authority, and remote tip, then enqueue an approved publication
            // cycle. The worker creates a deterministic linear descendant and
            // the shared notifier later performs an ordinary fast-forward push.
            let prepared = prepared_reset_lanes
                .remove(&row.repo_id)
                .expect("every reset lane was prepared before application");
            crate::services::remote_tracking_merge::approve_remote_tracking_rebase_publish(
                application_db.conn(),
                prepared.checkpoint.id,
                row.key(),
                &prepared.local_tip,
                &row.head_commit,
                &prepared.live_remote_tip,
                actor_user_id,
            )
            .await
            .map_err(|err| {
                Self::coordinated_validation_error(
                    "approved_rebase_publish",
                    "APPROVED_REBASE_PROOF_REJECTED",
                    "Approved rebase publication proof was rejected for this repo.",
                    vec![Self::repo_evidence(
                        row,
                        "approved_rebase_publish",
                        "blocked",
                        Some(err.to_string()),
                        None,
                    )],
                )
            })?;
            crate::models::branch_integration_queue::enqueue_authorized_branch_integration(
                application_db.conn(),
                row.key(),
                crate::models::branch_integration_queue::trigger_kind::APPROVED_REBASE_PUBLISH,
                requested_at,
                push_authority,
            )
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to enqueue approved rebase publication: {e}"),
            })?;
            enqueued += 1;

            let mut repo_evidence = Self::repo_evidence(
                row,
                "approved_rebase_publish",
                "queued",
                None,
                Some(prepared.local_tip),
            );
            repo_evidence.old_commit = Some(prepared.live_remote_tip);
            evidence.push(repo_evidence);
        }

        Ok((
            application_db,
            ApprovedRebasePublishResult {
                enqueued,
                coordinated: Some(CoordinatedOperationResult {
                    operation: "approved_rebase_publish".to_string(),
                    status: if enqueued == current_rows.len() as u64 {
                        "queued".to_string()
                    } else {
                        "queued_partial".to_string()
                    },
                    message: Some(format!(
                        "Queued approved rebase publication for {enqueued} repo(s)."
                    )),
                    repos: evidence,
                }),
            },
        ))
    }

    async fn load_branch_repo_for_remote_promotion(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
    ) -> Result<CoordinatedWorkspaceRepoRow, G5Error> {
        self.load_expected_coordinated_repo_rows(db, workspace_id, false)
            .await?
            .into_iter()
            .find(|row| row.repo_id == repo_id)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace repository {workspace_id}/{repo_id}"),
            })
    }

    /// Return true when `actor_user_id` is an org admin of `org_id`,
    /// using the same `org_memberships` role check as the other in-service
    /// org-admin gates (see the `delete_workspace` creator-or-admin preflight).
    /// Reads the system pool because `org_memberships` is a control-plane
    /// identity table outside tenant RLS.
    async fn actor_is_org_admin(&self, org_id: Uuid, actor_user_id: Uuid) -> Result<bool, G5Error> {
        let actor_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM org_memberships WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL",
        )
        .bind(org_id)
        .bind(actor_user_id)
        .fetch_optional(&self.system_pool)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load org role for Adopt-Remote authorization: {e}"),
        })?;
        Ok(matches!(actor_role.as_deref(), Some("admin")))
    }

    /// Make-Remote Adopt-Remote git-URL re-point (workspace_service D47 item 1).
    ///
    /// No-op unless a
    /// `git_url` that differs from the current identity is supplied — the
    /// [D28] promotion contract is otherwise preserved byte-for-byte. When it
    /// applies, it validates the shared identity against a short fresh
    /// transaction and mutates only the in-memory proposal. The canonical
    /// finalizer later compare-and-swaps the shared `repos` row together with
    /// tracking metadata and the complete workspace cut. The mirror origin is
    /// preparation state and may be refreshed before remote I/O; it is not
    /// accepted authority.
    async fn maybe_adopt_remote_repoint(
        &self,
        db: &ScopedDbReopener,
        row: &mut CoordinatedWorkspaceRepoRow,
        git_url: Option<&str>,
        allow_local_git_remotes: bool,
        actor_user_id: Uuid,
    ) -> Result<(), G5Error> {
        let Some(requested_url) = git_url.map(str::trim).filter(|u| !u.is_empty()) else {
            return Ok(());
        };
        // Equal URL → nothing to adopt; proceed as ordinary [D28] promotion
        // (idempotent no-op re-point).
        if requested_url == row.git_url {
            return Ok(());
        }

        // A differing git_url is an Adopt-Remote attempt on the shared identity.
        // Org-admin is required beyond the route's ordinary workspaces:write,
        // and is checked first so a non-admin cannot flip — or probe — the
        // shared identity even with push credentials.
        if !self.actor_is_org_admin(row.org_id, actor_user_id).await? {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["org_admin_make_remote".to_string()],
            });
        }

        // Authentication eligibility is not repository identity: an existing
        // remote remains bound even if its provider/URL metadata is now rejected.
        let is_local_identity = is_local_repo_identity(row.provider.as_deref(), &row.git_url);
        if !is_local_identity {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "MAKE_REMOTE_URL_CONFLICT",
                "This repository is already bound to a different remote URL and cannot be re-pointed.",
                vec![Self::repo_evidence(
                    row,
                    "remote_bind",
                    "blocked",
                    Some(format!(
                        "repo already remote at '{}'; refusing to re-point to '{requested_url}'",
                        row.git_url
                    )),
                    None,
                )],
            ));
        }

        // Tenant-controlled URLs cannot choose where a member's stored token is sent.
        if !(self
            .repository_credentials
            .origin_policy()
            .provider_for_url(requested_url)
            .is_some()
            || allow_local_git_remotes && is_local_git_remote(requested_url))
        {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "MAKE_REMOTE_URL_INVALID",
                "The supplied remote URL must use a configured SCM HTTPS origin without userinfo.",
                vec![Self::repo_evidence(
                    row,
                    "remote_bind",
                    "blocked",
                    Some("rejected remote git URL outside the supported SCM origins".to_string()),
                    None,
                )],
            ));
        }

        // Parse the provider identity from the supplied URL.
        let provider_full_name =
            provider_repository_name(self.repository_credentials.origin_policy(), requested_url)
                .ok_or_else(|| {
                    Self::coordinated_validation_error(
                        "remote_bind",
                        "MAKE_REMOTE_URL_INVALID",
                        "The supplied remote URL is not a valid owner/repo clone URL.",
                        vec![Self::repo_evidence(
                            row,
                            "remote_bind",
                            "blocked",
                            Some(format!("could not parse owner/repo from '{requested_url}'")),
                            None,
                        )],
                    )
                })?;
        let provider = self
            .repository_credentials
            .origin_policy()
            .provider_for_url(requested_url)
            .map(|provider| provider.name().to_string());

        // Uniqueness pre-check against UNIQUE(provider, provider_repo_id). The
        // slug is not rewritten by adoption, so UNIQUE(org_id, slug) cannot
        // newly collide for this stable row. Rows adopted before the
        // configured-instance parser stored an Enterprise path prefix in the
        // identity; match that legacy spelling too so a remote is claimed once.
        let mut identities = vec![provider_full_name.clone()];
        identities.extend(
            repo_full_name_from_git_url(requested_url)
                .filter(|legacy| *legacy != provider_full_name),
        );
        let mut validation_db = db.open().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to open Adopt-Remote validation transaction: {error}"),
        })?;
        let conflict_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id FROM repos
            WHERE provider = $1
              AND provider_repo_id = ANY($2)
              AND id <> $3
            LIMIT 1
            "#,
        )
        .bind(provider.as_deref())
        .bind(&identities)
        .bind(row.repo_id)
        .fetch_optional(validation_db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed Adopt-Remote uniqueness pre-check: {e}"),
        })?;
        validation_db
            .rollback()
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to close Adopt-Remote validation transaction: {error}"),
            })?;
        if conflict_id.is_some() {
            return Err(Self::coordinated_validation_error(
                "remote_bind",
                "MAKE_REMOTE_URL_CONFLICT",
                "Another repository in this deployment already claims that remote identity.",
                vec![Self::repo_evidence(
                    row,
                    "remote_bind",
                    "blocked",
                    Some(format!(
                        "remote identity '{provider_full_name}' ({}) already in use by another repo",
                        provider.as_deref().unwrap_or("unknown-provider")
                    )),
                    None,
                )],
            ));
        }

        // Mutate only the operation proposal. The finalizer owns the durable
        // repository identity CAS and maps a concurrent uniqueness winner to a
        // rejected binary operation.
        row.git_url = requested_url.to_string();
        row.provider = provider;
        row.provider_repo_id = Some(provider_full_name.clone());
        row.provider_full_name = Some(provider_full_name);

        info!(
            repo_id = %row.repo_id,
            org_id = %row.org_id,
            actor = %actor_user_id,
            "Adopt-Remote: prepared stable-repo-id remote identity proposal"
        );
        Ok(())
    }

    /// Refresh the shared mirror's `origin` remote URL after an Adopt-Remote
    /// re-point. This FAILS LOUDLY: only an absent/unmaterialized on-disk mirror
    /// is tolerated (there is nothing to refresh yet); once a mirror exists,
    /// `GitRepo::open` and `set_remote_url` errors propagate via `?` so a
    /// materialized-but-unwritable mirror aborts the whole promotion transaction
    /// rather than leaving `origin` silently stale.
    ///
    /// Note the DB `repos.git_url` — advanced in the same transaction — is the
    /// authoritative origin. `branch_integration_worker::fetch_mirror` reconciles
    /// the mirror's `origin` to that DB value whenever they drift (in EITHER
    /// direction) before every fetch, so even the window where this refresh runs
    /// pre-commit (routes/workspaces.rs) and the route later rolls back is
    /// self-healed on the next fetch: the mirror origin is repaired back to the
    /// committed (unchanged) DB `git_url`.
    async fn refresh_mirror_origin(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        repo_id: Uuid,
        new_url: &str,
    ) -> Result<(), G5Error> {
        let local_path = repos.repo_path(org_id, repo_id)?;
        let new_url = new_url.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), G5Error> {
            let mirror_exists =
                local_path.join(".git").exists() || local_path.join("HEAD").exists();
            if !mirror_exists {
                return Ok(());
            }
            let repo = GitRepo::open(&local_path)?;
            repo.set_remote_url("origin", &new_url)?;
            Ok(())
        })
        .await
        .map_err(|join_err| G5Error::Internal {
            message: format!("mirror origin refresh task panicked: {join_err}"),
        })?
    }

    async fn load_expected_coordinated_repo_rows(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        lock_rows: bool,
    ) -> Result<Vec<CoordinatedWorkspaceRepoRow>, G5Error> {
        // Publication, push, rebase publication and Make Remote all start here.
        crate::services::workspace::workspace_kind::refuse_personal(
            db.conn(),
            workspace_id,
            "publication",
        )
        .await?;
        let repos = self.repo_storage.scoped(&*db);
        let rows = crate::services::git::remote_integrations::capture_repositories(
            db.conn(),
            repos,
            workspace_id,
            lock_rows,
        )
        .await?;

        if rows.is_empty() {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM workspaces WHERE id = $1)")
                    .bind(workspace_id)
                    .fetch_one(db.conn())
                    .await
                    .map_err(|e| G5Error::Internal {
                        message: format!("Failed to check workspace existence: {e}"),
                    })?;
            if !exists {
                return Err(G5Error::NotFound {
                    resource: format!("workspace {workspace_id}"),
                });
            }
            return Err(Self::coordinated_validation_error(
                "coordinated_repo_set",
                "COORDINATED_REPO_SET_EMPTY",
                "The workspace has no materialized code/docs repo branches eligible for coordinated sync.",
                Vec::new(),
            ));
        }

        self.validate_expected_coordinated_repo_rows(&rows)?;
        Ok(rows)
    }

    fn validate_expected_coordinated_repo_rows(
        &self,
        rows: &[CoordinatedWorkspaceRepoRow],
    ) -> Result<(), G5Error> {
        let docs_repo_id = rows.first().and_then(|row| row.docs_repo_id);
        let Some(docs_repo_id) = docs_repo_id else {
            return Err(Self::coordinated_validation_error(
                "coordinated_repo_set",
                "DOCS_REPO_REQUIRED",
                "Coordinated sync requires the workspace docs repo to be configured.",
                rows.iter()
                    .map(|row| {
                        Self::repo_evidence(row, "coordinated_repo_set", "blocked", None, None)
                    })
                    .collect(),
            ));
        };
        let has_docs = rows.iter().any(|row| row.repo_id == docs_repo_id);
        let has_code = rows.iter().any(|row| row.repo_id != docs_repo_id);
        let mut seen = std::collections::HashSet::new();
        let duplicate = rows
            .iter()
            .find(|row| !seen.insert(row.repo_id))
            .map(|row| row.repo_name.clone());
        if !has_docs || !has_code || duplicate.is_some() {
            return Err(Self::coordinated_validation_error(
                "coordinated_repo_set",
                "COORDINATED_REPO_SET_INCOMPLETE",
                "Coordinated sync requires exactly one docs branch and at least one code repo branch.",
                rows.iter()
                    .map(|row| {
                        let reason = if !has_docs && row.repo_id != docs_repo_id {
                            Some("docs repo branch is missing".to_string())
                        } else if !has_code && row.repo_id == docs_repo_id {
                            Some("code repo branch is missing".to_string())
                        } else {
                            duplicate
                                .as_ref()
                                .map(|name| format!("duplicate branch for repo {name}"))
                        };
                        Self::repo_evidence(row, "coordinated_repo_set", "blocked", reason, None)
                    })
                    .collect(),
            ));
        }
        Ok(())
    }

    fn preflight_branch_repo_clean_and_materialized(
        &self,
        row: &CoordinatedWorkspaceRepoRow,
        operation: &str,
    ) -> Result<(), G5Error> {
        if row.materialization_status != "materialized"
            || branch_materialization_status(&row.head_commit) != "materialized"
        {
            return Err(Self::coordinated_validation_error(
                operation,
                "BRANCH_NOT_MATERIALIZED",
                "The branch repo must be materialized before coordinated sync.",
                vec![Self::repo_evidence(
                    row,
                    operation,
                    "blocked",
                    Some("branch repo is not materialized".to_string()),
                    None,
                )],
            ));
        }
        if row.dirty_count > 0 {
            return Err(Self::coordinated_validation_error(
                operation,
                "BRANCH_DIRTY",
                "The branch repo must be clean before coordinated sync.",
                vec![Self::repo_evidence(
                    row,
                    operation,
                    "blocked",
                    Some(format!("{} dirty draft file(s)", row.dirty_count)),
                    None,
                )],
            ));
        }
        Ok(())
    }

    async fn acquire_remote_tracking_locks(
        &self,
        db: &mut ScopedDb,
        rows: &[CoordinatedWorkspaceRepoRow],
        operation: &str,
    ) -> Result<(), G5Error> {
        for row in rows {
            if let Some(remote_branch) = row.remote_branch.as_deref() {
                let acquired: bool = sqlx::query_scalar(
                    r#"SELECT pg_try_advisory_xact_lock(hashtext($1::text), hashtext($2))"#,
                )
                .bind(row.repo_id)
                .bind(remote_branch)
                .fetch_one(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to acquire remote-tracking lock: {e}"),
                })?;
                if !acquired {
                    return Err(Self::coordinated_validation_error(
                        operation,
                        "REMOTE_TRACKING_LOCK_BUSY",
                        "Another sync cycle is already integrating one of the required repos.",
                        vec![Self::repo_evidence(
                            row,
                            operation,
                            "blocked",
                            Some("remote-tracking lock is busy".to_string()),
                            None,
                        )],
                    ));
                }
            }
        }
        Ok(())
    }

    async fn resolve_remote_tip_for_row(
        &self,
        row: &CoordinatedWorkspaceRepoRow,
        remote_branch: &str,
        actor_user_id: Option<Uuid>,
        adopted_from: Option<&str>,
    ) -> Result<(String, GitAuth), G5Error> {
        let effective_provider = self
            .repository_credentials
            .origin_policy()
            .effective_provider(row.provider.as_deref(), &row.git_url);
        let actor = actor_user_id
            .or(row.owner_user_id)
            .ok_or_else(|| G5Error::Conflict {
                reason: "workspace_owner_missing".into(),
            })?;
        let auth = match adopted_from {
            Some(expected) => {
                self.repository_credentials
                    .resolve_adoption(
                        row.org_id,
                        row.repo_id,
                        actor,
                        expected,
                        &row.git_url,
                        crate::auth::RepositoryOperation::ReadGit,
                    )
                    .await?
            }
            None => {
                self.repository_credentials
                    .resolve(
                        row.org_id,
                        row.repo_id,
                        actor,
                        &row.git_url,
                        crate::auth::RepositoryOperation::ReadGit,
                    )
                    .await?
            }
        };

        let outcome = crate::git_runtime::network(NETWORK_DEADLINE)?
            .remote_branch_tip(&row.git_url, remote_branch, &auth)
            .await;

        match outcome {
            Ok(Some(remote_tip)) => Ok((remote_tip, auth)),
            Ok(None) => Err(G5Error::NotFound {
                resource: format!(
                    "BRANCH_NOT_FOUND: remote branch '{}' does not exist on '{}'",
                    remote_branch, row.git_url
                ),
            }),
            Err(error) => {
                let message = error.to_string();
                let provider_known = effective_provider.is_some();
                // The runner types a refused credential. GitHub also answers an
                // inaccessible private repository with "not found".
                let looks_like_auth = error.kind == g5_scm::native::FailureKind::Unauthorized
                    || matches!(auth, GitAuth::None) && provider_known
                    || message.to_lowercase().contains("not found");
                if looks_like_auth {
                    Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "code": "INSUFFICIENT_SCOPE",
                            "operation": "remote_tip",
                            "repoId": row.repo_id,
                            "repoName": row.repo_name,
                            "remoteBranch": remote_branch,
                            "message": format!(
                                "ls-remote on '{}' failed (likely missing or insufficient SCM credentials): {}",
                                row.git_url, message
                            ),
                        }),
                    })
                } else {
                    Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "code": "REMOTE_UNREACHABLE",
                            "operation": "remote_tip",
                            "repoId": row.repo_id,
                            "repoName": row.repo_name,
                            "remoteBranch": remote_branch,
                            "message": format!("ls-remote on '{}' failed: {}", row.git_url, message),
                        }),
                    })
                }
            }
        }
    }

    async fn prepare_mirror_and_classify(
        &self,
        repos: &ScopedRepoStorage,
        row: &CoordinatedWorkspaceRepoRow,
        remote_branch: &str,
        remote_tip: &str,
        auth: &GitAuth,
    ) -> Result<CoordinatedTipRelation, G5Error> {
        crate::services::remote_integrations::prepare_pull(
            repos,
            crate::services::remote_integrations::PullInput {
                key: repos.key(row.org_id, row.repo_id)?,
                git_url: row.git_url.clone(),
                remote_branch: remote_branch.to_string(),
                local_head: row.head_commit.clone(),
                watermark: row.baseline_commit.clone(),
                observed_remote_tip: Some(remote_tip.to_string()),
            },
            auth.clone(),
        )
        .await
        .map(|prepared| prepared.relation)
    }

    fn repo_evidence(
        row: &CoordinatedWorkspaceRepoRow,
        operation: &str,
        status: &str,
        error: Option<String>,
        new_commit: Option<String>,
    ) -> CoordinatedRepoEvidence {
        CoordinatedRepoEvidence {
            workspace_id: Some(row.workspace_id),
            repo_id: row.repo_id,
            repo_name: row.repo_name.clone(),
            branch_name: row.branch_name.clone(),
            remote_branch: row.remote_branch.clone(),
            operation: operation.to_string(),
            status: status.to_string(),
            old_commit: Some(row.head_commit.clone()),
            new_commit,
            error,
        }
    }

    fn coordinated_validation_error(
        operation: &str,
        code: &str,
        message: &str,
        repos: Vec<CoordinatedRepoEvidence>,
    ) -> G5Error {
        G5Error::ValidationError {
            details: serde_json::json!({
                "code": code,
                "operation": operation,
                "message": message,
                "modal": {
                    "severity": "error",
                    "title": "Coordinated repo sync failed",
                    "message": message,
                    "repos": repos,
                }
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Sync & Merge
    // -----------------------------------------------------------------------

    /// Request a remote pull for every remote-tracked member of the workspace.
    ///
    /// Each member is queued for the per-repository integration worker, which
    /// fetches, classifies and imports it under the member's policy. Base uses
    /// this same path (`g5.workspace.remote.base-parity`). Unbound members are
    /// reported as skipped.
    pub async fn sync_workspace(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        trigger_kind: &'static str,
    ) -> Result<(ScopedDb, SyncResult), G5Error> {
        let has_branches: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM workspace_repos WHERE workspace_id = $1)",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to inspect workspace sync inventory: {e}"),
        })?;
        if !has_branches {
            return Ok((
                db,
                SyncResult {
                    branches_updated: 0,
                    branches_skipped: 0,
                    details: Vec::new(),
                },
            ));
        }

        let members = self
            .load_workspace_repository_rows(&mut db, &[workspace_id])
            .await?;
        let enqueued =
            crate::models::branch_integration_queue::enqueue_branch_integration_for_workspace(
                db.conn(),
                workspace_id,
                trigger_kind,
                crate::models::branch_integration_queue::WorkspaceEnqueueConflict::Precedence,
            )
            .await?;
        let mut details = Vec::with_capacity(members.len());
        for member in members {
            let held_by = if member.remote_branch.is_some() {
                crate::services::remote_integrations::import_hold(
                    db.conn(),
                    workspace_id,
                    member.repo_id,
                )
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to read the merged-PR hold: {e}"),
                })?
            } else {
                None
            };
            details.push(BranchSyncDetail {
                workspace_id,
                repo_id: member.repo_id,
                repo_name: member.repo_id.to_string(),
                branch_name: member.branch_name,
                status: if member.remote_branch.is_some() {
                    "queued"
                } else {
                    "skipped"
                }
                .into(),
                old_commit: Some(member.head_commit),
                new_commit: None,
                held_by,
            });
        }
        let skipped = details
            .iter()
            .filter(|member| member.status == "skipped")
            .count() as i32;
        info!(%workspace_id, enqueued, "Queued remote workspace pull");
        Ok((
            db,
            SyncResult {
                branches_updated: 0,
                branches_skipped: skipped,
                details,
            },
        ))
    }

    /// Merge workspace branches into their base branches.
    ///
    /// The actual git merge is delegated to the git service. This method
    /// records the results.
    pub async fn merge_workspace(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
        event_bus: &crate::services::EventBus,
    ) -> Result<(ScopedDb, MergeResult), G5Error> {
        // Destination resolution: a non-NULL merge_target_workspace_id is
        // preferred over cloned_from_id; NULL keeps the clone-parent behavior
        // (models/workspace D19).
        let destination = self.get_merge_destination(&mut db, workspace_id).await?;
        if let Some(destination_workspace_id) = destination.destination_id {
            if crate::services::git::remote_integrations::has_bindings(
                db.conn(),
                destination_workspace_id,
            )
            .await?
            {
                return Err(G5Error::Conflict {
                    reason: "This target tracks a remote branch. Merge its pull request so the provider accepts the change before G5 imports it.".into(),
                });
            }
            // A stored merge target or clone parent outlives the access that
            // justified creating the relationship. Advancing its branch is a
            // write, so visibility or org-admin access is insufficient: the
            // same user must still own the destination at execution time.
            if let Some(user_id) = caller_user_id {
                let destination_owner = sqlx::query_scalar::<_, Option<Uuid>>(
                    "SELECT owner_user_id FROM workspaces
                     WHERE org_id = $1 AND id = $2 AND deleted_at IS NULL",
                )
                .bind(destination.org_id)
                .bind(destination_workspace_id)
                .fetch_optional(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to check merge-target ownership: {e}"),
                })?
                .ok_or_else(|| G5Error::NotFound {
                    resource: format!("workspace {destination_workspace_id}"),
                })?;
                if destination_owner != Some(user_id) {
                    return Err(G5Error::Forbidden {
                        missing_scopes: vec![],
                    });
                }
            }
            // A Base re-verification run completes when its pull request into
            // Base merges (g5.reverification.run), so it never merges directly.
            if crate::services::workspace::base_reverification::is_run_into_base(
                db.conn(),
                destination.org_id,
                workspace_id,
                destination_workspace_id,
            )
            .await?
            {
                return Err(G5Error::Conflict {
                    reason: "A Base re-verification merges into Base through its pull request"
                        .into(),
                });
            }
            return self
                .merge_cloned_workspace_into_parent(
                    db,
                    workspace_id,
                    destination_workspace_id,
                    caller_user_id,
                    event_bus,
                )
                .await;
        }

        Err(G5Error::Conflict {
            reason: "Workspace has no live merge destination".into(),
        })
    }

    async fn get_merge_destination(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<MergeDestination, G5Error> {
        let row = sqlx::query_as::<_, (Option<Uuid>, Option<Uuid>, Uuid)>(
            r#"
            SELECT
                (SELECT t.id FROM workspaces t
                  WHERE t.id = w.merge_target_workspace_id
                    AND t.deleted_at IS NULL),
                w.cloned_from_id,
                w.org_id
            FROM workspaces w
            WHERE w.id = $1
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load workspace merge-destination metadata: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        let (live_override_id, cloned_from_id, org_id) = row;
        Ok(MergeDestination {
            destination_id: live_override_id.or(cloned_from_id),
            org_id,
        })
    }

    async fn fetch_clone_branch_pairs(
        &self,
        db: &mut ScopedDb,
        child_snapshot: &CommittedBindingSnapshot,
        parent_snapshot: &CommittedBindingSnapshot,
    ) -> Result<Vec<CloneBranchPairRow>, G5Error> {
        let _ = db;
        let child_ids: BTreeSet<_> = child_snapshot
            .repos
            .iter()
            .map(|repo| repo.repo_id)
            .collect();
        let parent_ids: BTreeSet<_> = parent_snapshot
            .repos
            .iter()
            .map(|repo| repo.repo_id)
            .collect();
        if child_ids != parent_ids {
            return Err(G5Error::Conflict {
                reason: "Merge workspaces must have the same repository membership".into(),
            });
        }
        Ok(child_snapshot
            .repos
            .iter()
            .map(|child| {
                let parent = parent_snapshot
                    .repos
                    .iter()
                    .find(|parent| parent.repo_id == child.repo_id)
                    .unwrap();
                CloneBranchPairRow {
                    repo_id: child.repo_id,
                    repo_name: child.repo_name.clone(),
                    child_workspace_id: child.workspace_id,
                    child_branch_name: child.branch_name.clone(),
                    child_head_commit: child.head_commit.clone(),
                    parent_workspace_id: Some(parent.workspace_id),
                    parent_branch_name: Some(parent.branch_name.clone()),
                    parent_head_commit: Some(parent.head_commit.clone()),
                }
            })
            .collect())
    }

    async fn merge_cloned_workspace_into_parent(
        &self,
        db: ScopedDb,
        workspace_id: Uuid,
        parent_workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
        _event_bus: &crate::services::EventBus,
    ) -> Result<(ScopedDb, MergeResult), G5Error> {
        let application_scope = db.reopener();
        // The child's heads are merged into the parent without a later check
        // against the child, so they must be one committed cut
        // (`g5.fork.source.cut`).
        let child_snapshot = crate::services::workspace_reader::read_committed_cut(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
            "merge",
        )
        .await?;
        let mut db = application_scope
            .open()
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to open clone-merge parent capture: {error}"),
            })?;
        let parent_snapshot = resolve_committed_snapshot(
            &mut db,
            &self.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: parent_workspace_id,
            },
        )
        .await?;
        // Merge admission for the pinned child cut
        // (g5.governance.approval.requirements): the direct merge is gated
        // by the target's Integration Engineering requirements like a pull
        // request merge, evaluated under the policy locks against exactly
        // the heads merged below (one committed cut, never re-read).
        {
            let mut cut: Vec<g5_contracts::types::MergeSourceHead> = child_snapshot
                .repos
                .iter()
                .map(|repo| g5_contracts::types::MergeSourceHead {
                    repo_id: repo.repo_id.to_string(),
                    branch_name: repo.branch_name.clone(),
                    workspace_id: repo.workspace_id.to_string(),
                    head_commit: repo.head_commit.clone(),
                })
                .collect();
            cut.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
            crate::services::workspace::approval_config_service::lock_merge_admission(
                &mut db,
                child_snapshot.org_id,
                parent_workspace_id,
            )
            .await?;
            let admission =
                crate::services::workspace::approval_config_service::evaluate_merge_admission_with_policy(
                    &mut db,
                    child_snapshot.org_id,
                    workspace_id,
                    parent_workspace_id,
                    &cut,
                    self.approval_policy.on_source_commit_vector_change,
                )
                .await?;
            if !admission.admitted {
                return Err(G5Error::Conflict {
                    reason: admission.reason.unwrap_or_else(|| {
                        "Integration Engineering approval required before merging".to_string()
                    }),
                });
            }
        }
        let branch_pairs = self
            .fetch_clone_branch_pairs(&mut db, &child_snapshot, &parent_snapshot)
            .await?;
        let repos = self.repo_storage.scoped(&db);
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close clone-merge capture transaction: {error}"),
        })?;

        let mut branches_merged = 0;
        let mut branches_failed = 0;
        let mut details = Vec::with_capacity(branch_pairs.len());
        let mut prepared_heads = BTreeMap::new();

        for pair in branch_pairs {
            let Some(parent_workspace_id) = pair.parent_workspace_id else {
                details.push(BranchMergeDetail {
                    workspace_id: pair.child_workspace_id,
                    repo_id: pair.repo_id,
                    repo_name: pair.repo_name,
                    branch_name: pair.child_branch_name,
                    status: "conflict".to_string(),
                    merge_commit: None,
                    error: Some("Missing parent branch".to_string()),
                });
                branches_failed += 1;
                continue;
            };

            let Some(parent_branch_name) = pair.parent_branch_name.as_deref() else {
                details.push(BranchMergeDetail {
                    workspace_id: pair.child_workspace_id,
                    repo_id: pair.repo_id,
                    repo_name: pair.repo_name,
                    branch_name: pair.child_branch_name,
                    status: "conflict".to_string(),
                    merge_commit: None,
                    error: Some("Missing parent branch".to_string()),
                });
                branches_failed += 1;
                continue;
            };

            match self.merge_source_into_target_branch(
                &repos,
                child_snapshot.org_id,
                &pair.repo_id,
                &pair.child_branch_name,
                &pair.child_head_commit,
                parent_branch_name,
                pair.parent_head_commit.as_deref(),
            ) {
                Ok(MergeSourceIntoTargetOutcome::Updated(new_head)) => {
                    let old_parent_head = pair.parent_head_commit.clone().unwrap_or_default();
                    if new_head != old_parent_head {
                        prepared_heads.insert(pair.repo_id, new_head.clone());
                    }

                    details.push(BranchMergeDetail {
                        workspace_id: pair.child_workspace_id,
                        repo_id: pair.repo_id,
                        repo_name: pair.repo_name,
                        branch_name: pair.child_branch_name,
                        status: "merged".to_string(),
                        merge_commit: Some(new_head),
                        error: None,
                    });
                    branches_merged += 1;
                }
                Ok(MergeSourceIntoTargetOutcome::SkippedUnmaterialized) => {
                    details.push(BranchMergeDetail {
                        workspace_id: pair.child_workspace_id,
                        repo_id: pair.repo_id,
                        repo_name: pair.repo_name,
                        branch_name: pair.child_branch_name,
                        status: "skipped: source branch not materialized".to_string(),
                        merge_commit: None,
                        error: Some("Source branch not materialized in git yet".to_string()),
                    });
                    branches_failed += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        child_workspace_id = %pair.child_workspace_id,
                        parent_workspace_id = %parent_workspace_id,
                        repo_id = %pair.repo_id,
                        %error,
                        "Clone merge failed for branch pair"
                    );
                    details.push(BranchMergeDetail {
                        workspace_id: pair.child_workspace_id,
                        repo_id: pair.repo_id,
                        repo_name: pair.repo_name,
                        branch_name: pair.child_branch_name,
                        status: "conflict".to_string(),
                        merge_commit: None,
                        error: Some(error.to_string()),
                    });
                    branches_failed += 1;
                }
            }
        }

        let authority_advance = if prepared_heads.is_empty() {
            None
        } else {
            let target = parent_snapshot.target.clone();
            let repos = exact_workspace_operation_repos(&parent_snapshot, &prepared_heads)?;
            Some(
                prepare_branch_commit(
                    &application_scope,
                    &self.repo_storage,
                    target,
                    repos,
                    "workspace_merge",
                )
                .await
                .map_err(map_branch_commit_error)?,
            )
        };

        #[cfg(feature = "test-helpers")]
        if authority_advance.is_some() {
            clone_merge_test_barrier::wait(parent_workspace_id).await;
        }

        // Re-authorize at the application boundary, after the filesystem Git
        // work is complete. Holding this row through authority landing keeps a
        // transfer from admitting the old owner and then letting that owner
        // advance the successor's branch, without retaining a database row
        // lock across the potentially slow merge preparation above.
        let mut application_db =
            application_scope
                .open()
                .await
                .map_err(|error| G5Error::Internal {
                    message: format!("Failed to open clone-merge application transaction: {error}"),
                })?;
        // A target can become remote-bound while Git preparation runs. Hold
        // the same fence as binding publication through the eventual landing,
        // and recheck policy before admitting any locally prepared target cut.
        // The fence comes before the owner check's workspace row lock: every
        // landing takes the fence and then that row, so the reverse order
        // would deadlock against a concurrent landing on the parent.
        crate::services::git::branch_commit::acquire_workspace_commit_fence(
            application_db.conn(),
            parent_workspace_id,
        )
        .await?;
        if caller_user_id.is_some() {
            crate::services::identity::workspace_mutation_policy::ensure_workspace_owner(
                &mut application_db,
                parent_workspace_id,
            )
            .await?;
        }
        sqlx::query("SELECT g5_require_workspace_not_deleting($1)")
            .bind(parent_workspace_id)
            .execute(application_db.conn())
            .await?;
        if crate::services::git::remote_integrations::has_bindings(
            application_db.conn(),
            parent_workspace_id,
        )
        .await?
        {
            return Err(G5Error::Conflict {
                reason: "This target now tracks a remote branch. Merge its pull request so the provider accepts the change before G5 imports it.".into(),
            });
        }

        Ok((
            application_db,
            MergeResult {
                branches_merged,
                branches_failed,
                details,
                authority_advance,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_source_into_target_branch(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        repo_id: &Uuid,
        source_branch: &str,
        source_head_commit: &str,
        target_branch: &str,
        target_head_commit: Option<&str>,
    ) -> Result<MergeSourceIntoTargetOutcome, G5Error> {
        let repo_path = repos.repo_path(org_id, *repo_id)?;

        let git_repo = g5_scm::GitRepo::open(&repo_path)?;

        let Some(branch_sha) = normalize_head_commit(source_head_commit) else {
            tracing::debug!(
                branch = %source_branch,
                "Branch not yet materialised in git, skipping merge"
            );
            return Ok(MergeSourceIntoTargetOutcome::SkippedUnmaterialized);
        };
        if !git_repo.has_commit(&branch_sha)? {
            return Err(G5Error::Internal {
                message: format!(
                    "Database-selected source commit {branch_sha} for branch {source_branch} is missing from repo {repo_id}"
                ),
            });
        }

        let Some(base_sha) = target_head_commit.and_then(normalize_head_commit) else {
            tracing::info!(
                source_branch = %source_branch,
                target_branch = %target_branch,
                sha = %branch_sha,
                "Prepared source commit for an unmaterialized target"
            );
            return Ok(MergeSourceIntoTargetOutcome::Updated(branch_sha));
        };
        if !git_repo.has_commit(&base_sha)? {
            return Err(G5Error::Internal {
                message: format!(
                    "Database-selected target commit {base_sha} for branch {target_branch} is missing from repo {repo_id}"
                ),
            });
        }

        if branch_sha == base_sha {
            return Ok(MergeSourceIntoTargetOutcome::Updated(base_sha));
        }

        if git_repo.is_ancestor_of(&branch_sha, &base_sha)? {
            return Ok(MergeSourceIntoTargetOutcome::Updated(base_sha));
        }
        if git_repo.is_ancestor_of(&base_sha, &branch_sha)? {
            tracing::info!(
                source_branch = %source_branch,
                target_branch = %target_branch,
                sha = %branch_sha,
                "Prepared fast-forward workspace merge result"
            );
            return Ok(MergeSourceIntoTargetOutcome::Updated(branch_sha));
        }

        let new_head =
            git_repo.rebase_onto(&branch_sha, &base_sha, source_branch, target_branch)?;
        tracing::info!(
            source_branch = %source_branch,
            target_branch = %target_branch,
            rebase_sha = %new_head,
            "Prepared workspace rebase result without advancing a branch ref"
        );
        Ok(MergeSourceIntoTargetOutcome::Updated(new_head))
    }

    /// Validate a git ref name via `g5_scm::is_valid_ref_name`
    /// (a thin wrapper around `git2::Reference::is_valid_name`).
    ///
    /// Implements `workspace_repo_isolation.security_view.md` D14:
    /// rejects NFC/NFD normalization differences, embedded `..`,
    /// leading `-`, trailing `.lock`, control characters. Returns a
    /// structured `ValidationError` with field-tagged context per arch
    /// view D47.
    ///
    /// Phase 1.B of the remote-repo-tracking work.
    fn validate_ref_name(name: &str, field_label: &'static str) -> Result<(), G5Error> {
        if !g5_scm::is_valid_ref_name(name) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "BRANCH_NAME_INVALID",
                    "field": field_label,
                    "reason": format!(
                        "'{}' fails git ref-name validation",
                        name
                    ),
                }),
            });
        }
        Ok(())
    }

    /// Look up other workspaces in the same org that already track the
    /// same `(repo_id, remote_branch)` pair. Returns the co-tracking
    /// advisory surface for the workspace-create response.
    ///
    /// Returns "main" only if the repo doesn't exist (shouldn't happen).
    async fn load_repo_names<I>(
        &self,
        db: &mut ScopedDb,
        repo_ids: I,
    ) -> Result<HashMap<Uuid, String>, G5Error>
    where
        I: IntoIterator<Item = Uuid>,
    {
        #[derive(sqlx::FromRow)]
        struct RepoNameRow {
            id: Uuid,
            name: String,
        }

        let repo_ids: Vec<Uuid> = repo_ids.into_iter().collect();
        if repo_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows =
            sqlx::query_as::<_, RepoNameRow>("SELECT id, name FROM repos WHERE id = ANY($1)")
                .bind(repo_ids.as_slice())
                .fetch_all(db.conn())
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to fetch repo names: {e}"),
                })?;

        Ok(rows.into_iter().map(|row| (row.id, row.name)).collect())
    }

    /// Load chip-display metadata (`name`, `provider_full_name`, `git_url`) for
    /// a set of repos, keyed by id. Used by `fetch_branch_summaries` so the
    /// workspace-detail response can carry the remote identity for
    /// remote-tracking repo chips. One query; only the detail path calls it.
    async fn load_repo_chip_meta<I>(
        &self,
        db: &mut ScopedDb,
        repo_ids: I,
    ) -> Result<HashMap<Uuid, RepoChipMeta>, G5Error>
    where
        I: IntoIterator<Item = Uuid>,
    {
        #[derive(sqlx::FromRow)]
        struct RepoChipMetaRow {
            id: Uuid,
            name: String,
            provider_full_name: Option<String>,
            git_url: String,
        }

        let repo_ids: Vec<Uuid> = repo_ids.into_iter().collect();
        if repo_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = sqlx::query_as::<_, RepoChipMetaRow>(
            "SELECT id, name, provider_full_name, git_url FROM repos WHERE id = ANY($1)",
        )
        .bind(repo_ids.as_slice())
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch repo chip metadata: {e}"),
        })?;

        let policy = self.repository_credentials.origin_policy();
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.id,
                    RepoChipMeta::new(row.name, row.provider_full_name, &row.git_url, policy),
                )
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Fetch branch summaries for a workspace (used by get_workspace and
    /// update_workspace).
    /// Option 1 (G5 #1198): toggle per-child passive auto-pull. Ontology
    /// `software_architecture/g5-api/services/workspace_service` D42.
    ///
    /// Validates that the target is a live, non-Base `remote_tracking` child with
    /// a bound `remote_branch`, then performs an idempotent guarded UPDATE. On
    /// enable it wakes exactly one deduplicated passive `background_inbound` item;
    /// on disable it cancels pending passive rows without touching explicit
    /// (push/commit/manual) rows. Returns the authoritative new value. Callers
    /// hold an org-scoped `ScopedDb` and have already verified the branch/repo
    /// belong to the workspace.
    pub async fn set_branch_repo_auto_pull(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        enabled: bool,
        actor_user_id: Uuid,
    ) -> Result<bool, G5Error> {
        let row = self
            .load_branch_repo_for_remote_promotion(db, workspace_id, repo_id)
            .await?;
        crate::services::remote_integrations::set_auto_pull(
            db.conn(),
            row.key(),
            enabled,
            actor_user_id,
        )
        .await
    }

    /// Whether the workspace is its organization's Base, which adopts provider
    /// history without a VM (`g5.workspace.remote.base-provider-rewrite`).
    pub async fn workspace_is_base(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<bool, G5Error> {
        Ok(sqlx::query_scalar(
            "SELECT is_base FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_one(db.conn())
        .await?)
    }

    /// Show what adopting a repository's provider history would do, on Base
    /// or for an ordinary workspace's open divergence. Read-only: fetches
    /// provider objects into the shared mirror but moves no ref.
    pub async fn preview_provider_history_acceptance(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<ProviderHistoryPreview, G5Error> {
        let capture = self
            .capture_provider_history_acceptance(&mut db, workspace_id, repo_id, actor_user_id)
            .await?;
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close provider-history preview transaction: {error}"),
        })?;
        Ok(capture.preview)
    }

    /// Prepare Base's adoption of its provider history for one repository
    /// (`g5.workspace.remote.base-provider-rewrite`). The owner's reviewed
    /// provider tip and Base head must still be current. The caller lands the
    /// returned publication.
    pub async fn accept_provider_history(
        &self,
        mut db: ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        expected_provider_tip: &str,
        expected_head: &str,
        actor_user_id: Uuid,
    ) -> Result<
        (
            ScopedDb,
            crate::services::remote_integrations::PreparedRemotePublication,
        ),
        G5Error,
    > {
        let capture = self
            .capture_provider_history_acceptance(&mut db, workspace_id, repo_id, actor_user_id)
            .await?;
        if capture.preview.provider_tip != expected_provider_tip
            || capture.preview.head != expected_head
        {
            return Err(Self::coordinated_validation_error(
                "accept_provider_history",
                "PROVIDER_HISTORY_CHANGED",
                "The provider branch or Base changed since you reviewed it. Review the provider history again.",
                vec![Self::repo_evidence(
                    &capture.row,
                    "accept_provider_history",
                    "blocked",
                    Some("reviewed provider tip or Base head is stale".to_string()),
                    Some(capture.preview.provider_tip.clone()),
                )],
            ));
        }
        let application_scope = db.reopener();
        db.rollback().await.map_err(|error| G5Error::Internal {
            message: format!("Failed to close provider-history capture transaction: {error}"),
        })?;
        let captured_heads = capture
            .authority_inventory
            .repos
            .iter()
            .map(|repo| (repo.repo_id, repo.head_commit.clone()))
            .collect();
        let publication =
            crate::services::remote_integrations::prepare_provider_history_acceptance(
                &application_scope,
                &self.repo_storage,
                capture.authority_inventory.target.clone(),
                &captured_heads,
                &capture.row,
                &capture.preview.provider_tip,
                actor_user_id,
            )
            .await
            .map_err(|error| G5Error::Conflict {
                reason: format!("Could not prepare provider-history acceptance: {error}"),
            })?;
        let application_db = application_scope
            .open()
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to open provider-history acceptance transaction: {error}"),
            })?;
        Ok((application_db, publication))
    }

    /// Shared preflight: remote-tracked, at its authoritative cut, and whose
    /// history differs from the provider's (paused for a rewrite or
    /// divergence, or not a fast-forward). Outside Base the repository must
    /// be paused diverged with a divergence run no agent is resolving
    /// (`g5.workspace.remote.adopt-provider-history`).
    async fn capture_provider_history_acceptance(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<ProviderHistoryCapture, G5Error> {
        let row = self
            .load_branch_repo_for_remote_promotion(db, workspace_id, repo_id)
            .await?;
        if !row.is_base {
            let adoptable: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM workspace_merge_runs
                      WHERE workspace_id = $1 AND repo_id = $2
                        AND source_kind = 'remote_tracking'
                        AND phase = 'workspace_rebase_preflight'
                        AND status IN ('pending', 'waiting_for_human')
                        AND resolution_task_id IS NULL)",
            )
            .bind(workspace_id)
            .bind(repo_id)
            .fetch_one(db.conn())
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to load the divergence run: {error}"),
            })?;
            if row.sync_paused_reason.as_deref() != Some("diverged") || !adoptable {
                return Err(Self::coordinated_validation_error(
                    "accept_provider_history",
                    "PROVIDER_HISTORY_REQUIRES_DIVERGENCE",
                    "Adopting the provider history settles an open divergence that no agent is resolving.",
                    Vec::new(),
                ));
            }
        }
        let Some(remote_branch) = row.remote_branch.clone() else {
            return Err(Self::coordinated_validation_error(
                "accept_provider_history",
                "PROVIDER_HISTORY_REQUIRES_REMOTE_TRACKING",
                "Make this repository remote before accepting provider history.",
                Vec::new(),
            ));
        };
        self.preflight_branch_repo_clean_and_materialized(&row, "accept_provider_history")?;
        let authority_inventory = resolve_committed_snapshot(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace { workspace_id },
        )
        .await?;
        if !authority_inventory.repos.iter().any(|member| {
            member.repo_id == row.repo_id
                && member.workspace_id == row.workspace_id
                && member.head_commit == row.head_commit
        }) {
            return Err(Self::coordinated_validation_error(
                "accept_provider_history",
                "PROVIDER_HISTORY_HEAD_RACE",
                "Base no longer matches its complete authoritative cut. Try again.",
                Vec::new(),
            ));
        }
        let (provider_tip, auth) = self
            .resolve_remote_tip_for_row(&row, &remote_branch, Some(actor_user_id), None)
            .await?;
        let relation = self
            .prepare_mirror_and_classify(
                &self.repo_storage.scoped(&*db),
                &row,
                &remote_branch,
                &provider_tip,
                &auth,
            )
            .await?;
        let paused_for_history = matches!(
            row.sync_paused_reason.as_deref(),
            Some("diverged") | Some("watermark_violation")
        );
        let differs = matches!(
            relation,
            CoordinatedTipRelation::WorkspaceAhead
                | CoordinatedTipRelation::WorkspaceRewritten
                | CoordinatedTipRelation::Diverged
                | CoordinatedTipRelation::WatermarkViolation
        );
        if !paused_for_history && !differs {
            return Err(Self::coordinated_validation_error(
                "accept_provider_history",
                "PROVIDER_HISTORY_ALREADY_FOLLOWED",
                "Base already follows the provider history. Use Fetch from Remote to import new commits.",
                Vec::new(),
            ));
        }
        let path = self.repo_storage.repo_path(&*db, row.org_id, row.repo_id)?;
        let (head, tip) = (row.head_commit.clone(), provider_tip.clone());
        let (discarded_commits, initial_only) = tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&path)?;
            let (ahead, _) = repo.graph_ahead_behind(&head, &tip)?;
            let initial_only = ahead == 1 && repo.commit_is_parentless(&head)?;
            Ok::<_, G5Error>((ahead, initial_only))
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Provider-history comparison task failed: {error}"),
        })??;
        let preview = ProviderHistoryPreview {
            workspace_id,
            repo_id,
            remote_branch,
            head: row.head_commit.clone(),
            provider_tip,
            discarded_commits,
            initial_only,
            sync_paused_reason: row.sync_paused_reason.clone(),
        };
        Ok(ProviderHistoryCapture {
            row,
            authority_inventory,
            preview,
        })
    }

    /// Option 2 (G5 #1198): explicit Publish of a local-only/pinned branch child.
    /// Ontology `software_architecture/g5-api/services/workspace_service` D43.
    ///
    /// Creates a new remote ref with a create-only refspec (never overwriting an
    /// unrelated branch), confirms it, and only then transitions the child to
    /// `remote_tracking`. A failed/partial publication leaves the child local-only
    /// and retryable. Callers hold an org-scoped `ScopedDb` and have verified the
    /// branch/repo belong to the workspace.
    pub async fn publish_branch_repo(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<PublishBranchRepoResult, G5Error> {
        let row = self
            .load_branch_repo_for_remote_promotion(db, workspace_id, repo_id)
            .await?;
        if row.is_base {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "PUBLISH_NOT_ON_BASE",
                    "message": "Base tracks each repository's existing default branch; use Make Remote instead of Publish."
                }),
            });
        }
        if row.tracking_mode == "remote_tracking" {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "PUBLISH_ALREADY_REMOTE_TRACKING",
                    "message": "This repository branch is already published/remote-tracking."
                }),
            });
        }
        let git_url = row.git_url.clone();
        if git_url.trim().is_empty() {
            return Err(G5Error::Conflict {
                reason: "Repository has no remote URL".into(),
            });
        }
        // New remote ref under the reserved `g5/` prefix (same convention as the
        // isolate fork create-ref, clone_service D26).
        let remote_branch = format!("g5/{}", row.branch_name);
        let head = row.head_commit.clone();

        // Resolve the repository credential for this explicit publication.
        let auth = self
            .repository_credentials
            .resolve(
                row.org_id,
                repo_id,
                actor_user_id,
                &row.git_url,
                crate::auth::RepositoryOperation::WriteGit,
            )
            .await?;

        // Conflict/attach guard: an existing target ref at the SAME head is an
        // idempotent attach (safe to re-run / retry); an existing ref at a
        // DIFFERENT head is an explicit conflict, never permission to overwrite
        // (spec §6.2 pt 4). A free ref name is created.
        let needs_create = {
            let probe = remote_branch_tip(&git_url, &remote_branch, &auth, NETWORK_DEADLINE).await;
            match probe {
                Ok(existing) if existing == head => false, // attach — already at our head
                Ok(_existing) => {
                    return Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "code": "PUBLISH_TARGET_REF_EXISTS",
                            "operation": "publish",
                            "repoId": repo_id,
                            "repoName": row.repo_name,
                            "remoteBranch": remote_branch,
                            "message": format!("remote branch '{remote_branch}' already exists on '{git_url}' at a different commit; publication would not overwrite it"),
                        }),
                    });
                }
                Err(G5Error::NotFound { .. }) => true, // free to create
                Err(other) => return Err(other),
            }
        };

        // Create-only push of the local head to the new remote ref, then confirm.
        // Skipped when attaching to a ref that already sits at our head.
        if needs_create {
            let local_path = self.repo_storage.repo_path(&*db, row.org_id, repo_id)?;
            // The local head is the workspace's committed tip, which lives in
            // the shared per-repo mirror. If it is absent the branch cannot be
            // published and stays local-only.
            publish_create_ref(&local_path, &git_url, &head, &remote_branch, &auth).await?;
        }

        // Confirm the remote ref landed at the exact head before any DB transition.
        let confirmed_tip =
            remote_branch_tip(&git_url, &remote_branch, &auth, NETWORK_DEADLINE).await?;
        if confirmed_tip != head {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "PUBLISH_UNCONFIRMED",
                    "operation": "publish",
                    "repoId": repo_id,
                    "repoName": row.repo_name,
                    "remoteBranch": remote_branch,
                    "message": format!("remote branch '{remote_branch}' did not confirm at the published head; leaving the branch local-only"),
                }),
            });
        }

        // Only after confirmation: transition to remote_tracking. This does not
        // touch `auto_pull_enabled` (spec §10 D-open-3: newly published stays
        // auto_pull_enabled = false unless a separate user action enables it).
        // The remote publication is independently confirmed. Policy can be committed
        // with the caller's PR transaction; the integration worker establishes the
        // Git synchronization watermark on its next fenced cycle.
        crate::services::remote_integrations::bind_published(db.conn(), row.key(), &remote_branch)
            .await?;

        tracing::info!(
            target: "g5_audit",
            workspace_id = %workspace_id,
            repo_id = %repo_id,
            actor_user_id = %actor_user_id,
            remote_branch = %remote_branch,
            head_commit = %head,
            "branch repo published to a new remote ref"
        );

        Ok(PublishBranchRepoResult {
            workspace_id,
            repo_id,
            remote_branch,
            head_commit: head,
        })
    }

    /// Synchronous publication of a `remote_tracking` PR source's committed
    /// local head to its existing remote branch, before the PR opens
    /// (workspace_service D44, `@ref core_workflow/external_git_integration#D20 `).
    /// Holds the per-`(repo, remote_branch)` advisory lock (same key as
    /// `acquire_remote_tracking_locks`) so it never races the async
    /// branch-integration worker, confirms the remote tip landed at the head, and
    /// only then advances the child's baseline/advisory watermark. The observed
    /// remote tip selects a plain fast-forward or a force-with-lease publication,
    /// matching the explicit push-to-remote contract; the lease prevents a
    /// concurrent third-party advance from being overwritten.
    /// Apply forwarding's history policy to a tracked PR source
    /// (`g5.governance.remote.workspace-rewrite`): publish a fast-forward or
    /// an accepted workspace rewrite over the unchanged synchronization point,
    /// never over a provider rewrite or provider commits the head lacks.
    /// Without a watermark nothing proves a rewrite, so only a fast-forward
    /// publishes.
    async fn require_publishable_tracked_source(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        input: &TrackedPrSourcePushInput,
        observed_tip: &str,
        watermark: Option<String>,
        auth: &g5_scm::GitAuth,
    ) -> Result<(), G5Error> {
        use crate::services::branch_integration_service::TipRelation;
        let prepared = crate::services::remote_integrations::prepare_pull(
            repos,
            crate::services::remote_integrations::PullInput {
                key: repos.key(org_id, input.repo_id)?,
                git_url: input.git_url.clone(),
                remote_branch: input.remote_branch.clone(),
                local_head: input.head.clone(),
                watermark,
                observed_remote_tip: Some(observed_tip.to_owned()),
            },
            auth.clone(),
        )
        .await?;
        let (code, message) = match prepared.relation {
            TipRelation::Equal | TipRelation::WorkspaceAhead | TipRelation::WorkspaceRewritten => {
                return Ok(())
            }
            TipRelation::MirrorAhead | TipRelation::Diverged => (
                "PR_SOURCE_PROVIDER_ADVANCED",
                "the remote branch has commits this workspace does not contain; pull or reconcile them, then retry",
            ),
            TipRelation::WatermarkViolation => (
                "PR_SOURCE_PROVIDER_REWRITTEN",
                "the remote branch history was rewritten outside G5; review it in remote tracking, then retry",
            ),
        };
        Err(G5Error::ValidationError {
            details: serde_json::json!({
                "code": code,
                "operation": "push",
                "repoId": input.repo_id,
                "remoteBranch": input.remote_branch,
                "remoteTip": observed_tip,
                "head": input.head,
                "message": message,
            }),
        })
    }

    async fn push_tracked_pr_source(
        &self,
        db: &mut ScopedDb,
        input: TrackedPrSourcePushInput,
    ) -> Result<(), G5Error> {
        let acquired: bool = sqlx::query_scalar(
            r#"SELECT pg_try_advisory_xact_lock(hashtext($1::text), hashtext($2))"#,
        )
        .bind(input.repo_id)
        .bind(&input.remote_branch)
        .fetch_one(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to acquire remote-tracking lock for PR-source push: {e}"),
        })?;
        if !acquired {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "REMOTE_TRACKING_LOCK_BUSY",
                    "operation": "push",
                    "repoId": input.repo_id,
                    "remoteBranch": input.remote_branch,
                    "message": "another sync cycle is integrating this repo; retry Create PR",
                }),
            });
        }

        let org_id = db
            .primary_org_id()
            .map_err(|_| G5Error::Forbidden {
                missing_scopes: vec![],
            })?
            .into_uuid();
        let auth = self
            .repository_credentials
            .resolve(
                org_id,
                input.repo_id,
                input.actor_user_id,
                &input.git_url,
                crate::auth::RepositoryOperation::WriteGit,
            )
            .await?;

        let local_path = self.repo_storage.repo_path(&*db, org_id, input.repo_id)?;
        {
            ensure_publication_mirror(&local_path, &input.git_url, &auth).await?;
            // The local head is checked before contacting the remote, so a
            // drifted source is reported as such even when the remote fails.
            let watermark = {
                let lp = local_path.clone();
                let git_url_b = input.git_url.clone();
                let local_branch_b = input.local_branch.clone();
                let head_b = input.head.clone();
                tokio::task::spawn_blocking(move || -> Result<Option<String>, G5Error> {
                    let repo = GitRepo::open(&lp)?;
                    repo.ensure_remote_url("origin", &git_url_b)?;
                    let local_head = repo.resolve_branch_head(&local_branch_b)?;
                    if local_head != head_b {
                        return Err(G5Error::ValidationError {
                            details: serde_json::json!({
                                "code": "PR_SOURCE_HEAD_DRIFT",
                                "operation": "push",
                                "repoId": input.repo_id,
                                "localBranch": local_branch_b.clone(),
                                "expectedHead": head_b.clone(),
                                "actualHead": local_head,
                                "message": "the local source branch changed after PR preflight; retry Create PR",
                            }),
                        });
                    }
                    repo.read_sync_watermark(&local_branch_b)
                })
                .await
                .map_err(|join_err| G5Error::Internal {
                    message: format!("PR-source head check task panicked: {join_err}"),
                })??
            };
            let observed_remote_tip = match remote_branch_tip(
                &input.git_url,
                &input.remote_branch,
                &auth,
                NETWORK_DEADLINE,
            )
            .await
            {
                Ok(tip) => Some(tip),
                Err(G5Error::NotFound { .. }) => None,
                Err(err) => return Err(err),
            };
            if let Some(tip) = observed_remote_tip
                .as_deref()
                .filter(|tip| *tip != input.head)
            {
                let watermark = watermark.filter(|sha| sha.bytes().any(|byte| byte != b'0'));
                self.require_publishable_tracked_source(
                    &self.repo_storage.scoped(&*db),
                    org_id,
                    &input,
                    tip,
                    watermark,
                    &auth,
                )
                .await?;
            }
            if observed_remote_tip.as_deref() != Some(input.head.as_str()) {
                let git = crate::git_runtime::network(NETWORK_DEADLINE)?;
                let lp = local_path.clone();
                let remote_branch_b = input.remote_branch.clone();
                let auth_b = auth.clone();
                let head_b = input.head.clone();
                tokio::task::spawn_blocking(move || -> Result<(), G5Error> {
                    GitRepo::open(&lp)?.publish_commit(
                        &git,
                        "origin",
                        &head_b,
                        &remote_branch_b,
                        observed_remote_tip.as_deref(),
                        &auth_b,
                    )
                })
                .await
                .map_err(|join_err| G5Error::Internal {
                    message: format!("PR-source push task panicked: {join_err}"),
                })??;
            }
        }

        let confirmed_tip = remote_branch_tip(
            &input.git_url,
            &input.remote_branch,
            &auth,
            NETWORK_DEADLINE,
        )
        .await?;
        if confirmed_tip != input.head {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "PR_SOURCE_PUSH_UNCONFIRMED",
                    "operation": "push",
                    "repoId": input.repo_id,
                    "remoteBranch": input.remote_branch,
                    "message": format!("remote branch '{}' did not confirm at the pushed head", input.remote_branch),
                }),
            });
        }

        // Reconciliation records synchronization in Git, never in a SQL SHA cache.
        let org_id: Uuid = sqlx::query_scalar(
            "SELECT org_id FROM workspace_repos WHERE workspace_id = $1 AND repo_id = $2",
        )
        .bind(input.source_workspace_id)
        .bind(input.repo_id)
        .fetch_one(db.conn())
        .await?;
        crate::models::branch_integration_queue::enqueue_branch_integration(
            db.conn(),
            WorkspaceRepoKey {
                org_id,
                workspace_id: input.source_workspace_id,
                repo_id: input.repo_id,
            },
            crate::models::branch_integration_queue::trigger_kind::MANUAL,
        )
        .await?;

        tracing::info!(
            target: "g5_audit",
            repo_id = %input.repo_id,
            actor_user_id = %input.actor_user_id,
            remote_branch = %input.remote_branch,
            head_commit = %input.head,
            "remote_tracking PR source pushed to remote before opening the PR"
        );
        Ok(())
    }

    /// Option 2 (G5 #1198) Create-PR source preparation (§10 D-open-2;
    /// `@ref core_workflow/external_git_integration#D20 `, workspace_service D44):
    /// before the local PR row is committed and the provider PR API is called,
    /// prepare every PR source child so the opened PR reflects the latest work.
    /// For each source repo whose target child is remote-tracking (there is a base
    /// branch to PR against) and whose repo has a remote: a **local-only** source
    /// is PUBLISHED (create-and-confirm a new remote ref); an already
    /// **remote_tracking** source is PUSHED using the observed-tip FF/lease policy
    /// to the existing remote branch. A source with no remote target is SKIPPED.
    /// All-or-none over the eligible repos: any eligible push/publish failure
    /// returns `Err`, aborting the whole Create PR (no PR is created). Returns the
    /// number of repos prepared (published or pushed).
    pub async fn prepare_pr_source_publication(
        &self,
        db: &mut ScopedDb,
        pull_request_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<usize, G5Error> {
        let pr: Option<(Uuid, serde_json::Value)> = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
            r#"
                SELECT source_workspace_id, preflight
                FROM workspace_pull_requests
                WHERE id = $1
                  AND status = 'open'
                  AND org_id = ANY(authz.current_org_ids())
                "#,
        )
        .bind(pull_request_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to load pull request for publish coupling: {e}"),
        })?;

        let Some((source_workspace_id, preflight_json)) = pr else {
            return Ok(0);
        };
        let preflight: crate::services::workspace_merge_service::MergePreflightResponse =
            serde_json::from_value(preflight_json).map_err(|e| G5Error::Internal {
                message: format!("Failed to parse PR preflight for publish coupling: {e}"),
            })?;

        // Per-repo workflow gating (D44, reconciled to
        // `@ref core_workflow/merge_workflow#D31 ` /
        // `@ref software_architecture/g5-api/services/coordinated_repo_publication#D13 `,
        // issue #869) via the shared `MergePreflightRepoResult::workflow_required_gate`
        // classifier the provider lifecycle also consumes, so the two halves of
        // Create PR cannot diverge. A source-side stale/unreadable snapshot is
        // NOT safe to publish — pushing it could publish a wrong revision — so
        // that repo is skipped and its durable blocked/stale evidence stands.
        // But a `WorkflowRequired` whose sole cause is target-side drift keeps a
        // current source head, so the source stays eligible and is
        // published/pushed exactly as the provider layer opens the PR against
        // the current remote target. `NotBlocked` repos proceed normally.
        if preflight.source_workspace_id != Some(source_workspace_id) {
            return Err(G5Error::Conflict {
                reason: "PR source changed since its preflight".into(),
            });
        }
        let mut prepared = 0usize;
        for repo in &preflight.repo_results {
            if matches!(
                repo.workflow_required_gate(),
                crate::services::workspace_merge_service::WorkflowRequiredGate::SourceBlocked
            ) {
                continue;
            }

            let eligible = crate::services::git::remote_integrations::eligible_pr_source(
                db.conn(),
                repo.source_workspace_id,
                repo.target_workspace_id,
                repo.repo_id,
            )
            .await?;

            let Some(EligiblePrSourceRow {
                source_mode,
                target_mode,
                git_url,
                source_local_branch,
                source_remote_branch,
                source_workspace_id,
            }) = eligible
            else {
                continue;
            };

            // A source with no remote target to PR against needs neither publish
            // nor push — skip it. A skipped non-eligible repo never aborts Create PR.
            let Some(git_url) = git_url else {
                continue;
            };
            if target_mode != "remote_tracking" {
                continue;
            }

            if source_mode != "remote_tracking" {
                // Local-only source with a remote target → publish (create ref).
                // Abort-on-failure (D10): a publish failure aborts the whole PR.
                self.publish_branch_repo(db, source_workspace_id, repo.repo_id, actor_user_id)
                    .await?;
                prepared += 1;
            } else if let Some(remote_branch) = source_remote_branch {
                // Already-remote_tracking source → fast-forward push the head the
                // provider PR will reference (`repo.source_head`, the merge
                // preflight's git-ref effective tip) to the existing remote branch,
                // so the GitHub source branch matches the provider preflight. Using
                // the DB `head_commit` column here published a stale baseline for a
                // fresh un-pushed commit, causing "action required — GitHub source
                // branch is at X, expected Y" (workspace_service D44). Abort-on-failure.
                self.push_tracked_pr_source(
                    db,
                    TrackedPrSourcePushInput {
                        source_workspace_id,
                        repo_id: repo.repo_id,
                        git_url,
                        local_branch: source_local_branch,
                        remote_branch,
                        head: repo.source_head.clone(),
                        actor_user_id,
                    },
                )
                .await?;
                prepared += 1;
            }
        }
        Ok(prepared)
    }

    async fn load_workspace_repository_rows(
        &self,
        db: &mut ScopedDb,
        workspace_ids: &[Uuid],
    ) -> Result<Vec<WorkspaceRepositoryRow>, G5Error> {
        let repos = self.repo_storage.scoped(&*db);
        crate::services::git::remote_integrations::repository_summaries(
            db.conn(),
            &repos,
            workspace_ids,
        )
        .await
    }

    async fn fetch_branch_summaries(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
        branch_name: Option<&str>,
    ) -> Result<Vec<BranchSummary>, G5Error> {
        let mut rows = self
            .load_workspace_repository_rows(db, &[workspace_id])
            .await?;
        if let Some(branch_name) = branch_name {
            rows.retain(|row| row.branch_name == branch_name);
        }
        let repo_meta = self
            .load_repo_chip_meta(db, rows.iter().map(|row| row.repo_id))
            .await?;

        let mut summaries: Vec<BranchSummary> = rows
            .into_iter()
            .map(|b| {
                let meta = repo_meta.get(&b.repo_id);
                // Remote identity is surfaced only for remote-tracking branches;
                // legacy/pinned branches keep the G5-internal identity (None).
                let is_remote_tracking = b.tracking_mode == "remote_tracking";
                let (remote_branch, remote_repo_name) =
                    remote_identity_for(is_remote_tracking, b.remote_branch, meta);
                BranchSummary {
                    workspace_id: b.workspace_id,
                    repo_id: b.repo_id,
                    repo_name: meta
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    branch_name: b.branch_name,

                    tracking_mode: b.tracking_mode,
                    head_commit: b.head_commit,
                    materialization_status: b.materialization_status,
                    divergence: None,
                    behind_base: None,
                    has_draft: b.has_draft,
                    dirty_file_count: b.dirty_file_count,
                    remote_branch,
                    local_branch_name: None,
                    remote_repo_name,
                    co_tracking_workspaces: None,
                    co_tracking_workspace_count: b.co_tracking_workspace_count,
                    // Authoritative per-child auto-pull; the SELECT already
                    // gates this to remote_tracking children (else NULL).
                    auto_pull_enabled: b.auto_pull_enabled,
                    remote_eligible: b.remote_eligible,
                }
            })
            .collect();
        summaries.sort_by(|left, right| {
            left.repo_name
                .cmp(&right.repo_name)
                .then_with(|| left.branch_name.cmp(&right.branch_name))
                .then_with(|| left.repo_id.cmp(&right.repo_id))
        });

        // Issue #1995 (ontology D50): the detail path is the one surface that
        // computes real ahead/behind, at request time from git ancestry.
        // Everything stays per-branch fail-soft: a branch whose counts cannot
        // be computed keeps `None` and the endpoint still succeeds.
        self.populate_branch_divergence(db, workspace_id, caller_user_id, &mut summaries)
            .await?;

        Ok(summaries)
    }

    /// Issue #1995 (ontology D50): populate real `divergence`/`behind_base`
    /// on workspace-detail branch summaries from request-time git ancestry.
    ///
    /// The comparison workspace is a live `merge_target_workspace_id` first,
    /// else a live `cloned_from_id` — the merge destination precedence
    /// (models/workspace D19, plan Q1). Branches pair by `repo_id` (the merge
    /// preflight pairing), both sides read the committed child-preferred
    /// effective head, and every failure degrades that branch to `None`
    /// (unknown) with one warn log — the detail endpoint never fails because
    /// divergence could not be computed.
    async fn populate_branch_divergence(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
        summaries: &mut [BranchSummary],
    ) -> Result<(), G5Error> {
        let Some(comparison_workspace_id) = self
            .resolve_divergence_comparison_workspace(db, workspace_id, caller_user_id)
            .await?
        else {
            // Base or standalone workspace: no comparison target, every
            // branch keeps the honest unknown (`None`).
            return Ok(());
        };
        let base_heads = self
            .committed_branch_heads_by_repo(db, comparison_workspace_id)
            .await?;
        if summaries.len() != base_heads.len()
            || summaries
                .iter()
                .any(|summary| !base_heads.contains_key(&summary.repo_id))
        {
            return Ok(());
        }
        let org_id =
            crate::services::workspace_reader::workspace_org(db.conn(), workspace_id).await?;
        let repos = self.repo_storage.scoped(&*db);
        for summary in summaries.iter_mut() {
            let workspace_head = summary.head_commit.clone();
            let Some(base_head) = base_heads.get(&summary.repo_id) else {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    comparison_workspace_id = %comparison_workspace_id,
                    repo_id = %summary.repo_id,
                    head = %summary.head_commit,
                    "Branch divergence unknown: comparison workspace has no branch for repo"
                );
                continue;
            };
            if !is_computable_git_head(&workspace_head) || !is_computable_git_head(base_head) {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    repo_id = %summary.repo_id,
                    head = %workspace_head,
                    base_head = %base_head,
                    "Branch divergence unknown: unborn or zero head"
                );
                continue;
            }
            match self
                .branch_ahead_behind_counts(
                    &repos,
                    org_id,
                    summary.repo_id,
                    &workspace_head,
                    base_head,
                )
                .await
            {
                Ok((ahead, behind)) => {
                    summary.divergence = Some(i32::try_from(ahead).unwrap_or(i32::MAX));
                    summary.behind_base = Some(i32::try_from(behind).unwrap_or(i32::MAX));
                }
                Err(error) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        repo_id = %summary.repo_id,
                        head = %workspace_head,
                        base_head = %base_head,
                        error = %error,
                        "Branch divergence unknown: git ahead/behind failed"
                    );
                }
            }
        }
        Ok(())
    }

    /// Live merge-target-first, clone-parent-fallback comparison resolution
    /// (ontology D50; plan Q1 answer). The selected workspace is reauthorized
    /// for the caller before any branch heads are read. A denied live merge
    /// target returns unknown and never falls back; only an absent/tombstoned
    /// merge target permits the live clone-parent fallback.
    async fn resolve_divergence_comparison_workspace(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        caller_user_id: Option<Uuid>,
    ) -> Result<Option<Uuid>, G5Error> {
        let Some(actor) = caller_user_id else {
            return Ok(None);
        };
        match crate::services::workspace_repositories::readable_parent_workspace(
            db.conn(),
            workspace_id,
            actor,
        )
        .await
        {
            Ok(parent) => Ok(Some(parent)),
            Err(G5Error::DiffGraphUnavailable { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Committed child-preferred branch heads of a workspace keyed by
    /// `repo_id`: `COALESCE(wbr.head_commit, wb.head_commit)` over canonical
    /// workspace-scope rows only, so a lagging parent row cannot produce
    /// phantom divergence and transient transport cuts never leak into the comparison.
    /// Current head of every workspace-scoped branch of `workspace_id`, by
    /// repository, resolved from `refs/heads/<branch>` in the internal mirror
    /// (P1.1). A branch whose ref is absent is omitted and logged: the
    /// caller treats a missing comparison head as unknown divergence.
    async fn committed_branch_heads_by_repo(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<HashMap<Uuid, String>, G5Error> {
        let org_id =
            crate::services::workspace_reader::workspace_org(db.conn(), workspace_id).await?;
        let members = crate::services::workspace_repositories::load_workspace_repositories(
            db.conn(),
            workspace_id,
            false,
        )
        .await?
        .into_iter()
        .map(|member| (member.repo_id, member.branch_name))
        .collect::<Vec<_>>();
        let heads =
            read_branch_head_oids(&self.repo_storage.scoped(&*db), org_id, &members).await?;
        let mut by_repo = HashMap::new();
        for ((repo_id, branch_name), head) in heads {
            match head {
                Some(head) => {
                    by_repo.insert(repo_id, head);
                }
                None => tracing::warn!(
                    workspace_id = %workspace_id,
                    repo_id = %repo_id,
                    branch = %branch_name,
                    "Comparison branch ref is absent from the internal mirror"
                ),
            }
        }
        Ok(by_repo)
    }

    /// One `graph_ahead_behind(workspace_head, base_head)` on a blocking
    /// thread against the repo's shared git object store — all workspace
    /// branches of a repo share one store, so both SHAs resolve in one
    /// repository (ontology D50).
    async fn branch_ahead_behind_counts(
        &self,
        repos: &ScopedRepoStorage,
        org_id: Uuid,
        repo_id: Uuid,
        workspace_head: &str,
        base_head: &str,
    ) -> Result<(usize, usize), G5Error> {
        let repo_path = repos.repo_path(org_id, repo_id)?;
        let workspace_head = workspace_head.to_string();
        let base_head = base_head.to_string();
        tokio::task::spawn_blocking(move || {
            let repo = GitRepo::open(&repo_path)?;
            repo.graph_ahead_behind(&workspace_head, &base_head)
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Branch ahead/behind task failed: {error}"),
        })?
    }

    /// Update the default branch for a repo after bootstrap detection.
    pub async fn update_repo_default_branch(
        &self,
        db: &mut ScopedDb,
        repo_id: Uuid,
        default_branch: &str,
    ) -> Result<(), G5Error> {
        sqlx::query!(
            "UPDATE repos SET default_branch = $1 WHERE id = $2",
            default_branch,
            repo_id
        )
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to update repo default branch: {e}"),
        })?;
        Ok(())
    }

    /// Committed repository storage shared with the loaders in sibling
    /// modules (`workbench_config.rs`).
    pub(crate) fn repo_storage(&self) -> &TenantRepoStorage {
        &self.repo_storage
    }

    pub async fn get_workspace_repository(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        repo_id: Uuid,
    ) -> Result<crate::models::workspace::WorkspaceRepositoryState, G5Error> {
        self.list_workspace_repository_states(db, workspace_id)
            .await?
            .into_iter()
            .find(|member| member.repo_id == repo_id)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("workspace repository {workspace_id}/{repo_id}"),
            })
    }

    pub(crate) async fn list_workspace_repository_states(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Vec<crate::models::workspace::WorkspaceRepositoryState>, G5Error> {
        let org_id: Uuid = sqlx::query_scalar(
            "SELECT org_id FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;
        Ok(self
            .load_workspace_repository_rows(db, &[workspace_id])
            .await?
            .into_iter()
            .map(
                |member| crate::models::workspace::WorkspaceRepositoryState {
                    workspace_id,
                    org_id,
                    repo_id: member.repo_id,
                    branch_name: member.branch_name,
                    head_commit: member.head_commit,
                    tracking_mode: member.tracking_mode,
                    materialization_status: member.materialization_status,
                },
            )
            .collect())
    }

    pub async fn list_binding_file_tree(
        &self,
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        binding: &RepoBindingVersion,
        path: &str,
        depth: i32,
    ) -> Result<FileTreeListing, G5Error> {
        let repo_view = CommittedRepoReadView::from_binding(scope, org_id, binding)?;
        self.list_file_tree_at_version(&repo_view, path, depth)
            .await
    }

    /// The committed cut's paths under `path` for a live listing to keep:
    /// Git never ignores a tracked path. Read from tree objects only, and
    /// only up to `max_paths`: a tracked set past the live tree's cap
    /// cannot fit the listing either, so no more of it is read.
    pub async fn committed_tracked_paths(
        &self,
        repo_view: &RepoReadView,
        path: &str,
        max_paths: usize,
    ) -> Result<g5_scm::TreePaths, G5Error> {
        repo_view
            .committed_tracked_paths(&self.repo_storage, path, max_paths)
            .await
    }

    /// The Files tree of the effective current cut under `path` to `depth`
    /// (negative for the whole subtree), within the tree's caps
    /// (`workspace_reader::FILE_TREE_LIMITS`): the committed walk stops past
    /// a cap and the listing says so, as the live tree's does. A `path` that
    /// is a file lists that file, sized from its object header.
    pub async fn list_effective_file_tree(
        &self,
        repo_view: &RepoReadView,
        path: &str,
        depth: i32,
    ) -> Result<FileTreeListing, G5Error> {
        let effective_path = if path.is_empty() || path == "/" {
            ""
        } else {
            path
        };
        let storage_depth = if depth < 0 { u32::MAX } else { depth as u32 };
        let listing = match repo_view
            .list_tree_bounded(
                &self.repo_storage,
                effective_path,
                storage_depth,
                FILE_TREE_LIMITS,
            )
            .await
        {
            Ok(listing) => listing,
            Err(G5Error::NotFound { .. }) => {
                return self
                    .single_file_tree(repo_view.committed_view(), effective_path)
                    .await
            }
            Err(error) => return Err(error),
        };
        Ok(Self::render_tree_listing(listing, effective_path))
    }

    pub async fn list_historical_file_tree(
        &self,
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        version: &HistoricalGitVersion,
        path: &str,
        depth: i32,
    ) -> Result<FileTreeListing, G5Error> {
        let repo_view = CommittedRepoReadView::from_historical(scope, org_id, version)?;
        self.list_file_tree_at_version(&repo_view, path, depth)
            .await
    }

    async fn list_file_tree_at_version(
        &self,
        repo_view: &CommittedRepoReadView,
        path: &str,
        depth: i32,
    ) -> Result<FileTreeListing, G5Error> {
        let effective_path = if path.is_empty() || path == "/" {
            ""
        } else {
            path
        };
        let storage_depth = if depth < 0 { u32::MAX } else { depth as u32 };

        let listing = match repo_view
            .list_tree_bounded(
                &self.repo_storage,
                effective_path,
                storage_depth,
                FILE_TREE_LIMITS,
            )
            .await
        {
            Ok(listing) => listing,
            Err(G5Error::NotFound { .. }) => {
                return self.single_file_tree(repo_view, effective_path).await
            }
            Err(error) => return Err(error),
        };
        Ok(Self::render_tree_listing(listing, effective_path))
    }

    /// The listing of a `path` that is not a directory: the file itself,
    /// sized from its object header so a large blob is never loaded to be
    /// measured, or nothing when the commit has no such path.
    async fn single_file_tree(
        &self,
        repo_view: &CommittedRepoReadView,
        path: &str,
    ) -> Result<FileTreeListing, G5Error> {
        let entries = if path.is_empty() {
            Vec::new()
        } else {
            repo_view
                .file_metadata(&self.repo_storage, std::slice::from_ref(&path.to_string()))
                .await?
                .remove(path)
                .flatten()
                .map(|(_hash, size)| {
                    vec![g5_storage::StorageEntry {
                        path: path.to_string(),
                        is_dir: false,
                        size: Some(size as u64),
                        last_modified: None,
                    }]
                })
                .unwrap_or_default()
        };
        Ok(FileTreeListing {
            entries: Self::render_file_tree_entries(entries, path),
            truncated: false,
        })
    }

    fn render_tree_listing(listing: g5_scm::TreeListing, effective_path: &str) -> FileTreeListing {
        let entries = listing
            .entries
            .into_iter()
            .map(|entry| g5_storage::StorageEntry {
                path: entry.path,
                is_dir: matches!(entry.entry_type, g5_scm::TreeEntryType::Directory),
                size: entry.size,
                last_modified: None,
            })
            .collect::<Vec<_>>();
        FileTreeListing {
            entries: Self::render_file_tree_entries(entries, effective_path),
            truncated: listing.truncated,
        }
    }

    pub fn render_file_tree_entries(
        entries: Vec<g5_storage::StorageEntry>,
        effective_path: &str,
    ) -> Vec<FileTreeEntry> {
        // Convert StorageEntry → FileTreeEntry, filtering hidden files and
        // building a nested tree structure from the flat storage entries.
        //
        // The storage backends return a flat list with paths like:
        //   "src", "src/main.rs", "src/lib.rs", "Cargo.toml"
        // We need to nest children under their parent directory entries.

        use std::collections::HashMap;

        // First pass: build flat FileTreeEntry list (filtering hidden)
        let flat_entries: Vec<FileTreeEntry> = entries
            .into_iter()
            .filter(|e| {
                // Delegate tree visibility to the shared g5_paths classifier
                // (workspace_service spec D38 / g5-common git spec D12):
                // non-dot paths, `.semlift/**`, and dot-`.jsonl` components
                // stay visible, plus durable `.g5` repository content
                // (`workspace.yaml`, `preview_profiles/*.yaml`,
                // `experience/**`). Runtime `.g5` metadata and `.git/**`
                // never appear in the Files tree.
                g5_contracts::g5_paths::is_files_tree_visible_path(&e.path)
            })
            .map(|e| {
                let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_string();
                FileTreeEntry {
                    name,
                    path: e.path,
                    entry_type: if e.is_dir {
                        "directory".to_string()
                    } else {
                        "file".to_string()
                    },
                    size: e.size.map(|s| s as i64),

                    children: if e.is_dir { Some(Vec::new()) } else { None },
                }
            })
            .collect();

        // Build path → entry map for nesting. We process entries sorted by
        // path depth (shallowest first) so parents are always created before
        // children.
        let mut by_path: HashMap<String, FileTreeEntry> =
            HashMap::with_capacity(flat_entries.len());

        let mut sorted = flat_entries;
        sorted.sort_by(|a, b| {
            let a_depth = a.path.matches('/').count();
            let b_depth = b.path.matches('/').count();
            a_depth.cmp(&b_depth).then_with(|| {
                let a_is_dir = a.entry_type == "directory";
                let b_is_dir = b.entry_type == "directory";
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            })
        });

        // Collect root-level paths in order. When listing a subdirectory from
        // git, returned paths remain repo-root-relative (e.g. path=src yields
        // `src/main.rs`), so direct children of the scoped path are also roots
        // for this response.
        let scoped_root = effective_path.trim_end_matches('/');
        let mut root_paths: Vec<String> = Vec::new();

        for entry in sorted {
            let path = entry.path.clone();
            if let Some(slash_pos) = path.rfind('/') {
                let parent_path = &path[..slash_pos];
                // Insert child entry, then attach to parent
                by_path.insert(path.clone(), entry);
                if let Some(parent) = by_path.get_mut(parent_path) {
                    if let Some(ref mut children) = parent.children {
                        // Store just the path reference; we'll resolve later
                        children.push(FileTreeEntry {
                            name: String::new(),
                            path: path.clone(),
                            entry_type: String::new(),
                            size: None,

                            children: None,
                        });
                    }
                } else if !scoped_root.is_empty() && parent_path == scoped_root {
                    // The requested directory itself is not included in the
                    // child listing, so direct children of that directory are
                    // response roots.
                    root_paths.push(path.clone());
                }
            } else {
                // Root-level entry
                root_paths.push(path.clone());
                by_path.insert(path, entry);
            }
        }

        // Recursively resolve the tree from the flat map
        fn resolve_entry(
            path: &str,
            by_path: &mut HashMap<String, FileTreeEntry>,
        ) -> Option<FileTreeEntry> {
            let mut entry = by_path.remove(path)?;
            if let Some(child_stubs) = entry.children.take() {
                let mut resolved_children: Vec<FileTreeEntry> = child_stubs
                    .into_iter()
                    .filter_map(|stub| resolve_entry(&stub.path, by_path))
                    .collect();
                // Sort children: directories first, then alphabetical
                resolved_children.sort_by(|a, b| {
                    let a_is_dir = a.entry_type == "directory";
                    let b_is_dir = b.entry_type == "directory";
                    match (a_is_dir, b_is_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    }
                });
                if !resolved_children.is_empty() {
                    entry.children = Some(resolved_children);
                } else {
                    entry.children = Some(Vec::new());
                }
            }
            Some(entry)
        }

        let mut tree: Vec<FileTreeEntry> = root_paths
            .iter()
            .filter_map(|p| resolve_entry(p, &mut by_path))
            .collect();

        // Sort root: directories first, then alphabetical
        tree.sort_by(|a, b| {
            let a_is_dir = a.entry_type == "directory";
            let b_is_dir = b.entry_type == "directory";
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        tree
    }

    /// Lists commits since the Git merge base with the immediate parent.
    /// Parent and child OIDs come from captured cuts with current authorization
    /// and matching membership. An unavailable comparison fails explicitly.
    pub async fn list_branch_commits(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        limit: usize,
    ) -> Result<Vec<CommitSummaryDTO>, G5Error> {
        Self::ensure_branch_commit_listing_materialized(branch)?;

        let actor_id = db.actor_id().into_uuid();
        let snapshot = crate::services::workspace_reader::resolve_published_snapshot(
            db,
            &self.repo_storage,
            AuthorityTarget::Workspace {
                workspace_id: branch.workspace_id,
            },
        )
        .await?;
        let binding = snapshot
            .repo(branch.repo_id)
            .map_err(|error| G5Error::Conflict {
                reason: error.to_string(),
            })?;
        let head_view = CommittedRepoReadView::from_binding(&*db, snapshot.org_id, binding)?;
        let repos = self.repo_storage.scoped(&*db);
        let bases = crate::services::workspace_repositories::parent_comparison_bases(
            db.conn(),
            &repos,
            &snapshot,
            actor_id,
        )
        .await?;
        let Some(base) = bases.and_then(|bases| bases.get(&branch.repo_id).cloned()) else {
            return Ok(Vec::new());
        };
        let base_view = head_view.historical_sibling(&HistoricalGitVersion {
            repo_id: binding.repo_id,
            head_commit: base,
            reason: HistoricalReadReason::Diff,
        })?;
        self.list_branch_commits_between_views(&head_view, &base_view, limit)
            .await
    }

    fn ensure_branch_commit_listing_materialized(
        branch: &crate::models::workspace::WorkspaceRepositoryState,
    ) -> Result<(), G5Error> {
        if branch.materialization_status != "materialized" {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Cannot list commits for a workspace branch that is not materialized",
                    "workspace_id": branch.workspace_id,
                    "materialization_status": branch.materialization_status,
                }),
            });
        }
        Ok(())
    }

    async fn list_branch_commits_between_views(
        &self,
        head_view: &CommittedRepoReadView,
        base_view: &CommittedRepoReadView,
        limit: usize,
    ) -> Result<Vec<CommitSummaryDTO>, G5Error> {
        if head_view.head_commit() == base_view.head_commit()
            || base_view
                .is_descendant_of(&self.repo_storage, head_view)
                .await?
        {
            return Ok(Vec::new());
        }

        let commits = head_view
            .list_commits_since(&self.repo_storage, Some(base_view), limit)
            .await?;

        tracing::debug!(
            repo_id = %head_view.repo_id(),
            count = commits.len(),
            "Listed branch commits"
        );

        Ok(commits.into_iter().map(CommitSummaryDTO::from).collect())
    }

    pub async fn list_branch_commit_history(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<CommitHistoryPageDTO, G5Error> {
        let (binding, head_view) = self.resolve_branch_committed_view(db, branch).await?;
        let cursor_view = cursor
            .map(|head_commit| {
                head_view.historical_sibling(&HistoricalGitVersion {
                    repo_id: binding.repo_id,
                    head_commit: head_commit.to_string(),
                    reason: HistoricalReadReason::CommitHistory,
                })
            })
            .transpose()?;
        let page = head_view
            .list_first_parent_history(&self.repo_storage, cursor_view.as_ref(), limit)
            .await?;
        Ok(page.into())
    }

    pub async fn get_branch_commit_diff(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        commit_sha: &str,
    ) -> Result<CommitDiffDTO, G5Error> {
        let (binding, head_view) = self.resolve_branch_committed_view(db, branch).await?;
        let commit_view = head_view.historical_sibling(&HistoricalGitVersion {
            repo_id: binding.repo_id,
            head_commit: commit_sha.to_string(),
            reason: HistoricalReadReason::CommitHistory,
        })?;
        let diff = head_view
            .commit_diff(&self.repo_storage, &commit_view)
            .await?;
        Ok(diff.into())
    }

    pub async fn get_branch_commit_file_diff(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        commit_sha: &str,
        path: &str,
    ) -> Result<g5_scm::CommitFileDiff, G5Error> {
        let (binding, head_view) = self.resolve_branch_committed_view(db, branch).await?;
        let commit_view = head_view.historical_sibling(&HistoricalGitVersion {
            repo_id: binding.repo_id,
            head_commit: commit_sha.to_string(),
            reason: HistoricalReadReason::CommitHistory,
        })?;
        head_view
            .historical_file_diff(&self.repo_storage, &commit_view, path)
            .await
    }

    pub(crate) async fn resolve_branch_committed_view(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
    ) -> Result<(RepoBindingVersion, CommittedRepoReadView), G5Error> {
        if branch.materialization_status != "materialized" {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Source Control history requires a materialized workspace branch",
                    "workspace_id": branch.workspace_id,
                    "materialization_status": branch.materialization_status,
                }),
            });
        }
        let target = AuthorityTarget::Workspace {
            workspace_id: branch.workspace_id,
        };
        let snapshot = crate::services::workspace_reader::resolve_published_snapshot(
            db,
            &self.repo_storage,
            target,
        )
        .await?;
        let binding =
            snapshot
                .repo(branch.repo_id)
                .map_err(|message| G5Error::ValidationError {
                    details: serde_json::json!({
                        "message": message,
                        "workspace_id": branch.workspace_id,
                        "repo_id": branch.repo_id,
                    }),
                })?;
        if binding.workspace_id != branch.workspace_id {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Source Control branch is not the repository member selected by its authority scope",
                    "workspace_id": branch.workspace_id,
                    "authoritative_workspace_id": binding.workspace_id,
                    "repo_id": branch.repo_id,
                }),
            });
        }
        let binding = binding.clone();
        let view = CommittedRepoReadView::from_binding(&*db, snapshot.org_id, &binding)?;
        Ok((binding, view))
    }

    async fn preview_repo_read_view(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        merged: bool,
    ) -> Result<Option<CommittedRepoReadView>, G5Error> {
        if branch.materialization_status != "materialized" || branch.head_commit.is_empty() {
            return Ok(None);
        }
        let target = AuthorityTarget::Workspace {
            workspace_id: branch.workspace_id,
        };
        let snapshot = crate::services::workspace_reader::resolve_published_snapshot(
            db,
            &self.repo_storage,
            target,
        )
        .await?;
        let binding =
            snapshot
                .repo(branch.repo_id)
                .map_err(|message| G5Error::ValidationError {
                    details: serde_json::json!({
                        "message": message,
                        "workspace_id": branch.workspace_id,
                        "repo_id": branch.repo_id,
                    }),
                })?;
        if binding.workspace_id != branch.workspace_id {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Preview profile branch is not the repository member selected by its authority scope",
                    "workspace_id": branch.workspace_id,
                    "authoritative_workspace_id": binding.workspace_id,
                    "repo_id": branch.repo_id,
                }),
            });
        }

        let view = if merged {
            let actor = db.actor_id().into_uuid();
            let parent_id = crate::services::workspace_repositories::readable_parent_workspace(
                db.conn(),
                branch.workspace_id,
                actor,
            )
            .await?;
            let parent = crate::services::workspace_reader::resolve_published_snapshot(
                db,
                &self.repo_storage,
                AuthorityTarget::Workspace {
                    workspace_id: parent_id,
                },
            )
            .await?;
            if snapshot
                .repos
                .iter()
                .map(|repo| repo.repo_id)
                .collect::<BTreeSet<_>>()
                != parent
                    .repos
                    .iter()
                    .map(|repo| repo.repo_id)
                    .collect::<BTreeSet<_>>()
            {
                return Err(G5Error::Conflict {
                    reason: "Parent workspace repository set differs from the preview workspace"
                        .into(),
                });
            }
            CommittedRepoReadView::from_binding(
                &*db,
                parent.org_id,
                parent
                    .repo(branch.repo_id)
                    .map_err(|error| G5Error::Conflict {
                        reason: error.to_string(),
                    })?,
            )?
        } else {
            CommittedRepoReadView::from_binding(&*db, snapshot.org_id, binding)?
        };
        Ok(Some(view))
    }

    async fn list_preview_profiles_from_view(
        &self,
        repo_view: &CommittedRepoReadView,
    ) -> Result<Vec<PreviewProfileDTO>, G5Error> {
        let mut trees = repo_view
            .list_trees(
                &self.repo_storage,
                &[(".g5/preview_profiles".to_string(), 1)],
            )
            .await?;
        let entries = trees
            .remove(".g5/preview_profiles")
            .flatten()
            .unwrap_or_default();
        let paths = entries
            .into_iter()
            .filter(|entry| !matches!(entry.entry_type, g5_scm::TreeEntryType::Directory))
            .filter_map(|entry| {
                let file_name = std::path::Path::new(&entry.path)
                    .file_name()
                    .and_then(|name| name.to_str())?;
                let name = file_name.strip_suffix(".yaml")?.to_string();
                if g5_contracts::types::validate_preview_profile_name(&name).is_err() {
                    return None;
                }
                Some((entry.path, name))
            })
            .collect::<Vec<_>>();
        let requested_paths = paths
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let mut files = repo_view
            .read_files(&self.repo_storage, &requested_paths)
            .await?;
        let mut profiles = Vec::new();
        for (path, name) in paths {
            let Some(bytes) = files.remove(&path).flatten() else {
                continue;
            };
            // Launch's parser: a profile it would refuse is not offered.
            let Some(config) = std::str::from_utf8(&bytes).ok().and_then(|content| {
                read_preview_application(Some(name.clone()), path, content).ok()
            }) else {
                continue;
            };
            profiles.push(PreviewProfileDTO {
                name,
                command: config.command,
                port: config.port,
                working_dir: config.working_dir,
                description: config.description,
                env_keys: config.env_keys,
                cleanup_command: config.cleanup_command,
            });
        }
        profiles.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(profiles)
    }

    pub async fn list_preview_profiles(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        merged: bool,
    ) -> Result<Vec<PreviewProfileDTO>, G5Error> {
        let Some(repo_view) = self.preview_repo_read_view(db, branch, merged).await? else {
            return Ok(vec![]);
        };
        self.list_preview_profiles_from_view(&repo_view).await
    }

    /// The preview applications committed on the branch: the `preview` block
    /// of `.g5/workspace.yaml` and every valid named profile, read from the
    /// published snapshot the way `list_preview_profiles` does. This is the
    /// source while the owner's VM is not running
    /// (`g5.preview.start-and-preview`); the live checkout is the source while
    /// it runs. `merged` reads the base branch instead of the workspace HEAD.
    pub async fn list_committed_preview_applications(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        merged: bool,
    ) -> Result<Vec<PreviewApplicationDTO>, G5Error> {
        let Some(repo_view) = self.preview_repo_read_view(db, branch, merged).await? else {
            return Ok(vec![]);
        };
        let default_path = crate::services::preview_config::preview_config_path(None);
        let mut files = repo_view
            .read_files(&self.repo_storage, std::slice::from_ref(&default_path))
            .await?;
        let mut applications = Vec::new();
        if let Some(Some(bytes)) = files.remove(&default_path) {
            // Launch's parser: a block it would refuse is not offered.
            if let Some(application) = std::str::from_utf8(&bytes).ok().and_then(|content| {
                read_preview_application(None, default_path.clone(), content).ok()
            }) {
                applications.push(application);
            }
        }
        for profile in self.list_preview_profiles_from_view(&repo_view).await? {
            applications.push(PreviewApplicationDTO {
                source: PreviewApplicationSource::NamedProfile,
                config_path: crate::services::preview_config::preview_config_path(Some(
                    &profile.name,
                )),
                profile: Some(profile.name),
                command: profile.command,
                port: profile.port,
                working_dir: profile.working_dir,
                description: profile.description,
                env_keys: profile.env_keys,
                cleanup_command: profile.cleanup_command,
            });
        }
        Ok(applications)
    }

    /// One committed preview application of the workspace branch, by profile
    /// name (`None` is the workspace default), or `None` when the branch does
    /// not commit it. A committed one the launch would refuse is that
    /// refusal, so the owner learns what to fix rather than that nothing is
    /// committed.
    pub async fn read_committed_preview_application(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        profile: Option<&str>,
    ) -> Result<Option<PreviewApplicationDTO>, AppError> {
        if let Some(name) = profile {
            g5_contracts::types::validate_preview_profile_name(name).map_err(|error| {
                G5Error::ValidationError {
                    details: serde_json::json!({"field": "profile", "message": error.to_string()}),
                }
            })?;
        }
        let Some(repo_view) = self.preview_repo_read_view(db, branch, false).await? else {
            return Ok(None);
        };
        let path = crate::services::preview_config::preview_config_path(profile);
        let mut files = repo_view
            .read_files(&self.repo_storage, std::slice::from_ref(&path))
            .await?;
        let Some(Some(bytes)) = files.remove(&path) else {
            return Ok(None);
        };
        let content = std::str::from_utf8(&bytes).map_err(|_| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "config",
                "message": format!("{path} is not UTF-8 text"),
            }),
        })?;
        read_preview_application(profile.map(str::to_string), path.clone(), content)
            .map(Some)
            .map_err(|error| match error.inner {
                // The parser's refusal, named with the file, as the dialog
                // shows the reason verbatim.
                G5Error::ValidationError { details } => AppError::from_error(G5Error::Conflict {
                    reason: format!(
                        "the committed preview configuration {path} is refused: {}",
                        details["message"].as_str().unwrap_or("invalid")
                    ),
                }),
                other => AppError::from_error(other),
            })
    }

    /// Save a named preview profile in the attached owner's live checkout.
    /// The workspace flush lifecycle is the only path that advances Git.
    pub async fn save_preview_profile(
        &self,
        db: ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        user_id: Uuid,
        input: PreviewProfileInput,
        runtime: &crate::services::WorkspaceRuntime,
    ) -> Result<CommitPreviewProfileResult, crate::errors::AppError> {
        use crate::errors::AppError;

        g5_contracts::types::validate_preview_profile_name(&input.name).map_err(|e| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({ "field": "name", "message": e }),
            })
        })?;
        if input.command.trim().is_empty() || !(1..=65535).contains(&input.port) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "profile",
                    "message": "preview profile must set a non-empty `command` and a `port` in 1..=65535",
                }),
            }));
        }

        let rel_path = format!(".g5/preview_profiles/{}.yaml", input.name);
        let yaml = render_preview_profile_yaml(&input).map_err(AppError::from_error)?;
        self.save_live_config_file(
            db,
            branch,
            user_id,
            runtime,
            &rel_path,
            &yaml,
            input.expected_head_commit.as_deref(),
            input.overwrite,
            format!("Preview profile `{}` already exists", input.name),
        )
        .await?;

        Ok(CommitPreviewProfileResult {
            head_commit: branch.head_commit.clone(),
            profile: PreviewProfileDTO {
                name: input.name,
                command: input.command,
                port: input.port,
                working_dir: input.working_dir,
                description: input.description,
                env_keys: input.env.keys().cloned().collect(),
                cleanup_command: input.cleanup_command,
            },
        })
    }

    /// List saved explainer templates from the committed workspace branch.
    /// The attached owner can have a newer live version; list views remain a
    /// durable Git projection until the workspace is flushed.
    pub async fn list_explainer_templates(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
    ) -> Result<Vec<SavedExplainerTemplate>, G5Error> {
        if branch.materialization_status != "materialized" || branch.head_commit.is_empty() {
            return Ok(vec![]);
        }
        let (_, repo_view) = self.resolve_branch_committed_view(db, branch).await?;
        let entries = match repo_view
            .list_tree(&self.repo_storage, ".g5/explainer_templates", 1)
            .await
        {
            Ok(entries) => entries,
            Err(G5Error::NotFound { .. }) => return Ok(vec![]),
            Err(error) => return Err(error),
        };
        let candidates = entries
            .into_iter()
            .filter(|entry| !matches!(entry.entry_type, g5_scm::TreeEntryType::Directory))
            .filter_map(|entry| {
                let file_name = std::path::Path::new(&entry.path)
                    .file_name()
                    .and_then(|name| name.to_str())?;
                let slug = file_name.strip_suffix(".yaml")?.to_string();
                if g5_contracts::types::validate_explainer_template_slug(&slug).is_err()
                    || g5_contracts::types::is_reserved_explainer_template_slug(&slug)
                {
                    return None;
                }
                Some((entry.path, slug))
            })
            .collect::<Vec<_>>();
        let paths = candidates
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let mut files = repo_view.read_files(&self.repo_storage, &paths).await?;
        let mut templates = Vec::new();
        for (path, slug) in candidates {
            let Some(bytes) = files.remove(&path).flatten() else {
                continue;
            };
            let doc: serde_yaml::Value = match serde_yaml::from_slice(&bytes) {
                Ok(doc) => doc,
                Err(_) => continue,
            };
            let name = doc
                .get("name")
                .and_then(serde_yaml::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let prompt = doc
                .get("prompt")
                .and_then(serde_yaml::Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let (Some(name), Some(prompt)) = (name, prompt) else {
                continue;
            };
            templates.push(SavedExplainerTemplate {
                slug,
                name: name.to_string(),
                prompt: prompt.to_string(),
            });
        }
        templates.sort_by(|left, right| left.slug.cmp(&right.slug));
        Ok(templates)
    }

    /// Save an explainer template in the attached owner's live checkout.
    pub async fn save_explainer_template(
        &self,
        db: ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        user_id: Uuid,
        input: ExplainerTemplateInput,
        runtime: &crate::services::WorkspaceRuntime,
    ) -> Result<CommitExplainerTemplateResult, crate::errors::AppError> {
        use crate::errors::AppError;

        g5_contracts::types::validate_explainer_template_slug_for_save(&input.slug).map_err(
            |e| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({ "field": "slug", "message": e }),
                })
            },
        )?;
        if input.name.trim().is_empty() || input.prompt.trim().is_empty() {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "template",
                    "message": "explainer template must set non-empty `name` and `prompt` fields",
                }),
            }));
        }

        let rel_path = format!(".g5/explainer_templates/{}.yaml", input.slug);
        let yaml = render_explainer_template_yaml(&input).map_err(AppError::from_error)?;
        self.save_live_config_file(
            db,
            branch,
            user_id,
            runtime,
            &rel_path,
            &yaml,
            input.expected_head_commit.as_deref(),
            input.overwrite,
            format!("Explainer template `{}` already exists", input.slug),
        )
        .await?;

        Ok(CommitExplainerTemplateResult {
            head_commit: branch.head_commit.clone(),
            template: SavedExplainerTemplate {
                slug: input.slug,
                name: input.name,
                prompt: input.prompt,
            }
            .into(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_live_config_file(
        &self,
        db: ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        user_id: Uuid,
        runtime: &crate::services::WorkspaceRuntime,
        path: &str,
        content: &str,
        expected_head_commit: Option<&str>,
        overwrite: bool,
        duplicate_reason: String,
    ) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;

        if let Some(expected) = expected_head_commit {
            if expected != branch.head_commit {
                return Err(AppError::from_error(G5Error::Conflict {
                    reason: "branch head advanced; reload and retry saving the configuration file"
                        .to_string(),
                }));
            }
        }
        let handle = runtime.owner_handle(branch.workspace_id, user_id).await?;
        runtime
            .workspace_changes(&db, branch.workspace_id, user_id)
            .await?;
        db.rollback().await.map_err(AppError::from)?;

        let request = |operation: &str| {
            crate::services::workspace_runtime::RuntimeRequest::owner(
                &handle,
                user_id,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
                    + 30_000,
                format!("workspace-config:{operation}:{}", Uuid::new_v4()),
            )
        };
        let current = match runtime
            .file_read(&handle, &request("read"), branch.repo_id, path)
            .await
        {
            Ok(value) => Some(value),
            Err(error) if error.message == "FILE_MISSING" => None,
            Err(error) => {
                return Err(AppError::from_error(
                    crate::services::workspace_runtime::runtime_to_g5(error),
                ));
            }
        };
        if current.is_some() && !overwrite {
            return Err(AppError::from_error(G5Error::Conflict {
                reason: duplicate_reason,
            }));
        }
        let expected_hash = current
            .as_ref()
            .and_then(|value| value.get("hash"))
            .and_then(serde_json::Value::as_str);
        runtime
            .file_write(
                &handle,
                &request("write"),
                branch.repo_id,
                path,
                content,
                "utf-8",
                expected_hash,
            )
            .await
            .map_err(|error| match error.message.as_str() {
                "FILE_EXISTS" => AppError::from_error(G5Error::Conflict {
                    reason: duplicate_reason,
                }),
                "FILE_VERSION_CONFLICT" => AppError::from_error(G5Error::FileVersionConflict {
                    details: error.data,
                }),
                _ => AppError::from_error(crate::services::workspace_runtime::runtime_to_g5(error)),
            })?;
        Ok(())
    }

    /// Compatibility entrypoint for callers that still hold a validated
    /// workspace-branch row. Repository I/O remains transaction-free.
    pub async fn get_file_content(
        &self,
        db: &mut ScopedDb,
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        path: &str,
    ) -> Result<FileContentResult, G5Error> {
        self.get_branch_file_content(&*db, branch, path).await
    }

    pub async fn get_binding_file_content(
        &self,
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        binding: &RepoBindingVersion,
        path: &str,
    ) -> Result<FileContentResult, G5Error> {
        let repo_view = CommittedRepoReadView::from_binding(scope, org_id, binding)?;
        self.get_file_content_at_version(&repo_view, path).await
    }

    /// Transaction-free committed read: resolves the branch's `head_commit`
    /// against Git using only the already-loaded branch row. The metrics
    /// materialize phase depends on this holding no tenant DB scope
    /// (`tests/current_reader_contract.rs`).
    pub(crate) async fn get_branch_file_content(
        &self,
        scope: &(impl OrgScope + ?Sized),
        branch: &crate::models::workspace::WorkspaceRepositoryState,
        path: &str,
    ) -> Result<FileContentResult, G5Error> {
        if branch.materialization_status != "materialized" {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Cannot read files from a workspace branch that is not materialized",
                    "workspace_id": branch.workspace_id,
                    "materialization_status": branch.materialization_status,
                }),
            });
        }
        let version = HistoricalGitVersion {
            repo_id: branch.repo_id,
            head_commit: branch.head_commit.clone(),
            reason: HistoricalReadReason::OperationValidation,
        };
        self.get_historical_file_content(scope, branch.org_id, &version, path)
            .await
    }

    pub async fn get_historical_file_content(
        &self,
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        version: &HistoricalGitVersion,
        path: &str,
    ) -> Result<FileContentResult, G5Error> {
        let repo_view = CommittedRepoReadView::from_historical(scope, org_id, version)?;
        self.get_file_content_at_version(&repo_view, path).await
    }

    /// One committed file within `MAX_FILE_READ_BYTES`: a larger one is
    /// refused from its object header (`FileTooLarge`), never loaded.
    async fn get_file_content_at_version(
        &self,
        repo_view: &CommittedRepoReadView,
        path: &str,
    ) -> Result<FileContentResult, G5Error> {
        let bytes = repo_view
            .read_file_bounded(&self.repo_storage, path, MAX_FILE_READ_BYTES)
            .await?;

        let size = bytes.len() as i64;

        Ok(FileContentResult {
            content: bytes,
            size,
            draft_dirty: false,
        })
    }

    /// Get workspace settings.
    pub async fn get_workspace_settings(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<crate::models::WorkspaceSettings, G5Error> {
        if let Some(settings) = sqlx::query_as::<_, crate::models::WorkspaceSettings>(
            r#"SELECT workspace_id, org_id, max_spend_usd, spend_period,
                      spend_period_start, max_input_tokens, max_output_tokens,
                      idle_timeout_secs, response_deadline_secs,
                      bounded_execution_slots, permission_policy,
                      agent_may_modify_other_workspaces,
                      updated_by, updated_at
               FROM workspace_settings
               WHERE workspace_id = $1"#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch workspace settings: {e}"),
        })? {
            return Ok(settings);
        }

        let org_id = sqlx::query_scalar!(
            r#"SELECT org_id AS "org_id!" FROM workspaces WHERE id = $1"#,
            workspace_id,
        )
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to resolve workspace for settings bootstrap: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })?;

        // A workspace without settings starts with its organization's default
        // idle timeout, like every new workspace (`g5.lifecycle.idle.settings`).
        let idle_timeout_secs =
            crate::services::org_admin_settings::default_idle_timeout_secs(db.conn(), org_id)
                .await?;
        sqlx::query!(
            r#"
            INSERT INTO workspace_settings (workspace_id, org_id, idle_timeout_secs)
            VALUES ($1, $2, $3)
            ON CONFLICT (workspace_id) DO NOTHING
            "#,
            workspace_id,
            org_id,
            idle_timeout_secs,
        )
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to bootstrap workspace settings: {e}"),
        })?;

        sqlx::query_as::<_, crate::models::WorkspaceSettings>(
            r#"SELECT workspace_id, org_id, max_spend_usd, spend_period,
                      spend_period_start, max_input_tokens, max_output_tokens,
                      idle_timeout_secs, response_deadline_secs,
                      bounded_execution_slots, permission_policy,
                      agent_may_modify_other_workspaces,
                      updated_by, updated_at
               FROM workspace_settings
               WHERE workspace_id = $1"#,
        )
        .bind(workspace_id)
        .fetch_optional(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch bootstrapped workspace settings: {e}"),
        })?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("settings for workspace {workspace_id}"),
        })
    }

    /// Update workspace settings (partial merge update).
    ///
    /// Fetches the current settings, merges in the provided fields (only
    /// non-`None` fields from the request are applied), and UPDATEs the row.
    pub async fn update_workspace_settings(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        user_id: Uuid,
        req: UpdateWorkspaceSettingsRequest,
    ) -> Result<crate::models::WorkspaceSettings, G5Error> {
        // 1. Fetch existing settings
        let existing = self.get_workspace_settings(db, workspace_id).await?;

        // 2. Merge fields — if the request field is Some, use it; otherwise keep existing
        let spend_period = req.spend_period.unwrap_or(existing.spend_period);
        let idle_timeout_secs = req.idle_timeout_secs.unwrap_or(existing.idle_timeout_secs);
        let response_deadline_secs = req
            .response_deadline_secs
            .unwrap_or(existing.response_deadline_secs);
        let bounded_execution_slots = req
            .bounded_execution_slots
            .unwrap_or(existing.bounded_execution_slots);
        let permission_policy = req.permission_policy.or(existing.permission_policy);
        let agent_may_modify_other_workspaces = req
            .agent_may_modify_other_workspaces
            .unwrap_or(existing.agent_may_modify_other_workspaces);
        // Double-option fields: None = keep, Some(None) = set NULL, Some(Some(v)) = set value
        let max_spend_usd: Option<rust_decimal::Decimal> = match req.max_spend_usd {
            None => existing.max_spend_usd,
            Some(None) => None,
            Some(Some(v)) => Some(rust_decimal::Decimal::try_from(v).unwrap_or_default()),
        };
        let max_input_tokens: Option<i64> = match req.max_input_tokens {
            None => existing.max_input_tokens,
            Some(inner) => inner,
        };
        let max_output_tokens: Option<i64> = match req.max_output_tokens {
            None => existing.max_output_tokens,
            Some(inner) => inner,
        };

        // 3. Validate constraints
        if !(60..=86_400).contains(&idle_timeout_secs) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({ "idleTimeoutSecs": "Must be between 60 and 86400" }),
            });
        }
        if let Some(v) = max_spend_usd {
            if v < rust_decimal::Decimal::ZERO {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({ "maxSpendUsd": "Must be >= 0" }),
                });
            }
        }
        if let Some(v) = max_input_tokens {
            if v < 0 {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({ "maxInputTokens": "Must be >= 0" }),
                });
            }
        }
        if let Some(v) = max_output_tokens {
            if v < 0 {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({ "maxOutputTokens": "Must be >= 0" }),
                });
            }
        }
        if !(30..=86_400).contains(&response_deadline_secs) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({ "responseDeadlineSecs": "Must be between 30 and 86400" }),
            });
        }
        if !(1..=64).contains(&bounded_execution_slots) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({ "boundedExecutionSlots": "Must be between 1 and 64" }),
            });
        }
        // 4. UPDATE
        sqlx::query(
            r#"UPDATE workspace_settings
               SET max_spend_usd = $1,
                   spend_period = $2,
                   max_input_tokens = $3,
                   max_output_tokens = $4,
                   idle_timeout_secs = $5,
                   response_deadline_secs = $6,
                   bounded_execution_slots = $7,
                   permission_policy = $8,
                   agent_may_modify_other_workspaces = $9,
                   updated_by = $10,
                   updated_at = now()
               WHERE workspace_id = $11"#,
        )
        .bind(max_spend_usd)
        .bind(spend_period)
        .bind(max_input_tokens)
        .bind(max_output_tokens)
        .bind(idle_timeout_secs)
        .bind(response_deadline_secs)
        .bind(bounded_execution_slots)
        .bind(permission_policy)
        .bind(agent_may_modify_other_workspaces)
        .bind(user_id)
        .bind(workspace_id)
        .execute(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to update workspace settings: {e}"),
        })?;

        // 5. Return updated settings
        self.get_workspace_settings(db, workspace_id).await
    }
}

fn normalize_head_commit(head_commit: &str) -> Option<String> {
    let trimmed = head_commit.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '0') {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn validate_git_branch_name(branch: &str) -> Result<(), G5Error> {
    if branch.is_empty()
        || branch.len() > 250
        || branch.starts_with('-')
        || branch == "@"
        || branch.ends_with('/')
        || branch.ends_with('.')
        || branch.contains("..")
        || branch.contains("//")
        || branch.contains("@{")
        || branch.split('/').any(|component| {
            component.is_empty() || component.starts_with('.') || component.ends_with(".lock")
        })
        || branch
            .bytes()
            .any(|b| b == 0 || b < 0x20 || b == 0x7f || b == b' ')
        || branch
            .chars()
            .any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
    {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "field": "branchName",
                "message": "branch name is not a safe git branch name",
                "branchName": branch,
            }),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Ensure the shared per-repo mirror exists before publication, cloning it
/// through the API network runner when absent.
async fn ensure_publication_mirror(
    local_path: &std::path::Path,
    git_url: &str,
    auth: &g5_scm::GitAuth,
) -> Result<(), G5Error> {
    if local_path.join(".git").exists() || local_path.join("HEAD").exists() {
        return Ok(());
    }
    crate::git_runtime::network(NETWORK_DEADLINE)?
        .clone_checkout(git_url, local_path, None, auth)
        .await?;
    Ok(())
}

/// Create a new remote branch at an exact mirror commit. Git refuses an
/// existing destination; the caller confirms the result by reading the tip.
async fn publish_create_ref(
    local_path: &std::path::Path,
    git_url: &str,
    head: &str,
    remote_branch: &str,
    auth: &g5_scm::GitAuth,
) -> Result<(), G5Error> {
    ensure_publication_mirror(local_path, git_url, auth).await?;
    let git = crate::git_runtime::network(NETWORK_DEADLINE)?;
    let (path, head, remote_branch, auth) = (
        local_path.to_path_buf(),
        head.to_owned(),
        remote_branch.to_owned(),
        auth.clone(),
    );
    tokio::task::spawn_blocking(move || -> Result<(), G5Error> {
        let repo = GitRepo::open(&path)?;
        if !repo.has_commit(&head)? {
            return Err(G5Error::GitError {
                message: format!("local head {head} is not present in the mirror; cannot publish"),
            });
        }
        repo.push_create_ref(&git, "origin", &head, &remote_branch, &auth)
    })
    .await
    .map_err(|join_err| G5Error::Internal {
        message: format!("create-ref push task panicked: {join_err}"),
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clone-merge takes the parent's commit fence before the owner
    /// check's workspace row lock, the order every landing uses.
    #[test]
    fn clone_merge_takes_the_parent_fence_before_its_row_lock() {
        let source = include_str!("workspace_service.rs");
        let start = source
            .find("Failed to open clone-merge application transaction")
            .unwrap();
        let body = &source[start..start + 2000];
        let fence = body.find("acquire_workspace_commit_fence(").unwrap();
        let owner = body.find("ensure_workspace_owner(").unwrap();
        assert!(fence < owner, "fence must precede the owner row lock");
    }

    #[test]
    fn render_preview_profile_yaml_round_trips() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());
        let input = PreviewProfileInput {
            name: "web".to_string(),
            command: "  npm run dev  ".to_string(),
            port: 5173,
            working_dir: Some("client".to_string()),
            env,
            description: Some("Web client".to_string()),
            cleanup_command: Some(" npm run down ".to_string()),
            expected_head_commit: None,
            overwrite: false,
        };
        let yaml = render_preview_profile_yaml(&input).expect("render");
        // The rendered top-level shape parses back as the preview block (D18).
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("parse");
        assert_eq!(
            doc.get("command").and_then(|v| v.as_str()),
            Some("npm run dev"),
            "command is trimmed"
        );
        assert_eq!(doc.get("port").and_then(|v| v.as_u64()), Some(5173));
        assert_eq!(
            doc.get("working_dir").and_then(|v| v.as_str()),
            Some("client")
        );
        assert_eq!(
            doc.get("env")
                .and_then(|v| v.get("NODE_ENV"))
                .and_then(|v| v.as_str()),
            Some("production")
        );
        assert_eq!(
            doc.get("cleanup_command").and_then(|v| v.as_str()),
            Some("npm run down"),
            "cleanup_command is trimmed"
        );
    }

    #[test]
    fn render_preview_profile_yaml_omits_empty_optionals() {
        let input = PreviewProfileInput {
            name: "api".to_string(),
            command: "./serve".to_string(),
            port: 8080,
            working_dir: Some("   ".to_string()),
            env: std::collections::BTreeMap::new(),
            description: None,
            cleanup_command: Some(" ".to_string()),
            expected_head_commit: None,
            overwrite: false,
        };
        let yaml = render_preview_profile_yaml(&input).expect("render");
        // Blank working_dir, empty env, and absent description are omitted.
        assert!(
            !yaml.contains("cleanup_command"),
            "blank cleanup_command omitted"
        );
        assert!(!yaml.contains("working_dir"), "blank working_dir omitted");
        assert!(!yaml.contains("env"), "empty env omitted");
        assert!(!yaml.contains("description"), "absent description omitted");
        assert!(yaml.contains("command"));
        assert!(yaml.contains("port"));
    }

    fn branch_summary(
        repo_id: Uuid,
        branch_name: &str,
        head_commit: &str,
        materialization_status: &str,
    ) -> BranchSummary {
        BranchSummary {
            workspace_id: Uuid::new_v4(),
            repo_id,
            repo_name: "repo".to_string(),
            branch_name: branch_name.to_string(),
            tracking_mode: "tracking".to_string(),
            head_commit: head_commit.to_string(),
            materialization_status: materialization_status.to_string(),
            divergence: None,
            behind_base: None,
            has_draft: false,
            dirty_file_count: 0,
            remote_branch: None,
            local_branch_name: None,
            remote_repo_name: None,
            co_tracking_workspaces: None,
            co_tracking_workspace_count: None,
            auto_pull_enabled: None,
            remote_eligible: None,
        }
    }

    #[test]
    fn git_branch_name_rejects_argument_injection_shapes() {
        assert!(matches!(
            validate_git_branch_name(""),
            Err(G5Error::ValidationError { .. })
        ));

        for branch in [
            "-bad",
            "bad branch",
            "bad\nbranch",
            "bad..branch",
            "bad@{branch",
            "bad.lock",
            ".bad",
            "/bad",
            "bad/",
            "bad.",
            "bad:branch",
            "bad\\branch",
            "@",
        ] {
            let err =
                validate_git_branch_name(branch).expect_err("unsafe git branch should be rejected");
            assert!(matches!(err, G5Error::ValidationError { .. }));
        }
    }

    #[test]
    fn repo_full_name_from_git_url_parses_https_and_ssh() {
        // https, with and without the `.git` suffix
        assert_eq!(
            repo_full_name_from_git_url("https://github.com/octocat/hello-world.git"),
            Some("octocat/hello-world".to_string())
        );
        assert_eq!(
            repo_full_name_from_git_url("https://github.com/octocat/hello-world"),
            Some("octocat/hello-world".to_string())
        );
        // scp-like ssh form `git@host:owner/repo.git`
        assert_eq!(
            repo_full_name_from_git_url("git@github.com:octocat/hello-world.git"),
            Some("octocat/hello-world".to_string())
        );
        // GitLab subgroups keep their full path
        assert_eq!(
            repo_full_name_from_git_url("https://gitlab.com/group/subgroup/repo.git"),
            Some("group/subgroup/repo".to_string())
        );
    }

    #[test]
    fn repo_full_name_from_git_url_rejects_pathless() {
        assert_eq!(repo_full_name_from_git_url("https://github.com/"), None);
        assert_eq!(repo_full_name_from_git_url("not-a-url"), None);
        assert_eq!(repo_full_name_from_git_url(""), None);
    }

    #[test]
    fn local_repo_identity_requires_no_provider_and_a_local_url() {
        for url in ["g5-local://repo", "/tmp/local.git", "file:///tmp/local.git"] {
            assert!(is_local_repo_identity(None, url), "{url}");
            for provider in ["github", "gitlab", "unsupported"] {
                assert!(
                    !is_local_repo_identity(Some(provider), url),
                    "{provider}: {url}"
                );
            }
        }
        for url in [
            "https://github.com/o/r.git",
            "https://other.example/o/r.git",
            "ssh://git@other.example/o/r.git",
            "git@other.example:o/r.git",
            "/tmp/host:repo.git",
            "file://other.example/repo.git",
            "",
        ] {
            assert!(!is_local_repo_identity(None, url), "{url}");
        }
    }

    #[test]
    fn is_allowed_remote_git_scheme_accepts_only_provider_https() {
        assert!(is_allowed_remote_git_scheme("https://github.com/o/r.git"));
        assert!(is_allowed_remote_git_scheme(
            "https://gitlab.com/group/repo.git"
        ));
        for url in [
            "http://github.com/o/r.git",
            "https://github.com.attacker.example/o/r.git",
            "ssh://git@github.com/o/r.git",
            "git://github.com/o/r.git",
            "git@github.com:o/r.git",
        ] {
            assert!(!is_allowed_remote_git_scheme(url), "{url}");
            assert!(!is_local_git_remote(url), "{url}");
        }
        assert!(!is_local_git_remote("/tmp/attacker:repo.git"));
        assert!(is_local_git_remote("/tmp/local.git"));
        assert!(is_local_git_remote("file:///tmp/local.git"));
    }

    #[test]
    fn is_allowed_remote_git_scheme_rejects_local_and_exec_schemes() {
        // file:// and bare file: — server-local
        assert!(!is_allowed_remote_git_scheme("file:///srv/git/repo.git"));
        // ext:: — libgit2 command execution escalation
        assert!(!is_allowed_remote_git_scheme(
            "ext::sh -c 'touch /tmp/pwned'"
        ));
        // bare filesystem paths
        assert!(!is_allowed_remote_git_scheme("/srv/git/repo.git"));
        assert!(!is_allowed_remote_git_scheme("./relative/repo"));
        assert!(!is_allowed_remote_git_scheme("repo"));
        // empty
        assert!(!is_allowed_remote_git_scheme(""));
        assert!(!is_allowed_remote_git_scheme("   "));
    }

    #[test]
    fn is_allowed_remote_git_scheme_rejects_scheme_before_scp_at() {
        // Bypass shape: an explicit `scheme::` transport smuggled in ahead of a
        // decoy `@host:path` so the payload reaches the scp-style branch. The
        // `::` before the `@` must be rejected — otherwise a command-executing
        // `ext::` transport could be written to the shared `repos.git_url`.
        assert!(!is_allowed_remote_git_scheme(
            "ext::sh -c 'touch /tmp/pwned'`@evilhost`:owner/repo"
        ));
        assert!(!is_allowed_remote_git_scheme(
            "ext::git-upload-pack owner/repo@host:owner/repo"
        ));
        assert!(!is_allowed_remote_git_scheme(
            "fd::/tmp/socket@evilhost:owner/repo"
        ));
        // SSH/scp cannot carry the per-user HTTPS credential.
        assert!(!is_allowed_remote_git_scheme(
            "git@github.com:owner/repo.git"
        ));
        assert!(!is_allowed_remote_git_scheme(
            "git@gitlab.com:group/subgroup/repo.git"
        ));
    }

    #[test]
    fn repo_chip_meta_remote_repo_name_prefers_provider_then_git_url() {
        let policy = g5_scm::ScmOriginPolicy::default();
        // provider_full_name present → used verbatim
        let provider = RepoChipMeta::new(
            "backend".to_string(),
            Some("acme/backend".to_string()),
            "https://github.com/different/slug.git",
            &policy,
        );
        assert_eq!(
            provider.remote_repo_name(),
            Some("acme/backend".to_string())
        );

        // provider_full_name NULL (manually-added repo) → derive from git_url
        let manual = RepoChipMeta::new(
            "widgets".to_string(),
            None,
            "https://github.com/contrib/widgets.git",
            &policy,
        );
        assert_eq!(
            manual.remote_repo_name(),
            Some("contrib/widgets".to_string())
        );

        // provider_full_name NULL and git_url unparseable → None
        let opaque = RepoChipMeta::new("x".to_string(), None, "opaque", &policy);
        assert_eq!(opaque.remote_repo_name(), None);
    }

    #[test]
    fn provider_repository_name_uses_the_configured_instance_parser() {
        // An Enterprise prefix and port never enter stored identity.
        let enterprise =
            g5_scm::ScmOriginPolicy::default().with_github_web_url("https://ghe.test:8443/prefix");
        assert_eq!(
            provider_repository_name(&enterprise, "https://ghe.test:8443/prefix/o/r.git"),
            Some("o/r".to_string())
        );
        assert_eq!(
            provider_repository_name(&enterprise, "https://ghe.test:8443/prefix/o/r"),
            Some("o/r".to_string())
        );
        // A path that is not owner/repo under the configured prefix has no
        // GitHub identity; the generic parser is not consulted for GitHub.
        assert_eq!(
            provider_repository_name(&enterprise, "https://ghe.test:8443/prefix/o/r/extra.git"),
            None
        );
        let chip = RepoChipMeta::new(
            "r".to_string(),
            None,
            "https://ghe.test:8443/prefix/o/r.git",
            &enterprise,
        );
        assert_eq!(chip.remote_repo_name(), Some("o/r".to_string()));
        // Public GitHub and GitLab subgroups.
        let public = g5_scm::ScmOriginPolicy::default();
        assert_eq!(
            provider_repository_name(&public, "https://github.com/octocat/hello-world.git"),
            Some("octocat/hello-world".to_string())
        );
        assert_eq!(
            provider_repository_name(&public, "https://gitlab.com/group/subgroup/repo.git"),
            Some("group/subgroup/repo".to_string())
        );
        let gitlab = g5_scm::ScmOriginPolicy::from_gitlab_api_url(Some(
            "https://gitlab.internal:8443/api/v4/user",
        ));
        assert_eq!(
            provider_repository_name(&gitlab, "https://gitlab.internal:8443/a/b/c.git"),
            Some("a/b/c".to_string())
        );
    }

    #[test]
    fn remote_identity_for_only_populates_remote_tracking() {
        let meta = RepoChipMeta::new(
            "backend".to_string(),
            Some("acme/backend".to_string()),
            "https://github.com/acme/backend.git",
            &g5_scm::ScmOriginPolicy::default(),
        );

        // remote-tracking → both fields populated (detail + list paths share this)
        let (rb, rrn) = remote_identity_for(true, Some("main".to_string()), Some(&meta));
        assert_eq!(rb, Some("main".to_string()));
        assert_eq!(rrn, Some("acme/backend".to_string()));

        // non-remote-tracking → both None even when inputs are present
        let (rb, rrn) = remote_identity_for(false, Some("main".to_string()), Some(&meta));
        assert_eq!(rb, None);
        assert_eq!(rrn, None);

        // remote-tracking but repo meta missing → remote_branch passes through,
        // remote_repo_name is None
        let (rb, rrn) = remote_identity_for(true, Some("dev".to_string()), None);
        assert_eq!(rb, Some("dev".to_string()));
        assert_eq!(rrn, None);
    }

    #[test]
    fn test_update_workspace_request_deserialize() {
        let json = r#"{"description": "Updated"}"#;
        let req: UpdateWorkspaceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.description, Some("Updated".to_string()));
    }

    #[test]
    fn test_workspace_list_item_serialization() {
        let item = WorkspaceListItem {
            id: Uuid::nil(),
            name: "Test".to_string(),
            description: None,
            is_base: false,
            read_only: false,
            is_review: false,
            agent_review: None,
            kind: g5_contracts::types::WorkspaceKind::Working,
            owner_user_id: Some(Uuid::nil()),
            created_by: Some(Uuid::nil()),
            cloned_from_id: None,
            branches: vec![],
            tasks_completed: 5,
            total_tasks: 10,
            running_time: None,
            cost: 1.5,
            concurrent_tasks: 2,
            last_updated: "2024-01-01T00:00:00Z".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            primary_issue: None,
            phase: Some("be".to_string()),
        };
        let json = serde_json::to_value(&item).unwrap();
        // Verify camelCase serialization
        assert!(json.get("tasksCompleted").is_some());
        assert!(json.get("totalTasks").is_some());
        assert!(json.get("concurrentTasks").is_some());
        assert!(json.get("lastUpdated").is_some());
        assert!(json.get("createdAt").is_some());
        assert!(json.get("isBase").is_some());
        assert!(json.get("createdBy").is_some());
        assert!(json.get("clonedFromId").is_some());
        // Should NOT have snake_case keys
        assert!(json.get("tasks_completed").is_none());
        assert!(json.get("total_tasks").is_none());
    }

    #[test]
    fn test_workspace_detail_serialization() {
        let detail = WorkspaceDetail {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            name: "Test".to_string(),
            is_base: false,
            kind: g5_contracts::types::WorkspaceKind::Working,
            read_only: false,
            mutation_policy_transition: None,
            description: None,
            cloned_from_id: None,
            docs_repo_id: None,
            created_by: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            branches: vec![],
            docs_folder_path: workspace_docs_folder_path(Uuid::nil()),
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("orgId").is_some());
        assert!(json.get("isBase").is_some());
        assert!(json.get("createdBy").is_some());
        assert!(json.get("createdAt").is_some());
        assert!(json.get("updatedAt").is_some());
        assert!(json.get("clonedFromId").is_some());
        assert!(json.get("docsRepoId").is_some());
        assert!(json.get("docsBranchName").is_none());
        assert!(json.get("docsBranchId").is_none());
        assert_eq!(
            json.get("docsFolderPath").and_then(|v| v.as_str()),
            Some("docs/00000000-0000-0000-0000-000000000000")
        );
    }

    #[test]
    fn test_workspace_docs_folder_path_convention() {
        // The dedicated folder is derived from the immutable workspace id:
        // flat under docs/, lowercase hyphenated UUID, no trailing slash.
        let id = Uuid::parse_str("0192d3a8-1234-7abc-8def-0123456789ab").unwrap();
        assert_eq!(
            workspace_docs_folder_path(id),
            "docs/0192d3a8-1234-7abc-8def-0123456789ab"
        );
        // Rename-immunity: the helper takes only the id, so two workspaces
        // with different names but the same id (a rename) share the path.
        assert_eq!(
            workspace_docs_folder_path(id),
            workspace_docs_folder_path(id)
        );
    }

    #[test]
    fn test_canonical_workspace_summaries_include_every_member_regardless_of_ref_name() {
        let repo_id = Uuid::new_v4();
        let canonical = branch_summary(
            repo_id,
            "feat/workspace",
            "0123456789abcdef0123456789abcdef01234567",
            "materialized",
        );
        let unborn = branch_summary(
            repo_id,
            "feat/unborn",
            "0000000000000000000000000000000000000000",
            "unborn",
        );
        let remote_tracking_base = branch_summary(
            repo_id,
            "remote-tracking-base-123",
            "1111111111111111111111111111111111111111",
            "materialized",
        );

        let branches =
            canonical_workspace_summaries(vec![unborn, remote_tracking_base, canonical.clone()]);

        assert_eq!(branches.len(), 3);
        assert_eq!(branches[2].repo_id, canonical.repo_id);
    }

    fn source_download_binding(
        repo_id: Uuid,
        repo_name: &str,
        branch_name: &str,
        head_commit: &str,
    ) -> RepoBindingVersion {
        RepoBindingVersion {
            repo_id,
            repo_name: repo_name.to_string(),
            repo_slug: repo_name.to_string(),
            repo_role: if repo_name == "docs" {
                g5_contracts::types::RepoRole::Docs
            } else {
                g5_contracts::types::RepoRole::Code
            },
            workspace_id: Uuid::new_v4(),
            branch_name: branch_name.to_string(),
            tracking_mode: "pinned".to_string(),
            remote_branch: None,
            canonical_ref: format!("refs/heads/{branch_name}"),
            head_commit: head_commit.to_string(),
            root_path: repo_name.to_string(),
        }
    }

    fn source_download_snapshot(repos: Vec<RepoBindingVersion>) -> CommittedBindingSnapshot {
        let workspace_id = Uuid::new_v4();
        CommittedBindingSnapshot {
            target: AuthorityTarget::Workspace { workspace_id },
            org_id: Uuid::new_v4(),
            workspace_id,
            snapshot_at: chrono::Utc::now(),
            source: g5_contracts::types::DurableSnapshotSource::Committed,
            repos,
        }
    }

    #[test]
    fn source_download_projects_all_non_docs_members_from_one_snapshot() {
        let docs_repo_id = Uuid::new_v4();
        let code_repo_id = Uuid::new_v4();
        let worker_repo_id = Uuid::new_v4();
        let snapshot = source_download_snapshot(vec![
            source_download_binding(
                docs_repo_id,
                "docs",
                "main",
                "1111111111111111111111111111111111111111",
            ),
            source_download_binding(
                code_repo_id,
                "api",
                "feat/api",
                "2222222222222222222222222222222222222222",
            ),
            source_download_binding(
                worker_repo_id,
                "worker",
                "feat/work",
                "3333333333333333333333333333333333333333",
            ),
        ]);

        let selected = workspace_source_download_from_snapshot(
            "demo".to_string(),
            Some(docs_repo_id),
            snapshot,
        )
        .unwrap();

        assert_eq!(selected.workspace_name, "demo");
        assert_eq!(selected.repos.len(), 2);
        assert_eq!(selected.repos[0].repo_id, code_repo_id);
        assert_eq!(selected.repos[0].repo_name, "api");
        assert_eq!(selected.repos[0].branch_name, "feat/api");
        assert_eq!(selected.repos[1].repo_id, worker_repo_id);
        assert_eq!(selected.repos[1].repo_name, "worker");
    }

    #[test]
    fn source_download_returns_not_found_when_snapshot_contains_only_docs() {
        let docs_repo_id = Uuid::new_v4();
        let snapshot = source_download_snapshot(vec![source_download_binding(
            docs_repo_id,
            "docs",
            "main",
            "1111111111111111111111111111111111111111",
        )]);

        let err = workspace_source_download_from_snapshot(
            "demo".to_string(),
            Some(docs_repo_id),
            snapshot,
        )
        .unwrap_err();

        assert!(matches!(err, G5Error::NotFound { .. }));
    }

    #[test]
    fn source_download_preserves_the_exact_binding_without_branch_ranking() {
        let code_repo_id = Uuid::new_v4();
        let snapshot = source_download_snapshot(vec![source_download_binding(
            code_repo_id,
            "api",
            "exact/workspace-cut",
            "2222222222222222222222222222222222222222",
        )]);

        let selected =
            workspace_source_download_from_snapshot("demo".to_string(), None, snapshot).unwrap();

        assert_eq!(selected.repos.len(), 1);
        assert_eq!(selected.repos[0].branch_name, "exact/workspace-cut");
        assert_eq!(
            selected.repos[0].head_commit,
            "2222222222222222222222222222222222222222",
        );
    }

    #[test]
    fn test_branch_summary_serialization() {
        let summary = BranchSummary {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            repo_name: "docs".to_string(),
            branch_name: "feat/test".to_string(),
            tracking_mode: "pinned".to_string(),
            head_commit: "abc123".to_string(),
            materialization_status: "materialized".to_string(),
            divergence: Some(3),
            behind_base: None,
            has_draft: true,
            dirty_file_count: 5,
            remote_branch: None,
            local_branch_name: None,
            remote_repo_name: None,
            co_tracking_workspaces: None,
            co_tracking_workspace_count: None,
            auto_pull_enabled: None,
            remote_eligible: None,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert!(json.get("repoId").is_some());
        assert!(json.get("repoName").is_some());
        assert!(json.get("branchName").is_some());
        assert!(json.get("baseBranch").is_none());
        assert!(json.get("trackingMode").is_some());
        assert!(json.get("headCommit").is_some());
        assert!(json.get("materializationStatus").is_some());
        assert!(json.get("hasDraft").is_some());
        assert!(json.get("dirtyFileCount").is_some());
        // Nullable divergence contract (ontology D50): a known value
        // serializes as a number, and an unknown value serializes as an
        // EXPLICIT JSON null — the key is never omitted, so clients can
        // distinguish "unknown" from "field absent".
        assert_eq!(json["divergence"], serde_json::json!(3));
        assert!(json.get("behindBase").is_some());
        assert!(json["behindBase"].is_null());
    }

    #[test]
    fn test_workspace_stats_serialization() {
        let stats = WorkspaceStats {
            total_cost_usd: 42.5,
            tasks_completed: 10,
            total_tasks: 20,
            concurrent_tasks: 3,
            tasks_stopped: 2,
            by_task: vec![
                TaskCostBreakdown {
                    task_id: Uuid::nil(),
                    task_name: "sync main".to_string(),
                    task_type: "sync".to_string(),
                    cost_usd: 30.0,
                },
                TaskCostBreakdown {
                    task_id: Uuid::nil(),
                    task_name: "chat".to_string(),
                    task_type: "chat".to_string(),
                    cost_usd: 12.5,
                },
            ],
            branches: vec![],
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert!(json.get("totalCostUsd").is_some());
        assert!(json.get("tasksCompleted").is_some());
        assert!(json.get("totalTasks").is_some());
        assert!(json.get("concurrentTasks").is_some());
        assert!(json.get("tasksStopped").is_some());

        // The per-task breakdown serializes as `byTask` with camelCase entry
        // fields, and its costs sum to the headline `totalCostUsd`.
        let by_task = json.get("byTask").expect("byTask present");
        let entries = by_task.as_array().expect("byTask is an array");
        assert_eq!(entries.len(), 2);
        let first = &entries[0];
        assert!(first.get("taskId").is_some());
        assert!(first.get("taskName").is_some());
        assert!(first.get("taskType").is_some());
        assert!(first.get("costUsd").is_some());
        let sum: f64 = entries
            .iter()
            .map(|e| e.get("costUsd").unwrap().as_f64().unwrap())
            .sum();
        assert_eq!(sum, stats.total_cost_usd);
    }

    #[test]
    fn test_sync_result_serialization() {
        let result = SyncResult {
            branches_updated: 2,
            branches_skipped: 1,
            details: vec![],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("branchesUpdated").is_some());
        assert!(json.get("branchesSkipped").is_some());
    }

    #[test]
    fn test_merge_result_serialization() {
        let result = MergeResult {
            branches_merged: 3,
            branches_failed: 0,
            details: vec![],
            authority_advance: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("branchesMerged").is_some());
        assert!(json.get("branchesFailed").is_some());
    }

    #[test]
    fn test_branch_divergence_serialization() {
        let div = BranchDivergence {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            repo_name: "docs".to_string(),
            branch_name: "feat/test".to_string(),
            ahead_of_base: Some(5),
            behind_base: None,
        };
        let json = serde_json::to_value(&div).unwrap();
        assert!(json.get("repoId").is_some());
        assert!(json.get("repoName").is_some());
        assert!(json.get("branchName").is_some());
        assert!(json.get("aheadOfBase").is_some());
        assert!(json.get("behindBase").is_some());
    }

    #[test]
    fn test_workspace_branch_detail_serialization() {
        let detail = WorkspaceBranchDetail {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            repo_name: "docs".to_string(),
            branch_name: "feat/test".to_string(),
            head_commit: "abc123".to_string(),
            tracking_mode: "pinned".to_string(),
            materialization_status: "materialized".to_string(),
            has_draft: false,
            dirty_file_count: 0,
            dirty_files: vec!["file1.rs".to_string()],
            remote_branch: None,
            local_branch_name: None,
            co_tracking_workspaces: None,
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("workspaceId").is_some());
        assert!(json.get("repoId").is_some());
        assert!(json.get("repoName").is_some());
        assert!(json.get("branchName").is_some());
        assert!(json.get("baseBranch").is_none());
        assert!(json.get("headCommit").is_some());
        assert!(json.get("trackingMode").is_some());
        assert!(json.get("hasDraft").is_some());
        assert!(json.get("dirtyFileCount").is_some());
        assert!(json.get("dirtyFiles").is_some());
        // Remote-tracking fields are absent for a plain `pinned` detail
        // because `Option::is_none` is wired with `skip_serializing_if`.
        assert!(json.get("remoteBranch").is_none());
        assert!(json.get("localBranchName").is_none());
        assert!(json.get("coTrackingWorkspaces").is_none());
    }

    #[test]
    fn test_workspace_branch_detail_serialization_with_remote_tracking() {
        // Mirror the `bootstrap_remote_tracking_branch_repo` return shape:
        // the per-row response carries the new optional fields when the
        // workspace branch was created as `remote_tracking`.
        let detail = WorkspaceBranchDetail {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            repo_name: "backend".to_string(),
            branch_name: "feat/login".to_string(),
            head_commit: "abc123".to_string(),
            tracking_mode: "remote_tracking".to_string(),
            materialization_status: "materialized".to_string(),
            has_draft: false,
            dirty_file_count: 0,
            dirty_files: vec![],
            remote_branch: Some("feat/login".to_string()),
            local_branch_name: Some("feat/login".to_string()),
            co_tracking_workspaces: Some(vec![]),
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["trackingMode"], "remote_tracking");
        assert_eq!(json["remoteBranch"], "feat/login");
        assert_eq!(json["localBranchName"], "feat/login");
        // Empty co-tracking peer list still serializes (Some(vec![]) — present but empty).
        assert!(json["coTrackingWorkspaces"].is_array());
        assert_eq!(json["coTrackingWorkspaces"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_workspace_manifest_repo_serialization_with_remote_identity() {
        // D15: a remote-tracking repo's manifest entry carries the upstream
        // identity, while `branch` stays the canonical local name. Manifest
        // fields are snake_case (on-disk YAML schema), not camelCase.
        let repo = WorkspaceManifestRepo {
            name: "backend".to_string(),
            url: "https://api.example/git/00000000-0000-0000-0000-000000000000".to_string(),
            repo_id: Uuid::new_v4(),
            branch: "feat/login".to_string(),
            path: "repos/backend".to_string(),
            commit: Some("abc123".to_string()),
            remote_branch: Some("main".to_string()),
            remote_repo_name: Some("octocat/hello-world".to_string()),
        };
        let json = serde_json::to_value(&repo).unwrap();
        // `branch` is the canonical local name, never the remote name.
        assert_eq!(json["branch"], "feat/login");
        assert_eq!(json["remote_branch"], "main");
        assert_eq!(json["remote_repo_name"], "octocat/hello-world");
    }

    #[test]
    fn test_workspace_manifest_repo_omits_remote_identity_when_absent() {
        // D15 backward-compat: non-remote-tracking repos omit the new fields
        // entirely, so existing manifest payloads serialize byte-identically.
        let repo = WorkspaceManifestRepo {
            name: "backend".to_string(),
            url: "https://api.example/git/00000000-0000-0000-0000-000000000000".to_string(),
            repo_id: Uuid::new_v4(),
            branch: "main".to_string(),
            path: "repos/backend".to_string(),
            commit: Some("abc123".to_string()),
            remote_branch: None,
            remote_repo_name: None,
        };
        let json = serde_json::to_value(&repo).unwrap();
        let obj = json.as_object().unwrap();
        assert!(
            !obj.contains_key("remote_branch"),
            "remote_branch must be omitted when None"
        );
        assert!(
            !obj.contains_key("remote_repo_name"),
            "remote_repo_name must be omitted when None"
        );
    }

    #[test]
    fn test_branch_sync_detail_serialization() {
        let detail = BranchSyncDetail {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            repo_name: "docs".to_string(),
            branch_name: "feat/test".to_string(),
            status: "updated".to_string(),
            old_commit: Some("abc".to_string()),
            new_commit: Some("def".to_string()),
            held_by: None,
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("repoId").is_some());
        assert!(json.get("repoName").is_some());
        assert!(json.get("branchName").is_some());
        assert!(json.get("oldCommit").is_some());
        assert!(json.get("newCommit").is_some());
    }

    #[test]
    fn test_branch_merge_detail_serialization() {
        let detail = BranchMergeDetail {
            workspace_id: Uuid::nil(),
            repo_id: Uuid::nil(),
            repo_name: "docs".to_string(),
            branch_name: "feat/test".to_string(),
            status: "merged".to_string(),
            merge_commit: Some("abc123".to_string()),
            error: None,
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert!(json.get("repoId").is_some());
        assert!(json.get("repoName").is_some());
        assert!(json.get("branchName").is_some());
        assert!(json.get("mergeCommit").is_some());
    }

    #[test]
    fn test_is_reserved_workspace_name() {
        use crate::services::workspace_names::is_reserved_workspace_name;
        // Reserved org-level route segments (case-insensitive, trimmed).
        assert!(is_reserved_workspace_name("settings"));
        assert!(is_reserved_workspace_name("Settings"));
        assert!(is_reserved_workspace_name("  SETTINGS  "));
        assert!(is_reserved_workspace_name("members"));
        assert!(is_reserved_workspace_name("workers"));
        assert!(is_reserved_workspace_name("home"));
        assert!(is_reserved_workspace_name("Home"));
        assert!(is_reserved_workspace_name("  HOME  "));
        assert!(is_reserved_workspace_name("fork"));
        assert!(is_reserved_workspace_name("Fork"));
        // Ordinary names are allowed, including superstrings of reserved words.
        assert!(!is_reserved_workspace_name("base"));
        assert!(!is_reserved_workspace_name("my-settings"));
        assert!(!is_reserved_workspace_name("settings-v2"));
        assert!(!is_reserved_workspace_name("home-page"));
        assert!(!is_reserved_workspace_name("forks"));
        assert!(!is_reserved_workspace_name(""));
    }

    /// Service-level regression for the divergence-boundary commit list.
    ///
    /// Unlike the g5-common `list_commits_since_base_branch` helper test, this
    /// constructs a real `WorkspaceService` and invokes `list_branch_commits`,
    /// so reverting the SERVICE wiring — e.g. back to the old
    /// `resolve_branch_root_commit` + `list_commits` selection — fails HERE.
    /// No database is touched: `list_branch_commits` reads only `repo_storage`
    /// and the branch, so lazy pools and a stub authz suffice.
    #[tokio::test]
    async fn list_branch_commits_returns_only_workspace_commits_since_divergence() {
        use crate::services::git::repo_store::TestOrgScope;
        use g5_scm::auth::GitAuthor;
        use g5_scm::GitRepo;

        let base_dir = tempfile::tempdir().unwrap();
        let repo_storage = TenantRepoStorage::local_for_test(base_dir.path().to_path_buf());
        let org_id = uuid::Uuid::new_v4();

        // Seed a repo at the branch's storage path with a linear history
        //   base_root -> base_mid -> base_tip -> workspace_one -> head
        // plus a `base` branch ref at base_tip (the divergence boundary) and a
        // `base_at_head` ref at head (the not-yet-diverged case).
        let repo_id = uuid::Uuid::new_v4();
        let repo_path = repo_storage.test_path(org_id, repo_id);
        std::fs::create_dir_all(&repo_path).unwrap();
        git2::Repository::init(&repo_path).expect("init test repo");
        let repo = GitRepo::open(&repo_path).unwrap();
        let author = GitAuthor::from_profile(Some("Test Author"), Some("test@example.com"));
        let commit_file = |content: &str, msg: &str| -> String {
            std::fs::write(repo_path.join("f.txt"), content).unwrap();
            repo.add_all().unwrap();
            repo.commit(msg, &author).unwrap()
        };
        commit_file("0\n", "base root");
        let base_mid = commit_file("1\n", "base mid");
        let base_tip = commit_file("2\n", "base tip");
        let raw_repo = git2::Repository::open(&repo_path).expect("open raw test repo");
        raw_repo
            .reference(
                "refs/heads/base",
                git2::Oid::from_str(&base_tip).expect("base tip oid"),
                false,
                "seed test branch",
            )
            .expect("seed base branch");
        commit_file("3\n", "workspace one");
        let head = commit_file("4\n", "workspace two");
        raw_repo
            .reference(
                "refs/heads/base_at_head",
                git2::Oid::from_str(&head).expect("head oid"),
                false,
                "seed test branch",
            )
            .expect("seed at-head branch");

        let service = WorkspaceService::new(
            repo_storage.clone(),
            sqlx::PgPool::connect_lazy("postgres://invalid@localhost/nonexistent").unwrap(),
            sqlx::PgPool::connect_lazy("postgres://invalid@localhost/nonexistent").unwrap(),
            std::sync::Arc::new(crate::stubs::StubAuthz),
            Arc::new(crate::auth::RepositoryCredentials::new(
                sqlx::PgPool::connect_lazy("postgres://invalid@localhost/nonexistent").unwrap(),
                sqlx::PgPool::connect_lazy("postgres://invalid@localhost/nonexistent").unwrap(),
                Arc::new(crate::stubs::StubAuthz),
                None,
                None,
            )),
        );

        let branch = crate::models::workspace::WorkspaceRepositoryState {
            workspace_id: uuid::Uuid::new_v4(),
            org_id,
            repo_id,
            branch_name: "workspace".to_string(),
            head_commit: head.clone(),
            tracking_mode: "internal".to_string(),
            materialization_status: "materialized".to_string(),
        };

        // Diverged at base_tip: only the workspace's own commits, newest first.
        // Reverting the service to resolve the base ROOT would leak "base mid"
        // and "base tip" into this list.
        let head_view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org_id),
            org_id,
            &HistoricalGitVersion {
                repo_id,
                head_commit: head.clone(),
                reason: HistoricalReadReason::CommitHistory,
            },
        )
        .unwrap();
        let base_view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org_id),
            org_id,
            &HistoricalGitVersion {
                repo_id,
                head_commit: base_tip.clone(),
                reason: HistoricalReadReason::BaseBinding,
            },
        )
        .unwrap();
        let commits = service
            .list_branch_commits_between_views(&head_view, &base_view, 50)
            .await
            .unwrap();
        let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(
            summaries,
            vec!["workspace two", "workspace one"],
            "the service returns only the workspace's own post-divergence commits"
        );

        // Not-yet-diverged: the base tip equals head -> empty list.
        let at_head = crate::models::workspace::WorkspaceRepositoryState { ..branch.clone() };
        let at_head_view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org_id),
            org_id,
            &HistoricalGitVersion {
                repo_id,
                head_commit: at_head.head_commit.clone(),
                reason: HistoricalReadReason::CommitHistory,
            },
        )
        .unwrap();
        let none = service
            .list_branch_commits_between_views(&at_head_view, &at_head_view, 50)
            .await
            .unwrap();
        assert!(
            none.is_empty(),
            "a branch whose base tip equals its head returns no new commits"
        );

        // Head at/behind an advanced base tip: a materialized branch pinned at
        // base_mid whose `base_branch` still resolves to base_tip has no
        // post-divergence commits, so the service returns an empty list (the
        // base advanced past this workspace's head).
        let behind = crate::models::workspace::WorkspaceRepositoryState {
            head_commit: base_mid.clone(),
            ..branch.clone()
        };
        let behind_view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org_id),
            org_id,
            &HistoricalGitVersion {
                repo_id,
                head_commit: behind.head_commit.clone(),
                reason: HistoricalReadReason::CommitHistory,
            },
        )
        .unwrap();
        let none_behind = service
            .list_branch_commits_between_views(&behind_view, &base_view, 50)
            .await
            .unwrap();
        assert!(
            none_behind.is_empty(),
            "a head at or behind an advanced base tip returns no new commits"
        );

        // A non-materialized branch is rejected before any walk (service guard).
        let unmaterialized = crate::models::workspace::WorkspaceRepositoryState {
            materialization_status: "pending".to_string(),
            ..branch.clone()
        };
        assert!(
            WorkspaceService::ensure_branch_commit_listing_materialized(&unmaterialized).is_err(),
            "a non-materialized branch is a validation error"
        );
    }
}

/// One-shot rendezvous at the real operation boundaries; absent in production.
#[cfg(feature = "test-helpers")]
pub mod operation_test_barrier {
    use std::{
        collections::HashMap,
        sync::{Arc, LazyLock, Mutex},
    };
    use tokio::sync::Semaphore;
    use uuid::Uuid;

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Stage {
        PolicyCommit,
        PolicyTimeout,
        DeleteAdmitted,
    }
    struct Gate {
        reached: Semaphore,
        release: Semaphore,
        finished: Semaphore,
    }
    pub(super) struct Completion(Arc<Gate>);
    impl Drop for Completion {
        fn drop(&mut self) {
            self.0.finished.add_permits(1);
        }
    }
    type Gates = HashMap<(Uuid, Stage), Arc<Gate>>;
    static GATES: LazyLock<Mutex<Gates>> = LazyLock::new(Default::default);
    pub struct Barrier {
        key: (Uuid, Stage),
        gate: Arc<Gate>,
    }
    pub fn install(id: Uuid, stage: Stage) -> Barrier {
        let gate = Arc::new(Gate {
            reached: Semaphore::new(0),
            release: Semaphore::new(0),
            finished: Semaphore::new(0),
        });
        assert!(GATES
            .lock()
            .unwrap()
            .insert((id, stage), gate.clone())
            .is_none());
        Barrier {
            key: (id, stage),
            gate,
        }
    }
    impl Barrier {
        pub async fn reached(&self) {
            tokio::time::timeout(
                std::time::Duration::from_secs(20),
                self.gate.reached.acquire(),
            )
            .await
            .expect("operation reached boundary")
            .unwrap()
            .forget();
        }
        pub fn release(&self) {
            self.gate.release.add_permits(1);
        }
        pub async fn finished(&self) {
            tokio::time::timeout(
                std::time::Duration::from_secs(20),
                self.gate.finished.acquire(),
            )
            .await
            .expect("operation finished")
            .unwrap()
            .forget();
        }
    }
    impl Drop for Barrier {
        fn drop(&mut self) {
            GATES.lock().unwrap().remove(&self.key);
            self.release();
        }
    }
    pub(super) async fn wait(id: Uuid, stage: Stage) -> Option<Completion> {
        let gate = GATES.lock().unwrap().remove(&(id, stage));
        if let Some(gate) = gate {
            gate.reached.add_permits(1);
            tokio::time::timeout(std::time::Duration::from_secs(30), gate.release.acquire())
                .await
                .expect("operation test barrier released")
                .unwrap()
                .forget();
            return Some(Completion(gate));
        }
        None
    }
}

#[cfg(feature = "test-helpers")]
impl WorkspaceService {
    /// Persist admission without spawning the driver, as when a process exits
    /// immediately after committing it. Tests resume the real settlement paths.
    pub async fn admit_policy_drain_for_tests(
        &self,
        command: MutationPolicyTransitionCommand,
        transition_id: Uuid,
    ) -> Result<bool, G5Error> {
        self.enter_freezing(
            command.org_id,
            command.workspace_id,
            command.actor,
            transition_id,
        )
        .await
    }

    pub async fn settle_policy_drain_for_tests(
        &self,
        event_bus: &EventBus,
        command: MutationPolicyTransitionCommand,
        transition_id: Uuid,
        timed_out: bool,
    ) -> Result<(), G5Error> {
        if timed_out {
            Self::revert_freezing(
                &self.authz,
                &self.tenant_pool,
                event_bus,
                command.org_id,
                command.workspace_id,
                command.actor,
                transition_id,
            )
            .await
        } else {
            Self::commit_read_only(
                ReadOnlyCommitContext {
                    authz: &self.authz,
                    tenant_pool: &self.tenant_pool,
                    event_bus,
                    command,
                },
                Some(transition_id),
            )
            .await
            .map(|_| ())
        }
    }
}
