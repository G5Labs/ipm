//! Git primitives for the G5 system.
//!
//! Typed Git execution for API storage, member checkouts and host/guest protocols.
//! Native Git and libgit2 mechanics, backend handles and credential injection stay
//! here. Policy owners compose these operations into authorized workspace flows.
//!
//! # Module Organization
//!
//! - [`repo`] — [`GitRepo`] wrapper around `git2::Repository`
//! - [`auth`] — [`GitAuth`] and [`GitAuthor`] types
//! - [`safety`] — [`is_valid_ref_name()`] ref-name validation
//! - [`workspace`] — local workspace workflows (init, checkout, checkpoint) and path routing

//!
//! Backend handles and credential callbacks are private. Public operations remain
//! callable without assembling backend objects:
//!
//! ```
//! # #[cfg(feature = "libgit2")]
//! # {
//! use g5_scm::{GitAuth, GitRepo};
//! let _ = GitRepo::init;
//! let _ = GitRepo::current_branch;
//! let _ = GitRepo::publish_commit;
//! let _ = GitAuth::scm_token_for_provider;
//! # }
//! ```
//!
//! ```compile_fail
//! let _ = g5_scm::GitRepo::from_raw;
//! ```
//!
//! ```compile_fail
//! let _ = g5_scm::GitRepo::lock_inner;
//! ```
//!
//! ```compile_fail
//! let _ = g5_scm::GitAuth::configure_callbacks;
//! ```

/// Evidence of effects after an unsuccessful Git or provider operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureEffect {
    Refused,
    /// An effect was confirmed, but a later stage failed.
    Partial,
    Unknown,
}

#[cfg(feature = "providers")]
pub mod providers;

pub mod auth;
#[cfg(feature = "libgit2")]
mod comparison;
#[cfg(feature = "libgit2")]
mod history_index;
#[cfg(feature = "native")]
pub mod native;
#[cfg(feature = "libgit2")]
pub mod repo;
#[cfg(feature = "libgit2")]
mod resolution;
#[cfg(feature = "libgit2")]
pub use comparison::{ComparisonHistoryPage, PushedComparison, SourceHistoryCommit};
#[cfg(feature = "libgit2")]
pub mod safety;
pub mod scm_provider;
pub mod workspace;

#[cfg(feature = "libgit2")]
mod commit_preparation;
#[cfg(feature = "libgit2")]
pub use commit_preparation::{
    prepare_files_commit_from_exact_base, prepare_initial_empty_commit,
    prepare_initial_files_commit, prepare_tree_commit_from_exact_base, ExactBaseFilesCommit,
    ExactBaseTreeCommit, ExactTreeChange, InitialFilesCommit,
};

pub use auth::{GitAuth, GitAuthor};
#[cfg(feature = "libgit2")]
pub use repo::{
    entry_bytes, source_control_patch, text_file_diff, unified_diff_buffers,
    unified_diff_buffers_with_context, BlameAttribution, BranchCommitState, BranchMergeAnalysis,
    BranchMergeAnalysisOutcome, BranchRefCommit, BranchRefCutRefusal, BranchRefMismatch,
    BranchRefUpdate, CommitComparisonBase, CommitDiffFileSummary, CommitDiffHunk, CommitDiffLine,
    CommitDiffLineType, CommitDiffSummary, CommitFileDiff, CommitFileDiffStatus,
    CommitHistoryEntry, CommitHistoryPage, CommitRangeMessages, CommitSummary, DiffContext,
    DiffFileStatus, DiffFileSummary, DiffSummary, ForkCopyRefs, ForkRefsCreateOutcome, GitRepo,
    RangeCommitMessage, RefCommit, SourceCommit, SourceFilePatch, TreeEntry, TreeEntryType,
    TreeListLimits, TreeListing, TreePaths,
};
#[cfg(feature = "libgit2")]
pub use resolution::{
    ResolutionHistory, ResolutionPinError, ResolutionPinOutcome, ResolutionPinState,
};
#[cfg(feature = "libgit2")]
pub use safety::{hash_blob, is_valid_ref_name, object_id_is_zero};
pub use scm_provider::{ScmOriginPolicy, ScmProvider};

#[cfg(all(test, feature = "libgit2"))]
mod scm_transport_tests;

// Backend errors cannot cross into the contracts crate through a conversion
// implementation: that would make the shared contract depend on libgit2.
#[cfg(feature = "libgit2")]
fn backend_error(error: git2::Error) -> g5_contracts::error::G5Error {
    g5_contracts::error::G5Error::GitError {
        message: error.message().to_owned(),
    }
}

#[cfg(feature = "libgit2")]
#[cfg(any(test, feature = "test-helpers"))]
pub mod comparison_fixture;
