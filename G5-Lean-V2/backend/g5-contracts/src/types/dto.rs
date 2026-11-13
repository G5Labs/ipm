//! Miscellaneous DTOs: `DiffSummary`, `DiffFileSummary`, `DiffFileStatus`,
//! `TreeEntry`, `TreeEntryType`.
//!
//! These types are used for git diff and tree listing operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::task_context::RepoRole;

// ---------------------------------------------------------------------------
// Authority targets and committed cuts
// ---------------------------------------------------------------------------

/// The authoritative cut selected by a read or a landing.
///
/// Every target names its durable scope identity. The server resolves all
/// repository members through the normalized authority relation; no target
/// carries a representative repository branch or caller-selected branch name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorityTarget {
    Workspace { workspace_id: Uuid },
    Task { workspace_id: Uuid, task_id: Uuid },
}

impl AuthorityTarget {
    #[must_use]
    pub fn workspace_id(&self) -> Uuid {
        match self {
            Self::Workspace { workspace_id, .. } | Self::Task { workspace_id, .. } => *workspace_id,
        }
    }

    #[must_use]
    pub fn kind(&self) -> AuthorityTargetKind {
        match self {
            Self::Workspace { .. } => AuthorityTargetKind::Workspace,
            Self::Task { .. } => AuthorityTargetKind::Task,
        }
    }

    #[must_use]
    pub fn target_id(&self) -> Uuid {
        match self {
            Self::Workspace { workspace_id } => *workspace_id,
            Self::Task { task_id, .. } => *task_id,
        }
    }

    #[must_use]
    pub fn task_id(&self) -> Option<Uuid> {
        match self {
            Self::Task { task_id, .. } => Some(*task_id),
            Self::Workspace { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityTargetKind {
    Workspace,
    Task,
}

impl AuthorityTargetKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Task => "task",
        }
    }
}

/// One exact repository version in a typed committed binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoBindingVersion {
    pub repo_id: Uuid,
    pub repo_name: String,
    #[serde(default)]
    pub repo_slug: String,
    #[serde(default)]
    pub repo_role: RepoRole,
    pub workspace_id: Uuid,
    pub branch_name: String,
    #[serde(default)]
    pub tracking_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    pub canonical_ref: String,
    /// The commit `canonical_ref` pointed at in the internal mirror when the
    /// cut was captured, or the immutable flush snapshot selected for this read.
    pub head_commit: String,
    pub root_path: String,
}

/// A complete current committed cut: membership from one scoped database
/// read, every head from the member's branch ref.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DurableSnapshotSource {
    #[default]
    Committed,
    Flush,
}

impl DurableSnapshotSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Flush => "flush",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommittedBindingSnapshot {
    pub target: AuthorityTarget,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    /// Time at which the exact repository vector became observable. For a
    /// durable flush this is its completion time; for branch-ref reads this is
    /// the time the refs were resolved.
    pub snapshot_at: DateTime<Utc>,
    #[serde(default)]
    pub source: DurableSnapshotSource,
    pub repos: Vec<RepoBindingVersion>,
}

impl CommittedBindingSnapshot {
    pub fn repo(&self, repo_id: Uuid) -> Result<&RepoBindingVersion, &'static str> {
        self.repos
            .iter()
            .find(|repo| repo.repo_id == repo_id)
            .ok_or("repository is not a member of the committed binding")
    }
}

/// Metadata for a connected view over one committed binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedBindingView {
    pub snapshot: CommittedBindingSnapshot,
    pub live_generation: Option<u64>,
}

/// Reviewed purpose for an explicit immutable Git version read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoricalReadReason {
    BaseBinding,
    CommitHistory,
    Diff,
    CommentLineMapping,
    BlameAttribution,
    OperationValidation,
}

/// One explicitly authorized historical repository version.
///
/// This type can answer a version/history question; it must never be used to
/// advertise current workspace or task state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalGitVersion {
    pub repo_id: Uuid,
    pub head_commit: String,
    pub reason: HistoricalReadReason,
}

/// One repository member of a complete cut a producer wants landed: the head
/// its branch ref carried when the producer computed its result and the head
/// it should carry afterwards (an all-zero expectation initializes an unborn
/// member).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceHeadRepoInput {
    pub repo_id: Uuid,
    pub expected_old_head: String,
    pub proposed_new_head: String,
}

/// Server-selected transition for one repository member of a landed cut.
///
/// `Initialize` materializes an unborn member (its branch ref did not exist);
/// every other landing of a member produces `Advance`. `Reseed` replaces a
/// private task cut for a fresh execution retry and `Rebase` lands a
/// server-selected rebased result; both still compare-and-swap the member's
/// branch ref from the head the producer expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceHeadTransitionKind {
    Advance,
    Initialize,
    Reseed,
    Rebase,
}

impl WorkspaceHeadTransitionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Advance => "advance",
            Self::Initialize => "initialize",
            Self::Reseed => "reseed",
            Self::Rebase => "rebase",
        }
    }
}

// ---------------------------------------------------------------------------
// DiffFileStatus
// ---------------------------------------------------------------------------

/// Status of a file in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DiffFileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

// ---------------------------------------------------------------------------
// DiffFileSummary
// ---------------------------------------------------------------------------

/// Per-file diff statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileSummary {
    /// File path relative to repository root.
    pub path: String,

    /// Number of lines added.
    pub additions: usize,

    /// Number of lines deleted.
    pub deletions: usize,

    /// File status in the diff.
    pub status: DiffFileStatus,
}

// ---------------------------------------------------------------------------
// DiffSummary
// ---------------------------------------------------------------------------

/// Summary of changes between two git commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    /// Per-file diff statistics.
    pub files: Vec<DiffFileSummary>,

    /// Total lines added across all files.
    pub total_additions: usize,

    /// Total lines deleted across all files.
    pub total_deletions: usize,
}

// ---------------------------------------------------------------------------
// TreeEntryType
// ---------------------------------------------------------------------------

/// Type of entry in a git tree listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TreeEntryType {
    File,
    Directory,
}

// ---------------------------------------------------------------------------
// TreeEntry
// ---------------------------------------------------------------------------

/// Entry in a git tree listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    /// File or directory name.
    pub name: String,

    /// Path relative to repository root.
    pub path: String,

    /// Whether this is a file or directory.
    pub entry_type: TreeEntryType,

    /// Size in bytes (None for directories).
    pub size: Option<u64>,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
        )
    }

    #[test]
    fn authority_targets_roundtrip_with_explicit_identity() {
        let (workspace_id, _, _unused, task_id) = ids();
        let targets = [
            AuthorityTarget::Workspace { workspace_id },
            AuthorityTarget::Task {
                workspace_id,
                task_id,
            },
        ];

        for target in targets {
            let value = serde_json::to_value(&target).expect("serialize authority target");
            let decoded: AuthorityTarget =
                serde_json::from_value(value.clone()).expect("deserialize authority target");
            assert_eq!(decoded, target);
            assert_eq!(decoded.workspace_id(), workspace_id);
            assert_eq!(value["kind"], decoded.kind().as_str());
        }

        let task = AuthorityTarget::Task {
            workspace_id,
            task_id,
        };
        assert_eq!(task.target_id(), task_id);
        assert_eq!(task.task_id(), Some(task_id));
    }

    #[test]
    fn workspace_head_rebase_transition_roundtrips_with_exact_wire_value() {
        let transition = WorkspaceHeadTransitionKind::Rebase;
        let value = serde_json::to_value(transition).expect("serialize rebase transition");
        assert_eq!(value, serde_json::Value::String("rebase".to_string()));
        assert_eq!(
            serde_json::from_value::<WorkspaceHeadTransitionKind>(value)
                .expect("deserialize rebase transition"),
            transition
        );
        assert_eq!(transition.as_str(), "rebase");
    }

    #[test]
    fn committed_binding_lookup_never_falls_back_to_another_repo() {
        let (workspace_id, _, _, _) = ids();
        let repo_id = Uuid::from_u128(10);
        let snapshot = CommittedBindingSnapshot {
            target: AuthorityTarget::Workspace { workspace_id },
            org_id: Uuid::from_u128(11),
            workspace_id,
            snapshot_at: Utc::now(),
            source: DurableSnapshotSource::Committed,
            repos: vec![RepoBindingVersion {
                repo_id,
                repo_name: "code".to_string(),
                repo_slug: "code".to_string(),
                repo_role: RepoRole::Code,
                workspace_id,
                branch_name: "main".to_string(),
                tracking_mode: "pinned".to_string(),
                remote_branch: None,
                canonical_ref: "refs/heads/main".to_string(),
                head_commit: "c".repeat(40),
                root_path: "code".to_string(),
            }],
        };

        assert_eq!(
            snapshot.repo(repo_id).expect("bound repo").repo_name,
            "code"
        );
        assert!(snapshot.repo(Uuid::from_u128(13)).is_err());
    }

    // -- DiffFileStatus tests --

    #[test]
    fn test_diff_file_status_serde() {
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Added).unwrap(),
            "\"added\""
        );
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Modified).unwrap(),
            "\"modified\""
        );
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Deleted).unwrap(),
            "\"deleted\""
        );
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Renamed).unwrap(),
            "\"renamed\""
        );

        let parsed: DiffFileStatus = serde_json::from_str("\"renamed\"").unwrap();
        assert_eq!(parsed, DiffFileStatus::Renamed);
    }

    // -- DiffFileSummary tests --

    #[test]
    fn test_diff_file_summary_serde_camel_case() {
        let dfs = DiffFileSummary {
            path: "src/main.rs".to_string(),
            additions: 10,
            deletions: 5,
            status: DiffFileStatus::Modified,
        };
        let json = serde_json::to_value(&dfs).unwrap();
        assert!(json.get("path").is_some());
        assert!(json.get("additions").is_some());
        assert!(json.get("deletions").is_some());
        assert!(json.get("status").is_some());
        assert_eq!(json["status"], "modified");
    }

    // -- DiffSummary tests --

    #[test]
    fn test_diff_summary_serde_camel_case() {
        let ds = DiffSummary {
            files: vec![DiffFileSummary {
                path: "src/lib.rs".to_string(),
                additions: 20,
                deletions: 3,
                status: DiffFileStatus::Added,
            }],
            total_additions: 20,
            total_deletions: 3,
        };
        let json = serde_json::to_value(&ds).unwrap();
        assert!(json.get("files").is_some());
        assert!(json.get("totalAdditions").is_some());
        assert!(json.get("totalDeletions").is_some());
        // No snake_case
        assert!(json.get("total_additions").is_none());
        assert!(json.get("total_deletions").is_none());
    }

    #[test]
    fn test_diff_summary_roundtrip() {
        let ds = DiffSummary {
            files: vec![
                DiffFileSummary {
                    path: "src/a.rs".to_string(),
                    additions: 10,
                    deletions: 2,
                    status: DiffFileStatus::Modified,
                },
                DiffFileSummary {
                    path: "src/b.rs".to_string(),
                    additions: 5,
                    deletions: 0,
                    status: DiffFileStatus::Added,
                },
            ],
            total_additions: 15,
            total_deletions: 2,
        };
        let json = serde_json::to_string(&ds).unwrap();
        let parsed: DiffSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.total_additions, 15);
        assert_eq!(parsed.total_deletions, 2);
    }

    // -- TreeEntryType tests --

    #[test]
    fn test_tree_entry_type_serde() {
        assert_eq!(
            serde_json::to_string(&TreeEntryType::File).unwrap(),
            "\"file\""
        );
        assert_eq!(
            serde_json::to_string(&TreeEntryType::Directory).unwrap(),
            "\"directory\""
        );
        let parsed: TreeEntryType = serde_json::from_str("\"directory\"").unwrap();
        assert_eq!(parsed, TreeEntryType::Directory);
    }

    // -- TreeEntry tests --

    #[test]
    fn test_tree_entry_serde_camel_case() {
        let te = TreeEntry {
            name: "main.rs".to_string(),
            path: "src/main.rs".to_string(),
            entry_type: TreeEntryType::File,
            size: Some(1024),
        };
        let json = serde_json::to_value(&te).unwrap();
        assert!(json.get("name").is_some());
        assert!(json.get("path").is_some());
        assert!(json.get("entryType").is_some());
        assert!(json.get("size").is_some());
        // No snake_case
        assert!(json.get("entry_type").is_none());
        assert_eq!(json["entryType"], "file");
    }

    #[test]
    fn test_tree_entry_directory_no_size() {
        let te = TreeEntry {
            name: "src".to_string(),
            path: "src".to_string(),
            entry_type: TreeEntryType::Directory,
            size: None,
        };
        let json = serde_json::to_value(&te).unwrap();
        assert!(json["size"].is_null());
        assert_eq!(json["entryType"], "directory");
    }

    #[test]
    fn test_tree_entry_roundtrip() {
        let te = TreeEntry {
            name: "lib.rs".to_string(),
            path: "src/lib.rs".to_string(),
            entry_type: TreeEntryType::File,
            size: Some(2048),
        };
        let json = serde_json::to_string(&te).unwrap();
        let parsed: TreeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "lib.rs");
        assert_eq!(parsed.entry_type, TreeEntryType::File);
        assert_eq!(parsed.size, Some(2048));
    }
}
