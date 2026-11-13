//! Remote observations and object-only downloads. These operations change no
//! local or remote ref; callers retain admission and publication policy.
use super::{invalid, validate_url, FailureEffect, FailureKind, NativeGit, NativeGitError};
use crate::GitAuth;
use std::path::Path;

/// Bound for a complete branch advertisement: roughly 100k branches. A larger
/// advertisement is refused rather than silently truncated.
const BRANCH_LIST_LIMIT: usize = 8 * 1024 * 1024;

impl NativeGit {
    /// Every advertised ordinary branch and its tip, without fetching objects
    /// or changing local refs.
    pub async fn remote_branches(
        &self,
        url: &str,
        auth: &GitAuth,
    ) -> Result<Vec<(String, String)>, NativeGitError> {
        self.list_heads(url, auth, None).await
    }

    async fn list_heads(
        &self,
        url: &str,
        auth: &GitAuth,
        branch: Option<&str>,
    ) -> Result<Vec<(String, String)>, NativeGitError> {
        let operation = "list remote branches";
        let transport = auth.transport_url(url).into_owned();
        let result = async {
            validate_url(&transport, auth, operation)?;
            self.check_expansion(None, &transport, &transport, operation)
                .await?;
            let mut command = self.command(None, operation);
            self.authenticate(&mut command, &transport, auth)?;
            command.capture_limit(BRANCH_LIST_LIMIT);
            command
                .args(["ls-remote", "--heads", "--"])
                .arg(super::test_transport(&transport).unwrap_or_else(|| transport.clone()))
                .read_only();
            // The server filters a single-branch read; matching below stays exact.
            if let Some(branch) = branch {
                command.arg(format!("refs/heads/{branch}"));
            }
            let output = command.execute().await?;
            if !output.completed_successfully() {
                return Err(output.failure(
                    operation,
                    auth,
                    refusal_kind(&output),
                    FailureEffect::Refused,
                ));
            }
            if output.truncated {
                return Err(invalid(
                    operation,
                    "Remote branch list exceeds the read limit",
                ));
            }
            output
                .stdout_text()
                .lines()
                .map(|line| {
                    let (oid, name) = line.split_once('\t').ok_or_else(|| NativeGitError {
                        operation,
                        target: None,
                        kind: FailureKind::InvalidOutput,
                        effect: FailureEffect::Refused,
                        detail: "Git returned an unreadable remote branch list".into(),
                    })?;
                    let branch =
                        name.strip_prefix("refs/heads/")
                            .ok_or_else(|| NativeGitError {
                                operation,
                                target: None,
                                kind: FailureKind::InvalidOutput,
                                effect: FailureEffect::Refused,
                                detail: "Git listed a non-branch ref".into(),
                            })?;
                    Ok((branch.to_owned(), oid.to_owned()))
                })
                .collect()
        }
        .await;
        result.map_err(|error: NativeGitError| error.at_target(url, auth))
    }

    /// The advertised tip of one branch, or `None` when the remote lacks it.
    pub async fn remote_branch_tip(
        &self,
        url: &str,
        branch: &str,
        auth: &GitAuth,
    ) -> Result<Option<String>, NativeGitError> {
        if !super::valid_branch_name(branch) {
            return Err(invalid("read remote branch", "Invalid branch name").at_target(url, auth));
        }
        Ok(self
            .list_heads(url, auth, Some(branch))
            .await?
            .into_iter()
            .find_map(|(name, oid)| (name == branch).then_some(oid)))
    }

    /// Download the objects of one remote branch into `repository` without
    /// writing any ref or `FETCH_HEAD`. Classification callers inspect the
    /// exact provider commit while canonical refs stay unchanged.
    pub async fn fetch_branch_objects(
        &self,
        repository: &Path,
        url: &str,
        branch: &str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let operation = "download branch objects";
        let transport = auth.transport_url(url).into_owned();
        let result = async {
            validate_url(&transport, auth, operation)?;
            self.validate_branch(Some(repository), branch, operation)
                .await?;
            self.check_expansion(Some(repository), &transport, &transport, operation)
                .await?;
            let mut command = self.command(Some(repository), operation);
            self.authenticate(&mut command, &transport, auth)?;
            // Never start detached maintenance inside repository storage; the
            // storage owner schedules gc under its own lock and deadline.
            command.config("maintenance.auto", "false");
            command.config("gc.auto", "0");
            command
                .args([
                    "fetch",
                    "--no-write-fetch-head",
                    "--no-tags",
                    "--recurse-submodules=no",
                    "--",
                ])
                .arg(super::test_transport(&transport).unwrap_or_else(|| transport.clone()))
                .arg(format!("refs/heads/{branch}"));
            // Objects may have been written, but no ref changed either way.
            let output = command.execute().await?;
            if output.completed_successfully() {
                Ok(())
            } else {
                Err(output.failure(
                    operation,
                    auth,
                    refusal_kind(&output),
                    FailureEffect::Refused,
                ))
            }
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&format!("{url} refs/heads/{branch}"), auth)
        })
    }
}

fn refusal_kind(output: &super::Output) -> FailureKind {
    if output.credential_refused() {
        FailureKind::Unauthorized
    } else {
        FailureKind::Exit
    }
}
