//! Immutable committed-repository reads.
//!
//! Callers must select the repository and commit from an authoritative
//! workspace snapshot before entering this module. These helpers deliberately
//! never fall back to the mutable repository checkout: doing so would let a
//! read advertised at one commit observe files from another commit.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::services::git::repo_store::{OrgScope, RepoKey, ScopedRepoStorage, TenantRepoStorage};
use chrono::{DateTime, Utc};
use g5_authz::ScopedDb;
use g5_contracts::error::G5Error;
use g5_contracts::types::{
    AuthorityTarget, CommittedBindingSnapshot, ConnectedBindingView, DurableSnapshotSource,
    HistoricalGitVersion, RepoBindingVersion, RepoRole,
};
use g5_internal_api::workspace_flush::FlushRepoReceipt;
use g5_scm::{
    entry_bytes, BlameAttribution, CommitDiffSummary, CommitFileDiff, CommitHistoryPage,
    CommitSummary, DiffSummary, GitRepo, TreeEntry, TreeEntryType, TreeListLimits, TreeListing,
    TreePaths,
};
use sqlx::PgConnection;
use uuid::Uuid;

/// The most entries one Files tree listing carries, committed or live: the
/// cap the workspace runtime's `fs.tree` reply is asked for, applied to the
/// committed cut as well so a reader's tree never grows with the commit.
pub const FILE_TREE_ENTRY_CAP: usize = 10_000;

/// The most bytes of entries one Files tree listing is budgeted at
/// (`g5_scm::entry_bytes`), committed or live. The runtime accepts replies
/// up to 4 MiB; 3 MiB leaves the envelope and the per-entry estimate's slack
/// under it, and the committed cut keeps to the same budget.
pub const FILE_TREE_BYTE_CAP: usize = 3 * 1024 * 1024;

/// The bounds of one Files tree listing (`FILE_TREE_ENTRY_CAP`,
/// `FILE_TREE_BYTE_CAP`): a listing past them is served in part and says so.
pub const FILE_TREE_LIMITS: TreeListLimits = TreeListLimits {
    max_entries: FILE_TREE_ENTRY_CAP,
    max_bytes: FILE_TREE_BYTE_CAP,
};

/// The most bytes one file read materializes, committed or live: the
/// workspace runtime refuses a larger live file (`fs.read`,
/// `g5d_dispatch::files::MAX_FILE_BYTES`), and a committed file past it is
/// refused from its object header before any content is loaded, so a blob
/// of any size in a repository costs a reader no more than this.
pub const MAX_FILE_READ_BYTES: usize = 32 * 1024 * 1024;

/// The ceiling of a committed enumeration a service performs for itself
/// (spec, record and report directories, source archives): far above any
/// repository the product handles, and refused as [`G5Error::TreeTooLarge`]
/// rather than answered in part, so no consumer mistakes a cut-off listing
/// for the complete tree.
pub const COMMITTED_ENUMERATION_LIMITS: TreeListLimits = TreeListLimits {
    max_entries: 250_000,
    max_bytes: 32 * 1024 * 1024,
};

#[derive(Debug, sqlx::FromRow)]
struct BindingRepoRow {
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    repo_name: String,
    repo_slug: String,
    is_docs_repo: bool,
    is_base: bool,
    /// Base, or a personal workspace before its first commit
    /// (`g5.personal.lifecycle`): a member whose branch may be unborn.
    #[sqlx(default)]
    may_be_unborn: bool,
    branch_name: String,
    remote_branch: Option<String>,
    root_path: String,
}

/// One member of a structurally complete typed authority inventory.
///
/// Unlike a committed binding, an inventory may contain an explicitly unborn
/// member. Callers may use it only to validate or prepare first
/// materialization; executable reads must still resolve a committed snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorityInventoryRepo {
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub branch_name: String,
    pub is_base: bool,
    /// The member's branch may be unborn and first materialized: Base, and a
    /// personal workspace's private repository (`g5.personal.lifecycle`).
    pub may_be_unborn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorityInventorySnapshot {
    pub target: AuthorityTarget,
    pub org_id: Uuid,
    pub workspace_id: Uuid,
    pub repos: Vec<AuthorityInventoryRepo>,
}

/// A complete connected authority binding with live overlays indexed by repo.
///
/// The complete committed cut is captured once. Batch/list readers can then
/// read any selected repository without re-resolving authority or assembling
/// a mixed-time vector.
pub struct ResolvedConnectedBindingView {
    pub view: ConnectedBindingView,
    repos: BTreeMap<Uuid, RepoReadView>,
}

impl ResolvedConnectedBindingView {
    pub fn repo(&self, repo_id: Uuid) -> Result<RepoReadView, G5Error> {
        self.repos.get(&repo_id).cloned().ok_or_else(|| {
            authority_error(
                "repository was not selected in the connected binding view",
                &self.view.snapshot.target,
            )
        })
    }

    /// Iterate the repository views captured from this one authority cut.
    /// Callers must not use this as permission to re-resolve any member.
    pub fn repos(&self) -> impl Iterator<Item = (&Uuid, &RepoReadView)> {
        self.repos.iter()
    }
}

/// One immutable repository version authorized by a typed current binding,
/// committed operation, or explicit historical request.
///
/// This is the only production content reader that accepts a repository SHA.
/// Its constructors require typed provenance; callers cannot manufacture one
/// from a raw `(repo_id, sha)` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedRepoReadView {
    key: RepoKey,
    head_commit: String,
}

/// One path/range attribution request against an already-authorized exact
/// repository version.
#[derive(Debug, Clone)]
pub struct HistoricalBlameRequest {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl CommittedRepoReadView {
    fn from_authorized_version(
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        repo_id: Uuid,
        head_commit: &str,
        provenance: &'static str,
    ) -> Result<Self, G5Error> {
        let key = TenantRepoStorage::key(scope, org_id, repo_id)?;
        if !valid_git_sha(head_commit) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Authorized repository version does not contain an exact Git SHA",
                    "repo_id": repo_id,
                    "head_commit": head_commit,
                    "provenance": provenance,
                }),
            });
        }
        Ok(Self {
            key,
            head_commit: head_commit.to_string(),
        })
    }

    /// `org_id`'s member version, keyed only when `scope` covers `org_id`
    /// (`g5.repo.storage-tenancy`).
    pub fn from_binding(
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        binding: &RepoBindingVersion,
    ) -> Result<Self, G5Error> {
        Self::from_authorized_version(
            scope,
            org_id,
            binding.repo_id,
            &binding.head_commit,
            "committed_binding",
        )
    }

    pub fn from_historical(
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        version: &HistoricalGitVersion,
    ) -> Result<Self, G5Error> {
        Self::from_authorized_version(
            scope,
            org_id,
            version.repo_id,
            &version.head_commit,
            "historical_version",
        )
    }

    /// The exact version a landed cut put on (or found at) a member's branch
    /// ref; see `services::branch_commit`.
    pub fn from_landed_head(
        scope: &(impl OrgScope + ?Sized),
        org_id: Uuid,
        repo_id: Uuid,
        head_commit: &str,
    ) -> Result<Self, G5Error> {
        Self::from_authorized_version(scope, org_id, repo_id, head_commit, "landed_cut")
    }

    /// An exact version of a repository already keyed for its organization
    /// (`g5.repo.storage-tenancy`).
    pub fn from_key(
        key: RepoKey,
        head_commit: &str,
        reason: g5_contracts::types::HistoricalReadReason,
    ) -> Result<Self, G5Error> {
        let version = HistoricalGitVersion {
            repo_id: key.repo_id(),
            head_commit: head_commit.to_string(),
            reason,
        };
        Self {
            key,
            head_commit: String::new(),
        }
        .historical_sibling(&version)
    }

    /// Another exact version of this already-keyed repository, such as a
    /// historical commit a blame or comparison names.
    pub fn historical_sibling(&self, version: &HistoricalGitVersion) -> Result<Self, G5Error> {
        if version.repo_id != self.repo_id() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Historical version belongs to another repository",
                    "repo_id": self.repo_id(),
                    "version_repo_id": version.repo_id,
                }),
            });
        }
        if !valid_git_sha(&version.head_commit) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Authorized repository version does not contain an exact Git SHA",
                    "repo_id": version.repo_id,
                    "head_commit": version.head_commit,
                    "provenance": "historical_version",
                }),
            });
        }
        Ok(Self {
            key: self.key,
            head_commit: version.head_commit.clone(),
        })
    }

    pub fn repo_id(&self) -> Uuid {
        self.key.repo_id()
    }

    #[must_use]
    pub fn key(&self) -> &RepoKey {
        &self.key
    }

    #[must_use]
    pub fn head_commit(&self) -> &str {
        &self.head_commit
    }

    pub async fn read_file(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
    ) -> Result<Vec<u8>, G5Error> {
        self.read_files(repo_storage, &[path.to_string()])
            .await?
            .remove(path)
            .flatten()
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("File '{path}' at commit '{}'", self.head_commit),
            })
    }

    pub async fn read_files(
        &self,
        repo_storage: &TenantRepoStorage,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, G5Error> {
        read_files_at_commit(repo_storage, &self.key, &self.head_commit, paths).await
    }

    /// Refuse `path` from its object header when it holds more than
    /// `max_bytes` ([`G5Error::FileTooLarge`]), before any content is
    /// loaded; `NotFound` when the commit has no such file. The preflight
    /// every bounded committed read runs before [`Self::read_file`].
    pub async fn ensure_file_within(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        max_bytes: usize,
    ) -> Result<(), G5Error> {
        let size = self
            .file_metadata(repo_storage, std::slice::from_ref(&path.to_string()))
            .await?
            .remove(path)
            .flatten()
            .map(|(_hash, size)| size)
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("File '{path}' at commit '{}'", self.head_commit),
            })?;
        if size > max_bytes {
            return Err(G5Error::FileTooLarge {
                path: path.to_string(),
                size: size as u64,
                max_bytes: max_bytes as u64,
            });
        }
        Ok(())
    }

    /// [`Self::read_file`] within `max_bytes`: a larger file is refused
    /// from its header as [`G5Error::FileTooLarge`] and never materialized.
    pub async fn read_file_bounded(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, G5Error> {
        self.ensure_file_within(repo_storage, path, max_bytes)
            .await?;
        self.read_file(repo_storage, path).await
    }

    /// Read blob identity and byte size for several paths at this exact
    /// authorized version without materializing their contents.
    ///
    /// Preflight callers use this to enforce whole-request byte budgets before
    /// any potentially large committed blob is loaded. Missing paths are
    /// represented as `None`; repository or commit failures abort the batch.
    pub async fn file_metadata(
        &self,
        repo_storage: &TenantRepoStorage,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<(String, usize)>>, G5Error> {
        file_metadata_at_commit(repo_storage, &self.key, &self.head_commit, paths).await
    }

    pub async fn list_tree(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        depth: u32,
    ) -> Result<Vec<TreeEntry>, G5Error> {
        self.list_trees(repo_storage, &[(path.to_string(), depth)])
            .await?
            .remove(path)
            .flatten()
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("Tree '{path}' at commit '{}'", self.head_commit),
            })
    }

    /// List one tree root to `depth` within `limits`: a subtree past a bound
    /// is listed in part and reports `truncated`, for a surface that shows
    /// what fits (the Files tree) rather than one that must see everything.
    pub async fn list_tree_bounded(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        depth: u32,
        limits: TreeListLimits,
    ) -> Result<TreeListing, G5Error> {
        list_trees_at_commit(
            repo_storage,
            &self.key,
            &self.head_commit,
            &[(path.to_string(), depth)],
            limits,
        )
        .await?
        .remove(path)
        .flatten()
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("Tree '{path}' at commit '{}'", self.head_commit),
        })
    }

    /// Every file and directory path under `path` at this exact version,
    /// read from the tree objects alone (no blob is loaded), up to
    /// `max_paths`: past it the walk stops and the result says so. A `path`
    /// the commit does not have is empty.
    pub async fn tracked_paths(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        max_paths: usize,
    ) -> Result<TreePaths, G5Error> {
        tracked_paths_at_commit(repo_storage, &self.key, &self.head_commit, path, max_paths).await
    }

    /// Stable ordering evidence for this exact cut, independent of when it is read.
    pub async fn committed_at(
        &self,
        storage: &TenantRepoStorage,
    ) -> Result<DateTime<Utc>, G5Error> {
        let path = storage.path(&self.key)?;
        let oid = self.head_commit.clone();
        tokio::task::spawn_blocking(move || {
            let committed_at = GitRepo::open(&path)?.commit_committed_at(&oid)?;
            DateTime::from_timestamp(committed_at, 0).ok_or_else(|| G5Error::GitError {
                message: "Commit timestamp is outside the supported range".into(),
            })
        })
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Commit metadata task failed: {e}"),
        })?
    }

    /// List several immutable tree roots through one Git repository handle.
    ///
    /// A missing root is represented as `None`; repository or commit failures
    /// abort the complete batch. Repeating one root with conflicting depths is
    /// rejected rather than silently selecting one request. The batch as a
    /// whole keeps to [`COMMITTED_ENUMERATION_LIMITS`]: a root past what the
    /// earlier roots left fails the batch as [`G5Error::TreeTooLarge`], since
    /// callers here consume the whole listing and a partial one is never
    /// returned to them.
    pub async fn list_trees(
        &self,
        repo_storage: &TenantRepoStorage,
        requests: &[(String, u32)],
    ) -> Result<BTreeMap<String, Option<Vec<TreeEntry>>>, G5Error> {
        let limits = COMMITTED_ENUMERATION_LIMITS;
        list_trees_at_commit(repo_storage, &self.key, &self.head_commit, requests, limits)
            .await?
            .into_iter()
            .map(|(path, listing)| match listing {
                Some(TreeListing {
                    truncated: true, ..
                }) => Err(G5Error::TreeTooLarge {
                    path,
                    max_entries: limits.max_entries,
                    max_bytes: limits.max_bytes,
                }),
                Some(TreeListing { entries, .. }) => Ok((path, Some(entries))),
                None => Ok((path, None)),
            })
            .collect()
    }

    /// Compare several files in this exact version with another authorized
    /// version of the same repository through one Git handle.
    ///
    /// A path missing from the current version is not pending. A path present
    /// only in the current version is changed. Callers with no prior marker
    /// pass None, which marks every current path as changed.
    pub async fn changed_files_from(
        &self,
        repo_storage: &TenantRepoStorage,
        base: Option<&Self>,
        paths: &[String],
    ) -> Result<BTreeMap<String, bool>, G5Error> {
        if base.is_some_and(|base| base.key != self.key) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Committed file comparison requires two versions of the same repository",
                    "head_repo_id": self.key.repo_id(),
                    "base_repo_id": base.map(|base| base.key.repo_id()),
                }),
            });
        }
        changed_files_between_commits(
            repo_storage,
            &self.key,
            base.map(|base| base.head_commit.as_str()),
            &self.head_commit,
            paths,
        )
        .await
    }

    pub async fn file_changed_from(
        &self,
        repo_storage: &TenantRepoStorage,
        base: Option<&Self>,
        path: &str,
    ) -> Result<bool, G5Error> {
        self.changed_files_from(repo_storage, base, &[path.to_string()])
            .await?
            .remove(path)
            .ok_or_else(|| G5Error::Internal {
                message: format!("Committed file comparison omitted requested path '{path}'"),
            })
    }

    /// Produce path-scoped diffs from another authorized version of the same
    /// repository while opening Git once for the complete batch.
    pub async fn diff_files_from(
        &self,
        repo_storage: &TenantRepoStorage,
        base: &Self,
        paths: &[String],
    ) -> Result<BTreeMap<String, String>, G5Error> {
        if base.key != self.key {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Committed diff requires two versions of the same repository",
                    "head_repo_id": self.key.repo_id(),
                    "base_repo_id": base.key.repo_id(),
                }),
            });
        }
        diff_files_between_commits(
            repo_storage,
            &self.key,
            &base.head_commit,
            &self.head_commit,
            paths,
        )
        .await
    }

    /// Summarize every committed path changed from another authorized version
    /// of the same repository. The exact base and head versions are supplied
    /// by typed authority values; callers never open Git or resolve a ref.
    pub async fn diff_summary_from(
        &self,
        repo_storage: &TenantRepoStorage,
        base: &Self,
    ) -> Result<DiffSummary, G5Error> {
        if base.key != self.key {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Committed diff summary requires two versions of the same repository",
                    "head_repo_id": self.key.repo_id(),
                    "base_repo_id": base.key.repo_id(),
                }),
            });
        }
        diff_summary_between_commits(
            repo_storage,
            &self.key,
            &base.head_commit,
            &self.head_commit,
            false,
        )
        .await
    }

    /// A net Source Control summary, counting a renamed file as one identity.
    pub async fn source_control_summary_from(
        &self,
        repo_storage: &TenantRepoStorage,
        base: &Self,
    ) -> Result<DiffSummary, G5Error> {
        ensure_same_repo_version(self, base, "source-control summary")?;
        diff_summary_between_commits(
            repo_storage,
            &self.key,
            &base.head_commit,
            &self.head_commit,
            true,
        )
        .await
    }

    /// A net Source Control summary of this whole version, as if every file
    /// were added to an empty repository.
    pub async fn source_control_summary_from_empty(
        &self,
        repo_storage: &TenantRepoStorage,
    ) -> Result<DiffSummary, G5Error> {
        let repo_path = committed_repo_path(repo_storage, &self.key, &self.head_commit)?;
        let head_commit = self.head_commit.clone();
        tokio::task::spawn_blocking(move || {
            GitRepo::open(&repo_path)?.source_control_summary_from_empty(&head_commit)
        })
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Committed Git diff summary task failed: {error}"),
        })?
    }

    /// Prove that this exact version descends from another authorized version
    /// of the same repository.
    pub async fn is_descendant_of(
        &self,
        repo_storage: &TenantRepoStorage,
        ancestor: &Self,
    ) -> Result<bool, G5Error> {
        ensure_same_repo_version(self, ancestor, "ancestry")?;
        is_ancestor_at_versions(
            repo_storage,
            &self.key,
            &ancestor.head_commit,
            &self.head_commit,
        )
        .await
    }

    pub async fn diff_file_from(
        &self,
        repo_storage: &TenantRepoStorage,
        base: &Self,
        path: &str,
    ) -> Result<String, G5Error> {
        self.diff_files_from(repo_storage, base, &[path.to_string()])
            .await?
            .remove(path)
            .ok_or_else(|| G5Error::Internal {
                message: format!("Committed diff omitted requested path '{path}'"),
            })
    }

    /// List immutable history from this authorized head while optionally
    /// hiding another authorized version of the same repository.
    pub async fn list_commits_since(
        &self,
        repo_storage: &TenantRepoStorage,
        base: Option<&Self>,
        limit: usize,
    ) -> Result<Vec<CommitSummary>, G5Error> {
        if base.is_some_and(|base| base.key != self.key) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Committed history boundary belongs to another repository",
                    "head_repo_id": self.key.repo_id(),
                    "base_repo_id": base.map(|base| base.key.repo_id()),
                }),
            });
        }
        list_commits_at_version(
            repo_storage,
            &self.key,
            &self.head_commit,
            base.map(|base| base.head_commit.as_str()),
            limit,
        )
        .await
    }

    /// Return the newest first-parent commit touching any requested path in
    /// this exact authorized repository version.
    pub async fn latest_commit_touching_paths(
        &self,
        repo_storage: &TenantRepoStorage,
        paths: &[String],
    ) -> Result<Option<CommitSummary>, G5Error> {
        latest_commit_touching_paths_at_version(repo_storage, &self.key, &self.head_commit, paths)
            .await
    }

    pub async fn list_first_parent_history(
        &self,
        repo_storage: &TenantRepoStorage,
        cursor: Option<&Self>,
        limit: usize,
    ) -> Result<CommitHistoryPage, G5Error> {
        if cursor.is_some_and(|cursor| cursor.key != self.key) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "message": "Committed history cursor belongs to another repository",
                    "head_repo_id": self.key.repo_id(),
                    "cursor_repo_id": cursor.map(|cursor| cursor.key.repo_id()),
                }),
            });
        }
        list_first_parent_history_at_version(
            repo_storage,
            &self.key,
            &self.head_commit,
            cursor.map(|cursor| cursor.head_commit.as_str()),
            limit,
        )
        .await
    }

    pub async fn commit_diff(
        &self,
        repo_storage: &TenantRepoStorage,
        commit: &Self,
    ) -> Result<CommitDiffSummary, G5Error> {
        ensure_same_repo_version(self, commit, "commit diff")?;
        commit_diff_at_version(
            repo_storage,
            &self.key,
            &self.head_commit,
            &commit.head_commit,
        )
        .await
    }

    pub async fn historical_file_diff(
        &self,
        repo_storage: &TenantRepoStorage,
        commit: &Self,
        path: &str,
    ) -> Result<CommitFileDiff, G5Error> {
        ensure_same_repo_version(self, commit, "commit file diff")?;
        commit_file_diff_at_version(
            repo_storage,
            &self.key,
            &self.head_commit,
            &commit.head_commit,
            path,
        )
        .await
    }

    /// Attribute several committed line ranges through one repository handle.
    ///
    /// Establishing the repository and exact commit fails the whole batch.
    /// Path/range failures remain per-request results so audit projections can
    /// degrade one row without hiding a broken authority binding.
    pub async fn blame_ranges(
        &self,
        repo_storage: &TenantRepoStorage,
        requests: &[HistoricalBlameRequest],
    ) -> Result<Vec<Result<Option<BlameAttribution>, G5Error>>, G5Error> {
        blame_ranges_at_version(repo_storage, &self.key, &self.head_commit, requests).await
    }

    pub async fn blame_range(
        &self,
        repo_storage: &TenantRepoStorage,
        request: HistoricalBlameRequest,
    ) -> Result<Option<BlameAttribution>, G5Error> {
        self.blame_ranges(repo_storage, &[request])
            .await?
            .pop()
            .ok_or_else(|| G5Error::Internal {
                message: "Committed blame batch omitted its scalar request".to_string(),
            })?
    }
}

fn ensure_same_repo_version(
    head: &CommittedRepoReadView,
    other: &CommittedRepoReadView,
    operation: &str,
) -> Result<(), G5Error> {
    if head.key == other.key {
        return Ok(());
    }
    Err(G5Error::ValidationError {
        details: serde_json::json!({
            "message": format!("Committed {operation} requires versions of the same repository"),
            "head_repo_id": head.key.repo_id(),
            "other_repo_id": other.key.repo_id(),
        }),
    })
}

/// One repository selected from a complete committed cut.
#[derive(Clone)]
pub struct RepoReadView {
    pub repo: RepoBindingVersion,
    committed: CommittedRepoReadView,
    dirty_files: Arc<[String]>,
    snapshot_at: DateTime<Utc>,
    source: DurableSnapshotSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoFileOverlay {
    Content(String),
    Deleted,
    Clean,
}

impl RepoReadView {
    /// Select one committed-only repository from an already-resolved complete
    /// snapshot. This performs no authority lookup.
    pub fn from_committed_snapshot(
        scope: &(impl OrgScope + ?Sized),
        snapshot: &CommittedBindingSnapshot,
        repo_id: Uuid,
    ) -> Result<Self, G5Error> {
        let repo = snapshot
            .repo(repo_id)
            .map_err(|message| authority_error(message, &snapshot.target))?
            .clone();
        let committed = CommittedRepoReadView::from_binding(scope, snapshot.org_id, &repo)?;
        Ok(Self {
            repo: repo.clone(),
            committed,
            dirty_files: Arc::from([]),
            snapshot_at: snapshot.snapshot_at,
            source: snapshot.source,
        })
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[doc(hidden)]
    pub fn from_test_parts(
        org_id: Uuid,
        repo: RepoBindingVersion,
        dirty_files: Vec<String>,
    ) -> Self {
        let committed = CommittedRepoReadView::from_binding(
            &crate::services::git::repo_store::TestOrgScope(org_id),
            org_id,
            &repo,
        )
        .expect("test binding has exact Git SHA");
        Self {
            repo,
            committed,
            dirty_files: Arc::from(dirty_files),
            snapshot_at: Utc::now(),
            source: DurableSnapshotSource::Committed,
        }
    }

    #[must_use]
    pub fn dirty_files(&self) -> &[String] {
        self.dirty_files.as_ref()
    }

    #[must_use]
    pub fn committed_view(&self) -> &CommittedRepoReadView {
        &self.committed
    }

    #[must_use]
    pub fn snapshot_at(&self) -> DateTime<Utc> {
        self.snapshot_at
    }

    #[must_use]
    pub fn snapshot_source(&self) -> DurableSnapshotSource {
        self.source
    }

    /// Committed repository views have no mutable overlay.
    pub async fn file_overlay(&self, _path: &str) -> RepoFileOverlay {
        RepoFileOverlay::Clean
    }

    /// Read one file from the exact Git SHA carried by the repository member.
    pub async fn read_file(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
    ) -> Result<RepoFileRead, G5Error> {
        self.read_files(repo_storage, &[path.to_string()])
            .await?
            .remove(path)
            .ok_or_else(|| G5Error::Internal {
                message: format!("Bulk repository read omitted requested path '{path}'"),
            })
    }

    /// Read multiple effective current files from this one captured view.
    ///
    /// The selected repository and exact committed SHA are reused for the
    /// entire batch.
    pub async fn read_files(
        &self,
        repo_storage: &TenantRepoStorage,
        paths: &[String],
    ) -> Result<BTreeMap<String, RepoFileRead>, G5Error> {
        let requested = paths.iter().cloned().collect::<BTreeSet<_>>();
        let mut reads = BTreeMap::new();
        let mut committed_paths = Vec::new();

        for path in requested {
            committed_paths.push(path);
        }

        for (path, content) in self
            .read_committed_files(repo_storage, &committed_paths)
            .await?
        {
            reads.insert(
                path,
                match content {
                    Some(bytes) => RepoFileRead::Content {
                        bytes,
                        source: RepoFileSource::CommittedCut,
                    },
                    None => RepoFileRead::Missing,
                },
            );
        }

        Ok(reads)
    }

    /// Read the immutable committed member without applying the optional live
    /// overlay. This is used by batch readers that enumerate the committed base
    /// once and merge captured dirty paths separately.
    pub async fn read_committed_file(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
    ) -> Result<Vec<u8>, G5Error> {
        self.committed.read_file(repo_storage, path).await
    }

    /// Read multiple immutable committed files through one Git repository
    /// handle. Missing paths are represented as `None`; an unreadable commit
    /// or repository fails the whole batch.
    pub async fn read_committed_files(
        &self,
        repo_storage: &TenantRepoStorage,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, G5Error> {
        self.committed.read_files(repo_storage, paths).await
    }

    /// List the immutable committed tree belonging to this captured view.
    /// Callers that expose a live tree merge `dirty_files()` through
    /// `file_overlay()` without resolving authority again.
    pub async fn list_committed_tree(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        depth: u32,
    ) -> Result<Vec<TreeEntry>, G5Error> {
        self.committed.list_tree(repo_storage, path, depth).await
    }

    pub async fn list_committed_trees(
        &self,
        repo_storage: &TenantRepoStorage,
        requests: &[(String, u32)],
    ) -> Result<BTreeMap<String, Option<Vec<TreeEntry>>>, G5Error> {
        self.committed.list_trees(repo_storage, requests).await
    }

    /// The committed cut's paths under `path`, files and directories, read
    /// without loading blobs and stopping past `max_paths`; the overlay is
    /// not consulted.
    pub async fn committed_tracked_paths(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        max_paths: usize,
    ) -> Result<TreePaths, G5Error> {
        self.committed
            .tracked_paths(repo_storage, path, max_paths)
            .await
    }

    /// Enumerate one effective current tree within `limits`, as
    /// [`Self::list_tree`] does without a bound: the committed walk stops
    /// past a bound and the listing reports `truncated`; the captured dirty
    /// paths are merged onto what was walked.
    pub async fn list_tree_bounded(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        depth: u32,
        limits: TreeListLimits,
    ) -> Result<TreeListing, G5Error> {
        let (root, truncated) = match self
            .committed
            .list_tree_bounded(repo_storage, path, depth, limits)
            .await
        {
            Ok(listing) => (Some(listing.entries), listing.truncated),
            Err(G5Error::NotFound { .. }) => (None, false),
            Err(error) => return Err(error),
        };
        let mut committed = BTreeMap::new();
        committed.insert(path.to_string(), root);
        let merged = self
            .merge_dirty_overlay(repo_storage, &[(path.to_string(), depth)], committed)
            .await?
            .remove(path)
            .flatten()
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("Tree '{path}' at commit '{}'", self.repo.head_commit),
            })?;
        // The overlay can only add entries to a walk that stopped at a
        // bound: the merged listing keeps to the same bounds.
        let mut listing = TreeListing {
            entries: Vec::with_capacity(merged.len().min(limits.max_entries)),
            truncated,
            bytes: 0,
        };
        for entry in merged {
            let bytes = listing.bytes.saturating_add(entry_bytes(&entry.path));
            if listing.entries.len() >= limits.max_entries || bytes > limits.max_bytes {
                listing.truncated = true;
                break;
            }
            listing.bytes = bytes;
            listing.entries.push(entry);
        }
        Ok(listing)
    }

    /// Enumerate one effective current tree from this captured view.
    ///
    /// The immutable exact-cut tree is merged with the captured dirty-path
    /// set through the same overlay classifier as `read_files`. Callers may
    /// filter the resulting paths for a product surface, but must not rebuild
    /// committed-plus-overlay precedence themselves.
    pub async fn list_tree(
        &self,
        repo_storage: &TenantRepoStorage,
        path: &str,
        depth: u32,
    ) -> Result<Vec<TreeEntry>, G5Error> {
        self.list_trees(repo_storage, &[(path.to_string(), depth)])
            .await?
            .remove(path)
            .flatten()
            .ok_or_else(|| G5Error::NotFound {
                resource: format!(
                    "Effective tree '{path}' at commit '{}'",
                    self.repo.head_commit
                ),
            })
    }

    /// Enumerate several effective current tree roots through one committed
    /// repository open and one captured-overlay bulk read.
    pub async fn list_trees(
        &self,
        repo_storage: &TenantRepoStorage,
        requests: &[(String, u32)],
    ) -> Result<BTreeMap<String, Option<Vec<TreeEntry>>>, G5Error> {
        let committed = self.list_committed_trees(repo_storage, requests).await?;
        self.merge_dirty_overlay(repo_storage, requests, committed)
            .await
    }

    /// Merge the captured dirty paths onto `committed` listings of
    /// `requests`, classified as `read_files` classifies them (a path the
    /// cut has is content, otherwise missing). Only paths under a requested
    /// root are considered, and their sizes come from object headers: no
    /// dirty path's content is loaded to list a tree.
    async fn merge_dirty_overlay(
        &self,
        repo_storage: &TenantRepoStorage,
        requests: &[(String, u32)],
        committed: BTreeMap<String, Option<Vec<TreeEntry>>>,
    ) -> Result<BTreeMap<String, Option<Vec<TreeEntry>>>, G5Error> {
        if requests.is_empty() || self.dirty_files.is_empty() {
            return Ok(committed);
        }

        let scopes = requests
            .iter()
            .map(|(path, _)| normalize_effective_tree_path(path))
            .collect::<Vec<_>>();
        let dirty_paths = self
            .dirty_files
            .iter()
            .map(|path| normalize_effective_tree_path(path))
            .filter(|path| !path.is_empty())
            .filter(|path| {
                scopes
                    .iter()
                    .any(|scope| effective_tree_relative_path(path, scope).is_some())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if dirty_paths.is_empty() {
            return Ok(committed);
        }
        let effective_dirty = self
            .committed
            .file_metadata(repo_storage, &dirty_paths)
            .await?
            .into_iter()
            .map(|(path, metadata)| (path, metadata.map(|(_hash, size)| size as u64)))
            .collect::<BTreeMap<_, _>>();
        let mut trees = BTreeMap::new();

        for (requested_path, depth) in requests {
            let scope = normalize_effective_tree_path(requested_path);
            let committed_root_exists = committed.get(requested_path).is_some_and(Option::is_some);
            let mut entries = committed
                .get(requested_path)
                .and_then(|entries| entries.clone())
                .unwrap_or_default()
                .into_iter()
                .map(|entry| (entry.path.clone(), entry))
                .collect::<BTreeMap<_, _>>();

            for (dirty_path, size) in &effective_dirty {
                apply_effective_tree_overlay(&mut entries, &scope, *depth, dirty_path, *size);
            }
            prune_empty_effective_directories(&mut entries, &scope, *depth);

            let listed = entries.into_values().collect::<Vec<_>>();
            trees.insert(
                requested_path.clone(),
                (committed_root_exists || !listed.is_empty()).then_some(listed),
            );
        }

        Ok(trees)
    }
}

fn normalize_effective_tree_path(path: &str) -> String {
    path.trim()
        .trim_matches('/')
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn effective_tree_relative_path<'a>(path: &'a str, scope: &str) -> Option<&'a str> {
    if scope.is_empty() {
        return Some(path);
    }
    if path == scope {
        return Some("");
    }
    path.strip_prefix(scope)
        .and_then(|relative| relative.strip_prefix('/'))
}

/// Place one dirty `path` in the listing of `scope`: `size` is the file's
/// size in the cut (from its object header), or `None` when the cut has no
/// such file, which removes it.
fn apply_effective_tree_overlay(
    entries: &mut BTreeMap<String, TreeEntry>,
    scope: &str,
    depth: u32,
    path: &str,
    size: Option<u64>,
) {
    let Some(relative_path) = effective_tree_relative_path(path, scope) else {
        return;
    };

    match size {
        None => {
            entries.remove(path);
        }
        Some(size) => {
            if relative_path.is_empty() {
                let name = path.rsplit('/').next().unwrap_or(path).to_string();
                entries.insert(
                    path.to_string(),
                    TreeEntry {
                        name,
                        path: path.to_string(),
                        entry_type: TreeEntryType::File,
                        size: Some(size),
                    },
                );
                return;
            }

            let relative_segments = relative_path
                .split('/')
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>();
            let visible_segments = relative_segments.len().min(depth as usize);
            if visible_segments == 0 {
                return;
            }
            let scope_segments = if scope.is_empty() {
                Vec::new()
            } else {
                scope.split('/').collect::<Vec<_>>()
            };

            for index in 1..=visible_segments {
                let mut full_segments = scope_segments.clone();
                full_segments.extend(relative_segments.iter().take(index).copied());
                let entry_path = full_segments.join("/");
                let is_file = index == relative_segments.len();
                entries.insert(
                    entry_path.clone(),
                    TreeEntry {
                        name: full_segments
                            .last()
                            .copied()
                            .unwrap_or_default()
                            .to_string(),
                        path: entry_path,
                        entry_type: if is_file {
                            TreeEntryType::File
                        } else {
                            TreeEntryType::Directory
                        },
                        size: is_file.then_some(size),
                    },
                );
            }
        }
    }
}

fn prune_empty_effective_directories(
    entries: &mut BTreeMap<String, TreeEntry>,
    scope: &str,
    depth: u32,
) {
    let paths = entries.keys().cloned().collect::<BTreeSet<_>>();
    entries.retain(|path, entry| {
        if !matches!(&entry.entry_type, TreeEntryType::Directory) {
            return true;
        }
        let relative_depth = effective_tree_relative_path(path, scope)
            .map(|relative| relative.split('/').filter(|part| !part.is_empty()).count())
            .unwrap_or_default();
        if depth != u32::MAX && relative_depth >= depth as usize {
            return true;
        }
        paths.iter().any(|candidate| {
            candidate
                .strip_prefix(path)
                .is_some_and(|rest| rest.starts_with('/'))
        })
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoFileSource {
    DirtyOverlay,
    CommittedCut,
}

impl RepoFileSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirtyOverlay => "dirty_draft",
            Self::CommittedCut => "committed_cut",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoFileRead {
    Content {
        bytes: Vec<u8>,
        source: RepoFileSource,
    },
    Deleted,
    Missing,
}

fn authority_error(message: impl Into<String>, target: &AuthorityTarget) -> G5Error {
    G5Error::ValidationError {
        details: serde_json::json!({
            "message": message.into(),
            "target": target,
        }),
    }
}

fn valid_git_sha(value: &str) -> bool {
    value.len() == 40
        && value.as_bytes().iter().any(|byte| *byte != b'0')
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

/// Base and personal repositories with no Git history are expected during
/// bootstrap, and a personal workspace's branch may be unborn until its
/// initialization completes (`g5.personal.lifecycle`). A missing ref in any
/// other workspace is always an authority error.
pub(crate) async fn expected_unborn_workspace_authority_cuts(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    org_id: Uuid,
    workspace_ids: &[Uuid],
) -> Result<BTreeSet<Uuid>, G5Error> {
    let repo_storage = repo_storage.scoped(&*db);
    let mut unborn = BTreeSet::new();
    for workspace_id in workspace_ids.iter().copied().collect::<BTreeSet<_>>() {
        let rows = load_target_repos(
            db.conn(),
            &AuthorityTarget::Workspace { workspace_id },
            false,
        )
        .await?;
        if rows.is_empty()
            || rows
                .iter()
                .any(|row| row.org_id != org_id || !row.may_be_unborn)
        {
            continue;
        }
        let lookups = rows
            .into_iter()
            .map(|row| {
                Ok((
                    row.branch_name,
                    repo_storage.repo_path(row.org_id, row.repo_id)?,
                ))
            })
            .collect::<Result<Vec<_>, G5Error>>()?;
        let expected = tokio::task::spawn_blocking(move || -> Result<bool, G5Error> {
            let mut has_unborn = false;
            for (branch, path) in lookups {
                if !path.is_dir() {
                    has_unborn = true;
                    continue;
                }
                match GitRepo::open(&path)?.branch_commit_state(&branch)? {
                    g5_scm::BranchCommitState::Present => {}
                    g5_scm::BranchCommitState::UnbornRepository => has_unborn = true,
                    g5_scm::BranchCommitState::Unavailable => return Ok(false),
                }
            }
            Ok(has_unborn)
        })
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Bootstrap readiness task failed: {e}"),
        })??;
        if expected {
            unborn.insert(workspace_id);
        }
    }
    Ok(unborn)
}

async fn load_target_repos(
    conn: &mut PgConnection,
    target: &AuthorityTarget,
    for_update: bool,
) -> Result<Vec<BindingRepoRow>, G5Error> {
    // Lock the workspace row before its mappings, the order every policy
    // `load(.., true)` uses (`FOR UPDATE OF w, m, ..`), so a landing and a
    // policy update on the same workspace cannot each hold the row the other
    // waits for.
    let lock_clause = if for_update {
        "FOR UPDATE OF workspace, mapping"
    } else {
        ""
    };
    let query = format!(
        r#"
        SELECT mapping.org_id, mapping.workspace_id, mapping.repo_id,
               repo.name AS repo_name, repo.slug AS repo_slug,
               mapping.role = 'docs' AS is_docs_repo, workspace.is_base,
               (workspace.is_base OR workspace.personal_user_id IS NOT NULL) AS may_be_unborn,
               CASE WHEN workspace.is_base THEN repo.default_branch ELSE workspace.branch_name END AS branch_name,
               NULL::text AS remote_branch, mapping.root_path
        FROM workspace_repos mapping
        JOIN workspaces workspace ON workspace.id = mapping.workspace_id AND workspace.org_id = mapping.org_id
        JOIN repos repo ON repo.id = mapping.repo_id AND repo.org_id = mapping.org_id
        WHERE mapping.workspace_id = $1 AND workspace.deleted_at IS NULL
        ORDER BY mapping.repo_id {lock_clause}
        "#,
    );
    let mut rows = sqlx::query_as::<_, BindingRepoRow>(&query)
        .bind(target.workspace_id())
        .fetch_all(&mut *conn)
        .await?;
    let remotes = crate::services::git::remote_integrations::remote_branches(
        conn,
        target.workspace_id(),
        for_update,
    )
    .await?;
    for row in &mut rows {
        row.remote_branch = remotes.get(&row.repo_id).cloned();
    }
    Ok(rows)
}

async fn expected_workspace_repo_ids(
    conn: &mut PgConnection,
    workspace_id: Uuid,
    for_update: bool,
) -> Result<BTreeSet<Uuid>, G5Error> {
    let lock_clause = if for_update {
        "FOR UPDATE OF mapping"
    } else {
        ""
    };
    let query = format!(
        r#"
        SELECT mapping.repo_id
        FROM workspace_repos mapping
        WHERE mapping.workspace_id = $1
        ORDER BY mapping.repo_id
        {lock_clause}
        "#,
    );
    let repo_ids = sqlx::query_scalar::<_, Uuid>(&query)
        .bind(workspace_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(repo_ids.into_iter().collect())
}

/// Resolve one complete, exact committed workspace/task cut.
///
/// The caller selects only a typed target. Repository membership and branch
/// identity come from the scoped database read path; every head SHA comes
/// from the branch ref `refs/heads/<branch>` in the member repository's
/// internal mirror. Database membership contains no Git version.
///
/// A retained flush receipt stands in for the refs only while the
/// workspace's VM is durably stopped (`stopped` or `failed`): that cut,
/// dirty bytes included, is then the last state of the working tree. While
/// the VM is running, starting or stopping, the branch refs are authoritative:
/// a flush also runs when a review copy forks a running workspace, and a
/// commit pushed after it would otherwise stay invisible until the next stop.
pub async fn resolve_committed_snapshot(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let repos = repo_storage.scoped(&*db);
    resolve_committed_snapshot_inner(db.conn(), &repos, target, false, true).await
}

/// [`resolve_committed_snapshot`] as one committed cut (`g5.fork.source.cut`):
/// the workspace commit fence is held while the refs are read, so the vector
/// is one a landing completed, never a mix of two. Use it wherever the vector
/// is persisted or acted on outside a fenced landing. The fence is bounded and
/// refuses with `WORKSPACE_COMMIT_FENCE_BUSY` for the caller to retry. It
/// comes first in the canonical lock order: the caller's transaction must
/// hold no row lock yet, and should end promptly to release the fence.
pub async fn resolve_committed_snapshot_fenced(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    operation: &str,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let repos = repo_storage.scoped(&*db);
    let conn = db.conn();
    if !crate::services::branch_commit::acquire_workspace_commit_fence_bounded(
        conn,
        target.workspace_id(),
    )
    .await?
    {
        return Err(crate::services::branch_commit::commit_fence_busy(operation));
    }
    resolve_committed_snapshot_inner(conn, &repos, target, false, true).await
}

/// [`resolve_committed_snapshot_fenced`] in its own short transaction. Consumes
/// and rolls back the caller's read-only admission transaction before reopening
/// its scope, so concurrent readers cannot exhaust the pool with nested
/// acquisitions. The new transaction has no prior row locks (fence first) and
/// is rolled back immediately after capture to release the fence. Callers that
/// need a later database phase can retain `db.reopener()` before this call.
pub async fn read_committed_cut(
    db: g5_authz::ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    operation: &str,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let scope = db.reopener();
    db.rollback().await.map_err(|error| G5Error::Internal {
        message: format!("Failed to close committed-cut admission: {error}"),
    })?;
    let mut fenced = scope.open().await.map_err(|error| G5Error::Internal {
        message: format!("Failed to open the committed-cut read: {error}"),
    })?;
    let snapshot =
        resolve_committed_snapshot_fenced(&mut fenced, repo_storage, target, operation).await?;
    fenced.rollback().await.map_err(|error| G5Error::Internal {
        message: format!("Failed to release the committed-cut read: {error}"),
    })?;
    Ok(snapshot)
}

/// Check that member heads read without the commit fence form one committed
/// cut (`g5.fork.source.cut`). This is for callers that capture a vector inside
/// a transaction that already holds workspace row locks or runs slow work. The
/// fence is taken in that same transaction with the bounded try-lock and held
/// until it ends, so the check reopens no authorization and the vector cannot
/// move before the caller commits. A try-lock never queues behind another
/// lock, so taking it after row locks cannot form a deadlock: a landing that
/// needs those rows only delays the caller until the bound refuses it.
///
/// A vector torn between two landings no longer matches the refs once the
/// landing that tore it has completed, so a mismatch refuses with the
/// retryable `WORKSPACE_CUT_CHANGED`.
pub async fn verify_committed_heads(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    workspace_id: Uuid,
    heads: &[(Uuid, String, String)],
    operation: &str,
) -> Result<(), G5Error> {
    let members = heads
        .iter()
        .map(|(repo_id, branch, _)| (*repo_id, branch.clone()))
        .collect::<Vec<_>>();
    let current =
        read_committed_branch_heads(db, workspace_id, repo_storage, &members, operation).await?;
    let torn = heads.iter().any(|(repo_id, branch, head)| {
        current
            .get(&(*repo_id, branch.clone()))
            .and_then(Option::as_deref)
            != Some(head.as_str())
    });
    if torn {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "code": "WORKSPACE_CUT_CHANGED",
                "operation": operation,
                "message": "The workspace changed while its branch heads were captured; retry.",
            }),
        });
    }
    Ok(())
}

/// Resolve the workspace cut from its *published* branch refs only, ignoring
/// any retained flush receipt.
///
/// `resolve_committed_snapshot` prefers a durably stopped workspace's
/// working-tree cut, which can include dirty bytes. Reading paths that must
/// reason about what a workspace has published, such as the review
/// workspace reopen sync, use this instead. No rows are locked.
pub async fn resolve_published_snapshot(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let repos = repo_storage.scoped(&*db);
    resolve_committed_snapshot_inner(db.conn(), &repos, target, false, false).await
}

/// [`resolve_published_snapshot`] on a caller's connection, for flows that
/// hold the store already narrowed to their scope.
pub(crate) async fn resolve_published_snapshot_on(
    conn: &mut PgConnection,
    repo_storage: &ScopedRepoStorage,
    target: AuthorityTarget,
) -> Result<CommittedBindingSnapshot, G5Error> {
    resolve_committed_snapshot_inner(conn, repo_storage, target, false, false).await
}

/// Resolve the workspace cut used by an agent task. Tasks no longer create or
/// retain private Git authority; the configured target contributes only the
/// workspace identity.
pub async fn resolve_task_workspace_snapshot(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    _task_id: Uuid,
    configured_target: AuthorityTarget,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let workspace_id = match configured_target {
        AuthorityTarget::Workspace { workspace_id }
        | AuthorityTarget::Task { workspace_id, .. } => workspace_id,
    };
    resolve_committed_snapshot(
        db,
        repo_storage,
        AuthorityTarget::Workspace { workspace_id },
    )
    .await
}

/// Read a workspace's member heads as one committed cut
/// (`g5.fork.source.cut`). The read holds the workspace commit fence, so no
/// landing is between its per-repository ref writes, and refs are never
/// retracted once a landing releases the fence (`g5.git.landing.commit-point`).
/// The vector is therefore one a landing completed, or the untouched prior one.
///
/// The fence is acquired with the interactive bound; a fence that stays busy
/// refuses with `WORKSPACE_COMMIT_FENCE_BUSY` for the caller to retry. The
/// fence comes first in the canonical lock order, so the caller's transaction
/// must hold no row lock yet, and should end right after the read to release
/// the fence.
pub async fn read_committed_branch_heads(
    db: &mut ScopedDb,
    workspace_id: Uuid,
    repo_storage: &TenantRepoStorage,
    members: &[(Uuid, String)],
    operation: &str,
) -> Result<BTreeMap<(Uuid, String), Option<String>>, G5Error> {
    let repos = repo_storage.scoped(&*db);
    let org_id = workspace_org(db.conn(), workspace_id).await?;
    if !crate::services::branch_commit::acquire_workspace_commit_fence_bounded(
        db.conn(),
        workspace_id,
    )
    .await?
    {
        return Err(crate::services::branch_commit::commit_fence_busy(operation));
    }
    read_branch_head_oids(&repos, org_id, members).await
}

/// The organization of a workspace the caller's tenant transaction can read.
pub(crate) async fn workspace_org(
    conn: &mut PgConnection,
    workspace_id: Uuid,
) -> Result<Uuid, G5Error> {
    sqlx::query_scalar::<_, Uuid>("SELECT org_id FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {workspace_id}"),
        })
}

/// Read the current OID of `refs/heads/<branch>` for several repository
/// members in one blocking pass over their internal mirrors.
///
/// `None` means the branch does not exist in that repository (or the mirror
/// itself has not been created), which callers must treat as an unreadable
/// member. The ref target is reported without inspecting the object, exactly
/// as [`GitRepo::read_branch_oid`] does.
pub async fn read_branch_head_oids(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    members: &[(Uuid, String)],
) -> Result<BTreeMap<(Uuid, String), Option<String>>, G5Error> {
    let lookups = members
        .iter()
        .map(|(repo_id, branch)| {
            Ok((
                *repo_id,
                branch.clone(),
                repo_storage.repo_path(org_id, *repo_id)?,
            ))
        })
        .collect::<Result<Vec<_>, G5Error>>()?;
    tokio::task::spawn_blocking(move || {
        let mut heads = BTreeMap::new();
        for (repo_id, branch, repo_path) in lookups {
            let oid = if repo_path.is_dir() {
                GitRepo::open(&repo_path)?.read_branch_oid(&branch)?
            } else {
                None
            };
            #[cfg(feature = "test-helpers")]
            crate::services::git::git_test_barrier::after_read(repo_id, &branch);
            heads.insert((repo_id, branch), oid);
        }
        Ok(heads)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Branch ref read task failed: {error}"),
    })?
}

/// Read the current OID of `refs/heads/<branch>` for one repository.
pub async fn read_branch_head_oid(
    repo_storage: &ScopedRepoStorage,
    org_id: Uuid,
    repo_id: Uuid,
    branch: &str,
) -> Result<Option<String>, G5Error> {
    let mut heads =
        read_branch_head_oids(repo_storage, org_id, &[(repo_id, branch.to_string())]).await?;
    Ok(heads.remove(&(repo_id, branch.to_string())).flatten())
}

/// Resolve and fence one complete exact cut for a landing.
///
/// The database rows are locked; the head SHAs still come from the branch
/// refs, which the row lock does not fence.
pub async fn resolve_committed_snapshot_for_update(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let repos = repo_storage.scoped(&*db);
    resolve_committed_snapshot_inner(db.conn(), &repos, target, true, false).await
}

pub(crate) async fn resolve_authority_inventory(
    conn: &mut PgConnection,
    target: AuthorityTarget,
) -> Result<AuthorityInventorySnapshot, G5Error> {
    resolve_authority_inventory_inner(conn, target, false).await
}

pub(crate) async fn resolve_authority_inventory_for_update(
    conn: &mut PgConnection,
    target: AuthorityTarget,
) -> Result<AuthorityInventorySnapshot, G5Error> {
    resolve_authority_inventory_inner(conn, target, true).await
}

async fn resolve_authority_inventory_inner(
    conn: &mut PgConnection,
    target: AuthorityTarget,
    for_update: bool,
) -> Result<AuthorityInventorySnapshot, G5Error> {
    let rows = load_validated_target_inventory_rows(conn, &target, for_update).await?;
    let first = rows.first().expect("validated inventory is non-empty");
    Ok(AuthorityInventorySnapshot {
        target,
        org_id: first.org_id,
        workspace_id: first.workspace_id,
        repos: rows
            .into_iter()
            .map(|row| AuthorityInventoryRepo {
                workspace_id: row.workspace_id,
                repo_id: row.repo_id,
                branch_name: row.branch_name,
                is_base: row.is_base,
                may_be_unborn: row.may_be_unborn,
            })
            .collect(),
    })
}

async fn load_validated_target_inventory_rows(
    conn: &mut PgConnection,
    target: &AuthorityTarget,
    for_update: bool,
) -> Result<Vec<BindingRepoRow>, G5Error> {
    let rows = load_target_repos(conn, target, for_update).await?;
    let Some(first) = rows.first() else {
        return Err(authority_error(
            "Authoritative target binding does not exist",
            target,
        ));
    };
    let org_id = first.org_id;
    let workspace_id = first.workspace_id;
    if workspace_id != target.workspace_id()
        || rows
            .iter()
            .any(|row| row.org_id != org_id || row.workspace_id != workspace_id)
    {
        return Err(authority_error(
            "Authoritative target members do not share one workspace and organization",
            target,
        ));
    }
    let expected = expected_workspace_repo_ids(conn, workspace_id, for_update).await?;
    let actual = rows.iter().map(|row| row.repo_id).collect::<BTreeSet<_>>();
    if expected.is_empty() || actual != expected || actual.len() != rows.len() {
        return Err(authority_error(
            "Authoritative target does not contain the complete repository inventory",
            target,
        ));
    }

    Ok(rows)
}

/// Recovery receipts remain immutable; browsing selects them only while the
/// complete published branch vector still matches the receipt's observation.
pub(crate) fn flush_matches_branch_heads<'a>(
    receipts: &[FlushRepoReceipt],
    heads: impl IntoIterator<Item = (Uuid, Option<&'a str>)>,
) -> bool {
    let heads = heads.into_iter().collect::<BTreeMap<_, _>>();
    !receipts.is_empty()
        && receipts.len() == heads.len()
        && receipts
            .iter()
            .map(|receipt| receipt.repo_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == receipts.len()
        && receipts.iter().all(|receipt| {
            heads.get(&receipt.repo_id).copied().flatten() == Some(receipt.branch_oid.as_str())
        })
}

async fn resolve_committed_snapshot_inner(
    conn: &mut PgConnection,
    repo_storage: &ScopedRepoStorage,
    target: AuthorityTarget,
    for_update: bool,
    include_flush: bool,
) -> Result<CommittedBindingSnapshot, G5Error> {
    let rows = load_validated_target_inventory_rows(conn, &target, for_update).await?;
    let first = rows.first().expect("validated inventory is non-empty");
    let org_id = first.org_id;
    let workspace_id = first.workspace_id;

    // A complete retained flush is the durable working-tree cut for a stopped
    // workspace (including dirty bytes). It is consulted only while the VM is
    // durably stopped: `last_flush_id` is never cleared, a flush also runs
    // when a review copy forks a running workspace, and a running VM pushes
    // past that cut. Without a stopped VM's receipt, the branch refs remain
    // authoritative; the database stores membership, never Git heads.
    let durable_receipt = if !include_flush {
        // Mutation and landing paths must resolve the current workspace branch.
        // A retained flush is an immutable read snapshot and can legitimately
        // lag a later branch update.
        None
    } else {
        sqlx::query_as::<_, (serde_json::Value, DateTime<Utc>)>(
            r#"
            SELECT flush.repos, flush.complete_at
            FROM workspace_vms AS vm
            JOIN workspace_flushes AS flush
              ON flush.org_id = vm.org_id AND flush.flush_id = vm.last_flush_id
            WHERE vm.workspace_id = $1 AND flush.workspace_id = $1
              AND vm.status IN ('stopped', 'failed')
              AND flush.complete_at IS NOT NULL AND flush.pruned_at IS NULL
              AND flush.error IS NULL
            FOR SHARE OF flush
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(&mut *conn)
        .await?
    };
    let members = rows
        .iter()
        .map(|row| (row.repo_id, row.branch_name.clone()))
        .collect::<Vec<_>>();
    let mut heads = read_branch_head_oids(repo_storage, org_id, &members).await?;
    let (mut durable_heads, durable_snapshot_at) = match durable_receipt {
        Some((value, completed_at)) => {
            let receipts =
                serde_json::from_value::<Vec<FlushRepoReceipt>>(value).map_err(|error| {
                    authority_error(
                        format!("Durable workspace receipt is malformed: {error}"),
                        &target,
                    )
                })?;
            let expected = rows.iter().map(|row| row.repo_id).collect::<BTreeSet<_>>();
            let snapshots = receipts
                .iter()
                .map(|receipt| (receipt.repo_id, receipt.snapshot_oid.clone()))
                .collect::<BTreeMap<_, _>>();
            if receipts.len() != rows.len()
                || snapshots.keys().copied().collect::<BTreeSet<_>>() != expected
                || snapshots.values().any(|oid| !valid_git_sha(oid))
            {
                return Err(authority_error(
                    "Durable workspace receipt does not contain the complete repository vector",
                    &target,
                ));
            }
            // Recovery still restores the exact receipt. Browsing, however,
            // must not let an older saved tree mask a subsequent publication.
            // Select a complete cut, never a mixture of flush and branch trees.
            let unchanged = flush_matches_branch_heads(
                &receipts,
                rows.iter().map(|row| {
                    (
                        row.repo_id,
                        heads
                            .get(&(row.repo_id, row.branch_name.clone()))
                            .and_then(Option::as_deref),
                    )
                }),
            );
            if unchanged {
                (Some(snapshots), Some(completed_at))
            } else {
                (None, None)
            }
        }
        None => (None, None),
    };
    let snapshot_at = durable_snapshot_at.unwrap_or_else(Utc::now);

    let repos = rows
        .into_iter()
        .map(|row| {
            let head_commit = durable_heads
                .as_mut()
                .and_then(|heads| heads.remove(&row.repo_id))
                .or_else(|| {
                    heads
                        .remove(&(row.repo_id, row.branch_name.clone()))
                        .flatten()
                })
                .filter(|head| valid_git_sha(head))
                .ok_or_else(|| {
                    authority_error(
                        format!(
                            "Authoritative target member refs/heads/{} of repository {} is absent from the internal mirror",
                            row.branch_name, row.repo_id
                        ),
                        &target,
                    )
                })?;
            Ok(RepoBindingVersion {
                repo_id: row.repo_id,
                repo_name: row.repo_name,
                repo_slug: row.repo_slug,
                repo_role: if row.is_docs_repo {
                    RepoRole::Docs
                } else {
                    RepoRole::Code
                },
                workspace_id: row.workspace_id,
                canonical_ref: format!("refs/heads/{}", row.branch_name),
                branch_name: row.branch_name,
                tracking_mode: if row.remote_branch.is_some() { "remote_tracking" } else { "pinned" }.into(),
                remote_branch: row.remote_branch,
                head_commit,
                root_path: row.root_path,
            })
        })
        .collect::<Result<Vec<_>, G5Error>>()?;

    Ok(CommittedBindingSnapshot {
        target,
        org_id,
        workspace_id,
        snapshot_at,
        source: if durable_snapshot_at.is_some() {
            DurableSnapshotSource::Flush
        } else {
            DurableSnapshotSource::Committed
        },
        repos,
    })
}

/// Resolve connected metadata against the exact committed target.
pub async fn resolve_connected_view(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<ConnectedBindingView, G5Error> {
    let snapshot = resolve_committed_snapshot(db, repo_storage, target).await?;
    Ok(ConnectedBindingView {
        snapshot,
        live_generation: None,
    })
}

/// Resolve one repository's committed version through the same typed authority binding.
pub async fn resolve_connected_repo_view(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    repo_id: Uuid,
) -> Result<RepoReadView, G5Error> {
    resolve_connected_repo_views(db, repo_storage, target, &[repo_id])
        .await?
        .repo(repo_id)
}

/// Resolve a complete connected committed cut.
pub async fn resolve_connected_binding_view(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<ResolvedConnectedBindingView, G5Error> {
    let repos = repo_storage.scoped(&*db);
    let view = resolve_connected_view(db, repo_storage, target).await?;
    let repo_ids = view
        .snapshot
        .repos
        .iter()
        .map(|repo| repo.repo_id)
        .collect::<Vec<_>>();
    committed_binding_views(&repos, view, &repo_ids)
}

/// Read an execution's complete assigned committed cut.
pub async fn resolve_execution_read_binding(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<ResolvedConnectedBindingView, G5Error> {
    let repos = repo_storage.scoped(&*db);
    let view = resolve_connected_view(db, repo_storage, target).await?;
    let repo_ids = view
        .snapshot
        .repos
        .iter()
        .map(|repo| repo.repo_id)
        .collect::<Vec<_>>();
    committed_binding_views(&repos, view, &repo_ids)
}

/// Resolve a complete connected committed cut.
pub async fn resolve_mutable_connected_binding_view(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
) -> Result<ResolvedConnectedBindingView, G5Error> {
    let repos = repo_storage.scoped(&*db);
    let view = resolve_connected_view(db, repo_storage, target).await?;
    let repo_ids = view
        .snapshot
        .repos
        .iter()
        .map(|repo| repo.repo_id)
        .collect::<Vec<_>>();
    committed_binding_views(&repos, view, &repo_ids)
}

/// Resolve one complete connected cut while loading only the requested repos.
pub async fn resolve_connected_repo_views(
    db: &mut ScopedDb,
    repo_storage: &TenantRepoStorage,
    target: AuthorityTarget,
    repo_ids: &[Uuid],
) -> Result<ResolvedConnectedBindingView, G5Error> {
    let repos = repo_storage.scoped(&*db);
    let view = resolve_connected_view(db, repo_storage, target).await?;
    committed_binding_views(&repos, view, repo_ids)
}

/// Validate that a terminal task read matches the exact task snapshot.
pub async fn ensure_task_snapshot_has_no_dirty_overlay(
    _conn: &mut PgConnection,
    snapshot: &CommittedBindingSnapshot,
    task_id: Uuid,
) -> Result<(), G5Error> {
    match &snapshot.target {
        AuthorityTarget::Task {
            workspace_id,
            task_id: target_task_id,
        } if *workspace_id == snapshot.workspace_id && *target_task_id == task_id => {}
        _ => {
            return Err(authority_error(
                "Terminal task overlay validation requires the matching task snapshot",
                &snapshot.target,
            ));
        }
    }

    Ok(())
}
fn committed_binding_views(
    scope: &ScopedRepoStorage,
    view: ConnectedBindingView,
    repo_ids: &[Uuid],
) -> Result<ResolvedConnectedBindingView, G5Error> {
    let snapshot_at = view.snapshot.snapshot_at;
    let source = view.snapshot.source;
    let selected = repo_ids.iter().copied().collect::<BTreeSet<_>>();
    if selected.is_empty() {
        return Err(authority_error(
            "Connected binding requires at least one repository selection",
            &view.snapshot.target,
        ));
    }
    let mut repos = BTreeMap::new();
    for repo_id in selected {
        let repo = view
            .snapshot
            .repo(repo_id)
            .map_err(|message| authority_error(message, &view.snapshot.target))?
            .clone();
        let committed = CommittedRepoReadView::from_binding(scope, view.snapshot.org_id, &repo)?;
        repos.insert(
            repo_id,
            RepoReadView {
                repo,
                committed,
                dirty_files: Arc::from([]),
                snapshot_at,
                source,
            },
        );
    }

    Ok(ResolvedConnectedBindingView { view, repos })
}

fn committed_repo_path(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
) -> Result<std::path::PathBuf, G5Error> {
    if commit_sha.trim().is_empty() {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": "Committed repository reads require an explicit commit SHA",
                "repo_id": key.repo_id(),
            }),
        });
    }

    repo_storage.path(key)
}

/// Read multiple files from one exact immutable commit through one Git handle.
///
/// Missing files are returned as `None`. Repository or commit failures abort
/// the whole batch so a caller can never mistake an unreadable cut for an
/// empty corpus.
async fn read_files_at_commit(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    file_paths: &[String],
) -> Result<BTreeMap<String, Option<Vec<u8>>>, G5Error> {
    if file_paths.is_empty() {
        return Ok(BTreeMap::new());
    }
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let commit_sha = commit_sha.to_string();
    let file_paths = file_paths.iter().cloned().collect::<BTreeSet<_>>();

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        if !repo.commit_exists(&commit_sha) {
            return Err(G5Error::GitError {
                message: format!(
                    "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
                ),
            });
        }
        let mut files = BTreeMap::new();
        for file_path in file_paths {
            let content = match repo.show_file(&commit_sha, &file_path) {
                Ok(bytes) => Some(bytes),
                Err(G5Error::NotFound { .. }) => None,
                Err(error) => return Err(error),
            };
            files.insert(file_path, content);
        }
        Ok(files)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git file batch read task failed: {error}"),
    })?
}

/// Read one path from an exact committed repository snapshot.
///
/// This synchronous bridge exists for blocking service operations that already
/// batch adjacent Git work inside `spawn_blocking`. Raw blob access remains
/// private to this module so every API reader shares the same exact-commit
/// boundary.
/// The byte size of one path in an exact committed snapshot, from the
/// object header, so a caller can refuse an oversized file before
/// [`read_file_at_commit_sync`] materializes it.
pub(crate) fn file_size_at_commit_sync(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    file_path: &str,
) -> Result<u64, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let repo = GitRepo::open(&repo_path)?;
    if !repo.commit_exists(commit_sha) {
        return Err(G5Error::GitError {
            message: format!(
                "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
            ),
        });
    }
    repo.blob_size(commit_sha, file_path)
}

pub(crate) fn read_file_at_commit_sync(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    file_path: &str,
) -> Result<Vec<u8>, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let repo = GitRepo::open(&repo_path)?;
    if !repo.commit_exists(commit_sha) {
        return Err(G5Error::GitError {
            message: format!(
                "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
            ),
        });
    }
    repo.show_file(commit_sha, file_path)
}

/// The git blob id of `file_path` at one exact commit, from the object
/// header only: the bytes are never materialized, so a file of any size
/// answers at once.
pub(crate) fn blob_id_at_commit_sync(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    file_path: &str,
) -> Result<String, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let repo = GitRepo::open(&repo_path)?;
    if !repo.commit_exists(commit_sha) {
        return Err(G5Error::GitError {
            message: format!(
                "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
            ),
        });
    }
    repo.blob_id(commit_sha, file_path)
}

#[cfg(test)]
pub(crate) fn read_test_file_at_commit_sync(
    repo: &GitRepo,
    commit_sha: &str,
    file_path: &str,
) -> Result<Vec<u8>, G5Error> {
    repo.show_file(commit_sha, file_path)
}

/// Read blob identity and byte size for several paths from one exact immutable
/// commit through one Git handle, without loading blob contents.
async fn file_metadata_at_commit(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    file_paths: &[String],
) -> Result<BTreeMap<String, Option<(String, usize)>>, G5Error> {
    if file_paths.is_empty() {
        return Ok(BTreeMap::new());
    }
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let commit_sha = commit_sha.to_string();
    let file_paths = file_paths.iter().cloned().collect::<BTreeSet<_>>();

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        if !repo.commit_exists(&commit_sha) {
            return Err(G5Error::GitError {
                message: format!(
                    "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
                ),
            });
        }
        let mut metadata = BTreeMap::new();
        for file_path in file_paths {
            let value = match repo.blob_metadata(&commit_sha, &file_path) {
                Ok(value) => Some(value),
                Err(G5Error::NotFound { .. }) => None,
                Err(error) => return Err(error),
            };
            metadata.insert(file_path, value);
        }
        Ok(metadata)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git metadata batch task failed: {error}"),
    })?
}

/// Every path under `path` at one exact immutable commit, from the tree
/// objects alone. A missing root is empty.
async fn tracked_paths_at_commit(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    path: &str,
    max_paths: usize,
) -> Result<TreePaths, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let commit_sha = commit_sha.to_string();
    let path = path.trim_matches('/').to_string();
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        if !repo.commit_exists(&commit_sha) {
            return Err(G5Error::GitError {
                message: format!(
                    "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
                ),
            });
        }
        match repo.list_tree_paths(&commit_sha, &path, max_paths) {
            Ok(paths) => Ok(paths),
            Err(G5Error::NotFound { .. }) => Ok(TreePaths {
                paths: Vec::new(),
                truncated: false,
            }),
            Err(error) => Err(error),
        }
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git path listing task failed: {error}"),
    })?
}

/// List several tree roots from one exact immutable commit through one Git
/// repository handle, the batch as a whole within `limits`: each root is
/// walked with what the earlier roots left, so a request for many roots
/// retains no more than one root's allowance in total, and a root past the
/// remainder reports `truncated`.
///
/// Missing roots are returned as `None`. A repeated root with conflicting
/// depths is rejected so request order cannot change the result.
async fn list_trees_at_commit(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    commit_sha: &str,
    requests: &[(String, u32)],
    limits: TreeListLimits,
) -> Result<BTreeMap<String, Option<TreeListing>>, G5Error> {
    if requests.is_empty() {
        return Ok(BTreeMap::new());
    }
    let repo_path = committed_repo_path(repo_storage, key, commit_sha)?;
    let repo_id = key.repo_id();
    let commit_sha = commit_sha.to_string();
    let mut roots = BTreeMap::new();
    for (path, depth) in requests {
        if let Some(existing_depth) = roots.insert(path.clone(), *depth) {
            if existing_depth != *depth {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "message": "A committed tree batch cannot request one root at conflicting depths",
                        "path": path,
                        "first_depth": existing_depth,
                        "second_depth": depth,
                    }),
                });
            }
        }
    }

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        if !repo.commit_exists(&commit_sha) {
            return Err(G5Error::GitError {
                message: format!(
                    "Authoritative commit '{commit_sha}' is unreadable in repository {repo_id}"
                ),
            });
        }
        let mut trees = BTreeMap::new();
        let mut remaining = limits;
        for (path, depth) in roots {
            let listing = match repo.list_tree_recursive(&commit_sha, &path, depth, remaining) {
                Ok(listing) => Some(listing),
                Err(G5Error::NotFound { .. }) => None,
                Err(error) => return Err(error),
            };
            if let Some(listing) = &listing {
                remaining.max_entries = remaining.max_entries.saturating_sub(listing.entries.len());
                remaining.max_bytes = remaining.max_bytes.saturating_sub(listing.bytes);
            }
            trees.insert(path, listing);
        }
        Ok(trees)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git tree batch read task failed: {error}"),
    })?
}

async fn changed_files_between_commits(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    base_commit: Option<&str>,
    head_commit: &str,
    file_paths: &[String],
) -> Result<BTreeMap<String, bool>, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let base_commit = base_commit.map(ToString::to_string);
    let head_commit = head_commit.to_string();
    let file_paths = file_paths.iter().cloned().collect::<BTreeSet<_>>();

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        let mut changed = BTreeMap::new();
        for file_path in file_paths {
            let head_blob_id = match repo.blob_id(&head_commit, &file_path) {
                Ok(blob_id) => Some(blob_id),
                Err(G5Error::NotFound { .. }) => None,
                Err(error) => return Err(error),
            };
            let is_changed = match (head_blob_id, base_commit.as_deref()) {
                (None, _) => false,
                (Some(_), None) => true,
                (Some(_), Some(base)) if base == head_commit => false,
                (Some(head_blob_id), Some(base)) => match repo.blob_id(base, &file_path) {
                    Ok(base_blob_id) => base_blob_id != head_blob_id,
                    Err(G5Error::NotFound { .. }) => true,
                    Err(error) => return Err(error),
                },
            };
            changed.insert(file_path, is_changed);
        }
        Ok(changed)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git file comparison task failed: {error}"),
    })?
}

async fn diff_files_between_commits(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    base_commit: &str,
    head_commit: &str,
    file_paths: &[String],
) -> Result<BTreeMap<String, String>, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let base_commit = base_commit.to_string();
    let head_commit = head_commit.to_string();
    let file_paths = file_paths.iter().cloned().collect::<BTreeSet<_>>();

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        let mut diffs = BTreeMap::new();
        for file_path in file_paths {
            let diff =
                repo.diff_unified_between_commits(&base_commit, &head_commit, Some(&file_path))?;
            diffs.insert(file_path, diff);
        }
        Ok(diffs)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git diff batch task failed: {error}"),
    })?
}

async fn diff_summary_between_commits(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    base_commit: &str,
    head_commit: &str,
    renames: bool,
) -> Result<DiffSummary, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let base_commit = base_commit.to_string();
    let head_commit = head_commit.to_string();

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        if renames {
            repo.source_control_summary(&base_commit, &head_commit)
        } else {
            repo.diff_summary(&base_commit, &head_commit)
        }
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git diff summary task failed: {error}"),
    })?
}

async fn is_ancestor_at_versions(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    ancestor_commit: &str,
    descendant_commit: &str,
) -> Result<bool, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, descendant_commit)?;
    let ancestor_commit = ancestor_commit.to_string();
    let descendant_commit = descendant_commit.to_string();
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        repo.is_ancestor_of(&ancestor_commit, &descendant_commit)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed ancestry task failed: {error}"),
    })?
}

async fn list_commits_at_version(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    head_commit: &str,
    base_commit: Option<&str>,
    limit: usize,
) -> Result<Vec<CommitSummary>, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let head_commit = head_commit.to_string();
    let base_commit = base_commit.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        repo.list_commits(&head_commit, base_commit.as_deref(), limit)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git history task failed: {error}"),
    })?
}

async fn list_first_parent_history_at_version(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    head_commit: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<CommitHistoryPage, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let head_commit = head_commit.to_string();
    let cursor = cursor.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        repo.list_first_parent_commits(&head_commit, cursor.as_deref(), limit)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed first-parent history task failed: {error}"),
    })?
}

async fn latest_commit_touching_paths_at_version(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    head_commit: &str,
    paths: &[String],
) -> Result<Option<CommitSummary>, G5Error> {
    if paths.is_empty() {
        return Ok(None);
    }
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let head_commit = head_commit.to_string();
    let paths = paths.to_vec();
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        repo.latest_commit_touching_paths(&head_commit, &paths)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed path-history task failed: {error}"),
    })?
}

async fn commit_diff_at_version(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    head_commit: &str,
    commit: &str,
) -> Result<CommitDiffSummary, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let head_commit = head_commit.to_string();
    let commit = commit.to_string();
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        repo.commit_diff_summary(&head_commit, &commit)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git commit-diff task failed: {error}"),
    })?
}

async fn commit_file_diff_at_version(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    head_commit: &str,
    commit: &str,
    path: &str,
) -> Result<CommitFileDiff, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let head_commit = head_commit.to_string();
    let commit = commit.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        repo.commit_file_diff(&head_commit, &commit, &path)
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed Git commit-file-diff task failed: {error}"),
    })?
}

async fn blame_ranges_at_version(
    repo_storage: &TenantRepoStorage,
    key: &RepoKey,
    head_commit: &str,
    requests: &[HistoricalBlameRequest],
) -> Result<Vec<Result<Option<BlameAttribution>, G5Error>>, G5Error> {
    let repo_path = committed_repo_path(repo_storage, key, head_commit)?;
    let repo_id = key.repo_id();
    let head_commit = head_commit.to_string();
    let requests = requests
        .iter()
        .map(|request| (request.path.clone(), request.start_line, request.end_line))
        .collect::<Vec<_>>();

    tokio::task::spawn_blocking(move || {
        let repo = GitRepo::open(&repo_path)?;
        if !repo.commit_exists(&head_commit) {
            return Err(G5Error::NotFound {
                resource: format!("Commit '{head_commit}' for repository {repo_id}"),
            });
        }
        Ok(requests
            .into_iter()
            .map(|(path, start_line, end_line)| {
                repo.blame_for_path(&head_commit, &path, start_line, end_line)
            })
            .collect())
    })
    .await
    .map_err(|error| G5Error::Internal {
        message: format!("Committed blame batch task failed: {error}"),
    })?
}

/// Test-only bridge for older route fixtures that build an ad-hoc GitRepo.
/// Production source is guarded from calling raw blob readers anywhere except
/// this module.
#[cfg(any(test, feature = "test-helpers"))]
#[doc(hidden)]
pub async fn read_test_file_at_commit(
    git_repo: Arc<GitRepo>,
    commit: String,
    path: String,
) -> Result<Vec<u8>, G5Error> {
    tokio::task::spawn_blocking(move || git_repo.show_file(&commit, &path))
        .await
        .map_err(|error| G5Error::Internal {
            message: format!("Test committed-file read task failed: {error}"),
        })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::git::repo_store::TestOrgScope;
    use g5_scm::GitAuthor;
    use std::fs;
    use tempfile::TempDir;

    /// The fixtures' organization.
    fn org() -> Uuid {
        Uuid::from_u128(0x0a)
    }

    fn key(repo_id: Uuid) -> RepoKey {
        TenantRepoStorage::test_key(org(), repo_id)
    }

    /// Landings lock the workspace row before its mappings, matching every
    /// policy `load(.., true)`, so the two cannot deadlock on one workspace.
    #[test]
    fn landing_inventory_locks_the_workspace_row_before_its_mappings() {
        let source = include_str!("workspace_reader.rs");
        let body = &source[source.find("async fn load_target_repos(").unwrap()..];
        assert!(body.contains("\"FOR UPDATE OF workspace, mapping\""));
        let policy = include_str!("remote_integrations.rs");
        assert!(policy.contains("\"FOR UPDATE OF w, m, r, i\""));
    }

    #[tokio::test]
    async fn committed_read_rejects_an_empty_commit_instead_of_reading_the_checkout() {
        let error = CommittedRepoReadView::from_historical(
            &TestOrgScope(org()),
            org(),
            &HistoricalGitVersion {
                repo_id: Uuid::new_v4(),
                head_commit: String::new(),
                reason: g5_contracts::types::HistoricalReadReason::OperationValidation,
            },
        )
        .expect_err("empty commit must fail closed");

        assert!(matches!(error, G5Error::ValidationError { .. }));
    }

    #[tokio::test]
    async fn committed_tree_batch_uses_one_exact_cut_and_preserves_missing_roots() {
        let root = TempDir::new().expect("temp storage");
        let storage = TenantRepoStorage::local_for_test(root.path().to_path_buf());
        let repo_id = Uuid::new_v4();
        let repo_path = storage.test_path(org(), repo_id);
        fs::create_dir_all(repo_path.join("docs")).expect("create docs directory");
        fs::create_dir_all(repo_path.join("src")).expect("create source directory");
        git2::Repository::init(&repo_path).expect("initialize repository");
        fs::write(repo_path.join("docs/guide.md"), "guide\n").expect("write docs file");
        fs::write(repo_path.join("src/lib.rs"), "pub fn value() {}\n").expect("write source file");
        let repo = GitRepo::open(&repo_path).expect("open repository");
        repo.add_paths(["docs/guide.md", "src/lib.rs"])
            .expect("stage tree files");
        let head = repo
            .commit(
                "seed exact tree",
                &GitAuthor {
                    name: "Reader Test".to_string(),
                    email: "reader@example.com".to_string(),
                },
            )
            .expect("commit exact tree");

        fs::write(repo_path.join("docs/guide.md"), "ambient checkout\n")
            .expect("modify ambient checkout");
        let view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org()),
            org(),
            &HistoricalGitVersion {
                repo_id,
                head_commit: head,
                reason: g5_contracts::types::HistoricalReadReason::OperationValidation,
            },
        )
        .expect("authorize exact historical cut");

        let trees = view
            .list_trees(
                &storage,
                &[
                    ("docs".to_string(), 32),
                    ("src".to_string(), 32),
                    ("missing".to_string(), 32),
                ],
            )
            .await
            .expect("list exact tree batch");

        assert!(trees["docs"]
            .as_ref()
            .expect("docs root")
            .iter()
            .any(|entry| entry.path == "docs/guide.md"));
        assert!(trees["src"]
            .as_ref()
            .expect("source root")
            .iter()
            .any(|entry| entry.path == "src/lib.rs"));
        assert!(trees["missing"].is_none());
    }

    /// A repository with `docs/guide.md`, `src/lib.rs` and `assets/big.bin`
    /// (`big` bytes) committed, as a historical view over local storage.
    fn committed_fixture(big: usize) -> (TempDir, TenantRepoStorage, Uuid, String) {
        let root = TempDir::new().expect("temp storage");
        let storage = TenantRepoStorage::local_for_test(root.path().to_path_buf());
        let repo_id = Uuid::new_v4();
        let repo_path = storage.test_path(org(), repo_id);
        for directory in ["docs", "src", "assets"] {
            fs::create_dir_all(repo_path.join(directory)).expect("create directory");
        }
        git2::Repository::init(&repo_path).expect("initialize repository");
        fs::write(repo_path.join("docs/guide.md"), "guide\n").expect("write docs file");
        fs::write(repo_path.join("src/lib.rs"), "pub fn value() {}\n").expect("write source file");
        fs::write(repo_path.join("assets/big.bin"), vec![b'b'; big]).expect("write asset");
        let repo = GitRepo::open(&repo_path).expect("open repository");
        repo.add_paths(["docs/guide.md", "src/lib.rs", "assets/big.bin"])
            .expect("stage tree files");
        let head = repo
            .commit(
                "seed exact tree",
                &GitAuthor {
                    name: "Reader Test".to_string(),
                    email: "reader@example.com".to_string(),
                },
            )
            .expect("commit exact tree");
        (root, storage, repo_id, head)
    }

    #[tokio::test]
    async fn committed_tree_batch_spends_one_allowance_across_its_roots() {
        let (_root, storage, repo_id, head) = committed_fixture(8);
        let roots = [
            ("assets".to_string(), 32),
            ("docs".to_string(), 32),
            ("src".to_string(), 32),
        ];

        // Each root holds one entry: three fit, two do not.
        let fitting = list_trees_at_commit(
            &storage,
            &key(repo_id),
            &head,
            &roots,
            TreeListLimits {
                max_entries: 3,
                max_bytes: usize::MAX,
            },
        )
        .await
        .expect("list within the allowance");
        assert!(fitting
            .values()
            .all(|listing| !listing.as_ref().expect("root exists").truncated));

        let spent = list_trees_at_commit(
            &storage,
            &key(repo_id),
            &head,
            &roots,
            TreeListLimits {
                max_entries: 2,
                max_bytes: usize::MAX,
            },
        )
        .await
        .expect("list past the allowance");
        assert!(!spent["assets"].as_ref().unwrap().truncated);
        assert!(!spent["docs"].as_ref().unwrap().truncated);
        let last = spent["src"].as_ref().unwrap();
        assert!(
            last.truncated,
            "the third root gets nothing of the allowance"
        );
        assert!(last.entries.is_empty());

        // The byte budget is spent the same way.
        let budget = entry_bytes("assets/big.bin") + entry_bytes("docs/guide.md");
        let by_bytes = list_trees_at_commit(
            &storage,
            &key(repo_id),
            &head,
            &roots,
            TreeListLimits {
                max_entries: usize::MAX,
                max_bytes: budget,
            },
        )
        .await
        .expect("list within the byte budget");
        assert!(!by_bytes["docs"].as_ref().unwrap().truncated);
        assert!(by_bytes["src"].as_ref().unwrap().truncated);

        // The whole-listing API refuses the batch rather than returning the
        // part that fit.
        let view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org()),
            org(),
            &HistoricalGitVersion {
                repo_id,
                head_commit: head.clone(),
                reason: g5_contracts::types::HistoricalReadReason::OperationValidation,
            },
        )
        .expect("authorize exact historical cut");
        let whole = view
            .list_trees(&storage, &roots)
            .await
            .expect("three entries are far under the enumeration ceiling");
        assert_eq!(whole.len(), 3);
    }

    #[tokio::test]
    async fn effective_tree_overlay_sizes_dirty_paths_from_headers_within_the_bounds() {
        let big = 4096;
        let (_root, storage, repo_id, head) = committed_fixture(big);
        let binding = RepoBindingVersion {
            repo_id,
            repo_name: "code".to_string(),
            repo_slug: "code".to_string(),
            repo_role: g5_contracts::types::RepoRole::Code,
            workspace_id: Uuid::new_v4(),
            branch_name: "main".to_string(),
            tracking_mode: "pinned".to_string(),
            remote_branch: None,
            canonical_ref: "refs/heads/main".to_string(),
            head_commit: head,
            root_path: String::new(),
        };
        // A dirty path the cut has (sized from its header, never loaded), one
        // it does not have (removed), and one outside the requested root.
        let view = RepoReadView::from_test_parts(
            org(),
            binding,
            vec![
                "assets/big.bin".to_string(),
                "assets/gone.bin".to_string(),
                "src/lib.rs".to_string(),
            ],
        );
        let limits = TreeListLimits {
            max_entries: 10,
            max_bytes: usize::MAX,
        };

        let assets = view
            .list_tree_bounded(&storage, "assets", 32, limits)
            .await
            .expect("list the overlaid root");
        assert!(!assets.truncated);
        let paths = assets
            .entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry.size))
            .collect::<Vec<_>>();
        assert_eq!(paths, vec![("assets/big.bin", Some(big as u64))]);

        // The overlay keeps to the listing's bounds: with room for one entry
        // the merged listing is cut and says so.
        let one = view
            .list_tree_bounded(
                &storage,
                "",
                32,
                TreeListLimits {
                    max_entries: 1,
                    max_bytes: usize::MAX,
                },
            )
            .await
            .expect("list the root within one entry");
        assert!(one.truncated);
        assert_eq!(one.entries.len(), 1);
    }

    #[tokio::test]
    async fn committed_blame_batch_uses_one_exact_version_for_all_ranges() {
        let root = TempDir::new().expect("temp storage");
        let storage = TenantRepoStorage::local_for_test(root.path().to_path_buf());
        let repo_id = Uuid::new_v4();
        let repo_path = storage.test_path(org(), repo_id);
        fs::create_dir_all(&repo_path).expect("create repository path");
        git2::Repository::init(&repo_path).expect("initialize repository");
        let repo = GitRepo::open(&repo_path).expect("open repository");

        fs::write(repo_path.join("a.md"), "[I1. A]\nalpha\n").expect("write a.md");
        repo.add_paths(["a.md"]).expect("stage a.md");
        repo.commit(
            "add a",
            &GitAuthor {
                name: "Alice".to_string(),
                email: "alice@example.com".to_string(),
            },
        )
        .expect("commit a.md");

        fs::write(repo_path.join("b.md"), "[I1. B]\nbravo\n").expect("write b.md");
        repo.add_paths(["b.md"]).expect("stage b.md");
        let head = repo
            .commit(
                "add b",
                &GitAuthor {
                    name: "Bob".to_string(),
                    email: "bob@example.com".to_string(),
                },
            )
            .expect("commit b.md");

        let view = CommittedRepoReadView::from_historical(
            &TestOrgScope(org()),
            org(),
            &HistoricalGitVersion {
                repo_id,
                head_commit: head,
                reason: g5_contracts::types::HistoricalReadReason::BlameAttribution,
            },
        )
        .expect("authorize exact blame version");
        let results = view
            .blame_ranges(
                &storage,
                &[
                    HistoricalBlameRequest {
                        path: "a.md".to_string(),
                        start_line: 1,
                        end_line: 2,
                    },
                    HistoricalBlameRequest {
                        path: "b.md".to_string(),
                        start_line: 1,
                        end_line: 2,
                    },
                    HistoricalBlameRequest {
                        path: "missing.md".to_string(),
                        start_line: 1,
                        end_line: 1,
                    },
                ],
            )
            .await
            .expect("run exact-version blame batch");

        assert_eq!(
            results[0]
                .as_ref()
                .expect("a range")
                .as_ref()
                .map(|attribution| attribution.author.as_str()),
            Some("Alice")
        );
        assert_eq!(
            results[1]
                .as_ref()
                .expect("b range")
                .as_ref()
                .map(|attribution| attribution.author.as_str()),
            Some("Bob")
        );
        assert_eq!(results[2].as_ref().expect("missing range"), &None);
    }

    #[tokio::test]
    async fn committed_file_comparison_batches_two_exact_authorized_versions() {
        let root = TempDir::new().expect("temp storage");
        let storage = TenantRepoStorage::local_for_test(root.path().to_path_buf());
        let repo_id = Uuid::new_v4();
        let repo_path = storage.test_path(org(), repo_id);
        fs::create_dir_all(&repo_path).expect("create repository path");
        git2::Repository::init(&repo_path).expect("initialize repository");
        fs::write(repo_path.join("same.txt"), "same\n").expect("write same base");
        fs::write(repo_path.join("changed.txt"), "before\n").expect("write changed base");
        let repo = GitRepo::open(&repo_path).expect("open repository");
        repo.add_paths(["same.txt", "changed.txt"])
            .expect("stage base files");
        let author = GitAuthor {
            name: "Reader Test".to_string(),
            email: "reader@example.com".to_string(),
        };
        let base_head = repo.commit("base", &author).expect("commit base");

        fs::write(repo_path.join("changed.txt"), "after\n").expect("change file");
        fs::write(repo_path.join("new.txt"), "new\n").expect("write new file");
        repo.add_paths(["changed.txt", "new.txt"])
            .expect("stage head files");
        let head = repo.commit("head", &author).expect("commit head");

        // Ambient checkout state is outside both authorized versions.
        fs::write(repo_path.join("same.txt"), "ambient\n").expect("write ambient content");
        let base = CommittedRepoReadView::from_historical(
            &TestOrgScope(org()),
            org(),
            &HistoricalGitVersion {
                repo_id,
                head_commit: base_head,
                reason: g5_contracts::types::HistoricalReadReason::OperationValidation,
            },
        )
        .expect("authorize base");
        let current = CommittedRepoReadView::from_historical(
            &TestOrgScope(org()),
            org(),
            &HistoricalGitVersion {
                repo_id,
                head_commit: head,
                reason: g5_contracts::types::HistoricalReadReason::OperationValidation,
            },
        )
        .expect("authorize head");

        let changed = current
            .changed_files_from(
                &storage,
                Some(&base),
                &[
                    "same.txt".to_string(),
                    "changed.txt".to_string(),
                    "new.txt".to_string(),
                    "missing.txt".to_string(),
                    "same.txt".to_string(),
                ],
            )
            .await
            .expect("compare exact versions");

        assert_eq!(changed.len(), 4, "duplicate paths are compared once");
        assert!(!changed["same.txt"]);
        assert!(changed["changed.txt"]);
        assert!(changed["new.txt"]);
        assert!(!changed["missing.txt"]);
        assert!(current
            .is_descendant_of(&storage, &base)
            .await
            .expect("prove forward ancestry"));
        assert!(!base
            .is_descendant_of(&storage, &current)
            .await
            .expect("reject reverse ancestry"));
    }

    #[test]
    fn exact_git_sha_rejects_the_unborn_zero_sentinel() {
        assert!(valid_git_sha(&"a".repeat(40)));
        assert!(!valid_git_sha(&"0".repeat(40)));
        assert!(!valid_git_sha("HEAD"));
    }
}
