use super::*;
use std::process::Command;

pub(super) fn git(repository: &Path, args: &[&str]) -> String {
    let output = Command::new(process::resolve_binary().unwrap())
        .arg("-C")
        .arg(repository)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Native test")
        .env("GIT_AUTHOR_EMAIL", "native@example.com")
        .env("GIT_COMMITTER_NAME", "Native test")
        .env("GIT_COMMITTER_EMAIL", "native@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub(super) fn commit(repository: &Path, message: &str) -> String {
    git(repository, &["commit", "--allow-empty", "-m", message]);
    git(repository, &["rev-parse", "HEAD"])
}

#[tokio::test]
async fn review_snapshot_overflow_returns_actionable_read_guidance() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--initial-branch=main"]);
    std::fs::write(dir.path().join("large.txt"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    git(dir.path(), &["add", "large.txt"]);
    let head = commit(dir.path(), "large review input");
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    let error = runner
        .read_review_snapshot(
            dir.path(),
            ReviewSnapshotRead::File {
                commit: &head,
                path: "large.txt",
            },
        )
        .await
        .unwrap_err();
    assert!(error.detail.contains("select a smaller path"), "{error}");
}

#[tokio::test]
async fn clone_fetch_and_push_preserve_scope_and_exact_leases() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let initial = commit(&source, "initial");
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    let checkout = dir.path().join("checkout");
    let url = remote.to_str().unwrap();
    runner
        .clone_checkout(url, &checkout, Some("main"), &GitAuth::None)
        .await
        .unwrap();
    let next = commit(&checkout, "next");
    git(&checkout, &["tag", "-a", "do-not-push", "-m", "tag"]);
    git(&checkout, &["config", "push.followTags", "true"]);
    let push = BranchPush {
        repository: checkout.clone(),
        remote: "origin".into(),
        expected_url: url.into(),
        source: PushSource::Commit(next.clone()),
        branch: "main".into(),
        update: PushUpdate::Lease {
            expected: Some(initial.clone()),
        },
    };
    assert_eq!(
        runner
            .push_branch(push.clone(), &GitAuth::None)
            .await
            .unwrap(),
        PushOutcome::Updated
    );
    assert_eq!(git(&remote, &["rev-parse", "main"]), next);
    assert_eq!(git(&remote, &["tag", "--list"]), "");
    assert_eq!(
        runner
            .push_branch(push.clone(), &GitAuth::None)
            .await
            .unwrap(),
        PushOutcome::Unchanged
    );
    let third = commit(&checkout, "third");
    let stale = BranchPush {
        source: PushSource::Commit(third.clone()),
        ..push.clone()
    };
    let error = runner.push_branch(stale, &GitAuth::None).await.unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Rejected, FailureEffect::Refused)
    );
    assert_eq!(git(&remote, &["rev-parse", "main"]), next);
    let create = BranchPush {
        source: PushSource::Branch("main".into()),
        branch: "new".into(),
        update: PushUpdate::Lease { expected: None },
        ..push.clone()
    };
    runner.push_branch(create, &GitAuth::None).await.unwrap();
    assert_eq!(git(&remote, &["rev-parse", "new"]), third);
    let refuse_absent = BranchPush {
        source: PushSource::Commit(third),
        update: PushUpdate::Lease { expected: None },
        ..push.clone()
    };
    assert_eq!(
        runner
            .push_branch(refuse_absent, &GitAuth::None)
            .await
            .unwrap_err()
            .kind,
        FailureKind::Rejected
    );
    runner
        .fetch_checkout(&checkout, "origin", url, &GitAuth::None)
        .await
        .unwrap();
    assert_eq!(
        git(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
        next
    );
    let branch = BranchPush {
        source: PushSource::Branch("main".into()),
        update: PushUpdate::FastForward,
        ..push
    };
    runner
        .push_branch(branch.clone(), &GitAuth::None)
        .await
        .unwrap();

    // A remote policy refusal leaves the destination unchanged but is not a
    // ref conflict: refreshing and retrying cannot satisfy it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let hook = remote.join("hooks/pre-receive");
        std::fs::write(&hook, "#!/bin/sh\necho 'policy says no' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let before = git(&remote, &["rev-parse", "main"]);
        let fourth = commit(&checkout, "fourth");
        let error = runner
            .push_branch(
                BranchPush {
                    source: PushSource::Commit(fourth),
                    ..branch
                },
                &GitAuth::None,
            )
            .await
            .unwrap_err();
        assert_eq!(
            (error.kind, error.effect),
            (FailureKind::Exit, FailureEffect::Refused),
            "{error}"
        );
        assert!(error.detail.contains("[remote rejected]"), "{error}");
        assert_eq!(git(&remote, &["rev-parse", "main"]), before);
    }
}

#[test]
fn push_refusals_distinguish_ref_conflicts_from_remote_policy() {
    for summary in [
        "[rejected] (non-fast-forward)",
        "[rejected] (fetch first)",
        "[rejected] (stale info)",
        "[rejected] (already exists)",
        "[remote rejected] (failed to update ref)",
        "[remote rejected] (atomic transaction failed)",
    ] {
        assert_eq!(
            push_refusal(summary),
            Some(PushRefusal::RefConflict),
            "{summary}"
        );
    }
    for summary in [
        "[remote rejected] (pre-receive hook declined)",
        "[remote rejected] (Working directory has unstaged changes)",
        "[remote rejected] (unpacker error)",
        "[remote rejected] (deny updating a hidden ref)",
    ] {
        assert_eq!(
            push_refusal(summary),
            Some(PushRefusal::Policy),
            "{summary}"
        );
    }
    assert_eq!(push_refusal("[up to date]"), None);
    // GitHub's workflow-file protection names the token kind it refused.
    for (summary, permission) in [
        (
            "[remote rejected] (refusing to allow a GitHub App to create or update workflow `.github/workflows/ci.yml` without `workflows` permission)",
            ProviderPermission::GithubAppWorkflows,
        ),
        (
            "[remote rejected] (refusing to allow an OAuth App to create or update workflow `.github/workflows/ci.yml` without `workflow` scope)",
            ProviderPermission::GithubWorkflowScope,
        ),
        (
            "[remote rejected] (refusing to allow a Personal Access Token to create or update workflow `.github/workflows/ci.yml` without `workflow` scope)",
            ProviderPermission::GithubWorkflowScope,
        ),
    ] {
        assert_eq!(
            push_refusal(summary),
            Some(PushRefusal::ProviderPermission(permission)),
            "{summary}"
        );
    }
}

/// `g5.git.workflow-files.refusal`: a push GitHub refuses for a missing workflow
/// permission is a typed, refused, client-visible publication error; the
/// same push succeeds once the token carries the permission.
#[tokio::test]
async fn workflow_permission_refusal_is_typed_actionable_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    commit(&source, "initial");
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    emulate_github_workflow_permission_for_tests(&remote);
    let url = remote.to_str().unwrap();
    git(&source, &["remote", "add", "origin", url]);
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    let push = |oid: &str, branch: &str, update: PushUpdate| BranchPush {
        repository: source.clone(),
        remote: "origin".into(),
        expected_url: url.into(),
        source: PushSource::Commit(oid.to_owned()),
        branch: branch.into(),
        update,
    };

    std::fs::write(source.join("notes.txt"), "ordinary\n").unwrap();
    git(&source, &["add", "notes.txt"]);
    let ordinary = commit(&source, "ordinary change");
    runner
        .push_branch(
            push(&ordinary, "feature", PushUpdate::Lease { expected: None }),
            &GitAuth::None,
        )
        .await
        .expect("ordinary pushes need no workflow permission");
    assert_eq!(git(&remote, &["rev-parse", "feature"]), ordinary);

    std::fs::create_dir_all(source.join(".github/workflows")).unwrap();
    std::fs::write(source.join(".github/workflows/ci.yml"), "on: push\n").unwrap();
    git(&source, &["add", ".github"]);
    let workflow = commit(&source, "workflow change");
    for update in [
        PushUpdate::FastForward,
        PushUpdate::Lease {
            expected: Some(ordinary.clone()),
        },
    ] {
        let error = runner
            .push_branch(push(&workflow, "feature", update), &GitAuth::None)
            .await
            .unwrap_err();
        assert_eq!(
            (error.kind, error.effect),
            (
                FailureKind::ProviderPermission(ProviderPermission::GithubAppWorkflows),
                FailureEffect::Refused
            ),
            "{error}"
        );
        assert_eq!(git(&remote, &["rev-parse", "feature"]), ordinary);
        let error = G5Error::from(error);
        assert_eq!(error.status_code(), 409, "{error}");
        let response = error.to_error_response("req".into());
        for expected in [
            "GitHub refused to publish branch feature",
            ".github/workflows/ci.yml",
            "lacks the Workflows permission",
            "Nothing was published",
            "Workflows: Read and write",
            "Refresh from GitHub",
        ] {
            assert!(response.message.contains(expected), "{}", response.message);
        }
        let G5Error::ConflictDetails { details, .. } = &error else {
            panic!("{error:?}");
        };
        assert_eq!(details["code"], "PROVIDER_PERMISSION_REQUIRED");
        assert_eq!(details["permission"], "workflows");
        assert_eq!(details["credential"], "github_app");
        assert_eq!(details["path"], ".github/workflows/ci.yml");
        assert_eq!(details["remoteBranch"], "feature");
        assert_eq!(details["repository"], url);
    }

    std::fs::write(
        remote.join(TEST_TOKEN_PERMISSIONS_FILE),
        r#"{"contents":"write","workflows":"write"}"#,
    )
    .unwrap();
    runner
        .push_branch(
            push(&workflow, "feature", PushUpdate::FastForward),
            &GitAuth::None,
        )
        .await
        .expect("a token with workflows write may publish workflow changes");
    assert_eq!(git(&remote, &["rev-parse", "feature"]), workflow);
}

#[tokio::test]
async fn destination_mismatch_multiple_pushurls_and_rewrites_refuse_before_push() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--initial-branch=main"]);
    let oid = commit(dir.path(), "initial");
    git(
        dir.path(),
        &["remote", "add", "origin", "https://expected.invalid/repo"],
    );
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    let push = BranchPush {
        repository: dir.path().into(),
        remote: "origin".into(),
        expected_url: "https://expected.invalid/repo".into(),
        source: PushSource::Commit(oid),
        branch: "main".into(),
        update: PushUpdate::FastForward,
    };
    git(
        dir.path(),
        &[
            "config",
            "remote.origin.pushurl",
            "https://other.invalid/repo",
        ],
    );
    let auth = GitAuth::Token("private-token".into());
    let error = runner.push_branch(push.clone(), &auth).await.unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Destination, FailureEffect::Refused)
    );
    git(
        dir.path(),
        &[
            "config",
            "remote.origin.pushurl",
            "https://expected.invalid/repo",
        ],
    );
    git(
        dir.path(),
        &[
            "config",
            "--add",
            "remote.origin.pushurl",
            "https://other.invalid/repo",
        ],
    );
    assert_eq!(
        runner
            .push_branch(push.clone(), &auth)
            .await
            .unwrap_err()
            .kind,
        FailureKind::Destination
    );
    git(
        dir.path(),
        &["config", "--unset-all", "remote.origin.pushurl"],
    );
    git(
        dir.path(),
        &[
            "config",
            "url.https://other.invalid/.insteadOf",
            "https://expected.invalid/",
        ],
    );
    assert_eq!(
        runner.push_branch(push, &auth).await.unwrap_err().kind,
        FailureKind::Destination
    );
    assert_eq!(
        git(dir.path(), &["rev-parse", "main"]),
        git(dir.path(), &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn normalized_ssh_identity_refuses_both_kinds_of_push_rewrite() {
    for identity in [
        "git@github.com:review/repo.git",
        "ssh://git@github.com/review/repo.git",
    ] {
        for rewrite in ["insteadOf", "pushInsteadOf"] {
            let dir = tempfile::tempdir().unwrap();
            let alternate = dir.path().join("alternate.git");
            git(dir.path(), &["init", "--initial-branch=main"]);
            let oid = commit(dir.path(), "initial");
            git(dir.path(), &["init", "--bare", alternate.to_str().unwrap()]);
            git(dir.path(), &["remote", "add", "origin", identity]);
            git(
                dir.path(),
                &[
                    "config",
                    &format!("url.{}.{rewrite}", alternate.display()),
                    "https://github.com/review/repo.git",
                ],
            );
            let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
            let auth = GitAuth::Token("private-token".into());
            let error = runner
                .push_branch(
                    BranchPush {
                        repository: dir.path().into(),
                        remote: "origin".into(),
                        expected_url: identity.into(),
                        source: PushSource::Commit(oid.clone()),
                        branch: "main".into(),
                        update: PushUpdate::Lease { expected: None },
                    },
                    &auth,
                )
                .await
                .unwrap_err();
            assert_eq!(
                (error.kind, error.effect),
                (FailureKind::Destination, FailureEffect::Refused),
                "{identity}: {rewrite}"
            );
            assert_eq!(git(&alternate, &["for-each-ref"]), "");
            let error = runner
                .delete_branch(
                    BranchDelete {
                        repository: dir.path().into(),
                        remote: "origin".into(),
                        expected_url: identity.into(),
                        branch: "main".into(),
                        expected: oid,
                    },
                    &auth,
                )
                .await
                .unwrap_err();
            assert_eq!(
                (error.kind, error.effect),
                (FailureKind::Destination, FailureEffect::Refused)
            );
            assert_eq!(git(dir.path(), &["config", "remote.origin.url"]), identity);
            assert_eq!(git(dir.path(), &["remote"]), "origin");
        }
    }
}

#[tokio::test]
async fn normalized_push_refuses_ambiguous_maximum_rewrite_ties() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let alternate = dir.path().join("alternate.git");
    let identity = "git@github.com:review/repo.git";
    let normalized = "https://github.com/review/repo.git";
    git(dir.path(), &["init", "--initial-branch=main"]);
    let oid = commit(dir.path(), "initial");
    git(dir.path(), &["init", "--bare", alternate.to_str().unwrap()]);
    git(dir.path(), &["remote", "add", "origin", identity]);
    // Git groups by the first occurrence of each URL base. A flat first-match
    // scan sees identity first, but Git sees the alternate base first.
    // git config --add regroups existing sections, which would erase this
    // counterexample. Preserve the physical interleaving explicitly.
    let alternate_quoted = alternate
        .to_str()
        .unwrap()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join(".git/config"))
        .unwrap();
    writeln!(config,
        "\n[url \"{alternate_quoted}\"]\n pushInsteadOf = unmatched:\n[url \"{normalized}\"]\n pushInsteadOf = {normalized}\n[url \"{alternate_quoted}\"]\n pushInsteadOf = {normalized}"
    ).unwrap();
    drop(config);
    assert_eq!(
        git(dir.path(), &["config", "--get-regexp", "^url\\..*\\.pushinsteadof$"]),
        format!(
            "url.{}.pushinsteadof unmatched:\nurl.{normalized}.pushinsteadof {normalized}\nurl.{}.pushinsteadof {normalized}",
            alternate.display(), alternate.display()
        )
    );
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    let error = runner
        .push_branch(
            BranchPush {
                repository: dir.path().into(),
                remote: "origin".into(),
                expected_url: identity.into(),
                source: PushSource::Commit(oid),
                branch: "main".into(),
                update: PushUpdate::Lease { expected: None },
            },
            &GitAuth::Token("private-token".into()),
        )
        .await
        .unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Destination, FailureEffect::Refused)
    );
    assert_eq!(git(&alternate, &["for-each-ref"]), "");
}

#[cfg(unix)]
pub(super) fn script(directory: &Path, source: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join("fake-git");
    std::fs::write(&path, format!("#!/bin/sh\n{source}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[cfg(unix)]
#[tokio::test]
async fn credentials_never_enter_argv_or_errors_and_output_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let executable = script(
        dir.path(),
        r#"
        for arg in "$@"; do
            case "$arg" in *private-token*|*Authorization*) exit 99;; esac
        done
        env | /usr/bin/grep '^GIT_CONFIG_' >&2
        /usr/bin/awk 'BEGIN { for (i=0; i<100000; i++) printf "x" }'
        echo private-token >&2
        exit 1
    "#,
    );
    let auth = GitAuth::Token("private-token".into());
    let mut command = Invocation::new(&executable, None, "test", Duration::from_secs(5));
    command
        .authenticate("https://example.invalid/repo", &auth)
        .unwrap();
    let output = command.execute().await.unwrap();
    assert_eq!(output.status.and_then(|status| status.code()), Some(1));
    assert!(output.truncated);
    assert_eq!(output.stdout_text().len(), 65536);
    let error = output.success("test", &auth).unwrap_err();
    assert_eq!(error.effect, FailureEffect::Unknown);
    let text = format!("{error:?} {error}");
    assert!(!text.contains("private-token"));
    assert!(!text.contains("eC1hY2Nlc3MtdG9rZW46cHJpdmF0ZS10b2tlbg=="));
    assert!(text.contains("[REDACTED]"));
    assert!(text.contains("http.https://example.invalid/repo.extraHeader"));
}

#[cfg(unix)]
async fn wait_for_file(path: &Path) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn push_outcomes_preserve_confirmed_refs_and_separate_stale_proxy_from_auth_failure() {
    for (stdout, stderr, exit, kind, effect) in [
        (
            "",
            "RPC failed; HTTP 409",
            1,
            FailureKind::Rejected,
            FailureEffect::Refused,
        ),
        (
            "",
            "RPC failed; HTTP 403",
            1,
            FailureKind::Exit,
            FailureEffect::Unknown,
        ),
        (
            "",
            "response lost",
            1,
            FailureKind::Exit,
            FailureEffect::Unknown,
        ),
        ("", "", 0, FailureKind::Io, FailureEffect::Unknown),
        (
            "*\\trefs/heads/main:refs/heads/main\\t[new branch]\\n",
            "RPC failed; HTTP 409",
            1,
            FailureKind::Exit,
            FailureEffect::Partial,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let executable = script(
            dir.path(),
            &format!(
                r#"
            for arg in "$@"; do
                case "$arg" in
                    check-ref-format) exit 0;;
                    remote) echo https://example.invalid/repo; exit 0;;
                    push) printf '{stdout}'; printf '%s' '{stderr}' >&2; exit {exit};;
                esac
            done
            exit 99
        "#
            ),
        );
        let runner = NativeGit {
            process_scope: None,
            binary: executable,
            timeout: Duration::from_secs(5),
            deadline: None,
            cancellation: NativeCancellation::default(),
            context: Context::Cli,
            bootstrap_test_root: None,
            hidden_protocol_refs: Vec::new(),
        };
        let error = runner
            .push_branch(
                BranchPush {
                    repository: dir.path().into(),
                    remote: "origin".into(),
                    expected_url: "https://example.invalid/repo".into(),
                    source: PushSource::Branch("main".into()),
                    branch: "main".into(),
                    update: PushUpdate::FastForward,
                },
                &GitAuth::None,
            )
            .await
            .unwrap_err();
        assert_eq!((error.kind, error.effect), (kind, effect));
        assert!(error.to_string().contains("refs/heads/main"));
    }
}

#[test]
fn truncated_credentials_are_redacted() {
    for diagnostic in ["fatal: invalid branch", "remote path", "token is absent"] {
        assert_eq!(
            process::redact(
                diagnostic,
                &["http://127.0.0.1:3214".into(), "token-secret".into()]
            ),
            diagnostic
        );
    }
    assert_eq!(
        process::redact("failed: tiny", &["tiny".into()]),
        "failed: [REDACTED]"
    );
    assert_eq!(
        process::redact("failed: private-to", &["private-token".into()]),
        "failed: [REDACTED]"
    );
}

#[cfg(unix)]
#[test]
fn receive_status_proxy() {
    use std::io::{Read, Write};
    use std::os::fd::FromRawFd;
    let Some(repository) = std::env::var_os("G5_TEST_RECEIVE_REPOSITORY") else {
        return;
    };
    let mut child = Command::new(process::resolve_binary().unwrap())
        .arg("receive-pack")
        .arg(repository)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin().lock(), &mut input);
    });
    let mut output = child.stdout.take().unwrap();
    // The test-only shell transfers its protocol stdout as fd 3, leaving the
    // libtest harness's own output redirected to /dev/null.
    let mut protocol = unsafe { std::fs::File::from_raw_fd(3) };
    let ack = b"ok refs/heads/main\n";
    let omitted =
        format!("{:04x}{}", ack.len() + 4, std::str::from_utf8(ack).unwrap()).into_bytes();
    loop {
        let mut header = [0; 4];
        if output.read_exact(&mut header).is_err() {
            break;
        }
        let count = usize::from_str_radix(std::str::from_utf8(&header).unwrap(), 16).unwrap();
        if count == 0 {
            protocol.write_all(&header).unwrap();
            continue;
        }
        let mut payload = vec![0; count - 4];
        output.read_exact(&mut payload).unwrap();
        if payload.first() == Some(&1) {
            if let Some(start) = payload
                .windows(omitted.len())
                .position(|part| part == omitted)
            {
                payload.drain(start..start + omitted.len());
            }
        }
        write!(protocol, "{:04x}", payload.len() + 4).unwrap();
        protocol.write_all(&payload).unwrap();
        protocol.flush().unwrap();
    }
    assert!(child.wait().unwrap().success());
}

#[cfg(unix)]
#[tokio::test]
async fn lost_per_ref_status_is_unknown_even_when_the_remote_really_updated() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("checkout");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch=main"]);
    let old = commit(&repository, "initial");
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--bare",
            repository.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(
        &repository,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    let desired = commit(&repository, "accepted but missing response");
    let executable = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .replace('\'', "'\\''");
    let proxy = script(dir.path(), &format!("export G5_TEST_RECEIVE_REPOSITORY=\"$1\"\nexec '{executable}' --exact native::tests::receive_status_proxy --nocapture 3>&1 >/dev/null"));
    git(
        &repository,
        &[
            "config",
            "remote.origin.receivepack",
            proxy.to_str().unwrap(),
        ],
    );
    let error = NativeGit::cli(Duration::from_secs(10))
        .unwrap()
        .push_branch(
            BranchPush {
                repository,
                remote: "origin".into(),
                expected_url: remote.to_string_lossy().into_owned(),
                source: PushSource::Commit(desired.clone()),
                branch: "main".into(),
                update: PushUpdate::Lease {
                    expected: Some(old),
                },
            },
            &GitAuth::None,
        )
        .await
        .unwrap_err();
    assert_eq!(git(&remote, &["rev-parse", "main"]), desired);
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Exit, FailureEffect::Unknown)
    );
    assert!(error.to_string().contains("remote failed to report status"));
    let public = G5Error::from(error).to_string();
    assert!(
        public.contains("refs/heads/main")
            && public.contains("inspect current state before retrying")
    );
}

#[tokio::test]
async fn ambient_controls_child() {
    let Ok(repository) = std::env::var("G5_TEST_NATIVE_REPOSITORY") else {
        return;
    };
    let url = std::env::var("G5_TEST_NATIVE_URL").unwrap();
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    if std::env::var_os("G5_TEST_NATIVE_FETCH").is_some() {
        let error = runner
            .fetch_checkout(
                Path::new(&repository),
                "origin",
                &url,
                &GitAuth::Token("private-token".into()),
            )
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("private-token"));
    } else {
        runner
            .push_branch(
                BranchPush {
                    repository: repository.into(),
                    remote: "origin".into(),
                    expected_url: url,
                    source: PushSource::Branch("main".into()),
                    branch: "main".into(),
                    update: PushUpdate::FastForward,
                },
                &GitAuth::None,
            )
            .await
            .unwrap();
    }
}

#[test]
fn inherited_repository_controls_cannot_change_the_pushed_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let intended = dir.path().join("intended");
    let other = dir.path().join("other");
    let remote = dir.path().join("remote.git");
    for repository in [&intended, &other] {
        std::fs::create_dir(repository).unwrap();
        git(repository, &["init", "--initial-branch=main"]);
        commit(
            repository,
            repository.file_name().unwrap().to_str().unwrap(),
        );
        git(
            repository,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
    }
    git(dir.path(), &["init", "--bare", remote.to_str().unwrap()]);
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native::tests::ambient_controls_child",
            "--nocapture",
        ])
        .env("G5_TEST_NATIVE_REPOSITORY", &intended)
        .env("G5_TEST_NATIVE_URL", &remote)
        .env("GIT_DIR", other.join(".git"))
        .env("GIT_WORK_TREE", &other)
        .env("GIT_NAMESPACE", "unrelated")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    assert_eq!(
        git(&remote, &["rev-parse", "main"]),
        git(&intended, &["rev-parse", "HEAD"])
    );
    assert_ne!(
        git(&remote, &["rev-parse", "main"]),
        git(&other, &["rev-parse", "HEAD"])
    );
}

#[cfg(unix)]
pub(super) async fn assert_reaped(pid: i32) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while unsafe { libc::kill(pid, 0) } == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native child must be reaped");
}

#[cfg(unix)]
#[tokio::test]
async fn cancellation_reaps_child_and_stops_descendant_effects() {
    let dir = tempfile::tempdir().unwrap();
    let executable = script(
        dir.path(),
        r#"
        (while [ ! -f "$3" ]; do sleep 0.01; done; echo escaped > "$2") &
        echo $$ > "$1.tmp"
        mv "$1.tmp" "$1"
        wait
    "#,
    );
    let pidfile = dir.path().join("pid");
    let marker = dir.path().join("escaped");
    let release = dir.path().join("release");
    let mut command = Invocation::new(&executable, None, "test", Duration::from_secs(10));
    command.arg(&pidfile).arg(&marker).arg(&release);
    let task = tokio::spawn(command.execute());
    wait_for_file(&pidfile).await;
    let pid = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    assert_reaped(pid).await;
    // The descendant cannot act before cancellation, even on a loaded CI host.
    // An escaped descendant would write its marker when this gate opens.
    std::fs::write(&release, "go").unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!marker.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn deadline_includes_pipes_held_by_descendants_after_leader_exit() {
    let dir = tempfile::tempdir().unwrap();
    let executable = script(
        dir.path(),
        r#"
        (sleep 3; echo escaped > "$2") &
        echo $$ > "$1"
        exit 0
    "#,
    );
    let pidfile = dir.path().join("pid");
    let marker = dir.path().join("escaped");
    let mut command = Invocation::new(&executable, None, "test", Duration::from_secs(2));
    command.arg(&pidfile).arg(&marker);
    let error = match command.execute().await {
        Ok(_) => panic!("expected deadline"),
        Err(error) => error,
    };
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Deadline, FailureEffect::Unknown)
    );
    let pid = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_reaped(pid).await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(!marker.exists());
}

#[tokio::test]
async fn http_credentials_are_scoped_and_redirects_do_not_receive_them() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let first = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/repo", first.local_addr().unwrap());
    let redirect = format!("http://{}/stolen", other.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = first.accept().await.unwrap();
        let mut request = vec![0; 8192];
        let count = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..count]).into_owned();
        socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {redirect}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        request
    });
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--initial-branch=main"]);
    git(dir.path(), &["remote", "add", "origin", &url]);
    // Local URL-specific settings must not override the operation's redirect fence.
    git(
        dir.path(),
        &["config", &format!("http.{url}.followRedirects"), "true"],
    );
    let output = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native::tests::ambient_controls_child",
            "--nocapture",
        ])
        .env("G5_TEST_NATIVE_REPOSITORY", dir.path())
        .env("G5_TEST_NATIVE_URL", &url)
        .env("G5_TEST_NATIVE_FETCH", "1")
        .env(
            "GIT_CONFIG_PARAMETERS",
            format!("'http.{url}.followRedirects=true'"),
        )
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let request = server.await.unwrap();
    assert!(
        request
            .to_lowercase()
            .contains("authorization: basic ec1hy2nlc3mtdg9rzw46chjpdmf0zs10b2tlbg=="),
        "{request}"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), other.accept())
            .await
            .is_err()
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-token"));
}

#[tokio::test]
async fn runtime_member_relay_preserves_canonical_identity_without_injecting_credentials() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!(
        "http://{}/private-lease/git/member",
        listener.local_addr().unwrap()
    );
    let canonical = "https://api.example.invalid/git/member";
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("member");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch=main"]);
    commit(&repository, "initial");
    git(&repository, &["remote", "add", "origin", canonical]);
    let global = dir.path().join("gitconfig");
    let member = dir.path().join("native-git-member");
    // Same owner-created includeIf/mapping structure as GitRuntime's native
    // member configuration; the shared crate must not depend up on that owner.
    let condition = format!(
        "includeIf.gitdir:{}.path",
        repository.canonicalize().unwrap().join(".git").display()
    );
    git(
        &repository,
        &[
            "config",
            "--file",
            global.to_str().unwrap(),
            &condition,
            member.to_str().unwrap(),
        ],
    );
    git(
        &repository,
        &[
            "config",
            "--file",
            member.to_str().unwrap(),
            &format!("url.{relay}.insteadOf"),
            canonical,
        ],
    );
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut request = vec![0; 8192];
        let count = socket.read(&mut request).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        String::from_utf8(request[..count].to_vec()).unwrap()
    });
    let runner = NativeGit::workspace_cli(Duration::from_secs(5), global).unwrap();
    let error = runner
        .fetch_checkout(
            &repository,
            "origin",
            canonical,
            &GitAuth::Token("platform-token-must-not-enter-git".into()),
        )
        .await
        .unwrap_err();
    let request = server.await.unwrap();
    assert!(request.starts_with("GET /private-lease/git/member/info/refs"));
    assert!(!request.to_lowercase().contains("authorization"));
    assert!(error.to_string().contains(canonical));
    assert!(!format!("{error:?} {error}").contains("private-lease"));
    git(
        &repository,
        &[
            "config",
            "remote.origin.pushurl",
            "https://untrusted.invalid/git/member",
        ],
    );
    let error = runner
        .push_branch(
            BranchPush {
                repository,
                remote: "origin".into(),
                expected_url: canonical.into(),
                source: PushSource::Branch("main".into()),
                branch: "main".into(),
                update: PushUpdate::FastForward,
            },
            &GitAuth::None,
        )
        .await
        .unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Destination, FailureEffect::Refused)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn deadline_retains_confirmed_ref_after_real_push() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("checkout");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch=main"]);
    commit(&repository, "initial");
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--bare",
            repository.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(
        &repository,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    let desired = commit(&repository, "confirmed before deadline");
    let real_git = process::resolve_binary()
        .unwrap()
        .to_string_lossy()
        .replace('\'', "'\\''");
    let marker = dir.path().join("escaped");
    let marker_arg = marker.to_string_lossy().replace('\'', "'\\''");
    let binary = script(
        dir.path(),
        &format!(
            r#"
        is_push=false
        for arg in "$@"; do [ "$arg" = push ] && is_push=true; done
        '{real_git}' "$@"
        status=$?
        if [ "$is_push" = true ]; then
            (sleep 3; echo escaped > '{marker_arg}') &
        fi
        exit "$status"
    "#
        ),
    );
    let runner = NativeGit {
        binary,
        ..NativeGit::cli(Duration::from_secs(2)).unwrap()
    };
    let error = runner
        .push_branch(
            BranchPush {
                repository,
                remote: "origin".into(),
                expected_url: remote.to_string_lossy().into_owned(),
                source: PushSource::Commit(desired.clone()),
                branch: "main".into(),
                update: PushUpdate::FastForward,
            },
            &GitAuth::None,
        )
        .await
        .unwrap_err();
    assert_eq!(git(&remote, &["rev-parse", "main"]), desired);
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Deadline, FailureEffect::Partial)
    );
    let message = G5Error::from(error).to_string();
    assert!(message.contains("confirmed the requested remote ref"));
    assert!(message.contains("deadline elapsed"));
    assert!(message.contains("refs/heads/main"));
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(!marker.exists(), "descendant survived deadline cleanup");
}

#[tokio::test]
async fn runtime_member_relay_clones_without_existing_destination_or_ambient_member() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let expected = commit(&source, "initial");
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &[
            "clone",
            "--bare",
            source.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(&remote, &["update-server-info"]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!(
        "http://{}/private-lease/git/member",
        listener.local_addr().unwrap()
    );
    let canonical = "https://api.example.invalid/git/member";
    let global = dir.path().join("gitconfig");
    let member = dir.path().join("native-git-member");
    let condition = format!(
        "includeIf.gitdir:{}.path",
        source.canonicalize().unwrap().join(".git").display()
    );
    git(
        &source,
        &[
            "config",
            "--file",
            global.to_str().unwrap(),
            &condition,
            member.to_str().unwrap(),
        ],
    );
    git(
        &source,
        &[
            "config",
            "--file",
            member.to_str().unwrap(),
            &format!("url.{relay}.insteadOf"),
            canonical,
        ],
    );
    let server = tokio::spawn(async move {
        let mut requests = 0;
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).await.unwrap();
                assert!(count > 0 && request.len() < 16384);
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(!request.to_lowercase().contains("authorization"));
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap()
                .split('?')
                .next()
                .unwrap();
            let path = path.strip_prefix("/private-lease/git/member/").unwrap();
            assert!(!path.contains(".."));
            let (status, body) = match std::fs::read(remote.join(path)) {
                Ok(body) => ("200 OK", body),
                Err(_) => ("404 Not Found", Vec::new()),
            };
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            socket.write_all(&body).await.unwrap();
            requests += 1;
            // Keep serving until the test aborts the fixture.
            assert!(requests < 30);
        }
    });
    let destination = dir.path().join("fresh-checkout");
    assert!(!destination.exists());
    let runner = NativeGit::workspace_cli(Duration::from_secs(5), global).unwrap();
    runner
        .clone_checkout(
            canonical,
            &destination,
            Some("main"),
            &GitAuth::Token("must-not-enter-repository-git".into()),
        )
        .await
        .unwrap();
    // Pinned materialization fetches immediately after cloning. The new
    // checkout must retain access without a new persistent relay include.
    runner
        .fetch_checkout(&destination, "origin", canonical, &GitAuth::None)
        .await
        .unwrap();
    assert_eq!(git(&destination, &["rev-parse", "HEAD"]), expected);
    assert_eq!(
        git(&destination, &["remote", "get-url", "origin"]),
        canonical
    );
    assert!(!std::fs::read_to_string(destination.join(".git/config"))
        .unwrap()
        .contains("private-lease"));
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn provider_grants_refuse_other_origins_and_configured_destination_changes() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--initial-branch=main"]);
    let oid = commit(dir.path(), "initial");
    git(
        dir.path(),
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
    for (provider, intended, actual) in [
        (
            crate::ScmProvider::GitHub,
            "https://github.com.attacker.example/o/r.git",
            "https://github.com.attacker.example/o/r.git",
        ),
        (
            crate::ScmProvider::GitHub,
            "http://github.com/o/r.git",
            "http://github.com/o/r.git",
        ),
        (
            crate::ScmProvider::GitHub,
            "https://github.com:8443/o/r.git",
            "https://github.com:8443/o/r.git",
        ),
        (
            crate::ScmProvider::GitHub,
            "https://gitlab.com/o/r.git",
            "https://gitlab.com/o/r.git",
        ),
        (
            crate::ScmProvider::GitHub,
            "https://user@github.com/o/r.git",
            "https://user@github.com/o/r.git",
        ),
        (
            crate::ScmProvider::GitHub,
            "https://github.com/o/r.git",
            "https://github.com.attacker.example/o/r.git",
        ),
        (
            crate::ScmProvider::GitLabInstance {
                host: "gitlab.internal".into(),
                port: 8443,
            },
            "https://gitlab.internal/o/r.git",
            "https://gitlab.internal/o/r.git",
        ),
        (
            crate::ScmProvider::GitLabInstance {
                host: "gitlab.internal".into(),
                port: 8443,
            },
            "https://gitlab.com/o/r.git",
            "https://gitlab.com/o/r.git",
        ),
    ] {
        git(dir.path(), &["config", "remote.origin.pushurl", actual]);
        let auth = GitAuth::ScmToken {
            token: "private-provider-token".into(),
            provider,
        };
        let error = NativeGit::cli(Duration::from_secs(2))
            .unwrap()
            .delete_branch(
                BranchDelete {
                    repository: dir.path().to_owned(),
                    remote: "origin".into(),
                    expected_url: intended.into(),
                    branch: "main".into(),
                    expected: oid.clone(),
                },
                &auth,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.effect,
            FailureEffect::Refused,
            "{intended} -> {actual}: {error}"
        );
        assert!(matches!(
            error.kind,
            FailureKind::InvalidInput | FailureKind::Destination
        ));
        assert!(!format!("{error:?} {error}").contains("private-provider-token"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn ssh_credential_files_survive_execution_and_are_removed_after_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    for mode in ["complete", "deadline", "cancel"] {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("credentials");
        let binary = script(
            dir.path(),
            r#"
            response=$("$SSH_ASKPASS") || exit 91
            [ "$response" = private-key-phrase ] || exit 92
            printf '%s' "$response" >&2
            # Publish only after all paths are written; the parent waits for this file.
            printf '%s\n' "$GIT_SSH_COMMAND" "$SSH_ASKPASS" "$G5_SSH_KEY_PASSPHRASE_FILE" > "$1.tmp"
            mv "$1.tmp" "$1"
            if [ "$2" != complete ]; then
                (sleep 4; echo escaped > "$3") &
                wait
            fi
            exit 1
        "#,
        );
        let mut command = Invocation::new(
            &binary,
            None,
            "SSH credential lifetime",
            Duration::from_secs(2),
        );
        command
            .authenticate(
                "ssh://git@example.invalid/repo",
                &GitAuth::SshKey {
                    private_key: "private-key-bytes".into(),
                    passphrase: Some("private-key-phrase".into()),
                },
            )
            .unwrap();
        command
            .arg(&marker)
            .arg(mode)
            .arg(dir.path().join("escaped"));
        let task = tokio::spawn(command.execute());
        wait_for_file(&marker).await;
        let records = std::fs::read_to_string(&marker).unwrap();
        let lines: Vec<_> = records.lines().collect();
        let key = PathBuf::from(lines[0].split('\'').nth(1).unwrap());
        let files = [key, PathBuf::from(lines[1]), PathBuf::from(lines[2])];
        if mode != "complete" {
            for (path, expected) in files.iter().zip([0o600, 0o700, 0o600]) {
                assert_eq!(
                    std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    expected
                );
            }
        }
        if mode == "cancel" {
            task.abort();
        }
        let result = task.await;
        if mode == "cancel" {
            assert!(matches!(result, Err(error) if error.is_cancelled()));
        } else {
            let error = match result.unwrap() {
                Ok(output) => output
                    .success("SSH credential lifetime", &GitAuth::None)
                    .unwrap_err(),
                Err(error) => error,
            };
            assert_eq!(
                error.kind,
                if mode == "complete" {
                    FailureKind::Exit
                } else {
                    FailureKind::Deadline
                }
            );
            assert!(!format!("{error:?} {error}").contains("private-key-phrase"));
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while files.iter().any(|path| path.exists()) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!dir.path().join("escaped").exists());
    }
}

#[tokio::test]
async fn remote_helpers_are_refused_before_repository_or_credential_access() {
    let root = tempfile::tempdir().unwrap();
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    for auth in [
        GitAuth::None,
        GitAuth::SshKey {
            private_key: "private-key-bytes".into(),
            passphrase: Some("private-key-phrase".into()),
        },
    ] {
        for url in [
            "ext::unexpected-command",
            "file::/unexpected/repo",
            "custom::repo",
        ] {
            let error = runner
                .delete_branch(
                    BranchDelete {
                        repository: root.path().join("absent"),
                        remote: "origin".into(),
                        expected_url: url.into(),
                        branch: "main".into(),
                        expected: "a".repeat(40),
                    },
                    &auth,
                )
                .await
                .unwrap_err();
            assert_eq!(
                (error.kind, error.effect),
                (FailureKind::InvalidInput, FailureEffect::Refused),
                "{url}"
            );
        }
    }
}

#[tokio::test]
async fn api_gc_preserves_refs_index_checkout_and_thresholds() {
    let dir = tempfile::tempdir().unwrap();
    let repository = dir.path().join("checkout");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--initial-branch=main"]);
    git(&repository, &["config", "gc.auto", "0"]);
    for number in 0..3 {
        std::fs::write(repository.join("file"), format!("committed {number}")).unwrap();
        git(&repository, &["add", "file"]);
        commit(&repository, "saved");
    }
    std::fs::write(repository.join("file"), "staged").unwrap();
    git(&repository, &["add", "file"]);
    std::fs::write(repository.join("file"), "dirty").unwrap();
    let index = std::fs::read(repository.join(".git/index")).unwrap();
    let config = std::fs::read(repository.join(".git/config")).unwrap();
    let refs = git(&repository, &["show-ref"]);
    let head = git(&repository, &["rev-parse", "HEAD"]);
    let runner =
        NativeGit::api(&process::resolve_binary().unwrap(), Duration::from_secs(10)).unwrap();
    let ObjectMaintenance::BelowThreshold { loose_objects } =
        runner.collect_garbage(&repository, u64::MAX).await.unwrap()
    else {
        panic!("expected below-threshold result");
    };
    assert!(loose_objects > 0);
    assert_eq!(
        std::fs::read_dir(repository.join(".git/objects/pack"))
            .unwrap()
            .count(),
        0
    );
    let ObjectMaintenance::Collected {
        loose_before,
        loose_after,
    } = runner
        .collect_garbage(&repository, loose_objects)
        .await
        .unwrap()
    else {
        panic!("expected successful collection at the exact threshold");
    };
    assert_eq!(loose_before, loose_objects);
    assert!(loose_after < loose_before);
    assert!(std::fs::read_dir(repository.join(".git/objects/pack"))
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "pack")));
    assert_eq!(std::fs::read(repository.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read(repository.join(".git/config")).unwrap(),
        config
    );
    assert_eq!(
        std::fs::read_to_string(repository.join("file")).unwrap(),
        "dirty"
    );
    assert_eq!(git(&repository, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&repository, &["show-ref"]), refs);
    assert_eq!(
        runner
            .collect_garbage(&dir.path().join("missing"), 0)
            .await
            .unwrap(),
        ObjectMaintenance::NotMaterialized
    );
    assert_eq!(
        runner.collect_garbage(dir.path(), 0).await.unwrap(),
        ObjectMaintenance::NotMaterialized
    );
    let bare = dir.path().join("bare.git");
    git(dir.path(), &["init", "--bare", bare.to_str().unwrap()]);
    assert!(matches!(
        runner.collect_garbage(&bare, 0).await.unwrap(),
        ObjectMaintenance::Collected {
            loose_before: 0,
            loose_after: 0
        }
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn api_gc_reports_invalid_counts_and_known_completed_effects_without_replay() {
    for (count, gc_exit, final_exit, kind, effect) in [
        (
            "count: wrong",
            0,
            0,
            FailureKind::InvalidOutput,
            FailureEffect::Refused,
        ),
        (
            "in-pack: 0",
            0,
            0,
            FailureKind::InvalidOutput,
            FailureEffect::Refused,
        ),
        ("count: 9", 1, 0, FailureKind::Exit, FailureEffect::Unknown),
        ("count: 9", 0, 1, FailureKind::Exit, FailureEffect::Partial),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let binary = script(
            dir.path(),
            &format!(
                r#"
            cd "$2" || exit 90
            for arg in "$@"; do
                case "$arg" in
                    count-objects)
                        echo count >> attempts
                        if [ -f collected ]; then exit {final_exit}; fi
                        echo '{count}'; exit 0;;
                    gc)
                        echo gc >> attempts
                        touch collected
                        exit {gc_exit};;
                esac
            done
            exit 91
        "#
            ),
        );
        let runner = NativeGit::api(&binary, Duration::from_secs(3)).unwrap();
        let error = runner.collect_garbage(dir.path(), 1).await.unwrap_err();
        assert_eq!((error.kind, error.effect), (kind, effect));
        assert_eq!(
            error.target,
            Some(dir.path().to_string_lossy().into_owned())
        );
        let attempts = std::fs::read_to_string(dir.path().join("attempts")).unwrap();
        assert_eq!(
            attempts,
            if effect == FailureEffect::Refused {
                "count\n"
            } else if effect == FailureEffect::Unknown {
                "count\ngc\n"
            } else {
                "count\ngc\ncount\n"
            }
        );
        if effect == FailureEffect::Partial {
            assert!(error.to_string().contains("garbage collection completed"));
            assert!(error
                .to_string()
                .contains("inspect current state before retrying"));
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn api_gc_count_parser_and_nonzero_reads_preserve_refusal() {
    for (output, exit, expected) in [
        ("count: 4610\nsize: 1\nin-pack: 0", 0, Some(4610)),
        ("count: 9", 1, None),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let binary = script(
            dir.path(),
            &format!("printf '%s\\n' '{output}'; exit {exit}"),
        );
        let runner = NativeGit::api(&binary, Duration::from_secs(3)).unwrap();
        let result = runner.collect_garbage(dir.path(), u64::MAX).await;
        match expected {
            Some(count) => assert_eq!(
                result.unwrap(),
                ObjectMaintenance::BelowThreshold {
                    loose_objects: count
                }
            ),
            None => {
                let error = result.unwrap_err();
                assert_eq!(
                    (error.kind, error.effect),
                    (FailureKind::Exit, FailureEffect::Refused)
                );
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn api_gc_deadline_and_cancellation_reap_the_active_child() {
    for explicit_cancel in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let binary = script(
            dir.path(),
            r#"
            cd "$2" || exit 90
            for arg in "$@"; do
                case "$arg" in
                    count-objects) echo 'count: 9'; exit 0;;
                    gc) echo $$ > pid; while :; do :; done;;
                esac
            done
            exit 91
        "#,
        );
        let cancel = NativeCancellation::default();
        let timeout = if explicit_cancel {
            Duration::from_secs(10)
        } else {
            Duration::from_secs(1)
        };
        let runner = NativeGit::api(&binary, timeout)
            .unwrap()
            .with_cancellation(cancel.clone());
        let repository = dir.path().to_owned();
        let mut task = tokio::spawn(async move { runner.collect_garbage(&repository, 1).await });
        let pid: i32 = tokio::select! {
            early = &mut task => panic!("GC returned before reaching the hanging stage: {early:?}"),
            pid = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Ok(value) = std::fs::read_to_string(dir.path().join("pid")) {
                        if let Ok(pid) = value.trim().parse() { break pid; }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }) => pid.expect("GC must reach the hanging stage"),
        };
        if explicit_cancel {
            cancel.cancel();
        }
        let error = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.effect, FailureEffect::Unknown);
        assert_eq!(
            error.kind,
            if explicit_cancel {
                FailureKind::Cancelled
            } else {
                FailureKind::Deadline
            }
        );
        cancel.wait_idle().await;
        assert_reaped(pid).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn api_gc_environment_child() {
    let Some(repository) = std::env::var_os("G5_TEST_API_GC_REPOSITORY") else {
        return;
    };
    let binary = std::env::var_os("G5_TEST_API_GC_BINARY").unwrap();
    let runner = NativeGit::api(Path::new(&binary), Duration::from_secs(3)).unwrap();
    assert_eq!(
        runner
            .collect_garbage(Path::new(&repository), 100)
            .await
            .unwrap(),
        ObjectMaintenance::BelowThreshold { loose_objects: 9 }
    );
}

#[cfg(unix)]
#[test]
fn api_gc_context_clears_ambient_controls_and_uses_fixed_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let global = dir.path().join("global-config");
    std::fs::write(&global, "sentinel").unwrap();
    let binary = script(
        dir.path(),
        r#"
        [ "$PATH" = '/usr/bin:/usr/local/bin' ] || exit 81
        [ "$LC_ALL" = C ] || exit 82
        [ -z "${HOME+x}${GIT_DIR+x}${GIT_CONFIG_GLOBAL+x}${GIT_CONFIG_PARAMETERS+x}${G5D_TOKEN+x}${CUSTOM_API_SECRET+x}" ] || exit 83
        [ "$GIT_TERMINAL_PROMPT" = 0 ] || exit 84
        echo 'count: 9'
    "#,
    );
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native::tests::api_gc_environment_child",
            "--nocapture",
        ])
        .env("G5_TEST_API_GC_REPOSITORY", dir.path())
        .env("G5_TEST_API_GC_BINARY", &binary)
        .env("HOME", dir.path())
        .env("PATH", "/untrusted")
        .env("GIT_DIR", "/wrong/repository")
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("GIT_CONFIG_PARAMETERS", "'gc.pruneExpire=now'")
        .env("G5D_TOKEN", "private-token")
        .env("CUSTOM_API_SECRET", "private-value")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(global).unwrap(), "sentinel");
    let named = NativeGit::api(Path::new("git"), Duration::from_secs(1)).unwrap();
    assert!(named.binary.is_absolute());
    assert!(NativeGit::api(Path::new(""), Duration::from_secs(1)).is_err());
    assert!(NativeGit::api(&binary, Duration::ZERO).is_err());
}

#[tokio::test]
async fn api_observations_read_exact_objects_refs_and_ancestry_without_changes() {
    for format in ["sha1", "sha256"] {
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("checkout");
        std::fs::create_dir(&checkout).unwrap();
        git(
            &checkout,
            &[
                "init",
                "--initial-branch=main",
                &format!("--object-format={format}"),
            ],
        );
        std::fs::write(checkout.join("file"), "saved").unwrap();
        git(&checkout, &["add", "file"]);
        let first = commit(&checkout, "first");
        let second = commit(&checkout, "second");
        let tree = git(&checkout, &["rev-parse", "HEAD^{tree}"]);
        let blob = git(&checkout, &["rev-parse", "HEAD:file"]);
        git(&checkout, &["tag", "-a", "annotated", "-m", "tag"]);
        let tag = git(&checkout, &["rev-parse", "refs/tags/annotated"]);
        git(
            &checkout,
            &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
        );
        let bare = dir.path().join("bare.git");
        git(
            dir.path(),
            &[
                "clone",
                "--bare",
                checkout.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        git(
            &bare,
            &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
        );
        let linked = dir.path().join("linked");
        git(
            &checkout,
            &["worktree", "add", "--detach", linked.to_str().unwrap()],
        );
        std::fs::write(checkout.join("file"), "staged").unwrap();
        git(&checkout, &["add", "file"]);
        std::fs::write(checkout.join("file"), "dirty").unwrap();
        let index = std::fs::read(checkout.join(".git/index")).unwrap();
        let config = std::fs::read(checkout.join(".git/config")).unwrap();
        let refs = git(&checkout, &["show-ref"]);
        let runner =
            NativeGit::api(&process::resolve_binary().unwrap(), Duration::from_secs(10)).unwrap();
        for repository in [&checkout, &bare, &linked] {
            assert_eq!(
                runner.ref_tip(repository, "refs/heads/main").await.unwrap(),
                Some(second.clone())
            );
            assert_eq!(
                runner
                    .ref_tip(repository, "refs/heads/missing")
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                runner
                    .ref_tip(repository, "refs/heads/alias")
                    .await
                    .unwrap(),
                Some(second.clone())
            );
            assert_eq!(
                runner
                    .symbolic_ref_target(repository, "refs/heads/alias")
                    .await
                    .unwrap()
                    .as_deref(),
                Some("refs/heads/main")
            );
            for reference in ["refs/heads/main", "refs/heads/missing"] {
                assert_eq!(
                    runner
                        .symbolic_ref_target(repository, reference)
                        .await
                        .unwrap(),
                    None
                );
            }
            for (oid, kind) in [
                (&first, NativeObjectKind::Commit),
                (&tree, NativeObjectKind::Tree),
                (&blob, NativeObjectKind::Blob),
                (&tag, NativeObjectKind::Tag),
            ] {
                assert_eq!(
                    runner.object_kind(repository, oid).await.unwrap(),
                    Some(kind)
                );
            }
            assert_eq!(
                runner
                    .object_kind(repository, &"a".repeat(first.len()))
                    .await
                    .unwrap(),
                None
            );
            assert!(runner
                .is_ancestor(repository, &first, &second)
                .await
                .unwrap());
            assert!(!runner
                .is_ancestor(repository, &second, &first)
                .await
                .unwrap());
            assert!(runner
                .is_ancestor(repository, &"a".repeat(first.len()), &second)
                .await
                .is_err());
        }
        if format == "sha256" {
            for result in [
                runner
                    .object_kind(&checkout, &first[..40])
                    .await
                    .map(|_| ()),
                runner
                    .is_ancestor(&checkout, &first[..40], &second)
                    .await
                    .map(|_| ()),
            ] {
                assert_eq!(result.unwrap_err().kind, FailureKind::InvalidInput);
            }
        }
        let nested = checkout.join("missing-store");
        std::fs::create_dir(&nested).unwrap();
        for result in [
            runner.ref_tip(&nested, "refs/heads/main").await.map(|_| ()),
            runner
                .symbolic_ref_target(&nested, "refs/heads/alias")
                .await
                .map(|_| ()),
            runner.object_kind(&nested, &first).await.map(|_| ()),
            runner
                .is_ancestor(&nested, &first, &second)
                .await
                .map(|_| ()),
        ] {
            assert_eq!(result.unwrap_err().effect, FailureEffect::Refused);
        }
        assert_eq!(std::fs::read(checkout.join(".git/index")).unwrap(), index);
        assert_eq!(std::fs::read(checkout.join(".git/config")).unwrap(), config);
        assert_eq!(git(&checkout, &["show-ref"]), refs);
        assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), second);
        assert_eq!(
            std::fs::read_to_string(checkout.join("file")).unwrap(),
            "dirty"
        );
        for result in [
            runner
                .ref_tip(dir.path(), "refs/heads/missing")
                .await
                .map(|_| ()),
            runner
                .symbolic_ref_target(dir.path(), "refs/heads/main")
                .await
                .map(|_| ()),
            runner.object_kind(dir.path(), &first).await.map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(
                (error.kind, error.effect),
                (FailureKind::Exit, FailureEffect::Refused)
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn api_observations_reject_invalid_inputs_and_malformed_or_failed_output() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "for arg in \"$@\"; do [ \"$arg\" = check-ref-format ] && exit 0; done\ncat >/dev/null\necho malformed");
    let runner = NativeGit::api(&binary, Duration::from_secs(3)).unwrap();
    let oid = "a".repeat(40);
    for value in [
        "HEAD",
        "--all",
        "refs/heads/main\nrefs/heads/other",
        "refs/heads/main\0",
    ] {
        assert_eq!(
            runner.ref_tip(dir.path(), value).await.unwrap_err().kind,
            FailureKind::InvalidInput
        );
    }
    for value in ["HEAD", "--batch", "aaaa", "a\n", &"z".repeat(40)] {
        assert_eq!(
            runner
                .object_kind(dir.path(), value)
                .await
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
    }
    for result in [
        runner
            .ref_tip(dir.path(), "refs/heads/main")
            .await
            .map(|_| ()),
        runner.object_kind(dir.path(), &oid).await.map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert_eq!(
            (error.kind, error.effect),
            (FailureKind::InvalidOutput, FailureEffect::Refused)
        );
    }
    for (body, kind) in [
        (
            "cat >/dev/null; echo 'fatal: cannot open object store' >&2; exit 128",
            FailureKind::Exit,
        ),
        (
            "cat >/dev/null; echo 'commit'; echo 'tree'",
            FailureKind::InvalidOutput,
        ),
        (
            "cat >/dev/null; i=0; while [ $i -lt 10000 ]; do echo 'commit'; i=$((i+1)); done",
            FailureKind::InvalidOutput,
        ),
    ] {
        let binary = script(dir.path(), &format!("for arg in \"$@\"; do [ \"$arg\" = --show-object-format ] && {{ echo sha1; exit 0; }}; done\n{body}"));
        let runner = NativeGit::api(&binary, Duration::from_secs(3)).unwrap();
        let error = runner.object_kind(dir.path(), &oid).await.unwrap_err();
        assert_eq!((error.kind, error.effect), (kind, FailureEffect::Refused));
        assert_eq!(error.target.as_deref(), Some(oid.as_str()));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn api_observation_deadline_and_cancellation_reap_child_without_retry() {
    for mode in ["deadline", "cancel", "drop"] {
        let dir = tempfile::tempdir().unwrap();
        let binary = script(
            dir.path(),
            "for arg in \"$@\"; do [ \"$arg\" = --show-object-format ] && { echo sha1; exit 0; }; done\ncd \"$2\" || exit 90\necho attempt >> attempts\necho $$ > pid\nwhile :; do :; done",
        );
        let cancel = NativeCancellation::default();
        let timeout = if mode == "deadline" {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(10)
        };
        let runner = NativeGit::api(&binary, timeout)
            .unwrap()
            .with_cancellation(cancel.clone());
        let repository = dir.path().to_owned();
        let task =
            tokio::spawn(async move { runner.object_kind(&repository, &"a".repeat(40)).await });
        let pid: i32 = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(value) = std::fs::read_to_string(dir.path().join("pid")) {
                    if let Ok(pid) = value.trim().parse() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        match mode {
            "cancel" => cancel.cancel(),
            "drop" => task.abort(),
            _ => {}
        }
        let result = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap();
        if mode == "drop" {
            assert!(result.unwrap_err().is_cancelled());
        } else {
            let error = result.unwrap().unwrap_err();
            assert_eq!(error.effect, FailureEffect::Refused);
            assert_eq!(
                error.kind,
                if mode == "cancel" {
                    FailureKind::Cancelled
                } else {
                    FailureKind::Deadline
                }
            );
        }
        tokio::time::timeout(Duration::from_secs(3), cancel.wait_idle())
            .await
            .unwrap();
        assert_reaped(pid).await;
        assert_eq!(
            std::fs::read_to_string(dir.path().join("attempts")).unwrap(),
            "attempt\n"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn native_input_and_output_are_drained_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), &format!("cd '{}'\ndd if=/dev/zero bs=1024 count=256 2>/dev/null\ndd if=/dev/zero bs=1024 count=256 1>&2 2>/dev/null\nwc -c > input-size", dir.path().display()));
    // An input larger than a pipe must not be delivered before stdout/stderr drain.
    let mut invocation = process::Invocation::new(
        &binary,
        Some(dir.path()),
        "input test",
        Duration::from_secs(5),
    );
    invocation.read_only().input(vec![b'x'; 256 * 1024]);
    let output = invocation.execute().await.unwrap();
    assert!(output.completed_successfully());
    assert!(output.truncated);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("input-size"))
            .unwrap()
            .trim(),
        "262144"
    );
}

#[tokio::test]
async fn api_observations_distinguish_exact_absence_corruption_and_benign_warnings() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--initial-branch=main"]);
    let oid = commit(dir.path(), "initial");
    let runner =
        NativeGit::api(&process::resolve_binary().unwrap(), Duration::from_secs(5)).unwrap();
    let leaf = "refs/g5/flush/workspace/flush/snapshot";
    git(
        dir.path(),
        &["update-ref", &format!("refs/heads/{leaf}"), &oid],
    );
    assert_eq!(
        runner.ref_tip(dir.path(), leaf).await.unwrap(),
        None,
        "revision shorthand must not satisfy exact refs"
    );
    let reference = dir.path().join(".git/refs/heads/main");
    for content in ["garbage\n", "ref: refs/heads/missing\n"] {
        std::fs::write(&reference, content).unwrap();
        assert_eq!(
            runner
                .ref_tip(dir.path(), "refs/heads/main")
                .await
                .unwrap_err()
                .kind,
            FailureKind::Exit
        );
    }
    std::fs::write(&reference, format!("{oid}\n")).unwrap();
    let object = dir
        .path()
        .join(".git/objects")
        .join(&oid[..2])
        .join(&oid[2..]);
    std::fs::remove_file(&object).unwrap();
    std::fs::write(object, "broken").unwrap();
    assert_eq!(
        runner.object_kind(dir.path(), &oid).await.unwrap_err().kind,
        FailureKind::Exit
    );
    #[cfg(unix)]
    {
        let binary = script(dir.path(), "echo \"git: warning: confstr() failed with code 5: couldn't get path; using /tmp instead\" >&2\nfor arg in \"$@\"; do case \"$arg\" in check-ref-format) exit 0;; for-each-ref) exit 0;; symbolic-ref) exit 1;; --show-object-format) echo sha1; exit 0;; esac; done\nread oid; echo \"$oid missing\"");
        let runner = NativeGit::api(&binary, Duration::from_secs(5)).unwrap();
        assert_eq!(runner.ref_tip(dir.path(), leaf).await.unwrap(), None);
        assert_eq!(runner.object_kind(dir.path(), &oid).await.unwrap(), None);
    }
}

#[test]
fn checkout_queries_preserve_literal_paths_and_conservative_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "--initial-branch=main"]);
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    assert!(runner
        .version_blocking()
        .unwrap()
        .starts_with("git version "));
    assert!(runner
        .file_is_new_blocking(&root.join("untracked.md"))
        .unwrap());
    std::fs::create_dir(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/tracked.md"), "tracked").unwrap();
    std::fs::write(root.join(".gitignore"), "ignored.md\n").unwrap();
    git(root, &["add", "."]);
    let head = commit(root, "tracked");
    assert_eq!(
        runner.head_commit_blocking(&root.join("docs")).unwrap(),
        head
    );
    assert!(!runner
        .file_is_new_blocking(&root.join("docs/tracked.md"))
        .unwrap());
    assert!(runner
        .file_is_new_blocking(&root.join("docs/*.md"))
        .unwrap());
    assert!(runner
        .path_is_ignored_blocking(root, Path::new("ignored.md"))
        .unwrap());
    assert!(!runner
        .path_is_ignored_blocking(root, Path::new("ordinary.md"))
        .unwrap());
    git(root, &["checkout", "--detach", &head]);
    assert!(!runner
        .file_is_new_blocking(&root.join("docs/tracked.md"))
        .unwrap());
    std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/broken\n").unwrap();
    std::fs::write(root.join(".git/refs/heads/broken"), "not-an-oid\n").unwrap();
    assert!(runner
        .file_is_new_blocking(&root.join("untracked.md"))
        .is_err());
    let outside = tempfile::tempdir().unwrap();
    assert!(runner
        .file_is_new_blocking(&outside.path().join("file.md"))
        .is_err());
}

#[tokio::test]
async fn default_branch_and_local_mirror_use_explicit_scope_without_config_writes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let base = commit(&source, "base");
    git(&source, &["branch", "private"]);
    git(&source, &["tag", "private-tag"]);
    let mirror = dir.path().join("mirror.git");
    git(
        dir.path(),
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            mirror.to_str().unwrap(),
        ],
    );
    let config = std::fs::read(source.join(".git/config")).unwrap();
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();
    runner
        .push_local_mirror_blocking(&source, &mirror, "main")
        .unwrap();
    assert_eq!(
        git(&mirror, &["show-ref"]),
        format!("{base} refs/heads/main")
    );
    assert_eq!(
        runner
            .default_remote_branch(mirror.to_str().unwrap(), &GitAuth::None)
            .await
            .unwrap()
            .as_deref(),
        Some("main")
    );
    assert_eq!(std::fs::read(source.join(".git/config")).unwrap(), config);
    let checkout = dir.path().join("checkout");
    runner
        .clone_checkout(
            mirror.to_str().unwrap(),
            &checkout,
            Some("main"),
            &GitAuth::None,
        )
        .await
        .unwrap();
    let advanced = commit(&checkout, "advanced");
    runner
        .push_local_mirror_blocking(&checkout, &mirror, "main")
        .unwrap();
    commit(&source, "diverged");
    let error = runner
        .push_local_mirror_blocking(&source, &mirror, "main")
        .unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Rejected, FailureEffect::Refused)
    );
    assert_eq!(git(&mirror, &["rev-parse", "main"]), advanced);
    let elsewhere = dir.path().join("elsewhere.git");
    git(dir.path(), &["init", "--bare", elsewhere.to_str().unwrap()]);
    git(
        &source,
        &[
            "config",
            &format!("url.{}.insteadOf", elsewhere.display()),
            mirror.canonicalize().unwrap().to_str().unwrap(),
        ],
    );
    let error = runner
        .push_local_mirror_blocking(&source, &mirror, "main")
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Destination);
    assert_eq!(git(&elsewhere, &["for-each-ref"]), "");
}

#[test]
fn relative_mirror_destination_is_resolved_from_source_not_process_directory() {
    const CASE: &str = "G5_SCM_RELATIVE_MIRROR_CASE";
    if let Some(root) = std::env::var_os(CASE) {
        let root = PathBuf::from(root);
        NativeGit::cli(Duration::from_secs(10))
            .unwrap()
            .push_local_mirror_blocking(
                &root.join("location/source"),
                Path::new("../mirror.git"),
                "main",
            )
            .unwrap();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let source = root.join("location/source");
    let mirror = root.join("location/mirror.git");
    let decoy = root.join("mirror.git");
    let cwd = root.join("caller");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir(&cwd).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let head = commit(&source, "source");
    for target in [&mirror, &decoy] {
        git(&root, &["init", "--bare", target.to_str().unwrap()]);
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "native::tests::relative_mirror_destination_is_resolved_from_source_not_process_directory", "--nocapture"])
        .env(CASE, &root).current_dir(&cwd).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(&mirror, &["rev-parse", "refs/heads/main"]), head);
    assert_eq!(git(&decoy, &["for-each-ref"]), "");
}

#[cfg(unix)]
#[tokio::test]
async fn remote_reads_type_refused_credentials_and_validate_names_in_process() {
    let dir = tempfile::tempdir().unwrap();
    let calls = dir.path().join("calls");
    let binary = script(
        dir.path(),
        &format!(
            "echo \"$*\" >> '{}'\ncase \"$*\" in *--get-url*) echo https://example.invalid/r.git; exit 0;; esac\necho \"fatal: could not read Username for 'https://example.invalid': terminal prompts disabled\" >&2\nexit 128",
            calls.display()
        ),
    );
    let runner = NativeGit::api(&binary, Duration::from_secs(5)).unwrap();
    let url = "https://example.invalid/r.git";
    let error = runner
        .remote_branch_tip(url, "main", &GitAuth::None)
        .await
        .unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Unauthorized, FailureEffect::Refused)
    );
    assert_eq!(
        runner
            .remote_branches(url, &GitAuth::None)
            .await
            .unwrap_err()
            .kind,
        FailureKind::Unauthorized
    );
    // Name validation needs no process: invalid names refuse before any spawn.
    std::fs::remove_file(&calls).unwrap();
    for name in [
        "", "a..b", "-x", "a b", "a*", "a/", "x.lock", "a@{b", "a\\b", "/a",
    ] {
        let error = runner
            .remote_branch_tip(url, name, &GitAuth::None)
            .await
            .unwrap_err();
        assert_eq!(error.kind, FailureKind::InvalidInput, "{name:?}");
    }
    assert!(!calls.exists(), "branch validation spawned Git");
    // A server failure that is not a credential refusal stays an exit failure.
    let other = script(
        dir.path(),
        "case \"$*\" in *--get-url*) echo https://example.invalid/r.git; exit 0;; esac\necho 'fatal: unable to access: Could not resolve host' >&2\nexit 128",
    );
    let runner = NativeGit::api(&other, Duration::from_secs(5)).unwrap();
    assert_eq!(
        runner
            .remote_branches(url, &GitAuth::None)
            .await
            .unwrap_err()
            .kind,
        FailureKind::Exit
    );
}

#[tokio::test]
async fn full_clone_keeps_every_remote_branch_and_single_branch_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    commit(&source, "base");
    git(&source, &["branch", "other"]);
    let runner = NativeGit::cli(Duration::from_secs(30)).unwrap();
    let url = source.to_str().unwrap();
    let full = dir.path().join("full");
    runner
        .clone_full_checkout(url, &full, Some("main"), &GitAuth::None)
        .await
        .unwrap();
    assert!(git(&full, &["branch", "-r"]).contains("origin/other"));
    assert_eq!(
        git(&full, &["config", "--get", "remote.origin.fetch"]),
        "+refs/heads/*:refs/remotes/origin/*"
    );
    let single = dir.path().join("single");
    runner
        .clone_checkout(url, &single, Some("main"), &GitAuth::None)
        .await
        .unwrap();
    assert!(!git(&single, &["branch", "-r"]).contains("origin/other"));
}

#[tokio::test]
async fn single_branch_checkouts_can_track_another_branch_idempotently() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    commit(&source, "base");
    git(&source, &["branch", "other"]);
    let runner = NativeGit::cli(Duration::from_secs(30)).unwrap();
    let url = source.to_str().unwrap();
    let checkout = dir.path().join("checkout");
    runner
        .clone_checkout(url, &checkout, Some("main"), &GitAuth::None)
        .await
        .unwrap();
    for _ in 0..2 {
        runner
            .track_remote_branch(&checkout, "origin", "other")
            .await
            .unwrap();
    }
    let refspecs = git(&checkout, &["config", "--get-all", "remote.origin.fetch"]);
    assert_eq!(
        refspecs
            .lines()
            .filter(|line| line.contains("refs/heads/other"))
            .count(),
        1,
        "{refspecs}"
    );
    runner
        .fetch_checkout(&checkout, "origin", url, &GitAuth::None)
        .await
        .unwrap();
    assert!(git(&checkout, &["branch", "-r"]).contains("origin/other"));
    // A full clone's wildcard already covers every branch.
    let full = dir.path().join("full");
    runner
        .clone_full_checkout(url, &full, Some("main"), &GitAuth::None)
        .await
        .unwrap();
    runner
        .track_remote_branch(&full, "origin", "other")
        .await
        .unwrap();
    assert_eq!(
        git(&full, &["config", "--get-all", "remote.origin.fetch"]),
        "+refs/heads/*:refs/remotes/origin/*"
    );
    assert!(runner
        .track_remote_branch(&full, "origin", "bad..name")
        .await
        .is_err());
}

#[test]
fn cancellation_records_whether_any_native_operation_ran() {
    let cancellation = NativeCancellation::default();
    assert!(!cancellation.has_run());
    drop(cancellation.started());
    assert!(cancellation.has_run());
}

#[tokio::test]
async fn worktree_snapshot_reads_are_confined_and_commit_fetch_checks_its_destination() {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    let other = dir.path().join("other.git");
    let checkout = dir.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    git(dir.path(), &["init", "--bare", remote.to_str().unwrap()]);
    git(dir.path(), &["init", "--bare", other.to_str().unwrap()]);
    git(&checkout, &["init", "--initial-branch=main"]);
    let selected = FileSelection {
        directory: "docs/spec",
        suffix: ".md",
    };
    std::fs::create_dir_all(checkout.join("docs/spec/sub")).unwrap();
    std::fs::write(checkout.join("docs/spec/a.md"), "a\n").unwrap();
    std::fs::write(checkout.join("docs/spec/sub/b.md"), "b\n").unwrap();
    std::fs::write(checkout.join("docs/spec/skip.txt"), "x\n").unwrap();
    std::fs::write(checkout.join("docs/other.md"), "x\n").unwrap();
    git(&checkout, &["add", "-A"]);
    let first = commit(&checkout, "first");
    git(
        &checkout,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&checkout, &["push", "-q", "origin", "main"]);
    git(&checkout, &["fetch", "-q", "origin"]);
    let runner = NativeGit::cli(Duration::from_secs(10)).unwrap();

    // Only the selection, at any depth.
    let files = runner
        .commit_files(&checkout, &first, selected, 1024)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        ["docs/spec/a.md", "docs/spec/sub/b.md"]
    );
    assert!(runner
        .commit_files(&checkout, &first, selected, 1)
        .await
        .unwrap()
        .is_none());
    std::fs::write(checkout.join("docs/spec/new.md"), "n\n").unwrap();
    assert_eq!(
        runner
            .worktree_files(&checkout, selected, false)
            .await
            .unwrap(),
        ["docs/spec/new.md"]
    );
    assert_eq!(
        runner
            .worktree_files(&checkout, selected, true)
            .await
            .unwrap()
            .len(),
        3
    );

    // An unpushed commit: the base stays the pushed one; the patch covers both.
    std::fs::write(checkout.join("docs/spec/a.md"), "A\n").unwrap();
    git(&checkout, &["commit", "-qam", "unpushed"]);
    std::fs::write(checkout.join("docs/spec/sub/b.md"), "B\n").unwrap();
    let index = std::fs::read(checkout.join(".git/index")).unwrap();
    assert_eq!(
        runner.pushed_base(&checkout, "origin").await.unwrap(),
        Some(first.clone())
    );
    let patch = runner
        .worktree_patch(&checkout, &first, selected, 1024 * 1024)
        .await
        .unwrap()
        .unwrap();
    let patch = String::from_utf8(patch).unwrap();
    assert!(patch.contains("+A\n") && patch.contains("+B\n"), "{patch}");
    assert!(!patch.contains("skip.txt") && !patch.contains("new.md"));
    assert!(runner
        .worktree_patch(&checkout, &first, selected, 8)
        .await
        .unwrap()
        .is_none());
    assert_eq!(std::fs::read(checkout.join(".git/index")).unwrap(), index);

    // Selections never become pathspec magic or escape the checkout.
    for (directory, suffix) in [
        ("docs/*", ".md"),
        ("../docs", ".md"),
        ("/docs", ".md"),
        ("docs", "*.md"),
        ("docs", "a/.md"),
        (":(top)docs", ".md"),
        ("", ".md"),
    ] {
        let selection = FileSelection { directory, suffix };
        let error = runner
            .worktree_files(&checkout, selection, true)
            .await
            .unwrap_err();
        assert_eq!(
            error.kind,
            FailureKind::InvalidInput,
            "{directory} {suffix}"
        );
    }

    // A commit lost locally is fetched by ID without writing refs or FETCH_HEAD.
    git(&checkout, &["reset", "-q", "--hard", &first]);
    git(
        &checkout,
        &["push", "-q", "-f", "origin", "HEAD:refs/heads/keep"],
    );
    let lost = {
        let clone = dir.path().join("clone");
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                "-b",
                "main",
                remote.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        std::fs::write(clone.join("docs/spec/a.md"), "remote\n").unwrap();
        git(&clone, &["commit", "-qam", "remote only"]);
        git(&clone, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
        git(&clone, &["rev-parse", "HEAD"])
    };
    let refs_before = git(&checkout, &["for-each-ref"]);
    let fetch_head = std::fs::read(checkout.join(".git/FETCH_HEAD")).unwrap();
    assert_eq!(runner.object_kind(&checkout, &lost).await.unwrap(), None);
    // The configured origin is not the expected repository: refused, nothing fetched.
    let error = runner
        .fetch_commit(
            &checkout,
            "origin",
            other.to_str().unwrap(),
            &lost,
            &GitAuth::None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Destination, "{error}");
    assert_eq!(runner.object_kind(&checkout, &lost).await.unwrap(), None);
    runner
        .fetch_commit(
            &checkout,
            "origin",
            remote.to_str().unwrap(),
            &lost,
            &GitAuth::None,
        )
        .await
        .unwrap();
    assert_eq!(
        runner.object_kind(&checkout, &lost).await.unwrap(),
        Some(NativeObjectKind::Commit)
    );
    assert_eq!(git(&checkout, &["for-each-ref"]), refs_before);
    assert_eq!(
        std::fs::read(checkout.join(".git/FETCH_HEAD")).unwrap(),
        fetch_head
    );
}
