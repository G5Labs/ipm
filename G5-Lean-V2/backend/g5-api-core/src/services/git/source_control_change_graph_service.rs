//! Deterministic, read-only projection of workspace changes for Diff Graph, the
//! Review changes dialog and the rebase/merge preview (`g5.diff-graph.story`).
//! Files are nodes; ontology statement changes come from the shared statement
//! rule and authored links become typed relationships. No intent or grounding
//! ledgers are read.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

use crate::services::git::repo_store::{RepoKey, ScopedRepoStorage, TenantRepoStorage};
use g5_authz::ScopedDb;
use g5_contracts::types::grounding::FileType;
use g5_contracts::types::{
    current_ontology_anchor_counts, current_ontology_records, HistoricalGitVersion,
    HistoricalReadReason,
};
use g5_scm::{
    unified_diff_buffers, unified_diff_buffers_with_context, BlameAttribution, DiffContext, GitRepo,
};
use g5_semantic::ontology_comparison as ontology;
use g5_semantic::parsing::MarkdownAnchorExtractor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::errors::{AppError, G5Error};
use crate::services::unified_diff::parse_unified_diff as parse_task_unified_diff;
use crate::services::workspace_reader::{
    read_branch_head_oids, CommittedRepoReadView, HistoricalBlameRequest,
};
use crate::services::workspace_repositories::{
    load_workspace_repositories, readable_parent_workspace,
};
use g5_api_foundation::contracts::tasks::{DiffHunk, DiffLine};

const MAX_RELATIONSHIP_CONTEXT_NODES: usize = 256;
const MAX_GRAPH_EDGES: usize = 1_024;
const MAX_EVIDENCE_ROWS: usize = 4_096;
/// Distinct unchanged files a graph's links may look up; beyond this, links are
/// skipped with a visible budget diagnostic rather than read without bound.
const MAX_LINK_TARGET_FILES: usize = 1_024;
const MAX_DETAIL_DIFF_LINES: usize = 3_000;
const MAX_DETAIL_DIFF_BYTES: usize = 512 * 1024;
const MAX_FILE_DIFF_INPUT_BYTES: usize = 10 * 1024 * 1024;
const MAX_WORKSPACE_CHANGED_ROOTS: usize = 500;
const MAX_WORKSPACE_COMPARISON_INPUT_BYTES: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone)]
struct WorkspaceFileSnapshot {
    repo_id: Uuid,
    base_commit: Option<String>,
    file: String,
    base_content: Option<Vec<u8>>,
    content: Option<Vec<u8>>,
    additions: i64,
    deletions: i64,
    hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGraphComparison {
    #[default]
    Workspace,
    /// Not offered until it reports uncommitted changes (`g5.diff-graph.modes`):
    /// refused as unavailable rather than answered with an empty graph.
    Uncommitted,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGraphBaseKind {
    MergeBase,
    AcceptedHead,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphRange {
    pub base_kind: ChangeGraphBaseKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_workspace_name: Option<String>,
    pub repository_count: usize,
    /// Per repository: whether saved uncommitted edits are included, and why not.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub live_content: Vec<RepositoryLiveContent>,
}

/// Whether one repository's current content includes saved uncommitted edits.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryLiveContent {
    pub repo_id: Uuid,
    pub repo_name: String,
    pub includes_uncommitted: bool,
    /// Why only committed content is shown: `not_owner`, `vm_not_running`,
    /// `unpublished_commits`, `status_unavailable`, `report_unavailable` or
    /// `workspace_synchronizing`. Ontology files are complete whenever
    /// `ontology_live` is true, even with unpublished commits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub ontology_live: bool,
}

/// Saved workspace content the caller captured through the actor-aware live
/// readers, layered over committed heads for All changes (`g5.diff-graph.modes`).
#[derive(Debug, Clone, Default)]
pub struct WorkspaceLiveContent {
    pub repositories: Vec<RepositoryLiveOverlay>,
    /// Why repositories without an overlay show committed content only.
    pub committed_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RepositoryLiveOverlay {
    pub repo_id: Uuid,
    /// The published branch head the overlay was captured against. If the
    /// comparison captures a different head, the overlay is not used.
    pub expected_head: String,
    /// Saved content per path; `None` is a file deleted in the working tree.
    pub files: BTreeMap<String, Option<Vec<u8>>>,
    /// Every uncommitted change is in `files`; otherwise only ontology files are.
    pub includes_uncommitted: bool,
    pub ontology_live: bool,
    pub reason: Option<String>,
}

/// An added or edited link whose target exists on neither side
/// (`g5.diff-graph.missing-targets`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissingLinkTarget {
    pub statement_id: String,
    pub href: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOrigin {
    Committed,
    Uncommitted,
    Both,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphResponse {
    pub revision: String,
    pub comparison: ChangeGraphComparison,
    pub range: ChangeGraphRange,
    pub nodes: Vec<ChangeGraphNode>,
    pub edges: Vec<ChangeGraphEdge>,
    pub diagnostics: Vec<ChangeGraphDiagnostic>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphNode {
    pub id: String,
    pub column: ChangeGraphColumn,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_group: Option<IntentGroup>,
    /// Folder grouping inside the single Ontology column; none for top-level pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_group: Option<String>,
    pub title: String,
    pub artifact_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub change_state: ChangeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_origin: Option<ChangeOrigin>,
    /// Whether this node has at least one human-tagged changed record. Computed
    /// for every node regardless of the Human-intent lens.
    pub has_human_blocks: bool,
    pub connection_state: ConnectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletions: Option<i64>,
    pub changed_records: Vec<ChangedRecord>,
    /// A file version the statement rule refuses: shown, but no statement is
    /// classified (`g5.diff-graph.ontology-nodes`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_warning: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_targets: Vec<MissingLinkTarget>,
    pub route: ChangeGraphRoute,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGraphColumn {
    Intents,
    Ontology,
    SourceCode,
}

impl ChangeGraphColumn {
    fn rank(self) -> u8 {
        match self {
            Self::Intents => 0,
            Self::Ontology => 1,
            Self::SourceCode => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentGroup {
    Conversations,
    Documents,
    Other,
}

fn intent_group_rank(group: Option<IntentGroup>) -> u8 {
    match group {
        Some(IntentGroup::Documents) => 0,
        Some(IntentGroup::Conversations) => 1,
        Some(IntentGroup::Other) => 2,
        None => 0,
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeState {
    Added,
    Modified,
    Deleted,
    Unchanged,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connected,
    Bridge,
    Unlinked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedRecord {
    pub id: String,
    pub label: String,
    pub change_state: ChangeState,
    /// Statement Type (`I`, `D` or `Q`) of the version shown for the change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The shared Human rule (`g5.documents.ontology.human-statements`) on the
    /// version shown for the change.
    pub is_human: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphRoute {
    pub repo_id: Uuid,
    pub workspace_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub file_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub evidence: Vec<ChangeGraphEvidence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphEvidence {
    pub id: String,
    pub source_file_type: String,
    pub source_file: String,
    pub source_id: String,
    pub target_file_type: String,
    pub target_file: String,
    pub target_id: String,
    pub relationship: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<u8>,
    pub change_state: ChangeState,
    /// The target statement or file was deleted in this comparison.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub target_deleted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphDiagnostic {
    pub code: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphNodeDetailsResponse {
    pub revision: String,
    pub comparison: ChangeGraphComparison,
    pub node_id: String,
    #[serde(flatten)]
    pub detail: ChangeGraphNodeDetail,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphWorkspace {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebaseChangeGraphConflictKind {
    SameNode,
    SameBlock,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphConflict {
    pub id: String,
    pub artifact_key: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: RebaseChangeGraphConflictKind,
    pub overlapping_block_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlap_block_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphResponse {
    pub revision: String,
    pub source_workspace: RebaseChangeGraphWorkspace,
    pub target_workspace: RebaseChangeGraphWorkspace,
    pub common_base_repository_count: usize,
    pub source: ChangeGraphResponse,
    pub target: ChangeGraphResponse,
    pub conflicts: Vec<RebaseChangeGraphConflict>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphSideDetails {
    pub node: ChangeGraphNode,
    pub changed_line_count: i64,
    pub changed_block_count: usize,
    pub detail: ChangeGraphNodeDetail,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseChangeGraphNodeDetailsResponse {
    pub revision: String,
    pub artifact_key: String,
    pub title: String,
    pub artifact: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<RebaseChangeGraphSideDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<RebaseChangeGraphSideDetails>,
    pub overlap_line_count: i64,
    pub overlap_block_ids: Vec<String>,
    pub overlap_block_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ChangeGraphNodeDetail {
    FileDiff {
        diff: ChangeGraphDiffEvidence,
    },
    SemanticDiff {
        records: Vec<SemanticRecordDiff>,
        #[serde(skip_serializing_if = "Option::is_none")]
        other_file_changes: Option<ChangeGraphDiffEvidence>,
        truncated: bool,
    },
    UnchangedContext,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGraphDiffAvailability {
    Available,
    Binary,
    Oversized,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGraphDiffReason {
    BinaryContent,
    InputLimitExceeded,
    RetainedEvidenceMissing,
    OutputLimitReached,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeGraphDiffEvidence {
    pub availability: ChangeGraphDiffAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ChangeGraphDiffReason>,
    pub additions: i64,
    pub deletions: i64,
    pub hunks: Vec<DiffHunk>,
    pub returned_line_count: usize,
    pub omitted_line_count: usize,
    pub truncated: bool,
    pub partial_final_hunk: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRecordDiff {
    pub id: String,
    pub record_kind: String,
    pub title: String,
    pub change_state: ChangeState,
    /// The shared Human rule on the version shown for the change.
    pub is_human: bool,
    /// Under the Human intent lens, the git-blame author of a human statement's
    /// baseline lines. Left unset otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Provenance source of `author`: `git_blame`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_source: Option<String>,
    /// Under the Human intent lens, the git-blame commit time of a human
    /// statement's baseline lines. Left unset otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Provenance source of `timestamp`: `git_blame`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_source: Option<String>,
    pub diff: ChangeGraphDiffEvidence,
}

/// Blame-fallback targets for one lensed node-details payload: parallel
/// record indices and per-statement line-range requests against one file.
struct HumanBlameTargets {
    indices: Vec<usize>,
    requests: Vec<HistoricalBlameRequest>,
}

/// Select the records eligible for the git-blame fallback: human records with
/// an unresolved author or timestamp whose statement exists in the base
/// version. Added statements (absent from the base) stay `unknown`.
fn collect_human_blame_targets(
    records: &[SemanticRecordDiff],
    base_lines: &HashMap<String, (usize, usize)>,
    path: &str,
) -> HumanBlameTargets {
    let mut indices = Vec::new();
    let mut requests = Vec::new();
    for (index, record) in records.iter().enumerate() {
        if !record.is_human || (record.author.is_some() && record.timestamp.is_some()) {
            continue;
        }
        let Some((start_line, end_line)) = base_lines.get(&record.id).copied() else {
            continue;
        };
        indices.push(index);
        requests.push(HistoricalBlameRequest {
            path: path.to_string(),
            start_line,
            end_line,
        });
    }
    HumanBlameTargets { indices, requests }
}

/// Fill blame attributions into the targeted records. Filled values carry
/// source `git_blame`; a per-range failure or empty attribution leaves that
/// record's `unknown` state untouched.
fn apply_blame_results(
    records: &mut [SemanticRecordDiff],
    target_indices: &[usize],
    results: Vec<Result<Option<BlameAttribution>, G5Error>>,
) {
    for (index, result) in target_indices.iter().zip(results) {
        let Ok(Some(attribution)) = result else {
            continue;
        };
        let Some(record) = records.get_mut(*index) else {
            continue;
        };
        if record.author.is_none() && !attribution.author.trim().is_empty() {
            record.author = Some(attribution.author.clone());
            record.author_source = Some("git_blame".to_string());
        }
        if record.timestamp.is_none() && !attribution.committed_at.trim().is_empty() {
            record.timestamp = Some(attribution.committed_at);
            record.timestamp_source = Some("git_blame".to_string());
        }
    }
}

#[derive(Debug, Clone)]
struct SemanticBlock {
    id: String,
    title: String,
    content: String,
    byte_range: std::ops::Range<usize>,
    source_order: usize,
}

#[derive(Debug, Clone)]
struct DetailDiffBudget {
    remaining_lines: usize,
    remaining_bytes: usize,
}

impl Default for DetailDiffBudget {
    fn default() -> Self {
        Self {
            remaining_lines: MAX_DETAIL_DIFF_LINES,
            remaining_bytes: MAX_DETAIL_DIFF_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
struct RepoContext {
    workspace_id: Uuid,
    repo_id: Uuid,
    repo_name: String,
    repo_slug: String,
    commit: String,
    merge_base_commit: Option<String>,
    parent_head: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CommonBaseLineAnchor {
    ExistingLine(i64),
    Insertion { before_line: i64, ordinal: usize },
}

enum WorkspaceFinalContentSource {
    Commit { present: bool },
    Overlay(Option<Vec<u8>>),
}

struct PreparedWorkspaceSnapshot {
    repo_id: Uuid,
    base_commit: String,
    path: String,
    final_source: WorkspaceFinalContentSource,
    origin: ChangeOrigin,
}

#[derive(Clone)]
struct WorkspaceGraphContext {
    /// The store as the loading transaction's scope reaches it, and the
    /// workspace's organization (`g5.repo.storage-tenancy`).
    storage: ScopedRepoStorage,
    org_id: Uuid,
    workspace_name: String,
    is_base: bool,
    parent_workspace_id: Option<Uuid>,
    parent_workspace_name: Option<String>,
    docs_repo_id: Option<Uuid>,
    repos: Vec<RepoContext>,
}

struct RebaseChangeGraphProjection {
    response: RebaseChangeGraphResponse,
    source_snapshots: Vec<WorkspaceFileSnapshot>,
    target_snapshots: Vec<WorkspaceFileSnapshot>,
}

struct ProjectionInput {
    storage: ScopedRepoStorage,
    org_id: Uuid,
    comparison: ChangeGraphComparison,
    range: ChangeGraphRange,
    docs_repo_id: Option<Uuid>,
    repo_contexts: Vec<RepoContext>,
    snapshots: Vec<WorkspaceFileSnapshot>,
    origins: HashMap<(Uuid, String), ChangeOrigin>,
    revision_scope: Vec<u8>,
    workspace_input_bytes: usize,
}

struct WorkspaceSnapshotBuild {
    snapshots: Vec<WorkspaceFileSnapshot>,
    origins: HashMap<(Uuid, String), ChangeOrigin>,
    revision_scope: Vec<u8>,
    workspace_input_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ArtifactKey {
    repo_id: Uuid,
    column: u8,
    identity: String,
}

impl ArtifactKey {
    fn stable(&self) -> String {
        format!("{}|{}|{}", self.repo_id, self.column, self.identity)
    }
}

#[derive(Debug, Clone)]
struct ArtifactProjection {
    key: ArtifactKey,
    column: ChangeGraphColumn,
    intent_group: Option<IntentGroup>,
    ontology_group: Option<String>,
    title: String,
    sort_key: String,
    artifact_type: String,
    path: Option<String>,
    change_state: ChangeState,
    change_origin: Option<ChangeOrigin>,
    additions: Option<i64>,
    deletions: Option<i64>,
    changed_records: Vec<ChangedRecord>,
    comparison_warning: Option<String>,
    missing_targets: Vec<MissingLinkTarget>,
    route: ChangeGraphRoute,
    changed: bool,
}

/// One authored link between two file artifacts, before selection and budgets.
#[derive(Debug, Clone)]
struct LinkEdge {
    source: ArtifactKey,
    target: ArtifactKey,
    source_file: String,
    source_id: String,
    target_file: String,
    target_id: String,
    relationship: &'static str,
    change_state: ChangeState,
    target_deleted: bool,
}

struct GraphSelectionContext {
    diagnostic_counts: BTreeMap<String, usize>,
    revision_hash: String,
    comparison: ChangeGraphComparison,
    range: ChangeGraphRange,
    human_intent: bool,
}

/// Complete, bounded historical input for the statement comparison editor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyComparisonBaseline {
    pub revision: String,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub path: String,
    pub baseline_commit: String,
    pub source_commit: String,
    pub parent_commit: String,
    pub content: Option<String>,
}

/// Immutable per-member authority shared by the workspace ontology overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyComparisonRepository {
    #[serde(skip)]
    pub key: RepoKey,
    pub repo_id: Uuid,
    pub repo_name: String,
    pub source_commit: String,
    pub parent_commit: String,
    pub baseline_commit: String,
}

pub struct SourceControlChangeGraphService {
    repo_storage: TenantRepoStorage,
}

impl SourceControlChangeGraphService {
    pub fn new(repo_storage: TenantRepoStorage) -> Self {
        Self { repo_storage }
    }

    pub async fn ontology_comparison_repositories(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
    ) -> Result<Vec<OntologyComparisonRepository>, AppError> {
        let mut context =
            load_workspace_graph_context(db, &self.repo_storage, workspace_id).await?;
        if context.is_base {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "base_workspace".into(),
                missing_repo_count: 0,
            }));
        }
        let actor_id = db.actor_id().into_uuid();
        let parent_id = readable_parent_workspace(db.conn(), workspace_id, actor_id).await?;
        let parent = load_workspace_graph_context(db, &self.repo_storage, parent_id).await?;
        select_comparison_bases(&mut context, &parent).await?;
        let mut repos = context
            .repos
            .into_iter()
            .map(|repo| {
                Ok(OntologyComparisonRepository {
                    key: context
                        .storage
                        .key(context.org_id, repo.repo_id)
                        .map_err(AppError::from_error)?,
                    repo_id: repo.repo_id,
                    repo_name: repo.repo_name,
                    source_commit: repo.commit,
                    parent_commit: repo.parent_head.expect("selected parent"),
                    baseline_commit: repo.merge_base_commit.expect("selected baseline"),
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        repos.sort_by_key(|repo| repo.repo_id);
        Ok(repos)
    }

    /// Reuse All changes authority and merge-base selection without projecting a graph.
    pub async fn ontology_baseline(
        &self,
        repositories: &[OntologyComparisonRepository],
        workspace_id: Uuid,
        repo_id: Uuid,
        path: &str,
    ) -> Result<OntologyComparisonBaseline, AppError> {
        if !g5_contracts::g5_paths::is_ontology_path(path) {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({"message": "Expected a normalized .g5/ontology/**/*.g5.md path"}),
            }));
        }
        let repo = repositories
            .iter()
            .find(|r| r.repo_id == repo_id)
            .ok_or_else(|| {
                AppError::from_error(G5Error::NotFound {
                    resource: "Workspace repository".into(),
                })
            })?;
        let baseline_commit = repo.baseline_commit.clone();
        let parent_commit = repo.parent_commit.clone();
        let view = CommittedRepoReadView::from_key(
            repo.key,
            &baseline_commit,
            HistoricalReadReason::Diff,
        )?;
        // Metadata lookup distinguishes an absent path from unavailable history, and
        // bounded read refuses an oversized blob before materializing it.
        let exists = view
            .file_metadata(&self.repo_storage, &[path.to_owned()])
            .await?
            .get(path)
            .is_some_and(Option::is_some);
        let content = if exists {
            let bytes = view
                .read_file_bounded(&self.repo_storage, path, MAX_FILE_DIFF_INPUT_BYTES)
                .await?;
            Some(String::from_utf8(bytes).map_err(|_| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({"message": "Ontology baseline is not UTF-8 text"}),
                })
            })?)
        } else {
            None
        };
        let revision = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(
                    workspace_id,
                    repo_id,
                    path,
                    &baseline_commit,
                    repositories
                        .iter()
                        .map(|r| (r.repo_id, &r.source_commit, &r.parent_commit))
                        .collect::<Vec<_>>(),
                ))
                .expect("serializable comparison identity")
            )
        );
        Ok(OntologyComparisonBaseline {
            revision,
            workspace_id,
            repo_id,
            path: path.to_owned(),
            baseline_commit,
            source_commit: repo.source_commit.clone(),
            parent_commit,
            content,
        })
    }

    pub async fn project(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        comparison: ChangeGraphComparison,
        human_intent: bool,
        live: &WorkspaceLiveContent,
    ) -> Result<ChangeGraphResponse, AppError> {
        Ok(self
            .project_with_snapshots(db, workspace_id, comparison, human_intent, live)
            .await?
            .0)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn node_details(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        node_id: &str,
        requested_revision: &str,
        comparison: ChangeGraphComparison,
        human_intent: bool,
        live: &WorkspaceLiveContent,
    ) -> Result<ChangeGraphNodeDetailsResponse, AppError> {
        let (response, snapshots) = self
            .project_with_snapshots(db, workspace_id, comparison, human_intent, live)
            .await?;
        if response.revision != requested_revision {
            return Err(AppError::from_error(G5Error::StaleGraphRevision {
                requested_revision: requested_revision.to_string(),
                current_revision: response.revision,
            }));
        }
        let node = response
            .nodes
            .iter()
            .find(|candidate| candidate.id == node_id)
            .ok_or_else(|| {
                AppError::from_error(G5Error::NotFound {
                    resource: format!("change graph node {node_id}"),
                })
            })?;
        let detail = build_node_detail(workspace_id, node, &snapshots, human_intent);
        let detail = if human_intent {
            let org_id =
                crate::services::workspace_reader::workspace_org(db.conn(), workspace_id).await?;
            let storage = self.repo_storage.scoped(&*db);
            self.apply_lensed_blame_fallback(&storage, org_id, node, &snapshots, detail)
                .await
        } else {
            detail
        };
        tracing::info!(
            workspace_id = %workspace_id,
            node_id,
            revision = %response.revision,
            variant = detail.variant_name(),
            truncated = detail.is_truncated(),
            "Projected Source Control change graph node details"
        );
        Ok(ChangeGraphNodeDetailsResponse {
            revision: response.revision,
            comparison,
            node_id: node_id.to_string(),
            detail,
        })
    }

    /// Author and time for the Human intent lens: each human statement present
    /// in the baseline is attributed by blaming its baseline line range at the
    /// diff-base commit, batching the node's ranges into one request. Added
    /// statements, unreadable bases and blame failures leave the records
    /// `unknown` and never fail the details request.
    async fn apply_lensed_blame_fallback(
        &self,
        storage: &ScopedRepoStorage,
        org_id: Uuid,
        node: &ChangeGraphNode,
        snapshots: &[WorkspaceFileSnapshot],
        detail: ChangeGraphNodeDetail,
    ) -> ChangeGraphNodeDetail {
        let (mut records, other_file_changes, truncated) = match detail {
            ChangeGraphNodeDetail::SemanticDiff {
                records,
                other_file_changes,
                truncated,
            } => (records, other_file_changes, truncated),
            other => return other,
        };
        let done = |records| ChangeGraphNodeDetail::SemanticDiff {
            records,
            other_file_changes,
            truncated,
        };
        let Some(snapshot) = node.route.file_path.as_deref().and_then(|path| {
            snapshots
                .iter()
                .find(|snapshot| snapshot.repo_id == node.route.repo_id && snapshot.file == path)
        }) else {
            return done(records);
        };
        let Some(base_commit) = snapshot.base_commit.as_deref() else {
            return done(records);
        };
        let Some(base_text) = snapshot
            .base_content
            .as_deref()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
        else {
            return done(records);
        };
        let base_lines: HashMap<String, (usize, usize)> = ontology::parse_version(base_text)
            .map(|statements| {
                statements
                    .into_iter()
                    .map(|statement| (statement.id, (statement.start_line, statement.end_line)))
                    .collect()
            })
            .unwrap_or_default();
        let targets = collect_human_blame_targets(&records, &base_lines, &snapshot.file);
        if targets.indices.is_empty() {
            return done(records);
        }
        let view = match CommittedRepoReadView::from_historical(
            storage,
            org_id,
            &HistoricalGitVersion {
                repo_id: snapshot.repo_id,
                head_commit: base_commit.to_string(),
                reason: HistoricalReadReason::BlameAttribution,
            },
        ) {
            Ok(view) => view,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    repo_id = %snapshot.repo_id,
                    "human-intent blame fallback: base version not readable; records stay unknown"
                );
                return done(records);
            }
        };
        match view
            .blame_ranges(&self.repo_storage, &targets.requests)
            .await
        {
            Ok(results) => apply_blame_results(&mut records, &targets.indices, results),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    repo_id = %snapshot.repo_id,
                    "human-intent blame fallback failed; records stay unknown"
                );
            }
        }
        done(records)
    }

    pub async fn project_rebase_merge(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<RebaseChangeGraphResponse, AppError> {
        Ok(self
            .project_rebase_merge_with_snapshots(db, source_workspace_id, target_workspace_id)
            .await?
            .response)
    }

    pub async fn rebase_merge_node_details(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
        artifact_key: &str,
        requested_revision: &str,
    ) -> Result<RebaseChangeGraphNodeDetailsResponse, AppError> {
        let projection = self
            .project_rebase_merge_with_snapshots(db, source_workspace_id, target_workspace_id)
            .await?;
        if projection.response.revision != requested_revision {
            return Err(AppError::from_error(G5Error::StaleGraphRevision {
                requested_revision: requested_revision.to_string(),
                current_revision: projection.response.revision,
            }));
        }

        let source_node = projection
            .response
            .source
            .nodes
            .iter()
            .find(|node| node.id == artifact_key)
            .cloned();
        let target_node = projection
            .response
            .target
            .nodes
            .iter()
            .find(|node| node.id == artifact_key)
            .cloned();
        let primary = source_node
            .as_ref()
            .or(target_node.as_ref())
            .ok_or_else(|| {
                AppError::from_error(G5Error::NotFound {
                    resource: format!("rebase change graph artifact {artifact_key}"),
                })
            })?;
        let title = primary.title.clone();
        let artifact = primary.artifact_type.clone();
        let path = primary.path.clone();
        let overlap_block_ids = projection
            .response
            .conflicts
            .iter()
            .find(|conflict| conflict.artifact_key == artifact_key)
            .map(|conflict| conflict.overlapping_block_ids.clone())
            .unwrap_or_default();

        let overlap_line_count = match (source_node.as_ref(), target_node.as_ref()) {
            (Some(source), Some(target)) => exact_overlap_line_count(
                source,
                target,
                &overlap_block_ids,
                &projection.source_snapshots,
                &projection.target_snapshots,
            )?,
            _ => 0,
        };

        let source = source_node.map(|node| {
            let detail = build_node_detail(
                source_workspace_id,
                &node,
                &projection.source_snapshots,
                false,
            );
            RebaseChangeGraphSideDetails {
                changed_line_count: node.additions.unwrap_or(0) + node.deletions.unwrap_or(0),
                changed_block_count: semantic_record_count(&detail, &node),
                node,
                detail,
            }
        });
        let target = target_node.map(|node| {
            let detail = build_node_detail(
                target_workspace_id,
                &node,
                &projection.target_snapshots,
                false,
            );
            RebaseChangeGraphSideDetails {
                changed_line_count: node.additions.unwrap_or(0) + node.deletions.unwrap_or(0),
                changed_block_count: semantic_record_count(&detail, &node),
                node,
                detail,
            }
        });

        Ok(RebaseChangeGraphNodeDetailsResponse {
            revision: projection.response.revision,
            artifact_key: artifact_key.to_string(),
            title,
            artifact,
            path,
            source,
            target,
            overlap_line_count,
            overlap_block_count: overlap_block_ids.len(),
            overlap_block_ids,
        })
    }

    async fn project_rebase_merge_with_snapshots(
        &self,
        db: &mut ScopedDb,
        source_workspace_id: Uuid,
        target_workspace_id: Uuid,
    ) -> Result<RebaseChangeGraphProjection, AppError> {
        let mut source_context =
            load_workspace_graph_context(db, &self.repo_storage, source_workspace_id).await?;
        if source_context.is_base {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "base_workspace".to_string(),
                missing_repo_count: 0,
            }));
        }
        if source_context.parent_workspace_id != Some(target_workspace_id) {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "target_not_parent".to_string(),
                missing_repo_count: 0,
            }));
        }
        let actor_id = db.actor_id().into_uuid();
        let parent_id = readable_parent_workspace(db.conn(), source_workspace_id, actor_id).await?;
        if parent_id != target_workspace_id {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "target_not_parent".to_string(),
                missing_repo_count: 0,
            }));
        }
        let target_context =
            load_workspace_graph_context(db, &self.repo_storage, target_workspace_id).await?;
        let source_workspace_name = source_context.workspace_name.clone();
        let target_workspace_name = target_context.workspace_name.clone();
        select_comparison_bases(&mut source_context, &target_context).await?;
        let source_bases = source_context
            .repos
            .iter()
            .map(|r| {
                (
                    r.repo_id,
                    r.merge_base_commit
                        .clone()
                        .expect("comparison base selected"),
                )
            })
            .collect::<HashMap<_, _>>();
        let source_committed_views = committed_views_for_context(&source_context)?;
        let target_committed_views = committed_views_for_context(&target_context)?;
        // A merge applies committed heads, so the preview layers no saved edits.
        let committed_only = WorkspaceLiveContent::default();
        let source_input = self
            .build_projection_input(
                ChangeGraphComparison::Workspace,
                source_context,
                &source_committed_views,
                &committed_only,
            )
            .await?;
        let target_overlays = HashMap::new();
        let mut target_repos = target_context.repos;
        for repo in &mut target_repos {
            repo.merge_base_commit = source_bases.get(&repo.repo_id).cloned();
        }
        let WorkspaceSnapshotBuild {
            snapshots,
            origins,
            revision_scope,
            workspace_input_bytes,
        } = self
            .build_workspace_snapshots(&target_repos, &target_overlays, &target_committed_views)
            .await?;
        let target_input = ProjectionInput {
            comparison: ChangeGraphComparison::Workspace,
            range: ChangeGraphRange {
                base_kind: ChangeGraphBaseKind::MergeBase,
                parent_workspace_id: None,
                parent_workspace_name: None,
                repository_count: target_repos.len(),
                live_content: Vec::new(),
            },
            docs_repo_id: target_context.docs_repo_id,
            storage: target_context.storage.clone(),
            org_id: target_context.org_id,
            repo_contexts: target_repos,
            snapshots,
            origins,
            revision_scope,
            workspace_input_bytes,
        };

        let (source, source_snapshots) = self
            .project_prepared_input(source_workspace_id, source_input, false)
            .await?;
        let (target, target_snapshots) = self
            .project_prepared_input(target_workspace_id, target_input, false)
            .await?;
        let conflicts = classify_rebase_conflicts(&source.nodes, &target.nodes);
        let revision = short_hash(&format!(
            "rebase-change-graph|{source_workspace_id}|{target_workspace_id}|{}|{}",
            source.revision, target.revision
        ));
        let response = RebaseChangeGraphResponse {
            revision: revision[..16.min(revision.len())].to_string(),
            source_workspace: RebaseChangeGraphWorkspace {
                id: source_workspace_id,
                name: source_workspace_name,
            },
            target_workspace: RebaseChangeGraphWorkspace {
                id: target_workspace_id,
                name: target_workspace_name,
            },
            common_base_repository_count: source_bases.len(),
            source,
            target,
            conflicts,
        };
        Ok(RebaseChangeGraphProjection {
            response,
            source_snapshots,
            target_snapshots,
        })
    }

    async fn project_with_snapshots(
        &self,
        db: &mut ScopedDb,
        workspace_id: Uuid,
        comparison: ChangeGraphComparison,
        human_intent: bool,
        live: &WorkspaceLiveContent,
    ) -> Result<(ChangeGraphResponse, Vec<WorkspaceFileSnapshot>), AppError> {
        if comparison == ChangeGraphComparison::Uncommitted {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "uncommitted_unsupported".to_string(),
                missing_repo_count: 0,
            }));
        }
        let mut context =
            load_workspace_graph_context(db, &self.repo_storage, workspace_id).await?;
        if context.is_base {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "base_workspace".to_string(),
                missing_repo_count: 0,
            }));
        }
        if comparison == ChangeGraphComparison::Workspace {
            let actor_id = db.actor_id().into_uuid();
            let parent_id = readable_parent_workspace(db.conn(), workspace_id, actor_id).await?;
            let parent = load_workspace_graph_context(db, &self.repo_storage, parent_id).await?;
            select_comparison_bases(&mut context, &parent).await?;
        }
        let committed_views = committed_views_for_context(&context)?;
        let input = self
            .build_projection_input(comparison, context, &committed_views, live)
            .await?;
        self.project_prepared_input(workspace_id, input, human_intent)
            .await
    }

    async fn project_prepared_input(
        &self,
        workspace_id: Uuid,
        mut input: ProjectionInput,
        human_intent: bool,
    ) -> Result<(ChangeGraphResponse, Vec<WorkspaceFileSnapshot>), AppError> {
        let started = Instant::now();
        input
            .snapshots
            .sort_by(|a, b| a.repo_id.cmp(&b.repo_id).then_with(|| a.file.cmp(&b.file)));
        let mut diagnostic_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut revision = Sha256::new();
        revision.update(&input.revision_scope);
        for snapshot in &input.snapshots {
            revision.update(snapshot.repo_id.as_bytes());
            revision.update(snapshot.file.as_bytes());
            match &snapshot.content {
                Some(content) => revision.update(content),
                None => revision.update(b"<deleted>"),
            }
        }
        let revision_hash = format!("{:x}", revision.finalize());

        let mut artifacts: HashMap<ArtifactKey, ArtifactProjection> = HashMap::new();
        let mut compared = Vec::new();
        for snapshot in &input.snapshots {
            let is_docs_repo = Some(snapshot.repo_id) == input.docs_repo_id;
            let class = classify_path(&snapshot.file, is_docs_repo);
            let Some(repo) = input
                .repo_contexts
                .iter()
                .find(|repo| repo.repo_id == snapshot.repo_id)
            else {
                continue;
            };
            let mut artifact =
                file_artifact(repo, &snapshot.file, class, file_change_state(snapshot));
            artifact.changed = true;
            artifact.change_origin = input
                .origins
                .get(&(snapshot.repo_id, snapshot.file.clone()))
                .copied();
            artifact.additions = Some(snapshot.additions);
            artifact.deletions = Some(snapshot.deletions);
            if class.column == ChangeGraphColumn::Ontology {
                match compare_snapshot(snapshot) {
                    Ok(comparison) => {
                        // A file whose only differences are moves or blank lines
                        // changes no statement and no other text: not shown.
                        if comparison.changes.is_empty() && !comparison.other_text_changed() {
                            continue;
                        }
                        artifact.changed_records = comparison
                            .changes
                            .iter()
                            .map(changed_statement_record)
                            .collect();
                        compared.push((
                            artifact.key.clone(),
                            snapshot.repo_id,
                            snapshot.file.clone(),
                            comparison,
                        ));
                    }
                    Err(refusal) => {
                        bump(&mut diagnostic_counts, "ontology_comparison_refused");
                        artifact.comparison_warning = Some(refusal.to_string());
                    }
                }
            }
            artifacts.insert(artifact.key.clone(), artifact);
        }
        if input.comparison == ChangeGraphComparison::Workspace {
            enforce_workspace_comparison_limits(artifacts.len(), input.workspace_input_bytes)?;
        }

        let mut targets = LinkTargets::new(
            &input.storage,
            input.org_id,
            &input.repo_contexts,
            &input.snapshots,
            input.docs_repo_id,
        )?;
        let mut edges = Vec::new();
        for (source_key, repo_id, path, comparison) in &compared {
            for link in statement_links(comparison) {
                resolve_link(
                    &mut targets,
                    &mut artifacts,
                    &mut edges,
                    source_key,
                    *repo_id,
                    path,
                    &link,
                )
                .await;
            }
        }

        if targets.skipped > 0 {
            *diagnostic_counts
                .entry("link_target_budget_reached".to_string())
                .or_default() += targets.skipped;
        }
        let response = select_graph(
            artifacts,
            edges,
            GraphSelectionContext {
                diagnostic_counts,
                revision_hash,
                comparison: input.comparison,
                range: input.range,
                human_intent,
            },
        );

        tracing::info!(
            workspace_id = %workspace_id,
            comparison = ?response.comparison,
            duration_ms = started.elapsed().as_millis() as u64,
            changed_roots = response.nodes.iter().filter(|node| node.change_state != ChangeState::Unchanged).count(),
            relationship_context_nodes = response.nodes.iter().filter(|node| node.connection_state == ConnectionState::Bridge).count(),
            link_target_reads = targets.reads,
            edge_count = response.edges.len(),
            truncated = response.truncated,
            "Projected Source Control workspace Diff Graph"
        );

        Ok((response, input.snapshots))
    }

    async fn build_projection_input(
        &self,
        comparison: ChangeGraphComparison,
        context: WorkspaceGraphContext,
        committed_views: &HashMap<Uuid, CommittedRepoReadView>,
        live: &WorkspaceLiveContent,
    ) -> Result<ProjectionInput, AppError> {
        if comparison == ChangeGraphComparison::Uncommitted {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "uncommitted_unsupported".to_string(),
                missing_repo_count: 0,
            }));
        }
        let Some(parent_workspace_id) = context.parent_workspace_id else {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "not_forked".to_string(),
                missing_repo_count: 0,
            }));
        };
        let missing_repo_count = context
            .repos
            .iter()
            .filter(|repo| repo.merge_base_commit.is_none())
            .count();
        if missing_repo_count > 0 {
            return Err(AppError::from_error(G5Error::DiffGraphUnavailable {
                reason: "missing_merge_base".to_string(),
                missing_repo_count,
            }));
        }

        // Saved edits apply only against the head they were captured with; a
        // head that moved since then shows committed content, said so.
        let mut overlays: HashMap<Uuid, BTreeMap<String, Option<Vec<u8>>>> = HashMap::new();
        let mut live_content = Vec::with_capacity(context.repos.len());
        for repo in &context.repos {
            let overlay = live
                .repositories
                .iter()
                .find(|overlay| overlay.repo_id == repo.repo_id);
            let status = match overlay {
                Some(overlay) if overlay.expected_head == repo.commit => {
                    overlays.insert(repo.repo_id, overlay.files.clone());
                    RepositoryLiveContent {
                        repo_id: repo.repo_id,
                        repo_name: repo.repo_name.clone(),
                        includes_uncommitted: overlay.includes_uncommitted,
                        reason: overlay.reason.clone(),
                        ontology_live: overlay.ontology_live,
                    }
                }
                Some(_) => RepositoryLiveContent {
                    repo_id: repo.repo_id,
                    repo_name: repo.repo_name.clone(),
                    includes_uncommitted: false,
                    reason: Some("workspace_synchronizing".to_string()),
                    ontology_live: false,
                },
                None => RepositoryLiveContent {
                    repo_id: repo.repo_id,
                    repo_name: repo.repo_name.clone(),
                    includes_uncommitted: false,
                    reason: live.committed_reason.clone(),
                    ontology_live: false,
                },
            };
            live_content.push(status);
        }
        let WorkspaceSnapshotBuild {
            snapshots,
            origins,
            revision_scope,
            workspace_input_bytes,
        } = self
            .build_workspace_snapshots(&context.repos, &overlays, committed_views)
            .await?;
        let repository_count = context.repos.len();
        Ok(ProjectionInput {
            comparison,
            range: ChangeGraphRange {
                base_kind: ChangeGraphBaseKind::MergeBase,
                parent_workspace_id: Some(parent_workspace_id),
                parent_workspace_name: context.parent_workspace_name,
                repository_count,
                live_content,
            },
            docs_repo_id: context.docs_repo_id,
            storage: context.storage,
            org_id: context.org_id,
            repo_contexts: context.repos,
            snapshots,
            origins,
            revision_scope,
            workspace_input_bytes,
        })
    }

    async fn build_workspace_snapshots(
        &self,
        repo_contexts: &[RepoContext],
        overlays: &HashMap<Uuid, BTreeMap<String, Option<Vec<u8>>>>,
        committed_views: &HashMap<Uuid, CommittedRepoReadView>,
    ) -> Result<WorkspaceSnapshotBuild, AppError> {
        let mut prepared = Vec::new();
        let mut observed_bytes = 0usize;
        let mut revision_scope = b"comparison:workspace".to_vec();

        for context in repo_contexts {
            let base = context.merge_base_commit.as_deref().ok_or_else(|| {
                AppError::from_error(G5Error::DiffGraphUnavailable {
                    reason: "missing_merge_base".to_string(),
                    missing_repo_count: repo_contexts
                        .iter()
                        .filter(|repo| repo.merge_base_commit.is_none())
                        .count(),
                })
            })?;
            revision_scope.extend_from_slice(context.repo_id.as_bytes());
            revision_scope.extend_from_slice(base.as_bytes());
            revision_scope.extend_from_slice(context.commit.as_bytes());
            if let Some(parent) = &context.parent_head {
                revision_scope.extend_from_slice(parent.as_bytes());
            }

            let current_view = committed_views.get(&context.repo_id).ok_or_else(|| {
                AppError::from_error(G5Error::Internal {
                    message: "Comparison is missing its captured repository view".to_string(),
                })
            })?;
            if current_view.head_commit() != context.commit {
                return Err(AppError::from_error(G5Error::Internal {
                    message: "Comparison view does not match the selected commit".to_string(),
                }));
            }
            let base_view = current_view
                .historical_sibling(&HistoricalGitVersion {
                    repo_id: context.repo_id,
                    head_commit: base.to_string(),
                    reason: HistoricalReadReason::Diff,
                })
                .map_err(AppError::from_error)?;
            let committed_paths = current_view
                .diff_summary_from(&self.repo_storage, &base_view)
                .await
                .map_err(AppError::from_error)?
                .files
                .into_iter()
                .map(|file| file.path)
                .collect::<Vec<_>>();
            let committed_path_set = committed_paths.iter().cloned().collect::<HashSet<_>>();
            let draft_overlay = overlays.get(&context.repo_id);
            let mut candidates = committed_paths.into_iter().collect::<BTreeSet<_>>();
            if let Some(overlay) = draft_overlay {
                candidates.extend(overlay.keys().cloned());
            }
            let candidate_paths = candidates.iter().cloned().collect::<Vec<_>>();
            let base_metadata = base_view
                .file_metadata(&self.repo_storage, &candidate_paths)
                .await
                .map_err(AppError::from_error)?;
            let current_metadata = current_view
                .file_metadata(&self.repo_storage, &candidate_paths)
                .await
                .map_err(AppError::from_error)?;

            for path in candidates {
                let base_metadata = base_metadata.get(&path).cloned().flatten();
                let draft = draft_overlay.and_then(|overlay| overlay.get(&path));
                let (final_metadata, final_source, has_uncommitted_change) =
                    if let Some(content) = draft {
                        let metadata = content
                            .as_ref()
                            .map(|content| {
                                GitRepo::blob_id_for_bytes(content).map(|id| (id, content.len()))
                            })
                            .transpose()
                            .map_err(AppError::from)?;
                        let head = current_metadata.get(&path).cloned().flatten();
                        let uncommitted =
                            metadata.as_ref().map(|(id, _)| id) != head.as_ref().map(|(id, _)| id);
                        (
                            metadata,
                            WorkspaceFinalContentSource::Overlay(content.clone()),
                            uncommitted,
                        )
                    } else {
                        let metadata = current_metadata.get(&path).cloned().flatten();
                        let present = metadata.is_some();
                        (
                            metadata,
                            WorkspaceFinalContentSource::Commit { present },
                            false,
                        )
                    };
                if base_metadata.as_ref().map(|(id, _size)| id)
                    == final_metadata.as_ref().map(|(id, _size)| id)
                {
                    continue;
                }

                let origin = match (committed_path_set.contains(&path), has_uncommitted_change) {
                    (true, true) => ChangeOrigin::Both,
                    (true, false) => ChangeOrigin::Committed,
                    (false, true) => ChangeOrigin::Uncommitted,
                    (false, false) => continue,
                };
                let next_roots = prepared.len() + 1;
                observed_bytes = observed_bytes
                    .saturating_add(base_metadata.as_ref().map_or(0, |(_, size)| *size))
                    .saturating_add(final_metadata.as_ref().map_or(0, |(_, size)| *size));
                enforce_workspace_comparison_limits(next_roots, observed_bytes)?;

                prepared.push(PreparedWorkspaceSnapshot {
                    repo_id: context.repo_id,
                    base_commit: base.to_string(),
                    path,
                    final_source,
                    origin,
                });
            }
        }

        // The global root/byte preflight above intentionally completes before
        // any committed Git blob or semantic ledger is materialized.
        let mut prepared_by_repo: BTreeMap<Uuid, Vec<PreparedWorkspaceSnapshot>> = BTreeMap::new();
        for candidate in prepared {
            prepared_by_repo
                .entry(candidate.repo_id)
                .or_default()
                .push(candidate);
        }
        let prepared_count = prepared_by_repo.values().map(Vec::len).sum();
        let mut snapshots = Vec::with_capacity(prepared_count);
        let mut origins = HashMap::with_capacity(prepared_count);
        for (repo_id, candidates) in prepared_by_repo {
            let current_view = committed_views.get(&repo_id).ok_or_else(|| {
                AppError::from_error(G5Error::Internal {
                    message: format!(
                        "Prepared Diff Graph repository {repo_id} is missing from the committed authority map"
                    ),
                })
            })?;
            let base_commit = candidates
                .first()
                .ok_or_else(|| {
                    AppError::from_error(G5Error::Internal {
                        message: format!(
                            "Prepared Diff Graph repository {repo_id} has no candidate paths"
                        ),
                    })
                })?
                .base_commit
                .clone();
            let base_view = current_view
                .historical_sibling(&HistoricalGitVersion {
                    repo_id,
                    head_commit: base_commit,
                    reason: HistoricalReadReason::Diff,
                })
                .map_err(AppError::from_error)?;
            let paths = candidates
                .iter()
                .map(|candidate| candidate.path.clone())
                .collect::<Vec<_>>();
            let mut base_files = base_view
                .read_files(&self.repo_storage, &paths)
                .await
                .map_err(AppError::from_error)?;
            let mut current_files = current_view
                .read_files(&self.repo_storage, &paths)
                .await
                .map_err(AppError::from_error)?;

            for candidate in candidates {
                let base_content = base_files.remove(&candidate.path).flatten();
                let final_content = match candidate.final_source {
                    WorkspaceFinalContentSource::Commit { present } => {
                        let content = current_files.remove(&candidate.path).flatten();
                        debug_assert_eq!(content.is_some(), present);
                        content
                    }
                    WorkspaceFinalContentSource::Overlay(content) => content,
                };
                let diff_text = unified_diff_buffers(
                    base_content.as_deref().unwrap_or_default(),
                    final_content.as_deref().unwrap_or_default(),
                    &candidate.path,
                )
                .unwrap_or_default();
                let (hunks, additions, deletions) = parse_task_unified_diff(&diff_text);
                origins.insert(
                    (candidate.repo_id, candidate.path.clone()),
                    candidate.origin,
                );
                snapshots.push(WorkspaceFileSnapshot {
                    repo_id: candidate.repo_id,
                    base_commit: Some(candidate.base_commit),
                    file: candidate.path,
                    base_content,
                    content: final_content,
                    additions,
                    deletions,
                    hunks,
                });
            }
        }

        Ok(WorkspaceSnapshotBuild {
            snapshots,
            origins,
            revision_scope,
            workspace_input_bytes: observed_bytes,
        })
    }
}

fn committed_views_for_context(
    context: &WorkspaceGraphContext,
) -> Result<HashMap<Uuid, CommittedRepoReadView>, AppError> {
    context
        .repos
        .iter()
        .map(|repo| {
            let view = CommittedRepoReadView::from_historical(
                &context.storage,
                context.org_id,
                &HistoricalGitVersion {
                    repo_id: repo.repo_id,
                    head_commit: repo.commit.clone(),
                    reason: HistoricalReadReason::Diff,
                },
            )
            .map_err(AppError::from_error)?;
            Ok((repo.repo_id, view))
        })
        .collect()
}

async fn load_workspace_graph_context(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    workspace_id: Uuid,
) -> Result<WorkspaceGraphContext, AppError> {
    let workspace = sqlx::query_as::<_, (String, bool, Option<Uuid>)>(
        "SELECT name, is_base, cloned_from_id FROM workspaces
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(db.conn())
    .await?
    .ok_or_else(|| {
        AppError::from_error(G5Error::NotFound {
            resource: "workspace".to_string(),
        })
    })?;
    let org_id = crate::services::workspace_reader::workspace_org(db.conn(), workspace_id).await?;
    let storage = repo_storage.scoped(&*db);
    let members = load_workspace_repositories(db.conn(), workspace_id, false).await?;
    let refs = members
        .iter()
        .map(|m| (m.repo_id, m.branch_name.clone()))
        .collect::<Vec<_>>();
    let mut heads = read_branch_head_oids(&storage, org_id, &refs).await?;
    let docs_repo_id = members.iter().find(|m| m.role == "docs").map(|m| m.repo_id);
    let mut repos = Vec::with_capacity(members.len());
    for member in members {
        let commit = heads
            .remove(&(member.repo_id, member.branch_name))
            .flatten()
            .ok_or_else(|| {
                AppError::from_error(G5Error::DiffGraphUnavailable {
                    reason: "missing_ref".to_string(),
                    missing_repo_count: 1,
                })
            })?;
        repos.push(RepoContext {
            workspace_id,
            repo_id: member.repo_id,
            repo_name: member.repo_name,
            repo_slug: member.repo_slug,
            commit,
            merge_base_commit: None,
            parent_head: None,
        });
    }
    Ok(WorkspaceGraphContext {
        storage,
        org_id,
        workspace_name: workspace.0,
        is_base: workspace.1,
        parent_workspace_id: workspace.2,
        parent_workspace_name: None,
        docs_repo_id,
        repos,
    })
}

async fn select_comparison_bases(
    source: &mut WorkspaceGraphContext,
    parent: &WorkspaceGraphContext,
) -> Result<(), AppError> {
    let parent_heads = parent
        .repos
        .iter()
        .map(|r| (r.repo_id, r.commit.clone()))
        .collect::<BTreeMap<_, _>>();
    let source_heads = source
        .repos
        .iter()
        .map(|r| (r.repo_id, r.commit.clone()))
        .collect();
    let bases = crate::services::workspace_repositories::comparison_bases(
        &source.storage,
        source.org_id,
        &source_heads,
        &parent_heads,
    )
    .await?;
    for repo in &mut source.repos {
        repo.merge_base_commit = Some(bases[&repo.repo_id].clone());
        repo.parent_head = Some(parent_heads[&repo.repo_id].clone());
    }
    source.parent_workspace_name = Some(parent.workspace_name.clone());
    Ok(())
}

fn enforce_workspace_comparison_limits(
    observed_roots: usize,
    observed_bytes: usize,
) -> Result<(), AppError> {
    if observed_roots > MAX_WORKSPACE_CHANGED_ROOTS
        || observed_bytes > MAX_WORKSPACE_COMPARISON_INPUT_BYTES
    {
        return Err(AppError::from_error(G5Error::DiffGraphTooLarge {
            observed_roots,
            max_roots: MAX_WORKSPACE_CHANGED_ROOTS,
            observed_bytes,
            max_bytes: MAX_WORKSPACE_COMPARISON_INPUT_BYTES,
        }));
    }
    Ok(())
}

/// What a changed path is in the graph (`g5.diff-graph.columns`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathKind {
    Ontology,
    Chat,
    DesignPlan,
    ProjectDoc,
    View,
    Other,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PathClass {
    kind: PathKind,
    column: ChangeGraphColumn,
    intent_group: Option<IntentGroup>,
    is_docs_repo: bool,
}

/// Classify a repository-relative path, in rule order: ontology, chat intent
/// Markdown, design plans, Project Docs (minus question sets), other `.g5`
/// content, the rest of the docs repository, then source code. Legacy
/// `.semlift` layouts and intent ledgers have no special meaning.
fn classify_path(path: &str, is_docs_repo: bool) -> PathClass {
    let class = |kind, column, intent_group| PathClass {
        kind,
        column,
        intent_group,
        is_docs_repo,
    };
    let segments: Vec<&str> = path.split('/').collect();
    let valid = segments
        .iter()
        .all(|part| !matches!(*part, "" | "." | ".."));
    if g5_contracts::g5_paths::is_ontology_path(path) {
        return class(PathKind::Ontology, ChangeGraphColumn::Ontology, None);
    }
    if valid && path.starts_with(".g5/chat_intent/") && path.ends_with(".md") && segments.len() > 2
    {
        return class(
            PathKind::Chat,
            ChangeGraphColumn::Intents,
            Some(IntentGroup::Conversations),
        );
    }
    if valid
        && segments.len() == 4
        && segments[0] == ".g5"
        && segments[1] == "pr_ontology_diff_design"
        && segments[3] == "ontology-plan.md"
    {
        return class(
            PathKind::DesignPlan,
            ChangeGraphColumn::Intents,
            Some(IntentGroup::Documents),
        );
    }
    if is_docs_repo && path.starts_with("docs/") {
        let question_set =
            segments.len() == 4 && segments[2] == "questions" && segments[3].ends_with(".g5q.json");
        return if question_set {
            class(
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            )
        } else {
            class(
                PathKind::ProjectDoc,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Documents),
            )
        };
    }
    if segments.contains(&".g5") {
        return class(
            PathKind::Other,
            ChangeGraphColumn::Intents,
            Some(IntentGroup::Other),
        );
    }
    if is_docs_repo {
        let kind = if path.starts_with("views/") {
            PathKind::View
        } else {
            PathKind::Other
        };
        return class(kind, ChangeGraphColumn::Intents, Some(IntentGroup::Other));
    }
    class(PathKind::Code, ChangeGraphColumn::SourceCode, None)
}

fn route_file_type(class: PathClass) -> FileType {
    match class.kind {
        PathKind::Ontology => FileType::Ontology,
        PathKind::Chat => FileType::Chat,
        PathKind::ProjectDoc => FileType::ExternalDoc,
        PathKind::View => FileType::View,
        PathKind::Other if class.is_docs_repo => FileType::ExternalDoc,
        PathKind::DesignPlan | PathKind::Other | PathKind::Code => FileType::Code,
    }
}

fn artifact_type(kind: PathKind) -> &'static str {
    match kind {
        PathKind::Ontology => "ontology",
        PathKind::Chat => "conversation",
        PathKind::DesignPlan => "design_plan",
        PathKind::ProjectDoc => "document",
        PathKind::View => "view_document",
        PathKind::Other => "other",
        PathKind::Code => "source_file",
    }
}

/// Ontology folder grouping inside the single Ontology column: the first folder
/// below `.g5/ontology`, or none for top-level pages such as `root.g5.md`.
fn ontology_group(path: &str) -> Option<String> {
    let rest = path.strip_prefix(g5_contracts::g5_paths::ONTOLOGY_ROOT)?;
    let (folder, _) = rest.split_once('/')?;
    Some(folder.to_string())
}

fn artifact_title(path: &str, kind: PathKind) -> String {
    if kind == PathKind::Ontology {
        if let Some(rest) = path.strip_prefix(g5_contracts::g5_paths::ONTOLOGY_ROOT) {
            return rest.trim_end_matches(".g5.md").to_string();
        }
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn file_artifact(
    repo: &RepoContext,
    path: &str,
    class: PathClass,
    change_state: ChangeState,
) -> ArtifactProjection {
    let key = ArtifactKey {
        repo_id: repo.repo_id,
        column: class.column.rank(),
        identity: path.to_string(),
    };
    ArtifactProjection {
        sort_key: format!("{path}|{}", key.stable()),
        key,
        column: class.column,
        intent_group: class.intent_group,
        ontology_group: (class.kind == PathKind::Ontology)
            .then(|| ontology_group(path))
            .flatten(),
        title: artifact_title(path, class.kind),
        artifact_type: artifact_type(class.kind).to_string(),
        path: Some(path.to_string()),
        change_state,
        change_origin: None,
        additions: None,
        deletions: None,
        changed_records: Vec::new(),
        comparison_warning: None,
        missing_targets: Vec::new(),
        route: ChangeGraphRoute {
            repo_id: repo.repo_id,
            workspace_id: repo.workspace_id,
            file_path: Some(path.to_string()),
            file_type: route_file_type(class).to_string(),
            record_id: None,
            session_id: None,
        },
        changed: false,
    }
}

fn utf8(bytes: Option<&[u8]>) -> Result<Option<&str>, ontology::ComparisonRefusal> {
    bytes
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|_| ontology::ComparisonRefusal {
            diagnostic: "content is not UTF-8 text".to_string(),
        })
}

/// The shared statement rule over one changed file (`g5.diff-graph.ontology-nodes`).
fn compare_snapshot(
    snapshot: &WorkspaceFileSnapshot,
) -> Result<ontology::FileComparison, ontology::ComparisonRefusal> {
    let before = utf8(snapshot.base_content.as_deref())?;
    let after = utf8(snapshot.content.as_deref())?;
    ontology::compare(before, after)
}

fn statement_change_state(change: ontology::StatementChangeKind) -> ChangeState {
    match change {
        ontology::StatementChangeKind::Added => ChangeState::Added,
        ontology::StatementChangeKind::Modified => ChangeState::Modified,
        ontology::StatementChangeKind::Deleted => ChangeState::Deleted,
    }
}

fn changed_statement_record(change: &ontology::StatementChange) -> ChangedRecord {
    let shown = change.after.as_ref().or(change.before.as_ref());
    ChangedRecord {
        id: change.id.clone(),
        label: change.id.clone(),
        change_state: statement_change_state(change.change),
        kind: shown.map(|statement| statement.kind.to_string()),
        is_human: shown.is_some_and(|statement| statement.human),
        content: None,
    }
}

/// Which version a link belongs to: current text resolves against current
/// files, deleted text against the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LinkVersion {
    Current,
    Baseline,
}

#[derive(Debug, Clone)]
struct ChangedLink {
    statement_id: String,
    link: ontology::StatementLink,
    change_state: ChangeState,
    version: LinkVersion,
}

/// Every authored link of a shown ontology file, marked added, removed or
/// unchanged by statement, destination and kind.
fn statement_links(comparison: &ontology::FileComparison) -> Vec<ChangedLink> {
    let same = |a: &ontology::StatementLink, b: &ontology::StatementLink| {
        a.href == b.href && a.expands == b.expands
    };
    let unique = |links: &[ontology::StatementLink]| {
        let mut out: Vec<ontology::StatementLink> = Vec::new();
        for link in links {
            if !out.iter().any(|seen| same(seen, link)) {
                out.push(link.clone());
            }
        }
        out
    };
    let before: HashMap<&str, &ontology::OntologyStatement> = comparison
        .before
        .iter()
        .map(|statement| (statement.id.as_str(), statement))
        .collect();
    let after: HashMap<&str, &ontology::OntologyStatement> = comparison
        .after
        .iter()
        .map(|statement| (statement.id.as_str(), statement))
        .collect();
    // Unchanged links are drawn only from changed statements; an unchanged
    // statement contributes just the links that were added or removed.
    let changed: HashSet<&str> = comparison
        .changes
        .iter()
        .map(|change| change.id.as_str())
        .collect();
    let mut out = Vec::new();
    for statement in &comparison.after {
        let prior = before.get(statement.id.as_str());
        for link in unique(&statement.links) {
            let existed = prior.is_some_and(|prior| prior.links.iter().any(|old| same(old, &link)));
            if existed && !changed.contains(statement.id.as_str()) {
                continue;
            }
            out.push(ChangedLink {
                statement_id: statement.id.clone(),
                link,
                change_state: if existed {
                    ChangeState::Unchanged
                } else {
                    ChangeState::Added
                },
                version: LinkVersion::Current,
            });
        }
    }
    for statement in &comparison.before {
        let now = after.get(statement.id.as_str());
        for link in unique(&statement.links) {
            if now.is_some_and(|now| now.links.iter().any(|new| same(new, &link))) {
                continue;
            }
            out.push(ChangedLink {
                statement_id: statement.id.clone(),
                link,
                change_state: ChangeState::Deleted,
                version: LinkVersion::Baseline,
            });
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LinkDestination {
    /// External URLs, GitHub permalinks and same-file anchors draw nothing.
    Ignored,
    /// A relative link that cannot name a workspace file.
    Invalid,
    File {
        repo_id: Uuid,
        path: String,
        fragment: Option<String>,
    },
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = [bytes[i + 1], bytes[i + 2]];
            let hex = std::str::from_utf8(&hex).expect("ASCII hex digits");
            out.push(u8::from_str_radix(hex, 16).expect("hex digits"));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// Resolve a link destination like the editor's `resolveOntologyLink`:
/// relative to the source file, `/` from the repository root, never escaping
/// the repository; `g5://file/<path>?repo=<id or name>` names another
/// repository of the same workspace.
fn link_destination(
    source_repo: Uuid,
    source_path: &str,
    href: &str,
    repos: &[RepoContext],
) -> LinkDestination {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return LinkDestination::Ignored;
    }
    let (without_fragment, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(percent_decode(fragment))),
        None => (href, None),
    };
    if let Some(rest) = without_fragment.strip_prefix("g5://file/") {
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let repo_ref = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("repo="))
            .map(percent_decode);
        let Some(repo_ref) = repo_ref else {
            return LinkDestination::Invalid;
        };
        let matches: Vec<&RepoContext> = repos
            .iter()
            .filter(|repo| {
                repo.repo_id.to_string() == repo_ref
                    || repo.repo_name == repo_ref
                    || repo.repo_slug == repo_ref
            })
            .collect();
        let [repo] = matches.as_slice() else {
            return LinkDestination::Invalid;
        };
        return match normalize_link_path("", &percent_decode(path)) {
            Some(path) => LinkDestination::File {
                repo_id: repo.repo_id,
                path,
                fragment,
            },
            None => LinkDestination::Invalid,
        };
    }
    let has_scheme = without_fragment.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    });
    if has_scheme || without_fragment.starts_with("//") {
        return LinkDestination::Ignored;
    }
    if without_fragment.contains('?') || without_fragment.contains('\\') {
        return LinkDestination::Invalid;
    }
    let decoded = percent_decode(without_fragment);
    let base = if decoded.starts_with('/') {
        ""
    } else {
        source_path.rsplit_once('/').map_or("", |(dir, _)| dir)
    };
    match normalize_link_path(base, &decoded) {
        Some(path) => LinkDestination::File {
            repo_id: source_repo,
            path,
            fragment,
        },
        None => LinkDestination::Invalid,
    }
}

fn normalize_link_path(base: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty() && !relative.contains('\0')).then(|| parts.join("/"))
}

/// Link targets of one version, resolved like the editor's
/// `ontologyFragmentIndex`: a fragment must name exactly one anchor (explicit
/// or generated heading ID); it is a statement when a valid statement owns it,
/// by its ID or an alias anchor naming a former ID.
#[derive(Default)]
struct FragmentIndex {
    statements: HashMap<String, String>,
    anchors: HashMap<String, usize>,
}

impl FragmentIndex {
    fn target(&self, fragment: &str) -> Option<String> {
        match self.anchors.get(fragment) {
            Some(1) => Some(self.statements.get(fragment).cloned().unwrap_or_default()),
            _ => None,
        }
    }
}

fn fragment_index(text: &str) -> FragmentIndex {
    FragmentIndex {
        statements: current_ontology_records(text)
            .into_iter()
            .filter(|record| record.diagnostic.is_none())
            .flat_map(|record| {
                let id = record.id;
                record
                    .aliases
                    .into_iter()
                    .chain(std::iter::once(id.clone()))
                    .map(move |fragment| (fragment, id.clone()))
            })
            .collect(),
        anchors: current_ontology_anchor_counts(text),
    }
}

/// Bounded, cached lookups of link targets. A path absent from the changed
/// snapshots has identical baseline and current content, so the baseline
/// commit answers both versions.
struct LinkTargets<'a> {
    storage: &'a ScopedRepoStorage,
    repos: &'a [RepoContext],
    docs_repo_id: Option<Uuid>,
    snapshots: HashMap<(Uuid, &'a str), &'a WorkspaceFileSnapshot>,
    base_views: HashMap<Uuid, CommittedRepoReadView>,
    exists: HashMap<(Uuid, String), bool>,
    fragments: HashMap<(Uuid, String, LinkVersion), Option<FragmentIndex>>,
    reads: usize,
    /// Links skipped because the lookup budget was reached.
    skipped: usize,
    over_budget: bool,
}

impl<'a> LinkTargets<'a> {
    fn new(
        storage: &'a ScopedRepoStorage,
        org_id: Uuid,
        repos: &'a [RepoContext],
        snapshots: &'a [WorkspaceFileSnapshot],
        docs_repo_id: Option<Uuid>,
    ) -> Result<Self, AppError> {
        let mut base_views = HashMap::new();
        for repo in repos {
            if let Some(base) = &repo.merge_base_commit {
                base_views.insert(
                    repo.repo_id,
                    CommittedRepoReadView::from_historical(
                        storage,
                        org_id,
                        &HistoricalGitVersion {
                            repo_id: repo.repo_id,
                            head_commit: base.clone(),
                            reason: HistoricalReadReason::Diff,
                        },
                    )
                    .map_err(AppError::from_error)?,
                );
            }
        }
        Ok(Self {
            storage,
            repos,
            docs_repo_id,
            snapshots: snapshots
                .iter()
                .map(|snapshot| ((snapshot.repo_id, snapshot.file.as_str()), snapshot))
                .collect(),
            base_views,
            exists: HashMap::new(),
            fragments: HashMap::new(),
            reads: 0,
            skipped: 0,
            over_budget: false,
        })
    }

    fn class(&self, repo_id: Uuid, path: &str) -> PathClass {
        classify_path(path, Some(repo_id) == self.docs_repo_id)
    }

    async fn exists(&mut self, repo_id: Uuid, path: &str, version: LinkVersion) -> bool {
        if let Some(snapshot) = self.snapshots.get(&(repo_id, path)) {
            return match version {
                LinkVersion::Current => snapshot.content.is_some(),
                LinkVersion::Baseline => snapshot.base_content.is_some(),
            };
        }
        if let Some(found) = self.exists.get(&(repo_id, path.to_string())) {
            return *found;
        }
        if self.exists.len() >= MAX_LINK_TARGET_FILES {
            self.over_budget = true;
            return false;
        }
        let found = match self.base_views.get(&repo_id) {
            Some(view) => view
                .file_metadata(self.storage, &[path.to_string()])
                .await
                .ok()
                .and_then(|metadata| metadata.get(path).cloned().flatten())
                .is_some(),
            None => false,
        };
        self.exists.insert((repo_id, path.to_string()), found);
        found
    }

    async fn fragment_index(
        &mut self,
        repo_id: Uuid,
        path: &str,
        version: LinkVersion,
    ) -> Option<&FragmentIndex> {
        let key = (repo_id, path.to_string(), version);
        if !self.fragments.contains_key(&key) {
            let text = match self.snapshots.get(&(repo_id, path)) {
                Some(snapshot) => match version {
                    LinkVersion::Current => snapshot.content.clone(),
                    LinkVersion::Baseline => snapshot.base_content.clone(),
                },
                None => match self.base_views.get(&repo_id) {
                    Some(view) => {
                        self.reads += 1;
                        view.read_file_bounded(self.storage, path, MAX_FILE_DIFF_INPUT_BYTES)
                            .await
                            .ok()
                    }
                    None => None,
                },
            };
            let index = text
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .map(|text| fragment_index(&text));
            self.fragments.insert(key.clone(), index);
        }
        self.fragments.get(&key).and_then(Option::as_ref)
    }

    /// The statement ID (or empty for a whole-file target) a link reaches at a
    /// version, or `None` when the target is absent there.
    async fn resolve(
        &mut self,
        repo_id: Uuid,
        path: &str,
        fragment: Option<&str>,
        version: LinkVersion,
    ) -> Option<String> {
        if !self.exists(repo_id, path, version).await {
            return None;
        }
        let fragment = fragment.filter(|fragment| !fragment.is_empty());
        let Some(fragment) =
            fragment.filter(|_| self.class(repo_id, path).kind == PathKind::Ontology)
        else {
            return Some(String::new());
        };
        match self.fragment_index(repo_id, path, version).await {
            Some(index) => index.target(fragment),
            // Unreadable or oversized content cannot be checked; keep the file.
            None => Some(String::new()),
        }
    }
}

/// Draw one authored link (`g5.diff-graph.relationships`,
/// `g5.diff-graph.missing-targets`): never follow an ID elsewhere, point a link
/// to a target deleted in this comparison at the deleted node, and report any
/// other unresolved link only when the link itself was added.
async fn resolve_link(
    targets: &mut LinkTargets<'_>,
    artifacts: &mut HashMap<ArtifactKey, ArtifactProjection>,
    edges: &mut Vec<LinkEdge>,
    source: &ArtifactKey,
    source_repo: Uuid,
    source_path: &str,
    changed: &ChangedLink,
) {
    let link = &changed.link;
    let (repo_id, path, fragment) =
        match link_destination(source_repo, source_path, &link.href, targets.repos) {
            LinkDestination::Ignored => return,
            LinkDestination::Invalid => {
                report_missing(artifacts, source, changed);
                return;
            }
            LinkDestination::File {
                repo_id,
                path,
                fragment,
            } => (repo_id, path, fragment),
        };
    if repo_id == source_repo && path == source_path {
        return;
    }
    let class = targets.class(repo_id, &path);
    let relationship = if link.expands {
        "expands"
    } else {
        match class.kind {
            PathKind::Ontology => "reference",
            PathKind::Chat | PathKind::DesignPlan => "provenance",
            PathKind::ProjectDoc => "project_document",
            PathKind::Code => "implementation",
            PathKind::View | PathKind::Other => return,
        }
    };
    let fragment = fragment.as_deref();
    targets.over_budget = false;
    let resolved = targets
        .resolve(repo_id, &path, fragment, changed.version)
        .await;
    if targets.over_budget {
        targets.skipped += 1;
        return;
    }
    let (target_id, target_deleted) = match resolved {
        Some(target_id) => {
            let deleted = changed.version == LinkVersion::Baseline
                && targets
                    .resolve(repo_id, &path, fragment, LinkVersion::Current)
                    .await
                    .is_none();
            (target_id, deleted)
        }
        None if changed.version == LinkVersion::Current => {
            match targets
                .resolve(repo_id, &path, fragment, LinkVersion::Baseline)
                .await
            {
                Some(target_id) => (target_id, true),
                None if targets.over_budget => {
                    targets.skipped += 1;
                    return;
                }
                None => {
                    report_missing(artifacts, source, changed);
                    return;
                }
            }
        }
        None => return,
    };
    let Some(repo) = targets.repos.iter().find(|repo| repo.repo_id == repo_id) else {
        return;
    };
    let target_key = ArtifactKey {
        repo_id,
        column: class.column.rank(),
        identity: path.clone(),
    };
    artifacts
        .entry(target_key.clone())
        .or_insert_with(|| file_artifact(repo, &path, class, ChangeState::Unchanged));
    edges.push(LinkEdge {
        source: source.clone(),
        target: target_key,
        source_file: source_path.to_string(),
        source_id: changed.statement_id.clone(),
        target_file: path,
        target_id,
        relationship,
        change_state: changed.change_state,
        target_deleted,
    });
}

fn report_missing(
    artifacts: &mut HashMap<ArtifactKey, ArtifactProjection>,
    source: &ArtifactKey,
    changed: &ChangedLink,
) {
    if changed.change_state != ChangeState::Added {
        return;
    }
    if let Some(artifact) = artifacts.get_mut(source) {
        let missing = MissingLinkTarget {
            statement_id: changed.statement_id.clone(),
            href: changed.link.href.clone(),
            text: changed.link.text.clone(),
        };
        if !artifact.missing_targets.contains(&missing) {
            artifact.missing_targets.push(missing);
        }
    }
}

fn file_change_state(snapshot: &WorkspaceFileSnapshot) -> ChangeState {
    match (&snapshot.base_content, &snapshot.content) {
        (None, Some(_)) => ChangeState::Added,
        (Some(_), None) => ChangeState::Deleted,
        _ => ChangeState::Modified,
    }
}

fn select_graph(
    mut artifacts: HashMap<ArtifactKey, ArtifactProjection>,
    edges: Vec<LinkEdge>,
    context: GraphSelectionContext,
) -> ChangeGraphResponse {
    let GraphSelectionContext {
        mut diagnostic_counts,
        revision_hash,
        comparison,
        range,
        human_intent,
    } = context;
    // Human intent lens (`g5.diff-graph.human-lens`): keep ontology files with a
    // changed human statement and list only those statements. Nothing else is
    // human-attributed. Links and context nodes of kept files remain.
    if human_intent {
        for artifact in artifacts.values_mut().filter(|artifact| artifact.changed) {
            artifact.changed_records.retain(|record| record.is_human);
        }
        let kept: HashSet<ArtifactKey> = artifacts
            .values()
            .filter(|artifact| {
                artifact.changed
                    && artifact.column == ChangeGraphColumn::Ontology
                    && !artifact.changed_records.is_empty()
            })
            .map(|artifact| artifact.key.clone())
            .collect();
        let linked: HashSet<ArtifactKey> = edges
            .iter()
            .filter(|edge| kept.contains(&edge.source))
            .map(|edge| edge.target.clone())
            .collect();
        // Other changed artifacts a kept file links to stay as context, with
        // their own change state; the rest are hidden.
        artifacts.retain(|key, artifact| {
            kept.contains(key) || !artifact.changed || linked.contains(key)
        });
        for artifact in artifacts.values_mut() {
            if !kept.contains(&artifact.key) && artifact.changed {
                artifact.changed = false;
                artifact.changed_records.clear();
                artifact.missing_targets.clear();
            }
        }
    }

    let mut roots: Vec<ArtifactKey> = artifacts
        .values()
        .filter(|artifact| artifact.changed)
        .map(|artifact| artifact.key.clone())
        .collect();
    roots.sort_by_key(ArtifactKey::stable);
    let root_set: HashSet<ArtifactKey> = roots.iter().cloned().collect();
    let edges: Vec<LinkEdge> = edges
        .into_iter()
        .filter(|edge| root_set.contains(&edge.source) && artifacts.contains_key(&edge.target))
        .collect();
    for artifact in artifacts.values_mut() {
        if !root_set.contains(&artifact.key) {
            artifact.missing_targets.clear();
        }
    }

    let mut context_keys: Vec<ArtifactKey> = edges
        .iter()
        .map(|edge| edge.target.clone())
        .filter(|key| !root_set.contains(key))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    context_keys.sort_by_key(ArtifactKey::stable);
    let mut truncated = diagnostic_counts.contains_key("link_target_budget_reached");
    if context_keys.len() > MAX_RELATIONSHIP_CONTEXT_NODES {
        truncated = true;
        bump(
            &mut diagnostic_counts,
            "relationship_context_budget_reached",
        );
    }
    let mut included: HashSet<ArtifactKey> = root_set.clone();
    included.extend(
        context_keys
            .into_iter()
            .take(MAX_RELATIONSHIP_CONTEXT_NODES),
    );

    let mut grouped: BTreeMap<(ArtifactKey, ArtifactKey), Vec<LinkEdge>> = BTreeMap::new();
    for edge in edges {
        if edge.source != edge.target
            && included.contains(&edge.source)
            && included.contains(&edge.target)
        {
            grouped
                .entry((edge.source.clone(), edge.target.clone()))
                .or_default()
                .push(edge);
        }
    }
    if grouped.len() > MAX_GRAPH_EDGES {
        truncated = true;
        bump(&mut diagnostic_counts, "edge_budget_reached");
    }

    let node_ids: HashMap<ArtifactKey, String> = included
        .iter()
        .map(|key| (key.clone(), format!("node-{}", short_hash(&key.stable()))))
        .collect();
    let file_types: HashMap<ArtifactKey, String> = included
        .iter()
        .filter_map(|key| {
            artifacts
                .get(key)
                .map(|artifact| (key.clone(), artifact.route.file_type.clone()))
        })
        .collect();
    let mut evidence_count = 0usize;
    let mut output_edges = Vec::new();
    for ((source, target), mut group) in grouped.into_iter().take(MAX_GRAPH_EDGES) {
        group.sort_by(|a, b| {
            (
                &a.source_id,
                &a.target_id,
                a.relationship,
                a.change_state as u8,
            )
                .cmp(&(
                    &b.source_id,
                    &b.target_id,
                    b.relationship,
                    b.change_state as u8,
                ))
        });
        group.dedup_by(|a, b| {
            a.source_id == b.source_id
                && a.target_id == b.target_id
                && a.relationship == b.relationship
                && a.change_state == b.change_state
        });
        let mut evidence = Vec::new();
        for edge in group {
            if evidence_count >= MAX_EVIDENCE_ROWS {
                truncated = true;
                bump(&mut diagnostic_counts, "evidence_budget_reached");
                break;
            }
            evidence_count += 1;
            let stable = format!(
                "{}#{}>{}#{}|{}|{:?}",
                edge.source.stable(),
                edge.source_id,
                edge.target.stable(),
                edge.target_id,
                edge.relationship,
                edge.change_state
            );
            evidence.push(ChangeGraphEvidence {
                id: format!("evidence-{}", short_hash(&stable)),
                source_file_type: file_types.get(&source).cloned().unwrap_or_default(),
                source_file: edge.source_file,
                source_id: edge.source_id,
                target_file_type: file_types.get(&target).cloned().unwrap_or_default(),
                target_file: edge.target_file,
                target_id: edge.target_id,
                relationship: edge.relationship.to_string(),
                relevance: None,
                change_state: edge.change_state,
                target_deleted: edge.target_deleted,
            });
        }
        output_edges.push(ChangeGraphEdge {
            id: format!(
                "edge-{}",
                short_hash(&format!("{}>{}", source.stable(), target.stable()))
            ),
            source_node_id: node_ids[&source].clone(),
            target_node_id: node_ids[&target].clone(),
            evidence,
        });
    }

    let connected_nodes: HashSet<String> = output_edges
        .iter()
        .flat_map(|edge| [edge.source_node_id.clone(), edge.target_node_id.clone()])
        .collect();
    let mut nodes: Vec<(String, ChangeGraphNode)> = included
        .into_iter()
        .filter_map(|key| artifacts.remove(&key))
        .map(|artifact| {
            let id = node_ids[&artifact.key].clone();
            let connection_state = if !artifact.changed {
                ConnectionState::Bridge
            } else if connected_nodes.contains(&id) {
                ConnectionState::Connected
            } else {
                ConnectionState::Unlinked
            };
            let has_human_blocks = artifact
                .changed_records
                .iter()
                .any(|record| record.is_human);
            let node = ChangeGraphNode {
                id,
                column: artifact.column,
                intent_group: artifact.intent_group,
                ontology_group: artifact.ontology_group,
                title: artifact.title,
                artifact_type: artifact.artifact_type,
                path: artifact.path,
                change_state: artifact.change_state,
                change_origin: artifact.change_origin,
                has_human_blocks,
                connection_state,
                additions: artifact.additions,
                deletions: artifact.deletions,
                changed_records: artifact.changed_records,
                comparison_warning: artifact.comparison_warning,
                missing_targets: artifact.missing_targets,
                route: artifact.route,
            };
            (artifact.sort_key, node)
        })
        .collect();
    nodes.sort_by(|a, b| {
        a.1.column
            .rank()
            .cmp(&b.1.column.rank())
            .then_with(|| {
                intent_group_rank(a.1.intent_group).cmp(&intent_group_rank(b.1.intent_group))
            })
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    let nodes = nodes.into_iter().map(|(_, node)| node).collect();
    output_edges.sort_by(|a, b| a.id.cmp(&b.id));

    ChangeGraphResponse {
        revision: revision_hash[..16.min(revision_hash.len())].to_string(),
        comparison,
        range,
        nodes,
        edges: output_edges,
        diagnostics: diagnostic_counts
            .into_iter()
            .map(|(code, count)| ChangeGraphDiagnostic { code, count })
            .collect(),
        truncated,
    }
}

fn classify_rebase_conflicts(
    source_nodes: &[ChangeGraphNode],
    target_nodes: &[ChangeGraphNode],
) -> Vec<RebaseChangeGraphConflict> {
    let target_by_id: HashMap<&str, &ChangeGraphNode> = target_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut conflicts = Vec::new();
    for source in source_nodes {
        if source.column != ChangeGraphColumn::Ontology
            || source.change_state == ChangeState::Unchanged
        {
            continue;
        }
        let Some(target) = target_by_id.get(source.id.as_str()).copied() else {
            continue;
        };
        if target.change_state == ChangeState::Unchanged {
            continue;
        }
        let source_blocks: BTreeSet<&str> = source
            .changed_records
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        let target_blocks: BTreeSet<&str> = target
            .changed_records
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        let overlapping_block_ids: Vec<String> = source_blocks
            .intersection(&target_blocks)
            .map(|id| (*id).to_string())
            .collect();
        let kind = if overlapping_block_ids.is_empty() {
            RebaseChangeGraphConflictKind::SameNode
        } else {
            RebaseChangeGraphConflictKind::SameBlock
        };
        let stable = format!(
            "{}|{:?}|{}",
            source.id,
            kind,
            overlapping_block_ids.join(",")
        );
        conflicts.push(RebaseChangeGraphConflict {
            id: format!("conflict-{}", short_hash(&stable)),
            artifact_key: source.id.clone(),
            source_node_id: source.id.clone(),
            target_node_id: target.id.clone(),
            kind,
            overlap_block_count: (!overlapping_block_ids.is_empty())
                .then_some(overlapping_block_ids.len()),
            overlapping_block_ids,
        });
    }
    conflicts.sort_by(|a, b| a.artifact_key.cmp(&b.artifact_key));
    conflicts
}

fn semantic_record_count(detail: &ChangeGraphNodeDetail, node: &ChangeGraphNode) -> usize {
    match detail {
        ChangeGraphNodeDetail::SemanticDiff { records, .. } => records.len(),
        _ => node.changed_records.len(),
    }
}

fn exact_overlap_line_count(
    source_node: &ChangeGraphNode,
    target_node: &ChangeGraphNode,
    overlap_block_ids: &[String],
    source_snapshots: &[WorkspaceFileSnapshot],
    target_snapshots: &[WorkspaceFileSnapshot],
) -> Result<i64, AppError> {
    let source_snapshot = snapshot_for_node(source_node, source_snapshots).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "paired graph Source semantic evidence is unavailable".to_string(),
        })
    })?;
    let target_snapshot = snapshot_for_node(target_node, target_snapshots).ok_or_else(|| {
        AppError::from_error(G5Error::Internal {
            message: "paired graph Target semantic evidence is unavailable".to_string(),
        })
    })?;

    let source_before = semantic_block_content_by_id(source_snapshot.base_content.as_deref());
    let target_before = semantic_block_content_by_id(target_snapshot.base_content.as_deref());
    let source_after = semantic_block_content_by_id(source_snapshot.content.as_deref());
    let target_after = semantic_block_content_by_id(target_snapshot.content.as_deref());
    let mut overlap = 0usize;
    for block_id in overlap_block_ids {
        let source_base = source_before.get(block_id).map(String::as_str);
        let target_base = target_before.get(block_id).map(String::as_str);
        if source_base != target_base {
            return Err(AppError::from_error(G5Error::Internal {
                message: format!("paired graph common-base semantic block differs for {block_id}"),
            }));
        }
        let source_positions = common_base_change_anchors(
            source_base.unwrap_or_default(),
            source_after.get(block_id).map_or("", String::as_str),
            block_id,
        );
        let target_positions = common_base_change_anchors(
            target_base.unwrap_or_default(),
            target_after.get(block_id).map_or("", String::as_str),
            block_id,
        );
        overlap += source_positions.intersection(&target_positions).count();
    }
    Ok(overlap as i64)
}

fn snapshot_for_node<'a>(
    node: &ChangeGraphNode,
    snapshots: &'a [WorkspaceFileSnapshot],
) -> Option<&'a WorkspaceFileSnapshot> {
    let path = node.route.file_path.as_deref()?;
    snapshots
        .iter()
        .find(|snapshot| snapshot.repo_id == node.route.repo_id && snapshot.file == path)
}

fn semantic_block_content_by_id(content: Option<&[u8]>) -> HashMap<String, String> {
    content
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(semantic_blocks)
        .unwrap_or_default()
        .into_iter()
        .map(|block| (block.id, block.content))
        .collect()
}

fn common_base_change_anchors(
    before: &str,
    after: &str,
    block_id: &str,
) -> BTreeSet<CommonBaseLineAnchor> {
    let diff_text = unified_diff_buffers(
        before.as_bytes(),
        after.as_bytes(),
        &format!("semantic-block-{block_id}"),
    )
    .unwrap_or_default();
    let (hunks, _, _) = parse_task_unified_diff(&diff_text);
    let mut anchors = BTreeSet::new();
    for hunk in hunks {
        let mut old_cursor = hunk.start_line;
        let mut insertion_ordinals: HashMap<i64, usize> = HashMap::new();
        let mut deleted_since_context = false;
        for line in hunk.lines {
            if line.content == "\\ No newline at end of file" {
                continue;
            }
            match line.line_type.as_str() {
                "deletion" => {
                    let old_line = line.old_line.unwrap_or(old_cursor);
                    anchors.insert(CommonBaseLineAnchor::ExistingLine(old_line));
                    old_cursor = old_line.saturating_add(1);
                    deleted_since_context = true;
                }
                "addition" => {
                    if !deleted_since_context {
                        let ordinal = insertion_ordinals.entry(old_cursor).or_default();
                        anchors.insert(CommonBaseLineAnchor::Insertion {
                            before_line: old_cursor,
                            ordinal: *ordinal,
                        });
                        *ordinal += 1;
                    }
                }
                _ => {
                    if let Some(old_line) = line.old_line {
                        old_cursor = old_line.saturating_add(1);
                    }
                    deleted_since_context = false;
                }
            }
        }
    }
    anchors
}

impl ChangeGraphNodeDetail {
    fn variant_name(&self) -> &'static str {
        match self {
            Self::FileDiff { .. } => "file_diff",
            Self::SemanticDiff { .. } => "semantic_diff",
            Self::UnchangedContext => "unchanged_context",
        }
    }

    fn is_truncated(&self) -> bool {
        match self {
            Self::FileDiff { diff } => diff.truncated,
            Self::SemanticDiff {
                records,
                other_file_changes,
                truncated,
            } => {
                *truncated
                    || records.iter().any(|record| record.diff.truncated)
                    || other_file_changes
                        .as_ref()
                        .is_some_and(|diff| diff.truncated)
            }
            Self::UnchangedContext => false,
        }
    }
}

fn build_node_detail(
    workspace_id: Uuid,
    node: &ChangeGraphNode,
    snapshots: &[WorkspaceFileSnapshot],
    human_intent: bool,
) -> ChangeGraphNodeDetail {
    if node.change_state == ChangeState::Unchanged {
        return ChangeGraphNodeDetail::UnchangedContext;
    }
    let snapshot = node.route.file_path.as_deref().and_then(|path| {
        snapshots
            .iter()
            .find(|snapshot| snapshot.repo_id == node.route.repo_id && snapshot.file == path)
    });
    if node.route.file_type == FileType::Ontology.to_string() {
        return build_ontology_detail(snapshot, human_intent);
    }
    if is_current_workspace_project_doc_path(
        &node.route.file_type,
        node.route.file_path.as_deref(),
        workspace_id,
    ) {
        return build_semantic_detail(snapshot, node, human_intent);
    }
    ChangeGraphNodeDetail::FileDiff {
        diff: snapshot.map_or_else(unavailable_diff, diff_from_snapshot),
    }
}

/// Changed statements and the file-level Other changes note for one ontology
/// file (`g5.diff-graph.ontology-nodes`). A refused file shows its file diff.
fn build_ontology_detail(
    snapshot: Option<&WorkspaceFileSnapshot>,
    human_intent: bool,
) -> ChangeGraphNodeDetail {
    let Some(snapshot) = snapshot else {
        return ChangeGraphNodeDetail::SemanticDiff {
            records: Vec::new(),
            other_file_changes: Some(unavailable_diff()),
            truncated: false,
        };
    };
    let Ok(comparison) = compare_snapshot(snapshot) else {
        return ChangeGraphNodeDetail::FileDiff {
            diff: diff_from_snapshot(snapshot),
        };
    };
    let mut budget = DetailDiffBudget::default();
    let mut truncated = false;
    let mut records = Vec::new();
    for change in &comparison.changes {
        let shown = change.after.as_ref().or(change.before.as_ref());
        let is_human = shown.is_some_and(|statement| statement.human);
        if human_intent && !is_human {
            continue;
        }
        let diff = semantic_record_diff_from_text_with_budget(
            change
                .before
                .as_ref()
                .map_or("", |statement| statement.source.as_str()),
            change
                .after
                .as_ref()
                .map_or("", |statement| statement.source.as_str()),
            &format!("{}#{}", snapshot.file, change.id),
            &mut budget,
        );
        truncated |= diff.truncated;
        records.push(SemanticRecordDiff {
            id: change.id.clone(),
            record_kind: shown.map_or_else(String::new, |statement| statement.kind.to_string()),
            title: String::new(),
            change_state: statement_change_state(change.change),
            is_human,
            author: None,
            author_source: None,
            timestamp: None,
            timestamp_source: None,
            diff,
        });
    }
    let other_file_changes = comparison.other_text_changed().then(|| {
        diff_from_text_with_budget(
            &comparison.before_residual,
            &comparison.after_residual,
            &format!("{}#other", snapshot.file),
            &mut budget,
        )
    });
    truncated |= other_file_changes
        .as_ref()
        .is_some_and(|diff| diff.truncated);
    ChangeGraphNodeDetail::SemanticDiff {
        records,
        other_file_changes,
        truncated,
    }
}

fn is_current_workspace_project_doc_path(
    file_type: &str,
    file_path: Option<&str>,
    workspace_id: Uuid,
) -> bool {
    if file_type != FileType::ExternalDoc.to_string() {
        return false;
    }
    let Some(path) = file_path else {
        return false;
    };
    let prefix = format!("docs/{workspace_id}/");
    let Some(relative) = path.strip_prefix(&prefix) else {
        return false;
    };
    !relative.is_empty()
        && relative.ends_with(".md")
        && relative
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && !g5_contracts::types::is_managed_explainer_path(path)
}

fn build_semantic_detail(
    snapshot: Option<&WorkspaceFileSnapshot>,
    node: &ChangeGraphNode,
    human_intent: bool,
) -> ChangeGraphNodeDetail {
    let Some(snapshot) = snapshot else {
        return ChangeGraphNodeDetail::SemanticDiff {
            records: Vec::new(),
            other_file_changes: Some(unavailable_diff()),
            truncated: false,
        };
    };
    let base = snapshot
        .base_content
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    let current = snapshot
        .content
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    if snapshot
        .base_content
        .as_ref()
        .is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
        || snapshot
            .content
            .as_ref()
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
    {
        return ChangeGraphNodeDetail::SemanticDiff {
            records: Vec::new(),
            other_file_changes: Some(unavailable_diff_with(
                ChangeGraphDiffAvailability::Binary,
                snapshot.additions,
                snapshot.deletions,
            )),
            truncated: false,
        };
    }
    if snapshot
        .base_content
        .as_ref()
        .map_or(0, Vec::len)
        .saturating_add(snapshot.content.as_ref().map_or(0, Vec::len))
        > MAX_FILE_DIFF_INPUT_BYTES
    {
        return ChangeGraphNodeDetail::SemanticDiff {
            records: Vec::new(),
            other_file_changes: Some(unavailable_diff_with(
                ChangeGraphDiffAvailability::Oversized,
                snapshot.additions,
                snapshot.deletions,
            )),
            truncated: false,
        };
    }

    let before_blocks = semantic_blocks(base.unwrap_or_default());
    let after_blocks = semantic_blocks(current.unwrap_or_default());
    let before_by_id: HashMap<&str, &SemanticBlock> = before_blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let after_by_id: HashMap<&str, &SemanticBlock> = after_blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let mut ids: BTreeSet<String> = before_by_id
        .keys()
        .chain(after_by_id.keys())
        .map(|id| (*id).to_string())
        .collect();
    let mut ordered_ids = Vec::new();
    for block in &after_blocks {
        if ids.remove(&block.id) {
            ordered_ids.push(block.id.clone());
        }
    }
    for block in &before_blocks {
        if ids.remove(&block.id) {
            ordered_ids.push(block.id.clone());
        }
    }

    // Provenance joined onto the topology `ChangedRecord`s for this node. Under
    // the Human-intent lens the topology already restricted these to human
    // records, so non-human semantic records are dropped here as well.
    let changed_by_id: HashMap<&str, &ChangedRecord> = node
        .changed_records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect();
    let mut records = Vec::new();
    let mut globally_truncated = false;
    let mut budget = DetailDiffBudget::default();
    for id in ordered_ids {
        let before = before_by_id.get(id.as_str()).copied();
        let after = after_by_id.get(id.as_str()).copied();
        if before.map(|block| block.content.as_str()) == after.map(|block| block.content.as_str()) {
            continue;
        }
        let record_meta = changed_by_id.get(id.as_str()).copied();
        let is_human = record_meta.is_some_and(|record| record.is_human);
        if human_intent && !is_human {
            continue;
        }
        let (change_state, display) = match (before, after) {
            (None, Some(after)) => (ChangeState::Added, after),
            (Some(before), None) => (ChangeState::Deleted, before),
            (Some(_), Some(after)) => (ChangeState::Modified, after),
            (None, None) => continue,
        };
        let mut diff = semantic_record_diff_from_text_with_budget(
            before
                .map(|block| block.content.as_str())
                .unwrap_or_default(),
            after
                .map(|block| block.content.as_str())
                .unwrap_or_default(),
            &format!("{}#{id}", snapshot.file),
            &mut budget,
        );
        globally_truncated |= diff.truncated;
        records.push(SemanticRecordDiff {
            id: id.clone(),
            record_kind: id.chars().next().unwrap_or('I').to_string(),
            title: display.title.clone(),
            change_state,
            is_human,
            author: None,
            author_source: None,
            timestamp: None,
            timestamp_source: None,
            diff: {
                diff.additions = diff.additions.max(0);
                diff.deletions = diff.deletions.max(0);
                diff
            },
        });
    }

    let before_residual = residual_semantic_text(base.unwrap_or_default(), &before_blocks);
    let after_residual = residual_semantic_text(current.unwrap_or_default(), &after_blocks);
    let other_file_changes = (before_residual != after_residual).then(|| {
        diff_from_text_with_budget(
            &before_residual,
            &after_residual,
            &format!("{}#other", snapshot.file),
            &mut budget,
        )
    });
    globally_truncated |= other_file_changes
        .as_ref()
        .is_some_and(|diff| diff.truncated);
    ChangeGraphNodeDetail::SemanticDiff {
        records,
        other_file_changes,
        truncated: globally_truncated,
    }
}

fn semantic_blocks(text: &str) -> Vec<SemanticBlock> {
    MarkdownAnchorExtractor::extract(text)
        .blocks
        .into_iter()
        .enumerate()
        .filter_map(|(source_order, block)| {
            text.get(block.byte_range.clone())
                .map(|content| SemanticBlock {
                    id: block.identifier,
                    title: block.title,
                    content: content.trim().to_string(),
                    byte_range: block.byte_range,
                    source_order,
                })
        })
        .collect()
}

fn residual_semantic_text(text: &str, blocks: &[SemanticBlock]) -> String {
    let mut blocks = blocks.iter().collect::<Vec<_>>();
    blocks.sort_by_key(|block| (block.byte_range.start, block.source_order));
    let mut out = String::new();
    let mut cursor = 0usize;
    for block in blocks {
        if block.byte_range.start >= cursor {
            out.push_str(&text[cursor..block.byte_range.start]);
            cursor = block.byte_range.end;
        }
    }
    out.push_str(&text[cursor..]);
    collapse_blank_lines(&out)
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::new();
    let mut blank = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            if blank {
                continue;
            }
            blank = true;
        } else {
            blank = false;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn diff_from_snapshot(snapshot: &WorkspaceFileSnapshot) -> ChangeGraphDiffEvidence {
    let base_len = snapshot.base_content.as_ref().map_or(0, Vec::len);
    let draft_len = snapshot.content.as_ref().map_or(0, Vec::len);
    if base_len.saturating_add(draft_len) > MAX_FILE_DIFF_INPUT_BYTES {
        return unavailable_diff_with(
            ChangeGraphDiffAvailability::Oversized,
            snapshot.additions,
            snapshot.deletions,
        );
    }
    if snapshot
        .base_content
        .as_ref()
        .is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
        || snapshot
            .content
            .as_ref()
            .is_some_and(|bytes| std::str::from_utf8(bytes).is_err())
    {
        return unavailable_diff_with(
            ChangeGraphDiffAvailability::Binary,
            snapshot.additions,
            snapshot.deletions,
        );
    }
    bounded_diff(
        snapshot.hunks.clone(),
        snapshot.additions,
        snapshot.deletions,
    )
}

fn diff_from_text_with_budget(
    before: &str,
    after: &str,
    path: &str,
    budget: &mut DetailDiffBudget,
) -> ChangeGraphDiffEvidence {
    let diff_text =
        unified_diff_buffers(before.as_bytes(), after.as_bytes(), path).unwrap_or_default();
    let (hunks, additions, deletions) = parse_task_unified_diff(&diff_text);
    bounded_diff_with_budget(hunks, additions, deletions, budget)
}

fn semantic_record_diff_from_text_with_budget(
    before: &str,
    after: &str,
    path: &str,
    budget: &mut DetailDiffBudget,
) -> ChangeGraphDiffEvidence {
    let diff_text = unified_diff_buffers_with_context(
        before.as_bytes(),
        after.as_bytes(),
        path,
        DiffContext::Full,
    )
    .unwrap_or_default();
    let (hunks, additions, deletions) = parse_task_unified_diff(&diff_text);
    bounded_diff_with_budget(hunks, additions, deletions, budget)
}

fn bounded_diff(hunks: Vec<DiffHunk>, additions: i64, deletions: i64) -> ChangeGraphDiffEvidence {
    let mut budget = DetailDiffBudget::default();
    bounded_diff_with_budget(hunks, additions, deletions, &mut budget)
}

fn bounded_diff_with_budget(
    hunks: Vec<DiffHunk>,
    additions: i64,
    deletions: i64,
    budget: &mut DetailDiffBudget,
) -> ChangeGraphDiffEvidence {
    let total_line_count = hunks.iter().map(|hunk| hunk.lines.len()).sum::<usize>();
    let mut output = Vec::new();
    let mut truncated = false;
    let mut partial_final_hunk = false;
    for hunk in hunks {
        let full_lines = hunk.lines.len();
        let full_bytes = hunk
            .lines
            .iter()
            .map(|line| line.content.len())
            .sum::<usize>();
        if full_lines <= budget.remaining_lines && full_bytes <= budget.remaining_bytes {
            budget.remaining_lines -= full_lines;
            budget.remaining_bytes -= full_bytes;
            output.push(hunk);
            continue;
        }
        let mut prefix: Vec<DiffLine> = Vec::new();
        for line in hunk.lines {
            if budget.remaining_lines == 0 || line.content.len() > budget.remaining_bytes {
                break;
            }
            budget.remaining_lines -= 1;
            budget.remaining_bytes -= line.content.len();
            prefix.push(line);
        }
        if !prefix.is_empty() {
            partial_final_hunk = prefix.len() < full_lines;
            output.push(DiffHunk {
                start_line: hunk.start_line,
                header: hunk.header,
                lines: prefix,
            });
        }
        truncated = true;
        break;
    }
    let returned_line_count = output.iter().map(|hunk| hunk.lines.len()).sum::<usize>();
    ChangeGraphDiffEvidence {
        availability: ChangeGraphDiffAvailability::Available,
        reason: truncated.then_some(ChangeGraphDiffReason::OutputLimitReached),
        additions,
        deletions,
        hunks: output,
        returned_line_count,
        omitted_line_count: total_line_count.saturating_sub(returned_line_count),
        truncated,
        partial_final_hunk,
    }
}

fn unavailable_diff() -> ChangeGraphDiffEvidence {
    unavailable_diff_with(ChangeGraphDiffAvailability::Unavailable, 0, 0)
}

fn unavailable_diff_with(
    availability: ChangeGraphDiffAvailability,
    additions: i64,
    deletions: i64,
) -> ChangeGraphDiffEvidence {
    let reason = match availability {
        ChangeGraphDiffAvailability::Available => None,
        ChangeGraphDiffAvailability::Binary => Some(ChangeGraphDiffReason::BinaryContent),
        ChangeGraphDiffAvailability::Oversized => Some(ChangeGraphDiffReason::InputLimitExceeded),
        ChangeGraphDiffAvailability::Unavailable => {
            Some(ChangeGraphDiffReason::RetainedEvidenceMissing)
        }
    };
    ChangeGraphDiffEvidence {
        availability,
        reason,
        additions,
        deletions,
        hunks: Vec::new(),
        returned_line_count: 0,
        omitted_line_count: 0,
        truncated: false,
        partial_final_hunk: false,
    }
}

fn short_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    format!("{:x}", digest)[..16].to_string()
}

fn bump(counts: &mut BTreeMap<String, usize>, code: &str) {
    *counts.entry(code.to_string()).or_default() += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> RepoContext {
        RepoContext {
            workspace_id: Uuid::from_u128(1),
            repo_id: Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes()),
            repo_name: name.to_string(),
            repo_slug: format!("{name}-slug"),
            commit: "head".to_string(),
            merge_base_commit: Some("base".to_string()),
            parent_head: Some("parent".to_string()),
        }
    }

    fn snapshot(
        repo: &RepoContext,
        path: &str,
        base: Option<&str>,
        current: Option<&str>,
    ) -> WorkspaceFileSnapshot {
        WorkspaceFileSnapshot {
            repo_id: repo.repo_id,
            base_commit: Some("base".into()),
            file: path.to_string(),
            base_content: base.map(|text| text.as_bytes().to_vec()),
            content: current.map(|text| text.as_bytes().to_vec()),
            additions: 1,
            deletions: 1,
            hunks: Vec::new(),
        }
    }

    fn page(statements: &[(&str, &str, &str)]) -> String {
        let mut body = String::from("# Page\n\n");
        let mut footer = String::from("## Metadata\n\n");
        for (id, text, provenance) in statements {
            body.push_str(&format!("<a id=\"{id}\"></a>\n[I · {id}]\n\n{text}\n\n"));
            footer.push_str(&format!("### {id}\n\n- {provenance}\n\n"));
        }
        format!("{body}{footer}")
    }

    #[test]
    fn uncommitted_only_is_refused_and_workspace_is_the_default() {
        assert_eq!(
            ChangeGraphComparison::default(),
            ChangeGraphComparison::Workspace
        );
    }

    #[test]
    fn classification_follows_the_column_rules_in_order() {
        let cases = [
            (
                ".g5/ontology/root.g5.md",
                false,
                PathKind::Ontology,
                ChangeGraphColumn::Ontology,
                None,
            ),
            (
                ".g5/ontology/features/a.g5.md",
                true,
                PathKind::Ontology,
                ChangeGraphColumn::Ontology,
                None,
            ),
            (
                ".g5/ontology/notes.md",
                false,
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                ".g5/chat_intent/ws/request.md",
                false,
                PathKind::Chat,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Conversations),
            ),
            (
                ".g5/chat_intent/ws/deep/more.md",
                true,
                PathKind::Chat,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Conversations),
            ),
            (
                ".g5/chat_intent/ws/data.json",
                false,
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                ".g5/pr_ontology_diff_design/topic/ontology-plan.md",
                false,
                PathKind::DesignPlan,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Documents),
            ),
            (
                ".g5/pr_ontology_diff_design/topic/notes.md",
                false,
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                ".g5/preview_profiles/app.yaml",
                false,
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                "docs/ws/questions/scope.g5q.json",
                true,
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                "docs/ws/requirements.md",
                true,
                PathKind::ProjectDoc,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Documents),
            ),
            (
                "docs/ws/questions/scope.g5q.json",
                false,
                PathKind::Code,
                ChangeGraphColumn::SourceCode,
                None,
            ),
            (
                "README.md",
                true,
                PathKind::Other,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                "views/pm.md",
                true,
                PathKind::View,
                ChangeGraphColumn::Intents,
                Some(IntentGroup::Other),
            ),
            (
                ".semlift/specs/root.semlift.md",
                false,
                PathKind::Code,
                ChangeGraphColumn::SourceCode,
                None,
            ),
            (
                "intent_record/ontology/x.jsonl",
                false,
                PathKind::Code,
                ChangeGraphColumn::SourceCode,
                None,
            ),
            (
                "src/lib.rs",
                false,
                PathKind::Code,
                ChangeGraphColumn::SourceCode,
                None,
            ),
        ];
        for (path, docs, kind, column, group) in cases {
            let class = classify_path(path, docs);
            assert_eq!(
                (class.kind, class.column, class.intent_group),
                (kind, column, group),
                "{path}"
            );
        }
        assert_eq!(
            ontology_group(".g5/ontology/features/a.g5.md").as_deref(),
            Some("features")
        );
        assert_eq!(ontology_group(".g5/ontology/root.g5.md"), None);
        assert_eq!(
            artifact_title(".g5/ontology/concepts/data_model.g5.md", PathKind::Ontology),
            "concepts/data_model"
        );
    }

    #[test]
    fn link_destinations_resolve_like_the_editor() {
        let code = repo("code");
        let docs = repo("docs");
        let repos = [code.clone(), docs.clone()];
        let source = ".g5/ontology/features/a.g5.md";
        let file = |repo: &RepoContext, path: &str, fragment: Option<&str>| LinkDestination::File {
            repo_id: repo.repo_id,
            path: path.to_string(),
            fragment: fragment.map(str::to_string),
        };
        assert_eq!(
            link_destination(code.repo_id, source, "b.g5.md#x.y", &repos),
            file(&code, ".g5/ontology/features/b.g5.md", Some("x.y"))
        );
        assert_eq!(
            link_destination(code.repo_id, source, "../root.g5.md", &repos),
            file(&code, ".g5/ontology/root.g5.md", None)
        );
        assert_eq!(
            link_destination(code.repo_id, source, "/src/lib.rs", &repos),
            file(&code, "src/lib.rs", None)
        );
        assert_eq!(
            link_destination(
                code.repo_id,
                source,
                "../../chat_intent/w/r.md#human",
                &repos
            ),
            file(&code, ".g5/chat_intent/w/r.md", Some("human"))
        );
        assert_eq!(
            link_destination(code.repo_id, source, "with%20space.g5.md", &repos),
            file(&code, ".g5/ontology/features/with space.g5.md", None)
        );
        assert_eq!(
            link_destination(
                code.repo_id,
                source,
                &format!("g5://file/docs/ws/req.md?repo={}", docs.repo_id),
                &repos
            ),
            file(&docs, "docs/ws/req.md", None)
        );
        assert_eq!(
            link_destination(
                code.repo_id,
                source,
                "g5://file/docs/ws/req.md?repo=docs",
                &repos
            ),
            file(&docs, "docs/ws/req.md", None)
        );
        assert_eq!(
            link_destination(code.repo_id, source, "g5://file/x.md?repo=unknown", &repos),
            LinkDestination::Invalid
        );
        assert_eq!(
            link_destination(code.repo_id, source, "../../../../../../escape.md", &repos),
            LinkDestination::Invalid
        );
        assert_eq!(
            link_destination(code.repo_id, source, "a.md?x=1", &repos),
            LinkDestination::Invalid
        );
        for ignored in [
            "#local",
            "https://github.com/G5Labs/G5-Lean/blob/abc/.g5/ontology/root.g5.md",
            "mailto:a@b.c",
            "//cdn.example/x",
            "",
        ] {
            assert_eq!(
                link_destination(code.repo_id, source, ignored, &repos),
                LinkDestination::Ignored,
                "{ignored}"
            );
        }
    }

    #[test]
    fn links_come_from_changed_statements_and_changed_links_only() {
        let before = page(&[
            ("a", "Cites [b](b.g5.md#b) and [old](old.g5.md).", "agent"),
            ("gone", "Cites [c](c.g5.md).", "agent"),
            ("same", "Cites [e](e.g5.md).", "agent"),
            (
                "footer",
                "Unchanged body.",
                "agent; [old chat](old-chat.md)",
            ),
        ]);
        let after = page(&[
            (
                "a",
                "Cites [b, renamed text](b.g5.md#b) and [new](new.g5.md).",
                "agent",
            ),
            ("added", "Cites [d](d.g5.md).", "agent"),
            ("same", "Cites [e](e.g5.md).", "agent"),
            (
                "footer",
                "Unchanged body.",
                "agent; [new chat](new-chat.md)",
            ),
        ]);
        let comparison = ontology::compare(Some(&before), Some(&after)).unwrap();
        let mut states: Vec<(String, String, ChangeState, LinkVersion)> =
            statement_links(&comparison)
                .into_iter()
                .map(|link| {
                    (
                        link.statement_id,
                        link.link.href,
                        link.change_state,
                        link.version,
                    )
                })
                .collect();
        states.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        assert_eq!(
            states,
            vec![
                (
                    "a".into(),
                    "b.g5.md#b".into(),
                    ChangeState::Unchanged,
                    LinkVersion::Current
                ),
                (
                    "a".into(),
                    "new.g5.md".into(),
                    ChangeState::Added,
                    LinkVersion::Current
                ),
                (
                    "a".into(),
                    "old.g5.md".into(),
                    ChangeState::Deleted,
                    LinkVersion::Baseline
                ),
                (
                    "added".into(),
                    "d.g5.md".into(),
                    ChangeState::Added,
                    LinkVersion::Current
                ),
                (
                    "footer".into(),
                    "new-chat.md".into(),
                    ChangeState::Added,
                    LinkVersion::Current
                ),
                (
                    "footer".into(),
                    "old-chat.md".into(),
                    ChangeState::Deleted,
                    LinkVersion::Baseline
                ),
                (
                    "gone".into(),
                    "c.g5.md".into(),
                    ChangeState::Deleted,
                    LinkVersion::Baseline
                ),
            ]
        );
    }

    #[test]
    fn fragments_resolve_only_to_exactly_one_anchor_like_the_editor() {
        let text = "# Title\n\n## Natural Section\n\n## Q & A\n\n## Notes\n\n## Notes\n\n<a id=\"x.y\"></a>\n[I · x.y]\n\nBody <a name=\"inline\"></a> and `<a id=\"code\"></a>`.\n\n<a id=\"dup\"></a>\n<a id=\"dup\"></a>\n\n~~~md\n<a id=\"fenced\"></a>\n~~~\n\n## Metadata\n\n### x.y\n\n- agent\n";
        let index = fragment_index(text);
        assert_eq!(index.target("x.y").as_deref(), Some("x.y"));
        assert_eq!(index.target("natural-section").as_deref(), Some(""));
        assert_eq!(index.target("q-and-a").as_deref(), Some(""));
        assert_eq!(index.target("notes").as_deref(), Some(""));
        assert_eq!(index.target("notes-2").as_deref(), Some(""));
        assert_eq!(index.target("inline").as_deref(), Some(""));
        for unresolved in ["dup", "code", "fenced", "x-y-z", "metadata-2"] {
            assert_eq!(index.target(unresolved), None, "{unresolved}");
        }
    }

    #[test]
    fn alias_anchor_fragments_resolve_to_their_statement() {
        let text = "# T\n\n<a id=\"x.a\"></a>\n[I · x.a]\n\nFirst.\n\n<a id=\"old-b\"></a>\n<a id=\"x.b\"></a>\n[I · x.b]\n\nSecond.\n\n## Metadata\n\n### x.a\n\n- agent\n\n### x.b\n\n- agent\n";
        let index = fragment_index(text);
        assert_eq!(index.target("x.b").as_deref(), Some("x.b"));
        assert_eq!(index.target("old-b").as_deref(), Some("x.b"));
        assert_eq!(index.target("x.a").as_deref(), Some("x.a"));
    }

    fn graph_artifact(
        repo: &RepoContext,
        path: &str,
        changed: bool,
        records: Vec<ChangedRecord>,
    ) -> ArtifactProjection {
        let class = classify_path(path, false);
        let mut artifact = file_artifact(
            repo,
            path,
            class,
            if changed {
                ChangeState::Modified
            } else {
                ChangeState::Unchanged
            },
        );
        artifact.changed = changed;
        artifact.changed_records = records;
        artifact
    }

    fn record(id: &str, human: bool) -> ChangedRecord {
        ChangedRecord {
            id: id.into(),
            label: id.into(),
            change_state: ChangeState::Modified,
            kind: Some("I".into()),
            is_human: human,
            content: None,
        }
    }

    fn link(
        source: &ArtifactProjection,
        target: &ArtifactProjection,
        relationship: &'static str,
    ) -> LinkEdge {
        LinkEdge {
            source: source.key.clone(),
            target: target.key.clone(),
            source_file: source.path.clone().unwrap(),
            source_id: "s".into(),
            target_file: target.path.clone().unwrap(),
            target_id: String::new(),
            relationship,
            change_state: ChangeState::Added,
            target_deleted: false,
        }
    }

    fn selection(human_intent: bool) -> GraphSelectionContext {
        GraphSelectionContext {
            diagnostic_counts: BTreeMap::new(),
            revision_hash: "0123456789abcdef0123".into(),
            comparison: ChangeGraphComparison::Workspace,
            range: ChangeGraphRange {
                base_kind: ChangeGraphBaseKind::MergeBase,
                parent_workspace_id: None,
                parent_workspace_name: None,
                repository_count: 1,
                live_content: Vec::new(),
            },
            human_intent,
        }
    }

    #[test]
    fn human_lens_keeps_ontology_files_with_human_statements_and_their_links() {
        let code = repo("code");
        let human = graph_artifact(
            &code,
            ".g5/ontology/a.g5.md",
            true,
            vec![record("h", true), record("x", false)],
        );
        let agent = graph_artifact(
            &code,
            ".g5/ontology/b.g5.md",
            true,
            vec![record("y", false)],
        );
        let source = graph_artifact(&code, "src/lib.rs", true, Vec::new());
        let context = graph_artifact(&code, "src/other.rs", false, Vec::new());
        let orphan_context = graph_artifact(&code, "src/orphan.rs", false, Vec::new());
        let edges = vec![
            link(&human, &context, "implementation"),
            link(&human, &source, "implementation"),
            link(&agent, &orphan_context, "implementation"),
        ];
        let artifacts: HashMap<_, _> = [human.clone(), agent, source, context, orphan_context]
            .into_iter()
            .map(|artifact| (artifact.key.clone(), artifact))
            .collect();

        let all = select_graph(artifacts.clone(), edges.clone(), selection(false));
        assert_eq!(all.nodes.len(), 5);
        assert_eq!(all.edges.len(), 3);

        let lensed = select_graph(artifacts, edges, selection(true));
        let paths: Vec<_> = lensed
            .nodes
            .iter()
            .map(|node| node.path.clone().unwrap())
            .collect();
        assert_eq!(
            paths,
            vec![
                ".g5/ontology/a.g5.md".to_string(),
                "src/lib.rs".to_string(),
                "src/other.rs".to_string(),
            ]
        );
        assert_eq!(lensed.nodes[0].changed_records.len(), 1);
        assert!(lensed.nodes[0].has_human_blocks);
        // A changed file a kept page links to stays as context with its state.
        assert_eq!(lensed.nodes[1].connection_state, ConnectionState::Bridge);
        assert_eq!(lensed.nodes[1].change_state, ChangeState::Modified);
        assert_eq!(lensed.nodes[2].connection_state, ConnectionState::Bridge);
        assert_eq!(lensed.edges.len(), 2);
    }

    #[test]
    fn context_nodes_are_budgeted_and_only_linked_targets_are_context() {
        let code = repo("code");
        let source = graph_artifact(
            &code,
            ".g5/ontology/a.g5.md",
            true,
            vec![record("a", false)],
        );
        let mut artifacts = HashMap::from([(source.key.clone(), source.clone())]);
        let mut edges = Vec::new();
        for n in 0..MAX_RELATIONSHIP_CONTEXT_NODES + 3 {
            let target = graph_artifact(&code, &format!("src/f{n:04}.rs"), false, Vec::new());
            edges.push(link(&source, &target, "implementation"));
            artifacts.insert(target.key.clone(), target);
        }
        let response = select_graph(artifacts, edges, selection(false));
        assert!(response.truncated);
        assert_eq!(response.nodes.len(), MAX_RELATIONSHIP_CONTEXT_NODES + 1);
        assert!(response
            .diagnostics
            .iter()
            .any(|d| d.code == "relationship_context_budget_reached"));
    }

    #[test]
    fn ontology_detail_lists_changed_statements_and_other_changes() {
        let code = repo("code");
        let before = page(&[
            ("a", "Old body.", "agent"),
            ("b", "Same.", "agent"),
            ("c", "Gone.", "human-provided"),
        ]);
        let after = page(&[
            ("a", "New body.", "human-verified"),
            ("b", "Same.", "agent"),
            ("d", "Added.", "agent"),
        ])
        .replace("# Page", "# Page renamed");
        let snap = snapshot(&code, ".g5/ontology/x.g5.md", Some(&before), Some(&after));
        let ChangeGraphNodeDetail::SemanticDiff {
            records,
            other_file_changes,
            ..
        } = build_ontology_detail(Some(&snap), false)
        else {
            panic!("semantic detail");
        };
        let rows: Vec<_> = records
            .iter()
            .map(|r| {
                (
                    r.id.as_str(),
                    r.change_state,
                    r.is_human,
                    r.record_kind.as_str(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("a", ChangeState::Modified, true, "I"),
                ("c", ChangeState::Deleted, true, "I"),
                ("d", ChangeState::Added, false, "I"),
            ]
        );
        assert!(
            other_file_changes.is_some(),
            "heading and metadata edits are other changes"
        );
        let ChangeGraphNodeDetail::SemanticDiff { records, .. } =
            build_ontology_detail(Some(&snap), true)
        else {
            panic!("semantic detail");
        };
        assert_eq!(
            records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }

    #[test]
    fn refused_ontology_file_shows_its_file_diff() {
        let code = repo("code");
        let broken = "# P\n\n<a id=\"x\"></a>\n[I · x]\n\nNo footer.\n";
        let snap = snapshot(&code, ".g5/ontology/x.g5.md", None, Some(broken));
        assert!(compare_snapshot(&snap).is_err());
        assert!(matches!(
            build_ontology_detail(Some(&snap), false),
            ChangeGraphNodeDetail::FileDiff { .. }
        ));
    }

    fn graph_node(
        id: &str,
        column: ChangeGraphColumn,
        state: ChangeState,
        records: &[&str],
    ) -> ChangeGraphNode {
        ChangeGraphNode {
            id: id.into(),
            column,
            intent_group: None,
            ontology_group: None,
            title: id.into(),
            artifact_type: "ontology".into(),
            path: Some(format!("{id}.g5.md")),
            change_state: state,
            change_origin: None,
            has_human_blocks: false,
            connection_state: ConnectionState::Unlinked,
            additions: None,
            deletions: None,
            changed_records: records.iter().map(|id| record(id, false)).collect(),
            comparison_warning: None,
            missing_targets: Vec::new(),
            route: ChangeGraphRoute {
                repo_id: Uuid::nil(),
                workspace_id: Uuid::nil(),
                file_path: None,
                file_type: "ontology".into(),
                record_id: None,
                session_id: None,
            },
        }
    }

    #[test]
    fn rebase_conflicts_are_same_statement_or_same_file_within_one_ontology_file() {
        let source = vec![
            graph_node(
                "same-statement",
                ChangeGraphColumn::Ontology,
                ChangeState::Modified,
                &["a", "b"],
            ),
            graph_node(
                "same-file",
                ChangeGraphColumn::Ontology,
                ChangeState::Modified,
                &["a"],
            ),
            graph_node(
                "code",
                ChangeGraphColumn::SourceCode,
                ChangeState::Modified,
                &[],
            ),
        ];
        let target = vec![
            graph_node(
                "same-statement",
                ChangeGraphColumn::Ontology,
                ChangeState::Modified,
                &["b"],
            ),
            graph_node(
                "same-file",
                ChangeGraphColumn::Ontology,
                ChangeState::Modified,
                &["z"],
            ),
            graph_node(
                "code",
                ChangeGraphColumn::SourceCode,
                ChangeState::Modified,
                &[],
            ),
        ];
        let conflicts = classify_rebase_conflicts(&source, &target);
        let kinds: Vec<_> = conflicts
            .iter()
            .map(|c| {
                (
                    c.artifact_key.as_str(),
                    c.kind,
                    c.overlapping_block_ids.clone(),
                )
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("same-file", RebaseChangeGraphConflictKind::SameNode, vec![]),
                (
                    "same-statement",
                    RebaseChangeGraphConflictKind::SameBlock,
                    vec!["b".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn workspace_comparison_limits_accept_exact_boundaries_and_reject_one_over() {
        assert!(enforce_workspace_comparison_limits(
            MAX_WORKSPACE_CHANGED_ROOTS,
            MAX_WORKSPACE_COMPARISON_INPUT_BYTES
        )
        .is_ok());
        assert!(enforce_workspace_comparison_limits(MAX_WORKSPACE_CHANGED_ROOTS + 1, 0).is_err());
        assert!(
            enforce_workspace_comparison_limits(0, MAX_WORKSPACE_COMPARISON_INPUT_BYTES + 1)
                .is_err()
        );
    }

    #[test]
    fn detail_diff_budget_is_shared_and_marks_a_partial_final_hunk() {
        let mut budget = DetailDiffBudget {
            remaining_lines: 3,
            remaining_bytes: MAX_DETAIL_DIFF_BYTES,
        };
        let first = diff_from_text_with_budget("a\n", "b\n", "x", &mut budget);
        assert!(!first.truncated);
        let second = diff_from_text_with_budget("c\nd\n", "e\nf\n", "y", &mut budget);
        assert!(second.truncated);
        assert!(second.partial_final_hunk);
        assert_eq!(second.returned_line_count, 1);
    }
}
