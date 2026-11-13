//! Source rebase executed in the owner checkout (`g5.architecture.workspace_rebase`).
use super::{failed, is_stopped, validate_relay, MemberGit, MemberOperation};
use crate::native::{BranchChange, NativeCancellation, NativeGitError, PushSource, PushUpdate};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Exact inputs captured by the control plane. `expected_head` is the G5 head
/// of `branch` and the only lease the result may be published with.
#[derive(Clone)]
pub struct MemberRebase {
    pub repository: PathBuf,
    pub transport_url: String,
    pub branch: String,
    pub expected_head: String,
    pub onto_branch: String,
    pub onto_head: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MemberRebaseResult {
    /// The accepted G5 head after the operation.
    pub head: String,
    /// Whether G5 accepted a new head (false for a no-op).
    pub published: bool,
}

/// Why a member rebase stopped. Every refusal leaves G5 and the checkout
/// unchanged; only `Unpublished` leaves a local result behind, which is
/// reported, never reset.
#[derive(Debug)]
pub enum MemberRebaseFailure {
    /// The captured G5 or target head moved before replay.
    InputMoved(&'static str),
    /// The checkout's own commits conflict with newer G5 commits.
    Diverged {
        local: String,
        conflicts: Vec<String>,
    },
    /// Replay onto the target stopped on these paths.
    Conflicts(Vec<String>),
    /// The replayed result is local only: G5 refused or never confirmed it.
    Unpublished {
        head: String,
        error: NativeGitError,
    },
    Failed(NativeGitError),
}

impl From<NativeGitError> for MemberRebaseFailure {
    fn from(error: NativeGitError) -> Self {
        Self::Failed(error)
    }
}

impl MemberGit {
    /// Rebase the checked-out workspace branch onto the captured target head
    /// and publish it to G5 with a lease on the captured G5 head. The
    /// checkout first takes G5 work it lacks: a fast-forward, or a replay of
    /// its own commits onto the G5 head when both moved, which skips patches
    /// G5 already holds (as after an earlier central rebase). A target the
    /// branch already contains is a no-op; a branch the target contains
    /// fast-forwards to it. A refusal aborts any replay and restores the
    /// original checkout under a fresh, bounded cleanup context.
    pub async fn rebase_onto(
        &self,
        request: &MemberRebase,
    ) -> Result<MemberRebaseResult, MemberRebaseFailure> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("rebase member onto target", async move {
            member.rebase_onto_inner(&request).await
        })
        .await?
    }

    async fn rebase_onto_inner(
        &self,
        request: &MemberRebase,
    ) -> Result<MemberRebaseResult, MemberRebaseFailure> {
        let operation = "rebase member onto target";
        let mut git = MemberOperation::new(self, &request.repository, operation);
        git.transport = Some(&request.transport_url);
        let mut moved_from = None;
        let result = self.rebase_steps(&git, request, &mut moved_from).await;
        let redact = |mut error: NativeGitError| {
            error.detail = error
                .detail
                .replace(&request.transport_url, "[REDACTED:member_transport]");
            git.failure(error)
        };
        match result {
            Ok(done) => Ok(done),
            Err(MemberRebaseFailure::Unpublished { head, error }) => {
                Err(MemberRebaseFailure::Unpublished {
                    head,
                    error: redact(error),
                })
            }
            Err(failure) => {
                if let Some(original) = moved_from {
                    self.restore_checkout(&request.repository, &original).await;
                }
                Err(match failure {
                    MemberRebaseFailure::Failed(error) => {
                        MemberRebaseFailure::Failed(redact(error))
                    }
                    other => other,
                })
            }
        }
    }

    async fn rebase_steps(
        &self,
        git: &MemberOperation<'_>,
        request: &MemberRebase,
        moved_from: &mut Option<String>,
    ) -> Result<MemberRebaseResult, MemberRebaseFailure> {
        let root = &request.repository;
        validate_relay(&request.transport_url, git.name)?;
        self.check_branch(root, &request.branch, git.name).await?;
        self.check_branch(root, &request.onto_branch, git.name)
            .await?;
        for oid in [&request.expected_head, &request.onto_head] {
            if !crate::native::observations::full_oid(oid) {
                return Err(failed("Rebase requires exact source and target commits").into());
            }
        }
        if self.current_branch(root).await?.as_deref() != Some(&request.branch) {
            return Err(failed("BRANCH_ASSOCIATION_MISMATCH").into());
        }
        let local = self.head_commit(root).await?;
        git.check_relay_unchanged(&request.transport_url).await?;
        let tracking = format!("refs/remotes/origin/{}", request.branch);
        let onto_ref = format!("refs/remotes/origin/{}", request.onto_branch);
        git.read(&[
            "fetch",
            "--no-tags",
            "--recurse-submodules=no",
            "--",
            &request.transport_url,
            &format!("+refs/heads/{}:{tracking}", request.branch),
            &format!("+refs/heads/{}:{onto_ref}", request.onto_branch),
        ])
        .await?;
        // Exact inputs only: a moved head is reported, never adopted.
        if git.read(&["rev-parse", "--verify", &tracking]).await? != request.expected_head {
            return Err(MemberRebaseFailure::InputMoved("INTERNAL_BRANCH_MOVED"));
        }
        if git.read(&["rev-parse", "--verify", &onto_ref]).await? != request.onto_head {
            return Err(MemberRebaseFailure::InputMoved("TARGET_MOVED"));
        }
        if self.head_commit(root).await? != local
            || self.current_branch(root).await?.as_deref() != Some(&request.branch)
        {
            return Err(failed("Workspace branch changed while fetching rebase inputs").into());
        }
        let expected = request.expected_head.as_str();
        // Bring the checkout up to G5 first, keeping its own commits.
        *moved_from = Some(local.clone());
        if local != expected && !self.git.is_ancestor(root, expected, &local).await? {
            if self.git.is_ancestor(root, &local, expected).await? {
                git.read(&["merge", "--ff-only", expected]).await?;
            } else if let Err(conflicts) = replay(git, expected).await? {
                return Err(MemberRebaseFailure::Diverged { local, conflicts });
            }
        }
        let start = self.head_commit(root).await?;
        let onto = request.onto_head.as_str();
        if !self.git.is_ancestor(root, onto, &start).await? {
            if self.git.is_ancestor(root, &start, onto).await? {
                git.read(&["merge", "--ff-only", onto]).await?;
            } else if let Err(conflicts) = replay(git, onto).await? {
                return Err(MemberRebaseFailure::Conflicts(conflicts));
            }
        }
        let head = self.head_commit(root).await?;
        if head == expected {
            return Ok(MemberRebaseResult {
                head,
                published: false,
            });
        }
        // The captured G5 head is the lease, never a refreshed tracking ref.
        let pushed = self
            .git
            .change_branch(
                BranchChange {
                    repository: root.clone(),
                    remote: None,
                    expected_url: request.transport_url.clone(),
                    source: Some(PushSource::Commit(head.clone())),
                    branch: request.branch.clone(),
                    update: PushUpdate::Lease {
                        expected: Some(expected.to_owned()),
                    },
                },
                &crate::GitAuth::None,
            )
            .await;
        if let Err(error) = pushed {
            return Err(MemberRebaseFailure::Unpublished { head, error });
        }
        *moved_from = None;
        // G5 accepted the result. The local tracking ref is a convenience the
        // next refresh also repairs, so failing to move it cannot turn the
        // confirmed effect into a reported failure.
        let _ = git.read(&["update-ref", &tracking, &head, expected]).await;
        Ok(MemberRebaseResult {
            head,
            published: true,
        })
    }

    /// Abort any replay and return the branch to the commit the operation
    /// started from. The admitted tree was clean, so nothing else is lost.
    async fn restore_checkout(&self, root: &std::path::Path, original: &str) {
        let mut cleanup = self.clone();
        cleanup.git.cancellation = NativeCancellation::default();
        cleanup.deadline = Some(Instant::now() + Duration::from_secs(10));
        cleanup.git.deadline = cleanup.deadline;
        let restore = MemberOperation::new(&cleanup, root, "restore member checkout");
        let _ = restore.read(&["rebase", "--abort"]).await;
        let _ = restore.read(&["reset", "--hard", original]).await;
    }
}

/// Replay the checked-out commits onto `upstream`, dropping merges and
/// patches it already holds. A stop on unmerged paths returns them; the
/// caller restores the checkout.
async fn replay(
    git: &MemberOperation<'_>,
    upstream: &str,
) -> Result<Result<(), Vec<String>>, MemberRebaseFailure> {
    let Err(error) = git
        .read(&["rebase", "--no-rebase-merges", "--no-fork-point", upstream])
        .await
    else {
        return Ok(Ok(()));
    };
    if is_stopped(&error) {
        return Err(error.into());
    }
    let conflicts = git
        .read(&["diff", "--name-only", "-z", "--diff-filter=U"])
        .await
        .map(|text| {
            text.split('\0')
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if conflicts.is_empty() {
        return Err(error.into());
    }
    Ok(Err(conflicts))
}
