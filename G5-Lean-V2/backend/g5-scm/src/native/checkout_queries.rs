//! Bounded native observations used by CLI policy, with no command escape hatch.
use super::observations::{negative_observation, require_success};
use super::*;

impl NativeGit {
    /// Whether a literal repository path survives a HEAD-seeded capture: committed
    /// paths remain eligible even when ignored; uncommitted paths follow ignore rules.
    /// Also verify that the caller selected this checkout root, not a nested directory.
    pub fn path_is_capture_eligible_blocking(
        &self,
        repository: &Path,
        path: &Path,
    ) -> Result<bool, NativeGitError> {
        let runner = self.clone();
        let repository = repository.to_path_buf();
        let path = path.to_path_buf();
        run_blocking(
            "check capture eligibility",
            FailureEffect::Refused,
            async move {
                let operation = "check capture eligibility";
                if path.as_os_str().is_empty()
                    || path
                        .components()
                        .any(|part| !matches!(part, std::path::Component::Normal(_)))
                {
                    return Err(invalid(
                        operation,
                        "Capture path must be repository-relative",
                    ));
                }
                let root = runner.checkout_root(&repository, operation).await?;
                if root.canonicalize().map_err(|_| invalid_output(operation))?
                    != repository
                        .canonicalize()
                        .map_err(|_| invalid_output(operation))?
                {
                    return Err(invalid(
                        operation,
                        "Capture repository is not the selected Git root",
                    ));
                }
                let mut object = std::ffi::OsString::from("HEAD:");
                object.push(&path);
                let mut tracked = runner.command(Some(&repository), operation);
                tracked
                    .read_only()
                    .args(["rev-parse", "--verify", "--quiet"])
                    .arg(object);
                let tracked = tracked.execute().await?;
                if !negative_observation(&tracked, operation)? {
                    require_success(&tracked, operation)?;
                    return Ok(true);
                }
                let mut ignored = runner.command(Some(&repository), operation);
                ignored
                    .read_only()
                    .args(["check-ignore", "--quiet", "--no-index", "--"])
                    .arg(path);
                let ignored = ignored.execute().await?;
                if negative_observation(&ignored, operation)? {
                    return Ok(true);
                }
                require_success(&ignored, operation)?;
                Ok(false)
            },
        )
    }

    async fn checkout_root(
        &self,
        directory: &Path,
        operation: &'static str,
    ) -> Result<PathBuf, NativeGitError> {
        let mut command = self.command(Some(directory), operation);
        command.read_only().args(["rev-parse", "--show-toplevel"]);
        let output = command.execute().await?;
        require_success(&output, operation)?;
        Ok(PathBuf::from(output.stdout_text().trim_end_matches('\n')))
    }

    pub fn version_blocking(&self) -> Result<String, NativeGitError> {
        let runner = self.clone();
        run_blocking("read Git version", FailureEffect::Refused, async move {
            let mut command = runner.command(None, "read Git version");
            command.read_only().arg("--version");
            let output = command.execute().await?;
            require_success(&output, "read Git version")?;
            let version = output.stdout_text().trim().to_owned();
            if !version.starts_with("git version ") || version.contains('\n') {
                return Err(invalid_output("read Git version"));
            }
            Ok(version)
        })
    }

    /// Discover the remote's symbolic HEAD using the caller's transport grant.
    pub async fn default_remote_branch(
        &self,
        url: &str,
        auth: &GitAuth,
    ) -> Result<Option<String>, NativeGitError> {
        let operation = "read remote default branch";
        let result = async {
            validate_url(url, auth, operation)?;
            let transport = self.transport_destination(None, url, operation).await?;
            self.check_expansion(None, url, &transport, operation)
                .await?;
            let mut command = self.command(None, operation);
            if self.runtime_config().is_some() {
                command.member_rewrite(url, &transport);
            }
            self.authenticate(&mut command, &transport, auth)?;
            command
                .read_only()
                .args(["ls-remote", "--symref", "--", url, "HEAD"]);
            let output = command.execute().await?;
            require_success(&output, operation)?;
            let text = output.stdout_text();
            let mut branches = text.lines().filter_map(|line| {
                line.strip_prefix("ref: refs/heads/")
                    .and_then(|line| line.strip_suffix("\tHEAD"))
            });
            let branch = branches.next();
            if branches.next().is_some() {
                return Err(invalid_output(operation));
            }
            if let Some(branch) = branch {
                self.validate_branch(None, branch, operation).await?;
            }
            Ok(branch.map(str::to_owned))
        }
        .await;
        result.map_err(|error: NativeGitError| error.at_target(url, auth))
    }

    /// A checkout's uncommitted changes, untracked files included, as `git
    /// status` reports them. `g5d pr create` refuses to publish while any exist
    /// (`g5.git.pr-cli.create-save`). Truncated output is an error, never clean.
    pub async fn uncommitted_changes(
        &self,
        root: &Path,
    ) -> Result<Vec<MemberStatusEntry>, NativeGitError> {
        let operation = "read checkout status";
        let result = async {
            let mut command = self.command(Some(root), operation);
            command.read_only().args([
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ]);
            let output = command.execute().await?;
            require_success(&output, operation)?;
            if output.truncated {
                return Err(invalid_output(operation));
            }
            super::member::parse_status(&output.stdout_text())
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&root.display().to_string(), &GitAuth::None)
        })
    }

    /// The branch a checkout has checked out, or `None` when HEAD is detached.
    pub async fn checkout_branch(&self, root: &Path) -> Result<Option<String>, NativeGitError> {
        let operation = "read checkout branch";
        let result = async {
            let mut command = self.command(Some(root), operation);
            command
                .read_only()
                .args(["symbolic-ref", "--quiet", "HEAD"]);
            let output = command.execute().await?;
            if negative_observation(&output, operation)? {
                return Ok(None);
            }
            require_success(&output, operation)?;
            // The full ref: `--short` abbreviates to `heads/<branch>` when a
            // tag shares the branch's name.
            let full = output.stdout_text().trim_end_matches('\n').to_owned();
            let Some(branch) = full.strip_prefix("refs/heads/").map(str::to_owned) else {
                return Ok(None);
            };
            self.validate_branch(Some(root), &branch, operation).await?;
            Ok(Some(branch))
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&root.display().to_string(), &GitAuth::None)
        })
    }

    /// Resolve the current commit from a CLI directory (normal Git discovery).
    pub fn head_commit_blocking(&self, directory: &Path) -> Result<String, NativeGitError> {
        let runner = self.clone();
        let directory = directory.to_path_buf();
        let target = directory.display().to_string();
        run_blocking("read checkout HEAD", FailureEffect::Refused, async move {
            let operation = "read checkout HEAD";
            let mut command = runner.command(Some(&directory), operation);
            command
                .read_only()
                .args(["rev-parse", "--verify", "HEAD^{commit}"]);
            let output = command.execute().await?;
            require_success(&output, operation)?;
            oid(&output.stdout_text(), operation)
        })
        .map_err(|error| error.at_target(&target, &GitAuth::None))
    }

    /// Check one literal path against Git's ignore rules.
    pub fn path_is_ignored_blocking(
        &self,
        directory: &Path,
        path: &Path,
    ) -> Result<bool, NativeGitError> {
        let runner = self.clone();
        let directory = directory.to_path_buf();
        let target = directory.display().to_string();
        let path = path.to_path_buf();
        run_blocking("check ignored path", FailureEffect::Refused, async move {
            let operation = "check ignored path";
            let mut command = runner.command(Some(&directory), operation);
            command
                .read_only()
                .args(["check-ignore", "--quiet", "--"])
                .arg(path);
            let output = command.execute().await?;
            if negative_observation(&output, operation)? {
                return Ok(false);
            }
            require_success(&output, operation)?;
            Ok(true)
        })
        .map_err(|error| error.at_target(&target, &GitAuth::None))
    }

    /// A file is new when this valid checkout is unborn, or its current history
    /// has no entry for that literal path. Unavailable/corrupt Git is an error;
    /// the spec policy chooses its existing conservative fallback.
    pub fn file_is_new_blocking(&self, file: &Path) -> Result<bool, NativeGitError> {
        let runner = self.clone();
        let file = file.to_path_buf();
        let target = file.display().to_string();
        run_blocking("read file history", FailureEffect::Refused, async move {
            let operation = "read file history";
            let parent = file
                .parent()
                .ok_or_else(|| invalid(operation, "File requires a parent directory"))?;
            let name = file
                .file_name()
                .ok_or_else(|| invalid(operation, "File requires a name"))?;
            let repository = runner.checkout_root(parent, operation).await?;
            let mut head = runner.command(Some(parent), operation);
            head.read_only()
                .args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
            let head = head.execute().await?;
            if negative_observation(&head, operation)? {
                let mut symbolic = runner.command(Some(parent), operation);
                symbolic
                    .read_only()
                    .args(["symbolic-ref", "--quiet", "HEAD"]);
                let symbolic = symbolic.execute().await?;
                require_success(&symbolic, operation)?;
                let reference = symbolic.stdout_text();
                if runner
                    .ref_tip(&repository, reference.trim_end_matches('\n'))
                    .await?
                    .is_none()
                {
                    return Ok(true);
                }
                return Err(invalid_output(operation));
            }
            require_success(&head, operation)?;
            let head = oid(&head.stdout_text(), operation)?;
            let mut log = runner.command(Some(parent), operation);
            log.read_only()
                .args(["--literal-pathspecs", "log", "--max-count=1", "--format=%H"])
                .arg(head)
                .arg("--")
                .arg(name);
            let output = log.execute().await?;
            require_success(&output, operation)?;
            let output = output.stdout_text();
            if output.is_empty() {
                return Ok(true);
            }
            oid(&output, operation)?;
            Ok(false)
        })
        .map_err(|error| error.at_target(&target, &GitAuth::None))
    }
}

fn oid(value: &str, operation: &'static str) -> Result<String, NativeGitError> {
    let value = value.trim_end_matches('\n');
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value.to_owned())
    } else {
        Err(invalid_output(operation))
    }
}
fn invalid_output(operation: &'static str) -> NativeGitError {
    NativeGitError {
        operation,
        target: None,
        kind: FailureKind::InvalidOutput,
        effect: FailureEffect::Refused,
        detail: "Git returned an invalid checkout observation; inspect repository state".into(),
    }
}
