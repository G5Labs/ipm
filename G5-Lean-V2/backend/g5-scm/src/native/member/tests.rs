use super::*;
use crate::native::tests::{commit, git};

fn environment(root: &Path) -> MemberGitEnvironment {
    MemberGitEnvironment {
        search_path: std::env::var_os("PATH").unwrap(),
        home: root.into(),
        global_config: PathBuf::from("/dev/null"),
        workspace_socket: root.join("socket"),
        author: GitAuthor {
            name: "Member owner".into(),
            email: "member@example.test".into(),
        },
        uid: None,
        gid: None,
        library_path: None,
        dyld_library_path: None,
    }
}
fn member(root: &Path) -> MemberGit {
    MemberGit::new(
        environment(root),
        None,
        1024 * 1024,
        NativeCancellation::default(),
    )
    .unwrap()
}
#[tokio::test]
async fn checkpoint_cas_preserves_selected_staging_and_whole_tree_semantics() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    std::fs::write(root.join("selected\nfile"), "base\n").unwrap();
    std::fs::write(root.join("unrelated"), "base\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "base");
    std::fs::write(root.join("unrelated"), "staged\n").unwrap();
    git(root, &["add", "unrelated"]);
    std::fs::write(root.join("selected\nfile"), "selected\n").unwrap();
    let git_ops = member(root);
    let request = MemberCheckpoint {
        repository: root.into(),
        branch: "shared".into(),
        message: "selected".into(),
        paths: Some(vec!["selected\nfile".into()]),
        expected_blobs: None,
    };
    let saved = git_ops.checkpoint(&request).await.unwrap();
    assert!(saved.committed);
    assert_eq!(saved.head, git(root, &["rev-parse", "HEAD"]));
    assert_eq!(git(root, &["show", "HEAD:unrelated"]), "base");
    assert_eq!(git(root, &["show", ":unrelated"]), "staged");
    assert_eq!(git(root, &["show", "HEAD:selected\nfile"]), "selected");
    let before = saved.head;
    let mut invalid = request.clone();
    invalid.branch = "foreign".into();
    assert!(git_ops.checkpoint(&invalid).await.is_err());
    assert_eq!(git(root, &["rev-parse", "HEAD"]), before);
    let whole = MemberCheckpoint {
        paths: None,
        message: "whole".into(),
        ..request
    };
    let saved = git_ops.checkpoint(&whole).await.unwrap();
    assert!(saved.committed);
    assert_eq!(git(root, &["show", "HEAD:unrelated"]), "staged");
    assert!(git_ops.status(root).await.unwrap().is_empty());
    // Native staged-only changes that were restored in the worktree become clean.
    std::fs::write(root.join("unrelated"), "temporary\n").unwrap();
    git(root, &["add", "unrelated"]);
    std::fs::write(root.join("unrelated"), "staged\n").unwrap();
    let clean = git_ops.checkpoint(&whole).await.unwrap();
    assert!(!clean.committed);
    assert_eq!(clean.head, saved.head);
    assert!(git_ops.status(root).await.unwrap().is_empty());
}
#[tokio::test]
async fn member_status_preserves_literal_names_and_refuses_unavailable_or_truncated_output() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    commit(root, "base");
    std::fs::write(root.join("unicode-über\nend "), "content").unwrap();
    let status = member(root).status(root).await.unwrap();
    assert_eq!(status[0].path, "unicode-über\nend ");
    let limited =
        MemberGit::new(environment(root), None, 4, NativeCancellation::default()).unwrap();
    assert_eq!(
        limited.status(root).await.unwrap_err().kind,
        FailureKind::InvalidOutput
    );
    let absent = root.join("absent");
    std::fs::create_dir(&absent).unwrap();
    assert!(member(root).status(&absent).await.is_err());
    let expired = MemberGit::new(
        environment(root),
        Some(Instant::now()),
        1024,
        NativeCancellation::default(),
    )
    .unwrap();
    let error = expired.head_commit(root).await.unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Deadline, FailureEffect::Refused)
    );
}
#[tokio::test]
async fn successful_member_leader_cannot_leave_descendants_that_closed_pipes() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let executable = root.join("git");
    let marker = root.join("escaped");
    std::fs::write(&executable, format!("#!/bin/sh\n(sleep 0.3; echo escaped > '{}') >/dev/null 2>&1 </dev/null &\nprintf '%040d\\n' 1\nexit 0\n", marker.display())).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = environment(root);
    env.search_path = std::env::join_paths([
        root.to_path_buf(),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/bin"),
    ])
    .unwrap();
    let runner = MemberGit::new(env, None, 1024, NativeCancellation::default()).unwrap();
    assert_eq!(runner.head_commit(root).await.unwrap().len(), 40);
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert!(
        !marker.exists(),
        "successful guest operation released surviving descendant"
    );
}

#[tokio::test]
async fn switch_back_refuses_option_names_and_preserves_literal_branch_effects() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    git(root, &["remote", "add", "origin", root.to_str().unwrap()]);
    std::fs::write(root.join("file"), "saved\n").unwrap();
    git(root, &["add", "file"]);
    commit(root, "saved");
    let head = git(root, &["rev-parse", "HEAD"]);
    for branch in ["-f", "HEAD", "topic"] {
        git(
            root,
            &["update-ref", &format!("refs/heads/{branch}"), &head],
        );
        git(
            root,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{branch}"),
                &head,
            ],
        );
    }
    git(
        root,
        &["update-ref", "refs/remotes/origin/new-local", &head],
    );
    std::fs::write(root.join("file"), "dirty\n").unwrap();
    let index = std::fs::read(root.join(".git/index")).unwrap();
    for branch in ["-f", "HEAD", "@{-1}"] {
        assert!(member(root).switch_back(root, branch).await.is_err());
        assert_eq!(git(root, &["symbolic-ref", "--short", "HEAD"]), "shared");
        assert_eq!(std::fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(
            std::fs::read_to_string(root.join("file")).unwrap(),
            "dirty\n"
        );
    }
    assert!(!member(root).switch_back(root, "topic").await.unwrap());
    assert_eq!(git(root, &["symbolic-ref", "--short", "HEAD"]), "topic");
    assert!(member(root).switch_back(root, "new-local").await.unwrap());
    assert_eq!(git(root, &["symbolic-ref", "--short", "HEAD"]), "new-local");
    assert_eq!(
        std::fs::read_to_string(root.join("file")).unwrap(),
        "dirty\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn dropped_checkpoint_cleans_private_index_before_releasing_custody() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("checkout");
    let bin = temp.path().join("bin");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&bin).unwrap();
    git(&root, &["init", "-b", "shared"]);
    std::fs::write(root.join("file"), "before").unwrap();
    git(&root, &["add", "file"]);
    commit(&root, "before");
    let head = git(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("file"), "after").unwrap();
    let marker = temp.path().join("started");
    let real_git =
        super::super::process::resolve_host_binary(&std::env::var_os("PATH").unwrap()).unwrap();
    std::fs::write(bin.join("git"), format!(
        "#!/bin/sh\ncase \"$*\" in *commit-tree*) echo started > '{}'; sleep 30;; esac\nexec '{}' \"$@\"\n",
        marker.display(), real_git.display()
    )).unwrap();
    std::fs::set_permissions(bin.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = environment(&root);
    env.search_path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&env.search_path)))
            .unwrap();
    let cancellation = NativeCancellation::default();
    let member = MemberGit::new(env, None, 1024 * 1024, cancellation.clone()).unwrap();
    let request = MemberCheckpoint {
        repository: root.clone(),
        branch: "shared".into(),
        message: "after".into(),
        paths: None,
        expected_blobs: None,
    };
    let task = tokio::spawn(async move { member.checkpoint(&request).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(root.join(".git/index.lock").exists());
    task.abort();
    let _ = task.await;
    tokio::time::timeout(Duration::from_secs(5), cancellation.wait_idle())
        .await
        .unwrap();
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert!(!root.join(".git/index.lock").exists());
    for entry in std::fs::read_dir(root.join(".git")).unwrap() {
        assert!(!entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("index.g5-"));
    }
    git(&root, &["add", "file"]);
}

#[tokio::test]
async fn review_comparisons_distinguish_committed_dirty_untracked_and_deleted_without_mutating() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    std::fs::write(root.join("changed"), "original\n\n").unwrap();
    std::fs::write(root.join("deleted"), "old\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "pushed");
    let pushed = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("committed"), "local commit\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "unpushed");
    let head = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("changed"), "dirty\n").unwrap();
    std::fs::write(root.join("new name\nwith newline"), "new\n").unwrap();
    std::fs::remove_file(root.join("deleted")).unwrap();
    let before_status = git(root, &["status", "--porcelain"]);
    let ops = member(root);
    let dirty = ops
        .review_inspect(root, "uncommitted", &pushed, Some("changed"))
        .await
        .unwrap();
    assert_eq!(dirty["before"], "original\n\n");
    assert_eq!(dirty["beforeCommit"], head);
    assert!(!dirty["paths"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("committed")));
    let unpushed = ops
        .review_inspect(root, "unpushed", &pushed, None)
        .await
        .unwrap();
    assert_eq!(unpushed["beforeCommit"], pushed);
    for name in ["committed", "changed", "deleted", "new name\nwith newline"] {
        assert!(unpushed["paths"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(name)));
    }
    let all = ops
        .review_inspect(root, "all", &pushed, None)
        .await
        .unwrap();
    assert!(!all["paths"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("deleted")));
    assert!(ops
        .review_inspect(root, "unpushed", &pushed, Some("../outside"))
        .await
        .is_err());
    assert!(ops
        .review_inspect(root, "arbitrary-ref", &pushed, None)
        .await
        .is_err());
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(root, &["status", "--porcelain"]), before_status);
}

#[tokio::test]
async fn retained_review_commit_survives_rewrite_and_gc_until_workspace_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    commit(root, "base");
    let base = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("reviewed"), "reviewed bytes\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "reviewed");
    let reviewed = git(root, &["rev-parse", "HEAD"]);
    let workspace = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();
    let repo = crate::GitRepo::open(root).unwrap();
    repo.retain_review_commit(workspace, &reviewed).unwrap();
    repo.retain_review_commit(workspace, &reviewed).unwrap();
    repo.retain_review_commit(other, &base).unwrap();
    assert_eq!(git(root, &["rev-parse", "HEAD"]), reviewed);
    git(root, &["reset", "--hard", &base]);
    git(root, &["update-ref", "-d", "ORIG_HEAD"]);
    git(root, &["reflog", "expire", "--expire=now", "--all"]);
    git(root, &["gc", "--prune=now"]);
    assert_eq!(
        git(root, &["show", &format!("{reviewed}:reviewed")]),
        "reviewed bytes"
    );
    repo.delete_workspace_refs("shared", workspace, &[])
        .unwrap();
    assert_eq!(
        git(
            root,
            &[
                "for-each-ref",
                "--format=%(refname)",
                &format!("refs/g5/review-evidence/{workspace}")
            ]
        ),
        ""
    );
    assert!(!git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)",
            &format!("refs/g5/review-evidence/{other}")
        ]
    )
    .is_empty());
}

#[tokio::test]
async fn source_control_includes_empty_files_with_git_quoted_paths_before_and_after_commit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    std::fs::write(root.join("base"), "base\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "base");
    let pushed = git(root, &["rev-parse", "HEAD"]);
    let mut names = vec!["empty.md", "space b/path.md", "quoted\"tab\tü.md"];
    names.sort();
    for name in &names {
        std::fs::create_dir_all(root.join(name).parent().unwrap()).unwrap();
        std::fs::write(root.join(name), "").unwrap();
    }
    std::fs::write(root.join("text.md"), "NONEMPTY CONTENT\n").unwrap();
    let mut expected = names.clone();
    expected.push("text.md");
    expected.sort();
    let member = member(root);
    let state = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    assert_eq!(state["paths"], serde_json::json!(expected));
    let decode = |diff: serde_json::Value| {
        let patch = diff["patches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect::<String>();
        crate::source_control_patch(patch.as_bytes()).unwrap()
    };
    for committed in [false, true] {
        if committed {
            git(root, &["add", "-A"]);
            commit(root, "empty files");
        }
        let files = decode(
            member
                .review_inspect(root, "source_diff", &pushed, None)
                .await
                .unwrap(),
        );
        assert_eq!(files.len(), expected.len());
        let mut paths = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths, expected);
        assert!(files
            .iter()
            .all(|file| matches!(file.change, crate::DiffFileStatus::Added)
                && matches!(file.diff.status, crate::CommitFileDiffStatus::Text)
                && (file.path == "text.md" || file.diff.hunks.is_empty())));
    }
    assert!(decode(
        member
            .review_inspect(root, "source_diff", &pushed, None)
            .await
            .unwrap()
    )
    .iter()
    .find(|file| file.path == "text.md")
    .unwrap()
    .diff
    .hunks
    .iter()
    .flat_map(|hunk| &hunk.lines)
    .any(|line| line.content == "NONEMPTY CONTENT"));
    let committed = git(root, &["rev-parse", "HEAD"]);
    for name in &expected {
        std::fs::remove_file(root.join(name)).unwrap();
    }
    let files = decode(
        member
            .review_inspect(root, "source_diff", &committed, None)
            .await
            .unwrap(),
    );
    assert_eq!(files.len(), expected.len());
    assert!(files
        .iter()
        .all(|file| matches!(file.change, crate::DiffFileStatus::Deleted)
            && matches!(file.diff.status, crate::CommitFileDiffStatus::Text)
            && (file.path == "text.md" || file.diff.hunks.is_empty())));
    assert!(
        crate::source_control_patch(
            b"diff --git a/file b/file\nnew file mode 100644\nindex 0000000..aabbccd\n"
        )
        .is_err(),
        "a truncated nonempty patch must not be interpreted as an empty file"
    );
}

#[tokio::test]
async fn source_control_checkpoint_detects_repeated_dirty_edits_staging_and_local_publication() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    std::fs::write(root.join("file.txt"), "base\n").unwrap();
    std::fs::write(root.join(".gitignore"), "ignored\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "base");
    let pushed = git(root, &["rev-parse", "HEAD"]);
    let member = member(root);
    let first = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    assert!(first["paths"].as_array().unwrap().is_empty());
    std::fs::write(root.join("file.txt"), "one!\n").unwrap();
    let dirty = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    std::fs::write(root.join("file.txt"), "two!\n").unwrap();
    let repeated = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    assert_ne!(first["fingerprint"], dirty["fingerprint"]);
    assert_ne!(dirty["fingerprint"], repeated["fingerprint"]);
    git(root, &["add", "file.txt"]);
    let staged = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    assert_ne!(repeated["fingerprint"], staged["fingerprint"]);
    std::fs::write(root.join("new.txt"), "new\n").unwrap();
    std::fs::write(root.join("ignored"), "secret\n").unwrap();
    let with_new = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    assert_eq!(
        with_new["paths"],
        serde_json::json!(["file.txt", "new.txt"])
    );
    let diff = member
        .review_inspect(root, "source_diff", &pushed, None)
        .await
        .unwrap();
    assert_eq!(diff["patches"].as_array().unwrap().len(), 2);
    assert!(diff["patches"][0].as_str().unwrap().ends_with('\n'));
    git(root, &["add", "-A"]);
    commit(root, "local only");
    let local = member
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    assert_eq!(local["localCommits"].as_array().unwrap().len(), 1);
    assert_eq!(local["paths"].as_array().unwrap().len(), 2);
    let published = git(root, &["rev-parse", "HEAD"]);
    let clean = member
        .review_inspect(root, "source_state", &published, None)
        .await
        .unwrap();
    assert!(clean["localCommits"].as_array().unwrap().is_empty());
    assert!(clean["paths"].as_array().unwrap().is_empty());
    assert_eq!(
        clean,
        member
            .review_inspect(root, "source_state", &published, None)
            .await
            .unwrap()
    );
    std::fs::write(root.join("file.txt"), "staged different\n").unwrap();
    git(root, &["add", "file.txt"]);
    std::fs::write(root.join("file.txt"), "two!\n").unwrap();
    let index_only = member
        .review_inspect(root, "source_state", &published, None)
        .await
        .unwrap();
    assert_eq!(index_only["dirty"], true);
    assert!(index_only["paths"].as_array().unwrap().is_empty());
    assert_ne!(index_only["fingerprint"], clean["fingerprint"]);
    // A local rewrite sharing the old base cannot be stacked onto the newer
    // pushed tip as if it were additional branch work.
    git(root, &["reset", "--hard", &pushed]);
    std::fs::write(root.join("file.txt"), "rewritten locally\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "local rewrite");
    let diverged = member
        .review_inspect(root, "source_state", &published, None)
        .await
        .unwrap();
    assert_eq!(diverged["selectable"], false);
    assert!(diverged["unavailableReason"]
        .as_str()
        .unwrap()
        .contains("does not descend"));
}

#[cfg(unix)]
#[tokio::test]
async fn source_control_show_accepts_new_local_work_but_refuses_edits_during_its_read() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "shared"]);
    std::fs::write(root.join("file.txt"), "base\n").unwrap();
    git(root, &["add", "-A"]);
    commit(root, "base");
    let pushed = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("file.txt"), "first edit\n").unwrap();
    member(root)
        .review_inspect(root, "source_state", &pushed, None)
        .await
        .unwrap();
    std::fs::write(root.join("file.txt"), "latest edit\n").unwrap();
    let shown = member(root)
        .review_inspect(root, "source_diff", &pushed, None)
        .await
        .unwrap();
    assert!(shown["patches"].to_string().contains("latest edit"));
    assert_eq!(shown["stateVerified"], true);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), pushed);
    assert_eq!(git(root, &["diff", "--cached"]), "");

    // An independent writer edits an already-dirty file after Git emits the
    // patch. Its status stays M; the request-local content check must refuse it.
    let shim = tempfile::tempdir().unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let program = shim.path().join("git");
    std::fs::write(&program, format!("#!/bin/sh\n'{}' \"$@\"\nresult=$?\ncase \" $* \" in *' --src-prefix=a/ '*) printf 'raced edit\\n' > '{}/file.txt';; esac\nexit $result\n", real_git.display(), root.display())).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = environment(root);
    env.search_path = shim.path().as_os_str().to_owned();
    let reader = MemberGit::new(env, None, 1024 * 1024, NativeCancellation::default()).unwrap();
    let error = reader
        .review_inspect(root, "source_diff", &pushed, None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Files changed"), "{error}");
    assert_eq!(
        std::fs::read_to_string(root.join("file.txt")).unwrap(),
        "raced edit\n"
    );
    assert_eq!(git(root, &["rev-parse", "HEAD"]), pushed);
    assert_eq!(git(root, &["diff", "--cached"]), "");
}
