//! The API's native Git context. The executable is selected once from the
//! operator's `G5_GIT_BIN` override or the startup search path, and every API
//! Git invocation (protocol, maintenance and provider network operations) uses
//! that selection with the runner's cleared API environment.

use g5_contracts::G5Error;
use g5_scm::{native::NativeGit, GitAuth};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

/// Default budget for one provider network invocation (ls-remote, object
/// download, push). Callers with a tighter contract pass their own deadline.
pub const NETWORK_DEADLINE: Duration = Duration::from_secs(120);

/// The API Git executable, resolved once per process.
pub fn api_git_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        select_git_executable(std::env::var_os("G5_GIT_BIN"), std::env::var_os("PATH"))
    })
}

/// A runner for provider network operations in the API context. `deadline`
/// bounds the whole operation, however many Git invocations it needs.
pub fn network(deadline: Duration) -> Result<NativeGit, G5Error> {
    Ok(NativeGit::api(api_git_binary(), deadline)?
        .with_deadline(std::time::Instant::now() + deadline))
}

/// The advertised tip of `branch`, or `NotFound` when the remote lacks it.
pub async fn remote_branch_tip(
    url: &str,
    branch: &str,
    auth: &GitAuth,
    deadline: Duration,
) -> Result<String, G5Error> {
    network(deadline)?
        .remote_branch_tip(url, branch, auth)
        .await?
        .ok_or_else(|| G5Error::NotFound {
            resource: format!("remote branch '{branch}'"),
        })
}

/// Every advertised ordinary branch name, sorted and deduplicated.
pub async fn remote_branch_names(
    url: &str,
    auth: &GitAuth,
    deadline: Duration,
) -> Result<Vec<String>, G5Error> {
    let mut names: Vec<String> = network(deadline)?
        .remote_branches(url, auth)
        .await?
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// Preserve the API owner's configured override and startup fallback. SCM
/// owns executable lookup; no shell or extra process participates in setup.
fn select_git_executable(
    configured: Option<std::ffi::OsString>,
    search_path: Option<std::ffi::OsString>,
) -> PathBuf {
    configured
        .map(PathBuf::from)
        .or_else(|| search_path.and_then(|path| g5_scm::native::locate_executable(&path).ok()))
        .unwrap_or_else(|| PathBuf::from("/usr/bin/git"))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn api_git_selection_preserves_override_search_and_fallback() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        for (directory, mode) in [(&first, 0o610), (&second, 0o700)] {
            let path = directory.join("git");
            std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let search = std::env::join_paths([&first, &second]).unwrap();
        let selected = super::select_git_executable(None, Some(search.clone()));
        // Root may execute either, while the ordinary API identity must skip
        // the inapplicable group bit. Prove the selected executable works.
        assert!(std::process::Command::new(&selected)
            .status()
            .unwrap()
            .success());
        assert_eq!(
            super::select_git_executable(Some("/explicit/git".into()), Some(search)),
            std::path::PathBuf::from("/explicit/git")
        );
        assert_eq!(
            super::select_git_executable(None, Some(root.path().join("missing").into_os_string())),
            std::path::PathBuf::from("/usr/bin/git")
        );
        assert_eq!(
            super::select_git_executable(None, None),
            std::path::PathBuf::from("/usr/bin/git")
        );
    }
}
