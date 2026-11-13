//! Declared member tracking and publication effects over a runtime relay lease.
use super::{failed, validate_relay, MemberGit, MemberOperation};
use crate::native::{BranchChange, NativeGitError, PushSource, PushUpdate};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct MemberPush {
    pub repository: PathBuf,
    pub transport_url: String,
    pub branch: String,
    pub commit: String,
    pub expected: String,
}

#[derive(Clone)]
pub struct MemberTrackingRefresh {
    pub repository: PathBuf,
    pub transport_url: String,
    pub workspace_id: uuid::Uuid,
    pub branch: String,
    pub parent_branch: Option<String>,
    /// Fast-forward the checkout to the refreshed branch when it has no
    /// commits of its own and no local change would be overwritten. Only a
    /// personal workspace's private repository asks (`g5.personal.follow`);
    /// every other member leaves live files unchanged on import.
    pub follow_branch: bool,
}

#[derive(Debug)]
pub struct MemberTrackingResult {
    pub parent_refreshed: Option<bool>,
    pub parent_failure: Option<NativeGitError>,
    /// `Some(true)` when the checkout moved to the refreshed branch,
    /// `Some(false)` when it was behind but Git refused the fast-forward;
    /// `None` when not asked, or when there was nothing to fast-forward.
    pub fast_forwarded: Option<bool>,
}

impl MemberGit {
    pub async fn push_commit(&self, request: &MemberPush) -> Result<(), NativeGitError> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("publish member commit", async move {
            member.push_commit_inner(&request).await
        })
        .await?
    }

    async fn push_commit_inner(&self, request: &MemberPush) -> Result<(), NativeGitError> {
        let operation = "publish member commit";
        let mut git = MemberOperation::new(self, &request.repository, operation);
        git.transport = Some(&request.transport_url);
        let result = async {
            validate_relay(&request.transport_url, operation)?;
            for oid in [&request.commit, &request.expected] {
                if !crate::native::observations::full_oid(oid) {
                    return Err(failed(
                        "Publication requires exact source and old destination OIDs",
                    ));
                }
            }
            self.check_branch(&request.repository, &request.branch, operation)
                .await?;
            if !self
                .git
                .is_ancestor(&request.repository, &request.expected, &request.commit)
                .await?
            {
                return Err(failed("RECONCILIATION_REQUIRED"));
            }
            self.git
                .change_branch(
                    BranchChange {
                        repository: request.repository.clone(),
                        remote: None,
                        expected_url: request.transport_url.clone(),
                        source: Some(PushSource::Commit(request.commit.clone())),
                        branch: request.branch.clone(),
                        update: PushUpdate::Lease {
                            expected: Some(request.expected.clone()),
                        },
                    },
                    &crate::GitAuth::None,
                )
                .await?;
            git.completed
                .lock()
                .unwrap()
                .push("remote branch confirmed".into());
            git.read(&[
                "update-ref",
                &format!("refs/remotes/origin/{}", request.branch),
                &request.commit,
                &request.expected,
            ])
            .await?;
            Ok(())
        }
        .await;
        result.map_err(|mut error: NativeGitError| {
            error.detail = error
                .detail
                .replace(&request.transport_url, "[REDACTED:member_transport]");
            git.failure(error)
        })
    }

    /// Refresh exactly the admitted local tracking namespaces. A missing parent
    /// is informational; a cancelled/deadline parent fetch stops the operation.
    pub async fn refresh_tracking(
        &self,
        request: &MemberTrackingRefresh,
    ) -> Result<MemberTrackingResult, NativeGitError> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("refresh member tracking refs", async move {
            member.refresh_tracking_inner(&request).await
        })
        .await?
    }

    async fn refresh_tracking_inner(
        &self,
        request: &MemberTrackingRefresh,
    ) -> Result<MemberTrackingResult, NativeGitError> {
        let operation = "refresh member tracking refs";
        let mut git = MemberOperation::new(self, &request.repository, operation);
        git.transport = Some(&request.transport_url);
        let result = async {
            validate_relay(&request.transport_url, operation)?;
            self.check_branch(&request.repository, &request.branch, operation)
                .await?;
            if let Some(parent) = &request.parent_branch {
                self.check_branch(&request.repository, parent, operation)
                    .await?;
            }
            git.check_relay_unchanged(&request.transport_url).await?;
            let mut refs = vec![format!(
                "+refs/heads/{0}:refs/remotes/origin/{0}",
                request.branch
            )];
            refs.extend(g5_contracts::workspace_git::provider_fetch_refspecs(
                request.workspace_id,
            ));
            let mut args = vec![
                "fetch",
                "--no-tags",
                "--prune",
                "--recurse-submodules=no",
                "--",
                request.transport_url.as_str(),
            ];
            args.extend(refs.iter().map(String::as_str));
            git.read(&args).await?;
            let parent_failure = if let Some(parent) = request
                .parent_branch
                .as_deref()
                .filter(|parent| *parent != request.branch)
            {
                let result = git
                    .read(&[
                        "fetch",
                        "--no-tags",
                        "--recurse-submodules=no",
                        "--",
                        &request.transport_url,
                        &format!("+refs/heads/{parent}:refs/remotes/origin/{parent}"),
                    ])
                    .await;
                match result {
                    Ok(_) => None,
                    Err(error) if super::is_stopped(&error) => return Err(error),
                    Err(error) => Some(error),
                }
            } else {
                None
            };
            let fast_forwarded = if request.follow_branch {
                self.follow_branch(&git, &request.repository, &request.branch)
                    .await?
            } else {
                None
            };
            Ok(MemberTrackingResult {
                parent_refreshed: request
                    .parent_branch
                    .as_deref()
                    .filter(|parent| *parent != request.branch)
                    .map(|_| parent_failure.is_none()),
                parent_failure,
                fast_forwarded,
            })
        }
        .await;
        result.map_err(|error| git.failure(error))
    }

    /// Move a checkout that sits on `branch`, with no sequencer and no commits
    /// beyond its refreshed tracking ref, up to that ref. Git's `--ff-only`
    /// merge refuses when a local change or untracked file would be
    /// overwritten, and when a commit made since the ancestry check turned the
    /// move into a merge; either refusal leaves the checkout as it was.
    async fn follow_branch(
        &self,
        git: &MemberOperation<'_>,
        root: &Path,
        branch: &str,
    ) -> Result<Option<bool>, NativeGitError> {
        if self.current_branch(root).await?.as_deref() != Some(branch)
            || !self.sequencers(root).await?.is_empty()
        {
            return Ok(None);
        }
        let Some(tracking) = self.tracking_tip(root, branch).await? else {
            return Ok(None);
        };
        let local = self.head_commit(root).await?;
        if local == tracking || !self.git.is_ancestor(root, &local, &tracking).await? {
            return Ok(None);
        }
        match git
            .read(&["merge", "--ff-only", "--no-edit", "--quiet", &tracking])
            .await
        {
            Ok(_) => Ok(Some(true)),
            Err(error) if super::is_stopped(&error) => Err(error),
            Err(_) => Ok(Some(false)),
        }
    }

    /// Switch to the mapped branch or materialize it from an existing admitted
    /// tracking ref. This cannot create a remote branch.
    pub async fn switch_back(&self, root: &Path, branch: &str) -> Result<bool, NativeGitError> {
        let member = self.clone();
        let root = root.to_path_buf();
        let branch = branch.to_owned();
        self.supervise_operation("switch to member branch", async move {
            let operation = "switch to member branch";
            let git = MemberOperation::new(&member, &root, operation);
            let result = async {
                member.check_branch(&root, &branch, operation).await?;
                if member.tracking_tip(&root, &branch).await?.is_none() {
                    return Err(failed("MISSING_INTERNAL_BRANCH"));
                }
                let exists = member
                    .read_optional(
                        &root,
                        operation,
                        &[
                            "rev-parse",
                            "--verify",
                            "--quiet",
                            &format!("refs/heads/{branch}"),
                        ],
                    )
                    .await?
                    .is_some();
                if exists {
                    git.read(&["checkout", "--no-guess", "-q", &branch, "--"])
                        .await?;
                } else {
                    git.read(&[
                        "checkout",
                        "-q",
                        "-b",
                        &branch,
                        "--track",
                        &format!("refs/remotes/origin/{branch}"),
                        "--",
                    ])
                    .await?;
                }
                Ok(!exists)
            }
            .await;
            result.map_err(|error| git.failure(error))
        })
        .await?
    }
}
