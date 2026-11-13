//! Adopt the provider history in the owner checkout
//! (`g5.workspace.remote.adopt-provider-history`).
use super::{failed, validate_relay, MemberGit, MemberOperation};
use crate::native::{BranchChange, NativeCancellation, NativeGitError, PushSource, PushUpdate};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Exact inputs captured by the control plane. `expected_head` is the G5 head
/// of `branch` and the only lease; `provider_tip` is the reviewed provider
/// commit, pinned as the remote input of divergence run `run_id`.
#[derive(Clone)]
pub struct MemberAdopt {
    pub repository: PathBuf,
    pub transport_url: String,
    pub workspace_id: uuid::Uuid,
    pub branch: String,
    pub run_id: uuid::Uuid,
    pub expected_head: String,
    pub provider_tip: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MemberAdoptResult {
    /// The accepted G5 head after the operation: the provider tip.
    pub head: String,
    /// Whether G5 accepted a new head (false when it already held the tip).
    pub published: bool,
}

/// Why adopting stopped. Every refusal leaves G5 and the checkout unchanged.
#[derive(Debug)]
pub enum MemberAdoptFailure {
    /// The captured G5 head or the pinned provider tip moved.
    InputMoved(&'static str),
    /// The checkout holds commits G5 lacks; adopting would lose them.
    UnpushedCommits {
        local: String,
    },
    /// G5 refused or never confirmed the provider tip.
    Unpublished(NativeGitError),
    Failed(NativeGitError),
}

impl From<NativeGitError> for MemberAdoptFailure {
    fn from(error: NativeGitError) -> Self {
        Self::Failed(error)
    }
}

impl MemberGit {
    /// Reset the checked-out workspace branch to the reviewed provider tip and
    /// publish it to G5 with a lease on the captured G5 head, discarding the
    /// commits G5 holds beyond the provider. The admitted tree is clean and
    /// the checkout holds nothing G5 lacks, so a refusal restores the original
    /// checkout without losing work.
    pub async fn adopt_provider_tip(
        &self,
        request: &MemberAdopt,
    ) -> Result<MemberAdoptResult, MemberAdoptFailure> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("adopt provider history", async move {
            member.adopt_inner(&request).await
        })
        .await?
    }

    async fn adopt_inner(
        &self,
        request: &MemberAdopt,
    ) -> Result<MemberAdoptResult, MemberAdoptFailure> {
        let mut git = MemberOperation::new(self, &request.repository, "adopt provider history");
        git.transport = Some(&request.transport_url);
        let mut moved_from = None;
        let result = self.adopt_steps(&git, request, &mut moved_from).await;
        let redact = |mut error: NativeGitError| {
            error.detail = error
                .detail
                .replace(&request.transport_url, "[REDACTED:member_transport]");
            git.failure(error)
        };
        match result {
            Ok(done) => Ok(done),
            Err(failure) => {
                if let Some(original) = moved_from {
                    self.restore_adopted_checkout(&request.repository, &original)
                        .await;
                }
                Err(match failure {
                    MemberAdoptFailure::Failed(error) => MemberAdoptFailure::Failed(redact(error)),
                    MemberAdoptFailure::Unpublished(error) => {
                        MemberAdoptFailure::Unpublished(redact(error))
                    }
                    other => other,
                })
            }
        }
    }

    async fn adopt_steps(
        &self,
        git: &MemberOperation<'_>,
        request: &MemberAdopt,
        moved_from: &mut Option<String>,
    ) -> Result<MemberAdoptResult, MemberAdoptFailure> {
        let root = &request.repository;
        validate_relay(&request.transport_url, git.name)?;
        self.check_branch(root, &request.branch, git.name).await?;
        for oid in [&request.expected_head, &request.provider_tip] {
            if !crate::native::observations::full_oid(oid) {
                return Err(failed("Adopting requires exact G5 and provider commits").into());
            }
        }
        if self.current_branch(root).await?.as_deref() != Some(&request.branch) {
            return Err(failed("BRANCH_ASSOCIATION_MISMATCH").into());
        }
        let local = self.head_commit(root).await?;
        git.check_relay_unchanged(&request.transport_url).await?;
        let tracking = format!("refs/remotes/origin/{}", request.branch);
        let pinned = g5_contracts::workspace_git::local_resolution_ref(request.run_id, "remote");
        git.read(&[
            "fetch",
            "--no-tags",
            "--recurse-submodules=no",
            "--",
            &request.transport_url,
            &format!("+refs/heads/{}:{tracking}", request.branch),
            &format!(
                "+{}:{pinned}",
                g5_contracts::workspace_git::resolution_ref(
                    request.workspace_id,
                    request.run_id,
                    "remote"
                )
            ),
        ])
        .await?;
        // Exact inputs only: a moved head is reported, never adopted.
        if git.read(&["rev-parse", "--verify", &tracking]).await? != request.expected_head {
            return Err(MemberAdoptFailure::InputMoved("INTERNAL_BRANCH_MOVED"));
        }
        if git.read(&["rev-parse", "--verify", &pinned]).await? != request.provider_tip {
            return Err(MemberAdoptFailure::InputMoved("PROVIDER_INPUT_MOVED"));
        }
        if self.head_commit(root).await? != local
            || self.current_branch(root).await?.as_deref() != Some(&request.branch)
        {
            return Err(failed("Workspace branch changed while fetching adoption inputs").into());
        }
        let expected = request.expected_head.as_str();
        // A checkout behind G5 loses nothing; one with its own commits would.
        if local != expected && !self.git.is_ancestor(root, &local, expected).await? {
            return Err(MemberAdoptFailure::UnpushedCommits { local });
        }
        let tip = request.provider_tip.as_str();
        if expected == tip {
            if local != tip {
                *moved_from = Some(local);
                git.read(&["reset", "--hard", tip]).await?;
            }
            return Ok(MemberAdoptResult {
                head: tip.to_owned(),
                published: false,
            });
        }
        *moved_from = Some(local);
        git.read(&["reset", "--hard", tip]).await?;
        // The captured G5 head is the lease, never a refreshed tracking ref.
        let pushed = self
            .git
            .change_branch(
                BranchChange {
                    repository: root.clone(),
                    remote: None,
                    expected_url: request.transport_url.clone(),
                    source: Some(PushSource::Commit(tip.to_owned())),
                    branch: request.branch.clone(),
                    update: PushUpdate::Lease {
                        expected: Some(expected.to_owned()),
                    },
                },
                &crate::GitAuth::None,
            )
            .await;
        if let Err(error) = pushed {
            return Err(MemberAdoptFailure::Unpublished(error));
        }
        *moved_from = None;
        // G5 accepted the tip; the next refresh also repairs the tracking ref.
        let _ = git.read(&["update-ref", &tracking, tip, expected]).await;
        Ok(MemberAdoptResult {
            head: tip.to_owned(),
            published: true,
        })
    }

    /// Return the branch to the commit adopting started from, under a fresh,
    /// bounded cleanup context.
    async fn restore_adopted_checkout(&self, root: &std::path::Path, original: &str) {
        let mut cleanup = self.clone();
        cleanup.git.cancellation = NativeCancellation::default();
        cleanup.deadline = Some(Instant::now() + Duration::from_secs(10));
        cleanup.git.deadline = cleanup.deadline;
        let restore = MemberOperation::new(&cleanup, root, "restore member checkout");
        let _ = restore.read(&["reset", "--hard", original]).await;
    }
}
