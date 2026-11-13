#![cfg(feature = "libgit2")]
use g5_scm::{source_control_patch, GitRepo};
use std::{path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@g5.test")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@g5.test")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", message]);
    git(root, &["rev-parse", "HEAD"])
}
#[test]
fn source_control_net_reversions_renames_and_grouped_commit_endpoints() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    std::fs::write(root.join("file"), "base\n").unwrap();
    let base = commit(root, "base");
    std::fs::write(root.join("file"), "first\n").unwrap();
    let first = commit(root, "first");
    std::fs::write(
        root.join("skipped"),
        "must not leak into selected patches\n",
    )
    .unwrap();
    let _skipped = commit(root, "skip");
    std::fs::write(root.join("file"), "base\n").unwrap();
    let last = commit(root, "revert");
    let repo = GitRepo::open(root).unwrap();
    let net = repo.source_control_diff(&base, &last).unwrap();
    assert_eq!(net.len(), 1);
    assert_eq!(net[0].path, "skipped");
    for selected in [&first, &last] {
        let patch = repo.commit_diff_summary(&last, selected).unwrap();
        assert_eq!(patch.files.len(), 1);
        assert_eq!(patch.files[0].canonical_path(), "file");
    }
    std::fs::rename(root.join("file"), root.join("renamed")).unwrap();
    let renamed = commit(root, "rename");
    let net = repo.source_control_diff(&last, &renamed).unwrap();
    assert_eq!(net.len(), 1);
    assert_eq!(net[0].old_path.as_deref(), Some("file"));
    assert_eq!(net[0].path, "renamed");
    // Path-oriented consumers need both endpoints; Source Control totals use
    // one rename identity without changing those consumers' summary contract.
    assert_eq!(repo.diff_summary(&last, &renamed).unwrap().files.len(), 2);
    let summary = repo.source_control_summary(&last, &renamed).unwrap();
    assert_eq!(summary.files.len(), 1);
    assert_eq!(summary.files[0].path, "renamed");
    let patch = git(root, &["diff", "-M", &last, &renamed]);
    let decoded = source_control_patch(format!("{patch}\n").as_bytes()).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].path, "renamed");
}
#[test]
fn source_control_history_contains_merge_ancestry_and_binary_deletion_patches() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    std::fs::write(root.join("base"), "base\n").unwrap();
    let base = commit(root, "base");
    git(root, &["checkout", "-b", "side"]);
    std::fs::write(root.join("side"), "side\n").unwrap();
    let side = commit(root, "side");
    git(root, &["checkout", "main"]);
    std::fs::write(root.join("main"), "main\n").unwrap();
    commit(root, "main");
    git(root, &["merge", "--no-ff", "-m", "merge", "side"]);
    let merged = git(root, &["rev-parse", "HEAD"]);
    let repo = GitRepo::open(root).unwrap();
    let (commits, truncated) = repo.source_control_commits(&base, &merged, 250).unwrap();
    assert!(!truncated);
    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].parents.len(), 2);
    assert!(commits.iter().any(|c| c.sha == side));
    assert!(
        repo.commit_diff_summary(&merged, &merged)
            .unwrap()
            .first_parent_diff
    );
    assert_eq!(
        repo.commit_diff_summary(&merged, &side).unwrap().files[0].canonical_path(),
        "side"
    );
    assert!(repo.source_control_commits(&base, &merged, 1).unwrap().1);
    std::fs::remove_file(root.join("base")).unwrap();
    std::fs::write(root.join("binary"), [0, 255]).unwrap();
    let tip = commit(root, "binary/delete");
    let files = repo.source_control_diff(&merged, &tip).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .any(|f| matches!(f.diff.status, g5_scm::CommitFileDiffStatus::Binary)));
    let raw = git(root, &["diff", &merged, &tip]);
    let decoded = source_control_patch(format!("{raw}\n").as_bytes()).unwrap();
    assert!(decoded
        .iter()
        .any(|f| matches!(f.diff.status, g5_scm::CommitFileDiffStatus::Binary)));
}

#[test]
fn oversized_committed_blobs_keep_other_files_and_activity_summary_available() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    std::fs::write(root.join("ordinary.txt"), "base\n").unwrap();
    std::fs::write(root.join(".gitattributes"), "large.txt diff\n").unwrap();
    let base = commit(root, "base");
    let asset = root.join("large.txt");
    {
        // ASCII forces the summary to respect the content ceiling rather
        // than accidentally passing because NUL bytes mark the file binary.
        let mut file = std::fs::File::create(&asset).unwrap();
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..32 {
            file.write_all(&chunk).unwrap();
        }
        file.write_all(b"x").unwrap();
    }
    std::fs::write(root.join("ordinary.txt"), "asset added\n").unwrap();
    let added = commit(root, "add oversized asset");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&asset)
        .unwrap()
        .write_all(b"modified")
        .unwrap();
    std::fs::write(root.join("ordinary.txt"), "asset modified\n").unwrap();
    let modified = commit(root, "modify oversized asset");
    std::fs::remove_file(&asset).unwrap();
    std::fs::write(root.join("ordinary.txt"), "asset deleted\n").unwrap();
    let deleted = commit(root, "delete oversized asset");
    let repo = GitRepo::open(root).unwrap();

    for (before, after, change, content) in [
        (&base, &added, g5_scm::DiffFileStatus::Added, "asset added"),
        (
            &added,
            &modified,
            g5_scm::DiffFileStatus::Modified,
            "asset modified",
        ),
        (
            &modified,
            &deleted,
            g5_scm::DiffFileStatus::Deleted,
            "asset deleted",
        ),
    ] {
        for files in [
            repo.source_control_diff(before, after).unwrap(),
            repo.source_control_commit_diff(&deleted, after).unwrap(),
        ] {
            assert_eq!(files.len(), 2);
            let large = files.iter().find(|file| file.path == "large.txt").unwrap();
            assert_eq!(large.change, change);
            assert!(matches!(
                large.diff.status,
                g5_scm::CommitFileDiffStatus::TooLarge
            ));
            assert!(large.diff.hunks.is_empty());
            let ordinary = files
                .iter()
                .find(|file| file.path == "ordinary.txt")
                .unwrap();
            assert!(matches!(
                ordinary.diff.status,
                g5_scm::CommitFileDiffStatus::Text
            ));
            assert!(ordinary
                .diff
                .hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .any(|line| line.content == content));
        }
        let summary = repo.source_control_summary(before, after).unwrap();
        assert_eq!(summary.files.len(), 2);
        let large = summary
            .files
            .iter()
            .find(|file| file.path == "large.txt")
            .unwrap();
        assert_eq!(large.status, change);
        assert_eq!((large.additions, large.deletions), (0, 0));
        assert_eq!((summary.total_additions, summary.total_deletions), (1, 1));
    }
}

#[test]
fn gitlink_changes_do_not_require_submodule_objects_in_the_superproject() {
    let dependency = tempfile::tempdir().unwrap();
    git(dependency.path(), &["init", "-b", "main"]);
    std::fs::write(dependency.path().join("dependency.txt"), "first\n").unwrap();
    let first = commit(dependency.path(), "first dependency commit");
    std::fs::write(dependency.path().join("dependency.txt"), "second\n").unwrap();
    let second = commit(dependency.path(), "second dependency commit");

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    std::fs::write(root.join("ordinary.txt"), "base\n").unwrap();
    let base = commit(root, "base");
    let mut commits = vec![base];
    for (pointer, content) in [
        (Some(&first), "dependency added"),
        (Some(&second), "dependency modified"),
        (None, "dependency deleted"),
    ] {
        std::fs::write(root.join("ordinary.txt"), format!("{content}\n")).unwrap();
        git(root, &["add", "ordinary.txt"]);
        if let Some(pointer) = pointer {
            git(
                root,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("160000,{pointer},dependency"),
                ],
            );
        } else {
            git(root, &["update-index", "--force-remove", "dependency"]);
        }
        // Commit only the staged gitlink, without checking out or fetching
        // the dependency into this repository's object database.
        git(root, &["commit", "-qm", content]);
        commits.push(git(root, &["rev-parse", "HEAD"]));
    }
    let raw_repo = git2::Repository::open(root).unwrap();
    for pointer in [&first, &second] {
        assert!(!raw_repo
            .odb()
            .unwrap()
            .exists(git2::Oid::from_str(pointer).unwrap()));
    }
    let repo = GitRepo::open(root).unwrap();
    let tip = commits.last().unwrap();
    for ((before, after), (change, content, old_pointer, new_pointer)) in
        commits.windows(2).map(|pair| (&pair[0], &pair[1])).zip([
            (
                g5_scm::DiffFileStatus::Added,
                "dependency added",
                None,
                Some(&first),
            ),
            (
                g5_scm::DiffFileStatus::Modified,
                "dependency modified",
                Some(&first),
                Some(&second),
            ),
            (
                g5_scm::DiffFileStatus::Deleted,
                "dependency deleted",
                Some(&second),
                None,
            ),
        ])
    {
        for files in [
            repo.source_control_diff(before, after).unwrap(),
            repo.source_control_commit_diff(tip, after).unwrap(),
        ] {
            assert_eq!(files.len(), 2);
            let dependency = files.iter().find(|file| file.path == "dependency").unwrap();
            assert_eq!(dependency.change, change);
            let lines = dependency
                .diff
                .hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .collect::<Vec<_>>();
            for (pointer, line_type) in [
                (old_pointer, g5_scm::CommitDiffLineType::Deletion),
                (new_pointer, g5_scm::CommitDiffLineType::Addition),
            ] {
                if let Some(pointer) = pointer {
                    assert!(lines.iter().any(|line| {
                        std::mem::discriminant(&line.line_type)
                            == std::mem::discriminant(&line_type)
                            && line.content == format!("Subproject commit {pointer}")
                    }));
                }
            }
            let ordinary = files
                .iter()
                .find(|file| file.path == "ordinary.txt")
                .unwrap();
            assert!(ordinary
                .diff
                .hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .any(|line| line.content == content));
        }
        let summary = repo.source_control_summary(before, after).unwrap();
        assert_eq!(summary.files.len(), 2);
        assert_eq!(
            summary
                .files
                .iter()
                .find(|file| file.path == "dependency")
                .unwrap()
                .status,
            change
        );
    }
}
