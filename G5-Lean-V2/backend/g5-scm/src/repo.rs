//! Git repository wrapper providing G5-specific operations.
//!
//! [`GitRepo`] wraps `git2::Repository` and exposes the git operations needed
//! by both `g5-api` (bare repo: diff, show file, list tree) and `g5d`
//! (checkout/worktree: clone, fetch, commit, push, branch management).

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use super::auth::{GitAuth, GitAuthor};
use g5_contracts::g5_paths::is_g5_runtime_metadata_path;
use g5_contracts::G5Error;

/// Result of analyzing a branch-to-branch merge without mutating refs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchMergeAnalysis {
    pub source_head: String,
    pub target_head: String,
    pub outcome: BranchMergeAnalysisOutcome,
}

/// Conflict/no-conflict classification for a no-checkout branch merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum BranchMergeAnalysisOutcome {
    /// Source and target already resolve to the same commit.
    Identical,
    /// Source is already contained in target.
    SourceAlreadyMerged,
    /// Target can advance directly to source.
    FastForward,
    /// A normal merge commit can be created without git conflicts.
    CleanMerge,
    /// The merge index contains conflicts.
    Conflicts { paths: Vec<String> },
}

/// Attribution resolved from `git blame` for a contiguous line range.
///
/// Returned by [`GitRepo::blame_for_path`] only when the entire requested
/// range resolves to a single commit. The fields mirror the provenance shape
/// of an intent record (`created_by` / `created_at`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameAttribution {
    /// Author display name from the resolving commit's signature.
    pub author: String,
    /// Author time as an ISO-8601 UTC string (e.g. `2026-06-01T12:00:00Z`),
    /// matching the timestamp shape used by intent-record JSONL.
    pub committed_at: String,
}

/// Converts a `git2::Time` (epoch seconds) to an ISO-8601 UTC string with a
/// trailing `Z`, matching one of the two spellings used by intent records.
fn git_time_to_iso(time: &git2::Time) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(time.seconds(), 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// Outcome of [`GitRepo::apply_branch_commit`] and, per member, of
/// [`GitRepo::apply_branch_commits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchRefUpdate {
    /// The ref was moved to the new commit.
    Applied,
    /// The ref already pointed at the new commit; nothing was written.
    AlreadyApplied,
}

/// One branch ref update of a cut that may span repositories: move
/// `refs/heads/<branch>` of the repository at `repo_path` from
/// `expected_old` (all-zero: the ref must not exist) to `new_sha`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRefCommit {
    pub repo_path: PathBuf,
    pub branch: String,
    pub expected_old: String,
    pub new_sha: String,
}

/// An exact Git ref mutation, including synchronization refs. `None` denotes
/// an absent ref, allowing guarded creation and rollback of a newly made ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefCommit {
    pub repo_path: PathBuf,
    pub ref_name: String,
    pub expected_old: Option<String>,
    pub new_oid: Option<String>,
}

/// One member of a cut whose ref does not carry the head its update
/// expected (`current` is `None` when the ref does not exist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRefMismatch {
    pub index: usize,
    pub current: Option<String>,
}

/// Why [`GitRepo::apply_branch_commits`] did not apply a cut.
#[derive(Debug)]
pub enum BranchRefCutRefusal {
    /// At least one ref is at a head other than the one its update expected.
    /// Nothing was written: every ref of the cut keeps the head it had.
    Stale { mismatches: Vec<BranchRefMismatch> },
    /// Git could not open a repository, lock, inspect or write a ref.
    /// `index` names the member (when one is at fault) and `written` lists
    /// the members whose refs had already been committed when the failure
    /// struck (non-empty only when it struck while committing the locked
    /// updates); those refs stay where git put them.
    Failed {
        index: Option<usize>,
        error: Box<G5Error>,
        written: Vec<usize>,
    },
}

/// The direct target of `refname`, `None` when the ref does not exist.
fn branch_ref_target(repo: &git2::Repository, refname: &str) -> Result<Option<git2::Oid>, G5Error> {
    match repo.find_reference(refname) {
        Ok(reference) => Ok(reference
            .resolve()
            .ok()
            .and_then(|resolved| resolved.target())),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(error) => Err(G5Error::GitError {
            message: format!("Failed to inspect branch ref {refname}: {error}"),
        }),
    }
}

/// Thin wrapper around [`git2::Repository`] providing G5-specific git
/// operations.
///
/// Does not own authentication credentials — callers pass `&GitAuth` for
/// operations that need remote access.
pub struct GitRepo {
    inner: Mutex<git2::Repository>,
}

/// Repository evidence used before preparing a branch's first commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchCommitState {
    /// The selected branch peels to a commit.
    Present,
    /// HEAD is unborn and the repository has no refs of any kind.
    UnbornRepository,
    /// The selected branch is unavailable in a repository that is not unborn.
    Unavailable,
}

/// Result of a create-only workspace-fork ref transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkRefsCreateOutcome {
    Created,
    AlreadyApplied,
}

/// Additional immutable refs copied for a live workspace fork receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkCopyRefs {
    pub workspace_id: uuid::Uuid,
    pub flush_id: uuid::Uuid,
    pub snapshot_oid: String,
}

impl GitRepo {
    /// Acquires the inner repository lock.
    ///
    /// Returns `G5Error::Internal` if the mutex is poisoned.
    pub(super) fn repo(&self) -> Result<MutexGuard<'_, git2::Repository>, G5Error> {
        self.inner.lock().map_err(|_| G5Error::Internal {
            message: "git repository mutex poisoned".to_string(),
        })
    }

    /// Returns the name of the currently checked-out branch.
    ///
    /// Reads HEAD and extracts the short branch name. Returns an error if
    /// HEAD is detached.
    pub fn current_branch(&self) -> Result<String, G5Error> {
        let inner = self.repo()?;
        let head = inner.head().map_err(|e| G5Error::GitError {
            message: format!("Failed to read HEAD: {}", e),
        })?;

        if !head.is_branch() {
            return Err(G5Error::GitError {
                message: "HEAD is detached — no current branch".to_string(),
            });
        }

        head.shorthand()
            .map(str::to_string)
            .map_err(|e| G5Error::GitError {
                message: format!("HEAD branch name is not valid UTF-8: {}", e),
            })
    }

    /// Returns the working directory path, or `None` for bare repos.
    pub fn workdir(&self) -> Option<std::path::PathBuf> {
        self.repo()
            .ok()
            .and_then(|r| r.workdir().map(|p| p.to_path_buf()))
    }

    /// Constructs a `GitRepo` from a raw `git2::Repository`.
    ///
    /// Backend handles stay private to the execution crate.
    #[must_use]
    pub(crate) fn from_raw(repo: git2::Repository) -> Self {
        GitRepo {
            inner: Mutex::new(repo),
        }
    }

    /// Discover the containing checkout without exposing a backend handle.
    /// Bare repositories have no checkout root.
    pub fn discover_workdir(path: &Path) -> Result<Option<PathBuf>, G5Error> {
        let repository = git2::Repository::discover(path).map_err(crate::backend_error)?;
        Ok(repository.workdir().map(Path::to_path_buf))
    }

    /// Opens an existing repository (bare or non-bare) at the given path.
    ///
    /// Wraps `git2::Repository::open()`. Returns `G5Error::GitError` on
    /// failure.
    ///
    /// Disables strict ownership checks so that repos mounted into Docker
    /// containers (where the UID of the file owner may differ from the
    /// process UID) can still be opened.
    pub fn open(path: &Path) -> Result<Self, G5Error> {
        // Allow opening repos owned by a different user (common with Docker
        // bind mounts / named volumes).
        unsafe {
            git2::opts::set_verify_owner_validation(false).expect("set_verify_owner_validation");
        }
        let repo = git2::Repository::open(path).map_err(crate::backend_error)?;
        Ok(GitRepo {
            inner: Mutex::new(repo),
        })
    }

    /// Open existing bare/worktree storage, or initialize bare storage, and
    /// configure symbolic HEAD for a local repository's default branch.
    /// This does not create or advance that branch, write an index, or check out
    /// files. The caller separately prepares and authorizes the initial commit.
    pub fn open_or_init_with_default_branch(path: &Path, branch: &str) -> Result<Self, G5Error> {
        if !crate::is_valid_ref_name(branch) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({"branch": "Invalid Git branch name"}),
            });
        }
        let repo = git2::Repository::open_bare(path)
            .or_else(|_| git2::Repository::open(path))
            .or_else(|_| git2::Repository::init_bare(path))
            .map_err(|error| G5Error::GitError {
                message: format!(
                    "Failed to init local repository '{}': {error}",
                    path.display()
                ),
            })?;
        let reference = format!("refs/heads/{branch}");
        repo.set_head(&reference)
            .map_err(|error| G5Error::GitError {
                message: format!("Failed to set local repo HEAD to '{reference}': {error}"),
            })?;
        Ok(Self::from_raw(repo))
    }

    /// Keep a reviewed commit reachable without making it a workspace head.
    /// The name is content-addressed and cannot be retargeted.
    pub fn retain_review_commit(
        &self,
        workspace_id: uuid::Uuid,
        commit: &str,
    ) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let oid = git2::Oid::from_str(commit).map_err(crate::backend_error)?;
        repo.find_commit(oid).map_err(crate::backend_error)?;
        let name = format!("refs/g5/review-evidence/{workspace_id}/{oid}");
        match repo.find_reference(&name) {
            Ok(r) if r.target() == Some(oid) => return Ok(()),
            Ok(_) => {
                return Err(G5Error::GitError {
                    message: "Review evidence ref has conflicting contents".into(),
                })
            }
            Err(e) if e.code() == git2::ErrorCode::NotFound => {}
            Err(e) => return Err(crate::backend_error(e)),
        }
        repo.reference(&name, oid, false, "Retain workspace review evidence")
            .map_err(crate::backend_error)?;
        Ok(())
    }

    /// Inspect the selected branch without consulting checkout contents or
    /// treating a missing branch in a populated repository as bootstrap state.
    /// A found dangling/non-commit ref is an error. Other lookup failures remain
    /// unavailable; repository/ref enumeration failures are returned to the caller.
    pub fn branch_commit_state(&self, branch: &str) -> Result<BranchCommitState, G5Error> {
        let repo = self.repo()?;
        let state = match repo.find_reference(&format!("refs/heads/{branch}")) {
            Ok(reference) => {
                reference.peel_to_commit().map_err(crate::backend_error)?;
                BranchCommitState::Present
            }
            Err(error) if error.code() == git2::ErrorCode::NotFound => {
                // is_empty also compares HEAD with init.defaultBranch. An unborn
                // repository need not use that configured default name.
                let unborn_head = matches!(repo.head(), Err(error) if error.code() == git2::ErrorCode::UnbornBranch);
                let no_refs = repo
                    .references()
                    .map_err(crate::backend_error)?
                    .next()
                    .transpose()
                    .map_err(crate::backend_error)?
                    .is_none();
                if unborn_head && no_refs {
                    BranchCommitState::UnbornRepository
                } else {
                    BranchCommitState::Unavailable
                }
            }
            Err(_) => BranchCommitState::Unavailable,
        };
        Ok(state)
    }

    /// Initialize bare or worktree storage without creating a commit.
    pub fn init(path: &Path, bare: bool) -> Result<Self, G5Error> {
        let repo = if bare {
            git2::Repository::init_bare(path)
        } else {
            git2::Repository::init(path)
        }
        .map_err(|error| G5Error::GitError {
            message: format!("Failed to init repo at '{}': {error}", path.display()),
        })?;
        Ok(Self::from_raw(repo))
    }

    /// List ordinary local branch names from the repository's actual ref
    /// database. This deliberately bypasses database projections.
    pub fn local_branch_names(&self) -> Result<Vec<String>, G5Error> {
        let repo = self.repo()?;
        let mut names = Vec::new();
        let references =
            repo.references_glob("refs/heads/*")
                .map_err(|error| G5Error::GitError {
                    message: format!("Failed to enumerate local branch refs: {error}"),
                })?;
        for reference in references {
            let reference = reference.map_err(|error| G5Error::GitError {
                message: format!("Failed to enumerate local branch ref: {error}"),
            })?;
            if let Ok(name) = reference.name() {
                if let Some(name) = name.strip_prefix("refs/heads/") {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Retain an assigned object under a private, create-only evidence ref.
    /// Replay may observe exactly the same target but can never move a ref.
    pub fn create_approval_evidence_ref_if_absent_or_equal(
        &self,
        name: &str,
        expected_oid: &str,
    ) -> Result<(), G5Error> {
        if !name.starts_with(g5_contracts::types::APPROVAL_EVIDENCE_REF_PREFIX)
            || !git2::Reference::is_valid_name(name)
        {
            return Err(G5Error::GitError {
                message: "Invalid approval evidence ref".into(),
            });
        }
        let repo = self.repo()?;
        let oid = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid approval evidence commit: {error}"),
        })?;
        repo.find_commit(oid).map_err(|error| G5Error::GitError {
            message: format!("Approval evidence commit is unavailable: {error}"),
        })?;
        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start approval evidence ref transaction: {error}"),
        })?;
        transaction
            .lock_ref(name)
            .map_err(|error| G5Error::Conflict {
                reason: format!("Approval evidence ref is locked: {error}"),
            })?;
        match branch_ref_target(&repo, name)? {
            Some(current) if current == oid => return Ok(()),
            Some(_) => {
                return Err(G5Error::Conflict {
                    reason: format!("Approval evidence ref {name} differs from its assignment"),
                })
            }
            None => (),
        }
        transaction
            .set_target(name, oid, None, "g5 retained approval evidence")
            .and_then(|()| transaction.commit())
            .map_err(|error| G5Error::GitError {
                message: format!("Failed to retain approval evidence ref: {error}"),
            })
    }

    /// Create the exact destination branch and its server-only fork receipt in
    /// one repository ref transaction. An equal branch without the receipt is
    /// a conflict: equal content is not proof that this operation owns it.
    pub fn create_workspace_fork_refs(
        &self,
        branch_name: &str,
        fork_id: uuid::Uuid,
        expected_oid: &str,
    ) -> Result<ForkRefsCreateOutcome, G5Error> {
        let branch_ref = format!("refs/heads/{branch_name}");
        let receipt_ref = format!("refs/g5/forks/{fork_id}");
        let repo = self.repo()?;
        let oid = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid fork source OID '{expected_oid}': {error}"),
        })?;
        repo.find_commit(oid).map_err(|error| G5Error::GitError {
            message: format!("Fork source commit {expected_oid} is unavailable: {error}"),
        })?;
        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start fork ref transaction: {error}"),
        })?;
        transaction
            .lock_ref(&branch_ref)
            .and_then(|()| transaction.lock_ref(&receipt_ref))
            .map_err(|error| G5Error::Conflict {
                reason: format!("Fork destination ref namespace is unavailable: {error}"),
            })?;
        let branch = branch_ref_target(&repo, &branch_ref)?;
        let receipt = branch_ref_target(&repo, &receipt_ref)?;
        match (branch, receipt) {
            (None, None) => {}
            (Some(branch), Some(receipt)) if branch == oid && receipt == oid => {
                return Ok(ForkRefsCreateOutcome::AlreadyApplied);
            }
            _ => {
                return Err(G5Error::Conflict {
                    reason: format!(
                        "Fork refs conflict for {branch_ref}: branch={}, receipt={}",
                        branch.map_or_else(|| "absent".to_string(), |value| value.to_string()),
                        receipt.map_or_else(|| "absent".to_string(), |value| value.to_string())
                    ),
                });
            }
        }
        let signature =
            git2::Signature::now("g5", "g5@localhost").map_err(|error| G5Error::GitError {
                message: format!("Failed to build fork reflog signature: {error}"),
            })?;
        transaction
            .set_target(
                &branch_ref,
                oid,
                Some(&signature),
                "g5 workspace fork branch",
            )
            .and_then(|()| {
                transaction.set_target(
                    &receipt_ref,
                    oid,
                    Some(&signature),
                    "g5 workspace fork receipt",
                )
            })
            .and_then(|()| transaction.commit())
            .map_err(|error| G5Error::GitError {
                message: format!("Failed to create fork branch and receipt: {error}"),
            })?;
        Ok(ForkRefsCreateOutcome::Created)
    }

    /// Create a destination branch, fork proof, and copied flush proof refs in
    /// one ref transaction. This preserves a live source's dirty snapshot
    /// without turning that snapshot into the child's ordinary branch tip.
    pub fn create_workspace_fork_copy_refs(
        &self,
        branch_name: &str,
        fork_id: uuid::Uuid,
        expected_oid: &str,
        copy: &ForkCopyRefs,
    ) -> Result<ForkRefsCreateOutcome, G5Error> {
        let branch_ref = format!("refs/heads/{branch_name}");
        let receipt_ref = format!("refs/g5/forks/{fork_id}");
        let flush_prefix = format!("refs/g5/flush/{}/{}", copy.workspace_id, copy.flush_id);
        let flush_branch_ref = format!("{flush_prefix}/branch");
        let flush_snapshot_ref = format!("{flush_prefix}/snapshot");
        let repo = self.repo()?;
        let oid = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid fork source OID '{expected_oid}': {error}"),
        })?;
        let snapshot =
            git2::Oid::from_str(&copy.snapshot_oid).map_err(|error| G5Error::GitError {
                message: format!("Invalid fork snapshot OID '{}': {error}", copy.snapshot_oid),
            })?;
        repo.find_commit(oid).map_err(|error| G5Error::GitError {
            message: format!("Fork source commit {expected_oid} is unavailable: {error}"),
        })?;
        repo.find_commit(snapshot)
            .map_err(|error| G5Error::GitError {
                message: format!(
                    "Fork snapshot commit {} is unavailable: {error}",
                    copy.snapshot_oid
                ),
            })?;
        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start fork-copy ref transaction: {error}"),
        })?;
        for reference in [
            &branch_ref,
            &receipt_ref,
            &flush_branch_ref,
            &flush_snapshot_ref,
        ] {
            transaction
                .lock_ref(reference)
                .map_err(|error| G5Error::Conflict {
                    reason: format!("Fork destination ref namespace is unavailable: {error}"),
                })?;
        }
        let actual = [
            branch_ref_target(&repo, &branch_ref)?,
            branch_ref_target(&repo, &receipt_ref)?,
            branch_ref_target(&repo, &flush_branch_ref)?,
            branch_ref_target(&repo, &flush_snapshot_ref)?,
        ];
        let expected = [Some(oid), Some(oid), Some(oid), Some(snapshot)];
        if actual == expected {
            return Ok(ForkRefsCreateOutcome::AlreadyApplied);
        }
        if actual.iter().any(Option::is_some) {
            return Err(G5Error::Conflict {
                reason: format!("Fork-copy refs conflict for {branch_ref}"),
            });
        }
        let signature =
            git2::Signature::now("g5", "g5@localhost").map_err(|error| G5Error::GitError {
                message: format!("Failed to build fork-copy reflog signature: {error}"),
            })?;
        for (reference, target, message) in [
            (&branch_ref, oid, "g5 workspace fork branch"),
            (&receipt_ref, oid, "g5 workspace fork receipt"),
            (&flush_branch_ref, oid, "g5 workspace fork-copy branch"),
            (
                &flush_snapshot_ref,
                snapshot,
                "g5 workspace fork-copy snapshot",
            ),
        ] {
            transaction
                .set_target(reference, target, Some(&signature), message)
                .map_err(|error| G5Error::GitError {
                    message: format!("Failed to stage fork-copy ref: {error}"),
                })?;
        }
        transaction.commit().map_err(|error| G5Error::GitError {
            message: format!("Failed to create fork-copy refs: {error}"),
        })?;
        Ok(ForkRefsCreateOutcome::Created)
    }

    /// Verify the branch, fork proof, and copied flush proof refs.
    pub fn verify_workspace_fork_copy_refs(
        &self,
        branch_name: &str,
        fork_id: uuid::Uuid,
        expected_oid: &str,
        copy: &ForkCopyRefs,
    ) -> Result<(), G5Error> {
        self.verify_workspace_fork_refs(branch_name, fork_id, expected_oid)?;
        let repo = self.repo()?;
        let expected = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid fork source OID '{expected_oid}': {error}"),
        })?;
        let snapshot =
            git2::Oid::from_str(&copy.snapshot_oid).map_err(|error| G5Error::GitError {
                message: format!("Invalid fork snapshot OID '{}': {error}", copy.snapshot_oid),
            })?;
        let prefix = format!("refs/g5/flush/{}/{}", copy.workspace_id, copy.flush_id);
        if branch_ref_target(&repo, &format!("{prefix}/branch"))? != Some(expected)
            || branch_ref_target(&repo, &format!("{prefix}/snapshot"))? != Some(snapshot)
        {
            return Err(G5Error::Conflict {
                reason: format!("Fork-copy receipt no longer proves branch {branch_name}"),
            });
        }
        Ok(())
    }

    /// Remove all fork-copy refs only while every one still matches its
    /// operation-recorded OID.
    pub fn delete_workspace_fork_copy_refs_if_matches(
        &self,
        branch_name: &str,
        fork_id: uuid::Uuid,
        expected_oid: &str,
        copy: &ForkCopyRefs,
    ) -> Result<bool, G5Error> {
        let branch_ref = format!("refs/heads/{branch_name}");
        let receipt_ref = format!("refs/g5/forks/{fork_id}");
        let prefix = format!("refs/g5/flush/{}/{}", copy.workspace_id, copy.flush_id);
        let flush_branch_ref = format!("{prefix}/branch");
        let flush_snapshot_ref = format!("{prefix}/snapshot");
        let repo = self.repo()?;
        let expected = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid fork source OID '{expected_oid}': {error}"),
        })?;
        let snapshot =
            git2::Oid::from_str(&copy.snapshot_oid).map_err(|error| G5Error::GitError {
                message: format!("Invalid fork snapshot OID '{}': {error}", copy.snapshot_oid),
            })?;
        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start fork-copy cleanup transaction: {error}"),
        })?;
        for reference in [
            &branch_ref,
            &receipt_ref,
            &flush_branch_ref,
            &flush_snapshot_ref,
        ] {
            transaction
                .lock_ref(reference)
                .map_err(|error| G5Error::GitError {
                    message: format!("Failed to lock fork-copy refs for cleanup: {error}"),
                })?;
        }
        let actual = [
            branch_ref_target(&repo, &branch_ref)?,
            branch_ref_target(&repo, &receipt_ref)?,
            branch_ref_target(&repo, &flush_branch_ref)?,
            branch_ref_target(&repo, &flush_snapshot_ref)?,
        ];
        if actual.iter().all(Option::is_none) {
            return Ok(false);
        }
        // Branch creation and its operation receipt are one ref transaction.
        // A branch with no receipt and no copied-flush refs therefore predates
        // this operation; cleanup must leave that foreign collision alone.
        if actual[0].is_some() && actual[1].is_none() && actual[2].is_none() && actual[3].is_none()
        {
            return Ok(false);
        }
        if actual
            != [
                Some(expected),
                Some(expected),
                Some(expected),
                Some(snapshot),
            ]
        {
            return Err(G5Error::Conflict {
                reason: format!("Fork-copy cleanup cannot prove ownership of {branch_ref}"),
            });
        }
        for reference in [
            &branch_ref,
            &receipt_ref,
            &flush_branch_ref,
            &flush_snapshot_ref,
        ] {
            transaction
                .remove(reference)
                .map_err(|error| G5Error::GitError {
                    message: format!("Failed to stage fork-copy cleanup: {error}"),
                })?;
        }
        transaction.commit().map_err(|error| G5Error::GitError {
            message: format!("Failed to remove fork-copy refs: {error}"),
        })?;
        Ok(true)
    }

    /// Verify that both refs still prove the operation's planned write.
    pub fn verify_workspace_fork_refs(
        &self,
        branch_name: &str,
        fork_id: uuid::Uuid,
        expected_oid: &str,
    ) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let expected = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid fork source OID '{expected_oid}': {error}"),
        })?;
        let branch = branch_ref_target(&repo, &format!("refs/heads/{branch_name}"))?;
        let receipt = branch_ref_target(&repo, &format!("refs/g5/forks/{fork_id}"))?;
        if branch == Some(expected) && receipt == Some(expected) {
            Ok(())
        } else {
            Err(G5Error::Conflict {
                reason: format!(
                    "Fork receipt no longer proves branch {branch_name} at {expected_oid}"
                ),
            })
        }
    }

    /// Remove a fork branch and receipt only when both still equal the planned
    /// OID. A changed branch is preserved and reported for operator recovery.
    pub fn delete_workspace_fork_refs_if_matches(
        &self,
        branch_name: &str,
        fork_id: uuid::Uuid,
        expected_oid: &str,
    ) -> Result<bool, G5Error> {
        let branch_ref = format!("refs/heads/{branch_name}");
        let receipt_ref = format!("refs/g5/forks/{fork_id}");
        let repo = self.repo()?;
        let expected = git2::Oid::from_str(expected_oid).map_err(|error| G5Error::GitError {
            message: format!("Invalid fork source OID '{expected_oid}': {error}"),
        })?;
        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start fork cleanup transaction: {error}"),
        })?;
        transaction
            .lock_ref(&branch_ref)
            .and_then(|()| transaction.lock_ref(&receipt_ref))
            .map_err(|error| G5Error::GitError {
                message: format!("Failed to lock fork refs for cleanup: {error}"),
            })?;
        let branch = branch_ref_target(&repo, &branch_ref)?;
        let receipt = branch_ref_target(&repo, &receipt_ref)?;
        match (branch, receipt) {
            (None, None) => return Ok(false),
            // The branch and receipt are created atomically. An existing
            // branch without this operation's receipt is a foreign collision,
            // not a partially-created ref owned by this cleanup.
            (Some(_), None) => return Ok(false),
            (Some(branch), Some(receipt)) if branch == expected && receipt == expected => {}
            _ => {
                return Err(G5Error::Conflict {
                    reason: format!(
                        "Fork cleanup cannot prove ownership of {branch_ref} at {expected_oid}"
                    ),
                });
            }
        }
        transaction
            .remove(&branch_ref)
            .and_then(|()| transaction.remove(&receipt_ref))
            .and_then(|()| transaction.commit())
            .map_err(|error| G5Error::GitError {
                message: format!("Failed to remove fork branch and receipt: {error}"),
            })?;
        Ok(true)
    }

    /// Retire a checked fork's proof only after its branch is gone. The
    /// caller proves the matching workspace tombstone under repository custody.
    pub fn retire_workspace_fork_proof(
        &self,
        branch: &str,
        fork: uuid::Uuid,
        expected_oid: &str,
    ) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let reference = format!("refs/g5/forks/{fork}");
        let branch_ref = format!("refs/heads/{branch}");
        let expected = git2::Oid::from_str(expected_oid).map_err(|e| G5Error::GitError {
            message: e.to_string(),
        })?;
        let mut transaction = repo.transaction().map_err(|e| G5Error::GitError {
            message: e.to_string(),
        })?;
        transaction
            .lock_ref(&reference)
            .and_then(|()| transaction.lock_ref(&branch_ref))
            .map_err(|e| G5Error::GitError {
                message: e.to_string(),
            })?;
        if branch_ref_target(&repo, &branch_ref)?.is_some() {
            return Err(G5Error::Conflict {
                reason: "fork branch still exists while retiring its proof".into(),
            });
        }
        match branch_ref_target(&repo, &reference)? {
            None => return Ok(()),
            Some(oid) if oid == expected => {}
            _ => {
                return Err(G5Error::Conflict {
                    reason: "fork proof identity changed".into(),
                })
            }
        }
        transaction
            .remove(&reference)
            .and_then(|()| transaction.commit())
            .map_err(|e| G5Error::GitError {
                message: e.to_string(),
            })
    }

    /// Remove the mutable branch and every flush ref owned by a deleted
    /// workspace in one repository transaction.
    ///
    /// The caller serializes this operation with all other namespace writers
    /// by holding the repository's database row lock. Missing refs are an
    /// idempotent success so a retry can finish database cleanup after a lost
    /// reply or process crash. Fork proof refs are deliberately outside this
    /// ownership set and remain subject to their operation-audit retention.
    pub fn delete_workspace_refs(
        &self,
        branch_name: &str,
        workspace_id: uuid::Uuid,
        flush_ids: &[uuid::Uuid],
    ) -> Result<bool, G5Error> {
        let branch_ref = format!("refs/heads/{branch_name}");
        let mut refs = vec![branch_ref];
        let mut flush_ids = flush_ids.to_vec();
        flush_ids.sort_unstable();
        flush_ids.dedup();
        for flush_id in flush_ids {
            let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
            refs.push(format!("{prefix}/branch"));
            refs.push(format!("{prefix}/snapshot"));
        }

        let repo = self.repo()?;
        for reference in repo
            .references_glob(&format!("refs/g5/review-evidence/{workspace_id}/*"))
            .map_err(crate::backend_error)?
        {
            let reference = reference.map_err(crate::backend_error)?;
            refs.push(reference.name().map_err(crate::backend_error)?.to_owned());
        }
        refs.sort();
        refs.dedup();

        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start workspace deletion ref transaction: {error}"),
        })?;
        for reference in &refs {
            transaction
                .lock_ref(reference)
                .map_err(|error| G5Error::GitError {
                    message: format!(
                        "Failed to lock workspace deletion ref '{reference}': {error}"
                    ),
                })?;
        }

        let mut existing = Vec::new();
        for reference in refs {
            if branch_ref_target(&repo, &reference)?.is_some() {
                existing.push(reference);
            }
        }
        if existing.is_empty() {
            return Ok(false);
        }
        for reference in &existing {
            transaction
                .remove(reference)
                .map_err(|error| G5Error::GitError {
                    message: format!(
                        "Failed to stage workspace deletion ref '{reference}': {error}"
                    ),
                })?;
        }
        transaction.commit().map_err(|error| G5Error::GitError {
            message: format!("Failed to remove workspace-owned refs: {error}"),
        })?;
        Ok(true)
    }

    /// Remove the immutable snapshot and branch leaves owned by one flush
    /// receipt. The caller must hold the repository row lock and must have
    /// proved that the receipt is not current or pinned. Missing leaves are an
    /// idempotent success so an interrupted prune can be resumed safely.
    pub fn delete_workspace_flush_refs(
        &self,
        workspace_id: uuid::Uuid,
        flush_id: uuid::Uuid,
    ) -> Result<bool, G5Error> {
        let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
        let refs = [format!("{prefix}/branch"), format!("{prefix}/snapshot")];
        let repo = self.repo()?;
        let mut transaction = repo.transaction().map_err(|error| G5Error::GitError {
            message: format!("Failed to start flush prune transaction: {error}"),
        })?;
        for reference in &refs {
            transaction
                .lock_ref(reference)
                .map_err(|error| G5Error::GitError {
                    message: format!("Failed to lock flush ref '{reference}': {error}"),
                })?;
        }
        let mut existing = Vec::new();
        for reference in &refs {
            if branch_ref_target(&repo, reference)?.is_some() {
                existing.push(reference);
            }
        }
        if existing.is_empty() {
            return Ok(false);
        }
        for reference in existing {
            transaction
                .remove(reference)
                .map_err(|error| G5Error::GitError {
                    message: format!("Failed to stage flush ref removal '{reference}': {error}"),
                })?;
        }
        transaction.commit().map_err(|error| G5Error::GitError {
            message: format!("Failed to commit flush ref pruning: {error}"),
        })?;
        Ok(true)
    }

    /// Resolves the tip commit SHA of a local branch (`refs/heads/{branch}`).
    ///
    /// Returns the full hex SHA, or an error if the branch does not exist.
    pub fn resolve_branch_head(&self, branch: &str) -> Result<String, G5Error> {
        let refname = format!("refs/heads/{}", branch);
        let repo = self.repo()?;
        let reference = repo
            .find_reference(&refname)
            .map_err(|e| G5Error::GitError {
                message: format!("Branch '{}' not found: {}", branch, e.message()),
            })?;
        let commit = reference.peel_to_commit().map_err(|e| G5Error::GitError {
            message: format!("Failed to peel {} to commit: {}", branch, e.message()),
        })?;
        Ok(commit.id().to_string())
    }

    /// Rebases commits that exist on `source_sha` but not on `target_sha`
    /// onto the target commit.
    ///
    /// Walks the commit range `merge_base..source_sha`, then cherry-picks
    /// each commit (in topological order) on top of `target_sha`.
    ///
    /// If the source is already an ancestor of the target (nothing new),
    /// returns the target SHA unchanged.  Returns an error on conflicts.
    ///
    /// Fast-forward/cherry-pick only: two histories with **no merge base**
    /// (unrelated histories) always return `G5Error::GitError`.
    pub fn rebase_onto(
        &self,
        source_sha: &str,
        target_sha: &str,
        source_label: &str,
        target_label: &str,
    ) -> Result<String, G5Error> {
        self.rebase_onto_impl(source_sha, target_sha, source_label, target_label, false)
    }

    fn rebase_onto_impl(
        &self,
        source_sha: &str,
        target_sha: &str,
        source_label: &str,
        target_label: &str,
        allow_unrelated: bool,
    ) -> Result<String, G5Error> {
        let repo = self.repo()?;

        let source_oid = git2::Oid::from_str(source_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid source SHA '{}': {}", source_sha, e),
        })?;
        let target_oid = git2::Oid::from_str(target_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid target SHA '{}': {}", target_sha, e),
        })?;

        // Already merged — source is ancestor of target.
        if repo
            .graph_descendant_of(target_oid, source_oid)
            .unwrap_or(false)
        {
            return Ok(target_sha.to_string());
        }

        // Find merge base. Unrelated histories (no common ancestor) either take
        // the gated allow-unrelated fallback (replay LOCAL onto the REMOTE tip)
        // or keep the historical error for callers that did not opt in.
        let merge_base_oid = match repo.merge_base(target_oid, source_oid) {
            Ok(oid) => oid,
            Err(e) => {
                if allow_unrelated {
                    return reconcile_unrelated_onto(
                        &repo,
                        source_oid,
                        target_oid,
                        source_label,
                        target_label,
                    );
                }
                return Err(G5Error::GitError {
                    message: format!(
                        "No merge base between '{}' and '{}': {}",
                        target_label, source_label, e
                    ),
                });
            }
        };

        // Walk commits from merge_base (exclusive) to source_sha (inclusive)
        // in topological + time order so we replay them oldest-first.
        let mut revwalk = repo.revwalk().map_err(|e| G5Error::GitError {
            message: format!("Failed to create revwalk: {}", e),
        })?;
        revwalk.push(source_oid).map_err(|e| G5Error::GitError {
            message: format!("revwalk push failed: {}", e),
        })?;
        revwalk
            .hide(merge_base_oid)
            .map_err(|e| G5Error::GitError {
                message: format!("revwalk hide failed: {}", e),
            })?;
        revwalk
            .set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)
            .map_err(crate::backend_error)?;

        let commit_oids: Vec<git2::Oid> =
            revwalk
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| G5Error::GitError {
                    message: format!("revwalk iteration failed: {}", e),
                })?;

        if commit_oids.is_empty() {
            return Ok(target_sha.to_string());
        }

        // Cherry-pick each commit onto the current tip, starting from target.
        let mut current_tip_oid = target_oid;

        for oid in &commit_oids {
            let commit = repo.find_commit(*oid).map_err(|e| G5Error::GitError {
                message: format!("Commit {} not found: {}", oid, e),
            })?;

            let parent = commit.parent(0).map_err(|e| G5Error::GitError {
                message: format!("Commit {} has no parent: {}", oid, e),
            })?;

            let tip_commit = repo
                .find_commit(current_tip_oid)
                .map_err(crate::backend_error)?;

            // 3-way merge: parent_tree (base) + tip_tree (ours) + commit_tree (theirs)
            let mut merge_index = repo
                .merge_trees(
                    &parent.tree().map_err(crate::backend_error)?,
                    &tip_commit.tree().map_err(crate::backend_error)?,
                    &commit.tree().map_err(crate::backend_error)?,
                    None,
                )
                .map_err(|e| G5Error::GitError {
                    message: format!("Cherry-pick merge_trees failed for {}: {}", oid, e),
                })?;

            if merge_index.has_conflicts() {
                return Err(G5Error::MergeConflict {
                    message: format!(
                        "Rebase of '{}' onto '{}' has conflicts at commit {}",
                        source_label, target_label, oid
                    ),
                });
            }

            let tree_oid = merge_index
                .write_tree_to(&repo)
                .map_err(|e| G5Error::GitError {
                    message: format!("Failed to write rebased tree for {}: {}", oid, e),
                })?;
            let tree = repo.find_tree(tree_oid).map_err(crate::backend_error)?;

            // Re-use the original commit's author and message.
            let author = commit.author();
            let sig = git2::Signature::now("G5 System", "g5@system.local")
                .map_err(crate::backend_error)?;

            let new_oid = repo
                .commit(
                    None,
                    &author,
                    &sig,
                    commit.message().unwrap_or(""),
                    &tree,
                    &[&tip_commit],
                )
                .map_err(|e| G5Error::GitError {
                    message: format!("Failed to create rebased commit for {}: {}", oid, e),
                })?;

            current_tip_oid = new_oid;
        }

        Ok(current_tip_oid.to_string())
    }

    /// Preserves an operator-reviewed candidate tree while making publication
    /// descend from both the exact prior authority and reviewed remote tips.
    ///
    /// The commit object is deterministic and no ref or worktree is changed.
    pub fn normalize_reviewed_publication_tip(
        &self,
        reviewed_candidate_tip: &str,
        previous_authority_tip: &str,
        reviewed_remote_tip: &str,
    ) -> Result<String, G5Error> {
        let repo = self.repo()?;
        let parse_commit = |label: &str, sha: &str| {
            let oid = git2::Oid::from_str(sha).map_err(|error| G5Error::GitError {
                message: format!("Invalid {label} SHA '{sha}': {error}"),
            })?;
            repo.find_commit(oid).map_err(|error| G5Error::GitError {
                message: format!("{label} commit {sha} is unavailable: {error}"),
            })
        };
        let candidate = parse_commit("reviewed candidate", reviewed_candidate_tip)?;
        let previous = parse_commit("previous authority", previous_authority_tip)?;
        let remote = parse_commit("reviewed remote", reviewed_remote_tip)?;

        let descends_from = |tip: git2::Oid, ancestor: git2::Oid| {
            if tip == ancestor {
                Ok(true)
            } else {
                repo.graph_descendant_of(tip, ancestor)
                    .map_err(|error| G5Error::GitError {
                        message: format!(
                            "Failed to compare reviewed publication tip {tip} with ancestor \
                             {ancestor}: {error}"
                        ),
                    })
            }
        };

        let mut parents = vec![candidate];
        if !descends_from(parents[0].id(), previous.id())? {
            parents.push(previous);
        }
        let preserves_remote = parents
            .iter()
            .map(|parent| descends_from(parent.id(), remote.id()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .any(|preserves| preserves);
        if !preserves_remote && parents.iter().all(|parent| parent.id() != remote.id()) {
            parents.push(remote);
        }
        if parents.len() == 1 {
            return Ok(reviewed_candidate_tip.to_string());
        }

        let tree = parents[0].tree().map_err(crate::backend_error)?;
        let timestamp = parents
            .iter()
            .map(|parent| parent.time().seconds())
            .max()
            .unwrap_or_default()
            .saturating_add(1);
        let signature = git2::Signature::new(
            "G5 Authority",
            "authority@g5.local",
            &git2::Time::new(timestamp, 0),
        )
        .map_err(crate::backend_error)?;
        let parent_refs = parents.iter().collect::<Vec<_>>();
        let commit_oid = repo
            .commit(
                None,
                &signature,
                &signature,
                "g5: preserve reviewed publication ancestry",
                &tree,
                &parent_refs,
            )
            .map_err(|error| G5Error::GitError {
                message: format!(
                    "Failed to normalize reviewed candidate {reviewed_candidate_tip} against \
                     authority {previous_authority_tip} and remote {reviewed_remote_tip}: {error}"
                ),
            })?;

        Ok(commit_oid.to_string())
    }

    /// Enumerate local branch refs and peel each target to a commit. Non-branch
    /// refs are excluded. An unreadable ref or unavailable commit fails the read;
    /// it must not masquerade as an empty mirror or a deleted branch.
    pub fn head_ref_commits(&self) -> Result<Vec<(String, String)>, G5Error> {
        let repo = self.repo()?;
        let mut refs = Vec::new();
        for reference in repo
            .references_glob("refs/heads/*")
            .map_err(crate::backend_error)?
        {
            let reference = reference.map_err(crate::backend_error)?;
            refs.push((
                reference.name().map_err(crate::backend_error)?.to_owned(),
                reference
                    .peel_to_commit()
                    .map_err(crate::backend_error)?
                    .id()
                    .to_string(),
            ));
        }
        refs.sort();
        Ok(refs)
    }

    /// Peel an exact ref to its commit. Only an absent ref returns `None`;
    /// dangling symbolic refs and non-commit targets are errors. Maintenance
    /// callers retain ownership of repository selection and read authority.
    pub fn ref_commit_oid(&self, ref_name: &str) -> Result<Option<String>, G5Error> {
        let repo = self.repo()?;
        let reference = match repo.find_reference(ref_name) {
            Ok(reference) => reference,
            Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(None),
            Err(error) => return Err(crate::backend_error(error)),
        };
        let oid = reference
            .peel_to_commit()
            .map_err(crate::backend_error)?
            .id()
            .to_string();
        Ok(Some(oid))
    }

    /// Reads the commit OID that `refs/heads/{branch}` currently points at.
    ///
    /// This is the read-side head authority for a workspace branch: the
    /// mirror's ref, never a database column. Symbolic refs are followed; the
    /// target object is deliberately not inspected, so a ref left pointing at
    /// a missing object is reported as-is and fails at the content read like
    /// any other unavailable commit. Returns `None` when the branch does not
    /// exist in this repository.
    pub fn read_branch_oid(&self, branch: &str) -> Result<Option<String>, G5Error> {
        let refname = format!("refs/heads/{branch}");
        let repo = self.repo()?;
        let reference = match repo.find_reference(&refname) {
            Ok(reference) => reference,
            Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(None),
            Err(error) => {
                return Err(G5Error::GitError {
                    message: format!("Failed to read branch ref {refname}: {error}"),
                });
            }
        };
        let resolved = match reference.resolve() {
            Ok(resolved) => resolved,
            Err(error) if error.code() == git2::ErrorCode::NotFound => return Ok(None),
            Err(error) => {
                return Err(G5Error::GitError {
                    message: format!("Failed to resolve branch ref {refname}: {error}"),
                });
            }
        };
        let oid = resolved.target().ok_or_else(|| G5Error::GitError {
            message: format!("Branch ref {refname} has no direct commit target"),
        })?;
        Ok(Some(oid.to_string()))
    }

    /// Applies a commit to `refs/heads/{branch}` under git's own update rule.
    ///
    /// The ref must currently equal `expected_old` (the all-zero SHA means the
    /// ref must not exist yet) and is then moved to `new_sha`, which must be a
    /// commit present in this repository. A ref already at `new_sha` is an
    /// idempotent replay. A ref at any other value wins: the update is refused
    /// with [`G5Error::Conflict`] and nothing is written, because the ref is
    /// the head authority and the caller's expectation is what is stale.
    pub fn apply_branch_commit(
        &self,
        branch: &str,
        expected_old: &str,
        new_sha: &str,
    ) -> Result<BranchRefUpdate, G5Error> {
        let repo_path = {
            let repo = self.repo()?;
            repo.path().to_path_buf()
        };
        let refname = format!("refs/heads/{branch}");
        Self::apply_branch_commits(&[BranchRefCommit {
            repo_path,
            branch: branch.to_owned(),
            expected_old: expected_old.to_owned(),
            new_sha: new_sha.to_owned(),
        }])
        .map(|mut applied| applied.remove(0))
        .map_err(|refusal| match refusal {
            BranchRefCutRefusal::Stale { mismatches } => G5Error::Conflict {
                reason: format!(
                    "Branch ref {refname} is at {} instead of the expected {expected_old}; the ref is authoritative and the caller's expectation is stale",
                    mismatches
                        .first()
                        .and_then(|mismatch| mismatch.current.clone())
                        .unwrap_or_else(|| "<absent>".to_string())
                ),
            },
            BranchRefCutRefusal::Failed { error, .. } => *error,
        })
    }

    /// Apply a cut of branch ref updates, possibly across repositories, as
    /// one decision. Every ref is locked with git's own ref lock, which keeps
    /// every other writer out (a concurrent `git update-ref` fails to lock);
    /// every expectation is verified while the locks are held; only then are
    /// the locked updates written and committed. A ref already at its new
    /// commit is an idempotent replay. A ref at any other head refuses the
    /// whole cut before anything is written, so a refused cut never moves a
    /// ref, not even transiently, and there is no compensation that could
    /// undo a write git accepted from someone else. A failure while
    /// committing the locked updates is reported with the members already
    /// written; their refs stay where git put them.
    pub fn apply_branch_commits(
        updates: &[BranchRefCommit],
    ) -> Result<Vec<BranchRefUpdate>, BranchRefCutRefusal> {
        let refs = updates
            .iter()
            .map(|update| RefCommit {
                repo_path: update.repo_path.clone(),
                ref_name: format!("refs/heads/{}", update.branch),
                expected_old: (update.expected_old != "0000000000000000000000000000000000000000")
                    .then(|| update.expected_old.clone()),
                new_oid: Some(update.new_sha.clone()),
            })
            .collect::<Vec<_>>();
        Self::apply_ref_commits(&refs)
    }

    /// Lock and validate the complete cut before publishing any ref. Updates
    /// are committed in input order; an I/O failure reports the written prefix.
    pub fn apply_ref_commits(
        updates: &[RefCommit],
    ) -> Result<Vec<BranchRefUpdate>, BranchRefCutRefusal> {
        let failed = |index: Option<usize>, error: G5Error| BranchRefCutRefusal::Failed {
            index,
            error: Box::new(error),
            written: Vec::new(),
        };

        // Phase 1: open every repository and validate every input before a
        // single lock is taken.
        let mut repos = Vec::with_capacity(updates.len());
        let mut targets = Vec::with_capacity(updates.len());
        for (index, update) in updates.iter().enumerate() {
            let repo = git2::Repository::open(&update.repo_path).map_err(|error| {
                failed(
                    Some(index),
                    G5Error::GitError {
                        message: format!(
                            "Failed to open repository {}: {error}",
                            update.repo_path.display()
                        ),
                    },
                )
            })?;
            if !git2::Reference::is_valid_name(&update.ref_name)
                || updates[..index].iter().any(|previous| {
                    previous.repo_path == update.repo_path && previous.ref_name == update.ref_name
                })
            {
                return Err(failed(
                    Some(index),
                    G5Error::GitError {
                        message: format!("Invalid or duplicate ref in cut: {}", update.ref_name),
                    },
                ));
            }
            let parse = |value: &str| -> Result<git2::Oid, BranchRefCutRefusal> {
                let oid = git2::Oid::from_str(value).map_err(|error| {
                    failed(
                        Some(index),
                        G5Error::GitError {
                            message: format!("Invalid commit OID {value}: {error}"),
                        },
                    )
                })?;
                if oid.is_zero() {
                    return Err(failed(
                        Some(index),
                        G5Error::GitError {
                            message: "Use an absent ref expectation instead of a zero OID".into(),
                        },
                    ));
                }
                Ok(oid)
            };
            let new_oid = update.new_oid.as_deref().map(parse).transpose()?;
            if let Some(oid) = new_oid {
                repo.find_commit(oid).map_err(|error| {
                    failed(
                        Some(index),
                        G5Error::GitError {
                            message: format!(
                                "Commit {oid} is unavailable in the repository: {error}"
                            ),
                        },
                    )
                })?;
            }
            let expected = update.expected_old.as_deref().map(parse).transpose()?;
            repos.push(repo);
            targets.push((new_oid, expected));
        }

        // Phase 2: lock every ref, then verify every expectation under the
        // locks. Dropping the transactions releases the locks unchanged.
        let mut transactions = Vec::with_capacity(updates.len());
        let mut outcomes = Vec::with_capacity(updates.len());
        let mut mismatches = Vec::new();
        for (index, update) in updates.iter().enumerate() {
            let refname = &update.ref_name;
            let repo = &repos[index];
            let mut transaction = repo.transaction().map_err(|error| {
                failed(
                    Some(index),
                    G5Error::GitError {
                        message: format!(
                            "Failed to start a ref transaction for {refname}: {error}"
                        ),
                    },
                )
            })?;
            transaction.lock_ref(refname).map_err(|error| {
                failed(
                    Some(index),
                    G5Error::GitError {
                        message: format!(
                            "Branch ref {refname} could not be locked (another writer holds it?): {error}"
                        ),
                    },
                )
            })?;
            let current =
                branch_ref_target(repo, refname).map_err(|error| failed(Some(index), error))?;
            let (new_oid, expected) = targets[index];
            let outcome = if current == new_oid {
                BranchRefUpdate::AlreadyApplied
            } else {
                let matches_expectation = current == expected;
                if !matches_expectation {
                    mismatches.push(BranchRefMismatch {
                        index,
                        current: current.map(|oid| oid.to_string()),
                    });
                }
                BranchRefUpdate::Applied
            };
            outcomes.push(outcome);
            transactions.push(transaction);
        }
        if !mismatches.is_empty() {
            return Err(BranchRefCutRefusal::Stale { mismatches });
        }

        // Phase 3: stage every write on its locked ref.
        let signature = git2::Signature::now("g5", "g5@localhost").map_err(|error| {
            failed(
                None,
                G5Error::GitError {
                    message: format!("Failed to build the reflog signature: {error}"),
                },
            )
        })?;
        for (index, update) in updates.iter().enumerate() {
            if outcomes[index] != BranchRefUpdate::Applied {
                continue;
            }
            let refname = &update.ref_name;
            let staged = match targets[index].0 {
                Some(oid) => transactions[index].set_target(
                    refname,
                    oid,
                    Some(&signature),
                    "g5 apply ref cut",
                ),
                None => transactions[index].remove(refname),
            };
            staged.map_err(|error| {
                failed(
                    Some(index),
                    G5Error::GitError {
                        message: format!("Failed to stage ref {refname}: {error}"),
                    },
                )
            })?;
        }

        // Phase 4: commit the locked updates in order. Every lock is still
        // held, so nothing but an I/O failure can stop a commit.
        let mut written = Vec::new();
        for (index, transaction) in transactions.into_iter().enumerate() {
            if let Err(error) = transaction.commit() {
                return Err(BranchRefCutRefusal::Failed {
                    index: Some(index),
                    error: Box::new(G5Error::GitError {
                        message: format!(
                            "Failed to commit ref {}: {error}",
                            updates[index].ref_name
                        ),
                    }),
                    written,
                });
            }
            if outcomes[index] == BranchRefUpdate::Applied {
                written.push(index);
            }
        }
        Ok(outcomes)
    }

    /// Build a watermark mutation for an atomic multi-ref publication. Ref
    /// naming stays inside SCM; callers carry only expected and proposed tips.
    pub fn sync_watermark_update(
        repo_path: PathBuf,
        local_branch: &str,
        expected: Option<String>,
        new: Option<String>,
    ) -> RefCommit {
        RefCommit {
            repo_path,
            ref_name: Self::sync_watermark_ref(local_branch),
            expected_old: expected,
            new_oid: new,
        }
    }

    fn sync_watermark_ref(local_branch: &str) -> String {
        format!("refs/g5-tracking/{local_branch}/baseline")
    }

    /// The synchronization watermark is Git state, scoped to the local ref.
    /// It is independent from parent-workspace comparisons and fork history.
    pub fn read_sync_watermark(&self, local_branch: &str) -> Result<Option<String>, G5Error> {
        let repo = self.repo()?;
        let name = Self::sync_watermark_ref(local_branch);
        let oid = branch_ref_target(&repo, &name)?;
        if let Some(oid) = oid {
            repo.find_commit(oid).map_err(crate::backend_error)?;
        }
        Ok(oid.map(|oid| oid.to_string()))
    }

    /// Read the last delivered provider observation for operation evidence.
    /// It is not fresh provider authority; publication must revalidate it.
    pub fn read_provider_observation(
        &self,
        workspace_id: uuid::Uuid,
        branch: &str,
    ) -> Result<Option<String>, G5Error> {
        let repo = self.repo()?;
        let name = g5_contracts::workspace_git::provider_ref(workspace_id, branch);
        let oid = branch_ref_target(&repo, &name)?;
        if let Some(oid) = oid {
            repo.find_commit(oid).map_err(crate::backend_error)?;
        }
        Ok(oid.map(|oid| oid.to_string()))
    }

    /// A provider observation is a transport ref, never workspace authority.
    /// Return a fenced update so the integration can commit it with its receipt.
    pub fn provider_observation_update(
        repo_path: PathBuf,
        workspace_id: uuid::Uuid,
        branch: &str,
        tip: &str,
    ) -> Result<RefCommit, G5Error> {
        let ref_name = g5_contracts::workspace_git::provider_ref(workspace_id, branch);
        let repo = git2::Repository::open(&repo_path).map_err(crate::backend_error)?;
        repo.find_commit(git2::Oid::from_str(tip).map_err(crate::backend_error)?)
            .map_err(crate::backend_error)?;
        let expected_old = branch_ref_target(&repo, &ref_name)?.map(|oid| oid.to_string());
        Ok(RefCommit {
            repo_path,
            ref_name,
            expected_old,
            new_oid: Some(tip.into()),
        })
    }

    /// Writes the Git synchronization watermark for fixture/bootstrap setup.
    /// Production publication uses compare-and-swap with the selected local ref
    /// through `apply_ref_commits` and `sync_watermark_update`; there is no SQL
    /// head registry.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn write_baseline_ref(&self, local_branch_name: &str, sha: &str) -> Result<(), G5Error> {
        let refname = Self::sync_watermark_ref(local_branch_name);
        let repo = self.repo()?;
        let oid = git2::Oid::from_str(sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid baseline SHA '{sha}': {e}"),
        })?;
        repo.reference(&refname, oid, true, "g5 remote-tracking baseline update")
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to update baseline ref {refname}: {e}"),
            })?;
        Ok(())
    }

    /// Analyzes merging two exact heads without modifying refs, objects, worktree,
    /// or HEAD. Callers pass the heads they already verified, so no mutable
    /// branch is re-read between their check and this analysis.
    ///
    /// This reports raw Git conflicts instead of applying G5 structured-file
    /// conflict resolution. Callers use it to decide whether "direct merge"
    /// is a user-visible no-git-conflict option.
    pub fn analyze_head_merge_no_checkout(
        &self,
        source_head: &str,
        target_head: &str,
    ) -> Result<BranchMergeAnalysis, G5Error> {
        let repo = self.repo()?;
        let commit = |label: &str, sha: &str| {
            git2::Oid::from_str(sha)
                .and_then(|oid| repo.find_commit(oid))
                .map_err(|e| G5Error::GitError {
                    message: format!("{label} head '{sha}' is not a commit: {}", e.message()),
                })
        };
        let source_commit = commit("Source", source_head)?;
        let target_commit = commit("Target", target_head)?;

        let source_head = source_commit.id().to_string();
        let target_head = target_commit.id().to_string();

        if source_commit.id() == target_commit.id() {
            return Ok(BranchMergeAnalysis {
                source_head,
                target_head,
                outcome: BranchMergeAnalysisOutcome::Identical,
            });
        }

        if repo
            .graph_descendant_of(target_commit.id(), source_commit.id())
            .unwrap_or(false)
        {
            return Ok(BranchMergeAnalysis {
                source_head,
                target_head,
                outcome: BranchMergeAnalysisOutcome::SourceAlreadyMerged,
            });
        }

        if repo
            .graph_descendant_of(source_commit.id(), target_commit.id())
            .unwrap_or(false)
        {
            return Ok(BranchMergeAnalysis {
                source_head,
                target_head,
                outcome: BranchMergeAnalysisOutcome::FastForward,
            });
        }

        let merge_base_oid = repo
            .merge_base(target_commit.id(), source_commit.id())
            .map_err(|e| G5Error::GitError {
                message: format!(
                    "No merge base between '{}' and '{}': {}",
                    target_head,
                    source_head,
                    e.message()
                ),
            })?;
        let merge_base_commit = repo
            .find_commit(merge_base_oid)
            .map_err(crate::backend_error)?;
        let merge_base_tree = merge_base_commit.tree().map_err(crate::backend_error)?;
        let target_tree = target_commit.tree().map_err(crate::backend_error)?;
        let source_tree = source_commit.tree().map_err(crate::backend_error)?;

        let merge_index = repo
            .merge_trees(&merge_base_tree, &target_tree, &source_tree, None)
            .map_err(|e| G5Error::GitError {
                message: format!("merge_trees failed: {}", e),
            })?;

        let outcome = if merge_index.has_conflicts() {
            let conflicts = collect_index_conflicts(&merge_index)?;
            BranchMergeAnalysisOutcome::Conflicts {
                paths: conflict_paths(&conflicts)?,
            }
        } else {
            BranchMergeAnalysisOutcome::CleanMerge
        };

        Ok(BranchMergeAnalysis {
            source_head,
            target_head,
            outcome,
        })
    }

    /// Creates a clean merge result from two exact commit SHAs without
    /// consulting or updating any branch ref.
    ///
    /// Authority callers must use this form after resolving their committed
    /// cut from the database. The branch labels are diagnostic only.
    pub fn create_clean_merge_commit_from_heads_no_checkout(
        &self,
        source_head: &str,
        target_head: &str,
        source_label: &str,
        target_label: &str,
        author: &GitAuthor,
    ) -> Result<String, G5Error> {
        let repo = self.repo()?;
        let source_oid = git2::Oid::from_str(source_head).map_err(|error| G5Error::GitError {
            message: format!("Invalid source commit '{source_head}': {error}"),
        })?;
        let target_oid = git2::Oid::from_str(target_head).map_err(|error| G5Error::GitError {
            message: format!("Invalid target commit '{target_head}': {error}"),
        })?;
        let source_commit = repo
            .find_commit(source_oid)
            .map_err(|error| G5Error::GitError {
                message: format!(
                    "Exact source commit '{source_head}' ({source_label}) is unavailable: {error}"
                ),
            })?;
        let target_commit = repo
            .find_commit(target_oid)
            .map_err(|error| G5Error::GitError {
                message: format!(
                    "Exact target commit '{target_head}' ({target_label}) is unavailable: {error}"
                ),
            })?;

        if source_commit.id() == target_commit.id()
            || repo
                .graph_descendant_of(target_commit.id(), source_commit.id())
                .unwrap_or(false)
        {
            return Ok(target_head.to_string());
        }

        if repo
            .graph_descendant_of(source_commit.id(), target_commit.id())
            .unwrap_or(false)
        {
            return Ok(source_head.to_string());
        }

        let merge_base_oid = repo
            .merge_base(target_commit.id(), source_commit.id())
            .map_err(|e| G5Error::GitError {
                message: format!(
                    "No merge base between '{}' and '{}': {}",
                    target_label,
                    source_label,
                    e.message()
                ),
            })?;
        let merge_base_commit = repo
            .find_commit(merge_base_oid)
            .map_err(crate::backend_error)?;
        let merge_base_tree = merge_base_commit.tree().map_err(crate::backend_error)?;
        let target_tree = target_commit.tree().map_err(crate::backend_error)?;
        let source_tree = source_commit.tree().map_err(crate::backend_error)?;

        let mut merge_index = repo
            .merge_trees(&merge_base_tree, &target_tree, &source_tree, None)
            .map_err(|e| G5Error::GitError {
                message: format!("merge_trees failed: {}", e),
            })?;

        if merge_index.has_conflicts() {
            let conflicts = collect_index_conflicts(&merge_index)?;
            let paths = conflict_paths(&conflicts)?;
            let suffix = if paths.is_empty() {
                String::new()
            } else {
                format!(": {}", paths.join(", "))
            };
            return Err(G5Error::GitError {
                message: format!(
                    "Merge of '{}' into '{}' has conflicts{}",
                    source_label, target_label, suffix
                ),
            });
        }

        let tree_oid = merge_index
            .write_tree_to(&repo)
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to write merged tree: {}", e),
            })?;
        let tree = repo.find_tree(tree_oid).map_err(crate::backend_error)?;

        let sig =
            git2::Signature::now(&author.name, &author.email).map_err(crate::backend_error)?;
        let message = format!("Merge '{}' into '{}'", source_label, target_label);
        let merge_oid = repo
            .commit(
                None,
                &sig,
                &sig,
                &message,
                &tree,
                &[&target_commit, &source_commit],
            )
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to create merge commit: {}", e),
            })?;

        Ok(merge_oid.to_string())
    }

    /// Push an exact local commit through the caller's native runner. The
    /// destination is the named remote's configured push URL; native Git
    /// rechecks it and owns credential injection, deadlines and outcomes.
    fn push_commit_native(
        &self,
        git: &crate::native::NativeGit,
        remote: &str,
        commit_sha: &str,
        remote_branch: &str,
        update: crate::native::PushUpdate,
        auth: &GitAuth,
    ) -> Result<(), G5Error> {
        use crate::native::{BranchPush, FailureKind, PushSource, PushUpdate};
        let (repository, expected_url) = self.native_push_target(remote, commit_sha)?;
        let lease = match &update {
            PushUpdate::Lease { expected } => Some(expected.clone().unwrap_or_default()),
            PushUpdate::FastForward => None,
        };
        git.push_branch_blocking(
            BranchPush {
                repository,
                remote: remote.into(),
                expected_url,
                source: PushSource::Commit(commit_sha.to_owned()),
                branch: remote_branch.into(),
                update,
            },
            auth,
        )
        .map(|_| ())
        .map_err(|error| match (error.kind, lease) {
            (FailureKind::Rejected, Some(expected)) => G5Error::GitError {
                message: format!(
                    "Force-with-lease rejected for '{remote}/{remote_branch}': expected {}; {error}",
                    if expected.is_empty() { "absent" } else { expected.as_str() }
                ),
            },
            _ => error.into(),
        })
    }

    /// The repository path and configured push URL for a native push of an
    /// exact commit that must already be present locally.
    fn native_push_target(
        &self,
        remote: &str,
        commit_sha: &str,
    ) -> Result<(PathBuf, String), G5Error> {
        let repo = self.repo()?;
        let oid = git2::Oid::from_str(commit_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{commit_sha}': {e}"),
        })?;
        repo.find_commit(oid).map_err(|e| G5Error::GitError {
            message: format!("commit '{commit_sha}' is not present for push to '{remote}': {e}"),
        })?;
        let handle = repo
            .find_remote(remote)
            .map_err(|error| G5Error::GitError {
                message: format!("Failed to find remote '{remote}': {error}"),
            })?;
        let expected_url = handle
            .pushurl()
            .map_err(crate::backend_error)?
            .unwrap_or(handle.url().map_err(crate::backend_error)?)
            .to_owned();
        Ok((
            repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf(),
            expected_url,
        ))
    }

    /// Publish an exact commit given the caller's observed remote tip:
    ///
    /// - an absent tip, or one equal to or an ancestor of `commit_sha`, is a
    ///   fast-forward (or create) push;
    /// - otherwise the history was rewritten, and the push is leased on the
    ///   observed tip so a concurrent third-party advance is refused rather
    ///   than clobbered. A tip whose commit is not present locally cannot be
    ///   proven an ancestor and conservatively takes the lease path.
    ///
    /// The caller observes the tip and owns head-authority checks; this only
    /// chooses the push shape. It never re-resolves a mutable source branch.
    pub fn publish_commit(
        &self,
        git: &crate::native::NativeGit,
        remote: &str,
        commit_sha: &str,
        remote_branch: &str,
        observed_remote_tip: Option<&str>,
        auth: &GitAuth,
    ) -> Result<(), G5Error> {
        use crate::native::PushUpdate;
        let update = match observed_remote_tip {
            Some(tip)
                if tip != commit_sha && !self.is_ancestor_of(tip, commit_sha).unwrap_or(false) =>
            {
                PushUpdate::Lease {
                    expected: Some(tip.to_owned()),
                }
            }
            _ => PushUpdate::FastForward,
        };
        self.push_commit_native(git, remote, commit_sha, remote_branch, update, auth)
    }

    /// Update an existing provider branch at an exact observed tip. The lease
    /// rejects deletion or a concurrent move, so implicit publication can never
    /// recreate a branch. An ancestry check permits only fast-forwards unless
    /// `replace` is set: the caller has classified an accepted workspace
    /// rewrite against this exact, unchanged tip, and the lease alone governs.
    #[allow(clippy::too_many_arguments)]
    pub fn push_existing_commit(
        &self,
        git: &crate::native::NativeGit,
        remote: &str,
        commit_sha: &str,
        remote_branch: &str,
        expected_remote_sha: &str,
        replace: bool,
        auth: &GitAuth,
    ) -> Result<(), G5Error> {
        let expected = git2::Oid::from_str(expected_remote_sha).map_err(crate::backend_error)?;
        if expected.is_zero()
            || (!replace
                && commit_sha != expected_remote_sha
                && !self.is_ancestor_of(expected_remote_sha, commit_sha)?)
        {
            return Err(G5Error::GitError {
                message: "Provider publication requires an existing ancestor tip".into(),
            });
        }
        self.push_commit_native(
            git,
            remote,
            commit_sha,
            remote_branch,
            crate::native::PushUpdate::Lease {
                expected: Some(expected_remote_sha.to_owned()),
            },
            auth,
        )
    }

    /// Publish an already-present commit to a brand-new remote branch. The
    /// push is leased on an absent destination, so a pre-existing branch is
    /// refused by Git itself rather than probed first and then fast-forwarded.
    /// Used by the workspace-fork isolate path to mint `g5/<clone-name>`.
    pub fn push_create_ref(
        &self,
        git: &crate::native::NativeGit,
        remote: &str,
        commit_sha: &str,
        remote_branch: &str,
        auth: &GitAuth,
    ) -> Result<(), G5Error> {
        self.push_commit_native(
            git,
            remote,
            commit_sha,
            remote_branch,
            crate::native::PushUpdate::Lease { expected: None },
            auth,
        )
    }

    /// All best common ancestors of two exact commits. An unrelated history
    /// has no bases; callers must not confuse it with an unreadable object.
    /// Returning every base lets comparison callers reject ambiguous criss-cross
    /// histories instead of silently picking an arbitrary base.
    pub fn merge_bases(&self, a: &str, b: &str) -> Result<Vec<String>, G5Error> {
        let repo = self.repo()?;
        let a_oid = git2::Oid::from_str(a).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{a}': {e}"),
        })?;
        let b_oid = git2::Oid::from_str(b).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{b}': {e}"),
        })?;
        // Missing commits are errors, even when libgit2 also calls them NotFound.
        repo.find_commit(a_oid).map_err(|e| G5Error::GitError {
            message: format!("Cannot read comparison commit '{a}': {e}"),
        })?;
        repo.find_commit(b_oid).map_err(|e| G5Error::GitError {
            message: format!("Cannot read comparison commit '{b}': {e}"),
        })?;
        match repo.merge_bases(a_oid, b_oid) {
            Ok(bases) => {
                let mut bases = bases.iter().map(ToString::to_string).collect::<Vec<_>>();
                bases.sort();
                Ok(bases)
            }
            Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(Vec::new()),
            Err(error) => Err(G5Error::GitError {
                message: format!("merge bases of '{a}' and '{b}' failed: {error}"),
            }),
        }
    }

    /// Returns the merge-base (best common ancestor) of two commits as a hex
    /// SHA. Used by the workspace-fork **co-track** path to derive the fork's
    /// fresh `baseline_commit` = merge-base(fork tip, shared remote tip) —
    /// never inherited from the source (`designs/REMOTE_REPO_TRACKING.md` §3.3
    /// "Baseline is set fresh"; clone_service D27).
    pub fn merge_base(&self, a: &str, b: &str) -> Result<String, G5Error> {
        let repo = self.repo()?;
        let a_oid = git2::Oid::from_str(a).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{a}': {e}"),
        })?;
        let b_oid = git2::Oid::from_str(b).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{b}': {e}"),
        })?;
        let base = repo
            .merge_base(a_oid, b_oid)
            .map_err(|e| G5Error::GitError {
                message: format!("merge-base of '{a}' and '{b}' failed: {e}"),
            })?;
        Ok(base.to_string())
    }

    /// Returns the usable commit messages in
    /// `merge_base(target_sha, source_sha)..source_sha`, oldest-first, for
    /// PR title/body derivation.
    ///
    /// Walks the full unique-source graph in deterministic parent-before-child
    /// order (topological + commit-time tie-break, reversed), skipping merge
    /// commits (parent count > 1) and commits whose subject is empty,
    /// whitespace-only, or non-UTF-8. Second-parent non-merge commits remain
    /// candidates. The cap applies after filtering; `omitted_count` reports
    /// exactly how many usable commits it dropped. Never mutates refs or the
    /// worktree. Unresolvable SHAs and a missing merge base (unrelated
    /// histories) return `G5Error::GitError`.
    pub fn commit_messages_ahead(
        &self,
        target_sha: &str,
        source_sha: &str,
        cap: usize,
    ) -> Result<CommitRangeMessages, G5Error> {
        let repo = self.repo()?;
        let source_oid = git2::Oid::from_str(source_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{source_sha}': {e}"),
        })?;
        let target_oid = git2::Oid::from_str(target_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{target_sha}': {e}"),
        })?;
        let merge_base_oid =
            repo.merge_base(target_oid, source_oid)
                .map_err(|e| G5Error::GitError {
                    message: format!("merge-base of '{target_sha}' and '{source_sha}' failed: {e}"),
                })?;
        let mut revwalk = repo.revwalk().map_err(|e| G5Error::GitError {
            message: format!("Failed to create revwalk: {e}"),
        })?;
        revwalk.push(source_oid).map_err(|e| G5Error::GitError {
            message: format!("revwalk push failed: {e}"),
        })?;
        revwalk
            .hide(merge_base_oid)
            .map_err(|e| G5Error::GitError {
                message: format!("revwalk hide failed: {e}"),
            })?;
        revwalk
            .set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME | git2::Sort::REVERSE)
            .map_err(crate::backend_error)?;

        let mut commits = Vec::new();
        let mut omitted_count = 0usize;
        for oid in revwalk {
            let oid = oid.map_err(|e| G5Error::GitError {
                message: format!("revwalk iteration failed: {e}"),
            })?;
            let commit = repo.find_commit(oid).map_err(|e| G5Error::GitError {
                message: format!("Failed to load commit '{oid}': {e}"),
            })?;
            if commit.parent_count() > 1 {
                continue;
            }
            let message = commit.message().unwrap_or("");
            let (subject, body) = match message.split_once('\n') {
                Some((first, rest)) => (first.trim(), rest.trim()),
                None => (message.trim(), ""),
            };
            if subject.is_empty() {
                continue;
            }
            if commits.len() >= cap {
                omitted_count += 1;
                continue;
            }
            commits.push(RangeCommitMessage {
                sha: oid.to_string(),
                subject: subject.to_string(),
                body: body.to_string(),
            });
        }
        Ok(CommitRangeMessages {
            commits,
            omitted_count,
        })
    }

    /// Checks out a branch by name.
    ///
    /// If `refs/heads/{branch}` exists locally, checks it out directly.
    /// Otherwise, looks for `refs/remotes/origin/{branch}` and creates a
    /// local tracking branch from it before checking out.
    ///
    /// Sets HEAD and updates the working directory.
    pub fn checkout_branch(&self, branch: &str) -> Result<(), G5Error> {
        let refname = format!("refs/heads/{}", branch);
        let repo = self.repo()?;

        // Try local branch first.
        let reference = match repo.find_reference(&refname) {
            Ok(r) => r,
            Err(_) => {
                // Local branch doesn't exist — try creating from remote.
                let remote_refname = format!("refs/remotes/origin/{}", branch);
                let remote_ref =
                    repo.find_reference(&remote_refname)
                        .map_err(|e| G5Error::GitError {
                            message: format!(
                                "Branch '{}' not found locally or as origin/{}: {}",
                                branch,
                                branch,
                                e.message()
                            ),
                        })?;
                let target_commit = remote_ref.peel_to_commit().map_err(crate::backend_error)?;
                repo.branch(branch, &target_commit, false)
                    .map_err(|e| G5Error::GitError {
                        message: format!(
                            "Failed to create local branch '{}' from origin: {}",
                            branch,
                            e.message()
                        ),
                    })?;
                repo.find_reference(&refname)
                    .map_err(|e| G5Error::GitError {
                        message: format!(
                            "Branch '{}' not found after creation: {}",
                            branch,
                            e.message()
                        ),
                    })?
            }
        };

        let obj = reference
            .peel(git2::ObjectType::Commit)
            .map_err(crate::backend_error)?;
        repo.checkout_tree(&obj, None)
            .map_err(crate::backend_error)?;
        repo.set_head(&refname).map_err(crate::backend_error)?;
        Ok(())
    }

    /// Checks out a branch by name, forcibly discarding local checkout state.
    ///
    /// Fixture-only: disposable test checkouts where stale tracked or
    /// untracked files must not block switching refs.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn checkout_branch_force(&self, branch: &str) -> Result<(), G5Error> {
        let refname = format!("refs/heads/{}", branch);
        let repo = self.repo()?;

        let reference = match repo.find_reference(&refname) {
            Ok(r) => r,
            Err(_) => {
                let remote_refname = format!("refs/remotes/origin/{}", branch);
                let remote_ref =
                    repo.find_reference(&remote_refname)
                        .map_err(|e| G5Error::GitError {
                            message: format!(
                                "Branch '{}' not found locally or as origin/{}: {}",
                                branch,
                                branch,
                                e.message()
                            ),
                        })?;
                let target_commit = remote_ref.peel_to_commit().map_err(crate::backend_error)?;
                repo.branch(branch, &target_commit, false)
                    .map_err(|e| G5Error::GitError {
                        message: format!(
                            "Failed to create local branch '{}' from origin: {}",
                            branch,
                            e.message()
                        ),
                    })?;
                repo.find_reference(&refname)
                    .map_err(|e| G5Error::GitError {
                        message: format!(
                            "Branch '{}' not found after creation: {}",
                            branch,
                            e.message()
                        ),
                    })?
            }
        };

        let obj = reference
            .peel(git2::ObjectType::Commit)
            .map_err(crate::backend_error)?;
        let mut opts = git2::build::CheckoutBuilder::new();
        opts.force().remove_untracked(true).remove_ignored(true);
        repo.checkout_tree(&obj, Some(&mut opts))
            .map_err(crate::backend_error)?;
        repo.set_head(&refname).map_err(crate::backend_error)?;
        Ok(())
    }

    /// Checks out a specific commit by SHA (detached HEAD).
    ///
    /// Resolves the SHA to an `Oid`, peels to a commit, and sets HEAD +
    /// working directory.
    pub fn checkout_commit(&self, sha: &str) -> Result<(), G5Error> {
        let oid = git2::Oid::from_str(sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{}': {}", sha, e.message()),
        })?;

        let repo = self.repo()?;
        let commit = repo.find_commit(oid).map_err(crate::backend_error)?;
        let obj = commit.as_object();
        repo.checkout_tree(obj, None)
            .map_err(crate::backend_error)?;
        repo.set_head_detached(oid).map_err(crate::backend_error)?;
        Ok(())
    }

    /// Point local branch `branch` at the commit named by `rev` and check it
    /// out (`git checkout -B`). The working tree is updated safely first; the
    /// branch is then written once, even when it is the current branch. This
    /// is local checkout materialization, not mapped-branch authority.
    pub fn checkout_branch_at(&self, branch: &str, rev: &str) -> Result<(), G5Error> {
        let refname = format!("refs/heads/{branch}");
        if !git2::Reference::is_valid_name(&refname) {
            return Err(G5Error::GitError {
                message: format!("Invalid branch name '{branch}'"),
            });
        }
        let repo = self.repo()?;
        let commit = repo
            .revparse_single(rev)
            .and_then(|object| object.peel_to_commit())
            .map_err(|e| G5Error::GitError {
                message: format!("Cannot resolve '{rev}' to a commit: {}", e.message()),
            })?;
        repo.checkout_tree(commit.as_object(), None)
            .map_err(crate::backend_error)?;
        repo.set_head_detached(commit.id())
            .map_err(crate::backend_error)?;
        repo.branch(branch, &commit, true)
            .map_err(crate::backend_error)?;
        repo.set_head(&refname).map_err(crate::backend_error)?;
        Ok(())
    }

    /// Creates a new branch `name` pointing at the commit referenced by
    /// `from` (a branch name, tag, or SHA).
    ///
    /// Does not check out the new branch.
    pub fn create_branch(&self, name: &str, from: &str) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let obj = repo.revparse_single(from).map_err(crate::backend_error)?;
        let commit = obj.peel_to_commit().map_err(crate::backend_error)?;
        repo.branch(name, &commit, false)
            .map_err(crate::backend_error)?;
        Ok(())
    }

    /// Configures a local "alias" branch that tracks a differently-named
    /// upstream branch on `remote`.
    ///
    /// Used by `g5d clone --checkout-remote-names`: the local working-tree
    /// branch is named after the upstream `remote_branch` (e.g. `main`) while
    /// the proxy/server ref stays the canonical name. Sets
    /// `branch.<alias>.remote = <remote>`,
    /// `branch.<alias>.merge = refs/heads/<canonical>`, and
    /// `push.default = upstream`, so a plain `git pull` / `git push` map the
    /// alias <-> canonical on `remote`. This is the client-side mirror of the
    /// server's [`push_refspec`](Self::push_refspec) mapping.
    pub fn configure_branch_alias(
        &self,
        alias: &str,
        canonical: &str,
        remote: &str,
    ) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let mut config = repo.config().map_err(|e| G5Error::GitError {
            message: format!("Failed to open repo config: {}", e.message()),
        })?;
        config
            .set_str(&format!("branch.{alias}.remote"), remote)
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to set branch.{alias}.remote: {}", e.message()),
            })?;
        config
            .set_str(
                &format!("branch.{alias}.merge"),
                &format!("refs/heads/{canonical}"),
            )
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to set branch.{alias}.merge: {}", e.message()),
            })?;
        config
            .set_str("push.default", "upstream")
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to set push.default: {}", e.message()),
            })?;
        Ok(())
    }

    /// Returns the full hex SHA of the current HEAD commit.
    ///
    /// Resolves HEAD → commit → oid → hex string.
    pub fn current_head(&self) -> Result<String, G5Error> {
        let repo = self.repo()?;
        let head = repo.head().map_err(crate::backend_error)?;
        let commit = head.peel_to_commit().map_err(crate::backend_error)?;
        Ok(commit.id().to_string())
    }

    /// Returns whether the repository contains the specified commit SHA.
    ///
    /// Invalid SHAs are treated as absent instead of raising an error.
    pub fn has_commit(&self, sha: &str) -> Result<bool, G5Error> {
        let Ok(oid) = git2::Oid::from_str(sha) else {
            return Ok(false);
        };
        let repo = self.repo()?;
        let has_commit = repo.find_commit(oid).is_ok();
        Ok(has_commit)
    }

    /// Returns the default branch name by inspecting `HEAD`.
    ///
    /// After a fresh clone, `HEAD` is a symbolic ref pointing at the
    /// upstream default branch (e.g. `refs/heads/master`). This method
    /// reads that target and strips the `refs/heads/` prefix.
    ///
    /// Returns `None` if HEAD is detached or the symbolic target cannot
    /// be parsed.
    pub fn default_branch(&self) -> Option<String> {
        let repo = self.repo().ok()?;
        let head_ref = repo.find_reference("HEAD").ok()?;
        let target = head_ref.symbolic_target().ok().flatten()?;
        target.strip_prefix("refs/heads/").map(|s| s.to_string())
    }

    /// Resolves the SHA of a remote-tracking ref (e.g. `refs/remotes/origin/main`).
    ///
    /// Used after `fetch()` to determine the new upstream HEAD without
    /// checking out the branch.
    pub fn resolve_remote_ref(&self, remote: &str, branch: &str) -> Result<String, G5Error> {
        let refname = format!("refs/remotes/{}/{}", remote, branch);
        let repo = self.repo()?;
        let reference = repo
            .find_reference(&refname)
            .map_err(|e| G5Error::GitError {
                message: format!("Reference {} not found: {}", refname, e.message()),
            })?;
        let commit = reference.peel_to_commit().map_err(|e| G5Error::GitError {
            message: format!("Failed to peel {} to commit: {}", refname, e.message()),
        })?;
        Ok(commit.id().to_string())
    }

    /// Returns the configured URL for a named remote.
    pub fn remote_url(&self, remote: &str) -> Result<String, G5Error> {
        let repo = self.repo()?;
        let remote_name = remote.to_string();
        let remote = repo.find_remote(remote).map_err(|e| G5Error::GitError {
            message: format!("Remote '{}' not found: {}", remote_name, e.message()),
        })?;
        let url = remote.url().map_err(|e| G5Error::GitError {
            message: format!(
                "Remote '{}' URL is not valid UTF-8: {}",
                remote_name,
                e.message()
            ),
        })?;
        if url.is_empty() {
            return Err(G5Error::GitError {
                message: format!("Remote '{}' has no configured URL", remote_name),
            });
        }
        Ok(url.to_string())
    }

    /// Configure a checkout's named remote, creating it only when absent.
    pub fn configure_remote(&self, remote: &str, url: &str) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let exists = match repo.find_remote(remote) {
            Ok(_) => true,
            Err(error) if error.code() == git2::ErrorCode::NotFound => false,
            Err(error) => return Err(crate::backend_error(error)),
        };
        if exists {
            repo.remote_set_url(remote, url)
                .map_err(crate::backend_error)?;
        } else {
            repo.remote(remote, url).map_err(crate::backend_error)?;
        }
        Ok(())
    }

    /// Updates the configured URL for a named remote.
    pub fn set_remote_url(&self, remote: &str, url: &str) -> Result<(), G5Error> {
        let repo = self.repo()?;
        repo.remote_set_url(remote, url)
            .map_err(|e| G5Error::GitError {
                message: format!(
                    "Failed to update remote '{}' URL to '{}': {}",
                    remote,
                    url,
                    e.message()
                ),
            })
    }

    /// Point an existing remote at the caller's authoritative URL before a
    /// network operation, writing config only when it differs.
    pub fn ensure_remote_url(&self, remote: &str, url: &str) -> Result<(), G5Error> {
        if self.remote_url(remote)? != url {
            self.set_remote_url(remote, url)?;
        }
        Ok(())
    }

    /// Returns `(ahead, behind)` for two commits using git's graph analysis.
    pub fn graph_ahead_behind(
        &self,
        local_sha: &str,
        upstream_sha: &str,
    ) -> Result<(usize, usize), G5Error> {
        let repo = self.repo()?;
        let local_oid = git2::Oid::from_str(local_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid local SHA '{}': {}", local_sha, e),
        })?;
        let upstream_oid = git2::Oid::from_str(upstream_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid upstream SHA '{}': {}", upstream_sha, e),
        })?;
        repo.graph_ahead_behind(local_oid, upstream_oid)
            .map_err(|e| G5Error::GitError {
                message: format!(
                    "Failed to compute ahead/behind for '{}' vs '{}': {}",
                    local_sha, upstream_sha, e
                ),
            })
    }

    /// Stages all changes in the working directory (equivalent to `git add -A`).
    ///
    /// Uses `git2::Index::add_all` with `ADD_DEFAULT` flags. Skips `.g5/`
    /// runtime metadata while allowing durable `.g5` repository content such
    /// as experience files, workspace config, and preview profile YAML files.
    pub fn add_all(&self) -> Result<(), G5Error> {
        let repo = self.repo()?;
        let mut index = repo.index().map_err(crate::backend_error)?;
        index
            .add_all(
                ["*"].iter(),
                git2::IndexAddOption::DEFAULT,
                Some(&mut |path: &Path, _matched_spec: &[u8]| {
                    // Returning 0 = add the file, 1 = skip the file.
                    if is_g5_runtime_metadata_path(path) {
                        1 // skip internal runtime metadata
                    } else {
                        0
                    }
                }),
            )
            .map_err(crate::backend_error)?;
        index.write().map_err(crate::backend_error)?;
        Ok(())
    }

    /// Stages only the provided relative paths in the working directory.
    ///
    /// Paths must be relative to the repository root. Missing paths are treated
    /// as deletions and removed from the index if they were previously tracked.
    /// Internal `.g5/` metadata is ignored with the same durable-content
    /// exceptions as [`Self::add_all`].
    pub fn add_paths<I, S>(&self, paths: I) -> Result<(), G5Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let repo = self.repo()?;
        let workdir = repo.workdir().ok_or_else(|| G5Error::GitError {
            message: "Cannot stage paths on a bare repository".to_string(),
        })?;
        let mut index = repo.index().map_err(crate::backend_error)?;

        for rel in paths {
            let rel = rel.as_ref().trim();
            if rel.is_empty() || is_g5_runtime_metadata_path(Path::new(rel)) {
                continue;
            }

            let rel_path = Path::new(rel);
            let full_path = workdir.join(rel_path);
            if full_path.exists() {
                index.add_path(rel_path).map_err(|e| G5Error::GitError {
                    message: format!("Failed to stage path '{}': {}", rel, e),
                })?;
            } else {
                let _ = index.remove_path(rel_path);
            }
        }

        index.write().map_err(crate::backend_error)?;
        Ok(())
    }

    /// Creates a new commit on the current branch with the staged changes.
    ///
    /// Uses the provided [`GitAuthor`] for both author and committer fields.
    /// Returns the hex SHA of the new commit. The commit is parented to the
    /// current HEAD (or is an initial commit if HEAD is unborn).
    pub fn commit(&self, message: &str, author: &GitAuthor) -> Result<String, G5Error> {
        let sig =
            git2::Signature::now(&author.name, &author.email).map_err(crate::backend_error)?;

        let repo = self.repo()?;
        let mut index = repo.index().map_err(crate::backend_error)?;
        let tree_oid = index.write_tree().map_err(crate::backend_error)?;
        let tree = repo.find_tree(tree_oid).map_err(crate::backend_error)?;

        let parent_commit = if repo.head().is_ok() {
            let head = repo.head().map_err(crate::backend_error)?;
            Some(head.peel_to_commit().map_err(crate::backend_error)?)
        } else {
            // Unborn HEAD — initial commit
            None
        };

        let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();

        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(crate::backend_error)?;

        Ok(commit_oid.to_string())
    }

    /// Returns the deterministic set of file paths whose tree entries differ
    /// between two commits. This performs no patch or line-stat generation and
    /// includes both sides of a rename when their paths differ.
    pub fn diff_name_only_between_commits(
        &self,
        base_sha: &str,
        head_sha: &str,
    ) -> Result<Vec<String>, G5Error> {
        let base_oid = git2::Oid::from_str(base_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid base SHA '{}': {}", base_sha, e.message()),
        })?;
        let head_oid = git2::Oid::from_str(head_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid head SHA '{}': {}", head_sha, e.message()),
        })?;
        let repo = self.repo()?;
        let base_tree = repo
            .find_commit(base_oid)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let head_tree = repo
            .find_commit(head_oid)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let diff = repo
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
            .map_err(crate::backend_error)?;

        let mut paths = std::collections::BTreeSet::new();
        for delta in diff.deltas() {
            if let Some(path) = delta.old_file().path() {
                paths.insert(path.to_string_lossy().to_string());
            }
            if let Some(path) = delta.new_file().path() {
                paths.insert(path.to_string_lossy().to_string());
            }
        }
        Ok(paths.into_iter().collect())
    }

    /// Checks if `descendant_sha` is a descendant of `ancestor_sha`.
    ///
    /// Returns `true` if `ancestor_sha` is reachable from `descendant_sha`
    /// (i.e., a fast-forward from ancestor to descendant is possible).
    pub fn is_ancestor_of(
        &self,
        ancestor_sha: &str,
        descendant_sha: &str,
    ) -> Result<bool, G5Error> {
        let repo = self.repo()?;
        let ancestor_oid = git2::Oid::from_str(ancestor_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid ancestor SHA '{}': {}", ancestor_sha, e),
        })?;
        let descendant_oid =
            git2::Oid::from_str(descendant_sha).map_err(|e| G5Error::GitError {
                message: format!("Invalid descendant SHA '{}': {}", descendant_sha, e),
            })?;
        Ok(repo
            .graph_descendant_of(descendant_oid, ancestor_oid)
            .unwrap_or(false))
    }

    /// Whether `descendant_sha` equals `ancestor_sha` or extends it through
    /// single-parent commits only. Existing merges at/before the ancestor are
    /// allowed; a merge anywhere in the replayed range is not a linear rebase.
    pub fn is_linear_descendant_of(
        &self,
        ancestor_sha: &str,
        descendant_sha: &str,
    ) -> Result<bool, G5Error> {
        let repo = self.repo()?;
        let ancestor = git2::Oid::from_str(ancestor_sha).map_err(crate::backend_error)?;
        repo.find_commit(ancestor).map_err(crate::backend_error)?;
        let mut current = repo
            .find_commit(git2::Oid::from_str(descendant_sha).map_err(crate::backend_error)?)
            .map_err(crate::backend_error)?;
        while current.id() != ancestor {
            if current.parent_count() != 1 {
                return Ok(false);
            }
            current = current.parent(0).map_err(crate::backend_error)?;
        }
        Ok(true)
    }

    /// Lists commits reachable from `head_sha`, newest first.
    ///
    /// When `base_sha` is `Some` and differs from `head_sha`, the walk hides
    /// `base_sha` **together with its ancestors**, so the result is exactly the
    /// set reachable from `head_sha` but not from `base_sha` (`head` ^base). The
    /// outcome therefore depends on how the two relate:
    /// - `base_sha` is an ancestor of head -> the head's own commits since that
    ///   base;
    /// - `head_sha` is an ancestor of `base_sha` (the head is at or behind the
    ///   base) -> an **empty** list, because hiding the base also hides the
    ///   head;
    /// - `base_sha` is on an unrelated/disjoint history -> nothing is excluded,
    ///   so the full history from `head_sha`.
    ///
    /// The hide is skipped only when `base_sha` is `None` or equals `head_sha`,
    /// in which case the walk is the full unbounded history from `head_sha`.
    /// Commits are sorted by time then topology (`git2::Sort::TIME |
    /// TOPOLOGICAL`), newest first, and the result is capped at `limit`
    /// entries.
    pub fn list_commits(
        &self,
        head_sha: &str,
        base_sha: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CommitSummary>, G5Error> {
        let repo = self.repo()?;
        let head_oid = git2::Oid::from_str(head_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{}': {}", head_sha, e),
        })?;

        let mut revwalk = repo.revwalk().map_err(|e| G5Error::GitError {
            message: format!("Failed to create revwalk: {}", e),
        })?;
        // TIME orders by committer time (newest first); TOPOLOGICAL breaks ties
        // so a child never appears after its parent even when timestamps are
        // identical (e.g. several commits created within the same second).
        revwalk
            .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to set revwalk sorting: {}", e),
            })?;
        revwalk.push(head_oid).map_err(|e| G5Error::GitError {
            message: format!("Revwalk push failed: {}", e),
        })?;

        // Hide the base commit (and its ancestors) so the walk yields only the
        // commits reachable from head but not from base (`head` ^base). When
        // base and head are on unrelated histories this excludes nothing (full
        // head history); when head is at or behind base, hiding base also hides
        // head and the walk is empty. A malformed `base_sha` is ignored (fall
        // back to the unbounded-from-head walk).
        if let Some(base_sha) = base_sha {
            if base_sha != head_sha {
                if let Ok(base_oid) = git2::Oid::from_str(base_sha) {
                    let _ = revwalk.hide(base_oid);
                }
            }
        }

        let mut commits = Vec::new();
        for oid in revwalk {
            if commits.len() >= limit {
                break;
            }
            let oid = oid.map_err(|e| G5Error::GitError {
                message: format!("Revwalk iteration failed: {}", e),
            })?;
            let commit = repo.find_commit(oid).map_err(|e| G5Error::GitError {
                message: format!("Failed to load commit {}: {}", oid, e),
            })?;
            commits.push(CommitSummary {
                sha: oid.to_string(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
                author_name: commit.author().name().unwrap_or("").to_string(),
                committed_at: commit.time().seconds(),
            });
        }

        Ok(commits)
    }

    /// Merge commits reachable from `head_sha` but not from `base_sha`
    /// (`git rev-list --merges base..head`), newest first, capped at `limit`.
    pub fn merge_commits_between(
        &self,
        base_sha: &str,
        head_sha: &str,
        limit: usize,
    ) -> Result<Vec<CommitSummary>, G5Error> {
        let repo = self.repo()?;
        let mut revwalk = repo.revwalk().map_err(crate::backend_error)?;
        revwalk
            .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
            .map_err(crate::backend_error)?;
        revwalk
            .push(git2::Oid::from_str(head_sha).map_err(crate::backend_error)?)
            .map_err(crate::backend_error)?;
        revwalk
            .hide(git2::Oid::from_str(base_sha).map_err(crate::backend_error)?)
            .map_err(crate::backend_error)?;
        let mut merges = Vec::new();
        for oid in revwalk {
            if merges.len() >= limit {
                break;
            }
            let oid = oid.map_err(crate::backend_error)?;
            let commit = repo.find_commit(oid).map_err(crate::backend_error)?;
            if commit.parent_count() > 1 {
                merges.push(CommitSummary {
                    sha: oid.to_string(),
                    summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
                    author_name: commit.author().name().unwrap_or("").to_string(),
                    committed_at: commit.time().seconds(),
                });
            }
        }
        Ok(merges)
    }

    /// Return the newest commit on `head_sha`'s first-parent chain that
    /// changes any requested path.
    ///
    /// The caller supplies an exact authorized head and the complete path set;
    /// this primitive never resolves a ref or reads the worktree. Both sides of
    /// a rename are matched. An empty path set returns `None` without opening
    /// history.
    pub fn latest_commit_touching_paths(
        &self,
        head_sha: &str,
        paths: &[String],
    ) -> Result<Option<CommitSummary>, G5Error> {
        if paths.is_empty() {
            return Ok(None);
        }
        let requested: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
        self.latest_commit_touching_path_where_with_pathspec(head_sha, Some(paths), |path| {
            requested.contains(path)
        })
    }

    /// Return the newest first-parent commit whose old or new delta path
    /// satisfies `predicate`.
    #[cfg(test)]
    fn latest_commit_touching_path_where<F>(
        &self,
        head_sha: &str,
        predicate: F,
    ) -> Result<Option<CommitSummary>, G5Error>
    where
        F: Fn(&str) -> bool,
    {
        self.latest_commit_touching_path_where_with_pathspec(head_sha, None, predicate)
    }

    fn latest_commit_touching_path_where_with_pathspec<F>(
        &self,
        head_sha: &str,
        pathspecs: Option<&[String]>,
        predicate: F,
    ) -> Result<Option<CommitSummary>, G5Error>
    where
        F: Fn(&str) -> bool,
    {
        let repo = self.repo()?;
        let mut next_oid = Some(parse_full_commit_oid(head_sha, "branch head")?);

        while let Some(oid) = next_oid {
            let commit = repo.find_commit(oid).map_err(|_| G5Error::NotFound {
                resource: "branch head commit".to_string(),
            })?;
            let commit_tree = commit.tree().map_err(crate::backend_error)?;
            let parent_tree = if commit.parent_count() == 0 {
                None
            } else {
                Some(
                    commit
                        .parent(0)
                        .map_err(crate::backend_error)?
                        .tree()
                        .map_err(crate::backend_error)?,
                )
            };
            let mut diff_options = git2::DiffOptions::new();
            if let Some(pathspecs) = pathspecs {
                for path in pathspecs {
                    diff_options.pathspec(path);
                }
            }
            let mut diff = repo
                .diff_tree_to_tree(
                    parent_tree.as_ref(),
                    Some(&commit_tree),
                    pathspecs.map(|_| &mut diff_options),
                )
                .map_err(crate::backend_error)?;
            let mut find_options = git2::DiffFindOptions::new();
            find_options.renames(true);
            diff.find_similar(Some(&mut find_options))
                .map_err(crate::backend_error)?;

            let touches_requested_path = diff.deltas().any(|delta| {
                [delta.old_file().path(), delta.new_file().path()]
                    .into_iter()
                    .flatten()
                    .filter_map(Path::to_str)
                    .any(&predicate)
            });
            if touches_requested_path {
                return Ok(Some(CommitSummary {
                    sha: oid.to_string(),
                    summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
                    author_name: commit.author().name().unwrap_or("").to_string(),
                    committed_at: commit.time().seconds(),
                }));
            }

            next_oid = if commit.parent_count() == 0 {
                None
            } else {
                Some(commit.parent_id(0).map_err(|error| G5Error::GitError {
                    message: format!("Failed to resolve first parent for {oid}: {error}"),
                })?)
            };
        }

        Ok(None)
    }

    /// Returns one bounded page of the branch's full first-parent history.
    ///
    /// `cursor`, when present, identifies the first commit in the requested
    /// page. A cursor that is no longer reachable from `head_sha` is a
    /// recoverable stale-history conflict rather than an empty page.
    pub fn list_first_parent_commits(
        &self,
        head_sha: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<CommitHistoryPage, G5Error> {
        let repo = self.repo()?;
        let head_oid = parse_full_commit_oid(head_sha, "branch head")?;
        repo.find_commit(head_oid).map_err(|_| G5Error::NotFound {
            resource: "branch head commit".to_string(),
        })?;

        let start_oid = if let Some(cursor) = cursor {
            let cursor_oid = parse_full_commit_oid(cursor, "history cursor")?;
            repo.find_commit(cursor_oid)
                .map_err(|_| G5Error::StaleHistory)?;
            if !is_on_first_parent_chain(&repo, head_oid, cursor_oid)? {
                return Err(G5Error::StaleHistory);
            }
            cursor_oid
        } else {
            head_oid
        };

        let mut commits = Vec::with_capacity(limit);
        let mut next_oid = Some(start_oid);
        while commits.len() < limit {
            let Some(oid) = next_oid else { break };
            let commit = repo.find_commit(oid).map_err(|_| G5Error::NotFound {
                resource: "commit".to_string(),
            })?;
            commits.push(commit_history_entry(&commit));
            next_oid = if commit.parent_count() == 0 {
                None
            } else {
                Some(commit.parent_id(0).map_err(|e| G5Error::GitError {
                    message: format!("Failed to resolve first parent for {oid}: {e}"),
                })?)
            };
        }

        Ok(CommitHistoryPage {
            commits,
            next_cursor: next_oid.map(|oid| oid.to_string()),
        })
    }

    /// Summarizes the parent-zero (or root empty-tree) diff for one commit.
    pub fn commit_diff_summary(
        &self,
        head_sha: &str,
        commit_sha: &str,
    ) -> Result<CommitDiffSummary, G5Error> {
        let repo = self.repo()?;
        let commit = resolve_reachable_commit(&repo, head_sha, commit_sha)?;
        let commit_tree = commit.tree().map_err(crate::backend_error)?;
        let parent_tree = if commit.parent_count() == 0 {
            None
        } else {
            Some(
                commit
                    .parent(0)
                    .map_err(crate::backend_error)?
                    .tree()
                    .map_err(crate::backend_error)?,
            )
        };
        let mut diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)
            .map_err(crate::backend_error)?;
        let mut find_options = git2::DiffFindOptions::new();
        find_options.renames(true);
        diff.find_similar(Some(&mut find_options))
            .map_err(crate::backend_error)?;

        let stats = diff.stats().map_err(crate::backend_error)?;
        let mut files = Vec::with_capacity(diff.deltas().len());
        for (index, delta) in diff.deltas().enumerate() {
            let (additions, deletions) =
                match git2::Patch::from_diff(&diff, index).map_err(crate::backend_error)? {
                    Some(patch) => {
                        let (_, additions, deletions) =
                            patch.line_stats().map_err(crate::backend_error)?;
                        (additions, deletions)
                    }
                    None => (0, 0),
                };
            files.push(commit_diff_file_summary(&delta, additions, deletions));
        }

        Ok(CommitDiffSummary {
            commit: commit_history_entry(&commit),
            comparison_base: if commit.parent_count() == 0 {
                CommitComparisonBase::EmptyTree
            } else {
                CommitComparisonBase::Parent {
                    sha: commit
                        .parent_id(0)
                        .map_err(crate::backend_error)?
                        .to_string(),
                }
            },
            first_parent_diff: commit.parent_count() > 1,
            files,
            total_additions: stats.insertions(),
            total_deletions: stats.deletions(),
        })
    }

    /// Typed diffs of every file changed between two arbitrary commits
    /// (`base_sha` → `head_sha`), one tree diff for all of them, in the same
    /// shape as [`Self::commit_file_diff`]. Used by review publications,
    /// which compare a review branch with its merge-base against the source.
    /// Renames are not detected: a moved file is a deletion at its old path
    /// and an addition at its new one. Line content keeps a carriage return
    /// so a hunk reproduces the proposed bytes of a CRLF file.
    pub fn file_diffs_between_commits(
        &self,
        base_sha: &str,
        head_sha: &str,
    ) -> Result<Vec<CommitFileDiff>, G5Error> {
        let repo = self.repo()?;
        let base_oid = git2::Oid::from_str(base_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid base SHA '{}': {}", base_sha, e.message()),
        })?;
        let head_oid = git2::Oid::from_str(head_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid head SHA '{}': {}", head_sha, e.message()),
        })?;
        let base_tree = repo
            .find_commit(base_oid)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let head_tree = repo
            .find_commit(head_oid)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let diff = repo
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
            .map_err(crate::backend_error)?;

        let mut files = Vec::with_capacity(diff.deltas().len());
        for (index, delta) in diff.deltas().enumerate() {
            let path = commit_diff_file_summary(&delta, 0, 0)
                .canonical_path()
                .to_string();
            files.push(typed_file_diff(&repo, &diff, index, &delta, &path, true)?);
        }
        Ok(files)
    }

    /// Bounded repository changes since an immutable workspace base, including
    /// side-branch commits. Each commit keeps its real parents and identity.
    pub fn source_control_commits(
        &self,
        base: &str,
        tip: &str,
        limit: usize,
    ) -> Result<(Vec<SourceCommit>, bool), G5Error> {
        let repo = self.repo()?;
        let base = parse_full_commit_oid(base, "workspace base")?;
        let tip = parse_full_commit_oid(tip, "pushed tip")?;
        repo.find_commit(base).map_err(crate::backend_error)?;
        repo.find_commit(tip).map_err(crate::backend_error)?;
        let mut walk = repo.revwalk().map_err(crate::backend_error)?;
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
            .map_err(crate::backend_error)?;
        walk.push(tip).map_err(crate::backend_error)?;
        walk.hide(base).map_err(crate::backend_error)?;
        let mut commits = Vec::new();
        let mut truncated = false;
        for oid in walk.take(limit + 1) {
            if commits.len() == limit {
                truncated = true;
                break;
            }
            let commit = repo
                .find_commit(oid.map_err(crate::backend_error)?)
                .map_err(crate::backend_error)?;
            let tree = commit.tree().map_err(crate::backend_error)?;
            let parent_tree = if commit.parent_count() > 0 {
                Some(
                    commit
                        .parent(0)
                        .map_err(crate::backend_error)?
                        .tree()
                        .map_err(crate::backend_error)?,
                )
            } else {
                None
            };
            let diff = repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
                .map_err(crate::backend_error)?;
            if diff.deltas().len() > 1000 {
                return Err(G5Error::Conflict {
                    reason: "A commit exceeds the 1,000 changed-path history limit".into(),
                });
            }
            let paths = diff
                .deltas()
                .flat_map(|d| {
                    [
                        d.old_file().path().map(|p| p.to_string_lossy().to_string()),
                        d.new_file().path().map(|p| p.to_string_lossy().to_string()),
                    ]
                })
                .flatten()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            commits.push(SourceCommit {
                sha: commit.id().to_string(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_owned(),
                author_name: commit.author().name().unwrap_or("").to_owned(),
                committed_at: commit.time().seconds(),
                parents: commit.parent_ids().map(|p| p.to_string()).collect(),
                paths,
            });
        }
        Ok((commits, truncated))
    }

    /// Net tree comparison with rename identity retained for unique file counts.
    pub fn source_control_diff(
        &self,
        base: &str,
        tip: &str,
    ) -> Result<Vec<SourceFilePatch>, G5Error> {
        let repo = self.repo()?;
        let before = repo
            .find_commit(parse_full_commit_oid(base, "comparison base")?)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let after = repo
            .find_commit(parse_full_commit_oid(tip, "comparison tip")?)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let mut diff = repo
            .diff_tree_to_tree(Some(&before), Some(&after), None)
            .map_err(crate::backend_error)?;
        source_control_find_renames(&repo, &mut diff)?;
        source_control_files(&repo, &diff)
    }

    pub fn source_control_commit_diff(
        &self,
        tip: &str,
        sha: &str,
    ) -> Result<Vec<SourceFilePatch>, G5Error> {
        let repo = self.repo()?;
        let commit = resolve_reachable_commit(&repo, tip, sha)?;
        let after = commit.tree().map_err(crate::backend_error)?;
        let before = if commit.parent_count() > 0 {
            Some(
                commit
                    .parent(0)
                    .map_err(crate::backend_error)?
                    .tree()
                    .map_err(crate::backend_error)?,
            )
        } else {
            None
        };
        let mut diff = repo
            .diff_tree_to_tree(before.as_ref(), Some(&after), None)
            .map_err(crate::backend_error)?;
        source_control_find_renames(&repo, &mut diff)?;
        source_control_files(&repo, &diff)
    }

    /// Returns typed hunks for one canonical path in an immutable commit diff.
    pub fn commit_file_diff(
        &self,
        head_sha: &str,
        commit_sha: &str,
        canonical_path: &str,
    ) -> Result<CommitFileDiff, G5Error> {
        let repo = self.repo()?;
        let commit = resolve_reachable_commit(&repo, head_sha, commit_sha)?;
        let commit_tree = commit.tree().map_err(crate::backend_error)?;
        let parent_tree = if commit.parent_count() == 0 {
            None
        } else {
            Some(
                commit
                    .parent(0)
                    .map_err(crate::backend_error)?
                    .tree()
                    .map_err(crate::backend_error)?,
            )
        };
        let mut diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)
            .map_err(crate::backend_error)?;
        let mut find_options = git2::DiffFindOptions::new();
        find_options.renames(true);
        diff.find_similar(Some(&mut find_options))
            .map_err(crate::backend_error)?;

        for (index, delta) in diff.deltas().enumerate() {
            let summary = commit_diff_file_summary(&delta, 0, 0);
            if summary.canonical_path() != canonical_path {
                continue;
            }
            return typed_file_diff(&repo, &diff, index, &delta, canonical_path, false);
        }

        Err(G5Error::NotFound {
            resource: format!("commit diff file '{canonical_path}'"),
        })
    }

    /// Computes diff statistics between two commits identified by SHA.
    ///
    /// Returns per-file addition/deletion counts and totals. Uses
    /// `git2::Diff::tree_to_tree` and per-patch `line_stats()`.
    pub fn diff_summary(&self, base: &str, head: &str) -> Result<DiffSummary, G5Error> {
        self.diff_summary_options(Some(base), head, false)
    }

    /// Net source-control totals count a rename once. Ordinary path summaries
    /// retain both sides for consumers such as the ontology comparison graph.
    pub fn source_control_summary(&self, base: &str, head: &str) -> Result<DiffSummary, G5Error> {
        self.diff_summary_options(Some(base), head, true)
    }

    /// [`Self::source_control_summary`] from the empty tree: every file of
    /// `head` counts as added.
    pub fn source_control_summary_from_empty(&self, head: &str) -> Result<DiffSummary, G5Error> {
        self.diff_summary_options(None, head, true)
    }

    fn diff_summary_options(
        &self,
        base: Option<&str>,
        head: &str,
        renames: bool,
    ) -> Result<DiffSummary, G5Error> {
        let base_oid = base
            .map(|base| {
                git2::Oid::from_str(base).map_err(|e| G5Error::GitError {
                    message: format!("Invalid base SHA '{}': {}", base, e.message()),
                })
            })
            .transpose()?;
        let head_oid = git2::Oid::from_str(head).map_err(|e| G5Error::GitError {
            message: format!("Invalid head SHA '{}': {}", head, e.message()),
        })?;

        let repo = self.repo()?;
        let base_tree = base_oid
            .map(|oid| {
                repo.find_commit(oid)
                    .and_then(|commit| commit.tree())
                    .map_err(crate::backend_error)
            })
            .transpose()?;
        let head_commit = repo.find_commit(head_oid).map_err(crate::backend_error)?;
        let head_tree = head_commit.tree().map_err(crate::backend_error)?;

        let mut diff = repo
            .diff_tree_to_tree(base_tree.as_ref(), Some(&head_tree), None)
            .map_err(crate::backend_error)?;

        if renames {
            source_control_find_renames(&repo, &mut diff)?;
        }

        // Source Control keeps oversized file identities without materializing
        // their contents. Whole-diff stats would bypass that per-file guard,
        // including when attributes force an oversized blob to be text.
        let mut oversized = std::collections::HashSet::new();
        if renames {
            let odb = repo.odb().map_err(crate::backend_error)?;
            for (index, delta) in diff.deltas().enumerate() {
                if source_control_blob_too_large(&odb, &delta)? {
                    oversized.insert(index);
                }
            }
        }
        let (mut total_additions, mut total_deletions) = if oversized.is_empty() {
            let stats = diff.stats().map_err(crate::backend_error)?;
            (stats.insertions(), stats.deletions())
        } else {
            (0, 0)
        };

        // Build per-file stats using patch-by-patch iteration
        let num_deltas = diff.deltas().len();
        let mut files = Vec::with_capacity(num_deltas);

        for i in 0..num_deltas {
            let delta = diff.deltas().nth(i).unwrap();
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let status = match delta.status() {
                git2::Delta::Added => DiffFileStatus::Added,
                git2::Delta::Deleted => DiffFileStatus::Deleted,
                git2::Delta::Renamed => DiffFileStatus::Renamed,
                _ => DiffFileStatus::Modified,
            };

            // Get per-file line counts from the patch
            let (additions, deletions) = if oversized.contains(&i) {
                (0, 0)
            } else {
                match git2::Patch::from_diff(&diff, i).map_err(crate::backend_error)? {
                    Some(patch) => {
                        let (_, adds, dels) = patch.line_stats().map_err(crate::backend_error)?;
                        (adds, dels)
                    }
                    None => (0, 0),
                }
            };
            if !oversized.is_empty() {
                total_additions += additions;
                total_deletions += deletions;
            }

            files.push(DiffFileSummary {
                path,
                additions,
                deletions,
                status,
            });
        }

        Ok(DiffSummary {
            files,
            total_additions,
            total_deletions,
        })
    }

    /// Returns a unified-format patch string showing the textual diff between
    /// two commits identified by SHA.
    ///
    /// Optionally filters to a single path via `DiffOptions::pathspec()`.
    /// Uses `git2::Diff::tree_to_tree` (same as `diff_summary`) followed by
    /// `diff.print(DiffFormat::Patch, ...)` to produce unified output.
    /// Returns an empty string if there are no differences.
    pub fn diff_unified_between_commits(
        &self,
        base_sha: &str,
        head_sha: &str,
        path_filter: Option<&str>,
    ) -> Result<String, G5Error> {
        let base_oid = git2::Oid::from_str(base_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid base SHA '{}': {}", base_sha, e.message()),
        })?;
        let head_oid = git2::Oid::from_str(head_sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid head SHA '{}': {}", head_sha, e.message()),
        })?;

        let repo = self.repo()?;
        let base_commit = repo.find_commit(base_oid).map_err(crate::backend_error)?;
        let head_commit = repo.find_commit(head_oid).map_err(crate::backend_error)?;

        let base_tree = base_commit.tree().map_err(crate::backend_error)?;
        let head_tree = head_commit.tree().map_err(crate::backend_error)?;

        let mut diff_opts = git2::DiffOptions::new();
        if let Some(p) = path_filter {
            diff_opts.pathspec(p);
        }

        let diff = repo
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to compute tree-to-tree diff: {}", e),
            })?;

        let mut output = String::new();
        const MAX_DIFF_BYTES: usize = 100 * 1024; // 100KB guardrail
        let mut truncated = false;
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if truncated {
                return true; // stop accumulating but don't abort the callback
            }
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                'F' => "", // File header
                'H' => "", // Hunk header
                _ => "",
            };
            if line.origin() == 'F' || line.origin() == 'H' {
                if let Ok(content) = std::str::from_utf8(line.content()) {
                    output.push_str(content);
                }
            } else if let Ok(content) = std::str::from_utf8(line.content()) {
                output.push_str(prefix);
                output.push_str(content);
            }
            if output.len() > MAX_DIFF_BYTES {
                truncated = true;
            }
            true
        })
        .map_err(|e| G5Error::GitError {
            message: format!("Failed to format diff: {}", e),
        })?;

        if truncated {
            // Truncate to the last complete line to avoid mid-line cuts.
            if let Some(last_newline) = output[..MAX_DIFF_BYTES.min(output.len())].rfind('\n') {
                output.truncate(last_newline + 1);
            }
            output.push_str("\n[DIFF TRUNCATED — full diff exceeds 100KB. Use read_spec to read the full file.]\n");
        }

        Ok(output)
    }

    /// Returns true if `sha` is a well-formed object id that resolves to a
    /// commit present in this repository.
    ///
    /// Lets callers distinguish "the base commit itself is unreadable" (every
    /// `show_file` against it would fail) from "this one path is absent at an
    /// otherwise-valid commit" (a genuinely new file).
    pub fn commit_exists(&self, sha: &str) -> bool {
        let Ok(oid) = git2::Oid::from_str(sha) else {
            return false;
        };
        let Ok(repo) = self.repo() else {
            return false;
        };
        // Bind the result before returning so the borrowed `Commit` temporary
        // is dropped before `repo` (the repo handle) goes out of scope.
        let exists = repo.find_commit(oid).is_ok();
        exists
    }

    /// Returns whether `sha` resolves to a parentless commit.
    ///
    /// First materialization uses this to prove that an unattached bootstrap
    /// candidate does not smuggle pre-existing history into an unborn
    /// authority member.
    pub fn commit_is_parentless(&self, sha: &str) -> Result<bool, G5Error> {
        let oid = git2::Oid::from_str(sha).map_err(|error| G5Error::GitError {
            message: format!("Invalid commit SHA '{sha}': {error}"),
        })?;
        let repo = self.repo()?;
        let commit = repo.find_commit(oid).map_err(crate::backend_error)?;
        Ok(commit.parent_count() == 0)
    }

    /// The committer time of `sha` as seconds since the Unix epoch.
    pub fn commit_committed_at(&self, sha: &str) -> Result<i64, G5Error> {
        let oid = git2::Oid::from_str(sha).map_err(|error| G5Error::GitError {
            message: format!("Invalid commit SHA '{sha}': {error}"),
        })?;
        let repo = self.repo()?;
        let commit = repo.find_commit(oid).map_err(crate::backend_error)?;
        Ok(commit.time().seconds())
    }

    /// Returns the tree SHA of commit `sha`.
    ///
    /// A landing compares trees, not commit SHAs, to recognize a result an
    /// earlier attempt already put on a branch ref: a retry that recomputes
    /// the same content produces the same tree under a new commit SHA.
    pub fn commit_tree_id(&self, sha: &str) -> Result<String, G5Error> {
        let oid = git2::Oid::from_str(sha).map_err(|error| G5Error::GitError {
            message: format!("Invalid commit SHA '{sha}': {error}"),
        })?;
        let repo = self.repo()?;
        let commit = repo.find_commit(oid).map_err(crate::backend_error)?;
        Ok(commit.tree_id().to_string())
    }

    /// The Git file mode of `path` at `commit` (`0o100644`, `0o100755`,
    /// `0o120000` for a symlink, ...), or `NotFound`.
    /// The size in bytes of the blob at `path` in `commit`, from the object
    /// header only (the content is not loaded), or `NotFound`.
    pub fn blob_size(&self, commit: &str, path: &str) -> Result<u64, G5Error> {
        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;
        let repo = self.repo()?;
        let tree = repo
            .find_commit(oid)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| G5Error::NotFound {
                resource: format!("File '{}' at commit '{}'", path, commit),
            })?;
        // A tree (or a submodule commit) at the path is not a file: the
        // callers' "does the base have this file" question is answered no,
        // as `show_file` answers it.
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return Err(G5Error::NotFound {
                resource: format!("File '{}' at commit '{}'", path, commit),
            });
        }
        let (size, _kind) = repo
            .odb()
            .map_err(crate::backend_error)?
            .read_header(entry.id())
            .map_err(crate::backend_error)?;
        Ok(size as u64)
    }

    pub fn file_mode(&self, commit: &str, path: &str) -> Result<i32, G5Error> {
        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;
        let repo = self.repo()?;
        let tree = repo
            .find_commit(oid)
            .map_err(crate::backend_error)?
            .tree()
            .map_err(crate::backend_error)?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| G5Error::NotFound {
                resource: format!("File '{}' at commit '{}'", path, commit),
            })?;
        Ok(entry.filemode())
    }

    /// Reads the content of a file at a specific commit.
    ///
    /// Resolves commit → tree → blob at path. Returns raw bytes (not
    /// necessarily UTF-8). Returns `G5Error::NotFound` if the path does
    /// not exist at that commit.
    pub fn show_file(&self, commit: &str, path: &str) -> Result<Vec<u8>, G5Error> {
        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;

        let repo = self.repo()?;
        let commit_obj = repo.find_commit(oid).map_err(crate::backend_error)?;
        let tree = commit_obj.tree().map_err(crate::backend_error)?;

        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| G5Error::NotFound {
                resource: format!("File '{}' at commit '{}'", path, commit),
            })?;

        let obj = entry.to_object(&repo).map_err(crate::backend_error)?;
        let blob = obj.as_blob().ok_or_else(|| G5Error::NotFound {
            resource: format!("File '{}' at commit '{}' (not a blob)", path, commit),
        })?;

        Ok(blob.content().to_vec())
    }

    /// Returns the git blob object id for a file at a specific commit.
    ///
    /// Resolves commit → tree → blob at path without loading blob content.
    /// Returns `G5Error::NotFound` if the path does not exist at that commit
    /// or is not a blob.
    pub fn blob_id(&self, commit: &str, path: &str) -> Result<String, G5Error> {
        self.blob_metadata(commit, path).map(|(id, _size)| id)
    }

    /// Returns the git blob object id and byte size for a file at a specific
    /// commit without materializing the blob content.
    pub fn blob_metadata(&self, commit: &str, path: &str) -> Result<(String, usize), G5Error> {
        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;

        let repo = self.repo()?;
        let commit_obj = repo.find_commit(oid).map_err(crate::backend_error)?;
        let tree = commit_obj.tree().map_err(crate::backend_error)?;

        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| G5Error::NotFound {
                resource: format!("File '{}' at commit '{}'", path, commit),
            })?;

        if !matches!(entry.kind(), Some(git2::ObjectType::Blob)) {
            return Err(G5Error::NotFound {
                resource: format!("File '{}' at commit '{}' (not a blob)", path, commit),
            });
        }

        // The object header alone carries the size: a preflight that loaded
        // the blob to measure it would materialize what it exists to refuse.
        let (size, _kind) = repo
            .odb()
            .map_err(crate::backend_error)?
            .read_header(entry.id())
            .map_err(crate::backend_error)?;
        Ok((entry.id().to_string(), size))
    }

    /// Computes the Git blob object id for in-memory bytes without inserting
    /// an object into the repository.
    pub fn blob_id_for_bytes(content: &[u8]) -> Result<String, G5Error> {
        git2::Oid::hash_object(git2::ObjectType::Blob, content)
            .map(|oid| oid.to_string())
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to hash Git blob content: {}", e.message()),
            })
    }

    /// Blames the 1-based inclusive line range `[start_line, end_line]` of
    /// `path` as of `commit`, returning a single [`BlameAttribution`] **only**
    /// when the entire range resolves to exactly one commit.
    ///
    /// Returns `Ok(None)` when the path or range is absent at the commit, when
    /// the range is empty/invalid, or when the range spans more than one commit
    /// (ambiguous — e.g. a block that was edited or moved across commits). The
    /// caller treats `None` as "unknown" and never guesses, per the Intent Log
    /// git-attribution contract (`intent_log.pm_view.md` [D18]).
    ///
    /// This is a read-only operation against committed history; it does not
    /// touch the working tree.
    pub fn blame_for_path(
        &self,
        commit: &str,
        path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<Option<BlameAttribution>, G5Error> {
        if start_line == 0 || end_line < start_line {
            return Ok(None);
        }

        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;

        let repo = self.repo()?;
        // Ensure the commit resolves before blaming so we never silently blame
        // against an unrelated HEAD.
        repo.find_commit(oid).map_err(crate::backend_error)?;

        let mut opts = git2::BlameOptions::new();
        opts.newest_commit(oid)
            .min_line(start_line)
            .max_line(end_line);

        // A missing path (or any blame failure) is "unknown", not an error.
        let blame = match repo.blame_file(Path::new(path), Some(&mut opts)) {
            Ok(blame) => blame,
            Err(_) => return Ok(None),
        };

        let mut resolved: Option<(git2::Oid, BlameAttribution)> = None;
        for line in start_line..=end_line {
            let Some(hunk) = blame.get_line(line) else {
                continue;
            };
            let commit_id = hunk.final_commit_id();
            match &resolved {
                // Range touches a second commit → ambiguous, give up.
                Some((seen, _)) if *seen != commit_id => return Ok(None),
                Some(_) => {}
                None => {
                    let Some(sig) = hunk.final_signature() else {
                        return Ok(None);
                    };
                    let author = sig.name().unwrap_or("").to_string();
                    let committed_at = git_time_to_iso(&sig.when());
                    resolved = Some((
                        commit_id,
                        BlameAttribution {
                            author,
                            committed_at,
                        },
                    ));
                }
            }
        }

        Ok(resolved.map(|(_, attr)| attr))
    }

    /// Returns porcelain-style status lines for the working tree.
    ///
    /// Compares HEAD tree against the working directory (including untracked
    /// files) using `git2::Repository::statuses`. Each entry is a tuple of
    /// `(status_code, path)` where `status_code` matches `git status
    /// --porcelain` abbreviations (`M`, `A`, `D`, `??`, etc.).
    pub fn workdir_status(&self) -> Result<Vec<(String, String)>, G5Error> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let repo = self.repo()?;
        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| G5Error::GitError {
                message: format!("Failed to get working tree status: {}", e),
            })?;

        let mut result = Vec::with_capacity(statuses.len());
        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let code = match entry.status() {
                s if s.contains(git2::Status::WT_NEW) => "??",
                s if s.contains(git2::Status::INDEX_NEW) => "A ",
                s if s.contains(git2::Status::WT_MODIFIED) => " M",
                s if s.contains(git2::Status::INDEX_MODIFIED) => "M ",
                s if s.contains(git2::Status::WT_DELETED) => " D",
                s if s.contains(git2::Status::INDEX_DELETED) => "D ",
                s if s.contains(git2::Status::WT_RENAMED) => " R",
                s if s.contains(git2::Status::INDEX_RENAMED) => "R ",
                _ => "??",
            };
            result.push((code.to_string(), path));
        }
        Ok(result)
    }

    /// Lists directory contents at a specific commit and path.
    ///
    /// Returns entries with name, full path (relative to repo root), type
    /// (file/directory), and size (for files). If `path` is empty or `"/"`,
    /// lists the root tree.
    pub fn list_tree(&self, commit: &str, path: &str) -> Result<Vec<TreeEntry>, G5Error> {
        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;

        let repo = self.repo()?;
        let commit_obj = repo.find_commit(oid).map_err(crate::backend_error)?;
        let root_tree = commit_obj.tree().map_err(crate::backend_error)?;

        let tree = if path.is_empty() || path == "/" {
            root_tree
        } else {
            let entry = root_tree
                .get_path(Path::new(path))
                .map_err(|_| G5Error::NotFound {
                    resource: format!("Path '{}' at commit '{}'", path, commit),
                })?;
            let obj = entry.to_object(&repo).map_err(crate::backend_error)?;
            let tree_obj = obj.as_tree().ok_or_else(|| G5Error::NotFound {
                resource: format!("Path '{}' at commit '{}' (not a directory)", path, commit),
            })?;
            tree_obj.clone()
        };

        let mut entries = Vec::new();
        let prefix = if path.is_empty() || path == "/" {
            String::new()
        } else {
            format!("{}/", path.trim_end_matches('/'))
        };

        let odb = repo.odb().map_err(crate::backend_error)?;
        for entry in tree.iter() {
            let name = entry.name().unwrap_or("").to_string();
            let full_path = format!("{}{}", prefix, name);

            let (entry_type, size) = match entry.kind() {
                Some(git2::ObjectType::Blob) => (TreeEntryType::File, blob_size_in(&odb, &entry)),
                Some(git2::ObjectType::Tree) => (TreeEntryType::Directory, None),
                _ => continue, // Skip submodules, etc.
            };

            entries.push(TreeEntry {
                name,
                path: full_path,
                entry_type,
                size,
            });
        }

        Ok(entries)
    }

    /// Lists directory contents at a specific commit, recursing to `depth`.
    ///
    /// Like `list_tree`, but walks subdirectories up to `depth` levels.
    /// A `depth` of 1 lists only the immediate children (same as `list_tree`).
    /// Returns a flat list of all entries found — callers handle nesting.
    ///
    /// The walk is iterative and stops once it crosses either bound of
    /// `limits`, reporting `truncated` with the entries listed so far: a
    /// commit's tree is any width and depth, and one listing must not grow
    /// with it. File sizes come from object headers; no blob is loaded.
    pub fn list_tree_recursive(
        &self,
        commit: &str,
        path: &str,
        depth: u32,
        limits: TreeListLimits,
    ) -> Result<TreeListing, G5Error> {
        let repo = self.repo()?;
        let (start_tree, prefix) = Self::tree_at(&repo, commit, path)?;
        Ok(Self::walk_tree(
            &repo,
            &start_tree,
            &prefix,
            depth,
            limits,
            Sizes::FromHeaders,
        ))
    }

    /// Every file and directory path under `path` at `commit`, without
    /// loading a single blob: the tree objects alone carry the names. For
    /// callers that need membership, not sizes, on repositories with large
    /// committed assets. The walk stops past `max_paths` paths and reports
    /// `truncated`, so a caller learns the set is incomplete at a bounded
    /// cost instead of receiving every path of an arbitrarily wide tree.
    pub fn list_tree_paths(
        &self,
        commit: &str,
        path: &str,
        max_paths: usize,
    ) -> Result<TreePaths, G5Error> {
        let repo = self.repo()?;
        let (start_tree, prefix) = Self::tree_at(&repo, commit, path)?;
        let limits = TreeListLimits {
            max_entries: max_paths,
            max_bytes: usize::MAX,
        };
        let listing = Self::walk_tree(&repo, &start_tree, &prefix, u32::MAX, limits, Sizes::None);
        Ok(TreePaths {
            paths: listing
                .entries
                .into_iter()
                .map(|entry| entry.path)
                .collect(),
            truncated: listing.truncated,
        })
    }

    /// The tree at `path` in `commit` (the root for `""` or `"/"`) and the
    /// prefix its entries' paths carry.
    fn tree_at<'r>(
        repo: &'r git2::Repository,
        commit: &str,
        path: &str,
    ) -> Result<(git2::Tree<'r>, String), G5Error> {
        let oid = git2::Oid::from_str(commit).map_err(|e| G5Error::GitError {
            message: format!("Invalid commit SHA '{}': {}", commit, e.message()),
        })?;
        let commit_obj = repo.find_commit(oid).map_err(crate::backend_error)?;
        let root_tree = commit_obj.tree().map_err(crate::backend_error)?;

        if path.is_empty() || path == "/" {
            return Ok((root_tree, String::new()));
        }
        let entry = root_tree
            .get_path(Path::new(path))
            .map_err(|_| G5Error::NotFound {
                resource: format!("Path '{}' at commit '{}'", path, commit),
            })?;
        let obj = entry.to_object(repo).map_err(crate::backend_error)?;
        let tree = obj
            .as_tree()
            .ok_or_else(|| G5Error::NotFound {
                resource: format!("Path '{}' at commit '{}' (not a directory)", path, commit),
            })?
            .clone();
        Ok((tree, format!("{}/", path.trim_end_matches('/'))))
    }

    /// Walk `tree` breadth-first to `depth` levels (0 lists nothing), one
    /// directory at a time from an explicit queue, so a deep chain of
    /// directories costs entries, never stack. Blobs and trees are listed;
    /// submodules are skipped. The walk stops once more than
    /// `limits.max_entries` entries are listed or their budgeted bytes
    /// exceed `limits.max_bytes`, and reports `truncated`. A name that is
    /// not UTF-8 cannot be listed, and a directory so named is not entered.
    fn walk_tree<'r>(
        repo: &'r git2::Repository,
        tree: &git2::Tree<'r>,
        prefix: &str,
        depth: u32,
        limits: TreeListLimits,
        sizes: Sizes,
    ) -> TreeListing {
        let mut listing = TreeListing {
            entries: Vec::new(),
            truncated: false,
            bytes: 0,
        };
        if depth == 0 {
            return listing;
        }
        let odb = match sizes {
            Sizes::FromHeaders => repo.odb().ok(),
            Sizes::None => None,
        };
        let mut queue = VecDeque::from([(tree.clone(), prefix.to_string(), 1_u32)]);
        while let Some((tree, prefix, level)) = queue.pop_front() {
            for entry in tree.iter() {
                let Ok(name) = entry.name() else {
                    continue;
                };
                let (entry_type, size) = match entry.kind() {
                    Some(git2::ObjectType::Blob) => (
                        TreeEntryType::File,
                        odb.as_ref().and_then(|odb| blob_size_in(odb, &entry)),
                    ),
                    Some(git2::ObjectType::Tree) => (TreeEntryType::Directory, None),
                    _ => continue,
                };
                let full_path = format!("{prefix}{name}");
                let bytes = listing.bytes.saturating_add(entry_bytes(&full_path));
                if listing.entries.len() >= limits.max_entries || bytes > limits.max_bytes {
                    listing.truncated = true;
                    return listing;
                }
                listing.bytes = bytes;
                if matches!(entry_type, TreeEntryType::Directory) && level < depth {
                    if let Some(subtree) = entry
                        .to_object(repo)
                        .ok()
                        .and_then(|obj| obj.as_tree().cloned())
                    {
                        queue.push_back((subtree, format!("{full_path}/"), level + 1));
                    }
                }
                listing.entries.push(TreeEntry {
                    name: name.to_string(),
                    path: full_path,
                    entry_type,
                    size,
                });
            }
        }
        listing
    }
}

/// Whether a tree walk reads blob sizes from object headers or lists names
/// only.
#[derive(Clone, Copy)]
enum Sizes {
    FromHeaders,
    None,
}

/// The size of the blob `entry` names, from its object header (no content
/// is loaded); `None` when the object cannot be read.
fn blob_size_in(odb: &git2::Odb<'_>, entry: &git2::TreeEntry<'_>) -> Option<u64> {
    odb.read_header(entry.id())
        .ok()
        .map(|(size, _kind)| size as u64)
}

/// The serialized size one tree entry is budgeted at: its path as the JSON
/// string it becomes plus the envelope around it, with headroom. The same
/// estimate the workspace runtime's `fs.tree` applies, so a committed and a
/// live listing of one tree reach one byte bound at the same entry.
pub fn entry_bytes(path: &str) -> usize {
    let quoted = serde_json::to_string(path).map_or(path.len().saturating_mul(6), |s| s.len());
    quoted + 64
}

/// Reconciles two UNRELATED histories by replaying the local content on top of
/// the remote tip (leaf spec `git` D18).
///
/// `source_oid` is the LOCAL head (theirs), `target_oid` is the REMOTE tip
/// (ours). Performs one in-memory three-way tree merge with the empty tree as
/// the common base, then writes a single commit whose sole first parent is the
/// remote tip — so the returned tip DESCENDS FROM the remote tip and the remote
/// is never rebased onto local nor force-overwritten. Same-path/different-content
/// overlaps surface as a real conflict (`G5Error::GitError` naming the paths)
/// for the human merge workflow, never an auto-resolved clobber. Uses only the
/// leaf's existing git2 tree-merge primitives; never shells out to git.
fn reconcile_unrelated_onto(
    repo: &git2::Repository,
    source_oid: git2::Oid,
    target_oid: git2::Oid,
    source_label: &str,
    target_label: &str,
) -> Result<String, G5Error> {
    // ours = target/REMOTE tip, theirs = source/LOCAL head.
    let target_commit = repo
        .find_commit(target_oid)
        .map_err(|e| G5Error::GitError {
            message: format!("Target commit {} not found: {}", target_oid, e),
        })?;
    let source_commit = repo
        .find_commit(source_oid)
        .map_err(|e| G5Error::GitError {
            message: format!("Source commit {} not found: {}", source_oid, e),
        })?;

    // base = canonical empty tree (4b825dc6...), so files present on only one
    // side are additions and the same path on both sides with differing content
    // is an add/add conflict.
    let empty_tree_oid = repo
        .treebuilder(None)
        .and_then(|tb| tb.write())
        .map_err(|e| G5Error::GitError {
            message: format!("Failed to materialize empty tree base: {}", e),
        })?;
    let empty_tree = repo
        .find_tree(empty_tree_oid)
        .map_err(|e| G5Error::GitError {
            message: format!("Empty tree {} not found: {}", empty_tree_oid, e),
        })?;

    let mut merge_index = repo
        .merge_trees(
            &empty_tree,
            &target_commit.tree().map_err(crate::backend_error)?,
            &source_commit.tree().map_err(crate::backend_error)?,
            None,
        )
        .map_err(|e| G5Error::GitError {
            message: format!("Unrelated-history merge_trees failed: {}", e),
        })?;

    if merge_index.has_conflicts() {
        let conflicts = collect_index_conflicts(&merge_index)?;
        let paths = conflict_paths(&conflicts)?;
        return Err(G5Error::MergeConflict {
            message: format!(
                "Rebase of '{}' onto '{}' has unrelated-history conflicts at: {}",
                source_label,
                target_label,
                paths.join(", ")
            ),
        });
    }

    let tree_oid = merge_index
        .write_tree_to(repo)
        .map_err(|e| G5Error::GitError {
            message: format!("Failed to write unrelated-history merge tree: {}", e),
        })?;
    let tree = repo.find_tree(tree_oid).map_err(crate::backend_error)?;

    // Preserve local authorship; commit with the REMOTE tip as the sole (first)
    // parent so the reconciled tip descends from the remote.
    let author = source_commit.author();
    let committer =
        git2::Signature::now("G5 System", "g5@system.local").map_err(crate::backend_error)?;
    let message = source_commit.message().unwrap_or("");

    let new_oid = repo
        .commit(None, &author, &committer, message, &tree, &[&target_commit])
        .map_err(|e| G5Error::GitError {
            message: format!(
                "Failed to create unrelated-history reconciled commit: {}",
                e
            ),
        })?;

    Ok(new_oid.to_string())
}

fn collect_index_conflicts(index: &git2::Index) -> Result<Vec<git2::IndexConflict>, G5Error> {
    index
        .conflicts()
        .map_err(|e| G5Error::GitError {
            message: format!("Failed to iterate merge conflicts: {}", e),
        })?
        .map(|conflict| {
            conflict.map_err(|e| G5Error::GitError {
                message: format!("Failed to read merge conflict entry: {}", e),
            })
        })
        .collect()
}

fn conflict_paths(conflicts: &[git2::IndexConflict]) -> Result<Vec<String>, G5Error> {
    conflicts
        .iter()
        .map(|conflict| {
            let entry = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref())
                .ok_or_else(|| G5Error::Internal {
                    message: "merge conflict missing ancestor/our/their entries".to_string(),
                })?;

            std::str::from_utf8(&entry.path)
                .map(|s| s.to_string())
                .map_err(|e| G5Error::Internal {
                    message: format!("merge conflict path is not valid UTF-8: {}", e),
                })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// DiffSummary and related types
// ---------------------------------------------------------------------------

/// Summary of changes between two commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub files: Vec<DiffFileSummary>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

/// Per-file diff statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFileSummary {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    pub status: DiffFileStatus,
}

/// File status in a diff.
///
/// Serializes to lowercase strings matching API conventions:
/// - `Added` → `"added"`
/// - `Deleted` → `"deleted"`
/// - `Modified` → `"modified"`
/// - `Renamed` → `"renamed"`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DiffFileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
}

// ---------------------------------------------------------------------------
// TreeEntry and related types
// ---------------------------------------------------------------------------

/// Entry in a git tree listing.
///
/// Matches the `FileTreeEntry` TypeScript interface from `API_AND_STORAGE.md`
/// §3.14, enabling direct serialization for the File Tree API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub entry_type: TreeEntryType,
    pub size: Option<u64>,
}

/// How large one recursive tree listing may grow. Both bounds exist because
/// one reply carries the whole listing: the entry cap bounds what a panel
/// shows, the byte cap bounds the serialized reply even when paths are long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeListLimits {
    /// The walk stops once more than this many entries are listed.
    pub max_entries: usize,
    /// The walk stops once the entries' budgeted serialized size
    /// ([`entry_bytes`]) exceeds this many bytes.
    pub max_bytes: usize,
}

/// A recursive tree listing: the entries walked, and whether the walk
/// stopped at a [`TreeListLimits`] bound before the subtree ended.
#[derive(Debug, Clone)]
pub struct TreeListing {
    pub entries: Vec<TreeEntry>,
    /// The subtree has more than was listed; `entries` holds what was walked
    /// before the bound was crossed.
    pub truncated: bool,
    /// The budgeted serialized size of `entries` ([`entry_bytes`] summed),
    /// so a caller spending one budget across several listings can charge
    /// each one.
    pub bytes: usize,
}

/// The paths under one tree, and whether the walk stopped at its bound.
#[derive(Debug, Clone)]
pub struct TreePaths {
    pub paths: Vec<String>,
    pub truncated: bool,
}

/// Type of a tree entry (file or directory).
///
/// Serializes to lowercase strings:
/// - `File` → `"file"`
/// - `Directory` → `"directory"`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TreeEntryType {
    File,
    Directory,
}

/// Summary of a single commit reachable from a branch head.
///
/// Produced by [`GitRepo::list_commits`] for the workspace branch-commits
/// endpoint. `committed_at` is the committer time in Unix seconds.
#[derive(Debug, Clone)]
pub struct CommitSummary {
    /// Full 40-character hex commit SHA.
    pub sha: String,
    /// First line of the commit message (empty when unavailable).
    pub summary: String,
    /// Commit author name (empty when unavailable).
    pub author_name: String,
    /// Committer time in Unix seconds.
    pub committed_at: i64,
}

/// One retained commit message from a bounded `merge_base..source` walk.
///
/// Produced by [`GitRepo::commit_messages_ahead`]. Distinct from
/// [`CommitSummary`], which is the branch-history page item.
#[derive(Debug, Clone)]
pub struct RangeCommitMessage {
    /// Full 40-character hex commit SHA.
    pub sha: String,
    /// Trimmed first line of the commit message; never empty (empty-subject
    /// commits are filtered out of the walk).
    pub subject: String,
    /// Trimmed remainder of the commit message after the first line; may be
    /// empty.
    pub body: String,
}

/// Result of [`GitRepo::commit_messages_ahead`]: retained messages
/// oldest-first plus the exact count of usable commits the cap omitted.
#[derive(Debug, Clone)]
pub struct CommitRangeMessages {
    pub commits: Vec<RangeCommitMessage>,
    pub omitted_count: usize,
}

pub const MAX_COMMIT_FILE_DIFF_BYTES: usize = 100 * 1024;
pub const MAX_COMMIT_FILE_DIFF_LINES: usize = 3_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitHistoryPage {
    pub commits: Vec<CommitHistoryEntry>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitHistoryEntry {
    pub sha: String,
    pub summary: String,
    pub author_name: String,
    pub committed_at: i64,
    pub parent_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDiffSummary {
    pub commit: CommitHistoryEntry,
    pub comparison_base: CommitComparisonBase,
    pub first_parent_diff: bool,
    pub files: Vec<CommitDiffFileSummary>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommitComparisonBase {
    Parent { sha: String },
    EmptyTree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDiffFileSummary {
    pub status: DiffFileStatus,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub additions: usize,
    pub deletions: usize,
}

impl CommitDiffFileSummary {
    #[must_use]
    pub fn canonical_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileDiff {
    pub path: String,
    pub status: CommitFileDiffStatus,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<CommitDiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CommitFileDiffStatus {
    Text,
    Binary,
    TooLarge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDiffHunk {
    pub start_line: u32,
    pub header: String,
    pub lines: Vec<CommitDiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDiffLine {
    #[serde(rename = "type")]
    pub line_type: CommitDiffLineType,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitDiffLineType {
    Context,
    Addition,
    Deletion,
}

fn parse_full_commit_oid(value: &str, label: &str) -> Result<git2::Oid, G5Error> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(G5Error::ValidationError {
            details: serde_json::json!({
                "message": format!("{label} must be a full 40-character SHA")
            }),
        });
    }
    git2::Oid::from_str(value).map_err(|_| G5Error::ValidationError {
        details: serde_json::json!({
            "message": format!("{label} must be a full 40-character SHA")
        }),
    })
}

fn resolve_reachable_commit<'repo>(
    repo: &'repo git2::Repository,
    head_sha: &str,
    commit_sha: &str,
) -> Result<git2::Commit<'repo>, G5Error> {
    let head_oid = parse_full_commit_oid(head_sha, "branch head")?;
    let commit_oid = parse_full_commit_oid(commit_sha, "commit")?;
    repo.find_commit(head_oid).map_err(|_| G5Error::NotFound {
        resource: "branch head commit".to_string(),
    })?;
    let commit = repo
        .find_commit(commit_oid)
        .map_err(|_| G5Error::NotFound {
            resource: "commit".to_string(),
        })?;
    if commit_oid != head_oid
        && !repo
            .graph_descendant_of(head_oid, commit_oid)
            .map_err(crate::backend_error)?
    {
        return Err(G5Error::NotFound {
            resource: "commit".to_string(),
        });
    }
    Ok(commit)
}

fn commit_history_entry(commit: &git2::Commit<'_>) -> CommitHistoryEntry {
    CommitHistoryEntry {
        sha: commit.id().to_string(),
        summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
        author_name: commit.author().name().unwrap_or("").to_string(),
        committed_at: commit.time().seconds(),
        parent_count: commit.parent_count(),
    }
}

/// One delta of `diff` as a typed file diff: binary, too large, or its
/// hunks. `keep_carriage_return` keeps a trailing `\r` on each line (review
/// hunks must reproduce CRLF bytes); the commit viewer strips it.
fn typed_file_diff(
    repo: &git2::Repository,
    diff: &git2::Diff<'_>,
    index: usize,
    delta: &git2::DiffDelta<'_>,
    canonical_path: &str,
    keep_carriage_return: bool,
) -> Result<CommitFileDiff, G5Error> {
    if delta.old_file().is_binary()
        || delta.new_file().is_binary()
        || blob_is_binary(repo, delta.old_file().id())
        || blob_is_binary(repo, delta.new_file().id())
    {
        return Ok(CommitFileDiff {
            path: canonical_path.to_string(),
            status: CommitFileDiffStatus::Binary,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
        });
    }
    let Some(patch) = git2::Patch::from_diff(diff, index).map_err(crate::backend_error)? else {
        return Ok(CommitFileDiff {
            path: canonical_path.to_string(),
            status: CommitFileDiffStatus::Binary,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
        });
    };
    patch_file_diff(&patch, canonical_path, keep_carriage_return)
}

/// The typed hunks of one file's patch, or `TooLarge` past the commit diff
/// limits (`MAX_COMMIT_FILE_DIFF_BYTES`, `MAX_COMMIT_FILE_DIFF_LINES`).
/// `keep_carriage_return` keeps a trailing `\r` on each line (review hunks
/// must reproduce CRLF bytes); the commit viewer strips it.
fn patch_file_diff(
    patch: &git2::Patch<'_>,
    canonical_path: &str,
    keep_carriage_return: bool,
) -> Result<CommitFileDiff, G5Error> {
    let (context, additions, deletions) = patch.line_stats().map_err(crate::backend_error)?;
    let rendered_lines = context + additions + deletions;
    if patch.size(true, true, true) > MAX_COMMIT_FILE_DIFF_BYTES
        || rendered_lines > MAX_COMMIT_FILE_DIFF_LINES
    {
        return Ok(CommitFileDiff {
            path: canonical_path.to_string(),
            status: CommitFileDiffStatus::TooLarge,
            additions,
            deletions,
            hunks: Vec::new(),
        });
    }
    let mut hunks = Vec::with_capacity(patch.num_hunks());
    for hunk_index in 0..patch.num_hunks() {
        let (hunk, line_count) = patch.hunk(hunk_index).map_err(crate::backend_error)?;
        let mut lines = Vec::with_capacity(line_count);
        for line_index in 0..line_count {
            let line = patch
                .line_in_hunk(hunk_index, line_index)
                .map_err(crate::backend_error)?;
            let line_type = match line.origin() {
                ' ' => CommitDiffLineType::Context,
                '+' => CommitDiffLineType::Addition,
                '-' => CommitDiffLineType::Deletion,
                _ => continue,
            };
            let raw = String::from_utf8_lossy(line.content());
            let content = if keep_carriage_return {
                raw.strip_suffix('\n').unwrap_or(&raw).to_string()
            } else {
                raw.trim_end_matches(['\r', '\n']).to_string()
            };
            lines.push(CommitDiffLine {
                line_type,
                old_line: line.old_lineno(),
                new_line: line.new_lineno(),
                content,
            });
        }
        hunks.push(CommitDiffHunk {
            start_line: hunk.new_start(),
            header: String::from_utf8_lossy(hunk.header()).trim().to_string(),
            lines,
        });
    }
    Ok(CommitFileDiff {
        path: canonical_path.to_string(),
        status: CommitFileDiffStatus::Text,
        additions,
        deletions,
        hunks,
    })
}

fn commit_diff_file_summary(
    delta: &git2::DiffDelta<'_>,
    additions: usize,
    deletions: usize,
) -> CommitDiffFileSummary {
    let status = match delta.status() {
        git2::Delta::Added => DiffFileStatus::Added,
        git2::Delta::Deleted => DiffFileStatus::Deleted,
        git2::Delta::Renamed => DiffFileStatus::Renamed,
        _ => DiffFileStatus::Modified,
    };
    CommitDiffFileSummary {
        status,
        old_path: delta
            .old_file()
            .path()
            .map(|path| path.to_string_lossy().to_string()),
        new_path: delta
            .new_file()
            .path()
            .map(|path| path.to_string_lossy().to_string()),
        additions,
        deletions,
    }
}

fn blob_is_binary(repo: &git2::Repository, oid: git2::Oid) -> bool {
    !oid.is_zero()
        && repo
            .find_blob(oid)
            .map(|blob| blob.is_binary())
            .unwrap_or(false)
}

fn is_on_first_parent_chain(
    repo: &git2::Repository,
    head: git2::Oid,
    candidate: git2::Oid,
) -> Result<bool, G5Error> {
    let mut current = repo.find_commit(head).map_err(|_| G5Error::NotFound {
        resource: "branch head commit".to_string(),
    })?;
    loop {
        if current.id() == candidate {
            return Ok(true);
        }
        if current.parent_count() == 0 {
            return Ok(false);
        }
        current = current.parent(0).map_err(|e| G5Error::GitError {
            message: format!("Failed to walk first-parent history: {e}"),
        })?;
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(any(test, feature = "test-helpers"))]
impl GitRepo {
    /// Create a temporary git repo for testing.
    #[must_use]
    pub fn init_temp() -> (Self, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let repo = GitRepo {
            inner: Mutex::new(repo),
        };
        (repo, dir)
    }
}

/// Compute a unified diff between two in-memory file buffers.
///
/// Used to diff uncommitted draft (live workspace file) content against the committed base
/// content of a file without materializing anything to the git working tree.
/// The returned text is standard unified-diff format with `@@` hunk headers,
/// suitable for the same parser the task-branch diff uses.
pub fn unified_diff_buffers(old: &[u8], new: &[u8], path: &str) -> Result<String, G5Error> {
    unified_diff_buffers_with_context(old, new, path, DiffContext::Default)
}

/// Context required by a buffer-diff consumer, independent of the Git backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffContext {
    Default,
    Full,
}

/// Compute a unified buffer diff with ordinary or full-file context. This only
/// processes supplied bytes; it does not read repository or workspace content.
pub fn unified_diff_buffers_with_context(
    old: &[u8],
    new: &[u8],
    path: &str,
    context: DiffContext,
) -> Result<String, G5Error> {
    let as_path = Path::new(path);
    let mut options = git2::DiffOptions::new();
    if context == DiffContext::Full {
        // An upper bound on either file's line count also covers a final line
        // without a newline, without allocating or assuming UTF-8 input.
        let lines = |bytes: &[u8]| {
            bytes
                .iter()
                .filter(|&&byte| byte == b'\n')
                .count()
                .saturating_add(1)
        };
        options.context_lines(u32::try_from(lines(old).max(lines(new))).unwrap_or(u32::MAX));
    }
    let mut patch =
        git2::Patch::from_buffers(old, Some(as_path), new, Some(as_path), Some(&mut options))
            .map_err(|e| G5Error::GitError {
                message: format!("unified diff failed for '{}': {}", path, e.message()),
            })?;
    let buf = patch.to_buf().map_err(|e| G5Error::GitError {
        message: format!(
            "unified diff serialization failed for '{}': {}",
            path,
            e.message()
        ),
    })?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Typed hunks of two in-memory versions of one text file, cut the way a
/// review round cuts a changed file between two commits
/// ([`GitRepo::file_diffs_between_commits`]): libgit2's default three lines
/// of context, the commit diff limits (`TooLarge` past them) and carriage
/// returns kept on each line. Both buffers are diffed as text whatever
/// libgit2's binary heuristic says; the caller decides what counts as text.
pub fn text_file_diff(path: &str, old: &[u8], new: &[u8]) -> Result<CommitFileDiff, G5Error> {
    let as_path = Path::new(path);
    let mut options = git2::DiffOptions::new();
    options.force_text(true);
    let patch =
        git2::Patch::from_buffers(old, Some(as_path), new, Some(as_path), Some(&mut options))
            .map_err(|e| G5Error::GitError {
                message: format!("text diff failed for '{}': {}", path, e.message()),
            })?;
    patch_file_diff(&patch, path, true)
}

impl GitRepo {
    /// Create `refs/heads/{name}` at `sha` when no such ref exists, and
    /// report `Ok(false)` when one does (nothing is moved). The creation is
    /// atomic in git, so it doubles as the reservation of the name.
    pub fn create_branch_ref_if_absent(&self, name: &str, sha: &str) -> Result<bool, G5Error> {
        let oid = git2::Oid::from_str(sha).map_err(|e| G5Error::GitError {
            message: format!("Invalid SHA '{sha}': {}", e.message()),
        })?;
        let repo = self.repo()?;
        let commit = repo.find_commit(oid).map_err(|e| G5Error::GitError {
            message: format!("Commit {sha} is unavailable: {}", e.message()),
        })?;
        let created = match repo.branch(name, &commit, false) {
            Ok(_) => Ok(true),
            Err(error) if error.code() == git2::ErrorCode::Exists => Ok(false),
            Err(error) => Err(G5Error::GitError {
                message: format!("Failed to create branch {name}: {}", error.message()),
            }),
        };
        created
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_configuration_and_checkout_discovery_preserve_unrelated_state() {
        let (repo, dir) = GitRepo::init_temp();
        repo.configure_remote("origin", "https://example.invalid/first")
            .unwrap();
        {
            let raw = repo.repo().unwrap();
            raw.config()
                .unwrap()
                .set_str("remote.origin.pushurl", "https://example.invalid/push")
                .unwrap();
            raw.config()
                .unwrap()
                .set_str("user.email", "owner@example.com")
                .unwrap();
        }
        repo.configure_remote("origin", "https://example.invalid/second")
            .unwrap();
        let raw = repo.repo().unwrap();
        assert_eq!(
            raw.find_remote("origin").unwrap().url().unwrap(),
            "https://example.invalid/second"
        );
        assert_eq!(
            raw.config()
                .unwrap()
                .get_string("remote.origin.pushurl")
                .unwrap(),
            "https://example.invalid/push"
        );
        assert_eq!(
            raw.config().unwrap().get_string("user.email").unwrap(),
            "owner@example.com"
        );
        assert_eq!(raw.references().unwrap().count(), 0);
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        assert_eq!(
            GitRepo::discover_workdir(&nested)
                .unwrap()
                .unwrap()
                .canonicalize()
                .unwrap(),
            dir.path().canonicalize().unwrap()
        );
        let bare = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(bare.path()).unwrap();
        assert!(GitRepo::discover_workdir(bare.path()).unwrap().is_none());
    }

    #[test]
    fn branch_commit_state_distinguishes_unborn_missing_and_invalid_targets() {
        let (repo, _dir) = GitRepo::init_temp();
        let (commit, blob) = {
            let raw = repo.repo().unwrap();
            raw.set_head("refs/heads/custom-default").unwrap();
            raw.config()
                .unwrap()
                .set_str("init.defaultBranch", "other-default")
                .unwrap();
            let tree_id = raw.treebuilder(None).unwrap().write().unwrap();
            let signature = git2::Signature::now("Test", "test@example.com").unwrap();
            let commit = raw
                .commit(
                    None,
                    &signature,
                    &signature,
                    "unattached",
                    &raw.find_tree(tree_id).unwrap(),
                    &[],
                )
                .unwrap();
            (commit, raw.blob(b"not a commit").unwrap())
        };
        assert_eq!(
            repo.branch_commit_state("custom-default").unwrap(),
            BranchCommitState::UnbornRepository
        );
        repo.repo()
            .unwrap()
            .reference("refs/tags/object", blob, false, "fixture")
            .unwrap();
        assert_eq!(
            repo.branch_commit_state("custom-default").unwrap(),
            BranchCommitState::Unavailable
        );
        repo.repo()
            .unwrap()
            .reference("refs/heads/selected", commit, false, "fixture")
            .unwrap();
        assert_eq!(
            repo.branch_commit_state("selected").unwrap(),
            BranchCommitState::Present
        );
        assert_eq!(
            repo.branch_commit_state("missing").unwrap(),
            BranchCommitState::Unavailable
        );
        {
            let raw = repo.repo().unwrap();
            raw.reference_symbolic("refs/heads/dangling", "refs/heads/absent", false, "fixture")
                .unwrap();
            raw.reference("refs/heads/blob", blob, false, "fixture")
                .unwrap();
        }
        assert!(repo.branch_commit_state("dangling").is_err());
        assert!(repo.branch_commit_state("blob").is_err());
        assert_eq!(
            repo.resolve_branch_head("selected").unwrap(),
            commit.to_string()
        );
    }

    #[test]
    fn local_repository_preparation_configures_head_without_publishing_or_checkout() {
        let root = tempfile::tempdir().unwrap();
        let absent = root.path().join("invalid");
        assert!(matches!(
            GitRepo::open_or_init_with_default_branch(&absent, "bad..branch"),
            Err(G5Error::ValidationError { .. })
        ));
        assert!(!absent.exists());
        let bare = root.path().join("bare.git");
        let repo = GitRepo::open_or_init_with_default_branch(&bare, "trunk").unwrap();
        assert!(repo.workdir().is_none());
        assert_eq!(
            repo.branch_commit_state("trunk").unwrap(),
            BranchCommitState::UnbornRepository
        );
        assert_eq!(
            std::fs::read_to_string(bare.join("HEAD")).unwrap(),
            "ref: refs/heads/trunk\n"
        );
        drop(GitRepo::open_or_init_with_default_branch(&bare, "trunk").unwrap());
        assert_eq!(repo.repo().unwrap().references().unwrap().count(), 0);

        let (working, workdir) = GitRepo::init_temp();
        std::fs::write(workdir.path().join("file"), "committed\n").unwrap();
        let oid = commit_all(&working, "existing", &GitAuthor::test_author());
        let branch = working.default_branch().unwrap();
        std::fs::write(workdir.path().join("file"), "staged\n").unwrap();
        working.add_all().unwrap();
        let index_before = std::fs::read(workdir.path().join(".git/index")).unwrap();
        std::fs::write(workdir.path().join("file"), "dirty\n").unwrap();
        let reopened =
            GitRepo::open_or_init_with_default_branch(workdir.path(), "new-default").unwrap();
        assert_eq!(reopened.resolve_branch_head(&branch).unwrap(), oid);
        assert_eq!(reopened.read_branch_oid("new-default").unwrap(), None);
        assert_eq!(
            std::fs::read(workdir.path().join(".git/index")).unwrap(),
            index_before
        );
        assert_eq!(
            std::fs::read_to_string(workdir.path().join("file")).unwrap(),
            "dirty\n"
        );
    }

    #[test]
    fn maintenance_ref_readers_enumerate_only_heads_and_peel_exact_refs() {
        let (repo, dir) = GitRepo::init_temp();
        assert!(repo.head_ref_commits().unwrap().is_empty());
        std::fs::write(dir.path().join("file"), "content").unwrap();
        let oid = commit_all(&repo, "fixture", &GitAuthor::test_author());
        let oid = git2::Oid::from_str(&oid).unwrap();
        {
            let raw = repo.repo().unwrap();
            raw.reference("refs/heads/z/nested", oid, false, "fixture")
                .unwrap();
            raw.reference("refs/heads/a", oid, false, "fixture")
                .unwrap();
            raw.reference("refs/remotes/origin/main", oid, false, "fixture")
                .unwrap();
            let commit = raw.find_commit(oid).unwrap();
            raw.tag(
                "annotated",
                commit.as_object(),
                &git2::Signature::now("Test", "test@example.com").unwrap(),
                "tag",
                false,
            )
            .unwrap();
            raw.reference_symbolic("refs/heads/alias", "refs/heads/a", false, "fixture")
                .unwrap();
        }
        let heads = repo.head_ref_commits().unwrap();
        assert!(heads.len() >= 4);
        assert!(heads
            .iter()
            .all(|(name, id)| name.starts_with("refs/heads/") && id == &oid.to_string()));
        assert!(heads.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(heads.iter().any(|(name, _)| name == "refs/heads/z/nested"));
        assert_eq!(
            repo.ref_commit_oid("refs/tags/annotated").unwrap(),
            Some(oid.to_string())
        );
        assert_eq!(
            repo.ref_commit_oid("refs/heads/alias").unwrap(),
            Some(oid.to_string())
        );
        assert_eq!(repo.ref_commit_oid("refs/heads/missing").unwrap(), None);
    }

    #[test]
    fn maintenance_ref_readers_do_not_turn_bad_targets_into_deletion_evidence() {
        for symbolic in [false, true] {
            let (repo, _dir) = GitRepo::init_temp();
            {
                let raw = repo.repo().unwrap();
                if symbolic {
                    raw.reference_symbolic(
                        "refs/heads/broken",
                        "refs/heads/absent",
                        false,
                        "fixture",
                    )
                    .unwrap();
                } else {
                    let blob = raw.blob(b"not a commit").unwrap();
                    raw.reference("refs/heads/broken", blob, false, "fixture")
                        .unwrap();
                }
            }
            assert!(repo.head_ref_commits().is_err());
            assert!(repo.ref_commit_oid("refs/heads/broken").is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(GitRepo::open(&dir.path().join("missing-mirror")).is_err());
        assert!(!dir.path().join("missing-mirror").exists());
    }

    #[test]
    fn full_buffer_diff_preserves_context_without_changing_default_context() {
        let old = "opening\n1\n2\n3\n4\nold\n5\n6\n7\n8\nclosing";
        let new = old.replace("\nold\n", "\nnew\n");
        let ordinary = unified_diff_buffers(old.as_bytes(), new.as_bytes(), "file").unwrap();
        let full = unified_diff_buffers_with_context(
            old.as_bytes(),
            new.as_bytes(),
            "file",
            DiffContext::Full,
        )
        .unwrap();
        assert!(!ordinary.lines().any(|line| line == " opening"));
        assert!(!ordinary.lines().any(|line| line == " closing"));
        assert!(full.contains(" opening\n"));
        assert!(full.contains(" closing\n"));
        assert!(full.contains("-old\n+new\n"));
        assert!(full.contains("No newline at end of file"));
        assert!(!unified_diff_buffers_with_context(
            old.as_bytes(),
            old.as_bytes(),
            "file",
            DiffContext::Full
        )
        .unwrap()
        .contains("@@"));
    }

    #[test]
    fn linear_descent_rejects_merges_in_replayed_range_only() {
        let (git, dir) = GitRepo::init_temp();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let tree = repo
            .find_tree(repo.treebuilder(None).unwrap().write().unwrap())
            .unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        let commit = |label: &str, parents: &[git2::Oid]| {
            let parents: Vec<_> = parents
                .iter()
                .map(|oid| repo.find_commit(*oid).unwrap())
                .collect();
            repo.commit(
                None,
                &signature,
                &signature,
                label,
                &tree,
                &parents.iter().collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let root = commit("root", &[]);
        let provider = commit("provider", &[root]);
        let old_child = commit("old child", &[root]);
        let merged = commit("merge", &[old_child, provider]);
        let after_merge = commit("after merge", &[merged]);
        let replayed = commit("replayed", &[provider]);
        let replayed_twice = commit("replayed again", &[replayed]);
        let unrelated = commit("unrelated", &[]);
        for accepted in [provider, replayed, replayed_twice] {
            assert!(git
                .is_linear_descendant_of(&provider.to_string(), &accepted.to_string())
                .unwrap());
        }
        for refused in [old_child, merged, after_merge, unrelated] {
            assert!(!git
                .is_linear_descendant_of(&provider.to_string(), &refused.to_string())
                .unwrap());
        }
        // A provider is allowed to have its own historical merge commits.
        assert!(git
            .is_linear_descendant_of(&merged.to_string(), &after_merge.to_string())
            .unwrap());
        assert!(git
            .is_linear_descendant_of(&merged.to_string(), &merged.to_string())
            .unwrap());
        assert!(git
            .is_linear_descendant_of(&git2::Oid::ZERO_SHA1.to_string(), &provider.to_string())
            .is_err());
    }

    #[test]
    fn merge_commits_between_lists_only_merges_in_the_range() {
        let (git, dir) = GitRepo::init_temp();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let tree = repo
            .find_tree(repo.treebuilder(None).unwrap().write().unwrap())
            .unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        let commit = |label: &str, parents: &[git2::Oid]| {
            let parents: Vec<_> = parents
                .iter()
                .map(|oid| repo.find_commit(*oid).unwrap())
                .collect();
            repo.commit(
                None,
                &signature,
                &signature,
                label,
                &tree,
                &parents.iter().collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let root = commit("root", &[]);
        let side = commit("side", &[root]);
        let old_merge = commit("Merge side into main", &[root, side]);
        let target = commit("target", &[old_merge]);
        let work = commit("work", &[old_merge]);
        let merged = commit("Merge target into work", &[work, target]);
        let after = commit("after", &[merged]);

        let found = git
            .merge_commits_between(&target.to_string(), &after.to_string(), 10)
            .unwrap();
        assert_eq!(
            found.iter().map(|c| c.sha.clone()).collect::<Vec<_>>(),
            vec![merged.to_string()],
            "the target's own history merge is outside the range"
        );
        assert_eq!(found[0].summary, "Merge target into work");
        assert!(git
            .merge_commits_between(&target.to_string(), &work.to_string(), 10)
            .unwrap()
            .is_empty());
        assert_eq!(
            git.merge_commits_between(&root.to_string(), &after.to_string(), 1)
                .unwrap()
                .len(),
            1,
            "capped at the limit"
        );
    }

    #[test]
    fn comparison_merge_bases_follow_selected_history() {
        let (git, dir) = GitRepo::init_temp();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let tree_oid = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        let commit = |label: &str, parents: &[git2::Oid]| {
            let parents = parents
                .iter()
                .map(|oid| repo.find_commit(*oid).unwrap())
                .collect::<Vec<_>>();
            let parents = parents.iter().collect::<Vec<_>>();
            repo.commit(None, &signature, &signature, label, &tree, &parents)
                .unwrap()
        };
        let root = commit("root", &[]);
        let parent = commit("parent", &[root]);
        let child = commit("child", &[root]);
        assert_eq!(
            git.merge_bases(&parent.to_string(), &child.to_string())
                .unwrap(),
            vec![root.to_string()]
        );

        // A later merge changes the comparison base; the original fork is irrelevant.
        let merged_child = commit("merge parent", &[child, parent]);
        assert_eq!(
            git.merge_bases(&parent.to_string(), &merged_child.to_string())
                .unwrap(),
            vec![parent.to_string()]
        );
        // Rebasing onto a newer parent likewise selects that parent's commit.
        let advanced_parent = commit("parent advances", &[parent]);
        let rebased_child = commit("replayed child", &[advanced_parent]);
        assert_eq!(
            git.merge_bases(&advanced_parent.to_string(), &rebased_child.to_string())
                .unwrap(),
            vec![advanced_parent.to_string()]
        );

        // Independent merges in opposite directions have two best ancestors.
        let opposite_merge = commit("merge child", &[parent, child]);
        let mut expected = vec![parent.to_string(), child.to_string()];
        expected.sort();
        assert_eq!(
            git.merge_bases(&merged_child.to_string(), &opposite_merge.to_string())
                .unwrap(),
            expected
        );

        let unrelated = commit("unrelated root", &[]);
        assert!(git
            .merge_bases(&child.to_string(), &unrelated.to_string())
            .unwrap()
            .is_empty());
        assert!(git
            .merge_bases(
                &child.to_string(),
                "1111111111111111111111111111111111111111"
            )
            .is_err());
    }

    #[test]
    fn synchronization_watermark_is_fenced_by_git_head_and_previous_watermark() {
        let (git, dir) = GitRepo::init_temp();
        let repo = git2::Repository::open(dir.path()).unwrap();
        let tree = repo
            .find_tree(repo.treebuilder(None).unwrap().write().unwrap())
            .unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        let root = repo
            .commit(None, &signature, &signature, "root", &tree, &[])
            .unwrap();
        let root_commit = repo.find_commit(root).unwrap();
        let head = repo
            .commit(None, &signature, &signature, "head", &tree, &[&root_commit])
            .unwrap();
        repo.reference("refs/heads/work", head, false, "fixture")
            .unwrap();
        let root = root.to_string();
        let head = head.to_string();
        // Production fences the working head and publishes the watermark in
        // one checked transaction (`apply_cycle_watermarks`).
        let path = dir.path().to_path_buf();
        let apply = |branch: &str, head: &str, expected: Option<&str>, new: &str| {
            GitRepo::apply_ref_commits(&[
                RefCommit {
                    repo_path: path.clone(),
                    ref_name: format!("refs/heads/{branch}"),
                    expected_old: Some(head.to_owned()),
                    new_oid: Some(head.to_owned()),
                },
                GitRepo::sync_watermark_update(
                    path.clone(),
                    branch,
                    expected.map(str::to_owned),
                    Some(new.to_owned()),
                ),
            ])
            .map(|updates| updates[1])
        };
        assert_eq!(git.read_sync_watermark("work").unwrap(), None);
        assert_eq!(
            apply("work", &head, None, &root).unwrap(),
            BranchRefUpdate::Applied
        );
        assert_eq!(
            apply("work", &head, None, &root).unwrap(),
            BranchRefUpdate::AlreadyApplied
        );
        assert!(apply("work", &root, Some(&root), &head).is_err());
        assert!(apply("work", &head, None, &head).is_err());
        assert_eq!(
            git.read_sync_watermark("work").unwrap().as_deref(),
            Some(root.as_str())
        );
        assert_eq!(
            apply("work", &head, Some(&root), &head).unwrap(),
            BranchRefUpdate::Applied
        );
        assert_eq!(
            git.read_sync_watermark("work").unwrap().as_deref(),
            Some(head.as_str())
        );
        assert!(apply("missing", &head, None, &root).is_err());
        assert_eq!(git.read_sync_watermark("missing").unwrap(), None);
    }

    // -- Serde tests --

    fn commit_all(repo: &GitRepo, message: &str, author: &GitAuthor) -> String {
        repo.add_all().unwrap();
        repo.commit(message, author).unwrap()
    }

    fn author(name: &str) -> GitAuthor {
        GitAuthor {
            name: name.to_string(),
            email: format!("{name}@test.example"),
        }
    }

    fn bare_ref_tip(path: &Path, branch: &str) -> String {
        let repo = git2::Repository::open_bare(path).unwrap();
        let reference = repo
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap();
        reference.target().unwrap().to_string()
    }

    #[test]
    fn approval_evidence_ref_is_create_only_and_replay_verifies_the_exact_commit() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Evidence"));
        let name = "refs/g5/approval-evidence/test/repo/source";
        repo.create_approval_evidence_ref_if_absent_or_equal(name, &source)
            .unwrap();
        repo.create_approval_evidence_ref_if_absent_or_equal(name, &source)
            .unwrap();
        std::fs::write(dir.path().join("source.txt"), "moved\n").unwrap();
        let moved = commit_all(&repo, "moved", &author("Evidence"));
        assert!(matches!(
            repo.create_approval_evidence_ref_if_absent_or_equal(name, &moved),
            Err(G5Error::Conflict { .. })
        ));
        assert!(repo
            .create_approval_evidence_ref_if_absent_or_equal("refs/heads/public", &source)
            .is_err());
        let raw = repo.repo().unwrap();
        assert_eq!(
            raw.find_reference(name)
                .unwrap()
                .target()
                .unwrap()
                .to_string(),
            source
        );
    }

    #[test]
    fn workspace_fork_refs_are_atomic_idempotent_and_owned_by_receipt() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Fork"));
        let fork_id = uuid::Uuid::new_v4();

        assert_eq!(
            repo.create_workspace_fork_refs("exact/name", fork_id, &source)
                .unwrap(),
            ForkRefsCreateOutcome::Created
        );
        assert_eq!(
            repo.create_workspace_fork_refs("exact/name", fork_id, &source)
                .unwrap(),
            ForkRefsCreateOutcome::AlreadyApplied
        );
        repo.verify_workspace_fork_refs("exact/name", fork_id, &source)
            .unwrap();
        assert!(repo
            .local_branch_names()
            .unwrap()
            .contains(&"exact/name".to_string()));
        assert!(repo
            .delete_workspace_fork_refs_if_matches("exact/name", fork_id, &source)
            .unwrap());
        assert!(!repo
            .delete_workspace_fork_refs_if_matches("exact/name", fork_id, &source)
            .unwrap());
    }

    #[test]
    fn workspace_fork_ref_replay_rejects_equal_branch_without_receipt() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Fork"));
        repo.create_branch("occupied", &source).unwrap();

        let error = repo
            .create_workspace_fork_refs("occupied", uuid::Uuid::new_v4(), &source)
            .unwrap_err();
        assert!(matches!(error, G5Error::Conflict { .. }));
        assert_eq!(repo.resolve_branch_head("occupied").unwrap(), source);
    }

    #[test]
    fn workspace_fork_cleanup_ignores_foreign_branch_without_receipt() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Fork"));
        repo.create_branch("occupied", &source).unwrap();

        assert!(!repo
            .delete_workspace_fork_refs_if_matches("occupied", uuid::Uuid::new_v4(), &source,)
            .unwrap());
        assert_eq!(repo.resolve_branch_head("occupied").unwrap(), source);
    }

    #[test]
    fn workspace_fork_copy_cleanup_ignores_foreign_branch_without_receipt() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Fork"));
        repo.create_branch("occupied-copy", &source).unwrap();
        let fork_id = uuid::Uuid::new_v4();
        let copy = ForkCopyRefs {
            workspace_id: fork_id,
            flush_id: uuid::Uuid::new_v4(),
            snapshot_oid: source.clone(),
        };

        assert!(!repo
            .delete_workspace_fork_copy_refs_if_matches("occupied-copy", fork_id, &source, &copy,)
            .unwrap());
        assert_eq!(repo.resolve_branch_head("occupied-copy").unwrap(), source);
    }

    #[test]
    fn workspace_fork_copy_refs_keep_dirty_snapshot_separate_from_branch() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Fork"));
        std::fs::write(dir.path().join("dirty.txt"), "captured\n").unwrap();
        let snapshot = commit_all(&repo, "snapshot", &author("Fork"));
        let fork_id = uuid::Uuid::new_v4();
        let copy = ForkCopyRefs {
            workspace_id: fork_id,
            flush_id: uuid::Uuid::new_v4(),
            snapshot_oid: snapshot.clone(),
        };

        assert_eq!(
            repo.create_workspace_fork_copy_refs("live-copy", fork_id, &source, &copy)
                .unwrap(),
            ForkRefsCreateOutcome::Created
        );
        repo.verify_workspace_fork_copy_refs("live-copy", fork_id, &source, &copy)
            .unwrap();
        assert_eq!(repo.resolve_branch_head("live-copy").unwrap(), source);
        {
            let raw = repo.repo().unwrap();
            let prefix = format!("refs/g5/flush/{}/{}", copy.workspace_id, copy.flush_id);
            assert_eq!(
                raw.find_reference(&format!("{prefix}/snapshot"))
                    .unwrap()
                    .target()
                    .unwrap()
                    .to_string(),
                snapshot
            );
        }
        assert!(repo
            .delete_workspace_fork_copy_refs_if_matches("live-copy", fork_id, &source, &copy,)
            .unwrap());
    }

    #[test]
    fn workspace_fork_cleanup_preserves_a_changed_destination() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Fork"));
        let fork_id = uuid::Uuid::new_v4();
        repo.create_workspace_fork_refs("destination", fork_id, &source)
            .unwrap();

        std::fs::write(dir.path().join("changed.txt"), "changed\n").unwrap();
        let changed = commit_all(&repo, "changed", &author("Fork"));
        {
            let raw = repo.repo().unwrap();
            raw.reference(
                "refs/heads/destination",
                git2::Oid::from_str(&changed).unwrap(),
                true,
                "test mutation",
            )
            .unwrap();
        }

        let error = repo
            .delete_workspace_fork_refs_if_matches("destination", fork_id, &source)
            .unwrap_err();
        assert!(matches!(error, G5Error::Conflict { .. }));
        assert_eq!(repo.resolve_branch_head("destination").unwrap(), changed);
    }

    #[test]
    fn workspace_delete_removes_current_branch_and_flush_refs_but_keeps_fork_proof() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("Delete"));
        let workspace_id = uuid::Uuid::new_v4();
        let fork_id = uuid::Uuid::new_v4();
        let flush_id = uuid::Uuid::new_v4();
        repo.create_workspace_fork_refs("delete-me", fork_id, &source)
            .unwrap();

        std::fs::write(dir.path().join("changed.txt"), "changed\n").unwrap();
        let changed = commit_all(&repo, "changed", &author("Delete"));
        {
            let raw = repo.repo().unwrap();
            let changed = git2::Oid::from_str(&changed).unwrap();
            raw.reference(
                "refs/heads/delete-me",
                changed,
                true,
                "advance workspace branch",
            )
            .unwrap();
            let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
            raw.reference(
                &format!("{prefix}/branch"),
                git2::Oid::from_str(&source).unwrap(),
                false,
                "flush branch",
            )
            .unwrap();
            raw.reference(
                &format!("{prefix}/snapshot"),
                changed,
                false,
                "flush snapshot",
            )
            .unwrap();
        }

        assert!(repo
            .delete_workspace_refs("delete-me", workspace_id, &[flush_id])
            .unwrap());
        assert!(!repo
            .delete_workspace_refs("delete-me", workspace_id, &[flush_id])
            .unwrap());

        let raw = repo.repo().unwrap();
        assert!(raw.find_reference("refs/heads/delete-me").is_err());
        let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
        assert!(raw.find_reference(&format!("{prefix}/branch")).is_err());
        assert!(raw.find_reference(&format!("{prefix}/snapshot")).is_err());
        assert_eq!(
            raw.find_reference(&format!("refs/g5/forks/{fork_id}"))
                .unwrap()
                .target()
                .unwrap()
                .to_string(),
            source
        );
    }

    #[test]
    fn flush_prune_removes_only_the_selected_receipt_and_is_idempotent() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source = commit_all(&repo, "source", &author("FlushPrune"));
        let source_oid = git2::Oid::from_str(&source).unwrap();
        let workspace_id = uuid::Uuid::new_v4();
        let selected = uuid::Uuid::new_v4();
        let retained = uuid::Uuid::new_v4();
        {
            let raw = repo.repo().unwrap();
            for flush_id in [selected, retained] {
                let prefix = format!("refs/g5/flush/{workspace_id}/{flush_id}");
                raw.reference(
                    &format!("{prefix}/branch"),
                    source_oid,
                    false,
                    "flush branch",
                )
                .unwrap();
                raw.reference(
                    &format!("{prefix}/snapshot"),
                    source_oid,
                    false,
                    "flush snapshot",
                )
                .unwrap();
            }
        }

        assert!(repo
            .delete_workspace_flush_refs(workspace_id, selected)
            .unwrap());
        assert!(!repo
            .delete_workspace_flush_refs(workspace_id, selected)
            .unwrap());

        let raw = repo.repo().unwrap();
        let selected_prefix = format!("refs/g5/flush/{workspace_id}/{selected}");
        assert!(raw
            .find_reference(&format!("{selected_prefix}/branch"))
            .is_err());
        assert!(raw
            .find_reference(&format!("{selected_prefix}/snapshot"))
            .is_err());
        let retained_prefix = format!("refs/g5/flush/{workspace_id}/{retained}");
        assert_eq!(
            raw.find_reference(&format!("{retained_prefix}/branch"))
                .unwrap()
                .target(),
            Some(source_oid)
        );
        assert_eq!(
            raw.find_reference(&format!("{retained_prefix}/snapshot"))
                .unwrap()
                .target(),
            Some(source_oid)
        );
    }

    fn native_git() -> crate::native::NativeGit {
        crate::native::NativeGit::cli(std::time::Duration::from_secs(60)).unwrap()
    }

    fn block_on<T>(work: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(work)
    }

    fn repo_with_bare_origin(url: Option<&str>) -> (GitRepo, tempfile::TempDir, tempfile::TempDir) {
        let (repo, dir) = GitRepo::init_temp();
        let remote_dir = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(remote_dir.path()).unwrap();
        repo.repo()
            .unwrap()
            .remote("origin", url.unwrap_or(remote_dir.path().to_str().unwrap()))
            .unwrap();
        (repo, dir, remote_dir)
    }

    #[test]
    fn scm_publication_rejects_stale_origin_and_pushurl() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("file"), "one").unwrap();
        let head = commit_all(&repo, "one", &author("Origin"));
        {
            let raw = repo.repo().unwrap();
            raw.remote("origin", "https://github.com/o/r.git").unwrap();
            raw.remote_set_pushurl(
                "origin",
                Some("https://github.com.attacker.example/o/r.git"),
            )
            .unwrap();
        }
        let auth = GitAuth::scm_token(
            "stored-member-token".into(),
            "github",
            "https://github.com/o/r.git",
        );
        let git = native_git();
        let error = repo
            .push_create_ref(&git, "origin", &head, "main", &auth)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("SCM push destination is not authorized"),
            "{error}"
        );
        assert!(!error.contains("stored-member-token"));
        {
            let raw = repo.repo().unwrap();
            raw.remote_set_pushurl("origin", None).unwrap();
            raw.remote_set_url("origin", "http://github.com/o/r.git")
                .unwrap();
        }
        let error = repo
            .publish_commit(&git, "origin", &head, "main", None, &auth)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("SCM push destination is not authorized"),
            "{error}"
        );
    }

    #[test]
    fn implicit_provider_push_never_creates_or_recreates_a_branch() {
        let (repo, dir, remote_dir) = repo_with_bare_origin(None);
        let remote = git2::Repository::open_bare(remote_dir.path()).unwrap();
        let git = native_git();
        let author = author("Implicit push");
        std::fs::write(dir.path().join("file"), "one").unwrap();
        let a = commit_all(&repo, "one", &author);
        repo.publish_commit(&git, "origin", &a, "existing", None, &GitAuth::None)
            .unwrap();
        std::fs::write(dir.path().join("file"), "two").unwrap();
        let b = commit_all(&repo, "two", &author);
        repo.push_existing_commit(&git, "origin", &b, "existing", &a, false, &GitAuth::None)
            .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "existing"), b);
        assert!(repo
            .push_existing_commit(&git, "origin", &a, "existing", &b, false, &GitAuth::None)
            .is_err());
        assert!(repo
            .push_existing_commit(&git, "origin", &b, "missing", &a, false, &GitAuth::None)
            .is_err());
        assert!(remote.find_reference("refs/heads/missing").is_err());
        remote
            .find_reference("refs/heads/existing")
            .unwrap()
            .delete()
            .unwrap();
        assert!(repo
            .push_existing_commit(&git, "origin", &b, "existing", &a, false, &GitAuth::None)
            .is_err());
        assert!(remote.find_reference("refs/heads/existing").is_err());
        assert!(repo
            .push_existing_commit(
                &git,
                "origin",
                &b,
                "existing",
                &"0".repeat(40),
                true,
                &GitAuth::None
            )
            .is_err());
        // An accepted workspace rewrite replaces only the exact observed tip.
        repo.publish_commit(&git, "origin", &b, "existing", None, &GitAuth::None)
            .unwrap();
        let rewritten = {
            let raw = repo.repo().unwrap();
            let parent = raw.find_commit(git2::Oid::from_str(&a).unwrap()).unwrap();
            let tree = raw
                .find_commit(git2::Oid::from_str(&b).unwrap())
                .unwrap()
                .tree()
                .unwrap();
            let signature = git2::Signature::now("Rewrite", "rewrite@example.test").unwrap();
            raw.commit(None, &signature, &signature, "rewritten", &tree, &[&parent])
                .unwrap()
                .to_string()
        };
        assert!(repo
            .push_existing_commit(
                &git,
                "origin",
                &rewritten,
                "existing",
                &b,
                false,
                &GitAuth::None
            )
            .is_err());
        assert!(repo
            .push_existing_commit(
                &git,
                "origin",
                &rewritten,
                "existing",
                &a,
                true,
                &GitAuth::None
            )
            .unwrap_err()
            .to_string()
            .contains("Force-with-lease rejected"));
        assert_eq!(bare_ref_tip(remote_dir.path(), "existing"), b);
        repo.push_existing_commit(
            &git,
            "origin",
            &rewritten,
            "existing",
            &b,
            true,
            &GitAuth::None,
        )
        .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "existing"), rewritten);
        // Rewinding to an ancestor is a rewrite too.
        repo.push_existing_commit(
            &git,
            "origin",
            &a,
            "existing",
            &rewritten,
            true,
            &GitAuth::None,
        )
        .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "existing"), a);
    }

    #[test]
    fn create_ref_publication_refuses_an_existing_destination_without_moving_it() {
        let (repo, dir, remote_dir) = repo_with_bare_origin(None);
        let git = native_git();
        let author = author("Create only");
        std::fs::write(dir.path().join("file"), "one").unwrap();
        let base = commit_all(&repo, "one", &author);
        repo.push_create_ref(&git, "origin", &base, "g5/fork", &GitAuth::None)
            .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "g5/fork"), base);
        // A fast-forwardable descendant must still be refused: create-only
        // never advances a pre-existing branch.
        std::fs::write(dir.path().join("file"), "two").unwrap();
        let ahead = commit_all(&repo, "two", &author);
        let error = repo
            .push_create_ref(&git, "origin", &ahead, "g5/fork", &GitAuth::None)
            .unwrap_err();
        assert!(
            error.to_string().contains("Force-with-lease rejected")
                && error.to_string().contains("expected absent"),
            "{error}"
        );
        assert_eq!(bare_ref_tip(remote_dir.path(), "g5/fork"), base);
        let missing = "1".repeat(40);
        assert!(repo
            .push_create_ref(&git, "origin", &missing, "g5/other", &GitAuth::None)
            .unwrap_err()
            .to_string()
            .contains("is not present"));
    }

    #[test]
    fn publish_commit_fast_forwards_creates_and_leases_rewrites() {
        let (repo, dir, remote_dir) = repo_with_bare_origin(None);
        let git = native_git();
        let author = author("FfOrLease");

        // (1) Absent remote branch (observed tip None) -> create push.
        std::fs::write(dir.path().join("README.md"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);
        repo.publish_commit(&git, "origin", &base, "main", None, &GitAuth::None)
            .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), base);

        // (2) Observed tip is an ancestor -> plain fast-forward.
        std::fs::write(dir.path().join("README.md"), "ahead\n").unwrap();
        let ahead = commit_all(&repo, "ahead", &author);
        repo.publish_commit(&git, "origin", &ahead, "main", Some(&base), &GitAuth::None)
            .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), ahead);

        // (3) Rewrite: the new commit does not descend from the observed tip ->
        //     lease on the observed tip publishes the rewritten history.
        repo.create_branch("rebased", &base).unwrap();
        repo.checkout_branch("rebased").unwrap();
        std::fs::write(dir.path().join("README.md"), "rebased\n").unwrap();
        let rebased = commit_all(&repo, "rebased", &author);
        repo.publish_commit(
            &git,
            "origin",
            &rebased,
            "main",
            Some(&ahead),
            &GitAuth::None,
        )
        .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), rebased);

        // (4) Stale observed tip after a concurrent advance -> lease refused,
        //     remote untouched.
        std::fs::write(dir.path().join("README.md"), "second rebase\n").unwrap();
        let second = commit_all(&repo, "second rebase", &author);
        repo.create_branch("unrelated", &base).unwrap();
        repo.checkout_branch("unrelated").unwrap();
        std::fs::write(dir.path().join("README.md"), "unrelated\n").unwrap();
        let unrelated = commit_all(&repo, "unrelated", &author);
        let err = repo
            .publish_commit(
                &git,
                "origin",
                &unrelated,
                "main",
                Some(&ahead),
                &GitAuth::None,
            )
            .unwrap_err();
        assert!(
            format!("{err}").contains("Force-with-lease rejected"),
            "{err}"
        );
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), rebased);

        // (5) A stale fast-forward (remote moved past the observed ancestor)
        //     is refused by Git as non-fast-forward, never forced.
        assert!(repo
            .publish_commit(&git, "origin", &ahead, "main", Some(&base), &GitAuth::None)
            .is_err());
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), rebased);

        // (6) Recovery with the current tip, then an up-to-date no-op.
        repo.publish_commit(
            &git,
            "origin",
            &second,
            "main",
            Some(&rebased),
            &GitAuth::None,
        )
        .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), second);
        repo.publish_commit(
            &git,
            "origin",
            &second,
            "main",
            Some(&second),
            &GitAuth::None,
        )
        .unwrap();
        assert_eq!(bare_ref_tip(remote_dir.path(), "main"), second);
    }

    #[test]
    fn relative_remote_supports_leased_publication() {
        let root = tempfile::tempdir().unwrap();
        let local = root.path().join("local");
        git2::Repository::init(&local).unwrap();
        let repo = GitRepo::open(&local).unwrap();
        let remote_path = root.path().join("remote.git");
        git2::Repository::init_bare(&remote_path).unwrap();
        repo.repo()
            .unwrap()
            .remote("origin", "../remote.git")
            .unwrap();
        let git = native_git();
        let author = author("Relative lease");
        std::fs::write(local.join("README.md"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);
        repo.push_create_ref(&git, "origin", &base, "main", &GitAuth::None)
            .unwrap();
        assert_eq!(bare_ref_tip(&remote_path, "main"), base);
        std::fs::write(local.join("README.md"), "advanced\n").unwrap();
        let advanced = commit_all(&repo, "advanced", &author);
        repo.push_existing_commit(
            &git,
            "origin",
            &advanced,
            "main",
            &base,
            false,
            &GitAuth::None,
        )
        .unwrap();
        assert_eq!(bare_ref_tip(&remote_path, "main"), advanced);
        std::fs::write(local.join("README.md"), "later\n").unwrap();
        let later = commit_all(&repo, "later", &author);
        assert!(repo
            .push_existing_commit(&git, "origin", &later, "main", &base, false, &GitAuth::None)
            .unwrap_err()
            .to_string()
            .contains("Force-with-lease rejected"));
        assert_eq!(
            repo.repo()
                .unwrap()
                .find_remote("origin")
                .unwrap()
                .url()
                .unwrap(),
            "../remote.git"
        );
        assert_eq!(bare_ref_tip(&remote_path, "main"), advanced);
    }

    fn run_git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            // CI runners have no ambient Git identity.
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[test]
    fn remote_branch_reads_scale_past_the_diagnostic_capture_limit() {
        let (producer, producer_dir, remote_dir) = repo_with_bare_origin(None);
        let git = native_git();
        std::fs::write(producer_dir.path().join("f"), "x").unwrap();
        let tip = commit_all(&producer, "base", &author("Many"));
        producer
            .publish_commit(&git, "origin", &tip, "main", None, &GitAuth::None)
            .unwrap();
        let updates: String = (0..2500)
            .map(|index| format!("create refs/heads/team/feature-branch-{index:05} {tip}\n"))
            .collect();
        let mut child = std::process::Command::new("git")
            .current_dir(remote_dir.path())
            .args(["update-ref", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(child.stdin.as_mut().unwrap(), updates.as_bytes()).unwrap();
        assert!(child.wait().unwrap().success());
        let url = remote_dir.path().to_str().unwrap();
        // ~150 KiB of advertisement: more than the 64 KiB diagnostic capture.
        let branches = block_on(git.remote_branches(url, &GitAuth::None)).unwrap();
        assert_eq!(branches.len(), 2501);
        assert_eq!(
            block_on(git.remote_branch_tip(url, "team/feature-branch-02499", &GitAuth::None))
                .unwrap(),
            Some(tip.clone())
        );
        // A server-side pattern is a suffix match; the tip read stays exact.
        assert_eq!(
            block_on(git.remote_branch_tip(url, "feature-branch-02499", &GitAuth::None)).unwrap(),
            None
        );
    }

    #[test]
    fn object_download_never_runs_automatic_maintenance_in_storage() {
        let (producer, producer_dir, remote_dir) = repo_with_bare_origin(None);
        let git = native_git();
        std::fs::write(producer_dir.path().join("f"), "x").unwrap();
        let tip = commit_all(&producer, "base", &author("Maintenance"));
        producer
            .publish_commit(&git, "origin", &tip, "main", None, &GitAuth::None)
            .unwrap();
        let storage = tempfile::tempdir().unwrap();
        run_git(storage.path(), &["init", "--bare"]);
        // Synchronous auto-gc with the lowest threshold: any maintenance run
        // would pack the loose objects seeded below before fetch returns.
        for (key, value) in [
            ("gc.auto", "1"),
            ("gc.autoDetach", "false"),
            ("maintenance.auto", "true"),
            ("maintenance.autoDetach", "false"),
        ] {
            run_git(storage.path(), &["config", key, value]);
        }
        let mut seeded = 0;
        // gc --auto estimates loose objects from `objects/17`; exceed its
        // threshold so a maintenance run is certain if it is not disabled.
        let bucket = |path: &Path| {
            std::fs::read_dir(path.join("objects/17")).map_or(0, |entries| entries.count())
        };
        while bucket(storage.path()) < 3 {
            seeded += 1;
            let mut child = std::process::Command::new("git")
                .current_dir(storage.path())
                .args(["hash-object", "-w", "--stdin"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            std::io::Write::write_all(
                child.stdin.as_mut().unwrap(),
                format!("loose {seeded}").as_bytes(),
            )
            .unwrap();
            assert!(child.wait().unwrap().success());
        }
        // A reachable seed: gc's repack would pack it, while this small
        // fetch itself unpacks to loose objects. Any pack proves gc ran.
        let empty_tree = run_git(
            storage.path(),
            &["hash-object", "-t", "tree", "-w", "/dev/null"],
        );
        let seed = run_git(storage.path(), &["commit-tree", &empty_tree, "-m", "seed"]);
        run_git(storage.path(), &["update-ref", "refs/heads/seed", &seed]);
        let loose = |path: &Path| run_git(path, &["count-objects"]);
        let before = loose(storage.path());
        block_on(git.fetch_branch_objects(
            storage.path(),
            remote_dir.path().to_str().unwrap(),
            "main",
            &GitAuth::None,
        ))
        .unwrap();
        let after = loose(storage.path());
        let count = |text: &str| {
            text.split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        assert!(
            count(&after) >= count(&before),
            "maintenance packed loose objects: {before} -> {after}"
        );
        assert!(bucket(storage.path()) >= 3);
        let packs = std::fs::read_dir(storage.path().join("objects/pack"))
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "pack")
            })
            .count();
        assert_eq!(
            packs, 0,
            "automatic maintenance repacked repository storage"
        );
        assert!(GitRepo::open(storage.path())
            .unwrap()
            .has_commit(&tip)
            .unwrap());
    }

    #[test]
    fn remote_observations_and_object_download_change_no_refs() {
        let (producer, producer_dir, remote_dir) = repo_with_bare_origin(None);
        let git = native_git();
        let author = author("ObjectOnlyDownload");
        std::fs::write(producer_dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&producer, "base", &author);
        producer
            .publish_commit(&git, "origin", &base, "main", None, &GitAuth::None)
            .unwrap();
        let url = remote_dir.path().to_str().unwrap();
        assert_eq!(
            block_on(git.remote_branch_tip(url, "main", &GitAuth::None)).unwrap(),
            Some(base.clone())
        );
        assert_eq!(
            block_on(git.remote_branch_tip(url, "missing", &GitAuth::None)).unwrap(),
            None
        );
        assert_eq!(
            block_on(git.remote_branches(url, &GitAuth::None)).unwrap(),
            vec![("main".to_owned(), base.clone())]
        );

        let (consumer, consumer_dir) = GitRepo::init_temp();
        std::fs::write(consumer_dir.path().join("sentinel.txt"), "sentinel\n").unwrap();
        let sentinel = commit_all(&consumer, "sentinel", &author);
        let refs = |repo: &GitRepo| -> Vec<(String, Option<git2::Oid>)> {
            repo.repo()
                .unwrap()
                .references()
                .unwrap()
                .map(|reference| {
                    let reference = reference.unwrap();
                    (reference.name().unwrap().to_string(), reference.target())
                })
                .collect()
        };
        let fetch_head = consumer.repo().unwrap().path().join("FETCH_HEAD");
        std::fs::write(&fetch_head, format!("{sentinel}\t\tsentinel fetch head\n")).unwrap();
        let refs_before = refs(&consumer);
        assert!(!consumer.has_commit(&base).unwrap());

        block_on(git.fetch_branch_objects(consumer_dir.path(), url, "main", &GitAuth::None))
            .unwrap();

        assert!(consumer.has_commit(&base).unwrap());
        assert_eq!(refs(&consumer), refs_before);
        assert_eq!(
            std::fs::read_to_string(&fetch_head).unwrap(),
            format!("{sentinel}\t\tsentinel fetch head\n")
        );
        assert!(block_on(git.fetch_branch_objects(
            consumer_dir.path(),
            url,
            "missing",
            &GitAuth::None
        ))
        .is_err());
    }

    #[test]
    fn blame_for_path_single_commit_attribution() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(
            dir.path().join("spec.md"),
            "line one\nline two\nline three\n",
        )
        .unwrap();
        let commit = commit_all(&repo, "add spec", &author("Alice"));

        let attr = repo
            .blame_for_path(&commit, "spec.md", 1, 3)
            .unwrap()
            .expect("single-commit range resolves to one attribution");
        assert_eq!(attr.author, "Alice");
        assert!(
            attr.committed_at.ends_with('Z') && attr.committed_at.contains('T'),
            "expected ISO-8601 UTC, got {}",
            attr.committed_at
        );
    }

    #[test]
    fn blame_for_path_ambiguous_multi_commit_returns_none() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("spec.md"), "alpha\nbeta\ngamma\n").unwrap();
        commit_all(&repo, "v1", &author("Alice"));

        // Edit only the middle line and re-commit under a different author.
        std::fs::write(dir.path().join("spec.md"), "alpha\nBETA-edited\ngamma\n").unwrap();
        let c2 = commit_all(&repo, "v2", &author("Bob"));

        // Range covering both an Alice line and the Bob line is ambiguous.
        assert!(repo.blame_for_path(&c2, "spec.md", 1, 3).unwrap().is_none());

        // Single-author sub-ranges still resolve to that author.
        assert_eq!(
            repo.blame_for_path(&c2, "spec.md", 2, 2)
                .unwrap()
                .unwrap()
                .author,
            "Bob"
        );
        assert_eq!(
            repo.blame_for_path(&c2, "spec.md", 1, 1)
                .unwrap()
                .unwrap()
                .author,
            "Alice"
        );
    }

    #[test]
    fn blame_for_path_missing_or_invalid_returns_none() {
        let (repo, dir) = GitRepo::init_temp();
        std::fs::write(dir.path().join("spec.md"), "only line\n").unwrap();
        let commit = commit_all(&repo, "add spec", &author("Alice"));

        // Missing path → unknown, not an error.
        assert!(repo
            .blame_for_path(&commit, "does-not-exist.md", 1, 1)
            .unwrap()
            .is_none());
        // Invalid range → None.
        assert!(repo
            .blame_for_path(&commit, "spec.md", 0, 0)
            .unwrap()
            .is_none());
        assert!(repo
            .blame_for_path(&commit, "spec.md", 5, 2)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_diff_file_status_serialization() {
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Added).unwrap(),
            "\"added\""
        );
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Deleted).unwrap(),
            "\"deleted\""
        );
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Modified).unwrap(),
            "\"modified\""
        );
        assert_eq!(
            serde_json::to_string(&DiffFileStatus::Renamed).unwrap(),
            "\"renamed\""
        );
    }

    #[test]
    fn test_tree_entry_type_serialization() {
        assert_eq!(
            serde_json::to_string(&TreeEntryType::File).unwrap(),
            "\"file\""
        );
        assert_eq!(
            serde_json::to_string(&TreeEntryType::Directory).unwrap(),
            "\"directory\""
        );
    }

    #[test]
    fn test_diff_summary_camel_case_serialization() {
        let summary = DiffSummary {
            files: vec![DiffFileSummary {
                path: "src/main.rs".to_string(),
                additions: 10,
                deletions: 5,
                status: DiffFileStatus::Modified,
            }],
            total_additions: 10,
            total_deletions: 5,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert!(json.get("totalAdditions").is_some());
        assert!(json.get("totalDeletions").is_some());
        assert!(json.get("total_additions").is_none());
        assert!(json.get("total_deletions").is_none());

        let file = json["files"][0].as_object().unwrap();
        assert!(file.get("path").is_some());
        assert!(file.get("additions").is_some());
        assert!(file.get("deletions").is_some());
        assert!(file.get("status").is_some());
    }

    #[test]
    fn test_tree_entry_camel_case_serialization() {
        let entry = TreeEntry {
            name: "main.rs".to_string(),
            path: "src/main.rs".to_string(),
            entry_type: TreeEntryType::File,
            size: Some(1024),
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert!(json.get("entryType").is_some());
        assert!(json.get("entry_type").is_none());
        assert_eq!(json["entryType"], "file");
        assert_eq!(json["size"], 1024);
    }

    #[test]
    fn test_tree_entry_directory_no_size() {
        let entry = TreeEntry {
            name: "src".to_string(),
            path: "src".to_string(),
            entry_type: TreeEntryType::Directory,
            size: None,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["entryType"], "directory");
        assert!(json["size"].is_null());
    }

    // -- GitRepo tests (using init_temp) --

    #[test]
    fn test_open_existing_repo() {
        let (repo, dir) = GitRepo::init_temp();
        drop(repo);
        let reopened = GitRepo::open(dir.path());
        assert!(reopened.is_ok());
    }

    #[test]
    fn test_open_nonexistent_path() {
        let result = GitRepo::open(Path::new("/nonexistent/path/to/repo"));
        assert!(result.is_err());
    }

    #[test]
    fn test_current_head_after_initial_commit() {
        let (repo, _dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // Create an initial file and commit
        let file_path = _dir.path().join("README.md");
        std::fs::write(&file_path, "# Hello").unwrap();

        repo.add_all().unwrap();
        let sha = repo.commit("Initial commit", &author).unwrap();

        assert!(!sha.is_empty());
        assert_eq!(sha.len(), 40); // Full hex SHA

        let head = repo.current_head().unwrap();
        assert_eq!(head, sha);
    }

    #[test]
    fn test_list_commits_returns_newest_first_with_summaries() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        repo.add_all().unwrap();
        let first = repo.commit("first commit", &author).unwrap();

        std::fs::write(dir.path().join("b.txt"), "two\n").unwrap();
        repo.add_all().unwrap();
        let second = repo.commit("second commit", &author).unwrap();

        std::fs::write(dir.path().join("c.txt"), "three\n").unwrap();
        repo.add_all().unwrap();
        let head = repo.commit("third commit", &author).unwrap();

        let commits = repo.list_commits(&head, None, 10).unwrap();
        assert_eq!(commits.len(), 3, "all three commits should be returned");

        // Newest first.
        assert_eq!(commits[0].sha, head);
        assert_eq!(commits[0].summary, "third commit");
        assert_eq!(commits[1].sha, second);
        assert_eq!(commits[1].summary, "second commit");
        assert_eq!(commits[2].sha, first);
        assert_eq!(commits[2].summary, "first commit");

        // Author name and full SHA shape are populated.
        assert_eq!(commits[0].author_name, author.name);
        assert_eq!(commits[0].sha.len(), 40);
        assert!(commits[0].committed_at > 0);
    }

    #[test]
    fn test_list_commits_respects_limit() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        repo.add_all().unwrap();
        repo.commit("first commit", &author).unwrap();

        std::fs::write(dir.path().join("b.txt"), "two\n").unwrap();
        repo.add_all().unwrap();
        let head = repo.commit("second commit", &author).unwrap();

        let commits = repo.list_commits(&head, None, 1).unwrap();
        assert_eq!(commits.len(), 1, "limit should cap the result");
        assert_eq!(commits[0].summary, "second commit");
    }

    #[test]
    fn latest_commit_touching_paths_selects_newest_match_and_ignores_unrelated_commits() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        std::fs::create_dir_all(dir.path().join("docs/ws-1")).unwrap();
        std::fs::write(dir.path().join("docs/ws-1/code_review.md"), "first\n").unwrap();
        commit_all(&repo, "write review", &author);
        std::fs::write(dir.path().join("docs/ws-1/review_report.md"), "second\n").unwrap();
        let latest_report_commit = commit_all(&repo, "write newer review", &author);
        std::fs::write(dir.path().join("unrelated.txt"), "later\n").unwrap();
        let head = commit_all(&repo, "unrelated change", &author);

        let found = repo
            .latest_commit_touching_paths(
                &head,
                &[
                    "docs/ws-1/code_review.md".to_string(),
                    "docs/ws-1/review_report.md".to_string(),
                ],
            )
            .unwrap()
            .expect("report commit should be found");

        assert_eq!(found.sha, latest_report_commit);
        assert_eq!(found.summary, "write newer review");
    }

    #[test]
    fn latest_commit_touching_paths_matches_renamed_old_and_new_paths() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        std::fs::write(dir.path().join("old.md"), "review\n").unwrap();
        commit_all(&repo, "add old", &author);
        std::fs::rename(dir.path().join("old.md"), dir.path().join("new.md")).unwrap();
        let head = commit_all(&repo, "rename review", &author);

        for path in ["old.md", "new.md"] {
            let found = repo
                .latest_commit_touching_paths(&head, &[path.to_string()])
                .unwrap()
                .expect("either side of the rename should match");
            assert_eq!(found.sha, head);
        }
        assert!(repo
            .latest_commit_touching_paths(&head, &[])
            .unwrap()
            .is_none());
    }

    #[test]
    fn latest_commit_touching_path_where_finds_a_deleted_historical_match() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        let report = dir
            .path()
            .join("docs/ws-1/sync/job-1/review/code_review.md");
        std::fs::create_dir_all(report.parent().unwrap()).unwrap();
        std::fs::write(&report, "review\n").unwrap();
        commit_all(&repo, "add sync review", &author);
        std::fs::remove_file(&report).unwrap();
        let deletion = commit_all(&repo, "remove sync review", &author);
        std::fs::write(dir.path().join("later.txt"), "later\n").unwrap();
        let head = commit_all(&repo, "later change", &author);

        let found = repo
            .latest_commit_touching_path_where(&head, |path| {
                path.starts_with("docs/ws-1/sync/") && path.ends_with("/review/code_review.md")
            })
            .unwrap()
            .expect("deleted historical report should still be found");
        assert_eq!(found.sha, deletion);
    }

    #[test]
    fn list_commits_hides_base_tip_and_ancestors_not_just_root() {
        // Linear history: base_root -> base_mid -> base_tip -> head_one -> head.
        // The workspace branch diverged from its base at `base_tip`. Hiding the
        // base TIP must return only the post-divergence commits, whereas hiding
        // the base ROOT (the old, buggy behavior) removes only the first commit
        // and leaks the base-branch commits back into the list.
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("f.txt"), "0\n").unwrap();
        let base_root = commit_all(&repo, "base root", &author);
        std::fs::write(dir.path().join("f.txt"), "1\n").unwrap();
        let _base_mid = commit_all(&repo, "base mid", &author);
        std::fs::write(dir.path().join("f.txt"), "2\n").unwrap();
        let base_tip = commit_all(&repo, "base tip", &author);
        std::fs::write(dir.path().join("f.txt"), "3\n").unwrap();
        let head_one = commit_all(&repo, "head one", &author);
        std::fs::write(dir.path().join("f.txt"), "4\n").unwrap();
        let head = commit_all(&repo, "head two", &author);

        // Hiding the base TIP: only the branch's own commits since divergence,
        // newest first — the correct behavior.
        let since_tip = repo.list_commits(&head, Some(&base_tip), 50).unwrap();
        let since_tip_shas: Vec<&str> = since_tip.iter().map(|c| c.sha.as_str()).collect();
        assert_eq!(
            since_tip_shas,
            vec![head.as_str(), head_one.as_str()],
            "hiding the base tip returns only post-divergence commits, newest first"
        );

        // Hiding the base ROOT (old behavior): everything except the root, so
        // base-branch commits wrongly leak into the list.
        let since_root = repo.list_commits(&head, Some(&base_root), 50).unwrap();
        let since_root_shas: Vec<String> = since_root.iter().map(|c| c.sha.clone()).collect();
        assert_eq!(
            since_root.len(),
            4,
            "hiding only the root leaves the full history minus the root — the bug"
        );
        assert!(
            !since_root_shas.contains(&base_root),
            "the root itself is excluded when hidden"
        );
        assert!(
            since_root_shas.contains(&base_tip),
            "base-branch commits leak in when only the root is hidden"
        );
    }

    #[test]
    fn first_parent_history_paginates_ten_at_a_time_without_duplicates() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        let mut expected = Vec::new();
        for index in 0..23 {
            std::fs::write(dir.path().join("history.txt"), format!("{index}\n")).unwrap();
            expected.push(commit_all(&repo, &format!("commit {index}"), &author));
        }
        expected.reverse();
        let head = expected[0].clone();

        let first = repo.list_first_parent_commits(&head, None, 10).unwrap();
        assert_eq!(first.commits.len(), 10);
        assert_eq!(first.commits[0].sha, expected[0]);
        assert_eq!(first.next_cursor.as_deref(), Some(expected[10].as_str()));

        let second = repo
            .list_first_parent_commits(&head, first.next_cursor.as_deref(), 10)
            .unwrap();
        assert_eq!(second.commits.len(), 10);
        assert_eq!(second.commits[0].sha, expected[10]);
        assert_eq!(second.next_cursor.as_deref(), Some(expected[20].as_str()));

        let third = repo
            .list_first_parent_commits(&head, second.next_cursor.as_deref(), 10)
            .unwrap();
        assert_eq!(third.commits.len(), 3);
        assert!(third.next_cursor.is_none());

        let actual: Vec<_> = first
            .commits
            .into_iter()
            .chain(second.commits)
            .chain(third.commits)
            .map(|commit| commit.sha)
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn first_parent_history_skips_merge_side_parent_and_rejects_stale_cursor() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        std::fs::write(dir.path().join("root.txt"), "root\n").unwrap();
        let root = commit_all(&repo, "root", &author);
        std::fs::write(dir.path().join("main.txt"), "main\n").unwrap();
        let main = commit_all(&repo, "main", &author);

        let (side, merge) = {
            let raw = repo.repo().unwrap();
            let signature = git2::Signature::now("Test", "test@example.com").unwrap();
            let root_commit = raw
                .find_commit(git2::Oid::from_str(&root).unwrap())
                .unwrap();
            let main_commit = raw
                .find_commit(git2::Oid::from_str(&main).unwrap())
                .unwrap();
            let root_tree = root_commit.tree().unwrap();
            let side = raw
                .commit(
                    None,
                    &signature,
                    &signature,
                    "side",
                    &root_tree,
                    &[&root_commit],
                )
                .unwrap();
            let side_commit = raw.find_commit(side).unwrap();
            let main_tree = main_commit.tree().unwrap();
            let merge = raw
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    "merge",
                    &main_tree,
                    &[&main_commit, &side_commit],
                )
                .unwrap()
                .to_string();
            (side.to_string(), merge)
        };

        let page = repo.list_first_parent_commits(&merge, None, 10).unwrap();
        let shas: Vec<_> = page
            .commits
            .iter()
            .map(|entry| entry.sha.as_str())
            .collect();
        assert_eq!(shas, vec![merge.as_str(), main.as_str(), root.as_str()]);
        assert_eq!(page.commits[0].parent_count, 2);
        assert!(!shas.contains(&side.as_str()));
        assert!(matches!(
            repo.list_first_parent_commits(&merge, Some(&side), 10),
            Err(G5Error::StaleHistory)
        ));
    }

    #[test]
    fn commit_diff_covers_root_modify_add_delete_rename_and_text_hunks() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        std::fs::write(dir.path().join("root.txt"), "one\n").unwrap();
        std::fs::write(dir.path().join("delete.txt"), "gone\n").unwrap();
        std::fs::write(dir.path().join("rename-old.txt"), "rename me\n").unwrap();
        let root = commit_all(&repo, "root", &author);

        let root_diff = repo.commit_diff_summary(&root, &root).unwrap();
        assert!(matches!(
            root_diff.comparison_base,
            CommitComparisonBase::EmptyTree
        ));
        assert!(root_diff
            .files
            .iter()
            .all(|file| file.status == DiffFileStatus::Added));

        std::fs::write(dir.path().join("root.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.path().join("added.txt"), "added\n").unwrap();
        std::fs::remove_file(dir.path().join("delete.txt")).unwrap();
        std::fs::rename(
            dir.path().join("rename-old.txt"),
            dir.path().join("rename-new.txt"),
        )
        .unwrap();
        let head = commit_all(&repo, "all states", &author);

        let summary = repo.commit_diff_summary(&head, &head).unwrap();
        assert!(matches!(
            summary.comparison_base,
            CommitComparisonBase::Parent { ref sha } if sha == &root
        ));
        let statuses: Vec<_> = summary
            .files
            .iter()
            .map(|file| file.status.clone())
            .collect();
        assert!(statuses.contains(&DiffFileStatus::Added));
        assert!(statuses.contains(&DiffFileStatus::Modified));
        assert!(statuses.contains(&DiffFileStatus::Deleted));
        assert!(statuses.contains(&DiffFileStatus::Renamed));
        let renamed = summary
            .files
            .iter()
            .find(|file| file.status == DiffFileStatus::Renamed)
            .unwrap();
        assert_eq!(renamed.old_path.as_deref(), Some("rename-old.txt"));
        assert_eq!(renamed.new_path.as_deref(), Some("rename-new.txt"));

        let file = repo.commit_file_diff(&head, &head, "root.txt").unwrap();
        assert_eq!(file.status, CommitFileDiffStatus::Text);
        assert_eq!(file.additions, 1);
        assert!(!file.hunks.is_empty());
        assert!(file.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
            matches!(line.line_type, CommitDiffLineType::Addition) && line.content == "two"
        }));

        let empty = commit_all(&repo, "empty commit", &author);
        let empty_summary = repo.commit_diff_summary(&empty, &empty).unwrap();
        assert!(matches!(
            empty_summary.comparison_base,
            CommitComparisonBase::Parent { ref sha } if sha == &head
        ));
        assert!(empty_summary.files.is_empty());
        assert_eq!(empty_summary.total_additions, 0);
        assert_eq!(empty_summary.total_deletions, 0);
    }

    #[test]
    fn commit_file_diff_reports_binary_oversized_invalid_and_unreachable_inputs() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let root = commit_all(&repo, "root", &author);
        std::fs::write(dir.path().join("binary.bin"), [0, 1, 2, 0, 255]).unwrap();
        std::fs::write(
            dir.path().join("large.txt"),
            "x".repeat(MAX_COMMIT_FILE_DIFF_BYTES + 1),
        )
        .unwrap();
        let head = commit_all(&repo, "binary and large", &author);

        assert_eq!(
            repo.commit_file_diff(&head, &head, "binary.bin")
                .unwrap()
                .status,
            CommitFileDiffStatus::Binary
        );
        let large = repo.commit_file_diff(&head, &head, "large.txt").unwrap();
        assert_eq!(large.status, CommitFileDiffStatus::TooLarge);
        assert!(large.hunks.is_empty());

        std::fs::write(
            dir.path().join("many-lines.txt"),
            "line\n".repeat(MAX_COMMIT_FILE_DIFF_LINES + 1),
        )
        .unwrap();
        let line_head = commit_all(&repo, "many rendered lines", &author);
        let many_lines = repo
            .commit_file_diff(&line_head, &line_head, "many-lines.txt")
            .unwrap();
        assert_eq!(many_lines.status, CommitFileDiffStatus::TooLarge);
        assert!(many_lines.hunks.is_empty());

        assert!(matches!(
            repo.commit_diff_summary(&head, "abc"),
            Err(G5Error::ValidationError { .. })
        ));
        assert!(matches!(
            repo.commit_diff_summary(&root, &head),
            Err(G5Error::NotFound { .. })
        ));
    }

    /// Commit `content` as the only file `path` without index filters, so a
    /// global `core.autocrlf` cannot rewrite the bytes under test.
    fn commit_raw_file(repo: &GitRepo, parent: Option<&str>, path: &str, content: &[u8]) -> String {
        let inner = repo.repo().unwrap();
        let blob = inner.blob(content).unwrap();
        let mut builder = inner.treebuilder(None).unwrap();
        builder.insert(path, blob, 0o100644).unwrap();
        let tree = inner.find_tree(builder.write().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.com").unwrap();
        let parents: Vec<git2::Commit<'_>> = parent
            .map(|sha| {
                inner
                    .find_commit(git2::Oid::from_str(sha).unwrap())
                    .unwrap()
            })
            .into_iter()
            .collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        inner
            .commit(None, &signature, &signature, "raw", &tree, &parents)
            .unwrap()
            .to_string()
    }

    fn lines_of(diff: &CommitFileDiff) -> Vec<(char, Option<u32>, Option<u32>, &str)> {
        diff.hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .map(|line| {
                let kind = match line.line_type {
                    CommitDiffLineType::Context => ' ',
                    CommitDiffLineType::Addition => '+',
                    CommitDiffLineType::Deletion => '-',
                };
                (kind, line.old_line, line.new_line, line.content.as_str())
            })
            .collect()
    }

    #[test]
    fn text_file_diff_cuts_the_hunks_a_review_round_cuts_with_carriage_returns_kept() {
        let (repo, _dir) = GitRepo::init_temp();
        let old = "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten\r\n\
                   eleven\r\ntwelve\r\nthirteen\r\nfourteen\r\nfifteen\r\n";
        let new = "one\r\nTWO\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten\r\n\
                   eleven\r\ntwelve\r\nthirteen\r\nfourteen\r\nfifteen\r\nsixteen\r\n";
        let base = commit_raw_file(&repo, None, "crlf.txt", old.as_bytes());
        let head = commit_raw_file(&repo, Some(&base), "crlf.txt", new.as_bytes());
        let round = repo
            .file_diffs_between_commits(&base, &head)
            .unwrap()
            .remove(0);

        let buffers = text_file_diff("crlf.txt", old.as_bytes(), new.as_bytes()).unwrap();

        assert_eq!(buffers.status, CommitFileDiffStatus::Text);
        assert_eq!(buffers.hunks.len(), 2, "{buffers:?}");
        assert_eq!(
            serde_json::to_value(&buffers).unwrap(),
            serde_json::to_value(&round).unwrap(),
            "a buffer diff cuts the same hunks as a review round"
        );
        let lines = lines_of(&buffers);
        assert!(lines.contains(&('-', Some(2), None, "two\r")), "{lines:?}");
        assert!(lines.contains(&('+', None, Some(2), "TWO\r")), "{lines:?}");
        assert!(
            lines.contains(&(' ', Some(1), Some(1), "one\r")),
            "context lines keep their carriage return too: {lines:?}"
        );
        assert!(
            lines.contains(&('+', None, Some(16), "sixteen\r")),
            "{lines:?}"
        );
    }

    #[test]
    fn text_file_diff_keeps_three_lines_of_context_around_an_insertion() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
        let new = "1\n2\n3\n4\n5\ninserted\n6\n7\n8\n9\n10\n";

        let diff = text_file_diff("lines.txt", old.as_bytes(), new.as_bytes()).unwrap();

        assert_eq!(diff.status, CommitFileDiffStatus::Text);
        assert_eq!((diff.additions, diff.deletions), (1, 0));
        assert_eq!(diff.hunks.len(), 1);
        assert!(
            diff.hunks[0].header.starts_with("@@ -3,6 +3,7 @@"),
            "{}",
            diff.hunks[0].header
        );
        assert_eq!(
            lines_of(&diff),
            vec![
                (' ', Some(3), Some(3), "3"),
                (' ', Some(4), Some(4), "4"),
                (' ', Some(5), Some(5), "5"),
                ('+', None, Some(6), "inserted"),
                (' ', Some(6), Some(7), "6"),
                (' ', Some(7), Some(8), "7"),
                (' ', Some(8), Some(9), "8"),
            ]
        );
    }

    #[test]
    fn text_file_diff_of_an_empty_base_is_a_pure_insertion() {
        let diff = text_file_diff("new.txt", b"", b"first\nsecond\n").unwrap();

        assert_eq!(diff.status, CommitFileDiffStatus::Text);
        assert_eq!(diff.hunks.len(), 1);
        assert!(
            diff.hunks[0].header.starts_with("@@ -0,0 +1,2 @@"),
            "a pure insertion names no base line: {}",
            diff.hunks[0].header
        );
        assert_eq!(
            lines_of(&diff),
            vec![
                ('+', None, Some(1), "first"),
                ('+', None, Some(2), "second"),
            ]
        );

        let emptied = text_file_diff("new.txt", b"first\nsecond\n", b"").unwrap();
        assert!(
            emptied.hunks[0].header.starts_with("@@ -1,2 +0,0 @@"),
            "{}",
            emptied.hunks[0].header
        );
        assert_eq!((emptied.additions, emptied.deletions), (0, 2));
    }

    #[test]
    fn text_file_diff_of_identical_texts_has_no_hunks() {
        for text in ["", "same\n", "same\r\nlines\r\n"] {
            let diff = text_file_diff("same.txt", text.as_bytes(), text.as_bytes()).unwrap();
            assert_eq!(diff.status, CommitFileDiffStatus::Text, "{text:?}");
            assert!(diff.hunks.is_empty(), "{text:?}: {diff:?}");
            assert_eq!((diff.additions, diff.deletions), (0, 0));
        }
    }

    #[test]
    fn text_file_diff_reports_diffs_past_the_commit_limits_as_too_large() {
        let wide = text_file_diff(
            "wide.txt",
            b"x\n",
            format!("{}\n", "x".repeat(MAX_COMMIT_FILE_DIFF_BYTES + 1)).as_bytes(),
        )
        .unwrap();
        assert_eq!(wide.status, CommitFileDiffStatus::TooLarge);
        assert!(wide.hunks.is_empty());

        let long = text_file_diff(
            "long.txt",
            b"",
            "line\n".repeat(MAX_COMMIT_FILE_DIFF_LINES + 1).as_bytes(),
        )
        .unwrap();
        assert_eq!(long.status, CommitFileDiffStatus::TooLarge);
        assert!(long.hunks.is_empty());

        let within = text_file_diff(
            "within.txt",
            b"",
            "line\n".repeat(MAX_COMMIT_FILE_DIFF_LINES).as_bytes(),
        )
        .unwrap();
        assert_eq!(within.status, CommitFileDiffStatus::Text);
        assert_eq!(within.additions, MAX_COMMIT_FILE_DIFF_LINES);
    }

    #[test]
    fn text_file_diff_diffs_control_characters_as_text() {
        // libgit2 would call this binary (many control bytes), but it is
        // valid UTF-8 without NUL: the caller's text rule decides.
        let old = "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\nkeep\n";
        let new = "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{8}\nkeep\n";

        let diff = text_file_diff("controls.txt", old.as_bytes(), new.as_bytes()).unwrap();

        assert_eq!(diff.status, CommitFileDiffStatus::Text);
        assert_eq!(
            lines_of(&diff),
            vec![
                ('-', Some(1), None, "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}"),
                ('+', None, Some(1), "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{8}"),
                (' ', Some(2), Some(2), "keep"),
            ]
        );
    }

    #[test]
    fn test_add_all_and_commit() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // Create files
        std::fs::write(dir.path().join("file1.txt"), "content1").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "content2").unwrap();

        repo.add_all().unwrap();
        let sha1 = repo.commit("First commit", &author).unwrap();
        assert!(!sha1.is_empty());

        // Modify a file
        std::fs::write(dir.path().join("file1.txt"), "updated content").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Second commit", &author).unwrap();
        assert_ne!(sha1, sha2);
    }

    #[test]
    fn test_add_paths_stages_only_selected_files() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("file1.txt"), "content1").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "content2").unwrap();
        repo.add_all().unwrap();
        repo.commit("Initial commit", &author).unwrap();

        std::fs::write(dir.path().join("file1.txt"), "updated content").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "other update").unwrap();

        let initial = repo.current_head().unwrap();
        repo.add_paths(["file1.txt"]).unwrap();
        let scoped = repo.commit("Scoped commit", &author).unwrap();

        let changed = repo
            .diff_name_only_between_commits(&initial, &scoped)
            .unwrap();
        let status = repo.workdir_status().unwrap();

        assert!(changed.iter().any(|path| path == "file1.txt"));
        assert!(!changed.iter().any(|path| path == "file2.txt"));
        assert!(status.iter().any(|(_, path)| path == "file2.txt"));
    }

    #[test]
    fn diff_name_only_between_commits_is_sorted_and_includes_both_rename_paths() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("modified.txt"), "before\n").unwrap();
        std::fs::write(dir.path().join("old-name.txt"), "renamed\n").unwrap();
        std::fs::write(dir.path().join("deleted.txt"), "deleted\n").unwrap();
        repo.add_all().unwrap();
        let base = repo.commit("Base", &author).unwrap();

        std::fs::write(dir.path().join("modified.txt"), "after\n").unwrap();
        std::fs::rename(
            dir.path().join("old-name.txt"),
            dir.path().join("new-name.txt"),
        )
        .unwrap();
        std::fs::remove_file(dir.path().join("deleted.txt")).unwrap();
        std::fs::write(dir.path().join("added.txt"), "added\n").unwrap();
        repo.add_all().unwrap();
        let head = repo.commit("Head", &author).unwrap();

        assert_eq!(
            repo.diff_name_only_between_commits(&base, &head).unwrap(),
            vec![
                "added.txt".to_string(),
                "deleted.txt".to_string(),
                "modified.txt".to_string(),
                "new-name.txt".to_string(),
                "old-name.txt".to_string(),
            ]
        );
    }

    /// The exclude pattern list and the Rust classifier must agree on the
    /// durable roots: native Git (`git check-ignore`) hides exactly what
    /// `is_g5_runtime_metadata_path` rejects and shows what it accepts.
    #[test]
    fn g5_exclude_patterns_match_the_classifier_for_durable_roots() {
        let (_repo, dir) = GitRepo::init_temp();
        let exclude = dir.path().join(".git/info/exclude");
        std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
        std::fs::write(
            &exclude,
            g5_contracts::g5_paths::G5_INTERNAL_EXCLUDE_PATTERNS.join("\n") + "\n",
        )
        .unwrap();
        let ignored = |path: &str| {
            let status = std::process::Command::new("git")
                .args(["check-ignore", "-q", "--no-index", path])
                .current_dir(dir.path())
                .status()
                .expect("run git check-ignore");
            match status.code() {
                Some(0) => true,
                Some(1) => false,
                other => panic!("git check-ignore {path}: {other:?}"),
            }
        };
        for path in [
            ".g5/workbench/actions.yaml",
            ".g5/workbench/personas/be.md",
            ".g5/workspace.yaml",
            ".g5/pr_ontology_diff_design/",
            ".g5/pr_ontology_diff_design/topic/",
            ".g5/pr_ontology_diff_design/topic/ontology-plan.md",
            ".g5/pr_ontology_diff_design/topic/.gitignore",
            ".g5/pr_ontology_diff_design/topic/mocks/page.html",
            ".g5/pr_ontology_diff_design/topic/screenshots/nested/files.png",
        ] {
            assert!(!ignored(path), "{path} must not be ignored");
            assert!(!g5_contracts::g5_paths::is_g5_runtime_metadata_path(path));
        }
        for path in [
            ".g5/workbench/other.yaml",
            ".g5/workbench/personas/be.txt",
            ".g5/workbench/personas/nested/be.md",
            ".g5/auth_state.json",
            ".g5/pr_ontology_diff_design/.g5",
            ".g5/pr_ontology_diff_design/topic/.g5",
            ".g5/pr_ontology_diff_design/.g5/state.json",
            ".g5/pr_ontology_diff_design/topic/.g5/state.json",
            ".g5/pr_ontology_diff_design/.git",
            ".g5/pr_ontology_diff_design/topic/.git",
            ".g5/pr_ontology_diff_design/.git/config",
            ".g5/pr_ontology_diff_design/topic/.git/config",
            ".g5/pr_ontology_diff_design_old/plan.md",
        ] {
            assert!(ignored(path), "{path} must be ignored");
            assert!(g5_contracts::g5_paths::is_g5_runtime_metadata_path(path));
        }
    }

    #[test]
    fn test_add_paths_allows_committed_g5_content_only() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::create_dir_all(dir.path().join(".g5/preview_profiles")).unwrap();
        std::fs::create_dir_all(dir.path().join(".g5/conflicts/src")).unwrap();
        std::fs::create_dir_all(dir.path().join(".g5/experience/agent_llm_tooling")).unwrap();
        std::fs::create_dir_all(dir.path().join(".g5/experience/.g5/hook_state")).unwrap();
        std::fs::create_dir_all(dir.path().join(".g5/workbench/personas/nested")).unwrap();
        std::fs::write(
            dir.path().join(".g5/workbench/actions.yaml"),
            "be:\n  - name: Mock\n    actions: []\n",
        )
        .unwrap();
        std::fs::write(dir.path().join(".g5/workbench/personas/be.md"), "# BE\n").unwrap();
        std::fs::write(
            dir.path().join(".g5/workbench/personas/nested/be.md"),
            "# nested\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".g5/workspace.yaml"),
            "preview:\n  command: \"npm run preview\"\n  port: 3000\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".g5/preview_profiles/browser.yaml"),
            "command: \"npm run preview\"\nport: 3000\n",
        )
        .unwrap();
        std::fs::write(
            dir.path()
                .join(".g5/experience/agent_llm_tooling/lesson.md"),
            "# Lesson\n",
        )
        .unwrap();
        std::fs::write(dir.path().join(".g5/auth_state.json"), "{}").unwrap();
        std::fs::write(dir.path().join(".g5/conflicts/src/main.rs"), "runtime").unwrap();
        std::fs::write(
            dir.path()
                .join(".g5/experience/.g5/hook_state/pending.json"),
            "{}",
        )
        .unwrap();

        repo.add_paths([
            ".g5/workspace.yaml",
            ".g5/preview_profiles/browser.yaml",
            ".g5/experience/agent_llm_tooling/lesson.md",
            ".g5/workbench/actions.yaml",
            ".g5/workbench/personas/be.md",
            ".g5/workbench/personas/nested/be.md",
            ".g5/auth_state.json",
            ".g5/conflicts/src/main.rs",
            ".g5/experience/.g5/hook_state/pending.json",
        ])
        .unwrap();
        let sha = repo.commit("Scoped .g5 content", &author).unwrap();

        // Workbench templates and personas stage (workbench
        // g5.workbench.actions); nothing nested below `personas/` does.
        assert!(repo.show_file(&sha, ".g5/workbench/actions.yaml").is_ok());
        assert!(repo.show_file(&sha, ".g5/workbench/personas/be.md").is_ok());
        assert!(repo
            .show_file(&sha, ".g5/workbench/personas/nested/be.md")
            .is_err());

        assert!(repo.show_file(&sha, ".g5/workspace.yaml").is_ok());
        assert!(repo
            .show_file(&sha, ".g5/preview_profiles/browser.yaml")
            .is_ok());
        assert!(repo
            .show_file(&sha, ".g5/experience/agent_llm_tooling/lesson.md")
            .is_ok());
        assert!(repo.show_file(&sha, ".g5/auth_state.json").is_err());
        assert!(repo.show_file(&sha, ".g5/conflicts/src/main.rs").is_err());
        assert!(repo
            .show_file(&sha, ".g5/experience/.g5/hook_state/pending.json")
            .is_err());
    }

    #[test]
    fn test_create_branch_and_checkout() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // Need an initial commit first
        std::fs::write(dir.path().join("README.md"), "hello").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Initial", &author).unwrap();

        // Create and checkout a new branch
        repo.create_branch("feature", &sha).unwrap();
        repo.checkout_branch("feature").unwrap();
    }

    #[test]
    fn test_configure_branch_alias_sets_tracking_config() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("README.md"), "hello").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Initial", &author).unwrap();

        // Canonical (server-truth) branch and the upstream-named alias.
        repo.create_branch("feat/login", &sha).unwrap();
        if git2::Repository::open(dir.path())
            .unwrap()
            .find_branch("main", git2::BranchType::Local)
            .is_err()
        {
            repo.create_branch("main", &sha).unwrap();
        }

        repo.configure_branch_alias("main", "feat/login", "origin")
            .unwrap();

        // The alias maps back to the canonical branch on origin so a plain
        // `git pull` / `git push` round-trips through the proxy.
        let raw = git2::Repository::open(dir.path()).unwrap();
        let cfg = raw.config().unwrap();
        assert_eq!(cfg.get_string("branch.main.remote").unwrap(), "origin");
        assert_eq!(
            cfg.get_string("branch.main.merge").unwrap(),
            "refs/heads/feat/login"
        );
        assert_eq!(cfg.get_string("push.default").unwrap(), "upstream");
    }

    #[test]
    fn test_checkout_commit_detached() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("README.md"), "hello").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Initial", &author).unwrap();

        repo.checkout_commit(&sha).unwrap();
        // HEAD should be detached at the commit
        let head = repo.current_head().unwrap();
        assert_eq!(head, sha);
    }

    #[test]
    fn test_show_file() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        let content = b"Hello, World!";
        std::fs::write(dir.path().join("hello.txt"), content).unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Add hello", &author).unwrap();

        let file_content = repo.show_file(&sha, "hello.txt").unwrap();
        assert_eq!(file_content, content);
    }

    #[test]
    fn test_show_file_not_found() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("hello.txt"), "hi").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Add hello", &author).unwrap();

        let result = repo.show_file(&sha, "nonexistent.txt");
        assert!(result.is_err());
        match result.unwrap_err() {
            G5Error::NotFound { resource } => {
                assert!(resource.contains("nonexistent.txt"));
            }
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_blob_id_tracks_file_content_identity() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("tracked.md"), "same content").unwrap();
        std::fs::write(dir.path().join("other.md"), "first").unwrap();
        repo.add_all().unwrap();
        let first = repo.commit("Initial files", &author).unwrap();

        std::fs::write(dir.path().join("other.md"), "second").unwrap();
        repo.add_all().unwrap();
        let unrelated = repo.commit("Change unrelated file", &author).unwrap();

        let first_blob = repo.blob_id(&first, "tracked.md").unwrap();
        let (first_metadata_blob, first_size) = repo.blob_metadata(&first, "tracked.md").unwrap();
        assert_eq!(first_metadata_blob, first_blob);
        assert_eq!(first_size, "same content".len());
        assert_eq!(
            GitRepo::blob_id_for_bytes(b"same content").unwrap(),
            first_blob
        );
        let unrelated_blob = repo.blob_id(&unrelated, "tracked.md").unwrap();
        assert_eq!(
            first_blob, unrelated_blob,
            "unchanged file content should keep the same blob id across commits"
        );

        std::fs::write(dir.path().join("tracked.md"), "changed content").unwrap();
        repo.add_all().unwrap();
        let changed = repo.commit("Change tracked file", &author).unwrap();

        let changed_blob = repo.blob_id(&changed, "tracked.md").unwrap();
        assert_ne!(
            unrelated_blob, changed_blob,
            "changed file content should get a different blob id"
        );
    }

    #[test]
    fn test_blob_id_not_found_for_missing_or_directory_path() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::create_dir_all(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested/file.md"), "content").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Add nested file", &author).unwrap();

        let missing = repo.blob_id(&sha, "missing.md");
        assert!(matches!(missing, Err(G5Error::NotFound { .. })));

        let directory = repo.blob_id(&sha, "nested");
        assert!(matches!(directory, Err(G5Error::NotFound { .. })));
    }

    #[test]
    fn test_list_tree_root() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("file1.txt"), "content1").unwrap();
        std::fs::create_dir_all(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/file2.txt"), "content2").unwrap();

        repo.add_all().unwrap();
        let sha = repo.commit("Add files", &author).unwrap();

        let entries = repo.list_tree(&sha, "").unwrap();
        assert!(entries.len() >= 2); // file1.txt and subdir

        let file_entry = entries.iter().find(|e| e.name == "file1.txt");
        assert!(file_entry.is_some());
        let file_entry = file_entry.unwrap();
        assert!(matches!(file_entry.entry_type, TreeEntryType::File));
        assert!(file_entry.size.is_some());

        let dir_entry = entries.iter().find(|e| e.name == "subdir");
        assert!(dir_entry.is_some());
        let dir_entry = dir_entry.unwrap();
        assert!(matches!(dir_entry.entry_type, TreeEntryType::Directory));
        assert!(dir_entry.size.is_none());
    }

    #[test]
    fn test_list_tree_subdirectory() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        repo.add_all().unwrap();
        let sha = repo.commit("Add src", &author).unwrap();

        let entries = repo.list_tree(&sha, "src").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "main.rs");
        assert_eq!(entries[0].path, "src/main.rs");
    }

    #[test]
    fn test_list_tree_paths_names_every_file_and_directory_under_the_path() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("README.md"), "root").unwrap();
        std::fs::create_dir_all(dir.path().join("src/nested")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("src/nested/lib.rs"), "pub fn lib() {}").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Add tree", &author).unwrap();

        let listing = repo.list_tree_paths(&sha, "", 10).unwrap();
        assert!(!listing.truncated);
        let mut all = listing.paths;
        all.sort();
        assert_eq!(
            all,
            vec![
                "README.md",
                "src",
                "src/main.rs",
                "src/nested",
                "src/nested/lib.rs",
            ]
        );

        let mut under_src = repo.list_tree_paths(&sha, "src", 10).unwrap().paths;
        under_src.sort();
        assert_eq!(
            under_src,
            vec!["src/main.rs", "src/nested", "src/nested/lib.rs"]
        );

        assert!(matches!(
            repo.list_tree_paths(&sha, "missing", 10),
            Err(G5Error::NotFound { .. })
        ));
        assert!(matches!(
            repo.list_tree_paths(&sha, "README.md", 10),
            Err(G5Error::NotFound { .. })
        ));

        // Past the bound the walk stops: a caller learns the set is
        // incomplete without receiving every path of a wide tree.
        let bounded = repo.list_tree_paths(&sha, "", 2).unwrap();
        assert!(bounded.truncated);
        assert_eq!(bounded.paths.len(), 2);
    }

    /// A commit whose root holds `width` files, `chain` nested directories
    /// each holding one file, and one blob of `big` bytes; built from tree
    /// objects so the walk, not the filesystem, is what the test exercises.
    fn commit_wide_and_deep(repo: &GitRepo, width: usize, chain: usize, big: usize) -> String {
        let git = repo.repo().unwrap();
        let small = git.blob(b"x").unwrap();
        let large = git.blob(&vec![b'y'; big]).unwrap();
        let mut deepest = None;
        for level in (0..chain).rev() {
            let mut builder = git.treebuilder(None).unwrap();
            builder.insert("leaf.txt", small, 0o100644).unwrap();
            if let Some(child) = deepest {
                builder
                    .insert(format!("d{}", level + 1), child, 0o040000)
                    .unwrap();
            }
            deepest = Some(builder.write().unwrap());
        }
        let mut root = git.treebuilder(None).unwrap();
        for index in 0..width {
            root.insert(format!("file-{index:05}.txt"), small, 0o100644)
                .unwrap();
        }
        root.insert("big.bin", large, 0o100644).unwrap();
        if let Some(chain_root) = deepest {
            root.insert("d0", chain_root, 0o040000).unwrap();
        }
        let tree = git.find_tree(root.write().unwrap()).unwrap();
        let author = git2::Signature::now("G5 Test", "g5@test.local").unwrap();
        git.commit(None, &author, &author, "wide and deep", &tree, &[])
            .unwrap()
            .to_string()
    }

    const UNBOUNDED: TreeListLimits = TreeListLimits {
        max_entries: usize::MAX,
        max_bytes: usize::MAX,
    };

    #[test]
    fn list_tree_recursive_stops_at_the_entry_cap_and_reports_it() {
        let (repo, _dir) = GitRepo::init_temp();
        let sha = commit_wide_and_deep(&repo, 50, 0, 1);

        let whole = repo
            .list_tree_recursive(&sha, "", u32::MAX, UNBOUNDED)
            .unwrap();
        assert!(!whole.truncated);
        assert_eq!(whole.entries.len(), 51);

        let limits = TreeListLimits {
            max_entries: 10,
            max_bytes: usize::MAX,
        };
        let capped = repo
            .list_tree_recursive(&sha, "", u32::MAX, limits)
            .unwrap();
        assert!(capped.truncated, "the 11th entry crosses the cap");
        assert_eq!(capped.entries.len(), 10);

        let exact = TreeListLimits {
            max_entries: 51,
            max_bytes: usize::MAX,
        };
        let fitting = repo.list_tree_recursive(&sha, "", u32::MAX, exact).unwrap();
        assert!(!fitting.truncated, "a tree at the cap is complete");
        assert_eq!(fitting.entries.len(), 51);
    }

    #[test]
    fn list_tree_recursive_stops_at_the_byte_budget() {
        let (repo, _dir) = GitRepo::init_temp();
        let sha = commit_wide_and_deep(&repo, 50, 0, 1);
        let budget = entry_bytes("file-00000.txt") * 5;
        let limits = TreeListLimits {
            max_entries: usize::MAX,
            max_bytes: budget,
        };
        let listing = repo
            .list_tree_recursive(&sha, "", u32::MAX, limits)
            .unwrap();
        assert!(listing.truncated, "the byte bound stops the walk");
        let budgeted: usize = listing
            .entries
            .iter()
            .map(|entry| entry_bytes(&entry.path))
            .sum();
        assert!(budgeted <= budget, "{budgeted} > {budget}");
        assert!(listing.entries.len() < 50);
    }

    #[test]
    fn list_tree_recursive_walks_a_deep_chain_without_recursing() {
        let (repo, _dir) = GitRepo::init_temp();
        // A chain deeper than a small stack would carry one frame per
        // directory: the queue-driven walk costs one entry per level, so
        // it completes on a 128 KiB thread where a recursive walk could not.
        let chain = 2_000;
        let sha = commit_wide_and_deep(&repo, 0, chain, 1);
        let path = _dir.path().to_path_buf();

        let (whole, paths, capped) = std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || {
                let repo = GitRepo::open(&path).unwrap();
                let whole = repo
                    .list_tree_recursive(&sha, "", u32::MAX, UNBOUNDED)
                    .unwrap();
                let paths = repo.list_tree_paths(&sha, "", usize::MAX).unwrap();
                let capped = repo
                    .list_tree_recursive(
                        &sha,
                        "",
                        u32::MAX,
                        TreeListLimits {
                            max_entries: 100,
                            max_bytes: usize::MAX,
                        },
                    )
                    .unwrap();
                (whole, paths, capped)
            })
            .unwrap()
            .join()
            .unwrap();

        assert!(!whole.truncated);
        // big.bin, d0, and per level one leaf plus the next directory.
        assert_eq!(whole.entries.len(), 2 + 2 * chain - 1);
        let deepest = whole
            .entries
            .iter()
            .map(|entry| entry.path.matches('/').count())
            .max()
            .unwrap();
        assert_eq!(deepest, chain);

        assert!(!paths.truncated);
        assert_eq!(paths.paths.len(), whole.entries.len());

        assert!(capped.truncated);
        assert_eq!(capped.entries.len(), 100);
    }

    #[test]
    fn list_tree_recursive_honors_depth_and_sizes_files_from_headers() {
        let (repo, _dir) = GitRepo::init_temp();
        let sha = commit_wide_and_deep(&repo, 1, 3, 4096);

        let shallow = repo.list_tree_recursive(&sha, "", 1, UNBOUNDED).unwrap();
        assert!(!shallow.truncated);
        let mut top = shallow
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();
        top.sort_unstable();
        assert_eq!(top, vec!["big.bin", "d0", "file-00000.txt"]);
        let big = shallow
            .entries
            .iter()
            .find(|entry| entry.path == "big.bin")
            .unwrap();
        assert_eq!(big.size, Some(4096));
        assert!(matches!(big.entry_type, TreeEntryType::File));

        let two = repo.list_tree_recursive(&sha, "", 2, UNBOUNDED).unwrap();
        let mut level_two = two
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .filter(|path| path.starts_with("d0/"))
            .collect::<Vec<_>>();
        level_two.sort_unstable();
        assert_eq!(level_two, vec!["d0/d1", "d0/leaf.txt"]);

        let under = repo.list_tree_recursive(&sha, "d0", 1, UNBOUNDED).unwrap();
        let mut names = under
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, vec!["d1", "leaf.txt"]);

        assert!(repo
            .list_tree_recursive(&sha, "", 0, UNBOUNDED)
            .unwrap()
            .entries
            .is_empty());

        let (id, size) = repo.blob_metadata(&sha, "big.bin").unwrap();
        assert_eq!(size, 4096);
        assert_eq!(
            id,
            git2::Oid::hash_object(git2::ObjectType::Blob, &vec![b'y'; 4096])
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn test_list_tree_paths_skips_names_that_are_not_utf8() {
        let (repo, _dir) = GitRepo::init_temp();
        // Built from tree objects: the filesystem need not accept such names.
        let commit = {
            let git = repo.repo().unwrap();
            let blob = git.blob(b"content").unwrap();
            let mut inner = git.treebuilder(None).unwrap();
            inner.insert("inner.txt", blob, 0o100644).unwrap();
            let inner_oid = inner.write().unwrap();
            let mut root = git.treebuilder(None).unwrap();
            root.insert("README.md", blob, 0o100644).unwrap();
            root.insert("good", inner_oid, 0o040000).unwrap();
            root.insert(&b"bad\xff"[..], inner_oid, 0o040000).unwrap();
            root.insert(&b"bad\xfe.txt"[..], blob, 0o100644).unwrap();
            let tree = git.find_tree(root.write().unwrap()).unwrap();
            let author = git2::Signature::now("G5 Test", "g5@test.local").unwrap();
            git.commit(None, &author, &author, "names outside UTF-8", &tree, &[])
                .unwrap()
        };

        let mut paths = repo
            .list_tree_paths(&commit.to_string(), "", 10)
            .unwrap()
            .paths;
        paths.sort();
        assert_eq!(paths, vec!["README.md", "good", "good/inner.txt"]);
        let mut listed = repo
            .list_tree_recursive(&commit.to_string(), "", u32::MAX, UNBOUNDED)
            .unwrap()
            .entries
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        listed.sort();
        assert_eq!(listed, vec!["README.md", "good", "good/inner.txt"]);
    }

    #[test]
    fn test_diff_summary() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // First commit
        std::fs::write(dir.path().join("file.txt"), "line1\nline2\n").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        // Second commit — modify the file
        std::fs::write(dir.path().join("file.txt"), "line1\nline2\nline3\n").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Second", &author).unwrap();

        let summary = repo.diff_summary(&sha1, &sha2).unwrap();
        assert_eq!(summary.files.len(), 1);
        assert_eq!(summary.files[0].path, "file.txt");
        assert!(summary.total_additions > 0 || summary.total_deletions > 0);
    }

    #[test]
    fn test_diff_summary_added_file() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // First commit
        std::fs::write(dir.path().join("existing.txt"), "hello").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        // Second commit — add a new file
        std::fs::write(dir.path().join("new_file.txt"), "new content\n").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Add new file", &author).unwrap();

        let summary = repo.diff_summary(&sha1, &sha2).unwrap();
        let new_file = summary.files.iter().find(|f| f.path == "new_file.txt");
        assert!(new_file.is_some());
        assert!(matches!(new_file.unwrap().status, DiffFileStatus::Added));
    }

    #[test]
    fn test_diff_unified_between_commits_basic() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // First commit
        std::fs::write(dir.path().join("file.txt"), "line1\nline2\n").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        // Second commit — modify file
        std::fs::write(dir.path().join("file.txt"), "line1\nline2\nline3\n").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Second", &author).unwrap();

        let diff = repo
            .diff_unified_between_commits(&sha1, &sha2, None)
            .unwrap();
        assert!(!diff.is_empty(), "diff should not be empty");
        assert!(
            diff.contains("+line3"),
            "diff should contain the added line"
        );
    }

    #[test]
    fn test_diff_unified_between_commits_path_filter() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // First commit with two files
        std::fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        // Second commit — modify both files
        std::fs::write(dir.path().join("a.txt"), "alpha\nalpha2\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "beta\nbeta2\n").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Second", &author).unwrap();

        // Filter to a.txt only
        let diff = repo
            .diff_unified_between_commits(&sha1, &sha2, Some("a.txt"))
            .unwrap();
        assert!(diff.contains("alpha2"), "diff should contain a.txt changes");
        assert!(
            !diff.contains("beta2"),
            "diff should NOT contain b.txt changes"
        );
    }

    #[test]
    fn test_diff_unified_between_commits_no_changes() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("file.txt"), "same\n").unwrap();
        repo.add_all().unwrap();
        let sha = repo.commit("Only commit", &author).unwrap();

        // Diff a commit against itself → empty
        let diff = repo.diff_unified_between_commits(&sha, &sha, None).unwrap();
        assert!(
            diff.is_empty(),
            "diff of a commit against itself should be empty"
        );
    }

    /// TC-SGD-013: Diff without path filter includes all changes.
    #[test]
    fn test_diff_unified_no_filter_includes_all() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // First commit with two files
        std::fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        // Second commit — modify both
        std::fs::write(dir.path().join("a.txt"), "alpha\nalpha2\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "beta\nbeta2\n").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Second", &author).unwrap();

        // No path filter → both files in diff
        let diff = repo
            .diff_unified_between_commits(&sha1, &sha2, None)
            .unwrap();
        assert!(diff.contains("alpha2"), "diff should contain a.txt changes");
        assert!(diff.contains("beta2"), "diff should contain b.txt changes");
    }

    /// TC-SGD-014: Diff with invalid base SHA returns error.
    #[test]
    fn test_diff_unified_invalid_base_sha() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("file.txt"), "content\n").unwrap();
        repo.add_all().unwrap();
        let head = repo.commit("Initial", &author).unwrap();

        let result = repo.diff_unified_between_commits(
            "0000000000000000000000000000000000000000",
            &head,
            None,
        );
        assert!(result.is_err(), "Should fail with invalid base SHA");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("0000000000000000000000000000000000000000")
                || err_msg.to_lowercase().contains("not found")
                || err_msg.to_lowercase().contains("could not find")
                || err_msg.to_lowercase().contains("no match")
                || err_msg.to_lowercase().contains("null oid")
                || err_msg.to_lowercase().contains("cannot exist"),
            "Error should reference the bad SHA or null OID, got: {}",
            err_msg
        );
    }

    /// TC-SGD-015: Diff with file not present at base commit shows addition.
    #[test]
    fn test_diff_unified_new_file_added() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // First commit — just README
        std::fs::write(dir.path().join("README.md"), "hello\n").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("Initial", &author).unwrap();

        // Second commit — add a new file
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/auth.rs"), "fn auth() {}\n").unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Add auth", &author).unwrap();

        let diff = repo
            .diff_unified_between_commits(&sha1, &sha2, Some("src/auth.rs"))
            .unwrap();
        assert!(!diff.is_empty(), "Diff for new file should not be empty");
        // New file: all lines are additions (+)
        assert!(
            diff.contains("+fn auth()"),
            "Diff should show added lines, got: {}",
            diff
        );
        // Should reference the file path
        assert!(
            diff.contains("auth.rs"),
            "Diff header should reference auth.rs"
        );
    }

    /// TC-SGD-016: Diff output follows valid unified patch format.
    #[test]
    fn test_diff_unified_valid_patch_format() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("file.txt"), "line1\nline2\nline3\n").unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        std::fs::write(
            dir.path().join("file.txt"),
            "line1\nmodified\nline3\nnew_line\n",
        )
        .unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Second", &author).unwrap();

        let diff = repo
            .diff_unified_between_commits(&sha1, &sha2, Some("file.txt"))
            .unwrap();

        // Should contain hunk header with @@ pattern
        let has_hunk_header = diff
            .lines()
            .any(|line| line.starts_with("@@") && line.contains("-") && line.contains("+"));
        assert!(
            has_hunk_header,
            "Diff should contain @@ hunk headers, got:\n{}",
            diff
        );

        // Content lines should start with ' ', '+', or '-'
        let content_lines: Vec<&str> = diff
            .lines()
            .filter(|l| {
                !l.starts_with("diff ")
                    && !l.starts_with("---")
                    && !l.starts_with("+++")
                    && !l.starts_with("@@")
                    && !l.starts_with("index ")
                    && !l.starts_with("\\ No newline")
                    && !l.is_empty()
            })
            .collect();
        for line in &content_lines {
            assert!(
                line.starts_with(' ') || line.starts_with('+') || line.starts_with('-'),
                "Content line should start with ' ', '+', or '-', got: '{}'",
                line
            );
        }
    }

    /// TC-SGD-017: Large diff is truncated with guardrail note.
    #[test]
    fn test_diff_unified_large_truncation() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        // Create a large file (~120KB of content)
        let mut large_content = String::new();
        for i in 0..5000 {
            large_content.push_str(&format!(
                "line {} — original content for testing large diffs\n",
                i
            ));
        }
        std::fs::write(dir.path().join("large.md"), &large_content).unwrap();
        repo.add_all().unwrap();
        let sha1 = repo.commit("First", &author).unwrap();

        // Rewrite entire file with different content
        let mut new_content = String::new();
        for i in 0..5000 {
            new_content.push_str(&format!(
                "line {} — completely rewritten content version two\n",
                i
            ));
        }
        std::fs::write(dir.path().join("large.md"), &new_content).unwrap();
        repo.add_all().unwrap();
        let sha2 = repo.commit("Rewrite", &author).unwrap();

        let diff = repo
            .diff_unified_between_commits(&sha1, &sha2, Some("large.md"))
            .unwrap();

        // Should be truncated to ~100KB
        assert!(
            diff.len() <= 110 * 1024, // small margin for the truncation note
            "Diff should be truncated to ≤110KB, got {} bytes",
            diff.len()
        );

        // Should contain truncation note
        assert!(
            diff.contains("[DIFF TRUNCATED"),
            "Truncated diff should contain truncation note, got {} bytes without it",
            diff.len()
        );

        // Truncated output should not be cut mid-line
        let last_line_before_note = diff
            .lines()
            .rev()
            .find(|l| !l.starts_with("[DIFF") && !l.is_empty());
        assert!(
            last_line_before_note.is_some(),
            "Should have content before truncation note"
        );
    }

    #[test]
    fn test_checkout_nonexistent_branch_fails() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("README.md"), "hello").unwrap();
        repo.add_all().unwrap();
        repo.commit("Initial", &author).unwrap();

        let result = repo.checkout_branch("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_checkout_invalid_sha_fails() {
        let (repo, dir) = GitRepo::init_temp();
        let author = crate::auth::GitAuthor::test_author();

        std::fs::write(dir.path().join("README.md"), "hello").unwrap();
        repo.add_all().unwrap();
        repo.commit("Initial", &author).unwrap();

        let result = repo.checkout_commit("not-a-valid-sha");
        assert!(result.is_err());
    }

    #[test]
    fn test_analyze_head_merge_no_checkout_reports_clean_merge_without_mutation() {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::test_author();
        let default_branch = repo
            .default_branch()
            .unwrap_or_else(|| "master".to_string());

        std::fs::write(dir.path().join("README.md"), "base\n").unwrap();
        let base = commit_all(&repo, "Initial", &author);
        repo.create_branch("feature", &base).unwrap();

        std::fs::write(dir.path().join("target.txt"), "target\n").unwrap();
        let target_head = commit_all(&repo, "Target side", &author);

        repo.checkout_branch("feature").unwrap();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source_head = commit_all(&repo, "Source side", &author);
        repo.checkout_branch(&default_branch).unwrap();

        let analysis = repo
            .analyze_head_merge_no_checkout(
                &repo.resolve_branch_head("feature").unwrap(),
                &repo.resolve_branch_head(&default_branch).unwrap(),
            )
            .unwrap();

        assert_eq!(analysis.source_head, source_head);
        assert_eq!(analysis.target_head, target_head);
        assert_eq!(analysis.outcome, BranchMergeAnalysisOutcome::CleanMerge);
        assert_eq!(
            repo.resolve_branch_head(&default_branch).unwrap(),
            target_head,
            "preflight must not update target ref"
        );
    }

    #[test]
    fn test_analyze_head_merge_no_checkout_reports_conflict_paths() {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::test_author();
        let default_branch = repo
            .default_branch()
            .unwrap_or_else(|| "master".to_string());

        std::fs::write(dir.path().join("conflict.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "Initial", &author);
        repo.create_branch("feature", &base).unwrap();

        std::fs::write(dir.path().join("conflict.txt"), "target\n").unwrap();
        let target_head = commit_all(&repo, "Target side", &author);

        repo.checkout_branch("feature").unwrap();
        std::fs::write(dir.path().join("conflict.txt"), "source\n").unwrap();
        let source_head = commit_all(&repo, "Source side", &author);
        repo.checkout_branch(&default_branch).unwrap();

        let analysis = repo
            .analyze_head_merge_no_checkout(
                &repo.resolve_branch_head("feature").unwrap(),
                &repo.resolve_branch_head(&default_branch).unwrap(),
            )
            .unwrap();

        assert_eq!(analysis.source_head, source_head);
        assert_eq!(analysis.target_head, target_head);
        match analysis.outcome {
            BranchMergeAnalysisOutcome::Conflicts { paths } => {
                assert!(paths.iter().any(|path| path == "conflict.txt"));
            }
            other => panic!("expected conflicts, got {other:?}"),
        }
        assert_eq!(
            repo.resolve_branch_head(&default_branch).unwrap(),
            target_head,
            "conflict analysis must not update target ref"
        );
    }

    #[test]
    fn test_create_clean_merge_commit_from_heads_ignores_poisoned_refs() {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::test_author();
        let default_branch = repo
            .default_branch()
            .unwrap_or_else(|| "master".to_string());

        std::fs::write(dir.path().join("README.md"), "base\n").unwrap();
        let base = commit_all(&repo, "Initial", &author);
        repo.create_branch("feature", &base).unwrap();

        std::fs::write(dir.path().join("target.txt"), "target\n").unwrap();
        let target_head = commit_all(&repo, "Target side", &author);

        repo.checkout_branch("feature").unwrap();
        std::fs::write(dir.path().join("source.txt"), "source\n").unwrap();
        let source_head = commit_all(&repo, "Source side", &author);
        repo.checkout_branch(&default_branch).unwrap();

        repo.apply_branch_commit(&default_branch, &target_head, &source_head)
            .unwrap();
        repo.apply_branch_commit("feature", &source_head, &target_head)
            .unwrap();

        let merge_head = repo
            .create_clean_merge_commit_from_heads_no_checkout(
                &source_head,
                &target_head,
                "feature",
                &default_branch,
                &author,
            )
            .unwrap();

        {
            let guard = repo.repo().unwrap();
            let commit = guard
                .find_commit(git2::Oid::from_str(&merge_head).unwrap())
                .unwrap();
            assert_eq!(commit.parent_count(), 2);
            assert_eq!(commit.parent_id(0).unwrap().to_string(), target_head);
            assert_eq!(commit.parent_id(1).unwrap().to_string(), source_head);
        }
        assert_eq!(
            repo.resolve_branch_head(&default_branch).unwrap(),
            source_head,
            "exact-head merge must not read or repair the poisoned target ref"
        );
        assert_eq!(
            repo.resolve_branch_head("feature").unwrap(),
            target_head,
            "exact-head merge must not read or repair the poisoned source ref"
        );
    }

    #[test]
    fn normalize_reviewed_publication_tip_preserves_exact_tree_and_all_ancestry() {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::test_author();
        let local_branch = repo
            .default_branch()
            .unwrap_or_else(|| "master".to_string());

        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);
        repo.create_branch("authority", &base).unwrap();
        repo.create_branch("remote", &base).unwrap();

        std::fs::write(dir.path().join("candidate.txt"), "candidate\n").unwrap();
        let candidate = commit_all(&repo, "candidate", &author);

        repo.checkout_branch("authority").unwrap();
        std::fs::write(dir.path().join("authority.txt"), "authority\n").unwrap();
        let authority = commit_all(&repo, "authority", &author);

        repo.checkout_branch("remote").unwrap();
        std::fs::write(dir.path().join("remote.txt"), "remote\n").unwrap();
        let remote = commit_all(&repo, "remote", &author);
        repo.checkout_branch(&local_branch).unwrap();

        let normalized = repo
            .normalize_reviewed_publication_tip(&candidate, &authority, &remote)
            .unwrap();
        let retried = repo
            .normalize_reviewed_publication_tip(&candidate, &authority, &remote)
            .unwrap();

        assert_eq!(normalized, retried);
        assert!(repo.is_ancestor_of(&candidate, &normalized).unwrap());
        assert!(repo.is_ancestor_of(&authority, &normalized).unwrap());
        assert!(repo.is_ancestor_of(&remote, &normalized).unwrap());
        assert_eq!(
            repo.resolve_branch_head(&local_branch).unwrap(),
            candidate,
            "normalization must not move the reviewed candidate ref"
        );

        let guard = repo.repo().unwrap();
        let candidate_commit = guard
            .find_commit(git2::Oid::from_str(&candidate).unwrap())
            .unwrap();
        let normalized_commit = guard
            .find_commit(git2::Oid::from_str(&normalized).unwrap())
            .unwrap();
        assert_eq!(normalized_commit.tree_id(), candidate_commit.tree_id());
        assert_eq!(normalized_commit.parent_count(), 3);
        assert_eq!(
            normalized_commit.parent_id(0).unwrap().to_string(),
            candidate
        );
        assert_eq!(
            normalized_commit.parent_id(1).unwrap().to_string(),
            authority
        );
        assert_eq!(normalized_commit.parent_id(2).unwrap().to_string(), remote);
    }

    #[test]
    fn normalize_reviewed_publication_tip_reuses_fully_linear_candidate() {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::test_author();
        std::fs::write(dir.path().join("remote.txt"), "remote\n").unwrap();
        let remote = commit_all(&repo, "remote", &author);
        std::fs::write(dir.path().join("authority.txt"), "authority\n").unwrap();
        let authority = commit_all(&repo, "authority", &author);
        std::fs::write(dir.path().join("candidate.txt"), "candidate\n").unwrap();
        let candidate = commit_all(&repo, "candidate", &author);

        assert_eq!(
            repo.normalize_reviewed_publication_tip(&candidate, &authority, &remote)
                .unwrap(),
            candidate
        );
    }

    #[test]
    fn test_deserialization_roundtrip_diff_summary() {
        let summary = DiffSummary {
            files: vec![DiffFileSummary {
                path: "test.rs".to_string(),
                additions: 5,
                deletions: 3,
                status: DiffFileStatus::Modified,
            }],
            total_additions: 5,
            total_deletions: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: DiffSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_additions, 5);
        assert_eq!(deserialized.total_deletions, 3);
        assert_eq!(deserialized.files.len(), 1);
    }

    #[test]
    fn test_deserialization_roundtrip_tree_entry() {
        let entry = TreeEntry {
            name: "lib.rs".to_string(),
            path: "src/lib.rs".to_string(),
            entry_type: TreeEntryType::File,
            size: Some(2048),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: TreeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "lib.rs");
        assert_eq!(deserialized.path, "src/lib.rs");
        assert_eq!(deserialized.size, Some(2048));
    }

    #[test]
    fn unified_diff_buffers_reports_additions_and_deletions() {
        let old = b"line one\nline two\nline three\n";
        let new = b"line one\nline two changed\nline three\nline four\n";
        let diff = unified_diff_buffers(old, new, "foo.txt").unwrap();
        assert!(diff.contains("@@"), "has a hunk header: {diff}");
        assert!(diff.contains("+line two changed"), "added line: {diff}");
        assert!(diff.contains("-line two"), "removed line: {diff}");
        assert!(diff.contains("+line four"), "appended line: {diff}");
    }

    #[test]
    fn unified_diff_buffers_empty_for_identical_content() {
        let same = b"unchanged content\n";
        let diff = unified_diff_buffers(same, same, "foo.txt").unwrap();
        assert!(
            !diff.contains("@@"),
            "no hunks for identical buffers: {diff}"
        );
    }

    #[test]
    fn unified_diff_buffers_treats_empty_base_as_a_new_file() {
        let diff = unified_diff_buffers(b"", b"brand new line\n", "new.txt").unwrap();
        assert!(
            diff.contains("+brand new line"),
            "new file is all additions: {diff}"
        );
    }

    #[test]
    fn unified_diff_buffers_treats_empty_new_as_a_deletion() {
        // A draft deletion materializes as empty content; the diff must be
        // all-deletions, not a no-op.
        let diff = unified_diff_buffers(b"old line one\nold line two\n", b"", "gone.txt").unwrap();
        assert!(
            diff.contains("-old line one"),
            "deleted file is all deletions: {diff}"
        );
        assert!(
            diff.contains("-old line two"),
            "deleted file is all deletions: {diff}"
        );
    }

    #[test]
    fn unified_diff_buffers_does_not_error_on_binary_content() {
        // Binary buffers must not panic or error the whole draft-diff loop;
        // git2 emits a "Binary files differ" patch with no textual hunks.
        let old: &[u8] = &[0u8, 1, 2, 255, 0, 9];
        let new: &[u8] = &[0u8, 2, 2, 254, 0, 9, 7];
        let result = unified_diff_buffers(old, new, "data.bin");
        assert!(result.is_ok(), "binary diff returns Ok, not an error");
    }

    #[test]
    fn commit_messages_ahead_returns_linear_range_oldest_first() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        repo.create_branch("feat", &base).unwrap();
        repo.checkout_branch("feat").unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        commit_all(
            &repo,
            "First change\n\nDetails about the first change.",
            &author,
        );
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        let head = commit_all(&repo, "Follow-up fix", &author);

        let range = repo.commit_messages_ahead(&base, &head, 50).unwrap();
        assert_eq!(range.omitted_count, 0);
        let subjects: Vec<&str> = range.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, vec!["First change", "Follow-up fix"]);
        assert_eq!(range.commits[0].body, "Details about the first change.");
        assert_eq!(range.commits[1].body, "");
    }

    #[test]
    fn commit_messages_ahead_skips_merges_and_empty_subjects() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        repo.create_branch("feat", &base).unwrap();
        repo.checkout_branch("feat").unwrap();
        std::fs::write(dir.path().join("feat.txt"), "feat\n").unwrap();
        commit_all(&repo, "Feature work", &author);
        std::fs::write(dir.path().join("blank.txt"), "blank\n").unwrap();
        let feat = commit_all(&repo, "   \n\nbody without a subject", &author);

        repo.create_branch("side", &base).unwrap();
        repo.checkout_branch("side").unwrap();
        std::fs::write(dir.path().join("side.txt"), "side\n").unwrap();
        let side = commit_all(&repo, "Side work", &author);

        let merge_sha = repo
            .create_clean_merge_commit_from_heads_no_checkout(&feat, &side, "feat", "side", &author)
            .unwrap();

        let range = repo.commit_messages_ahead(&base, &merge_sha, 50).unwrap();
        assert_eq!(range.omitted_count, 0);
        let mut subjects: Vec<&str> = range.commits.iter().map(|c| c.subject.as_str()).collect();
        subjects.sort_unstable();
        // The merge commit and the empty-subject commit are filtered; commits
        // from both parents of the merge remain candidates.
        assert_eq!(subjects, vec!["Feature work", "Side work"]);
    }

    #[test]
    fn commit_messages_ahead_caps_after_filtering_and_counts_omitted() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        repo.create_branch("feat", &base).unwrap();
        repo.checkout_branch("feat").unwrap();
        let mut head = String::new();
        for i in 1..=4 {
            std::fs::write(dir.path().join(format!("f{i}.txt")), "x\n").unwrap();
            head = commit_all(&repo, &format!("Change {i}"), &author);
        }

        let range = repo.commit_messages_ahead(&base, &head, 2).unwrap();
        assert_eq!(range.omitted_count, 2);
        let subjects: Vec<&str> = range.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(
            subjects,
            vec!["Change 1", "Change 2"],
            "oldest usable commits are retained"
        );
    }

    /// Writes a raw commit object (arbitrary message bytes, fixed timestamps)
    /// so tests can build commits whose messages are not valid UTF-8.
    fn raw_commit(
        raw: &git2::Repository,
        tree: git2::Oid,
        parent: git2::Oid,
        timestamp: u64,
        message_bytes: &[u8],
    ) -> git2::Oid {
        let mut buf = Vec::new();
        buf.extend_from_slice(format!("tree {tree}\n").as_bytes());
        buf.extend_from_slice(format!("parent {parent}\n").as_bytes());
        buf.extend_from_slice(
            format!("author Range <r@test.example> {timestamp} +0000\n").as_bytes(),
        );
        buf.extend_from_slice(
            format!("committer Range <r@test.example> {timestamp} +0000\n\n").as_bytes(),
        );
        buf.extend_from_slice(message_bytes);
        raw.odb()
            .unwrap()
            .write(git2::ObjectType::Commit, &buf)
            .unwrap()
    }

    #[test]
    fn commit_messages_ahead_errors_on_unrelated_histories() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        // A parentless commit shares no history with `base`.
        let raw = git2::Repository::open(dir.path()).unwrap();
        let sig = git2::Signature::now("Range", "r@test.example").unwrap();
        let tree_id = raw.index().unwrap().write_tree().unwrap();
        let tree = raw.find_tree(tree_id).unwrap();
        let orphan = raw
            .commit(Some("refs/heads/orphan"), &sig, &sig, "orphan", &tree, &[])
            .unwrap();

        let result = repo.commit_messages_ahead(&base, &orphan.to_string(), 50);
        assert!(result.is_err(), "no merge base must error, not panic");
    }

    #[test]
    fn commit_messages_ahead_filters_non_utf8_subjects() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        let raw = git2::Repository::open(dir.path()).unwrap();
        let base_oid = git2::Oid::from_str(&base).unwrap();
        let tree = raw.find_commit(base_oid).unwrap().tree_id();
        let bad = raw_commit(&raw, tree, base_oid, 1_700_000_000, b"F\xFFx bad subject\n");
        let good = raw_commit(&raw, tree, bad, 1_700_000_100, b"Good subject\n");

        let range = repo
            .commit_messages_ahead(&base, &good.to_string(), 50)
            .unwrap();
        assert_eq!(range.omitted_count, 0);
        let subjects: Vec<&str> = range.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(
            subjects,
            vec!["Good subject"],
            "the non-UTF-8 commit is filtered, not fatal"
        );
    }

    #[test]
    fn commit_messages_ahead_cap_ignores_filtered_commits_between_usable_ones() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        repo.create_branch("feat", &base).unwrap();
        repo.checkout_branch("feat").unwrap();
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        commit_all(&repo, "Usable 1", &author);
        std::fs::write(dir.path().join("blank.txt"), "b\n").unwrap();
        commit_all(&repo, "   ", &author);
        std::fs::write(dir.path().join("c.txt"), "c\n").unwrap();
        commit_all(&repo, "Usable 2", &author);
        std::fs::write(dir.path().join("d.txt"), "d\n").unwrap();
        let head = commit_all(&repo, "Usable 3", &author);

        let range = repo.commit_messages_ahead(&base, &head, 2).unwrap();
        let subjects: Vec<&str> = range.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(
            subjects,
            vec!["Usable 1", "Usable 2"],
            "the cap counts only usable commits, skipping filtered ones in between"
        );
        assert_eq!(range.omitted_count, 1, "only 'Usable 3' is omitted");
    }

    #[test]
    fn commit_messages_ahead_identical_heads_is_empty_and_bad_sha_errors() {
        let (repo, dir) = GitRepo::init_temp();
        let author = author("Range");
        std::fs::write(dir.path().join("base.txt"), "base\n").unwrap();
        let base = commit_all(&repo, "base", &author);

        let range = repo.commit_messages_ahead(&base, &base, 50).unwrap();
        assert!(range.commits.is_empty());
        assert_eq!(range.omitted_count, 0);

        let err = repo.commit_messages_ahead("not-a-sha", &base, 50);
        assert!(err.is_err(), "invalid SHA must error, not panic");
    }

    /// Creates a parentless root commit and points `branch` at it, yielding a
    /// history unrelated to any other orphan branch in the same repo.
    fn commit_orphan_branch(
        repo: &GitRepo,
        branch: &str,
        files: &[(&str, &str)],
        message: &str,
    ) -> String {
        let raw = repo.repo().unwrap();
        let sig = git2::Signature::now("Orphan", "orphan@test.example").unwrap();
        let mut builder = raw.treebuilder(None).unwrap();
        for (name, content) in files {
            let blob = raw.blob(content.as_bytes()).unwrap();
            builder.insert(*name, blob, 0o100644).unwrap();
        }
        let tree = raw.find_tree(builder.write().unwrap()).unwrap();
        let oid = raw.commit(None, &sig, &sig, message, &tree, &[]).unwrap();
        raw.reference(&format!("refs/heads/{branch}"), oid, true, "orphan root")
            .unwrap();
        oid.to_string()
    }

    #[test]
    fn rebase_onto_default_still_errors_on_unrelated_histories() {
        let (repo, _dir) = GitRepo::init_temp();
        let remote = commit_orphan_branch(
            &repo,
            "remote",
            &[("remote.txt", "remote\n")],
            "remote root",
        );
        let local = commit_orphan_branch(&repo, "local", &[("local.txt", "local\n")], "local root");

        match repo.rebase_onto(&local, &remote, "local", "remote") {
            Err(G5Error::GitError { message }) => assert!(
                message.contains("No merge base"),
                "default caller must keep the No-merge-base error contract: {message}"
            ),
            other => panic!("default rebase_onto must error on unrelated histories, got {other:?}"),
        }
    }

    #[test]
    fn read_branch_oid_reports_absent_and_present_refs() {
        let (repo, dir) = GitRepo::init_temp();
        assert_eq!(repo.read_branch_oid("main").unwrap(), None);
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        repo.add_all().unwrap();
        let author = GitAuthor::from_profile(Some("T"), Some("t@example.com"));
        let head = repo.commit("seed", &author).unwrap();
        let branch = git2::Repository::open(dir.path())
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_string();
        assert_eq!(repo.read_branch_oid(&branch).unwrap(), Some(head.clone()));
        repo.create_branch("feature/x", &head).unwrap();
        assert_eq!(repo.read_branch_oid("feature/x").unwrap(), Some(head));
        assert_eq!(repo.read_branch_oid("feature/missing").unwrap(), None);
    }

    #[test]
    fn apply_branch_commit_creates_advances_replays_and_refuses_moved_refs() {
        let (repo, dir) = GitRepo::init_temp();
        let author = GitAuthor::from_profile(Some("T"), Some("t@example.com"));
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        repo.add_all().unwrap();
        let first = repo.commit("first", &author).unwrap();
        std::fs::write(dir.path().join("a.txt"), "b\n").unwrap();
        repo.add_all().unwrap();
        let second = repo.commit("second", &author).unwrap();
        let zero = "0".repeat(40);

        // Creation requires the ref to be absent.
        assert_eq!(
            repo.apply_branch_commit("ws", &zero, &first).unwrap(),
            BranchRefUpdate::Applied
        );
        assert_eq!(repo.read_branch_oid("ws").unwrap(), Some(first.clone()));
        let err = repo.apply_branch_commit("ws", &zero, &second).unwrap_err();
        assert!(matches!(err, G5Error::Conflict { .. }), "{err:?}");
        assert_eq!(repo.read_branch_oid("ws").unwrap(), Some(first.clone()));

        // A matching expectation advances the ref, non-fast-forward included.
        assert_eq!(
            repo.apply_branch_commit("ws", &first, &second).unwrap(),
            BranchRefUpdate::Applied
        );
        assert_eq!(repo.read_branch_oid("ws").unwrap(), Some(second.clone()));
        assert_eq!(
            repo.apply_branch_commit("ws", &second, &first).unwrap(),
            BranchRefUpdate::Applied
        );

        // Replaying an update whose result is already in place is idempotent.
        assert_eq!(
            repo.apply_branch_commit("ws", &second, &first).unwrap(),
            BranchRefUpdate::AlreadyApplied
        );

        // A ref that moved elsewhere wins; the stale expectation is refused
        // and the ref is left untouched.
        let err = repo
            .apply_branch_commit("ws", &second, &second)
            .unwrap_err();
        assert!(matches!(err, G5Error::Conflict { .. }), "{err:?}");
        assert_eq!(repo.read_branch_oid("ws").unwrap(), Some(first.clone()));

        // The new commit must exist.
        let err = repo
            .apply_branch_commit("ws", &first, &"f".repeat(40))
            .unwrap_err();
        assert!(matches!(err, G5Error::GitError { .. }), "{err:?}");
        assert_eq!(repo.read_branch_oid("ws").unwrap(), Some(first));
    }

    fn two_commits(repo: &GitRepo, dir: &std::path::Path) -> (String, String) {
        let author = GitAuthor::from_profile(Some("T"), Some("t@example.com"));
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        repo.add_all().unwrap();
        let first = repo.commit("first", &author).unwrap();
        std::fs::write(dir.join("a.txt"), "b\n").unwrap();
        repo.add_all().unwrap();
        let second = repo.commit("second", &author).unwrap();
        (first, second)
    }

    fn reflog_len(dir: &std::path::Path, branch: &str) -> usize {
        git2::Repository::open(dir)
            .unwrap()
            .reflog(&format!("refs/heads/{branch}"))
            .map(|reflog| reflog.len())
            .unwrap_or(0)
    }

    #[test]
    fn ref_cut_fences_unchanged_heads_and_watermarks_and_rolls_back_creation() {
        let (repo, dir) = GitRepo::init_temp();
        let (first, second) = two_commits(&repo, dir.path());
        repo.create_branch("ws", &first).unwrap();
        let cut = vec![
            RefCommit {
                repo_path: dir.path().into(),
                ref_name: "refs/heads/ws".into(),
                expected_old: Some(first.clone()),
                new_oid: Some(first.clone()),
            },
            RefCommit {
                repo_path: dir.path().into(),
                ref_name: "refs/g5-tracking/ws/baseline".into(),
                expected_old: None,
                new_oid: Some(first.clone()),
            },
        ];
        // An unchanged member still fences the synchronization acknowledgement.
        repo.apply_branch_commit("ws", &first, &second).unwrap();
        assert!(matches!(
            GitRepo::apply_ref_commits(&cut),
            Err(BranchRefCutRefusal::Stale { .. })
        ));
        assert_eq!(repo.read_sync_watermark("ws").unwrap(), None);
        repo.apply_branch_commit("ws", &second, &first).unwrap();
        assert_eq!(
            GitRepo::apply_ref_commits(&cut).unwrap(),
            vec![BranchRefUpdate::AlreadyApplied, BranchRefUpdate::Applied]
        );
        assert_eq!(
            GitRepo::apply_ref_commits(&cut).unwrap(),
            vec![BranchRefUpdate::AlreadyApplied; 2]
        );
        // A watermark changed by a different attempt refuses the whole cut.
        GitRepo::apply_ref_commits(&[GitRepo::sync_watermark_update(
            dir.path().into(),
            "ws",
            Some(first.clone()),
            Some(second.clone()),
        )])
        .unwrap();
        let mut advance = cut.clone();
        advance[0].new_oid = Some(second.clone());
        assert!(matches!(
            GitRepo::apply_ref_commits(&advance),
            Err(BranchRefCutRefusal::Stale { .. })
        ));
        assert_eq!(repo.read_branch_oid("ws").unwrap(), Some(first.clone()));
        let rollback = RefCommit {
            repo_path: dir.path().into(),
            ref_name: cut[1].ref_name.clone(),
            expected_old: Some(first),
            new_oid: None,
        };
        assert!(matches!(
            GitRepo::apply_ref_commits(std::slice::from_ref(&rollback)),
            Err(BranchRefCutRefusal::Stale { .. })
        ));
        let rollback = RefCommit {
            expected_old: Some(second),
            ..rollback
        };
        assert_eq!(
            GitRepo::apply_ref_commits(&[rollback]).unwrap(),
            vec![BranchRefUpdate::Applied]
        );
        assert_eq!(repo.read_sync_watermark("ws").unwrap(), None);
    }

    #[test]
    fn apply_branch_commits_applies_a_cut_across_repositories_or_nothing_at_all() {
        let (left, left_dir) = GitRepo::init_temp();
        let (right, right_dir) = GitRepo::init_temp();
        let (left_first, left_second) = two_commits(&left, left_dir.path());
        let (right_first, right_second) = two_commits(&right, right_dir.path());
        let zero = "0".repeat(40);
        left.create_branch("ws", &left_first).unwrap();
        left.checkout_branch_at("ws", &left_first).unwrap();
        let cut = |left_expected: &str, left_new: &str, right_expected: &str, right_new: &str| {
            vec![
                BranchRefCommit {
                    repo_path: left_dir.path().to_path_buf(),
                    branch: "ws".to_string(),
                    expected_old: left_expected.to_string(),
                    new_sha: left_new.to_string(),
                },
                BranchRefCommit {
                    repo_path: right_dir.path().to_path_buf(),
                    branch: "ws".to_string(),
                    expected_old: right_expected.to_string(),
                    new_sha: right_new.to_string(),
                },
            ]
        };

        // An advance and a creation land together.
        let outcomes =
            GitRepo::apply_branch_commits(&cut(&left_first, &left_second, &zero, &right_first))
                .unwrap();
        assert_eq!(
            outcomes,
            vec![BranchRefUpdate::Applied, BranchRefUpdate::Applied]
        );
        assert_eq!(
            left.read_branch_oid("ws").unwrap(),
            Some(left_second.clone())
        );
        assert_eq!(
            right.read_branch_oid("ws").unwrap(),
            Some(right_first.clone())
        );

        // Replaying the same cut is idempotent and writes nothing.
        let left_log = reflog_len(left_dir.path(), "ws");
        let right_log = reflog_len(right_dir.path(), "ws");
        let outcomes =
            GitRepo::apply_branch_commits(&cut(&left_first, &left_second, &zero, &right_first))
                .unwrap();
        assert_eq!(
            outcomes,
            vec![
                BranchRefUpdate::AlreadyApplied,
                BranchRefUpdate::AlreadyApplied
            ]
        );
        assert_eq!(reflog_len(left_dir.path(), "ws"), left_log);
        assert_eq!(reflog_len(right_dir.path(), "ws"), right_log);

        // One member at an unexpected head refuses the whole cut before any
        // ref is written: the matching member is not moved, not even
        // transiently (its reflog is unchanged), and the moved ref wins.
        let author = GitAuthor::from_profile(Some("T"), Some("t@example.com"));
        std::fs::write(right_dir.path().join("a.txt"), "c\n").unwrap();
        right.add_all().unwrap();
        let right_third = right.commit("third", &author).unwrap();
        assert_eq!(
            right
                .apply_branch_commit("ws", &right_first, &right_second)
                .unwrap(),
            BranchRefUpdate::Applied
        );
        let left_log = reflog_len(left_dir.path(), "ws");
        let right_log = reflog_len(right_dir.path(), "ws");
        let refusal = GitRepo::apply_branch_commits(&cut(
            &left_second,
            &left_first,
            &right_first,
            &right_third,
        ))
        .unwrap_err();
        match refusal {
            BranchRefCutRefusal::Stale { mismatches } => assert_eq!(
                mismatches,
                vec![BranchRefMismatch {
                    index: 1,
                    current: Some(right_second.clone()),
                }]
            ),
            other => panic!("a moved member must refuse the cut as stale: {other:?}"),
        }
        assert_eq!(
            left.read_branch_oid("ws").unwrap(),
            Some(left_second.clone())
        );
        assert_eq!(
            right.read_branch_oid("ws").unwrap(),
            Some(right_second.clone())
        );
        assert_eq!(reflog_len(left_dir.path(), "ws"), left_log);
        assert_eq!(reflog_len(right_dir.path(), "ws"), right_log);

        // The locks are released: the refs can be moved again afterwards.
        assert_eq!(
            left.apply_branch_commit("ws", &left_second, &left_first)
                .unwrap(),
            BranchRefUpdate::Applied
        );

        // A missing commit object refuses the cut as a git failure with
        // nothing written.
        let refusal = GitRepo::apply_branch_commits(&cut(
            &left_first,
            &"f".repeat(40),
            &right_second,
            &right_first,
        ))
        .unwrap_err();
        match refusal {
            BranchRefCutRefusal::Failed { index, written, .. } => {
                assert_eq!(index, Some(0));
                assert!(written.is_empty());
            }
            other => panic!("a missing object is a git failure: {other:?}"),
        }
        assert_eq!(right.read_branch_oid("ws").unwrap(), Some(right_second));
    }

    #[test]
    fn apply_branch_commits_refuses_a_cut_whose_ref_another_writer_holds_locked() {
        let (left, left_dir) = GitRepo::init_temp();
        let (right, right_dir) = GitRepo::init_temp();
        let (left_first, left_second) = two_commits(&left, left_dir.path());
        let (right_first, right_second) = two_commits(&right, right_dir.path());
        left.create_branch("ws", &left_first).unwrap();
        right.create_branch("ws", &right_first).unwrap();

        // Another writer holds git's lock on the right ref, the way an
        // in-flight `git update-ref` would.
        let other_writer = git2::Repository::open(right_dir.path()).unwrap();
        let mut held = other_writer.transaction().unwrap();
        held.lock_ref("refs/heads/ws").unwrap();

        let left_log = reflog_len(left_dir.path(), "ws");
        let refusal = GitRepo::apply_branch_commits(&[
            BranchRefCommit {
                repo_path: left_dir.path().to_path_buf(),
                branch: "ws".to_string(),
                expected_old: left_first.clone(),
                new_sha: left_second.clone(),
            },
            BranchRefCommit {
                repo_path: right_dir.path().to_path_buf(),
                branch: "ws".to_string(),
                expected_old: right_first.clone(),
                new_sha: right_second.clone(),
            },
        ])
        .unwrap_err();
        match refusal {
            BranchRefCutRefusal::Failed { index, written, .. } => {
                assert_eq!(index, Some(1));
                assert!(written.is_empty());
            }
            other => panic!("a locked ref is a git failure: {other:?}"),
        }
        // The left member matched but was not written: the cut is a single
        // decision.
        assert_eq!(left.read_branch_oid("ws").unwrap(), Some(left_first));
        assert_eq!(reflog_len(left_dir.path(), "ws"), left_log);
        assert_eq!(right.read_branch_oid("ws").unwrap(), Some(right_first));
        drop(held);
    }
}

/// Source Control history identity, independent of workspace vector observations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCommit {
    pub paths: Vec<String>,
    pub sha: String,
    pub summary: String,
    pub author_name: String,
    pub committed_at: i64,
    pub parents: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFilePatch {
    pub path: String,
    pub old_path: Option<String>,
    pub change: DiffFileStatus,
    pub diff: CommitFileDiff,
}

// Git omits ---/+++ for empty additions/deletions. libgit2 requires those
// headers even though the file has no hunks. Complete only that exact empty
// blob form, preserving Git's own path quoting and all other patch bytes.
fn complete_empty_file_headers(bytes: &[u8]) -> Vec<u8> {
    fn append(chunk: &[u8], output: &mut Vec<u8>) {
        output.extend_from_slice(chunk);
        let lines = chunk
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .take(4)
            .collect::<Vec<_>>();
        if lines.len() != 3 {
            return;
        }
        let added = lines[1].starts_with(b"new file mode ");
        let deleted = lines[1].starts_with(b"deleted file mode ");
        if !added && !deleted {
            return;
        }
        let Some(index) = lines[2].strip_prefix(b"index ") else {
            return;
        };
        let Some(separator) = index.windows(2).position(|pair| pair == b"..") else {
            return;
        };
        let (old, new) = (&index[..separator], &index[separator + 2..]);
        let (absent, empty) = if added { (old, new) } else { (new, old) };
        const EMPTY_BLOB: &[u8] = b"e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";
        if absent.is_empty()
            || !absent.iter().all(|b| *b == b'0')
            || empty.len() < 4
            || !EMPTY_BLOB.starts_with(empty)
        {
            return;
        }
        let Some(paths) = lines[0].strip_prefix(b"diff --git ") else {
            return;
        };
        // Both names are the same path with a/ and b/ prefixes, including
        // identical C quoting. Their equal lengths locate the separator even
        // when an unquoted path itself contains spaces or the string " b/".
        let middle = paths.len() / 2;
        if paths.len() % 2 != 1 || paths[middle] != b' ' {
            return;
        }
        let (before, after) = (&paths[..middle], &paths[middle + 1..]);
        let same_path = [
            (b"a/".as_slice(), b"b/".as_slice()),
            (b"\"a/".as_slice(), b"\"b/".as_slice()),
        ]
        .iter()
        .any(|(a, b)| {
            before
                .strip_prefix(*a)
                .zip(after.strip_prefix(*b))
                .is_some_and(|(a, b)| a == b)
        });
        if !same_path {
            return;
        }
        if !chunk.ends_with(b"\n") {
            output.push(b'\n');
        }
        output.extend_from_slice(b"--- ");
        output.extend_from_slice(if added { b"/dev/null" } else { before });
        output.extend_from_slice(b"\n+++ ");
        output.extend_from_slice(if deleted { b"/dev/null" } else { after });
        output.push(b'\n');
    }
    let mut output = Vec::with_capacity(bytes.len());
    let mut start = 0;
    let mut offset = 0;
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b"diff --git ") && offset > start {
            append(&bytes[start..offset], &mut output);
            start = offset;
        }
        offset += line.len();
    }
    append(&bytes[start..], &mut output);
    output
}

/// Decode a bounded native Git patch without opening an application filesystem.
pub fn source_control_patch(bytes: &[u8]) -> Result<Vec<SourceFilePatch>, G5Error> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let diff = git2::Diff::from_buffer(bytes)
        .or_else(|_| git2::Diff::from_buffer(&complete_empty_file_headers(bytes)))
        .map_err(crate::backend_error)?;
    if diff.deltas().len() > 1000 {
        return Err(G5Error::Conflict {
            reason: "Comparison exceeds 1,000 files; narrow the selection".into(),
        });
    }
    let mut files = Vec::new();
    for (i, delta) in diff.deltas().enumerate() {
        let summary = commit_diff_file_summary(&delta, 0, 0);
        let path = summary.canonical_path().to_owned();
        let file = if delta.flags().contains(git2::DiffFlags::BINARY)
            || delta.old_file().is_binary()
            || delta.new_file().is_binary()
        {
            CommitFileDiff {
                path: path.clone(),
                status: CommitFileDiffStatus::Binary,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            }
        } else {
            match git2::Patch::from_diff(&diff, i).map_err(crate::backend_error)? {
                Some(patch) => patch_file_diff(&patch, &path, true)?,
                None => CommitFileDiff {
                    path: path.clone(),
                    status: CommitFileDiffStatus::Binary,
                    additions: 0,
                    deletions: 0,
                    hunks: Vec::new(),
                },
            }
        };
        files.push(SourceFilePatch {
            path,
            old_path: summary.old_path,
            change: summary.status,
            diff: file,
        });
    }
    Ok(files)
}

fn source_control_files(
    repo: &git2::Repository,
    diff: &git2::Diff<'_>,
) -> Result<Vec<SourceFilePatch>, G5Error> {
    if diff.deltas().len() > 1000 {
        return Err(G5Error::Conflict {
            reason: "Comparison exceeds 1,000 files; narrow the selection".into(),
        });
    }
    let odb = repo.odb().map_err(crate::backend_error)?;
    let mut files = Vec::new();
    for (i, delta) in diff.deltas().enumerate() {
        let summary = commit_diff_file_summary(&delta, 0, 0);
        let path = summary.canonical_path().to_owned();
        let file = if source_control_blob_too_large(&odb, &delta)? {
            CommitFileDiff {
                path: path.clone(),
                status: CommitFileDiffStatus::TooLarge,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            }
        } else {
            typed_file_diff(repo, diff, i, &delta, &path, true)?
        };
        files.push(SourceFilePatch {
            path,
            old_path: summary.old_path,
            change: summary.status,
            diff: file,
        });
    }
    Ok(files)
}

fn source_control_blob_too_large(
    odb: &git2::Odb<'_>,
    delta: &git2::DiffDelta<'_>,
) -> Result<bool, G5Error> {
    for file in [delta.old_file(), delta.new_file()] {
        // Gitlinks name commits in another repository, not local blobs.
        if file.mode() != git2::FileMode::Commit
            && !file.id().is_zero()
            && odb.read_header(file.id()).map_err(crate::backend_error)?.0 > 32 * 1024 * 1024
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn source_control_find_renames(
    repo: &git2::Repository,
    diff: &mut git2::Diff<'_>,
) -> Result<(), G5Error> {
    // Similarity detection itself may load blob bytes. Admit headers first.
    let odb = repo.odb().map_err(crate::backend_error)?;
    for delta in diff.deltas() {
        if source_control_blob_too_large(&odb, &delta)? {
            // Keep add/delete identity without loading oversized similarity
            // inputs; the file projection reports TooLarge.
            return Ok(());
        }
    }
    diff.find_similar(Some(git2::DiffFindOptions::new().renames(true)))
        .map_err(crate::backend_error)
}
