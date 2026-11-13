//! Host materialization mechanics. Admission, cache locks and staging belong to callers.
use super::{invalid, validate_url, FailureEffect, NativeGit, NativeGitError};
use crate::GitAuth;
use std::path::Path;

impl NativeGit {
    /// Read-only preflight so a caller can validate every member before changing one.
    pub fn validate_bootstrap_checkout(
        &self,
        repository: &Path,
        oid: &str,
        canonical_url: &str,
    ) -> Result<(), NativeGitError> {
        let operation = "prepare bootstrap checkout";
        super::observations::check_oid(oid, operation)?;
        validate_url(canonical_url, &GitAuth::None, operation)?;
        let workspace_root = Path::new("/work");
        #[cfg(test)]
        let workspace_root = self
            .bootstrap_test_root
            .as_deref()
            .unwrap_or(workspace_root);
        if !matches!(self.context, super::Context::Bootstrap(_))
            || !repository.is_absolute()
            || !repository.starts_with(workspace_root)
            || repository.components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )
            })
        {
            return Err(invalid(
                operation,
                "Requires a fresh admitted guest repository",
            ));
        }
        let mut path = std::path::PathBuf::from("/");
        for part in repository.components().filter_map(|c| {
            if let std::path::Component::Normal(p) = c {
                Some(p)
            } else {
                None
            }
        }) {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_dir() => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                _ => {
                    return Err(invalid(
                        operation,
                        "Repository ancestor is not a real directory",
                    ))
                }
            }
        }
        let dotgit = repository.join(".git");
        if std::fs::symlink_metadata(&dotgit).is_ok()
            && (!std::fs::symlink_metadata(&dotgit).is_ok_and(|m| m.is_dir())
                || std::fs::symlink_metadata(dotgit.join("objects/info/alternates")).is_ok())
        {
            return Err(invalid(
                operation,
                "Seed Git database must be self-contained",
            ));
        }
        Ok(())
    }

    /// Initialize additional members or sanitize pristine seeded metadata before
    /// restoring an admitted receipt. Never use on a previously running workspace.
    /// Persist the admitted member URL, never the bootstrap-only transport address.
    pub async fn prepare_bootstrap_checkout(
        &self,
        repository: &Path,
        oid: &str,
        canonical_url: &str,
    ) -> Result<(), NativeGitError> {
        self.validate_bootstrap_checkout(repository, oid, canonical_url)?;
        let operation = "prepare bootstrap checkout";
        std::fs::create_dir_all(repository)
            .map_err(|_| invalid(operation, "Cannot create repository directory"))?;
        let dotgit = repository.join(".git");
        if dotgit.is_dir() {
            let config = dotgit.join("config");
            if std::fs::symlink_metadata(&config).is_ok() {
                std::fs::remove_file(&config)
                    .map_err(|_| invalid(operation, "Cannot replace seed configuration"))?;
            }
            let content = if oid.len() == 64 {
                "[core]\nrepositoryformatversion = 1\nbare = false\n[extensions]\nobjectformat = sha256\n"
            } else {
                "[core]\nrepositoryformatversion = 0\nbare = false\n"
            };
            std::fs::write(config, content)
                .map_err(|_| invalid(operation, "Cannot write bootstrap configuration"))?;
        } else {
            let mut init = self.command(None, operation);
            init.args(["init", "--quiet", "--template="])
                .arg(if oid.len() == 64 {
                    "--object-format=sha256"
                } else {
                    "--object-format=sha1"
                })
                .arg("--")
                .arg(repository);
            init.execute().await?.success(operation, &GitAuth::None)?;
        }
        // Both sanitized seeds and additional members lack a remote. Fetching
        // by URL restores refs but does not create origin or its fetch refspec.
        let mut command = self.command(Some(repository), operation);
        command.args(["remote", "add", "--", "origin", canonical_url]);
        command.execute().await?.success(operation, &GitAuth::None)
    }

    /// Build a disposable detached checkout from exactly one committed local
    /// bundle cut. Only an empty caller-owned directory is accepted; no remote
    /// URL, credential, branch policy or existing workspace state is involved.
    pub fn prepare_bundle_checkout_blocking(
        &self,
        repository: &Path,
        bundle: &Path,
        commit: &str,
    ) -> Result<(), NativeGitError> {
        let (runner, repository, bundle, commit) = (
            self.clone(),
            repository.to_path_buf(),
            bundle.to_path_buf(),
            commit.to_owned(),
        );
        super::run_blocking(
            "prepare bundle checkout",
            FailureEffect::Unknown,
            async move {
                let operation = "prepare bundle checkout";
                super::observations::check_oid(&commit, operation)?;
                if !matches!(&runner.context, super::Context::Host(_))
                    || !std::fs::symlink_metadata(&repository).is_ok_and(|m| m.is_dir())
                    || std::fs::read_dir(&repository)
                        .map_err(|_| invalid(operation, "Cannot inspect destination"))?
                        .next()
                        .is_some()
                    || !bundle.is_absolute()
                    || !std::fs::symlink_metadata(&bundle).is_ok_and(|m| m.is_file())
                {
                    return Err(invalid(
                        operation,
                        "Requires an empty owned directory and a local regular bundle",
                    ));
                }
                let mut init = runner.command(None, operation);
                init.args(["init", "--quiet", "--template="])
                    .arg(if commit.len() == 64 {
                        "--object-format=sha256"
                    } else {
                        "--object-format=sha1"
                    })
                    .arg("--")
                    .arg(&repository);
                init.execute().await?.success(operation, &GitAuth::None)?;
                let result = async {
                    let mut fetch = runner.command(Some(&repository), operation);
                    fetch.config("protocol.allow", "never");
                    fetch.config("protocol.file.allow", "always");
                    fetch
                        .args([
                            "fetch",
                            "--quiet",
                            "--no-tags",
                            "--no-write-fetch-head",
                            "--",
                        ])
                        .arg(&bundle)
                        .arg(&commit);
                    fetch.execute().await?.success(operation, &GitAuth::None)?;
                    let mut checkout = runner.command(Some(&repository), operation);
                    checkout.args(["checkout", "--quiet", "--detach", &commit, "--"]);
                    checkout
                        .execute()
                        .await?
                        .success(operation, &GitAuth::None)?;
                    let mut verify = runner.command(Some(&repository), operation);
                    verify.args(["fsck", "--full", "--no-reflogs"]).read_only();
                    verify.execute().await?.success(operation, &GitAuth::None)
                }
                .await;
                result.map_err(|mut error: NativeGitError| {
                    error.effect = FailureEffect::Partial;
                    error.at_target(&repository.display().to_string(), &GitAuth::None)
                })
            },
        )
    }

    /// Populate a host-owned bare cache, preserving all source branches. This
    /// is a materialization optimization, not a branch confidentiality boundary.
    pub async fn clone_bare_cache(
        &self,
        url: &str,
        destination: &Path,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let operation = "clone bare cache";
        let result = async {
            validate_url(url, auth, operation)?;
            self.check_expansion(None, url, url, operation).await?;
            let mut command = self.command(None, operation);
            self.authenticate(&mut command, url, auth)?;
            command
                .args(["clone", "--bare", "--quiet", "--"])
                .arg(url)
                .arg(destination);
            command.execute().await?.success(operation, auth)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&format!("{url} -> {}", destination.display()), auth)
        })
    }

    /// Replace/prune the host cache's local branch namespace from its admitted
    /// origin. Bare clones have no default fetch refspec, so supply it here.
    pub async fn refresh_bare_cache(
        &self,
        repository: &Path,
        expected_url: &str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let operation = "refresh bare cache";
        let result = async {
            validate_url(expected_url, auth, operation)?;
            // Cache identity has historically trimmed surrounding whitespace and
            // one trailing slash. Existing caches retain the original spelling.
            // Read the expanded origin, then bind credentials to that admitted URL.
            let mut origin = self.command(Some(repository), operation);
            origin.args(["remote", "get-url", "origin"]).read_only();
            let output = origin.execute().await?;
            super::observations::require_success(&output, operation)?;
            let transport = output.stdout_text().trim_end().to_owned();
            let identity = |url: &str| {
                let url = url.trim();
                url.strip_suffix('/').unwrap_or(url).to_owned()
            };
            if identity(&transport) != identity(expected_url) {
                return Err(super::destination_error(operation));
            }
            validate_url(&transport, auth, operation)?;
            self.check_expansion(Some(repository), &transport, &transport, operation)
                .await?;
            let mut command = self.command(Some(repository), operation);
            self.authenticate(&mut command, &transport, auth)?;
            command.args([
                "fetch",
                "--prune",
                "--quiet",
                "--recurse-submodules=no",
                "--",
                "origin",
                "+refs/heads/*:refs/heads/*",
            ]);
            command.execute().await?.success(operation, auth)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(
                &format!("{expected_url} -> {} refs/heads/*", repository.display()),
                auth,
            )
        })
    }

    /// Export a committed branch cut from the caller's locked host bare cache.
    /// The caller holds the cache writer lock through this operation.
    pub async fn export_seed_bundle(
        &self,
        repository: &Path,
        branch: &str,
        commit: &str,
        output: &Path,
        max_bytes: u64,
    ) -> Result<(), NativeGitError> {
        let operation = "export repository seed";
        if !matches!(self.context, super::Context::Host(_))
            || !output.is_absolute()
            || output.exists()
            || max_bytes == 0
            || !cfg!(unix)
        {
            return Err(invalid(
                operation,
                "Requires a host cache and fresh absolute bundle path",
            ));
        }
        super::observations::check_oid(commit, operation)?;
        self.validate_branch(Some(repository), branch, operation)
            .await?;
        let reference = format!("refs/heads/{branch}");
        if self.ref_tip(repository, &reference).await?.as_deref() != Some(commit) {
            return Err(invalid(
                operation,
                "Source branch moved; defer seed refresh",
            ));
        }
        let mut command = self.command(Some(repository), operation);
        command.limit_file_size(max_bytes);
        command
            .args(["bundle", "create"])
            .arg(output)
            .arg(&reference);
        command
            .execute()
            .await?
            .success(operation, &GitAuth::None)?;
        Ok(())
    }

    /// Turn the caller's fresh bare-cache copy at `<checkout>/.git` into a
    /// worktree pinned to an exact commit. Never consult a newer branch tip.
    pub async fn materialize_exact_checkout(
        &self,
        checkout: &Path,
        branch: &str,
        commit: &str,
    ) -> Result<(), NativeGitError> {
        let operation = "materialize exact checkout";
        let result = async {
            if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(invalid(
                    operation,
                    "Materialization requires a full commit OID",
                ));
            }
            self.validate_branch(Some(checkout), branch, operation)
                .await?;
            let mut probe = self.command(Some(checkout), operation);
            probe.args(["cat-file", "-t", commit]).read_only();
            let output = probe.execute().await?;
            super::observations::require_success(&output, operation)?;
            if output.stdout_text().trim() != "commit" {
                return Err(invalid(operation, "Materialization target is not a commit"));
            }
            let mut confirmed = false;
            for args in [
                vec!["config", "core.bare", "false"],
                vec!["config", "core.worktree", ".."],
                vec!["checkout", "--quiet", "-B", branch, commit, "--"],
            ] {
                let mut command = self.command(Some(checkout), operation);
                for arg in args {
                    command.arg(arg);
                }
                let step =
                    async { command.execute().await?.success(operation, &GitAuth::None) }.await;
                if let Err(mut error) = step {
                    if confirmed {
                        error.effect = FailureEffect::Partial;
                        error.detail =
                            format!("Checkout configuration was written; {}", error.detail);
                    }
                    return Err(error);
                }
                confirmed = true;
            }
            Ok(())
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(
                &format!("{} refs/heads/{branch} at {commit}", checkout.display()),
                &GitAuth::None,
            )
        })
    }
}
