//! Commit and checkpoint operations.

use crate::{GitAuthor, GitRepo};
use g5_contracts::G5Error;

/// Outcome of a checkpoint operation.
#[derive(Debug, Clone)]
pub struct CheckpointOutcome {
    /// Files that changed in this checkpoint (relative paths).
    pub files_changed: Vec<String>,
    /// Commit SHA, or `None` if the working tree was clean.
    pub commit: Option<String>,
}

/// Creates a checkpoint commit if the working tree has changes.
///
/// Stages all files, checks for changes via `workdir_status`, and commits
/// only if there are dirty files. Returns the list of changed files and
/// the commit SHA (or `None` if clean).
pub fn checkpoint(
    repo: &GitRepo,
    msg: &str,
    author: &GitAuthor,
) -> Result<CheckpointOutcome, G5Error> {
    let status = repo.workdir_status()?;
    if status.is_empty() {
        return Ok(CheckpointOutcome {
            files_changed: vec![],
            commit: None,
        });
    }

    let files_changed: Vec<String> = status.iter().map(|(_, path)| path.clone()).collect();

    repo.add_all()?;
    let sha = repo.commit(msg, author)?;

    Ok(CheckpointOutcome {
        files_changed,
        commit: Some(sha),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        prepare_files_commit_from_exact_base, prepare_initial_empty_commit,
        prepare_initial_files_commit, prepare_tree_commit_from_exact_base, ExactBaseFilesCommit,
        ExactBaseTreeCommit, ExactTreeChange, GitAuthor, InitialFilesCommit,
    };
    use std::path::Path;

    use std::sync::{Arc, Barrier};

    fn seed_repo() -> (GitRepo, tempfile::TempDir) {
        let (repo, dir) = GitRepo::init_temp();
        // Create an initial commit so HEAD is born.
        let author = GitAuthor::test_author();
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        repo.add_all().unwrap();
        repo.commit("Initial commit", &author).unwrap();
        (repo, dir)
    }

    #[test]
    fn test_checkpoint_adds_commits_returns_diff() {
        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();

        std::fs::write(dir.path().join("changed.txt"), "new content").unwrap();

        let outcome = checkpoint(&repo, "checkpoint", &author).unwrap();
        assert!(!outcome.files_changed.is_empty());
        assert!(outcome.commit.is_some());
        assert_eq!(outcome.commit.unwrap().len(), 40);
    }

    #[test]
    fn test_checkpoint_no_changes_returns_empty() {
        let (repo, _dir) = seed_repo();
        let author = GitAuthor::test_author();

        let outcome = checkpoint(&repo, "checkpoint", &author).unwrap();
        assert!(outcome.files_changed.is_empty());
        assert!(outcome.commit.is_none());
    }

    #[test]
    fn test_prepare_files_commit_from_exact_base_does_not_advance_branch() {
        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();
        let base_head = repo.current_head().unwrap();

        repo.create_branch("docs", &base_head).unwrap();
        repo.checkout_branch("docs").unwrap();
        std::fs::write(dir.path().join("README.md"), "advanced branch\n").unwrap();
        repo.add_all().unwrap();
        let advanced_head = repo.commit("Advance docs branch", &author).unwrap();

        let files = vec![("intent_record/views/a.jsonl".to_string(), b"{}\n".to_vec())];
        let prepared_head = prepare_files_commit_from_exact_base(
            &repo,
            ExactBaseFilesCommit {
                base_commit: &base_head,
                files: &files,
                message: "Prepare provenance backfill",
                author: &author,
            },
        )
        .unwrap();

        assert_eq!(repo.resolve_branch_head("docs").unwrap(), advanced_head);
        assert_eq!(
            repo.show_file(&prepared_head, "intent_record/views/a.jsonl")
                .unwrap(),
            b"{}\n"
        );
        let raw = git2::Repository::open(dir.path()).unwrap();
        let prepared = raw
            .find_commit(git2::Oid::from_str(&prepared_head).unwrap())
            .unwrap();
        assert_eq!(prepared.parent_count(), 1);
        assert_eq!(prepared.parent_id(0).unwrap().to_string(), base_head);
    }

    #[test]
    fn test_prepare_tree_commit_applies_mixed_changes_without_mutating_checkout_or_index() {
        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();
        std::fs::write(dir.path().join("modify.txt"), "before\n").unwrap();
        std::fs::write(dir.path().join("delete.txt"), "delete me\n").unwrap();
        std::fs::write(dir.path().join("untouched.txt"), "stable\n").unwrap();
        repo.add_all().unwrap();
        let base_head = repo.commit("Seed exact tree", &author).unwrap();

        std::fs::write(dir.path().join("README.md"), "dirty checkout\n").unwrap();
        let raw = git2::Repository::open(dir.path()).unwrap();
        let index_tree_before = raw.index().unwrap().write_tree().unwrap();
        let base = raw
            .find_commit(git2::Oid::from_str(&base_head).unwrap())
            .unwrap();
        let untouched_before = base
            .tree()
            .unwrap()
            .get_path(Path::new("untouched.txt"))
            .unwrap()
            .id();

        let changes = vec![
            ExactTreeChange::Write {
                path: "modify.txt".to_string(),
                content: b"after\n".to_vec(),
            },
            ExactTreeChange::Delete {
                path: "delete.txt".to_string(),
            },
            ExactTreeChange::Write {
                path: "nested/new.txt".to_string(),
                content: Vec::new(),
            },
        ];
        let prepared = prepare_tree_commit_from_exact_base(
            &repo,
            ExactBaseTreeCommit {
                base_commit: &base_head,
                changes: &changes,
                message: "Apply exact tree changes",
                author: &author,
            },
        )
        .unwrap();

        assert_eq!(repo.current_head().unwrap(), base_head);
        assert_eq!(
            raw.index().unwrap().write_tree().unwrap(),
            index_tree_before
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "dirty checkout\n"
        );
        assert_eq!(repo.show_file(&prepared, "modify.txt").unwrap(), b"after\n");
        assert_eq!(repo.show_file(&prepared, "nested/new.txt").unwrap(), b"");
        assert!(repo.show_file(&prepared, "delete.txt").is_err());

        let prepared_commit = raw
            .find_commit(git2::Oid::from_str(&prepared).unwrap())
            .unwrap();
        assert_eq!(prepared_commit.parent_count(), 1);
        assert_eq!(prepared_commit.parent_id(0).unwrap().to_string(), base_head);
        assert_eq!(
            prepared_commit.message().unwrap(),
            "Apply exact tree changes"
        );
        assert_eq!(prepared_commit.author().name().unwrap(), author.name);
        assert_eq!(
            prepared_commit
                .tree()
                .unwrap()
                .get_path(Path::new("untouched.txt"))
                .unwrap()
                .id(),
            untouched_before
        );
    }

    #[test]
    fn test_prepare_tree_commit_supports_concurrent_independent_repo_handles() {
        const CONCURRENT_PREPARERS: usize = 8;

        let (repo, dir) = seed_repo();
        let base_head = repo.current_head().unwrap();
        let raw = git2::Repository::open(dir.path()).unwrap();
        let index_tree_before = raw.index().unwrap().write_tree().unwrap();
        let barrier = Arc::new(Barrier::new(CONCURRENT_PREPARERS));
        let mut threads = Vec::new();

        for ordinal in 0..CONCURRENT_PREPARERS {
            let repo_path = dir.path().to_path_buf();
            let base_head = base_head.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                let repo = GitRepo::open(&repo_path).unwrap();
                let author = GitAuthor::test_author();
                let path = format!("concurrent/{ordinal}.txt");
                let content = format!("candidate {ordinal}\n").into_bytes();
                let changes = vec![ExactTreeChange::Write {
                    path: path.clone(),
                    content: content.clone(),
                }];
                barrier.wait();
                let prepared = prepare_tree_commit_from_exact_base(
                    &repo,
                    ExactBaseTreeCommit {
                        base_commit: &base_head,
                        changes: &changes,
                        message: "Concurrent exact-base preparation",
                        author: &author,
                    },
                )
                .unwrap();
                (prepared, path, content)
            }));
        }

        for thread in threads {
            let (prepared, path, content) = thread.join().unwrap();
            assert_eq!(repo.show_file(&prepared, &path).unwrap(), content);
            let commit = raw
                .find_commit(git2::Oid::from_str(&prepared).unwrap())
                .unwrap();
            assert_eq!(commit.parent_count(), 1);
            assert_eq!(commit.parent_id(0).unwrap().to_string(), base_head);
        }
        assert_eq!(repo.current_head().unwrap(), base_head);
        assert_eq!(
            raw.index().unwrap().write_tree().unwrap(),
            index_tree_before
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_prepare_tree_commit_preserves_executable_and_symlink_modes() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();
        let executable = dir.path().join("run.sh");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).unwrap();
        symlink("README.md", dir.path().join("readme-link")).unwrap();
        repo.add_all().unwrap();
        let base_head = repo.commit("Seed modes", &author).unwrap();

        let changes = vec![
            ExactTreeChange::Write {
                path: "run.sh".to_string(),
                content: b"#!/bin/sh\nexit 1\n".to_vec(),
            },
            ExactTreeChange::Write {
                path: "readme-link".to_string(),
                content: b"run.sh".to_vec(),
            },
            ExactTreeChange::Write {
                path: "new.txt".to_string(),
                content: b"new\n".to_vec(),
            },
        ];
        let prepared = prepare_tree_commit_from_exact_base(
            &repo,
            ExactBaseTreeCommit {
                base_commit: &base_head,
                changes: &changes,
                message: "Preserve modes",
                author: &author,
            },
        )
        .unwrap();
        let raw = git2::Repository::open(dir.path()).unwrap();
        let tree = raw
            .find_commit(git2::Oid::from_str(&prepared).unwrap())
            .unwrap()
            .tree()
            .unwrap();
        assert_eq!(
            tree.get_path(Path::new("run.sh")).unwrap().filemode(),
            0o100755
        );
        assert_eq!(
            tree.get_path(Path::new("readme-link")).unwrap().filemode(),
            0o120000
        );
        assert_eq!(
            tree.get_path(Path::new("new.txt")).unwrap().filemode(),
            0o100644
        );
        assert_eq!(repo.show_file(&prepared, "readme-link").unwrap(), b"run.sh");
    }

    #[test]
    fn test_prepare_tree_commit_returns_base_for_noop() {
        let (repo, _dir) = seed_repo();
        let author = GitAuthor::test_author();
        let base_head = repo.current_head().unwrap();
        let changes = vec![ExactTreeChange::Write {
            path: "README.md".to_string(),
            content: b"# Test\n".to_vec(),
        }];

        let prepared = prepare_tree_commit_from_exact_base(
            &repo,
            ExactBaseTreeCommit {
                base_commit: &base_head,
                changes: &changes,
                message: "No-op",
                author: &author,
            },
        )
        .unwrap();
        assert_eq!(prepared, base_head);
    }

    #[test]
    fn invalid_exact_tree_batch_writes_no_objects() {
        let (repo, dir) = seed_repo();
        let base = repo.current_head().unwrap();
        let author = GitAuthor::test_author();
        let bytes = b"must not be stored before the complete batch is valid";
        let oid = git2::Oid::hash_object(git2::ObjectType::Blob, bytes).unwrap();
        let raw = git2::Repository::open(dir.path()).unwrap();
        assert!(raw.find_blob(oid).is_err());
        let changes = [
            ExactTreeChange::Write {
                path: "valid.txt".into(),
                content: bytes.to_vec(),
            },
            ExactTreeChange::Delete {
                path: "absent.txt".into(),
            },
        ];
        assert!(matches!(
            prepare_tree_commit_from_exact_base(
                &repo,
                ExactBaseTreeCommit {
                    base_commit: &base,
                    changes: &changes,
                    message: "invalid batch",
                    author: &author,
                }
            ),
            Err(G5Error::ValidationError { .. })
        ));
        assert!(raw.find_blob(oid).is_err());
        assert_eq!(repo.current_head().unwrap(), base);
    }

    #[test]
    fn exact_tree_paths_are_literal_even_when_they_look_like_pathspecs() {
        let (repo, dir) = seed_repo();
        let base = repo.current_head().unwrap();
        let author = GitAuthor::test_author();
        let changes = [ExactTreeChange::Write {
            path: ":(glob)*.txt".into(),
            content: b"literal".to_vec(),
        }];
        let candidate = prepare_tree_commit_from_exact_base(
            &repo,
            ExactBaseTreeCommit {
                base_commit: &base,
                changes: &changes,
                message: "literal path",
                author: &author,
            },
        )
        .unwrap();
        let raw = git2::Repository::open(dir.path()).unwrap();
        let tree = raw
            .find_commit(git2::Oid::from_str(&candidate).unwrap())
            .unwrap()
            .tree()
            .unwrap();
        let entry = tree.get_name(":(glob)*.txt").unwrap();
        assert_eq!(raw.find_blob(entry.id()).unwrap().content(), b"literal");
        assert_eq!(repo.current_head().unwrap(), base);
        assert!(!dir.path().join(":(glob)*.txt").exists());
    }

    #[test]
    fn test_prepare_tree_commit_rejects_invalid_and_colliding_paths() {
        let (repo, _dir) = seed_repo();
        let author = GitAuthor::test_author();
        let base_head = repo.current_head().unwrap();
        let invalid_batches = vec![
            vec![],
            vec![ExactTreeChange::Write {
                path: "/absolute.txt".to_string(),
                content: Vec::new(),
            }],
            vec![ExactTreeChange::Write {
                path: "../escape.txt".to_string(),
                content: Vec::new(),
            }],
            vec![ExactTreeChange::Write {
                path: "./noncanonical.txt".to_string(),
                content: Vec::new(),
            }],
            vec![ExactTreeChange::Write {
                path: ".git/config".to_string(),
                content: Vec::new(),
            }],
            vec![ExactTreeChange::Write {
                path: ".g5/runtime.json".to_string(),
                content: Vec::new(),
            }],
            vec![ExactTreeChange::Delete {
                path: "missing.txt".to_string(),
            }],
            vec![
                ExactTreeChange::Write {
                    path: "same.txt".to_string(),
                    content: Vec::new(),
                },
                ExactTreeChange::Delete {
                    path: "same.txt".to_string(),
                },
            ],
            vec![
                ExactTreeChange::Write {
                    path: "parent".to_string(),
                    content: Vec::new(),
                },
                ExactTreeChange::Write {
                    path: "parent/child".to_string(),
                    content: Vec::new(),
                },
            ],
        ];

        for changes in invalid_batches {
            let result = prepare_tree_commit_from_exact_base(
                &repo,
                ExactBaseTreeCommit {
                    base_commit: &base_head,
                    changes: &changes,
                    message: "Reject invalid",
                    author: &author,
                },
            );
            assert!(matches!(result, Err(G5Error::ValidationError { .. })));
        }

        let changes = vec![ExactTreeChange::Write {
            path: "valid.txt".to_string(),
            content: Vec::new(),
        }];
        let base_tree = {
            let inner = git2::Repository::open(_dir.path()).unwrap();
            let base_commit = inner
                .find_commit(git2::Oid::from_str(&base_head).unwrap())
                .unwrap();
            base_commit.tree_id().to_string()
        };
        for invalid_base in [
            "0000000000000000000000000000000000000000".to_string(),
            base_tree,
        ] {
            let result = prepare_tree_commit_from_exact_base(
                &repo,
                ExactBaseTreeCommit {
                    base_commit: &invalid_base,
                    changes: &changes,
                    message: "Reject invalid base",
                    author: &author,
                },
            );
            assert!(matches!(result, Err(G5Error::NotFound { .. })));
        }
    }

    #[test]
    fn test_prepare_tree_commit_rejects_tree_gitlink_and_base_ancestor_collisions() {
        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();
        std::fs::create_dir_all(dir.path().join("directory")).unwrap();
        std::fs::write(dir.path().join("directory/file.txt"), "content\n").unwrap();
        std::fs::write(dir.path().join("blocking-file"), "content\n").unwrap();
        repo.add_all().unwrap();
        let ordinary_base = repo.commit("Seed collisions", &author).unwrap();

        for path in ["directory", "blocking-file/child.txt"] {
            let changes = vec![ExactTreeChange::Write {
                path: path.to_string(),
                content: b"replacement\n".to_vec(),
            }];
            let result = prepare_tree_commit_from_exact_base(
                &repo,
                ExactBaseTreeCommit {
                    base_commit: &ordinary_base,
                    changes: &changes,
                    message: "Reject base collision",
                    author: &author,
                },
            );
            assert!(matches!(result, Err(G5Error::ValidationError { .. })));
        }

        let raw = git2::Repository::open(dir.path()).unwrap();
        let parent = raw
            .find_commit(git2::Oid::from_str(&ordinary_base).unwrap())
            .unwrap();
        let parent_tree = parent.tree().unwrap();
        let mut builder = raw.treebuilder(Some(&parent_tree)).unwrap();
        builder
            .insert("nested-repo", parent.id(), 0o160000)
            .unwrap();
        let tree = raw.find_tree(builder.write().unwrap()).unwrap();
        let signature = git2::Signature::now(&author.name, &author.email).unwrap();
        let gitlink_base = raw
            .commit(
                None,
                &signature,
                &signature,
                "Seed gitlink",
                &tree,
                &[&parent],
            )
            .unwrap()
            .to_string();
        let changes = vec![ExactTreeChange::Write {
            path: "nested-repo".to_string(),
            content: b"not a gitlink\n".to_vec(),
        }];
        let result = prepare_tree_commit_from_exact_base(
            &repo,
            ExactBaseTreeCommit {
                base_commit: &gitlink_base,
                changes: &changes,
                message: "Reject gitlink",
                author: &author,
            },
        );
        assert!(matches!(result, Err(G5Error::ValidationError { .. })));
    }

    #[test]
    fn test_prepare_tree_commit_allows_durable_g5_content_in_bare_repo() {
        let (source, source_dir) = seed_repo();
        let base_head = source.current_head().unwrap();
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = git2::build::RepoBuilder::new()
            .bare(true)
            .clone(source_dir.path().to_str().unwrap(), bare_dir.path())
            .unwrap();
        drop(bare);
        let repo = GitRepo::open(bare_dir.path()).unwrap();
        let author = GitAuthor::test_author();
        let changes = vec![ExactTreeChange::Write {
            path: ".g5/preview_profiles/review.yaml".to_string(),
            content: b"name: review\n".to_vec(),
        }];

        let prepared = prepare_tree_commit_from_exact_base(
            &repo,
            ExactBaseTreeCommit {
                base_commit: &base_head,
                changes: &changes,
                message: "Add durable profile",
                author: &author,
            },
        )
        .unwrap();
        assert_eq!(
            repo.show_file(&prepared, ".g5/preview_profiles/review.yaml")
                .unwrap(),
            b"name: review\n"
        );
        assert_eq!(repo.current_head().unwrap(), base_head);
    }

    #[test]
    fn test_prepare_initial_files_commit_is_parentless_and_does_not_advance_refs() {
        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();
        let ambient_head = repo.current_head().unwrap();
        let files = vec![("docs/first.md".to_string(), b"first\n".to_vec())];

        let prepared_head = prepare_initial_files_commit(
            &repo,
            InitialFilesCommit {
                files: &files,
                message: "Initialize docs",
                author: &author,
            },
        )
        .unwrap();

        assert_eq!(repo.current_head().unwrap(), ambient_head);
        assert_eq!(
            repo.show_file(&prepared_head, "docs/first.md").unwrap(),
            b"first\n"
        );
        let raw = git2::Repository::open(dir.path()).unwrap();
        let prepared = raw
            .find_commit(git2::Oid::from_str(&prepared_head).unwrap())
            .unwrap();
        assert_eq!(prepared.parent_count(), 0);
        assert!(repo.commit_is_parentless(&prepared_head).unwrap());
    }

    #[test]
    fn initial_file_batches_validate_before_writing_objects_or_losing_entries() {
        let (repo, dir) = seed_repo();
        let base = repo.current_head().unwrap();
        let author = GitAuthor::test_author();
        std::fs::write(dir.path().join("README.md"), b"unrelated dirty bytes").unwrap();
        let raw = git2::Repository::open(dir.path()).unwrap();
        let index = std::fs::read(dir.path().join(".git/index")).unwrap();
        let objects = || {
            let mut ids = std::collections::BTreeSet::new();
            raw.odb()
                .unwrap()
                .foreach(|oid| {
                    ids.insert(oid.to_string());
                    true
                })
                .unwrap();
            ids
        };
        let before = objects();
        let mut batches = Vec::new();
        for invalid in [
            ".git/config",
            "nested/.GIT/config",
            "bad\0name",
            "a//b",
            "./a",
            "../escape",
            ".g5/runtime.json",
        ] {
            batches.push(vec![
                ("okay".into(), b"valid-first-unique".to_vec()),
                (invalid.into(), b"invalid-later-unique".to_vec()),
            ]);
        }
        for (first, second) in [("a", "a/b"), ("a/b", "a"), ("same", " same ")] {
            batches.push(vec![
                (first.into(), b"first".to_vec()),
                (second.into(), b"second".to_vec()),
            ]);
        }
        batches.push(Vec::new());
        for files in batches {
            assert!(
                matches!(
                    prepare_initial_files_commit(
                        &repo,
                        InitialFilesCommit {
                            files: &files,
                            message: "invalid batch",
                            author: &author,
                        }
                    ),
                    Err(G5Error::ValidationError { .. })
                ),
                "{files:?}"
            );
            assert_eq!(objects(), before);
            assert_eq!(repo.current_head().unwrap(), base);
            assert_eq!(std::fs::read(dir.path().join(".git/index")).unwrap(), index);
            assert_eq!(
                std::fs::read(dir.path().join("README.md")).unwrap(),
                b"unrelated dirty bytes"
            );
        }
        let files = [
            (" README.md ".into(), b"initial".to_vec()),
            ("nested/:(glob)*.txt".into(), b"literal".to_vec()),
        ];
        let candidate = prepare_initial_files_commit(
            &repo,
            InitialFilesCommit {
                files: &files,
                message: "valid initial batch",
                author: &author,
            },
        )
        .unwrap();
        let commit = raw
            .find_commit(git2::Oid::from_str(&candidate).unwrap())
            .unwrap();
        assert_eq!(commit.parent_count(), 0);
        for (path, content) in files {
            assert_eq!(repo.show_file(&candidate, path.trim()).unwrap(), content);
        }
        assert_eq!(repo.current_head().unwrap(), base);
        assert_eq!(std::fs::read(dir.path().join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read(dir.path().join("README.md")).unwrap(),
            b"unrelated dirty bytes"
        );
    }

    #[test]
    fn test_prepare_initial_empty_commit_is_parentless_and_does_not_advance_refs() {
        let (repo, dir) = seed_repo();
        let author = GitAuthor::test_author();
        let ambient_head = repo.current_head().unwrap();

        let prepared_head =
            prepare_initial_empty_commit(&repo, "Initialize empty docs", &author).unwrap();

        assert_eq!(repo.current_head().unwrap(), ambient_head);
        let raw = git2::Repository::open(dir.path()).unwrap();
        let prepared = raw
            .find_commit(git2::Oid::from_str(&prepared_head).unwrap())
            .unwrap();
        assert_eq!(prepared.parent_count(), 0);
        assert!(repo.commit_is_parentless(&prepared_head).unwrap());
        assert_eq!(prepared.tree().unwrap().len(), 0);
    }
}
