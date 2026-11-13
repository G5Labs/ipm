//! Workspace checkout and repository-cache support for `g5d host`.
//!
//! See `g5/g5d/workspace` ontology for the full behavioral contract and
//! `aux_files/future_implementation/microvm_completion_plan.md` rev 7 §5.4
//! for the architectural context.

use std::path::PathBuf;

use g5_contracts::types::RepoInfo;
use g5_scm::workspace::{resolve_path_owner, WorkspaceOverlayRepoEntry};

pub mod error;
pub mod repo_cache;
pub mod restore;

pub use error::WorkspaceError;
pub use repo_cache::{parse_http_allow_hosts, CacheKey, RepoCache, SeedBundleExport};
pub use restore::WorkspaceCheckout;

/// A workspace-relative path decoded to its owning mounted repository through
/// assignment metadata only.
pub struct RepoOwnedPath<'a> {
    pub repo: &'a RepoInfo,
    pub repo_relative_path: String,
}

struct OverlayAliasEntry<'a> {
    repo: &'a RepoInfo,
    preserve_requested_path: bool,
    entry: WorkspaceOverlayRepoEntry,
}

fn canonical_repo_mount_root(repo: &RepoInfo) -> Option<String> {
    repo.root_path
        .as_deref()
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(|root| root.trim_matches('/').to_string())
        .or_else(|| {
            repo.root_paths
                .iter()
                .map(|root| root.trim())
                .find(|root| !root.is_empty())
                .map(|root| root.trim_matches('/').to_string())
        })
}

fn repo_root_aliases(repo: &RepoInfo) -> Vec<(String, bool)> {
    let canonical_mount_root = canonical_repo_mount_root(repo);
    let mut aliases: Vec<(String, bool)> = repo
        .root_paths
        .iter()
        .map(|root| {
            let root = root.trim_matches('/').to_string();
            let preserve_requested_path = canonical_mount_root
                .as_deref()
                .is_some_and(|canonical| !root.is_empty() && root != canonical);
            (root, preserve_requested_path)
        })
        .collect();
    if aliases.is_empty() {
        if let Some(root) = repo.root_path.as_deref() {
            aliases.push((root.trim_matches('/').to_string(), false));
        }
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

fn repo_workspace_route_aliases(repo: &RepoInfo) -> Vec<(String, bool)> {
    let mut aliases = repo_root_aliases(repo);
    let repo_name = repo.name.trim().trim_matches('/');
    if !repo_name.is_empty() && !aliases.iter().any(|(alias, _)| alias == repo_name) {
        aliases.push((repo_name.to_string(), false));
    }
    aliases.sort();
    aliases.dedup_by(|left, right| left.0 == right.0);
    aliases
}

fn workspace_overlay_alias_entries(repos: &[RepoInfo]) -> Vec<OverlayAliasEntry<'_>> {
    repos
        .iter()
        .enumerate()
        .flat_map(|(idx, repo)| {
            let repo_id = uuid::Uuid::from_u128(idx as u128 + 1);
            repo_workspace_route_aliases(repo).into_iter().map(
                move |(root_path, preserve_requested_path)| OverlayAliasEntry {
                    repo,
                    preserve_requested_path,
                    entry: WorkspaceOverlayRepoEntry { repo_id, root_path },
                },
            )
        })
        .collect()
}

/// Resolve a workspace-relative path without probing the filesystem.
#[must_use]
pub fn resolve_repo_owned_path<'a>(
    repos: &'a [RepoInfo],
    workspace_relative_path: &str,
) -> Option<RepoOwnedPath<'a>> {
    if repos.is_empty() || std::path::Path::new(workspace_relative_path).is_absolute() {
        return None;
    }
    let aliases = workspace_overlay_alias_entries(repos);
    let entries: Vec<WorkspaceOverlayRepoEntry> =
        aliases.iter().map(|alias| alias.entry.clone()).collect();
    let resolved = resolve_path_owner(&entries, workspace_relative_path).ok()?;
    let alias = aliases
        .iter()
        .find(|alias| alias.entry == *resolved.owner)?;
    let repo_relative_path = if alias.preserve_requested_path {
        workspace_relative_path.trim_matches('/').to_string()
    } else {
        resolved.repo_relative_path
    };
    Some(RepoOwnedPath {
        repo: alias.repo,
        repo_relative_path,
    })
}

/// Root for workspace VM directories under the signed host image
/// layout (rev-7 §5.4 + the Packer template's `/var/g5d-data/` mkdir).
pub const DEFAULT_WORKSPACES_ROOT: &str = "/var/g5d-data/lib/workspaces";

/// Root for warm bare-repo cache directories under the signed worker image
/// layout (rev-7 §5.4 + the Packer template's `/var/g5d-data/` mkdir).
pub const DEFAULT_REPO_CACHE_ROOT: &str = "/var/g5d-data/cache/repos";

/// Env var operators set to override the workspace root used by
/// both the supervisor (production materializer) and the `g5d doctor`
/// reflink probe. Single source of truth so the probe always tests the
/// directory the supervisor will actually write to (codex review B1 of
/// PR-rev7-6).
pub const ENV_WORKSPACES_ROOT: &str = "G5D_HOST_WORKSPACES_ROOT";

/// Env var operators set to override the warm bare-repo cache root used by
/// both the supervisor and the doctor probe.
pub const ENV_REPO_CACHE_ROOT: &str = "G5D_HOST_CACHE_ROOT";

/// Resolve the workspace root from env or fall back to
/// `DEFAULT_WORKSPACES_ROOT`. Empty values are ignored so a stray
/// `G5D_HOST_WORKSPACES_ROOT=` does not coerce to a useless `""` path.
pub fn resolved_workspaces_root() -> PathBuf {
    std::env::var(ENV_WORKSPACES_ROOT)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WORKSPACES_ROOT))
}

/// Resolve the bare-repo cache root from env or fall back to
/// `DEFAULT_REPO_CACHE_ROOT`. Same emptiness-filter as
/// `resolved_workspaces_root`.
pub fn resolved_repo_cache_root() -> PathBuf {
    std::env::var(ENV_REPO_CACHE_ROOT)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPO_CACHE_ROOT))
}
