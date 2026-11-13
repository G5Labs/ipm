use super::tests::{commit, git};
use super::*;
use tokio::io::AsyncReadExt;

fn packet(payload: &[u8]) -> Vec<u8> {
    let mut bytes = format!("{:04x}", payload.len() + 4).into_bytes();
    bytes.extend_from_slice(payload);
    bytes
}
fn request(updates: &[ReceiveRefUpdate], atomic: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (index, update) in updates.iter().enumerate() {
        bytes.extend(packet(
            format!(
                "{} {} {}{}\n",
                update.old_oid,
                update.new_oid,
                update.reference,
                if index == 0 {
                    if atomic {
                        "\0report-status atomic"
                    } else {
                        "\0report-status"
                    }
                } else {
                    ""
                }
            )
            .as_bytes(),
        ));
    }
    bytes.extend_from_slice(b"0000");
    bytes.extend_from_slice(&[
        80, 65, 67, 75, 0, 0, 0, 2, 0, 0, 0, 0, 2, 157, 8, 130, 59, 216, 168, 234, 181, 16, 173,
        106, 199, 92, 130, 60, 253, 62, 211, 30,
    ]);
    bytes
}
fn runner() -> NativeGit {
    NativeGit::api(&process::resolve_binary().unwrap(), Duration::from_secs(10)).unwrap()
}

#[tokio::test]
async fn protocols_preserve_large_binary_fetches_and_worktree_receive_behavior() {
    for bare in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "--initial-branch=main"]);
        let old = commit(&source, "old");
        let mut random = 17u32;
        let blob: Vec<u8> = (0..512 * 1024)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        std::fs::write(source.join("binary"), &blob).unwrap();
        git(&source, &["add", "binary"]);
        let new = commit(&source, "binary");
        let repo = if bare {
            let repo = dir.path().join("bare.git");
            git(
                dir.path(),
                &[
                    "clone",
                    "--bare",
                    source.to_str().unwrap(),
                    repo.to_str().unwrap(),
                ],
            );
            git(&repo, &["update-ref", "refs/heads/main", &old]);
            repo
        } else {
            git(&source, &["reset", "--hard", &old]);
            source
        };
        let runner = runner();
        let advertisement = runner
            .advertise_refs(&repo, GitService::UploadPack)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&advertisement).contains(&old));
        let update = ReceiveRefUpdate {
            reference: "refs/heads/main".into(),
            old_oid: old.clone(),
            new_oid: new.clone(),
            fast_forward_only: false,
        };
        let result = runner
            .receive_pack(
                &repo,
                request(std::slice::from_ref(&update), true),
                &[update],
            )
            .await
            .unwrap();
        assert!(result.atomic && result.has_report && result.failure.is_none());
        assert_eq!(result.statuses.len(), 1);
        assert!(result.statuses[0].accepted, "{result:?}");
        assert_eq!(git(&repo, &["rev-parse", "main"]), new);
        if !bare {
            assert_eq!(std::fs::read(repo.join("binary")).unwrap(), blob);
        }
        let mut fetch = packet(format!("want {new}\n").as_bytes());
        fetch.extend_from_slice(b"00000009done\n");
        let mut output = Vec::new();
        runner
            .upload_pack(&repo, fetch)
            .await
            .unwrap()
            .read_to_end(&mut output)
            .await
            .unwrap();
        assert!(output.starts_with(b"0008NAK\nPACK"));
        assert!(
            output.len() > 512 * 1024,
            "pack must not be diagnostic-truncated"
        );
    }
}

#[tokio::test]
async fn receive_scope_cas_and_atomicity_are_enforced_before_or_by_git() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--bare"]);
    // Use a separate checkout to produce objects, then copy them through normal Git.
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let old = commit(&source, "old");
    let new = commit(&source, "new");
    git(&source, &["push", dir.path().to_str().unwrap(), "main"]);
    // `newer` exists remotely only as an object, so a wrongly accepted main
    // update would be observable as main moving from `new` to `newer`.
    let newer = commit(&source, "newer");
    git(
        &source,
        &[
            "push",
            dir.path().to_str().unwrap(),
            "main:refs/heads/staging",
        ],
    );
    let runner = runner();
    let updates = vec![
        ReceiveRefUpdate {
            reference: "refs/heads/created".into(),
            old_oid: "0".repeat(40),
            new_oid: new.clone(),
            fast_forward_only: false,
        },
        ReceiveRefUpdate {
            reference: "refs/heads/main".into(),
            // Stale: main is at `new`.
            old_oid: old,
            new_oid: newer.clone(),
            fast_forward_only: false,
        },
    ];
    let main_tip = || async { runner.ref_tip(dir.path(), "refs/heads/main").await.unwrap() };
    let mismatch = runner
        .receive_pack(dir.path(), request(&updates, false), &updates[..1])
        .await
        .unwrap_err();
    assert_eq!(
        (mismatch.kind, mismatch.effect),
        (FailureKind::InvalidInput, FailureEffect::Refused)
    );
    assert!(runner
        .ref_tip(dir.path(), "refs/heads/created")
        .await
        .unwrap()
        .is_none());
    let atomic = runner
        .receive_pack(dir.path(), request(&updates, true), &updates)
        .await
        .unwrap();
    assert!(atomic.atomic && atomic.statuses.iter().all(|status| !status.accepted));
    assert!(runner
        .ref_tip(dir.path(), "refs/heads/created")
        .await
        .unwrap()
        .is_none());
    assert_eq!(main_tip().await, Some(new.clone()));
    let partial = runner
        .receive_pack(dir.path(), request(&updates, false), &updates)
        .await
        .unwrap();
    assert!(
        !partial.atomic && partial.statuses[0].accepted && !partial.statuses[1].accepted,
        "{partial:?}"
    );
    assert_eq!(
        runner
            .ref_tip(dir.path(), "refs/heads/created")
            .await
            .unwrap(),
        Some(new.clone())
    );
    assert_eq!(main_tip().await, Some(new));
    assert_ne!(main_tip().await, Some(newer));
}

#[tokio::test]
async fn fast_forward_only_commands_refuse_a_rewrite_that_policy_would_otherwise_admit() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "--bare"]);
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let base = commit(&source, "base");
    let tip = commit(&source, "tip");
    git(&source, &["push", dir.path().to_str().unwrap(), "main"]);
    // `ahead` descends from the tip; `side` does not. Both reach the remote
    // as objects only, so each update below sends an empty pack.
    let ahead = commit(&source, "ahead");
    git(&source, &["reset", "--hard", &base]);
    let side = commit(&source, "side");
    for (name, oid) in [("ahead", &ahead), ("side", &side)] {
        git(
            &source,
            &[
                "push",
                dir.path().to_str().unwrap(),
                &format!("{oid}:refs/heads/{name}"),
            ],
        );
    }
    let runner = runner();
    let update = |new: &str, fast_forward_only: bool, old: &str| ReceiveRefUpdate {
        reference: "refs/heads/main".into(),
        old_oid: old.to_owned(),
        new_oid: new.to_owned(),
        fast_forward_only,
    };
    let main_tip = || async { runner.ref_tip(dir.path(), "refs/heads/main").await.unwrap() };

    let rewrite = update(&side, true, &tip);
    let refused = runner
        .receive_pack(
            dir.path(),
            request(std::slice::from_ref(&rewrite), false),
            &[rewrite],
        )
        .await
        .unwrap();
    assert!(!refused.statuses[0].accepted, "{refused:?}");
    assert_eq!(main_tip().await, Some(tip.clone()), "a rewrite never lands");

    let forward = update(&ahead, true, &tip);
    let accepted = runner
        .receive_pack(
            dir.path(),
            request(std::slice::from_ref(&forward), false),
            &[forward],
        )
        .await
        .unwrap();
    assert!(accepted.statuses[0].accepted, "{accepted:?}");
    assert_eq!(main_tip().await, Some(ahead.clone()));

    // Without the flag the same rewrite is Git's to accept, as before.
    let rewrite = update(&side, false, &ahead);
    let rewritten = runner
        .receive_pack(
            dir.path(),
            request(std::slice::from_ref(&rewrite), false),
            &[rewrite],
        )
        .await
        .unwrap();
    assert!(rewritten.statuses[0].accepted, "{rewritten:?}");
    assert_eq!(main_tip().await, Some(side.clone()));

    // One push cannot mix the two policies.
    let mixed = [update(&tip, true, &side), {
        let mut other = update(&base, false, "0".repeat(40).as_str());
        other.reference = "refs/heads/other".into();
        other
    }];
    let error = runner
        .receive_pack(dir.path(), request(&mixed, false), &mixed)
        .await
        .unwrap_err();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::InvalidInput, FailureEffect::Refused)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn protocol_buffers_are_distinct_from_diagnostics_and_late_exit_keeps_acceptance() {
    use super::tests::script;
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "for arg in \"$@\"; do case \"$arg\" in check-ref-format) exit 0;; --show-object-format) echo sha1; exit 0;; esac; done\n/bin/cat >/dev/null\n/bin/dd if=/dev/zero bs=1024 count=256 1>&2 2>/dev/null\nprintf '000eunpack ok\\n0017ok refs/heads/main\\n0000'\nexit 7");
    let runner = NativeGit::api(&binary, Duration::from_secs(5)).unwrap();
    let update = ReceiveRefUpdate {
        reference: "refs/heads/main".into(),
        old_oid: "0".repeat(40),
        new_oid: "a".repeat(40),
        fast_forward_only: false,
    };
    let result = runner
        .receive_pack(
            dir.path(),
            request(std::slice::from_ref(&update), false),
            &[update],
        )
        .await
        .unwrap();
    assert!(result.statuses[0].accepted, "{result:?}");
    let error = result.failure.unwrap();
    assert_eq!(
        (error.kind, error.effect),
        (FailureKind::Exit, FailureEffect::Partial)
    );
    assert!(error.detail.len() < 66 * 1024);
    assert!(error.detail.contains("output truncated"), "{error:?}");
    let binary = script(dir.path(), "for arg in \"$@\"; do case \"$arg\" in check-ref-format) exit 0;; --show-object-format) echo sha1; exit 0;; esac; done\nprintf '000eunpack ok\\n0017ok refs/heads/main\\n0000'\nexit 7");
    let runner = NativeGit::api(&binary, Duration::from_secs(5)).unwrap();
    let update = ReceiveRefUpdate {
        reference: "refs/heads/main".into(),
        old_oid: "0".repeat(40),
        new_oid: "a".repeat(40),
        fast_forward_only: false,
    };
    let mut input = request(std::slice::from_ref(&update), false);
    input.extend(vec![0; 1024 * 1024]);
    let result = runner
        .receive_pack(dir.path(), input, &[update])
        .await
        .unwrap();
    assert!(result.statuses[0].accepted);
    assert_eq!(result.failure.unwrap().effect, FailureEffect::Partial);
    // Advertisement bytes larger than the diagnostic cap remain intact.
    let binary = script(
        dir.path(),
        "/bin/dd if=/dev/zero bs=1024 count=256 2>/dev/null",
    );
    let runner = NativeGit::api(&binary, Duration::from_secs(5)).unwrap();
    assert_eq!(
        runner
            .advertise_refs(dir.path(), GitService::UploadPack)
            .await
            .unwrap(),
        vec![0; 256 * 1024]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn upload_stream_reports_exit_failure_and_reaps_on_deadline_or_drop() {
    use super::tests::{assert_reaped, script};
    for mode in ["exit", "deadline", "drop"] {
        let dir = tempfile::tempdir().unwrap();
        let body = if mode == "exit" {
            "printf binary; echo 'fatal: failed upload' >&2; exit 2"
        } else {
            "echo $$ > pid; echo attempt >> attempts; printf binary; while :; do :; done"
        };
        let binary = script(dir.path(), &format!("cd \"$2\" || exit 90\n{body}"));
        let cancellation = NativeCancellation::default();
        let runner = NativeGit::api(&binary, Duration::from_millis(1500))
            .unwrap()
            .with_cancellation(cancellation.clone());
        let mut stream = runner.upload_pack(dir.path(), vec![]).await.unwrap();
        if mode == "drop" {
            let mut prefix = [0; 6];
            stream.read_exact(&mut prefix).await.unwrap();
            assert_eq!(&prefix, b"binary");
            drop(stream);
        } else {
            let mut output = Vec::new();
            let error = stream.read_to_end(&mut output).await.unwrap_err();
            assert!(error.to_string().contains(if mode == "exit" {
                "failed upload"
            } else {
                "deadline"
            }));
            assert_eq!(output, b"binary");
        }
        tokio::time::timeout(Duration::from_secs(3), cancellation.wait_idle())
            .await
            .unwrap();
        if mode != "exit" {
            let pid = std::fs::read_to_string(dir.path().join("pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_reaped(pid).await;
            assert_eq!(
                std::fs::read_to_string(dir.path().join("attempts")).unwrap(),
                "attempt\n"
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn upload_stream_backpressure_preserves_budget_and_cancellation() {
    use super::tests::{assert_reaped, script};
    for mode in ["complete", "deadline", "stalled", "drop"] {
        let dir = tempfile::tempdir().unwrap();
        let tail = if matches!(mode, "complete" | "stalled") {
            "exit 0"
        } else {
            "while :; do :; done"
        };
        let binary = script(dir.path(), &format!(
            "cd \"$2\" || exit 90\necho $$ > pid\n/bin/dd if=/dev/zero bs=1024 count=512 2>/dev/null\n{tail}"
        ));
        let cancellation = NativeCancellation::default();
        let runner = NativeGit::api(&binary, Duration::from_millis(1500))
            .unwrap()
            .with_cancellation(cancellation.clone());
        let mut stream = runner.upload_pack(dir.path(), vec![]).await.unwrap();
        let mut bytes = Vec::new();
        match mode {
            "drop" => {
                tokio::time::sleep(Duration::from_millis(2500)).await;
                drop(stream);
            }
            "stalled" => {
                // One delivery blocked longer than the whole budget: the
                // connected-but-silent reader no longer holds Git.
                tokio::time::sleep(Duration::from_millis(2500)).await;
                let error = stream.read_to_end(&mut bytes).await.unwrap_err();
                assert!(error.to_string().contains("accepted no data"), "{error}");
                assert!(bytes.len() < 512 * 1024);
            }
            _ => {
                // Several pauses, each shorter than the budget but together
                // longer, are excluded from Git's execution time.
                let mut chunk = vec![0; 64 * 1024];
                for _ in 0..3 {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    let read = stream.read(&mut chunk).await.unwrap();
                    bytes.extend_from_slice(&chunk[..read]);
                }
                let result = stream.read_to_end(&mut bytes).await;
                if mode == "complete" {
                    result.unwrap();
                } else {
                    let error = result.unwrap_err().to_string();
                    assert!(
                        error.contains("deadline") && !error.contains("accepted no data"),
                        "{error}"
                    );
                }
                assert_eq!(bytes, vec![0; 512 * 1024]);
            }
        }
        tokio::time::timeout(Duration::from_secs(3), cancellation.wait_idle())
            .await
            .unwrap();
        let pid = std::fs::read_to_string(dir.path().join("pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_reaped(pid).await;
    }
}
