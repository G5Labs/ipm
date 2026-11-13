//! Workspace-scoped repository checkout backed by the shared bare-repository cache.
//!
//! One checkout is created per workspace UUID and retained across ordinary VM
//! stops. This module deliberately has no task, worker, assignment, or sticky
//! runtime concepts.

use std::path::{Component, Path, PathBuf};

use g5_contracts::types::{RepoInfo, RepoRole};
use tokio::process::Command;
use uuid::Uuid;

use super::{RepoCache, SeedBundleExport, WorkspaceError};

pub const PROJECT_DOCS_ALIAS: &str = "project_docs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDocsAliasSpec {
    pub relative_target: PathBuf,
    /// Docs-repository-local canonical folder, including its trailing separator.
    pub repo_local_folder: String,
}

/// Materializes the repository vector for a long-lived workspace VM.
pub struct WorkspaceCheckout {
    repo_cache: RepoCache,
    workspaces_root: PathBuf,
    /// A process-mode host (`g5.runtime.boot.backend.process-local`) whose
    /// data root lacks reflink copies the bare cache instead; every other
    /// host keeps reflink-or-fail-loud.
    copy_when_reflink_unsupported: bool,
}

#[derive(Debug)]
struct CheckoutRepo {
    repo_id: String,
    git_url: String,
    mount_path: PathBuf,
    branch: String,
    commit: String,
    role: RepoRole,
}

impl WorkspaceCheckout {
    #[must_use]
    pub fn new(repo_cache: RepoCache, workspaces_root: PathBuf) -> Self {
        Self {
            repo_cache,
            workspaces_root,
            copy_when_reflink_unsupported: false,
        }
    }

    /// Materialize by plain copy where the data root's filesystem refuses
    /// reflinks. Only a process-mode host opts in: it is localhost
    /// development, possibly on ext4 without reflink support. A checkout-sized
    /// copy is a cost borne by that local developer.
    #[must_use]
    pub fn with_copy_when_reflink_unsupported(mut self, enabled: bool) -> Self {
        self.copy_when_reflink_unsupported = enabled;
        self
    }

    #[must_use]
    pub fn workspace_dir(&self, workspace_id: Uuid) -> PathBuf {
        self.workspaces_root.join(workspace_id.to_string())
    }

    /// Build an exact workspace checkout using only the operation-scoped VM token.
    /// The token is consumed by the cache fetch command and is never written to
    /// the checkout, command line, or host metadata.
    pub async fn materialize_with_git_token(
        &self,
        workspace_id: Uuid,
        repos: &[RepoInfo],
        token: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        self.materialize_inner(workspace_id, Some(repos), token)
            .await
    }

    pub fn cache_root(&self) -> &Path {
        self.repo_cache.root()
    }

    pub async fn export_seed_bundle(
        &self,
        url: &str,
        token: &str,
        export: SeedBundleExport<'_>,
        process_scope: &Path,
    ) -> Result<(), WorkspaceError> {
        self.repo_cache
            .export_seed_bundle(url, token, export, process_scope)
            .await
    }

    pub async fn refresh_source_in_scope(
        &self,
        url: &str,
        token: &str,
        process_scope: &Path,
    ) -> Result<(), WorkspaceError> {
        self.repo_cache
            .refresh_in_scope(url, token, process_scope)
            .await
    }

    /// Prepare fresh host-owned metadata for a pinned block seed. No host checkout
    /// is created; the caller must already own workspace lifecycle admission.
    pub async fn prepare_metadata_only(
        &self,
        workspace_id: Uuid,
    ) -> Result<PathBuf, WorkspaceError> {
        self.materialize_inner(workspace_id, None, "").await
    }

    /// Refresh source authority without copying a worktree. These are host-owned
    /// bare caches, never guest directories; each fetch uses this invocation's token.
    pub async fn authorized_repository_stores(
        &self,
        repos: &[RepoInfo],
        token: &str,
    ) -> Result<std::collections::BTreeMap<Uuid, PathBuf>, WorkspaceError> {
        if repos.is_empty() {
            return Err(WorkspaceError::MissingRepoInventory);
        }
        let validated = repos
            .iter()
            .map(validate_repo)
            .collect::<Result<Vec<_>, _>>()?;
        validate_mount_conflicts(&validated)?;
        let mut stores = std::collections::BTreeMap::new();
        for repo in validated {
            let path = self
                .repo_cache
                .ensure_warm_with_token(&repo.git_url, token)
                .await?;
            // repo IDs were supplied by the admitted typed specification.
            let id =
                Uuid::parse_str(&repo.repo_id).map_err(|_| WorkspaceError::MissingRepoInventory)?;
            stores.insert(id, path);
        }
        Ok(stores)
    }

    async fn materialize_inner(
        &self,
        workspace_id: Uuid,
        repos: Option<&[RepoInfo]>,
        token: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        if repos.is_some_and(|repos| repos.is_empty()) {
            return Err(WorkspaceError::MissingRepoInventory);
        }
        let metadata_only = repos.is_none();
        let repos = repos.unwrap_or_default();
        let repos = repos
            .iter()
            .map(validate_repo)
            .collect::<Result<Vec<_>, _>>()?;
        validate_mount_conflicts(&repos)?;

        tokio::fs::create_dir_all(&self.workspaces_root)
            .await
            .map_err(|source| workspace_io(&self.workspaces_root, source))?;
        reject_symlink(&self.workspaces_root).await?;

        let suffix = Uuid::new_v4();
        let staging = self
            .workspaces_root
            .join(format!(".{workspace_id}.staging-{suffix}"));
        let backup = self
            .workspaces_root
            .join(format!(".{workspace_id}.previous-{suffix}"));
        remove_path_if_present(&staging).await?;
        remove_path_if_present(&backup).await?;
        tokio::fs::create_dir(&staging)
            .await
            .map_err(|source| workspace_io(&staging, source))?;
        if let Err(source) = tokio::fs::create_dir(staging.join(".g5")).await {
            // A capacity refusal here must not strand a staging inode on every
            // optional-seed fallback or capacity retry.
            let _ = remove_path_if_present(&staging).await;
            return Err(workspace_io(&staging.join(".g5"), source));
        }

        let build_result = if metadata_only {
            Ok(())
        } else {
            self.materialize_repositories(&staging, workspace_id, &repos, token)
                .await
        };
        if let Err(error) = build_result {
            let _ = remove_path_if_present(&staging).await;
            return Err(error);
        }

        let target = self.workspace_dir(workspace_id);
        let target_exists = tokio::fs::symlink_metadata(&target).await.is_ok();
        if target_exists {
            reject_symlink(&target).await?;
            tokio::fs::rename(&target, &backup)
                .await
                .map_err(|source| workspace_io(&target, source))?;
        }
        if let Err(source) = tokio::fs::rename(&staging, &target).await {
            if target_exists {
                let _ = tokio::fs::rename(&backup, &target).await;
            }
            let _ = remove_path_if_present(&staging).await;
            return Err(workspace_io(&target, source));
        }
        if target_exists {
            if let Err(error) = remove_path_if_present(&backup).await {
                tracing::warn!(path = %backup.display(), %error, "could not remove replaced workspace root");
            }
        }
        Ok(target)
    }

    async fn materialize_repositories(
        &self,
        staging: &Path,
        workspace_id: Uuid,
        repos: &[CheckoutRepo],
        token: &str,
    ) -> Result<(), WorkspaceError> {
        for repo in repos {
            let cache_dir = self
                .repo_cache
                .ensure_warm_with_token(&repo.git_url, token)
                .await?;
            let checkout = staging.join(&repo.mount_path);
            if let Some(parent) = checkout.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|source| workspace_io(parent, source))?;
            }
            tokio::fs::create_dir(&checkout)
                .await
                .map_err(|source| workspace_io(&checkout, source))?;
            reflink_bare_repo(
                &cache_dir,
                &checkout.join(".git"),
                self.copy_when_reflink_unsupported,
            )
            .await?;
            self.repo_cache
                .native_git()?
                .materialize_exact_checkout(&checkout, &repo.branch, &repo.commit)
                .await
                .map_err(|source| WorkspaceError::GitExecution { source })?;
        }
        materialize_project_docs_alias(staging, workspace_id, repos).await
    }
}

fn validate_repo(repo: &RepoInfo) -> Result<CheckoutRepo, WorkspaceError> {
    let repo_id = repo.repo_id.trim().to_string();
    let repo_name = repo.name.trim().to_string();
    let git_url = repo
        .git_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WorkspaceError::MissingRepoGitUrl {
            repo_id: repo_id.clone(),
            repo_name: repo_name.clone(),
        })?
        .to_string();
    let raw_mount =
        repo.canonical_workspace_mount()
            .ok_or_else(|| WorkspaceError::MissingRepoMountPath {
                repo_id: repo_id.clone(),
                repo_name: repo_name.clone(),
            })?;
    let mount_path = validate_mount(raw_mount, &repo_id, &repo_name)?;
    let branch = repo
        .branch_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WorkspaceError::MissingRepoBranchName {
            repo_id: repo_id.clone(),
            repo_name: repo_name.clone(),
        })?;
    validate_branch(branch)?;
    let commit = repo
        .base_commit
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(WorkspaceError::InvalidRepoBaseCommit {
            repo_id,
            repo_name,
            base_commit: commit.to_string(),
        });
    }
    Ok(CheckoutRepo {
        repo_id,
        git_url,
        mount_path,
        branch: branch.to_string(),
        commit: commit.to_string(),
        role: repo.repo_role,
    })
}

fn validate_mount(raw: &str, repo_id: &str, repo_name: &str) -> Result<PathBuf, WorkspaceError> {
    let mut path = PathBuf::new();
    if raw.contains('\0') || raw.contains('\\') {
        return Err(unsafe_mount(repo_id, repo_name, raw));
    }
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(segment) if path.as_os_str().is_empty() && segment == ".g5" => {
                return Err(unsafe_mount(repo_id, repo_name, raw));
            }
            Component::Normal(segment) => path.push(segment),
            _ => return Err(unsafe_mount(repo_id, repo_name, raw)),
        }
    }
    if path.as_os_str().is_empty() {
        return Err(unsafe_mount(repo_id, repo_name, raw));
    }
    Ok(path)
}

fn unsafe_mount(repo_id: &str, repo_name: &str, raw: &str) -> WorkspaceError {
    WorkspaceError::UnsafeRepoMountPath {
        repo_id: repo_id.to_string(),
        repo_name: repo_name.to_string(),
        mount_path: raw.to_string(),
    }
}

fn validate_mount_conflicts(repos: &[CheckoutRepo]) -> Result<(), WorkspaceError> {
    for (index, left) in repos.iter().enumerate() {
        for right in &repos[index + 1..] {
            if left.mount_path == right.mount_path
                || left.mount_path.starts_with(&right.mount_path)
                || right.mount_path.starts_with(&left.mount_path)
            {
                return Err(WorkspaceError::RepoMountPathConflict {
                    first_repo_id: left.repo_id.clone(),
                    first_mount_path: left.mount_path.clone(),
                    second_repo_id: right.repo_id.clone(),
                    second_mount_path: right.mount_path.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_branch(branch: &str) -> Result<(), WorkspaceError> {
    if branch.len() > 250
        || branch.starts_with('-')
        || branch
            .bytes()
            .any(|byte| byte == 0 || byte < 0x20 || byte == b' ')
    {
        return Err(WorkspaceError::InvalidBranchName {
            branch: branch.to_string(),
        });
    }
    Ok(())
}

/// Clone the bare cache into the checkout. Reflink-or-fail-loud, except that
/// a caller that opted in (a process-mode host) copies when the filesystem
/// refuses reflinks outright; any other copy failure still fails.
async fn reflink_bare_repo(
    source: &Path,
    destination: &Path,
    copy_when_unsupported: bool,
) -> Result<(), WorkspaceError> {
    let strict = reflink_bare_repo_strict(source, destination).await;
    match fallback_after(strict, copy_when_unsupported) {
        Fallback::Done(result) => result,
        Fallback::Copy { stderr } => {
            tracing::warn!(
                source = %source.display(),
                destination = %destination.display(),
                %stderr,
                "reflink unsupported on this filesystem; process-mode host copies the bare cache instead"
            );
            copy_bare_repo(source, destination).await
        }
    }
}

/// What follows a strict reflink attempt: its own result, or a copy, which
/// only an opted-in caller gets and only for a reflink the filesystem
/// refused; every other failure propagates unchanged.
enum Fallback {
    Done(Result<(), WorkspaceError>),
    Copy { stderr: String },
}

fn fallback_after(strict: Result<(), WorkspaceError>, copy_when_unsupported: bool) -> Fallback {
    match strict {
        Err(WorkspaceError::ReflinkUnsupported { stderr, .. }) if copy_when_unsupported => {
            Fallback::Copy { stderr }
        }
        other => Fallback::Done(other),
    }
}

/// Plain copy of the bare cache into `destination`, replacing whatever a
/// failed reflink attempt left there: `cp --reflink=always` creates the
/// destination and can leave part of it before failing, and `cp -a` into an
/// existing directory would nest the repository under it instead.
async fn copy_bare_repo(source: &Path, destination: &Path) -> Result<(), WorkspaceError> {
    match tokio::fs::remove_dir_all(destination).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(workspace_io(destination, error)),
    }
    let output = Command::new("cp")
        .args(["-a"])
        .arg(source)
        .arg(destination)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|source| WorkspaceError::Spawn {
            program: "cp".to_string(),
            source,
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(WorkspaceError::PlainCopyFailed {
            src: source.to_path_buf(),
            dst: destination.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

async fn reflink_bare_repo_strict(source: &Path, destination: &Path) -> Result<(), WorkspaceError> {
    #[cfg(target_os = "macos")]
    darwin_assert_clone_capable(source, destination)?;
    let mut command = Command::new("cp");
    #[cfg(target_os = "macos")]
    command.args(["-c", "-a"]);
    #[cfg(not(target_os = "macos"))]
    command.args(["--reflink=always", "-a"]);
    let output = command
        .arg(source)
        .arg(destination)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|source| WorkspaceError::Spawn {
            program: "cp".to_string(),
            source,
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let unsupported = stderr.contains("Operation not supported")
        || stderr.contains("Invalid cross-device link")
        || stderr.contains("FICLONE")
        || stderr.contains("reflink")
        || stderr.contains("clone is not supported");
    if unsupported {
        Err(WorkspaceError::ReflinkUnsupported {
            src: source.to_path_buf(),
            dst: destination.to_path_buf(),
            exit_code: output.status.code(),
            stderr,
        })
    } else {
        Err(WorkspaceError::ReflinkCopyFailed {
            src: source.to_path_buf(),
            dst: destination.to_path_buf(),
            exit_code: output.status.code(),
            stderr,
        })
    }
}

async fn materialize_project_docs_alias(
    root: &Path,
    workspace_id: Uuid,
    repos: &[CheckoutRepo],
) -> Result<(), WorkspaceError> {
    let docs = repos
        .iter()
        .filter(|repo| repo.role == RepoRole::Docs)
        .collect::<Vec<_>>();
    if docs.len() > 1 {
        return Err(WorkspaceError::AmbiguousProjectDocsRepo { count: docs.len() });
    }
    let Some(docs) = docs.first() else {
        return Ok(());
    };
    let relative_target = docs.mount_path.join("docs").join(workspace_id.to_string());
    let target = root.join(&relative_target);
    tokio::fs::create_dir_all(&target)
        .await
        .map_err(|source| workspace_io(&target, source))?;
    let alias = root.join(PROJECT_DOCS_ALIAS);
    #[cfg(unix)]
    tokio::fs::symlink(&relative_target, &alias)
        .await
        .map_err(|source| workspace_io(&alias, source))?;
    #[cfg(not(unix))]
    return Err(WorkspaceError::ProjectDocsAliasUnsupportedPlatform);
    Ok(())
}

fn project_docs_alias_spec(
    repos: &[RepoInfo],
    workspace_id: &str,
) -> Result<Option<ProjectDocsAliasSpec>, WorkspaceError> {
    if workspace_id.is_empty()
        || workspace_id.len() > 128
        || !workspace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(WorkspaceError::InvalidProjectDocsWorkspaceId {
            workspace_id: workspace_id.to_string(),
        });
    }
    let docs = repos
        .iter()
        .filter(|repo| repo.repo_role == RepoRole::Docs)
        .collect::<Vec<_>>();
    if docs.len() > 1 {
        return Err(WorkspaceError::AmbiguousProjectDocsRepo { count: docs.len() });
    }
    let Some(docs) = docs.first() else {
        return Ok(None);
    };
    let mount =
        docs.canonical_workspace_mount()
            .ok_or_else(|| WorkspaceError::MissingRepoMountPath {
                repo_id: docs.repo_id.clone(),
                repo_name: docs.name.clone(),
            })?;
    let mount = validate_mount(mount, &docs.repo_id, &docs.name)?;
    Ok(Some(ProjectDocsAliasSpec {
        relative_target: mount.join("docs").join(workspace_id),
        repo_local_folder: format!("docs/{workspace_id}/"),
    }))
}

/// Revalidate the physical Project Docs alias before a write-capable tool uses it.
pub async fn validate_materialized_project_docs_alias_async(
    workspace_root: &Path,
    repos: &[RepoInfo],
    workspace_id: &str,
) -> Result<Option<ProjectDocsAliasSpec>, WorkspaceError> {
    let Some(spec) = project_docs_alias_spec(repos, workspace_id)? else {
        return Ok(None);
    };
    let alias = workspace_root.join(PROJECT_DOCS_ALIAS);
    let metadata = tokio::fs::symlink_metadata(&alias)
        .await
        .map_err(|source| workspace_io(&alias, source))?;
    if !metadata.file_type().is_symlink() {
        return Err(WorkspaceError::ProjectDocsAliasCollision {
            path: alias,
            reason: "alias is not a symbolic link".to_string(),
        });
    }
    let link = tokio::fs::read_link(&alias)
        .await
        .map_err(|source| workspace_io(&alias, source))?;
    if link != spec.relative_target {
        return Err(WorkspaceError::UnsafeProjectDocsTarget {
            path: alias,
            reason: format!(
                "alias points to {}, expected {}",
                link.display(),
                spec.relative_target.display()
            ),
        });
    }
    let root = tokio::fs::canonicalize(workspace_root)
        .await
        .map_err(|source| workspace_io(workspace_root, source))?;
    let target_path = workspace_root.join(&spec.relative_target);
    let target = tokio::fs::canonicalize(&target_path)
        .await
        .map_err(|source| workspace_io(&target_path, source))?;
    if !target.starts_with(&root) {
        return Err(WorkspaceError::UnsafeProjectDocsTarget {
            path: target,
            reason: "alias target escapes the workspace checkout".to_string(),
        });
    }
    Ok(Some(spec))
}

#[doc(hidden)]
pub async fn materialize_project_docs_alias_fixture(
    workspace_root: &Path,
    repos: &[RepoInfo],
    workspace_id: &str,
) -> Result<Option<ProjectDocsAliasSpec>, WorkspaceError> {
    let Some(spec) = project_docs_alias_spec(repos, workspace_id)? else {
        return Ok(None);
    };
    let target = workspace_root.join(&spec.relative_target);
    tokio::fs::create_dir_all(&target)
        .await
        .map_err(|source| workspace_io(&target, source))?;
    let alias = workspace_root.join(PROJECT_DOCS_ALIAS);
    remove_path_if_present(&alias).await?;
    #[cfg(unix)]
    tokio::fs::symlink(&spec.relative_target, &alias)
        .await
        .map_err(|source| workspace_io(&alias, source))?;
    #[cfg(not(unix))]
    return Err(WorkspaceError::ProjectDocsAliasUnsupportedPlatform);
    Ok(Some(spec))
}

async fn reject_symlink(path: &Path) -> Result<(), WorkspaceError> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|source| workspace_io(path, source))?;
    if metadata.file_type().is_symlink() {
        return Err(WorkspaceError::UnsafeProjectDocsTarget {
            path: path.to_path_buf(),
            reason: "workspace checkout root is a symlink".to_string(),
        });
    }
    Ok(())
}

async fn remove_path_if_present(path: &Path) -> Result<(), WorkspaceError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(workspace_io(path, source)),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|source| workspace_io(path, source))
    } else {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|source| workspace_io(path, source))
    }
}

fn workspace_io(path: &Path, source: std::io::Error) -> WorkspaceError {
    WorkspaceError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// On macOS, `cp -c` silently falls back outside APFS. Refuse that substrate.
#[cfg(target_os = "macos")]
pub fn darwin_assert_clone_capable(
    source: &Path,
    destination: &Path,
) -> Result<(), WorkspaceError> {
    use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt};

    let parent = destination.parent().unwrap_or(destination);
    let path = CString::new(parent.as_os_str().as_bytes()).map_err(|_| {
        workspace_io(
            parent,
            std::io::Error::from(std::io::ErrorKind::InvalidInput),
        )
    })?;
    let mut info = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: `path` is NUL-terminated and `info` points to writable storage.
    if unsafe { libc::statfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return Err(workspace_io(parent, std::io::Error::last_os_error()));
    }
    // SAFETY: a successful statfs initializes `info`.
    let info = unsafe { info.assume_init() };
    let file_system = info
        .f_fstypename
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte as u8)
        .collect::<Vec<_>>();
    if file_system.eq_ignore_ascii_case(b"apfs") {
        Ok(())
    } else {
        Err(WorkspaceError::ReflinkUnsupported {
            src: source.to_path_buf(),
            dst: destination.to_path_buf(),
            exit_code: None,
            stderr: format!(
                "workspace root is on {}, not an APFS clone-capable volume",
                String::from_utf8_lossy(&file_system)
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A process-mode host copies the bare cache where reflinks are refused
    /// (`g5.runtime.boot.backend.process-local`); the strict path is what every
    /// other host keeps. On a reflink-capable temp dir both succeed; on one
    /// without reflink only the copy succeeds and the strict path names it.
    #[tokio::test]
    async fn process_mode_copies_the_bare_cache_where_reflink_is_unsupported() {
        let root = tempfile::tempdir().expect("temporary checkout root");
        let cache = root.path().join("cache.git");
        std::fs::create_dir(&cache).expect("create cached repository");
        std::fs::write(cache.join("HEAD"), "ref: refs/heads/main\n").expect("write cached HEAD");
        let copied = root.path().join("copied.git");
        reflink_bare_repo(&cache, &copied, true)
            .await
            .expect("the copy fallback materializes on any filesystem");
        assert_eq!(
            std::fs::read_to_string(copied.join("HEAD")).unwrap(),
            "ref: refs/heads/main\n"
        );
        let strict = root.path().join("strict.git");
        match reflink_bare_repo(&cache, &strict, false).await {
            Ok(()) => assert!(strict.join("HEAD").is_file()),
            // A refused reflink may leave a partial destination; strict mode
            // reports it and never completes it by copying.
            Err(WorkspaceError::ReflinkUnsupported { .. }) => {}
            Err(other) => panic!("unexpected strict failure: {other}"),
        }
    }

    /// Only an opted-in caller copies, and only after a refused reflink; a
    /// successful reflink, a refused one without opt-in, and any other
    /// failure come back as they were. Injected results, so the decision is
    /// exercised whatever the test filesystem supports.
    #[test]
    fn only_a_refused_reflink_with_opt_in_falls_back_to_a_copy() {
        let refused = || WorkspaceError::ReflinkUnsupported {
            src: PathBuf::from("/cache"),
            dst: PathBuf::from("/checkout/.git"),
            exit_code: Some(1),
            stderr: "cp: failed to clone: Operation not supported".into(),
        };
        let other = || WorkspaceError::ReflinkCopyFailed {
            src: PathBuf::from("/cache"),
            dst: PathBuf::from("/checkout/.git"),
            exit_code: Some(1),
            stderr: "cp: Permission denied".into(),
        };
        assert!(matches!(
            fallback_after(Err(refused()), true),
            Fallback::Copy { stderr } if stderr.contains("Operation not supported")
        ));
        assert!(matches!(
            fallback_after(Err(refused()), false),
            Fallback::Done(Err(WorkspaceError::ReflinkUnsupported { .. }))
        ));
        assert!(matches!(
            fallback_after(Err(other()), true),
            Fallback::Done(Err(WorkspaceError::ReflinkCopyFailed { .. }))
        ));
        assert!(matches!(
            fallback_after(Ok(()), true),
            Fallback::Done(Ok(()))
        ));
    }

    /// The copy replaces what a failed reflink attempt left behind: a plain
    /// `cp -a` into that directory would nest the repository under it.
    #[tokio::test]
    async fn the_copy_fallback_replaces_a_partial_destination() {
        let root = tempfile::tempdir().expect("temporary checkout root");
        let cache = root.path().join("cache.git");
        std::fs::create_dir(&cache).expect("create cached repository");
        std::fs::write(cache.join("HEAD"), "ref: refs/heads/main\n").expect("write cached HEAD");
        let destination = root.path().join("checkout").join(".git");
        std::fs::create_dir_all(&destination).expect("partial destination");
        std::fs::write(destination.join("HEAD"), "").expect("empty HEAD left by cp");
        copy_bare_repo(&cache, &destination)
            .await
            .expect("copy replaces the partial destination");
        assert_eq!(
            std::fs::read_to_string(destination.join("HEAD")).unwrap(),
            "ref: refs/heads/main\n"
        );
        assert!(
            !destination.join("cache.git").exists(),
            "the repository must not be nested under the partial destination"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_clone_check_accepts_apfs_checkout_path() {
        let checkout = std::env::temp_dir().join("g5-clone-check");
        darwin_assert_clone_capable(Path::new("/unused/bare-repo"), &checkout)
            .expect("APFS checkout path must be accepted even when diskutil cannot resolve it");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn darwin_reflink_materializes_repository_on_apfs() {
        let root = tempfile::tempdir().expect("temporary checkout root");
        let cache = root.path().join("cache.git");
        std::fs::create_dir(&cache).expect("create cached repository");
        std::fs::write(cache.join("HEAD"), "ref: refs/heads/main\n").expect("write cached HEAD");
        let checkout = root.path().join("checkout");
        std::fs::create_dir(&checkout).expect("create checkout");

        let destination = checkout.join(".git");
        reflink_bare_repo(&cache, &destination, false)
            .await
            .expect("materialize cached repository into workspace checkout");
        assert_eq!(
            std::fs::read_to_string(destination.join("HEAD")).unwrap(),
            "ref: refs/heads/main\n"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_clone_check_rejects_non_apfs_checkout_path() {
        let checkout = Path::new("/dev/g5-clone-check");
        assert!(matches!(
            darwin_assert_clone_capable(Path::new("/unused/bare-repo"), checkout),
            Err(WorkspaceError::ReflinkUnsupported { .. })
        ));
    }

    fn repo(mount: &str) -> RepoInfo {
        RepoInfo {
            repo_id: "repo-1".to_string(),
            name: "repo".to_string(),
            root_path: Some(mount.to_string()),
            root_paths: vec![mount.to_string()],
            repo_role: RepoRole::Code,
            git_url: Some("https://example.com/repo.git".to_string()),
            branch_name: Some("workspace/main".to_string()),
            base_commit: Some("a".repeat(40)),
        }
    }

    #[test]
    fn rejects_reserved_and_traversing_mounts() {
        for mount in [".g5/repo", "../repo", "/repo", "repo\\nested"] {
            assert!(validate_repo(&repo(mount)).is_err(), "accepted {mount}");
        }
    }

    #[test]
    fn rejects_nested_repository_mounts() {
        let repos = vec![
            validate_repo(&repo("code")).unwrap(),
            validate_repo(&repo("code/lib")).unwrap(),
        ];
        assert!(matches!(
            validate_mount_conflicts(&repos),
            Err(WorkspaceError::RepoMountPathConflict { .. })
        ));
    }

    #[test]
    fn rejects_branch_argument_injection() {
        let mut malicious = repo("code");
        malicious.branch_name = Some("--orphan attacker".to_string());
        assert!(matches!(
            validate_repo(&malicious),
            Err(WorkspaceError::InvalidBranchName { .. })
        ));
    }

    #[test]
    fn project_docs_alias_keeps_a_separated_repo_local_folder() {
        let mut docs = repo("docs");
        docs.repo_role = RepoRole::Docs;
        let spec = project_docs_alias_spec(&[docs], "workspace-1")
            .unwrap()
            .expect("docs repository enables the project-docs alias");

        assert_eq!(spec.relative_target, PathBuf::from("docs/docs/workspace-1"));
        assert_eq!(spec.repo_local_folder, "docs/workspace-1/");
    }
}
