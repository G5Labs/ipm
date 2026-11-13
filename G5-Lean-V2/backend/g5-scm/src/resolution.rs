//! Immutable inputs for an agent's Git conflict-resolution run.

use g5_contracts::error::GitOperationStage;
use g5_contracts::{credential_scrub::scrub, workspace_git::resolution_ref, G5Error};
use uuid::Uuid;

use crate::GitRepo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionHistory {
    Related,
    Unrelated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionPinOutcome {
    Created(ResolutionHistory),
    AlreadyPinned(ResolutionHistory),
    Replaced(ResolutionHistory),
}

impl ResolutionPinOutcome {
    pub fn history(self) -> ResolutionHistory {
        match self {
            Self::Created(history) | Self::AlreadyPinned(history) | Self::Replaced(history) => {
                history
            }
        }
    }
}

/// Observed direct targets for an explicitly authorized replacement. `None`
/// means absent; it is not permission to overwrite an arbitrary value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionPinState {
    pub local_oid: Option<String>,
    pub remote_oid: Option<String>,
}

fn direct_target(repo: &git2::Repository, name: &str) -> Result<Option<String>, G5Error> {
    match repo.find_reference(name) {
        Ok(reference) => reference
            .target()
            .map(|oid| Some(oid.to_string()))
            .ok_or_else(|| G5Error::Conflict {
                reason: format!("Resolution input {name} is symbolic, not an immutable pin"),
            }),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(error) => Err(crate::backend_error(error)),
    }
}

/// A refusal proves this invocation changed no refs. A commit failure does not:
/// libgit2 writes a locked transaction's refs one by one without rollback.
#[derive(Debug)]
pub enum ResolutionPinError {
    Refused {
        stage: GitOperationStage,
        error: G5Error,
    },
    Uncertain(G5Error),
}

impl std::fmt::Display for ResolutionPinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused { error, .. } | Self::Uncertain(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ResolutionPinError {}

impl GitRepo {
    /// Pin the local and remote commits for one resolution run. Both refs must
    /// be absent or already directly target the same pair. Lock both before
    /// checking, and stage both in one transaction; never repair or overwrite a
    /// partial/conflicting pair. Authority and run identity belong to the caller.
    ///
    /// An I/O failure during commit may leave one or both pins written. Return
    /// that uncertainty without retrying; the caller can inspect the named refs.
    pub fn pin_resolution_inputs(
        &self,
        workspace_id: Uuid,
        resolution_id: Uuid,
        local_oid: &str,
        remote_oid: &str,
    ) -> Result<ResolutionPinOutcome, ResolutionPinError> {
        self.write_resolution_inputs(workspace_id, resolution_id, local_oid, remote_oid, None)
    }

    /// Inspect abandoned preparation. The policy owner decides whether it may
    /// be replaced; the write rechecks both observed values under both locks.
    pub fn resolution_pin_state(
        &self,
        workspace_id: Uuid,
        resolution_id: Uuid,
    ) -> Result<ResolutionPinState, G5Error> {
        let repo = self.repo()?;
        Ok(ResolutionPinState {
            local_oid: direct_target(&repo, &resolution_ref(workspace_id, resolution_id, "local"))?,
            remote_oid: direct_target(
                &repo,
                &resolution_ref(workspace_id, resolution_id, "remote"),
            )?,
        })
    }

    /// Replace uncommitted preparation only when both direct targets still
    /// equal the caller's observations (including absence). A stale expectation
    /// refuses even if the desired new pair is already present. The caller must
    /// establish that no task owns these pins using its existing policy fences.
    pub fn replace_resolution_inputs(
        &self,
        workspace_id: Uuid,
        resolution_id: Uuid,
        local_oid: &str,
        remote_oid: &str,
        expected: &ResolutionPinState,
    ) -> Result<ResolutionPinOutcome, ResolutionPinError> {
        self.write_resolution_inputs(
            workspace_id,
            resolution_id,
            local_oid,
            remote_oid,
            Some(expected),
        )
    }

    fn write_resolution_inputs(
        &self,
        workspace_id: Uuid,
        resolution_id: Uuid,
        local_oid: &str,
        remote_oid: &str,
        expected: Option<&ResolutionPinState>,
    ) -> Result<ResolutionPinOutcome, ResolutionPinError> {
        let local_ref = resolution_ref(workspace_id, resolution_id, "local");
        let remote_ref = resolution_ref(workspace_id, resolution_id, "remote");
        let refused = |stage, error: G5Error| {
            let message = format!(
                "Pin resolution inputs {local_ref} and {remote_ref} refused before ref updates: {}. \
                 Inspect the requested commits and both refs before deciding whether to retry",
                scrub(&error.to_string())
            );
            ResolutionPinError::Refused {
                stage,
                error: match error {
                    G5Error::Conflict { .. } => G5Error::Conflict { reason: message },
                    _ => G5Error::GitError { message },
                },
            }
        };
        let refused_git =
            |stage| move |error: git2::Error| refused(stage, crate::backend_error(error));
        let repo = self
            .repo()
            .map_err(|error| refused(GitOperationStage::OpenRepository, error))?;
        let local = git2::Oid::from_str(local_oid)
            .map_err(refused_git(GitOperationStage::ValidateCommits))?;
        let remote = git2::Oid::from_str(remote_oid)
            .map_err(refused_git(GitOperationStage::ValidateCommits))?;
        repo.find_commit(local)
            .map_err(refused_git(GitOperationStage::ValidateCommits))?;
        repo.find_commit(remote)
            .map_err(refused_git(GitOperationStage::ValidateCommits))?;
        let history = match repo.merge_base(local, remote) {
            Ok(_) => ResolutionHistory::Related,
            Err(error) if error.code() == git2::ErrorCode::NotFound => ResolutionHistory::Unrelated,
            Err(error) => return Err(refused_git(GitOperationStage::ValidateCommits)(error)),
        };

        let mut transaction = repo
            .transaction()
            .map_err(refused_git(GitOperationStage::LockRefs))?;
        // The generated local/remote names have a stable lexicographic order.
        transaction
            .lock_ref(&local_ref)
            .map_err(refused_git(GitOperationStage::LockRefs))?;
        transaction
            .lock_ref(&remote_ref)
            .map_err(refused_git(GitOperationStage::LockRefs))?;
        let current = ResolutionPinState {
            local_oid: direct_target(&repo, &local_ref)
                .map_err(|error| refused(GitOperationStage::InspectRefs, error))?,
            remote_oid: direct_target(&repo, &remote_ref)
                .map_err(|error| refused(GitOperationStage::InspectRefs, error))?,
        };
        if let Some(expected) = expected {
            if &current != expected {
                return Err(refused(
                    GitOperationStage::CheckPreconditions,
                    G5Error::Conflict {
                        reason: format!(
                            "Resolution inputs changed: expected {expected:?}, found {current:?}"
                        ),
                    },
                ));
            }
        }
        let desired = ResolutionPinState {
            local_oid: Some(local.to_string()),
            remote_oid: Some(remote.to_string()),
        };
        if current == desired {
            return Ok(ResolutionPinOutcome::AlreadyPinned(history));
        }
        let absent = current.local_oid.is_none() && current.remote_oid.is_none();
        if expected.is_none() && !absent {
            return Err(refused(
                GitOperationStage::CheckPreconditions,
                G5Error::Conflict {
                    reason: format!(
                        "Resolution inputs conflict: current {current:?}, requested {desired:?}"
                    ),
                },
            ));
        }
        let signature = git2::Signature::now("G5", "g5@localhost")
            .map_err(refused_git(GitOperationStage::StageRefs))?;
        transaction
            .set_target(&local_ref, local, Some(&signature), "resolution input")
            .map_err(refused_git(GitOperationStage::StageRefs))?;
        transaction
            .set_target(&remote_ref, remote, Some(&signature), "resolution input")
            .map_err(refused_git(GitOperationStage::StageRefs))?;
        transaction.commit().map_err(|error| {
            ResolutionPinError::Uncertain(G5Error::GitError {
                message: format!(
                    "Pin resolution inputs {local_ref} and {remote_ref} failed during commit: {}. \
                     One or both refs may have changed; inspect both refs before deciding how to retry",
                    scrub(error.message())
                ),
            })
        })?;
        Ok(if absent {
            ResolutionPinOutcome::Created(history)
        } else {
            ResolutionPinOutcome::Replaced(history)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    struct Fixture {
        dir: tempfile::TempDir,
        raw: git2::Repository,
        repo: GitRepo,
        workspace: Uuid,
        run: Uuid,
        local: git2::Oid,
        remote: git2::Oid,
    }

    impl Fixture {
        fn new(related: bool) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let raw = git2::Repository::init_bare(dir.path()).unwrap();
            let local = commit(&raw, "local", None);
            let remote = commit(&raw, "remote", related.then_some(local));
            Self {
                repo: GitRepo::open(dir.path()).unwrap(),
                dir,
                raw,
                workspace: Uuid::new_v4(),
                run: Uuid::new_v4(),
                local,
                remote,
            }
        }

        fn name(&self, side: &str) -> String {
            resolution_ref(self.workspace, self.run, side)
        }

        fn pin(&self) -> Result<ResolutionPinOutcome, ResolutionPinError> {
            self.repo.pin_resolution_inputs(
                self.workspace,
                self.run,
                &self.local.to_string(),
                &self.remote.to_string(),
            )
        }

        fn targets(&self) -> (Option<String>, Option<String>) {
            (
                self.repo.ref_commit_oid(&self.name("local")).unwrap(),
                self.repo.ref_commit_oid(&self.name("remote")).unwrap(),
            )
        }
    }

    fn commit(repo: &git2::Repository, message: &str, parent: Option<git2::Oid>) -> git2::Oid {
        let tree = repo.treebuilder(None).unwrap().write().unwrap();
        let tree = repo.find_tree(tree).unwrap();
        let signature = git2::Signature::now("Test", "test@example.com").unwrap();
        let parents = parent.map(|id| repo.find_commit(id).unwrap());
        repo.commit(
            None,
            &signature,
            &signature,
            message,
            &tree,
            &parents.iter().collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn pins_related_and_unrelated_inputs_and_identical_replay_does_not_write() {
        for (related, history) in [
            (true, ResolutionHistory::Related),
            (false, ResolutionHistory::Unrelated),
        ] {
            let f = Fixture::new(related);
            assert_eq!(f.pin().unwrap(), ResolutionPinOutcome::Created(history));
            assert_eq!(
                f.targets(),
                (Some(f.local.to_string()), Some(f.remote.to_string()))
            );
            let local_path = f.dir.path().join(f.name("local"));
            let remote_path = f.dir.path().join(f.name("remote"));
            let modified =
                |path: &std::path::Path| std::fs::metadata(path).unwrap().modified().unwrap();
            let before = (modified(&local_path), modified(&remote_path));
            assert_eq!(
                f.pin().unwrap(),
                ResolutionPinOutcome::AlreadyPinned(history)
            );
            assert_eq!(before, (modified(&local_path), modified(&remote_path)));
            assert_eq!(f.raw.references().unwrap().count(), 2);
            assert!(!f.dir.path().join("index").exists());
        }
    }

    #[test]
    fn refuses_conflicting_or_incomplete_pairs_without_changing_either_ref() {
        for (local, remote) in [(true, false), (false, true), (true, true)] {
            let f = Fixture::new(true);
            if local {
                f.raw
                    .reference(&f.name("local"), f.local, false, "fixture")
                    .unwrap();
            }
            if remote {
                // A complete pair with a different remote also refuses.
                f.raw
                    .reference(&f.name("remote"), f.local, false, "fixture")
                    .unwrap();
            }
            let before = f.targets();
            let error = f.pin().unwrap_err();
            assert!(matches!(
                error,
                ResolutionPinError::Refused {
                    error: G5Error::Conflict { .. },
                    ..
                }
            ));
            assert!(error.to_string().contains("before ref updates"));
            assert!(error.to_string().contains(&f.name("local")));
            assert_eq!(before, f.targets());
        }
    }

    #[test]
    fn symbolic_pins_are_not_treated_as_absent_or_immutable_even_when_they_resolve() {
        for dangling in [false, true] {
            let f = Fixture::new(true);
            if !dangling {
                f.raw
                    .reference("refs/heads/local", f.local, false, "fixture")
                    .unwrap();
            }
            f.raw
                .reference_symbolic(&f.name("local"), "refs/heads/local", false, "fixture")
                .unwrap();
            assert!(matches!(
                f.pin(),
                Err(ResolutionPinError::Refused {
                    error: G5Error::Conflict { .. },
                    ..
                })
            ));
            assert_eq!(
                f.raw
                    .find_reference(&f.name("local"))
                    .unwrap()
                    .symbolic_target()
                    .unwrap(),
                Some("refs/heads/local")
            );
            assert!(f.raw.find_reference(&f.name("remote")).is_err());
        }
    }

    #[test]
    fn invalid_or_noncommit_inputs_refuse_before_creating_refs() {
        let f = Fixture::new(true);
        let blob = f.raw.blob(b"not a commit").unwrap().to_string();
        for oid in ["invalid", &blob, "1111111111111111111111111111111111111111"] {
            assert!(matches!(
                f.repo
                    .pin_resolution_inputs(f.workspace, f.run, &f.local.to_string(), oid),
                Err(ResolutionPinError::Refused {
                    stage: GitOperationStage::ValidateCommits,
                    ..
                })
            ));
            assert_eq!(f.targets(), (None, None));
        }
    }

    #[test]
    fn checked_replacement_recovers_abandoned_complete_and_partial_pairs() {
        for partial in [false, true] {
            let f = Fixture::new(true);
            f.pin().unwrap();
            if partial {
                f.raw
                    .find_reference(&f.name("local"))
                    .unwrap()
                    .delete()
                    .unwrap();
            }
            let expected = f.repo.resolution_pin_state(f.workspace, f.run).unwrap();
            assert_eq!(expected.local_oid.is_none(), partial);
            let next_local = commit(&f.raw, "local advanced", Some(f.local)).to_string();
            let next_remote = commit(&f.raw, "remote advanced", Some(f.remote)).to_string();
            assert!(matches!(
                f.repo
                    .replace_resolution_inputs(
                        f.workspace,
                        f.run,
                        &next_local,
                        &next_remote,
                        &expected
                    )
                    .unwrap(),
                ResolutionPinOutcome::Replaced(_)
            ));
            assert_eq!(
                f.targets(),
                (Some(next_local.clone()), Some(next_remote.clone()))
            );
            // A stale expectation refuses even if the desired pair was written.
            assert!(matches!(
                f.repo.replace_resolution_inputs(
                    f.workspace,
                    f.run,
                    &next_local,
                    &next_remote,
                    &expected
                ),
                Err(ResolutionPinError::Refused {
                    error: G5Error::Conflict { .. },
                    ..
                })
            ));
            let current = f.repo.resolution_pin_state(f.workspace, f.run).unwrap();
            assert!(matches!(
                f.repo
                    .replace_resolution_inputs(
                        f.workspace,
                        f.run,
                        &next_local,
                        &next_remote,
                        &current
                    )
                    .unwrap(),
                ResolutionPinOutcome::AlreadyPinned(_)
            ));
            assert_eq!(f.targets(), (Some(next_local), Some(next_remote)));
        }
    }

    #[test]
    fn replacement_refuses_a_changed_pair_before_writing_either_ref() {
        for side in ["local", "remote"] {
            let f = Fixture::new(true);
            f.pin().unwrap();
            let expected = f.repo.resolution_pin_state(f.workspace, f.run).unwrap();
            let changed = commit(&f.raw, "another writer", Some(f.local));
            f.raw
                .reference(&f.name(side), changed, true, "intervening write")
                .unwrap();
            let before = f.targets();
            assert!(matches!(
                f.repo.replace_resolution_inputs(
                    f.workspace,
                    f.run,
                    &f.remote.to_string(),
                    &f.local.to_string(),
                    &expected
                ),
                Err(ResolutionPinError::Refused {
                    error: G5Error::Conflict { .. },
                    stage: GitOperationStage::CheckPreconditions,
                    ..
                })
            ));
            assert_eq!(f.targets(), before);
        }
    }

    #[test]
    fn lock_contention_refuses_without_writes_and_releases_its_other_lock() {
        let f = Fixture::new(true);
        let remote_lock = f.dir.path().join(format!("{}.lock", f.name("remote")));
        std::fs::create_dir_all(remote_lock.parent().unwrap()).unwrap();
        std::fs::write(&remote_lock, "other writer").unwrap();
        assert!(matches!(
            f.pin(),
            Err(ResolutionPinError::Refused {
                stage: GitOperationStage::LockRefs,
                ..
            })
        ));
        assert_eq!(f.targets(), (None, None));
        assert!(!f
            .dir
            .path()
            .join(format!("{}.lock", f.name("local")))
            .exists());
        assert_eq!(
            std::fs::read_to_string(remote_lock).unwrap(),
            "other writer"
        );
    }

    #[test]
    fn commit_failure_reports_uncertainty_without_replaying() {
        let f = Fixture::new(true);
        // Reflogs are written at commit, after both refs have been locked and
        // checked. Block the enabled log with a nonempty directory; this also
        // fails when tests run as root and needs no production injection hook.
        f.raw
            .config()
            .unwrap()
            .set_str("core.logAllRefUpdates", "always")
            .unwrap();
        let log = f.dir.path().join("logs").join(f.name("remote"));
        std::fs::create_dir_all(&log).unwrap();
        std::fs::write(log.join("blocked"), "preserve me").unwrap();
        let error = f.pin().unwrap_err();
        assert!(
            matches!(error, ResolutionPinError::Uncertain(_)),
            "{error:?}"
        );
        assert!(error
            .to_string()
            .contains("One or both refs may have changed"));
        assert!(error.to_string().contains("inspect both refs"));
        assert_eq!(
            std::fs::read_to_string(log.join("blocked")).unwrap(),
            "preserve me"
        );
        for side in ["local", "remote"] {
            assert!(!f.dir.path().join(format!("{}.lock", f.name(side))).exists());
        }
    }

    #[test]
    fn competing_run_inputs_cannot_produce_a_mixed_pair() {
        let f = Fixture::new(true);
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = [(f.local, f.remote), (f.remote, f.local)]
            .into_iter()
            .map(|(local, remote)| {
                let repo = GitRepo::open(f.dir.path()).unwrap();
                let barrier = barrier.clone();
                let (workspace, run) = (f.workspace, f.run);
                std::thread::spawn(move || {
                    barrier.wait();
                    repo.pin_resolution_inputs(
                        workspace,
                        run,
                        &local.to_string(),
                        &remote.to_string(),
                    )
                })
            })
            .collect();
        let results: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(results
            .iter()
            .any(|result| matches!(result, Err(ResolutionPinError::Refused { .. }))));
        let targets = f.targets();
        assert!(
            targets == (Some(f.local.to_string()), Some(f.remote.to_string()))
                || targets == (Some(f.remote.to_string()), Some(f.local.to_string()))
        );
    }
}
