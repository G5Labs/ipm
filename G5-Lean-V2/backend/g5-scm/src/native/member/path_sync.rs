//! Commit and publish only selected directories of a checkout that other
//! writers share (`g5.personal.memory-sync`). Best effort: a refusal leaves
//! the checkout's files, index and other commits as they were.
use super::checkpoint::{commit_selected_paths, CheckpointContext};
use super::{failed, is_stopped, validate_relay, MemberGit, MemberOperation};
use crate::native::{BranchChange, NativeCancellation, NativeGitError, PushSource, PushUpdate};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct MemberPathSync {
    pub repository: PathBuf,
    pub transport_url: String,
    pub branch: String,
    /// Repository-relative directories. Only files below them are committed,
    /// and only commits that change nothing else are published.
    pub directories: Vec<String>,
    pub message: String,
    /// Attributes for the replay onto a moved branch, such as a union merge
    /// for an index file. The checkout and its Git directory stay untouched.
    pub replay_attributes: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MemberPathSyncOutcome {
    /// Nothing below the directories is unpublished.
    Current,
    /// The branch now points at `head`; `files` were committed by this sync.
    Published { head: String, files: usize },
    /// Left for a later sync. Any commit made here stays on the checkout.
    Skipped {
        reason: &'static str,
        detail: Option<String>,
    },
}

fn skipped(reason: &'static str, detail: Option<String>) -> MemberPathSyncOutcome {
    MemberPathSyncOutcome::Skipped { reason, detail }
}

impl MemberGit {
    pub async fn sync_paths(
        &self,
        request: &MemberPathSync,
    ) -> Result<MemberPathSyncOutcome, NativeGitError> {
        let member = self.clone();
        let request = request.clone();
        self.supervise_operation("sync member directories", async move {
            member.sync_paths_inner(&request).await
        })
        .await?
    }

    async fn sync_paths_inner(
        &self,
        request: &MemberPathSync,
    ) -> Result<MemberPathSyncOutcome, NativeGitError> {
        let mut git = MemberOperation::new(self, &request.repository, "sync member directories");
        git.transport = Some(&request.transport_url);
        let redact = |outcome: MemberPathSyncOutcome| match outcome {
            MemberPathSyncOutcome::Skipped { reason, detail } => skipped(
                reason,
                detail.map(|detail| {
                    detail.replace(&request.transport_url, "[REDACTED:member_transport]")
                }),
            ),
            other => other,
        };
        match self.sync_steps(&git, request).await {
            Ok(outcome) => Ok(redact(outcome)),
            Err(mut error) => {
                error.detail = error
                    .detail
                    .replace(&request.transport_url, "[REDACTED:member_transport]");
                Err(git.failure(error))
            }
        }
    }

    async fn sync_steps(
        &self,
        git: &MemberOperation<'_>,
        request: &MemberPathSync,
    ) -> Result<MemberPathSyncOutcome, NativeGitError> {
        let root = &request.repository;
        validate_relay(&request.transport_url, git.name)?;
        self.check_branch(root, &request.branch, git.name).await?;
        if request.directories.is_empty()
            || request.directories.iter().any(|directory| {
                directory.is_empty()
                    || !Path::new(directory)
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
            })
        {
            return Err(failed(
                "Directory sync requires nonempty repository-relative directories",
            ));
        }
        let prefixes: Vec<String> = request
            .directories
            .iter()
            .map(|directory| format!("{}/", directory.trim_end_matches('/')))
            .collect();
        let selected = |path: &str| prefixes.iter().any(|prefix| path.starts_with(prefix));
        if self.current_branch(root).await?.as_deref() != Some(&request.branch)
            || !self.sequencers(root).await?.is_empty()
        {
            return Ok(skipped("OFF_BRANCH", None));
        }

        // Commit the changed files below the directories. The selected-path
        // checkpoint keeps the user's index and other staged work.
        let mut files = Vec::new();
        for entry in self.status(root).await? {
            for path in std::iter::once(&entry.path).chain(entry.from.as_ref()) {
                if selected(path) && !files.contains(path) {
                    files.push(path.clone());
                }
            }
        }
        let mut committed = 0;
        if !files.is_empty() {
            let ctx = CheckpointContext {
                operation: MemberOperation::new(self, root, git.name),
            };
            match commit_selected_paths(&ctx, root, &request.branch, &files, None, &request.message)
                .await
            {
                Ok((_, _, count)) => committed = count,
                Err(error) if is_stopped(&error) => return Err(error),
                Err(error) => return Ok(skipped("COMMIT_REFUSED", Some(error.detail))),
            }
        }

        git.check_relay_unchanged(&request.transport_url).await?;
        let tracking = format!("refs/remotes/origin/{}", request.branch);
        git.read(&[
            "fetch",
            "--no-tags",
            "--recurse-submodules=no",
            "--",
            &request.transport_url,
            &format!("+refs/heads/{}:{tracking}", request.branch),
        ])
        .await?;
        let published = git.read(&["rev-parse", "--verify", &tracking]).await?;
        let head = self.head_commit(root).await?;
        let range = format!("{published}..{head}");
        let local = git.read(&["rev-list", "--reverse", &range]).await?;
        if local.is_empty() {
            return Ok(MemberPathSyncOutcome::Current);
        }
        // Publish memory only: never the owner's other unpublished work.
        if !git
            .read(&["rev-list", "--merges", &range])
            .await?
            .is_empty()
        {
            return Ok(skipped("FOREIGN_COMMITS", None));
        }
        for commit in local.lines() {
            let parent = format!("{commit}^");
            let changed = git
                .read(&["diff", "--name-only", "-z", "--no-renames", &parent, commit])
                .await?;
            if changed
                .split('\0')
                .any(|path| !path.is_empty() && !selected(path))
            {
                return Ok(skipped("FOREIGN_COMMITS", None));
            }
        }

        if !self.git.is_ancestor(root, &published, &head).await? {
            // Git refuses to replay over tracked changes. Checking first keeps
            // the usual case quiet; Git's own refusal covers a later edit.
            if self
                .status(root)
                .await?
                .iter()
                .any(|entry| entry.status != "??")
            {
                return Ok(skipped("DIRTY", None));
            }
            let attributes = request
                .replay_attributes
                .as_ref()
                .map(|path| format!("core.attributesFile={}", path.display()));
            let mut args = Vec::new();
            if let Some(attributes) = &attributes {
                args.extend(["-c", attributes.as_str()]);
            }
            args.extend([
                "rebase",
                "--no-rebase-merges",
                "--no-fork-point",
                "--no-autostash",
                published.as_str(),
            ]);
            if let Err(error) = git.read(&args).await {
                // Abort only: Git restores the branch and the files it replayed.
                self.abort_replay(root).await;
                if is_stopped(&error) {
                    return Err(error);
                }
                return Ok(skipped("REPLAY_REFUSED", Some(error.detail)));
            }
        }

        let head = self.head_commit(root).await?;
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
                        expected: Some(published.clone()),
                    },
                },
                &crate::GitAuth::None,
            )
            .await;
        match pushed {
            Ok(_) => {
                // A convenience the next fetch also repairs.
                let _ = git
                    .read(&["update-ref", &tracking, &head, &published])
                    .await;
                Ok(MemberPathSyncOutcome::Published {
                    head,
                    files: committed,
                })
            }
            Err(error) if is_stopped(&error) => Err(error),
            Err(error) => Ok(skipped("PUSH_REFUSED", Some(error.detail))),
        }
    }

    /// End an interrupted or conflicted replay under a fresh, bounded context.
    /// Aborts unless the checkout is known to have no replay in progress, and
    /// never resets: a file an agent changed meanwhile stays as it is.
    async fn abort_replay(&self, root: &Path) {
        let mut cleanup = self.clone();
        cleanup.git.cancellation = NativeCancellation::default();
        cleanup.deadline = Some(Instant::now() + Duration::from_secs(10));
        cleanup.git.deadline = cleanup.deadline;
        if cleanup
            .sequencers(root)
            .await
            .is_ok_and(|sequencers| sequencers.is_empty())
        {
            return;
        }
        let abort = MemberOperation::new(&cleanup, root, "abort member directory replay");
        let _ = abort.read(&["rebase", "--abort"]).await;
    }
}
