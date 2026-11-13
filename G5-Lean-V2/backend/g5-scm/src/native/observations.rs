//! Read-only Git evidence. Authorization and proof interpretation stay with callers.
use super::{invalid, FailureEffect, FailureKind, NativeGit, NativeGitError};
use crate::GitAuth;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeObjectKind {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl NativeGit {
    /// Git's remerge patch contains only the merge's own conflict resolutions.
    pub fn remerge_diff_blocking(
        &self,
        repository: &Path,
        commit: &str,
    ) -> Result<Vec<u8>, NativeGitError> {
        let (runner, repository, commit) =
            (self.clone(), repository.to_path_buf(), commit.to_owned());
        super::run_blocking("read remerge diff", FailureEffect::Refused, async move {
            let operation = "read remerge diff";
            check_oid(&commit, operation)?;
            let mut command = runner.observation(&repository, operation);
            command.capture_limit(16 * 1024 * 1024);
            command.args([
                "show",
                "--remerge-diff",
                "--format=",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                &commit,
                "--",
            ]);
            let output = command.execute().await?;
            require_success(&output, operation)?;
            if output.truncated {
                return Err(invalid(operation, "Remerge diff exceeds 16 MiB"));
            }
            Ok(output.into_protocol_bytes())
        })
    }

    /// Synchronous startup consumers retain the same checked observation and
    /// cancellation/deadline supervisor as asynchronous callers.
    pub fn object_kind_blocking(
        &self,
        repository: &Path,
        oid: &str,
    ) -> Result<Option<NativeObjectKind>, NativeGitError> {
        let (runner, repository, oid) = (self.clone(), repository.to_path_buf(), oid.to_owned());
        super::run_blocking("read object type", FailureEffect::Refused, async move {
            runner.object_kind(&repository, &oid).await
        })
    }

    /// Resolve a full ref name, including symbolic refs. `None` means Git could
    /// not resolve that ref; it does not authorize creation or replace a CAS.
    pub async fn ref_tip(
        &self,
        repository: &Path,
        reference: &str,
    ) -> Result<Option<String>, NativeGitError> {
        let operation = "read ref tip";
        let result = async {
            self.check_full_ref(reference, operation).await?;
            let mut command = self.observation(repository, operation);
            command.args(["for-each-ref", "--format=%(refname) %(objectname)", reference]);
            let output = command.execute().await?;
            require_success(&output, operation)?;
            for line in output.stdout_text().lines() {
                let Some((name, oid)) = line.split_once(' ') else { return Err(invalid_output(operation)); };
                if !full_oid(oid) { return Err(invalid_output(operation)); }
                // for-each-ref may also list descendants of the supplied name.
                // Only exact equality is evidence; never use revision expansion.
                if name == reference { return Ok(Some(oid.to_owned())); }
            }
            // for-each-ref diagnoses broken direct refs but omits dangling
            // symbolic refs silently. An unresolved symbolic ref is not absence.
            if self
                .unchecked_symbolic_ref_target(repository, reference, operation)
                .await?
                .is_some()
            {
                return Err(NativeGitError {
                    operation, target: None, kind: FailureKind::Exit, effect: FailureEffect::Refused,
                    detail: "Symbolic ref has no resolvable tip; inspect repository state before retrying".into(),
                });
            }
            Ok(None)
        }
        .await;
        result.map_err(|error: NativeGitError| error.at_target(reference, &GitAuth::None))
    }

    /// Inspect symbolic-ref identity without following it. A missing/direct ref
    /// returns `None`; callers separately read its tip under their admission.
    pub async fn symbolic_ref_target(
        &self,
        repository: &Path,
        reference: &str,
    ) -> Result<Option<String>, NativeGitError> {
        let operation = "read symbolic ref";
        let result = async {
            self.check_full_ref(reference, operation).await?;
            self.unchecked_symbolic_ref_target(repository, reference, operation)
                .await
        }
        .await;
        result.map_err(|error: NativeGitError| error.at_target(reference, &GitAuth::None))
    }

    /// [`Self::symbolic_ref_target`] for a reference its caller already
    /// validated with [`Self::check_full_ref`].
    async fn unchecked_symbolic_ref_target(
        &self,
        repository: &Path,
        reference: &str,
        operation: &'static str,
    ) -> Result<Option<String>, NativeGitError> {
        let mut command = self.observation(repository, operation);
        command
            .read_only()
            .args(["symbolic-ref", "--quiet", "--no-recurse", reference]);
        let output = command.execute().await?;
        if negative_observation(&output, operation)? {
            return Ok(None);
        }
        require_success(&output, operation)?;
        let value = output.stdout_text();
        let target = value.trim_end_matches('\n');
        self.check_full_ref(target, operation)
            .await
            .map_err(|error| {
                if error.kind == FailureKind::InvalidInput {
                    invalid_output(operation)
                } else {
                    error
                }
            })?;
        Ok(Some(target.to_owned()))
    }

    /// Inspect the exact object (without peeling annotated tags). Missing
    /// objects are distinct from execution/repository errors.
    pub async fn object_kind(
        &self,
        repository: &Path,
        oid: &str,
    ) -> Result<Option<NativeObjectKind>, NativeGitError> {
        let operation = "read object type";
        let result = async {
            check_oid(oid, operation)?;
            self.check_object_format(repository, &[oid], operation)
                .await?;
            let mut command = self.observation(repository, operation);
            command
                .read_only()
                .args(["cat-file", "--batch-check=%(objecttype)"])
                .input(format!("{oid}\n").into_bytes());
            let output = command.execute().await?;
            require_success(&output, operation)?;
            match output.stdout_text().as_str() {
                "commit\n" => Ok(Some(NativeObjectKind::Commit)),
                "tree\n" => Ok(Some(NativeObjectKind::Tree)),
                "blob\n" => Ok(Some(NativeObjectKind::Blob)),
                "tag\n" => Ok(Some(NativeObjectKind::Tag)),
                missing if missing == format!("{oid} missing\n") => Ok(None),
                _ => Err(invalid_output(operation)),
            }
        }
        .await;
        result.map_err(|error: NativeGitError| error.at_target(oid, &GitAuth::None))
    }

    /// Ask Git whether one full object ID is an ancestor of another. Invalid
    /// or unavailable objects are errors, not a negative ancestry observation.
    pub async fn is_ancestor(
        &self,
        repository: &Path,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, NativeGitError> {
        let operation = "read commit ancestry";
        let result = async {
            check_oid(ancestor, operation)?;
            check_oid(descendant, operation)?;
            self.check_object_format(repository, &[ancestor, descendant], operation)
                .await?;
            let mut command = self.observation(repository, operation);
            command
                .read_only()
                .args(["merge-base", "--is-ancestor", ancestor, descendant]);
            let output = command.execute().await?;
            if negative_observation(&output, operation)? {
                return Ok(false);
            }
            require_success(&output, operation)?;
            Ok(true)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&format!("{ancestor}..{descendant}"), &GitAuth::None)
        })
    }

    pub(super) fn observation(
        &self,
        repository: &Path,
        operation: &'static str,
    ) -> super::Invocation {
        let mut command = self.command(Some(repository), operation);
        // Relative to -C, and explicit even when storage is damaged: never
        // discover a parent's repository. Git understands linked .git files.
        command.read_only();
        super::explicit_git_dir(&mut command, Some(repository));
        command
    }

    pub(super) async fn check_object_format(
        &self,
        repository: &Path,
        oids: &[&str],
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        let mut command = self.observation(repository, operation);
        command.args(["rev-parse", "--show-object-format"]);
        let output = command.execute().await?;
        require_success(&output, operation)?;
        let length = match output.stdout_text().as_str() {
            "sha1\n" => 40,
            "sha256\n" => 64,
            _ => return Err(invalid_output(operation)),
        };
        if oids.iter().any(|oid| oid.len() != length) {
            return Err(invalid(
                operation,
                "Object ID must be full length for this repository's object format",
            ));
        }
        Ok(())
    }

    pub(super) async fn check_full_ref(
        &self,
        reference: &str,
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        if !reference.starts_with("refs/") || reference.contains(['\0', '\n', '\r']) {
            return Err(invalid(operation, "Expected a full Git ref name"));
        }
        let mut command = self.command(None, operation);
        command.read_only().args(["check-ref-format", reference]);
        let output = command.execute().await?;
        if output.status.and_then(|status| status.code()) == Some(1) {
            return Err(invalid(operation, "Invalid Git ref name"));
        }
        require_success(&output, operation)
    }
}

pub(super) fn full_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}
pub(super) fn check_oid(oid: &str, operation: &'static str) -> Result<(), NativeGitError> {
    if full_oid(oid) {
        Ok(())
    } else {
        Err(invalid(operation, "Expected a full Git object ID"))
    }
}
pub(super) fn require_success(
    output: &super::Output,
    operation: &'static str,
) -> Result<(), NativeGitError> {
    check_diagnostics(output, operation)?;
    if !output.completed_successfully() {
        return Err(output.failure(
            operation,
            &GitAuth::None,
            FailureKind::Exit,
            FailureEffect::Refused,
        ));
    }
    if output.truncated {
        return Err(invalid_output(operation));
    }
    Ok(())
}
// Git can return success/absence while diagnosing a corrupt object or ref.
// Keep benign platform warnings (notably Apple Git's confstr warning) compatible.
fn check_diagnostics(
    output: &super::Output,
    operation: &'static str,
) -> Result<(), NativeGitError> {
    if output.stderr_text().lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("error:")
            || line.starts_with("fatal:")
            || line.starts_with("warning: ignoring broken ref ")
            || line.starts_with("warning: ignoring dangling symref ")
    }) {
        return Err(output.failure(
            operation,
            &GitAuth::None,
            FailureKind::Exit,
            FailureEffect::Refused,
        ));
    }
    Ok(())
}
pub(super) fn negative_observation(
    output: &super::Output,
    operation: &'static str,
) -> Result<bool, NativeGitError> {
    check_diagnostics(output, operation)?;
    if output.status.and_then(|status| status.code()) != Some(1) {
        return Ok(false);
    }
    if output.truncated || !output.stdout_text().is_empty() {
        return Err(invalid_output(operation));
    }
    Ok(true)
}
fn invalid_output(operation: &'static str) -> NativeGitError {
    NativeGitError {
        operation,
        target: None,
        kind: FailureKind::InvalidOutput,
        effect: FailureEffect::Refused,
        detail: "Git returned an invalid observation; inspect repository state before retrying"
            .into(),
    }
}

/// Closed read operations for the formal-review tool surface. OIDs and paths
/// are validated here; callers never supply Git arguments or configuration.
#[derive(Debug, Clone, Copy)]
pub enum ReviewSnapshotRead<'a> {
    File {
        commit: &'a str,
        path: &'a str,
    },
    Paths {
        commit: &'a str,
    },
    Diff {
        base: &'a str,
        source: &'a str,
        path: Option<&'a str>,
    },
}

impl NativeGit {
    pub async fn read_review_snapshot(
        &self,
        repository: &Path,
        read: ReviewSnapshotRead<'_>,
    ) -> Result<Vec<u8>, NativeGitError> {
        let operation = "read review snapshot";
        let mut command = self.observation(repository, operation);
        command.capture_limit(1024 * 1024);
        let valid_path = |path: &str| -> Result<(), NativeGitError> {
            if path.is_empty()
                || path.contains('\0')
                || path.contains(':')
                || Path::new(path)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err(invalid(
                    operation,
                    "Expected a confined repository-relative path",
                ));
            }
            Ok(())
        };
        match read {
            ReviewSnapshotRead::File { commit, path } => {
                check_oid(commit, operation)?;
                valid_path(path)?;
                // cat-file reads the Git object, never a symlink's target or a
                // checkout filter. No textconv, hooks, executable or shell.
                command.args(["cat-file", "blob", &format!("{commit}:{path}")]);
            }
            ReviewSnapshotRead::Paths { commit } => {
                check_oid(commit, operation)?;
                command.args(["ls-tree", "-r", "--name-only", commit]);
            }
            ReviewSnapshotRead::Diff { base, source, path } => {
                check_oid(base, operation)?;
                check_oid(source, operation)?;
                command.args(["diff", "--no-ext-diff", "--no-textconv", base, source, "--"]);
                if let Some(path) = path {
                    valid_path(path)?;
                    command.args([path]);
                }
            }
        }
        let output = command.execute().await?;
        if output.completed_successfully() && output.truncated {
            return Err(invalid(
                operation,
                "Review read exceeds 1 MiB; select a smaller path",
            ));
        }
        require_success(&output, operation)?;
        Ok(output.into_protocol_bytes())
    }
}
