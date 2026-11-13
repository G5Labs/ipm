use super::tests::{commit, git};
use super::*;

fn host_environment() -> HostGitEnvironment {
    HostGitEnvironment {
        search_path: std::env::var_os("PATH").unwrap(),
        home: None,
        xdg_config_home: None,
        global_config: Some(PathBuf::from("/dev/null")),
        system_config: false,
        system_config_path: None,
        transport: Default::default(),
        ssh_agent_socket: None,
        library_path: std::env::var_os("LD_LIBRARY_PATH"),
        dyld_library_path: std::env::var_os("DYLD_LIBRARY_PATH"),
        ca_bundle: None,
        ca_directory: None,
    }
}

#[test]
fn pinned_bundle_checkout_is_detached_and_refuses_existing_or_unpinned_state() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let initial = commit(&source, "initial");
    let bundle = temp.path().join("source.bundle");
    git(
        &source,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
    );
    let destination = temp.path().join("seed");
    std::fs::create_dir(&destination).unwrap();
    let runner = NativeGit::host(host_environment(), Duration::from_secs(15)).unwrap();
    assert_eq!(
        runner
            .prepare_bundle_checkout_blocking(&destination, &bundle, "HEAD")
            .unwrap_err()
            .effect,
        FailureEffect::Refused
    );
    assert!(std::fs::read_dir(&destination).unwrap().next().is_none());
    runner
        .prepare_bundle_checkout_blocking(&destination, &bundle, &initial)
        .unwrap();
    assert_eq!(git(&destination, &["rev-parse", "HEAD"]), initial);
    assert_eq!(
        std::fs::read_to_string(destination.join(".git/HEAD"))
            .unwrap()
            .trim(),
        initial
    );
    assert_eq!(git(&destination, &["remote"]), "");
    assert_eq!(
        runner.object_kind_blocking(&destination, &initial).unwrap(),
        Some(NativeObjectKind::Commit)
    );
    assert_eq!(
        runner
            .object_kind_blocking(&destination, &"0".repeat(40))
            .unwrap(),
        None
    );
    std::fs::write(destination.join("user-work"), b"keep").unwrap();
    assert_eq!(
        runner
            .prepare_bundle_checkout_blocking(&destination, &bundle, &initial)
            .unwrap_err()
            .effect,
        FailureEffect::Refused
    );
    assert_eq!(
        std::fs::read(destination.join("user-work")).unwrap(),
        b"keep"
    );
}

#[tokio::test]
async fn host_cache_refresh_preserves_all_branches_prunes_and_pins_exact_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let initial = commit(&source, "initial");
    git(&source, &["branch", "deleted"]);
    let cache = temp.path().join("cache");
    let runner = NativeGit::host(host_environment(), Duration::from_secs(15)).unwrap();
    runner
        .clone_bare_cache(source.to_str().unwrap(), &cache, &GitAuth::None)
        .await
        .unwrap();
    git(&source, &["branch", "new"]);
    git(&source, &["branch", "-D", "deleted"]);
    let newer = commit(&source, "newer");
    runner
        .refresh_bare_cache(&cache, source.to_str().unwrap(), &GitAuth::None)
        .await
        .unwrap();
    assert_eq!(git(&cache, &["rev-parse", "main"]), newer);
    assert_eq!(git(&cache, &["rev-parse", "new"]), initial);
    assert_eq!(git(&cache, &["for-each-ref", "refs/heads/deleted"]), "");
    let checkout = temp.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    // A fresh bare clone has the same layout as the owner's reflink copy.
    runner
        .clone_bare_cache(
            source.to_str().unwrap(),
            &checkout.join(".git"),
            &GitAuth::None,
        )
        .await
        .unwrap();
    runner
        .materialize_exact_checkout(&checkout, "main", &initial)
        .await
        .unwrap();
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), initial);
    assert_eq!(git(&checkout, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    let err = runner
        .materialize_exact_checkout(&checkout, "main", "HEAD")
        .await
        .unwrap_err();
    assert_eq!(err.effect, FailureEffect::Refused);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), initial);
}

#[cfg(unix)]
#[tokio::test]
async fn host_cache_authentication_is_scoped_and_absent_from_arguments() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("trace");
    let script = temp.path().join("git");
    // Record only the argument list and selected invocation controls in this
    // isolated fake transport. Real shared HTTP tests verify delivery/redirects.
    std::fs::write(
        &script,
        format!(
            r##"#!/bin/sh
case "$*" in
  *ls-remote*) printf '%s\n' 'https://example.invalid/repo.git';;
  *) printf '%s\n' "$@" > '{trace}'
     env | sort > '{trace}.env';;
esac
"##,
            trace = trace.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut environment = host_environment();
    environment.search_path =
        std::env::join_paths([temp.path(), Path::new("/usr/bin"), Path::new("/bin")]).unwrap();
    environment.xdg_config_home = Some(temp.path().join("xdg"));
    environment.system_config_path = Some(temp.path().join("system"));
    environment.transport.https_proxy =
        Some("http://proxy-user:proxy-secret@example.invalid".into());
    environment.transport.no_proxy = Some("example.invalid".into());
    environment.transport.ssh_command = Some("ssh -F /selected/ssh-config".into());
    assert!(!format!("{environment:?}").contains("proxy-secret"));
    let git = NativeGit::host(environment, Duration::from_secs(5)).unwrap();
    git.clone_bare_cache(
        "https://example.invalid/repo.git",
        &temp.path().join("cache"),
        &GitAuth::Token("selected-host-secret".into()),
    )
    .await
    .unwrap();
    let args = std::fs::read_to_string(&trace).unwrap();
    assert!(!args.contains("selected-host-secret"));
    assert!(!args.contains("extraHeader"));
    let env = std::fs::read_to_string(trace.with_extension("env")).unwrap();
    assert!(env.contains("http.https://example.invalid/repo.git.extraHeader"));
    assert!(env.contains("http.followRedirects"));
    assert!(env.contains("Authorization: Basic "));
    assert!(!env.contains("G5D_TOKEN="));
    assert!(env.contains(&format!(
        "XDG_CONFIG_HOME={}",
        temp.path().join("xdg").display()
    )));
    assert!(env.contains(&format!(
        "GIT_CONFIG_SYSTEM={}",
        temp.path().join("system").display()
    )));
    assert!(env.contains("https_proxy=http://proxy-user:proxy-secret@example.invalid"));
    assert!(env.contains("no_proxy=example.invalid"));
    assert!(env.contains("GIT_SSH_COMMAND=ssh -F /selected/ssh-config"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn host_cache_supports_non_utf8_destination_paths() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let oid = commit(&source, "initial");
    let cache = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"cache-\xff".to_vec()));
    let runner = NativeGit::host(host_environment(), Duration::from_secs(15)).unwrap();
    runner
        .clone_bare_cache(source.to_str().unwrap(), &cache, &GitAuth::None)
        .await
        .unwrap();
    runner
        .refresh_bare_cache(&cache, source.to_str().unwrap(), &GitAuth::None)
        .await
        .unwrap();
    assert_eq!(git(&cache, &["rev-parse", "main"]), oid);
}

#[cfg(unix)]
#[tokio::test]
async fn host_cache_preserves_destination_argument_bytes() {
    use std::os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    };
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("args");
    let script = temp.path().join("git");
    std::fs::write(&script, format!("#!/bin/sh\ncase \"$*\" in *ls-remote*) printf '%s\\n' 'https://example.invalid/repo.git';; *) printf '%s\\0' \"$@\" > '{}';; esac\n", trace.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut environment = host_environment();
    environment.search_path = std::env::join_paths([temp.path(), Path::new("/bin")]).unwrap();
    let runner = NativeGit::host(environment, Duration::from_secs(5)).unwrap();
    let destination = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"cache-\xff".to_vec()));
    runner
        .clone_bare_cache(
            "https://example.invalid/repo.git",
            &destination,
            &GitAuth::None,
        )
        .await
        .unwrap();
    let args = std::fs::read(trace).unwrap();
    assert!(args
        .split(|byte| *byte == 0)
        .any(|arg| arg == destination.as_os_str().as_bytes()));
}

#[tokio::test]
async fn host_checkout_refuses_damaged_child_without_mutating_parent() {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "--initial-branch=main"]);
    let oid = commit(temp.path(), "parent");
    let before = std::fs::read(temp.path().join(".git/config")).unwrap();
    let child = temp.path().join("damaged");
    std::fs::create_dir(&child).unwrap();
    let runner = NativeGit::host(host_environment(), Duration::from_secs(15)).unwrap();
    assert!(runner
        .materialize_exact_checkout(&child, "wrong", &oid)
        .await
        .is_err());
    assert_eq!(
        std::fs::read(temp.path().join(".git/config")).unwrap(),
        before
    );
    assert_eq!(
        git(temp.path(), &["symbolic-ref", "HEAD"]),
        "refs/heads/main"
    );
    assert_eq!(git(temp.path(), &["for-each-ref", "refs/heads/wrong"]), "");
}

#[tokio::test]
async fn host_restore_refuses_wrong_oid_width_before_fetch() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let checkout = temp.path().join("checkout");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&checkout).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let oid = commit(&source, "original");
    git(&checkout, &["init", "--initial-branch=main"]);
    let runner = NativeGit::host(host_environment(), Duration::from_secs(15)).unwrap();
    let mut request = RestoreCheckout {
        repository: checkout.clone(),
        url: source.to_str().unwrap().into(),
        workspace_id: uuid::Uuid::new_v4(),
        repo_id: uuid::Uuid::new_v4(),
        branch: "main".into(),
        branch_oid: "a".repeat(64),
        upstream_branch: None,
        snapshot: None,
    };
    for snapshot_case in [false, true] {
        if snapshot_case {
            request.branch_oid = oid.clone();
            request.snapshot = Some(RestoreSnapshot {
                flush_id: uuid::Uuid::new_v4(),
                local_branch_oid: oid.clone(),
                local_head_oid: oid.clone(),
                snapshot_oid: "a".repeat(64),
                head_ref: None,
            });
        }
        let err = runner
            .restore_checkout(&request, &GitAuth::None, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(err.effect, FailureEffect::Refused);
        assert!(err.detail.contains("object format"), "{err}");
        assert_eq!(git(&checkout, &["for-each-ref"]), "");
        assert!(!checkout.join(".git/FETCH_HEAD").exists());
    }
}

/// Match the API's protocol-v0 advertisement and stateless upload-pack path.
/// File transport bypasses remote-curl and cannot detect lost negotiation tips.
async fn bootstrap_http_fixture(source: PathBuf) -> (String, tokio::task::JoinSet<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/repo", listener.local_addr().unwrap());
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
        let server = NativeGit::api(Path::new("git"), Duration::from_secs(15)).unwrap();
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
                assert!(request.len() < 16384);
            }
            let headers = String::from_utf8(request).unwrap();
            let length = headers.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            }).unwrap_or(0);
            assert!(length < 65536);
            let mut request = vec![0; length];
            socket.read_exact(&mut request).await.unwrap();
            let (kind, body) = if headers.starts_with("GET /repo/info/refs?service=git-upload-pack ") {
                let mut body = b"001e# service=git-upload-pack\n0000".to_vec();
                body.extend(server.advertise_refs(&source, GitService::UploadPack).await.unwrap());
                ("advertisement", body)
            } else {
                assert!(headers.starts_with("POST /repo/git-upload-pack "));
                let mut body = Vec::new();
                server.upload_pack(&source, request).await.unwrap()
                    .read_to_end(&mut body).await.unwrap();
                ("result", body)
            };
            socket.write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-git-upload-pack-{kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()
            ).as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
        }
    });
    (url, tasks)
}

#[tokio::test]
async fn bootstrap_restore_reuses_detached_seed_history_for_dirty_receipts() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    // Enough incompressible objects to expose a redundant pack, rather than
    // letting unpack-objects silently discard duplicates in a tiny fixture.
    let mut random = 17u32;
    for index in 0..160 {
        let blob: Vec<u8> = (0..8192)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        std::fs::write(source.join(format!("file-{index}")), blob).unwrap();
    }
    git(&source, &["add", "."]);
    let baseline = commit(&source, "seed baseline");
    let bundle = temp.path().join("seed.bundle");
    git(
        &source,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
    );
    std::fs::write(source.join("committed"), b"new since seed\n").unwrap();
    git(&source, &["add", "."]);
    let local_head = commit(&source, "workspace commit after seed");
    std::fs::write(source.join("file-0"), b"uncommitted change\n").unwrap();
    std::fs::remove_file(source.join("file-1")).unwrap();
    std::fs::write(source.join("new-binary"), [0, 1, 255, 0]).unwrap();
    git(&source, &["add", "-A"]);
    let snapshot = commit(&source, "receipt snapshot");
    let workspace_id = uuid::Uuid::new_v4();
    let repo_id = uuid::Uuid::new_v4();
    let flush_id = uuid::Uuid::new_v4();
    for (leaf, oid) in [("snapshot", &snapshot), ("branch", &local_head)] {
        git(
            &source,
            &[
                "update-ref",
                &format!("refs/g5/flush/{workspace_id}/{flush_id}/{leaf}"),
                oid,
            ],
        );
    }
    // A newer remote branch must not replace the exact receipt HEAD.
    let remote_head = commit(&source, "new remote tip");
    let host = NativeGit::host(host_environment(), Duration::from_secs(30)).unwrap();
    let mut bootstrap = host.clone();
    bootstrap.context = Context::Bootstrap(host_environment());
    bootstrap.bootstrap_test_root = Some(temp.path().canonicalize().unwrap());
    let (http_url, _http_tasks) = bootstrap_http_fixture(source.clone()).await;
    for (transport, protocol) in [("file", "0"), ("file", "2"), ("http", "0")] {
        for seeded in [true, false] {
            let checkout = temp
                .path()
                .join(format!("restore-{transport}-{protocol}-{seeded}"));
            std::fs::create_dir(&checkout).unwrap();
            let checkout = checkout.canonicalize().unwrap();
            if seeded {
                host.prepare_bundle_checkout_blocking(&checkout, &bundle, &baseline)
                    .unwrap();
                assert_eq!(git(&checkout, &["for-each-ref"]), "");
                assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), baseline);
            }
            // Exercise the complete preparation entrypoint, including config
            // sanitization for a seed and initialization for an absent member.
            let canonical_url = "https://g5.example.invalid/git/member.git";
            bootstrap
                .prepare_bootstrap_checkout(&checkout, &remote_head, canonical_url)
                .await
                .unwrap();
            git(&checkout, &["config", "protocol.version", protocol]);
            let request = RestoreCheckout {
                repository: checkout.clone(),
                url: if transport == "http" {
                    http_url.clone()
                } else {
                    url::Url::from_file_path(&source).unwrap().to_string()
                },
                workspace_id,
                repo_id,
                branch: "main".into(),
                branch_oid: remote_head.clone(),
                upstream_branch: Some("missing-upstream".into()),
                snapshot: Some(RestoreSnapshot {
                    flush_id,
                    local_branch_oid: local_head.clone(),
                    local_head_oid: local_head.clone(),
                    snapshot_oid: snapshot.clone(),
                    head_ref: Some("refs/heads/main".into()),
                }),
            };
            let result = bootstrap
                .restore_checkout(&request, &GitAuth::None, Duration::from_secs(5))
                .await
                .unwrap();
            assert!(result.upstream_failure.is_some());
            assert_eq!(git(&checkout, &["remote"]), "origin");
            assert_eq!(
                git(&checkout, &["config", "--local", "remote.origin.url"]),
                canonical_url
            );
            assert_eq!(
                git(&checkout, &["config", "--get-all", "remote.origin.fetch"]),
                "+refs/heads/*:refs/remotes/origin/*"
            );
            // Exercise ordinary Git against origin, not another direct URL
            // fetch that would conceal a missing remote/refspec again. Use the
            // same scoped routing/upstream configuration installed by dispatch.
            let native_config = temp
                .path()
                .join(format!("native-git-{transport}-{protocol}-{seeded}"));
            configuration::configure_member_remotes(
                &native_config,
                &[configuration::MemberRemoteConfiguration {
                    checkout: checkout.clone(),
                    config_path: native_config.with_extension("member"),
                    canonical_url: canonical_url.into(),
                    transport_url: request.url.clone(),
                    branch: request.branch.clone(),
                }],
            )
            .unwrap();
            let fetch_checkout = checkout.clone();
            let include = format!("include.path={}", native_config.display());
            tokio::task::spawn_blocking(move || {
                git(
                    &fetch_checkout,
                    &["-c", &include, "fetch", "--quiet", "origin"],
                );
                assert_eq!(
                    git(
                        &fetch_checkout,
                        &[
                            "-c",
                            &include,
                            "rev-parse",
                            "--symbolic-full-name",
                            "@{upstream}",
                        ],
                    ),
                    "refs/remotes/origin/main"
                );
            })
            .await
            .unwrap();
            assert_eq!(
                git(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
                remote_head
            );
            assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), local_head);
            assert_eq!(
                std::fs::read(checkout.join("file-0")).unwrap(),
                b"uncommitted change\n"
            );
            assert!(!checkout.join("file-1").exists());
            assert_eq!(
                std::fs::read(checkout.join("new-binary")).unwrap(),
                [0, 1, 255, 0]
            );
            assert_eq!(
                std::fs::read(checkout.join("committed")).unwrap(),
                b"new since seed\n"
            );
            assert_eq!(git(&checkout, &["diff", "--cached", "--name-only"]), "");
            assert_eq!(
                git(&checkout, &["for-each-ref", "refs/g5/restore/negotiation"]),
                ""
            );
            git(&checkout, &["fsck", "--full"]);
            if seeded {
                assert!(
                    result.fetched_kib < 128,
                    "{transport} protocol {protocol}: restore fetched {} KiB despite a populated seed",
                    result.fetched_kib
                );
            }
            let error = bootstrap
                .restore_checkout(
                    &RestoreCheckout {
                        branch: "absent".into(),
                        ..request.clone()
                    },
                    &GitAuth::None,
                    Duration::from_secs(5),
                )
                .await
                .unwrap_err();
            assert_eq!(error.kind, FailureKind::Exit);
            assert_eq!(
                git(&checkout, &["for-each-ref", "refs/g5/restore/negotiation"]),
                ""
            );
            assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), local_head);
        }
    }
}

#[tokio::test]
async fn bootstrap_preparation_replaces_seed_configuration_and_refuses_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().canonicalize().unwrap();
    git(&checkout, &["init", "--template="]);
    git(
        &checkout,
        &["remote", "add", "origin", "https://old.invalid/seed.git"],
    );
    git(&checkout, &["config", "credential.helper", "!false"]);
    git(
        &checkout,
        &["config", "core.hooksPath", "/unused-seed-hooks"],
    );
    let mut bootstrap = NativeGit::host(host_environment(), Duration::from_secs(10)).unwrap();
    bootstrap.context = Context::Bootstrap(host_environment());
    let oid = "a".repeat(40);
    let canonical = "https://example.invalid/member.git";
    // Production still refuses arbitrary roots. Only this unit-test runner can
    // substitute its private temporary namespace for the fixed guest /work.
    assert!(bootstrap
        .validate_bootstrap_checkout(&checkout, &oid, canonical)
        .is_err());
    bootstrap.bootstrap_test_root = Some(checkout.clone());
    let config = checkout.join(".git/config");
    let before = std::fs::read(&config).unwrap();
    for url in [
        "https://user:secret@example.invalid/repo.git",
        "https://example.invalid/repo.git?token=secret",
        "https://example.invalid/repo.git\n[credential]",
    ] {
        assert!(bootstrap
            .validate_bootstrap_checkout(&checkout, &oid, url)
            .is_err());
        assert!(bootstrap
            .prepare_bootstrap_checkout(&checkout, &oid, url)
            .await
            .is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
    }
    bootstrap
        .prepare_bootstrap_checkout(&checkout, &oid, canonical)
        .await
        .unwrap();
    assert_eq!(git(&checkout, &["remote", "get-url", "origin"]), canonical);
    let restored = std::fs::read_to_string(&config).unwrap();
    assert!(!restored.contains("old.invalid"));
    assert!(!restored.contains("credential"));
    assert!(!restored.contains("hooksPath"));
}

/// Each location kind keeps its own layout (`g5.architecture.git_locations`):
/// API storage and host caches are bare, member checkouts are worktrees, and
/// a development mirror is a separate bare destination. All of them sit inside
/// an unrelated enclosing repository that must never be discovered, including
/// for a damaged location that has lost its Git directory.
#[tokio::test]
async fn distinct_location_layouts_resolve_their_own_git_directory() {
    let temp = tempfile::tempdir().unwrap();
    let enclosing = temp.path().join("enclosing");
    std::fs::create_dir(&enclosing).unwrap();
    git(&enclosing, &["init", "--initial-branch=main"]);
    let enclosing_tip = commit(&enclosing, "enclosing");

    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "--initial-branch=main"]);
    let tip = commit(&source, "content");
    assert_ne!(tip, enclosing_tip);

    let api = NativeGit::api(&process::resolve_binary().unwrap(), Duration::from_secs(15)).unwrap();
    let host = NativeGit::host(host_environment(), Duration::from_secs(15)).unwrap();
    let cli = NativeGit::cli(Duration::from_secs(15)).unwrap();
    let main = "refs/heads/main";

    // API repository storage: a bare repository at the owner-selected path.
    let storage = enclosing.join("storage");
    git(&enclosing, &["init", "--bare", "storage"]);
    git(&source, &["push", storage.to_str().unwrap(), "main"]);
    assert_eq!(
        api.ref_tip(&storage, main).await.unwrap(),
        Some(tip.clone())
    );

    // Host cache: a bare clone whose heads mirror the source.
    let cache = enclosing.join("cache");
    host.clone_bare_cache(storage.to_str().unwrap(), &cache, &GitAuth::None)
        .await
        .unwrap();
    assert_eq!(host.ref_tip(&cache, main).await.unwrap(), Some(tip.clone()));

    // Member checkout: a worktree with `.git`, tracking under origin.
    let member = enclosing.join("member");
    cli.clone_checkout(
        storage.to_str().unwrap(),
        &member,
        Some("main"),
        &GitAuth::None,
    )
    .await
    .unwrap();
    assert_eq!(
        host.ref_tip(&member, main).await.unwrap(),
        Some(tip.clone())
    );
    assert_eq!(
        host.ref_tip(&member, "refs/remotes/origin/main")
            .await
            .unwrap(),
        Some(tip.clone())
    );

    // Development mirror: a separate bare destination fed by a local push.
    let mirror = enclosing.join("mirror.git");
    git(&enclosing, &["init", "--bare", "mirror.git"]);
    cli.push_local_mirror_blocking(&member, &mirror, "main")
        .unwrap();
    assert_eq!(api.ref_tip(&mirror, main).await.unwrap(), Some(tip));

    // A damaged location is refused, never resolved to the enclosing repo.
    let damaged = enclosing.join("damaged");
    std::fs::create_dir(&damaged).unwrap();
    for runner in [&api, &host] {
        match runner.ref_tip(&damaged, main).await {
            Err(error) => assert_eq!(error.effect, FailureEffect::Refused),
            Ok(found) => panic!("damaged location resolved {found:?}"),
        }
    }
    assert_eq!(git(&enclosing, &["rev-parse", "main"]), enclosing_tip);
}
