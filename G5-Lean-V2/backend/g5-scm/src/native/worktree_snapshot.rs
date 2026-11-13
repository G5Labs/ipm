//! Read-only evidence of a checkout's files under one directory, for a chat
//! turn's ontology start (`g5.chat.ontology-card`). Callers name a directory,
//! a file-name suffix and full object IDs; they never supply Git arguments,
//! pathspec magic or configuration. Nothing here writes the index, a ref or
//! `FETCH_HEAD`; the one fetch adds only objects.
use super::observations::{check_oid, full_oid, negative_observation, require_success};
use super::{invalid, NativeGit, NativeGitError};
use crate::GitAuth;
use std::collections::BTreeMap;
use std::path::Path;

/// Files under `directory` (repository-relative, any depth) whose names end
/// with `suffix`, such as `.g5/ontology` and `.g5.md`.
#[derive(Debug, Clone, Copy)]
pub struct FileSelection<'a> {
    pub directory: &'a str,
    pub suffix: &'a str,
}

impl FileSelection<'_> {
    fn pathspec(&self, operation: &'static str) -> Result<String, NativeGitError> {
        let literal = |value: &str| {
            !value.is_empty()
                && !value
                    .chars()
                    .any(|c| matches!(c, '*' | '?' | '[' | ']' | '\\' | ':' | '\0' | '\n'))
        };
        if !literal(self.directory)
            || !literal(self.suffix)
            || self.suffix.contains('/')
            || Path::new(self.directory)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(invalid(
                operation,
                "Expected a confined directory and a literal file suffix",
            ));
        }
        Ok(format!(":(glob){}/**/*{}", self.directory, self.suffix))
    }

    fn selects(&self, path: &str) -> bool {
        path.strip_prefix(self.directory)
            .is_some_and(|rest| rest.starts_with('/'))
            && path.ends_with(self.suffix)
    }
}

impl NativeGit {
    /// The newest commit the remote already has that HEAD contains: HEAD
    /// itself when its branch is pushed, else its merge base with the branch's
    /// remote-tracking ref, else HEAD. `None` for an unborn HEAD.
    pub async fn pushed_base(
        &self,
        repository: &Path,
        remote: &str,
    ) -> Result<Option<String>, NativeGitError> {
        let operation = "read pushed base";
        super::validate_remote(remote, operation)?;
        let mut head = self.observation(repository, operation);
        head.args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]);
        let head = head.execute().await?;
        if negative_observation(&head, operation)? {
            return Ok(None);
        }
        require_success(&head, operation)?;
        let head = head.stdout_text().trim_end().to_owned();
        if !full_oid(&head) {
            return Err(invalid(operation, "Git returned an invalid HEAD"));
        }
        let mut branch = self.observation(repository, operation);
        branch.args(["symbolic-ref", "--quiet", "HEAD"]);
        let branch = branch.execute().await?;
        if negative_observation(&branch, operation)? {
            return Ok(Some(head)); // Detached.
        }
        require_success(&branch, operation)?;
        let branch = branch.stdout_text();
        let Some(name) = branch.trim_end().strip_prefix("refs/heads/") else {
            return Ok(Some(head));
        };
        let tracking = format!("refs/remotes/{remote}/{name}");
        let Some(pushed) = self.ref_tip(repository, &tracking).await? else {
            return Ok(Some(head));
        };
        let mut base = self.observation(repository, operation);
        base.args(["merge-base", &head, &pushed]);
        let base = base.execute().await?;
        if negative_observation(&base, operation)? {
            return Ok(Some(head)); // No shared history.
        }
        require_success(&base, operation)?;
        let base = base.stdout_text().trim_end().to_owned();
        if !full_oid(&base) {
            return Err(invalid(operation, "Git returned an invalid merge base"));
        }
        Ok(Some(base))
    }

    /// A zero-context patch of the selected tracked files from `base` to the
    /// working tree: staged, unstaged and deleted changes, without renames.
    /// Uses `diff-index`, which never refreshes or writes the index. `None`
    /// when the patch exceeds `limit` bytes.
    pub async fn worktree_patch(
        &self,
        repository: &Path,
        base: &str,
        selection: FileSelection<'_>,
        limit: usize,
    ) -> Result<Option<Vec<u8>>, NativeGitError> {
        let operation = "read worktree patch";
        check_oid(base, operation)?;
        let pathspec = selection.pathspec(operation)?;
        let mut command = self.observation(repository, operation);
        command.config("core.quotePath", "false");
        command.capture_limit(limit.saturating_add(1));
        command.args([
            "--no-optional-locks",
            "diff-index",
            "-p",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--no-relative",
            "--text",
            "--full-index",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            base,
            "--",
            &pathspec,
        ]);
        let output = command.execute().await?;
        if output.completed_successfully() && output.truncated {
            return Ok(None);
        }
        require_success(&output, operation)?;
        Ok(Some(output.into_protocol_bytes()))
    }

    /// The selected files Git sees in the working tree: untracked files that
    /// ignore rules do not exclude, and with `tracked` also the index's files.
    pub async fn worktree_files(
        &self,
        repository: &Path,
        selection: FileSelection<'_>,
        tracked: bool,
    ) -> Result<Vec<String>, NativeGitError> {
        let operation = "list worktree files";
        let pathspec = selection.pathspec(operation)?;
        let mut command = self.observation(repository, operation);
        command.capture_limit(4 * 1024 * 1024);
        command.args(["--no-optional-locks", "ls-files", "-z"]);
        if tracked {
            command.args(["--cached"]);
        }
        command.args(["--others", "--exclude-standard", "--", &pathspec]);
        let output = command.execute().await?;
        require_success(&output, operation)?;
        let mut paths: Vec<String> = output
            .protocol_bytes()
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| String::from_utf8_lossy(path).into_owned())
            .filter(|path| selection.selects(path))
            .collect();
        paths.dedup();
        Ok(paths)
    }

    /// The selected regular files of `commit`, with their contents. `None`
    /// when their total size exceeds `limit` bytes.
    pub async fn commit_files(
        &self,
        repository: &Path,
        commit: &str,
        selection: FileSelection<'_>,
        limit: usize,
    ) -> Result<Option<BTreeMap<String, Vec<u8>>>, NativeGitError> {
        let operation = "read commit files";
        check_oid(commit, operation)?;
        selection.pathspec(operation)?;
        let mut listing = self.observation(repository, operation);
        listing.capture_limit(4 * 1024 * 1024);
        listing.args([
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            commit,
            "--",
            selection.directory,
        ]);
        let listing = listing.execute().await?;
        require_success(&listing, operation)?;
        let mut paths = Vec::new();
        for entry in listing.protocol_bytes().split(|byte| *byte == 0) {
            if entry.is_empty() {
                continue;
            }
            let entry = String::from_utf8_lossy(entry);
            let Some((meta, path)) = entry.split_once('\t') else {
                return Err(invalid(operation, "Git returned an invalid tree listing"));
            };
            let regular = meta.starts_with("100644 blob ") || meta.starts_with("100755 blob ");
            if regular && selection.selects(path) && !path.contains('\n') {
                paths.push(path.to_owned());
            }
        }
        let mut files = BTreeMap::new();
        if paths.is_empty() {
            return Ok(Some(files));
        }
        let request: String = paths.iter().map(|p| format!("{commit}:{p}\n")).collect();
        let mut contents = self.observation(repository, operation);
        contents.capture_limit(limit.saturating_add(paths.len() * 128));
        contents
            .args(["cat-file", "--batch"])
            .input(request.into_bytes());
        let contents = contents.execute().await?;
        if contents.completed_successfully() && contents.truncated {
            return Ok(None);
        }
        require_success(&contents, operation)?;
        let mut bytes = contents.protocol_bytes();
        let mut total = 0usize;
        for path in paths {
            let header_end = bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or_else(|| invalid(operation, "Git returned an incomplete object"))?;
            let header = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
            let mut parts = header.split(' ');
            let (_, kind, size) = (parts.next(), parts.next(), parts.next());
            let size: usize = match (kind, size.and_then(|s| s.parse().ok())) {
                (Some("blob"), Some(size)) => size,
                _ => return Err(invalid(operation, "Git returned an unreadable file")),
            };
            let start = header_end + 1;
            if bytes.len() < start + size + 1 {
                return Err(invalid(operation, "Git returned an incomplete object"));
            }
            total += size;
            if total > limit {
                return Ok(None);
            }
            files.insert(path, bytes[start..start + size].to_vec());
            bytes = &bytes[start + size + 1..];
        }
        Ok(Some(files))
    }

    /// Fetch one commit by object ID from the named remote, checked against
    /// the expected URL like [`Self::fetch_checkout`]. Only objects are
    /// written: no ref, tag, `FETCH_HEAD`, submodule or background maintenance.
    pub async fn fetch_commit(
        &self,
        repository: &Path,
        remote: &str,
        expected_url: &str,
        commit: &str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let result = async {
            let operation = "fetch commit";
            super::validate_url(expected_url, auth, operation)?;
            check_oid(commit, operation)?;
            let transport = self
                .check_remote(repository, remote, expected_url, false, operation)
                .await?;
            let mut command = self.command(Some(repository), operation);
            if self.runtime_config().is_some() {
                command.member_rewrite(expected_url, &transport);
            }
            self.authenticate(&mut command, &transport, auth)?;
            command.config("gc.auto", "0");
            command.config("maintenance.auto", "false");
            command.args([
                "fetch",
                "--no-tags",
                "--no-recurse-submodules",
                "--no-write-fetch-head",
                "--quiet",
                "--",
                remote,
                commit,
            ]);
            command.execute().await?.success(operation, auth)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&format!("{expected_url} remote {remote}"), auth)
        })
    }
}
