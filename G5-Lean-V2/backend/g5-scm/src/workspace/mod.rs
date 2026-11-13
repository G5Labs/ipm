//! Local workspace Git workflows shared by `g5d` and `g5-api`.
//!
//! Composes local `GitRepo` primitives (initialize, checkpoint, branch
//! checkout) and workspace path ownership. Operations that contact a remote
//! use the native runner in `crate::native` directly.
//!
//! # Module Organization
//!
//! - [`branch`] — current branch and checkout
//! - [`clone`] — repository initialization
//! - [`commit`] — checkpoint commits
//! - [`paths`] — backend-free workspace path routing, built without libgit2

#[cfg(feature = "libgit2")]
pub mod branch;
#[cfg(feature = "libgit2")]
pub mod clone;
#[cfg(feature = "libgit2")]
pub mod commit;
pub mod paths;

#[cfg(feature = "libgit2")]
pub use branch::{checkout_branch, current_branch};
#[cfg(feature = "libgit2")]
pub use clone::init;
#[cfg(feature = "libgit2")]
pub use commit::{checkpoint, CheckpointOutcome};
pub use paths::{resolve_path_owner, ResolvedWorkspacePath, WorkspaceOverlayRepoEntry};
