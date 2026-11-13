//! Production-boundary acceptance: launch the real CLI and speak the private mux protocol.
//! This suite is part of the required rust-test lane, without a live API or provider.
use std::path::Path;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use g5_internal_api::stream_mux::Channel;
use g5_internal_api::stream_mux::MAX_LIVE_STREAMS;
use g5d::dispatch_test_support::{
    error, git, now_ms, result, workspace_token as token, DispatcherOptions, RepoSpec,
    TestDispatcher,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

struct Dispatcher;
impl Dispatcher {
    async fn start(ttl: u64) -> TestDispatcher {
        Self::start_with_gate(ttl, false).await
    }
    async fn start_with_gate(ttl: u64, gate: bool) -> TestDispatcher {
        TestDispatcher::spawn(
            Path::new(env!("CARGO_BIN_EXE_g5d")),
            DispatcherOptions {
                lease_ttl_secs: ttl,
                mutation_gate: gate,
                env: inherited_loader_env(),
                ..DispatcherOptions::default()
            },
        )
        .await
    }
}

fn inherited_loader_env() -> Vec<(String, String)> {
    ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"]
        .into_iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| (key.to_string(), value))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_first_connection_renews_promptly_after_ready() {
    let fixture = Dispatcher::start(300).await;
    // connect() requires runtime.ready as the first control frame. An early
    // renewal must preserve that host handshake, then arrive without 150s delay.
    let mut rpc = fixture.connect().await;
    let renewal = timeout(Duration::from_secs(5), rpc.event("lease.renew"))
        .await
        .expect("initial renewal follows Ready promptly");
    assert_eq!(renewal["params"]["seq"], 1);
    let before = result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert_eq!(
        before["lease_renewal_seq"], 0,
        "a request does not grant custody"
    );
    rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"result":{
        "seq":1,"token":token(fixture.vm),"lease_ttl_secs":300
    }}))
    .await;
    let after = result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert_eq!(after["lease_renewal_seq"], 1);
    assert_eq!(after["lease_valid"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_retries_when_another_process_claims_its_initial_port() {
    let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let occupied_port = listener.local_addr().unwrap().port();
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            initial_port: Some(occupied_port),
            env: inherited_loader_env(),
            ..DispatcherOptions::default()
        },
    )
    .await;
    assert_ne!(fixture.port, occupied_port);
    let _ = fixture.connect().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_retries_when_another_process_claims_its_restart_port() {
    let mut fixture = Dispatcher::start(120).await;
    drop(fixture.connect().await);
    fixture.child.kill().await.unwrap();
    let occupied_port = fixture.port;
    let _listener = TcpListener::bind(("0.0.0.0", occupied_port)).await.unwrap();
    fixture.restart().await;
    assert_ne!(fixture.port, occupied_port);
    let _ = fixture.connect().await;
}

/// Exercise the real dispatcher startup and fs.write boundary followed by the
/// same unprivileged Git/file operations a provider performs. The root Docker
/// lane models both a root-owned Linux checkout and Mac uid-501 stale entries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn project_docs_are_writable_and_pushable_by_the_module_identity() {
    use g5d::dispatch_test_support::RepoSpec;
    use std::os::unix::fs::{lchown, PermissionsExt};
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("skipped: needs root to launch the production uid-1000 child");
        return;
    }
    let mut env = inherited_loader_env();
    env.extend([
        ("G5D_MODULE_UID".into(), "1000".into()),
        ("G5D_MODULE_GID".into(), "1000".into()),
    ]);
    let mut fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            repos: ["docs", "code"]
                .into_iter()
                .map(|root_path| RepoSpec {
                    root_path: root_path.into(),
                    branch: "demo".into(),
                    ..RepoSpec::default()
                })
                .collect(),
            ..DispatcherOptions::default()
        },
    )
    .await;
    // The harness's outer temp directory is intentionally 0700, unlike /work.
    std::fs::set_permissions(fixture.root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    for round in 0..2 {
        if round == 1 {
            // Simulate partial recovery: the mount root remains owned by 1000,
            // but some Git metadata has been restored with the host identity.
            for index in 0..2 {
                lchown(fixture.member_root(index).join(".git"), Some(501), Some(20)).unwrap();
                lchown(
                    fixture.member_root(index).join(".git/index"),
                    Some(501),
                    Some(20),
                )
                .unwrap();
            }
            fixture.restart().await;
        }
        let mut rpc = fixture.connect().await;
        for index in 0..2 {
            let repo = fixture.member_root(index);
            let folder = format!("docs/round-{round}");
            // Existing intermediate dirs left by a failed file publication
            // also need repair, not just dirs created by this request.
            std::fs::create_dir_all(repo.join(&folder)).unwrap();
            let path = format!("{folder}/nested/test.md");
            result(rpc.call("fs.write", fixture.params(false, json!({
                "repo_id":fixture.repos[index].0, "path":path,
                "content":"this is just for test sync", "encoding":"utf-8", "expected_hash":null
            }))).await);
            let remote = fixture.workspace_root().join(format!("remote-{index}.git"));
            let mut command = tokio::process::Command::new("sh");
            command
                .env_clear()
                .env("PATH", std::env::var("PATH").unwrap())
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", fixture.state_dir().join("gitconfig"))
                .env("GIT_AUTHOR_NAME", "Owner")
                .env("GIT_AUTHOR_EMAIL", "owner@example.test")
                .env("GIT_COMMITTER_NAME", "Owner")
                .env("GIT_COMMITTER_EMAIL", "owner@example.test")
                .uid(1000)
                .gid(1000)
                .kill_on_drop(true)
                .arg("-ec")
                .arg(
                    r#"
                    test "$(id -u)" = 1000
                    test "$(cat "$2")" = 'this is just for test sync'
                    mv "$2" "$2.moved"
                    mv "$2.moved" "$2"
                    printf sibling > "$(dirname "$2")/sibling.md"
                    git add docs
                    git commit -qm 'project doc'
                    git init --bare -q "$1"
                    git push -q "$1" HEAD:refs/heads/demo
                    git --git-dir="$1" show "refs/heads/demo:$2"
                "#,
                )
                .arg("project-doc-check")
                .arg(&remote)
                .arg(&path)
                .current_dir(&repo);
            let output = timeout(Duration::from_secs(30), command.output())
                .await
                .unwrap()
                .unwrap();
            assert!(
                output.status.success(),
                "round {round}, repo {index}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "this is just for test sync"
            );
        }
    }
}

/// A live fork leaves the source VM running. Privileged snapshot/fetch must
/// not leave object-store directories that poison later module Git writes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn live_fork_returns_git_object_writes_to_the_module_after_success_and_failure() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("skipped: needs root to launch the production uid-1000 child");
        return;
    }
    let mut env = inherited_loader_env();
    env.extend([
        ("G5D_MODULE_UID".into(), "1000".into()),
        ("G5D_MODULE_GID".into(), "1000".into()),
    ]);
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            git_origin: true,
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    std::fs::set_permissions(fixture.root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let repo = fixture.repo_root();
    let root_git = |args: &[&str]| {
        let safe = format!("safe.directory={}", repo.display());
        let mut options = vec!["-c", safe.as_str()];
        options.extend_from_slice(args);
        git(&repo, &options)
    };
    std::fs::write(repo.join("base.txt"), "base").unwrap();
    root_git(&["add", "."]);
    root_git(&["commit", "-qm", "base"]);
    let bare = fixture.origin.as_ref().unwrap().bare(fixture.repo);
    root_git(&[
        "push",
        "-q",
        bare.to_str().unwrap(),
        "HEAD:refs/heads/shared",
    ]);
    g5d_foundation::ownership::chown_tree(&repo, Some(1000), Some(1000)).unwrap();

    // Produce deterministic fanout collisions, rather than relying on a random
    // later commit happening to share one of the flush's new object prefixes.
    let blob = |label: &str, accept: &dyn Fn(&str) -> bool| {
        (0..100_000)
            .find_map(|n| {
                let bytes = format!("{label}-{n}");
                let oid = git2::Oid::hash_object(git2::ObjectType::Blob, bytes.as_bytes())
                    .unwrap()
                    .to_string();
                accept(&oid[..2]).then_some((bytes, oid))
            })
            .expect("find a blob with the requested prefix")
    };
    let mut rpc = fixture.connect().await;
    for fail in [false, true] {
        let (dirty, dirty_oid) = blob("snapshot", &|prefix| {
            !repo.join(".git/objects").join(prefix).exists()
        });
        let prefix = &dirty_oid[..2];
        result(
            rpc.call(
                "fs.write",
                fixture.params(
                    false,
                    json!({
                        "repo_id":fixture.repo, "path":"a-dirty.txt", "content":dirty,
                        "encoding":"utf-8", "expected_hash":null
                    }),
                ),
            )
            .await,
        );
        let mut prefixes = vec![prefix.to_owned()];
        if !fail {
            // Advance origin without teaching the live checkout the new tip,
            // forcing the privileged capture's conditional fetch as well.
            let seed = fixture.root.path().join("seed");
            git(
                fixture.root.path(),
                &[
                    "clone",
                    "-q",
                    "-b",
                    "shared",
                    bare.to_str().unwrap(),
                    seed.to_str().unwrap(),
                ],
            );
            let (remote, oid) = blob("remote", &|p| {
                p != prefix && !repo.join(".git/objects").join(p).exists()
            });
            std::fs::write(seed.join("remote.txt"), remote).unwrap();
            git(&seed, &["add", "."]);
            git(&seed, &["commit", "-qm", "remote advance"]);
            git(&seed, &["push", "-q", "origin", "shared"]);
            prefixes.push(oid[..2].to_owned());
        } else {
            // Git stores a-dirty's blob before this required clean filter
            // fails. Even the failed capture must hand that directory back.
            root_git(&["config", "filter.reject.clean", "false"]);
            root_git(&["config", "filter.reject.required", "true"]);
            std::fs::write(repo.join(".gitattributes"), "z-reject.txt filter=reject\n").unwrap();
            std::fs::write(repo.join("z-reject.txt"), "reject").unwrap();
        }
        let operation = format!("ownership-fork-{fail}");
        result(
            rpc.call(
                "admission.close",
                fixture.params(
                    true,
                    json!({
                        "mode":"fork", "interrupt":false, "operation_id":operation
                    }),
                ),
            )
            .await,
        );
        let mut params = fixture.params(true, json!({"reason":"fork"}));
        params["context"]["idempotency_key"] = json!(operation);
        let captured = rpc.call("workspace.flush", params.clone()).await;
        if fail {
            error(captured, "FLUSH_GIT_FAILED");
        } else {
            let receipt = result(captured);
            assert_eq!(result(rpc.call("workspace.flush", params).await), receipt);
        }
        result(
            rpc.call(
                "admission.open",
                fixture.params(true, json!({"operation_id":operation})),
            )
            .await,
        );
        assert!(
            repo.join(".git/objects")
                .join(prefix)
                .join(&dirty_oid[2..])
                .exists(),
            "capture must have written the dirty blob even on failure"
        );
        for (index, prefix) in prefixes.iter().enumerate() {
            assert!(repo.join(".git/objects").join(prefix).is_dir());
            let (bytes, _) = blob("module", &|p| p == prefix);
            result(rpc.call("fs.write", fixture.params(false, json!({
                "repo_id":fixture.repo, "path":format!("module-{fail}-{index}.txt"), "content":bytes,
                "encoding":"utf-8", "expected_hash":null
            }))).await);
        }
        if fail {
            std::fs::remove_file(repo.join(".gitattributes")).unwrap();
            std::fs::remove_file(repo.join("z-reject.txt")).unwrap();
        }
        let output = tokio::process::Command::new("sh")
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", fixture.state_dir().join("gitconfig"))
            .env("GIT_AUTHOR_NAME", "Owner")
            .env("GIT_AUTHOR_EMAIL", "owner@example.test")
            .env("GIT_COMMITTER_NAME", "Owner")
            .env("GIT_COMMITTER_EMAIL", "owner@example.test")
            .uid(1000)
            .gid(1000)
            .kill_on_drop(true)
            .current_dir(&repo)
            .args(["-ec", "git add .; git commit -qm 'module after fork'"])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "after fail={fail}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Leave a fresh untracked filename for the failed-capture round.
        std::fs::remove_file(repo.join("a-dirty.txt")).unwrap();
    }
}

// Each case launches a real dispatcher with native filesystem watchers and a
// host port scanner. Production dispatchers run in separate VMs; serializing
// this host-process suite avoids exhausting shared laptop watcher resources.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_keeps_terminal_alive_while_serving_more_rpc_streams_on_one_mux() {
    let fixture = Dispatcher::start(120).await;
    let mut control = fixture.connect().await;
    let handle = control._mux.handle();
    let (pty_writer, mut pty_reader) = handle.open(Channel::Pty).await.unwrap().into_split();

    pty_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    pty_writer
        .send(b"{\"type\":\"input\",\"data\":\"printf terminal-before-rpc\\n\"}\n")
        .await
        .unwrap();
    wait_for_pty(&mut pty_reader, b"terminal-before-rpc").await;

    let (rpc_writer, mut rpc_reader) = handle.open(Channel::Rpc).await.unwrap().into_split();
    rpc_writer
        .send(
            serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "health",
                "params": fixture.params(true, json!({})),
            }))
            .unwrap()
            .as_bytes(),
        )
        .await
        .unwrap();
    rpc_writer.send(b"\n").await.unwrap();
    let reply: Value = serde_json::from_str(
        &timeout(Duration::from_secs(10), rpc_reader.read_line())
            .await
            .expect("secondary RPC response timeout")
            .expect("secondary RPC stream closed"),
    )
    .unwrap();
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["vm_id"], fixture.vm.to_string());

    pty_writer
        .send(b"{\"type\":\"input\",\"data\":\"printf terminal-after-rpc\\n\"}\n")
        .await
        .unwrap();
    wait_for_pty(&mut pty_reader, b"terminal-after-rpc").await;
    assert_eq!(
        result(
            control
                .call("health", fixture.params(true, json!({})))
                .await
        )["vm_id"],
        fixture.vm.to_string()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn one_request_rpc_streams_close_and_share_the_control_file_watcher() {
    let fixture = Dispatcher::start(120).await;
    let mut control = fixture.connect().await;
    let handle = control._mux.handle();

    for id in 0..(MAX_LIVE_STREAMS + 8) {
        let (writer, mut reader) = handle.open(Channel::Rpc).await.unwrap().into_split();
        let mut request = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "health",
            "params": fixture.params(true, json!({})),
        }))
        .unwrap();
        request.push(b'\n');
        writer.send(&request).await.unwrap();
        writer.close().await;
        let reply: Value = serde_json::from_str(
            &timeout(Duration::from_secs(10), reader.read_line())
                .await
                .expect("one-request RPC response timeout")
                .expect("one-request RPC stream closed before its response"),
        )
        .unwrap();
        assert_eq!(reply["id"], id);
        assert!(
            timeout(Duration::from_secs(10), reader.recv())
                .await
                .expect("dispatcher did not close its reply direction")
                .is_none(),
            "one-request RPC stream remained registered after its reply"
        );
    }

    let (writer, mut reader) = handle.open(Channel::Rpc).await.unwrap().into_split();
    let mut request = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": "watch",
        "method": "fs.watch",
        "params": fixture.params(false, fixture.file("")),
    }))
    .unwrap();
    request.push(b'\n');
    writer.send(&request).await.unwrap();
    writer.close().await;
    let reply: Value = serde_json::from_str(
        &timeout(Duration::from_secs(10), reader.read_line())
            .await
            .expect("watch response timeout")
            .expect("watch stream closed before its response"),
    )
    .unwrap();
    assert_eq!(reply["result"]["watching"], true);

    std::fs::write(fixture.repo_root().join("shared-watch.txt"), b"changed").unwrap();
    let changed = timeout(Duration::from_secs(10), async {
        loop {
            let event = control.recv().await;
            if event["method"] == "file.changed" && event["params"]["path"] == "shared-watch.txt" {
                return event;
            }
        }
    })
    .await
    .expect("shared watcher event did not reach the control stream");
    assert_eq!(changed["params"]["repo_id"], fixture.repo.to_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn request_eof_closes_http_mux_streams_before_the_response_direction() {
    let fixture = Dispatcher::start(120).await;
    let mut control = fixture.connect().await;
    let handle = control._mux.handle();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                while socket.read(&mut buffer).await.unwrap_or(0) != 0 {}
            });
        }
    });
    loop {
        if control.event("port.opened").await["params"]["port"] == port {
            break;
        }
    }

    for _ in 0..(MAX_LIVE_STREAMS + 8) {
        let (writer, mut reader) = handle.open(Channel::Http).await.unwrap().into_split();
        writer.send(&port.to_be_bytes()).await.unwrap();
        writer.close().await;
        assert!(
            timeout(Duration::from_secs(10), reader.recv())
                .await
                .expect("dispatcher did not close the HTTP response direction")
                .is_none(),
            "HTTP response direction produced data after request EOF"
        );
    }
    acceptor.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_noninterrupt_fork_does_not_wait_for_an_idle_http_bridge() {
    let fixture = Dispatcher::start(120).await;
    let mut rpc = fixture.connect().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        let mut buffer = [0_u8; 1024];
        let mut socket = loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Other CI processes may probe scanned listeners without sending HTTP.
            if socket.read(&mut buffer).await.unwrap() > 0 {
                break socket;
            }
        };
        socket
            .write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
            .await
            .unwrap();
        while let Ok(read) = socket.read(&mut buffer).await {
            if read == 0 || socket.write_all(&buffer[..read]).await.is_err() {
                break;
            }
        }
    });
    loop {
        if rpc.event("port.opened").await["params"]["port"] == port {
            break;
        }
    }
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Http)
        .await
        .unwrap()
        .into_split();
    let mut request = port.to_be_bytes().to_vec();
    request.extend_from_slice(b"GET /socket HTTP/1.1\r\nHost: preview\r\n\r\n");
    writer.send(&request).await.unwrap();
    let handshake = timeout(Duration::from_secs(5), reader.recv())
        .await
        .expect("HTTP bridge handshake timeout")
        .expect("HTTP bridge closed before handshake");
    assert!(String::from_utf8_lossy(&handshake).contains("101 Switching Protocols"));

    let operation_id = "fork-with-idle-http-bridge";
    let mut close = fixture.params(
        true,
        json!({"mode":"fork","interrupt":false,"operation_id":operation_id}),
    );
    close["context"]["deadline_ms"] = json!(now_ms() + 1_500);
    let closed = timeout(Duration::from_secs(3), rpc.call("admission.close", close))
        .await
        .expect("non-interrupting fork waited on an idle HTTP bridge");
    assert_eq!(result(closed)["admission_open"], false);
    result(
        rpc.call(
            "admission.open",
            fixture.params(true, json!({"operation_id":operation_id})),
        )
        .await,
    );
    writer.send(b"http-after-fork").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), reader.recv())
            .await
            .expect("HTTP bridge response timeout")
            .expect("HTTP bridge closed after non-interrupting fork"),
        b"http-after-fork"
    );
    writer.close().await;
    timeout(Duration::from_secs(5), upstream)
        .await
        .expect("upstream remained connected after HTTP stream close")
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_stop_closes_idle_http_bridge_to_scanned_listener() {
    let fixture = Dispatcher::start(120).await;
    let mut rpc = fixture.connect().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        let mut buffer = [0_u8; 1024];
        let mut socket = loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Other CI processes may probe scanned listeners without sending HTTP.
            if socket.read(&mut buffer).await.unwrap() > 0 {
                break socket;
            }
        };
        socket
            .write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
            .await
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(8), socket.read(&mut buffer))
                .await
                .expect("stop left scanned listener bridge open")
                .unwrap(),
            0
        );
    });
    loop {
        if rpc.event("port.opened").await["params"]["port"] == port {
            break;
        }
    }
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Http)
        .await
        .unwrap()
        .into_split();
    let mut request = port.to_be_bytes().to_vec();
    request.extend_from_slice(b"GET /socket HTTP/1.1\r\nHost: preview\r\n\r\n");
    writer.send(&request).await.unwrap();
    let handshake = timeout(Duration::from_secs(5), reader.recv())
        .await
        .expect("HTTP bridge handshake timeout")
        .expect("HTTP bridge closed before handshake");
    assert!(String::from_utf8_lossy(&handshake).contains("101 Switching Protocols"));

    let mut close = fixture.params(true, json!({"operation_id":"stop-idle-http-bridge"}));
    close["context"]["deadline_ms"] = json!(now_ms() + 5_000);
    let closed = timeout(Duration::from_secs(8), rpc.call("admission.close", close))
        .await
        .expect("stop waited on an idle HTTP bridge");
    assert_eq!(result(closed)["admission_open"], false);
    assert!(timeout(Duration::from_secs(5), reader.recv())
        .await
        .expect("stop did not close HTTP stream")
        .is_none());
    timeout(Duration::from_secs(5), upstream)
        .await
        .expect("stop did not disconnect upstream")
        .unwrap();
}

async fn wait_for_pty(
    reader: &mut g5_internal_api::stream_mux::MuxReader,
    expected: &[u8],
) -> Vec<u8> {
    timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        loop {
            output.extend(reader.recv().await.unwrap_or_else(|| {
                panic!(
                    "PTY stream closed before {:?}; output: {}",
                    String::from_utf8_lossy(expected),
                    String::from_utf8_lossy(&output)
                )
            }));
            if output
                .windows(expected.len())
                .any(|window| window == expected)
            {
                return output;
            }
        }
    })
    .await
    .expect("PTY output timeout")
}

async fn opened_control(reader: &mut g5_internal_api::stream_mux::MuxReader) -> Value {
    let frame = timeout(Duration::from_secs(10), reader.recv())
        .await
        .expect("PTY open response timeout")
        .expect("PTY stream closed before open response");
    let control: Value = serde_json::from_slice(&frame).expect("PTY open control JSON");
    assert_eq!(
        control["type"], "opened",
        "unexpected PTY control: {control}"
    );
    control
}

async fn opened_terminal(reader: &mut g5_internal_api::stream_mux::MuxReader) -> (String, bool) {
    let control = opened_control(reader).await;
    (
        control["terminal_id"].as_str().unwrap().to_owned(),
        control["reattached"].as_bool().unwrap_or(false),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn terminal_runs_g5d_pr_directly_with_workspace_context_and_preserves_errors() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };
    let api = MockServer::start().await;
    let image_tools = tempfile::tempdir().unwrap();
    std::fs::set_permissions(image_tools.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let git = std::process::Command::new("/bin/sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(git.status.success(), "test requires image Git");
    let image_git = image_tools.path().join("git");
    symlink(String::from_utf8(git.stdout).unwrap().trim(), &image_git).unwrap();
    let mut env = inherited_loader_env();
    env.extend([
        ("G5D_API_URL".into(), api.uri()),
        (
            "PATH".into(),
            format!(
                "{}:{}",
                image_tools.path().display(),
                std::env::var("PATH").unwrap()
            ),
        ),
        (
            "HOME".into(),
            "/dispatcher-home-must-not-be-inherited".into(),
        ),
        // [g5.runtime.image-env]: the host names the image's surviving keys;
        // the dispatcher forwards the exportable one and keeps its own HOME
        // even though the image named it.
        ("IMAGE_TOOL_HOME".into(), "/opt/image-tool".into()),
        ("G5D_IMAGE_ENV_KEYS".into(), "IMAGE_TOOL_HOME:HOME".into()),
    ]);
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let pr = Uuid::new_v4();
    let record = json!({"id":pr,"sourceWorkspaceId":fixture.workspace,"title":"Terminal PR","status":"open","providerStatus":"local_only"});
    let auth = format!("Bearer {}", fixture.workspace_token);
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/internal/workspaces/{}/pull-requests/info",
            fixture.workspace
        )))
        .and(header("authorization", auth.as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"source":"g5","pullRequests":[record.clone()]}})),
        )
        .expect(1)
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/internal/workspaces/{}/pull-requests",
            fixture.workspace
        )))
        .and(header("authorization", auth.as_str()))
        .and(body_json(
            json!({"title":"Terminal PR","description":"Reviewed work","mode":"create","replace":false,"save":false}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"pullRequest":record,"alreadyExisted":false,
                    "publication":{"complete":true,"repositories":[]}}})),
        )
        .expect(1)
        .mount(&api)
        .await;
    let rpc = fixture.connect().await;
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    opened_terminal(&mut reader).await;
    writer
        .send(
            format!(
                "{}\n",
                json!({"type":"input","data":r#"(
set -e
test -z "${G5D_DISPATCH_TOKEN-}"
test -z "${G5D_HOST_JWT-}"
test -z "${LD_LIBRARY_PATH-}"
test -z "${DYLD_LIBRARY_PATH-}"
test "${IMAGE_TOOL_HOME-}" = /opt/image-tool
test -z "${G5D_IMAGE_ENV_KEYS-}"
command -v g5d > "$HOME/cli-path"
command -v git > "$HOME/image-git-path"
g5d pr --help > "$HOME/cli-help"
g5d pr view --json id,title > "$HOME/pr-view.json"
g5d pr create --title 'Terminal PR' --body 'Reviewed work' --json id,title > "$HOME/pr-create.json"
test -z "${LD_LIBRARY_PATH-}"
printf 'G5_%s\n' CLI_DONE
)
"#})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    wait_for_pty(&mut reader, b"G5_CLI_DONE").await;
    let home = fixture.root.path().join("home/agent");
    assert_eq!(
        std::fs::read_to_string(home.join("image-git-path"))
            .unwrap()
            .trim(),
        image_git.to_str().unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(home.join("cli-path"))
            .unwrap()
            .trim(),
        fixture
            .root
            .path()
            .join("var/lib/g5d/bin/g5d")
            .to_str()
            .unwrap()
    );
    for file in ["pr-view.json", "pr-create.json"] {
        let value: Value =
            serde_json::from_slice(&std::fs::read(home.join(file)).unwrap()).unwrap();
        assert_eq!(value, json!({"id":pr,"title":"Terminal PR"}));
    }
    api.verify().await;
    api.reset().await;
    Mock::given(method("POST"))
        .and(path(format!("/api/internal/workspaces/{}/pull-requests", fixture.workspace)))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"error":{"code":"CONFLICT","message":format!("G5 PR {pr} saved; provider publication incomplete"),"details":{},"requestId":"terminal-pr"}})))
        .expect(1).mount(&api).await;
    writer.send(format!("{}\n", json!({"type":"input","data":r#"g5d pr update --json id > "$HOME/pr-failed.out" 2> "$HOME/pr-failed.err"; test "$?" -ne 0 && printf 'G5_%s\n' CLI_REFUSED
"#})).as_bytes()).await.unwrap();
    wait_for_pty(&mut reader, b"G5_CLI_REFUSED").await;
    assert!(std::fs::read_to_string(home.join("pr-failed.err"))
        .unwrap()
        .contains(&format!(
            "G5 PR {pr} saved; provider publication incomplete"
        )));
    api.verify().await;

    // Terminal settings cannot replace the runtime-supplied workspace
    // identity; the refusal names its reason (`g5.terminal.secret-admission`).
    let (denied_writer, mut denied_reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    denied_writer.send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{\"G5D_WORKSPACE_TOKEN\":\"override\"}}\n").await.unwrap();
    let rejected = rejected_terminal(&mut denied_reader).await;
    assert_eq!(rejected["reason"], "secrets_invalid");
    assert!(
        rejected["detail"]
            .as_str()
            .unwrap()
            .contains("`G5D_WORKSPACE_TOKEN` is reserved"),
        "{rejected}"
    );
    wait_for_pty_close(&mut denied_reader).await;
}

async fn rejected_terminal(reader: &mut g5_internal_api::stream_mux::MuxReader) -> Value {
    let frame = timeout(Duration::from_secs(10), reader.recv())
        .await
        .expect("PTY rejection timeout")
        .expect("PTY stream closed before its rejection");
    let control: Value = serde_json::from_slice(&frame).expect("PTY rejection JSON");
    assert_eq!(
        control["type"], "rejected",
        "unexpected PTY control: {control}"
    );
    control
}

/// `g5.terminal.secret-admission`: the guest admits a terminal's secrets
/// under the shared policy. A set at every limit opens, with the values in
/// the shell; each violation answers a structured refusal naming the key or
/// the limit, never a silent close.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn terminal_open_admits_secrets_under_the_shared_policy() {
    use g5_contracts::types::task_env::{
        MAX_TASK_ENV_SECRETS, MAX_TASK_ENV_SERIALIZED_BYTES, MAX_TASK_ENV_VALUE_BYTES,
    };
    let fixture = Dispatcher::start(120).await;
    let rpc = fixture.connect().await;
    let handle = rpc._mux.handle();

    // Boundary-valid: the most entries, one of them at the value limit, and
    // an open line well past the old 64 KiB frame limit.
    let mut secrets = serde_json::Map::new();
    for index in 0..MAX_TASK_ENV_SECRETS - 2 {
        secrets.insert(format!("G5TEST_K{index}"), json!("v"));
    }
    // A prefix of `G5` without the underscore is not reserved.
    secrets.insert(
        "G5TEST_LARGE".into(),
        json!("x".repeat(MAX_TASK_ENV_VALUE_BYTES)),
    );
    secrets.insert("G5TEST_MARK".into(), json!("boundary-secret-value"));
    assert_eq!(secrets.len(), MAX_TASK_ENV_SECRETS);
    let open = json!({"type":"open","cols":80,"rows":24,"secrets":secrets});
    let mut line = serde_json::to_vec(&open).unwrap();
    assert!(line.len() > 64 * 1024 && line.len() <= MAX_TASK_ENV_SERIALIZED_BYTES + 4096);
    line.push(b'\n');
    let (writer, mut reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    writer.send(&line).await.unwrap();
    let (terminal_id, reattached) = opened_terminal(&mut reader).await;
    assert!(!reattached);
    let mut input = serde_json::to_vec(&json!({
        "type": "input",
        "data": "printf '%s:%s' \"$G5TEST_MARK\" \"${#G5TEST_LARGE}\"; printf '\\nSECRETS_%s\\n' SEEN\n"
    }))
    .unwrap();
    input.push(b'\n');
    writer.send(&input).await.unwrap();
    let output = wait_for_pty(&mut reader, b"SECRETS_SEEN").await;
    let shown = format!("boundary-secret-value:{MAX_TASK_ENV_VALUE_BYTES}");
    assert!(
        output.windows(shown.len()).any(|w| w == shown.as_bytes()),
        "{}",
        String::from_utf8_lossy(&output)
    );
    writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut reader).await;
    let _ = terminal_id;

    // Each violation: a structured refusal, then the stream closes.
    let over_value = "x".repeat(MAX_TASK_ENV_VALUE_BYTES + 1);
    let too_many: serde_json::Map<String, Value> = (0..=MAX_TASK_ENV_SECRETS)
        .map(|index| (format!("G5TEST_K{index}"), json!("v")))
        .collect();
    let too_long_name = "A".repeat(129);
    for (secrets, expected) in [
        (json!({"lower": "v"}), "`lower` is not an environment name"),
        (json!({"HOME": "/tmp"}), "`HOME` is reserved"),
        (
            json!({"GIT_AUTHOR_EMAIL": "x@y"}),
            "`GIT_AUTHOR_EMAIL` is reserved",
        ),
        (json!({"CODEX_HOME": "/tmp"}), "`CODEX_HOME` is reserved"),
        (
            json!({too_long_name.clone(): "v"}),
            "is not an environment name",
        ),
        (json!({"BIG": over_value}), "the limit is 65536 bytes"),
        (json!({"NUL": "a\u{0}b"}), "contains a NUL character"),
        (Value::Object(too_many), "limit is 128"),
    ] {
        let mut line =
            serde_json::to_vec(&json!({"type":"open","cols":80,"rows":24,"secrets":secrets}))
                .unwrap();
        line.push(b'\n');
        let (writer, mut reader) = handle.open(Channel::Pty).await.unwrap().into_split();
        writer.send(&line).await.unwrap();
        let rejected = rejected_terminal(&mut reader).await;
        assert_eq!(rejected["reason"], "secrets_invalid", "{rejected}");
        assert!(
            rejected["detail"].as_str().unwrap().contains(expected),
            "expected {expected:?} in {rejected}"
        );
        wait_for_pty_close(&mut reader).await;
    }
    // A valid set still opens after the refusals: nothing leaked a slot.
    let (writer, mut reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{\"G5TEST_OK\":\"1\"}}\n")
        .await
        .unwrap();
    opened_terminal(&mut reader).await;
    writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut reader).await;
}

async fn wait_for_pty_close(reader: &mut g5_internal_api::stream_mux::MuxReader) {
    timeout(Duration::from_secs(10), async {
        while reader.recv().await.is_some() {}
    })
    .await
    .expect("PTY stream did not close");
}

async fn terminal_processes(
    writer: &g5_internal_api::stream_mux::MuxWriter,
    reader: &mut g5_internal_api::stream_mux::MuxReader,
) -> (libc::pid_t, libc::pid_t) {
    // Disable job control so the background sleep stays in the shell's
    // process group. Separate job-control groups are covered on Linux by
    // terminal_pty's session-wide shutdown test.
    // Split the marker in the input so the PTY's command echo cannot satisfy
    // the assertion before the shell actually prints its process IDs.
    writer
        .send(
            serde_json::to_string(&json!({
                "type": "input",
                "data": "set +H\nset +m\nsleep 600 & printf 'PTY_%s:%s:%s\\n' PIDS \"$$\" \"$!\"\n"
            }))
            .unwrap()
            .as_bytes(),
        )
        .await
        .unwrap();
    writer.send(b"\n").await.unwrap();
    timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        loop {
            output.extend(reader.recv().await.expect("PTY closed before PID report"));
            let text = String::from_utf8_lossy(&output);
            if let Some(report) = text.split("PTY_PIDS:").nth(1) {
                if let Some((line, _)) = report.split_once('\n') {
                    let mut fields = line.split(|character: char| !character.is_ascii_digit());
                    if let (Some(shell), Some(child)) = (fields.next(), fields.next()) {
                        if let (Ok(shell), Ok(child)) = (shell.parse(), child.parse()) {
                            return (shell, child);
                        }
                    }
                }
            }
        }
    })
    .await
    .expect("PTY did not report shell and child PIDs")
}

fn process_is_running(pid: libc::pid_t) -> bool {
    #[cfg(target_os = "linux")]
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        return !stat
            .rfind(')')
            .and_then(|end| stat[end + 1..].split_whitespace().next())
            .is_some_and(|state| state == "Z" || state == "X");
    }
    // SAFETY: signal 0 only inspects the process created by this test.
    unsafe { libc::kill(pid, 0) == 0 }
}

async fn assert_processes_gone(pids: (libc::pid_t, libc::pid_t)) {
    timeout(Duration::from_secs(5), async {
        while process_is_running(pids.0) || process_is_running(pids.1) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("PTY shell {} or child {} survived teardown", pids.0, pids.1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_noninterrupt_fork_does_not_wait_for_an_idle_terminal() {
    let fixture = Dispatcher::start(120).await;
    let mut rpc = fixture.connect().await;
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let (terminal_id, reattached) = opened_terminal(&mut reader).await;
    assert!(!reattached);
    assert!(!terminal_id.is_empty());

    let operation_id = "fork-with-idle-terminal";
    let mut close = fixture.params(
        true,
        json!({"mode":"fork","interrupt":false,"operation_id":operation_id}),
    );
    close["context"]["deadline_ms"] = json!(now_ms() + 1_500);
    let closed = timeout(Duration::from_secs(3), rpc.call("admission.close", close))
        .await
        .expect("non-interrupting fork waited on an idle terminal");
    assert_eq!(result(closed)["admission_open"], false);
    let (refused_writer, mut refused_reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    refused_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    assert!(timeout(Duration::from_secs(3), refused_reader.recv())
        .await
        .expect("fork barrier did not reject a new PTY")
        .is_none());
    result(
        rpc.call(
            "admission.open",
            fixture.params(true, json!({"operation_id":operation_id})),
        )
        .await,
    );
    writer
        .send(b"{\"type\":\"input\",\"data\":\"printf FORK_TERMINAL_ALIVE\\n\"}\n")
        .await
        .unwrap();
    wait_for_pty(&mut reader, b"FORK_TERMINAL_ALIVE").await;
    writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut reader).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_stop_closes_terminal_and_reaps_its_processes() {
    let mut env = inherited_loader_env();
    env.push(("RUST_LOG".into(), "info".into()));
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = fixture.connect().await;
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let (terminal_id, _) = opened_terminal(&mut reader).await;
    let pids = terminal_processes(&writer, &mut reader).await;

    let mut close = fixture.params(true, json!({"operation_id":"stop-with-terminal"}));
    close["context"]["deadline_ms"] = json!(now_ms() + 10_000);
    let closed = timeout(Duration::from_secs(12), rpc.call("admission.close", close))
        .await
        .expect("stop waited indefinitely on terminal transport");
    assert_eq!(result(closed)["admission_open"], false);
    wait_for_pty_close(&mut reader).await;
    assert_processes_gone(pids).await;
    assert!(fixture.stderr().lines().any(|line| {
        line.contains("closing terminal during manager shutdown")
            && line.contains(&terminal_id)
            && line.contains("workspace_stop")
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_interrupting_fork_closes_terminal_with_its_reason() {
    let mut env = inherited_loader_env();
    env.push(("RUST_LOG".into(), "info".into()));
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = fixture.connect().await;
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let (terminal_id, _) = opened_terminal(&mut reader).await;
    let pids = terminal_processes(&writer, &mut reader).await;

    let mut close = fixture.params(
        true,
        json!({"mode":"fork","interrupt":true,"operation_id":"fork-interrupt-terminal"}),
    );
    close["context"]["deadline_ms"] = json!(now_ms() + 10_000);
    let closed = timeout(Duration::from_secs(12), rpc.call("admission.close", close))
        .await
        .expect("fork interrupt waited indefinitely on terminal transport");
    assert_eq!(result(closed)["admission_open"], false);
    wait_for_pty_close(&mut reader).await;
    assert_processes_gone(pids).await;
    assert!(fixture.stderr().lines().any(|line| {
        line.contains("closing terminal during manager shutdown")
            && line.contains(&terminal_id)
            && line.contains("fork_interrupt")
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_stop_reaps_terminal_opened_after_shutdown_snapshot() {
    let mut env = inherited_loader_env();
    env.extend([
        ("G5D_DISPATCH_TEST_WAIT_FOR_SHUTDOWN".into(), "1".into()),
        (
            "G5D_DISPATCH_TEST_STALL_AFTER_ATTACH_MS".into(),
            "100".into(),
        ),
    ]);
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let mut rpc = fixture.connect().await;
    let (writer, mut reader) = rpc
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{},\"test_hook\":\"late_attachment\"}\n")
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while !fixture
            .stderr()
            .contains("test waiting for terminal shutdown snapshot")
        {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("terminal did not enter the post-admission wait");

    let mut close = fixture.params(true, json!({"operation_id":"stop-racing-open"}));
    close["context"]["deadline_ms"] = json!(now_ms() + 10_000);
    let closed = timeout(Duration::from_secs(12), rpc.call("admission.close", close))
        .await
        .expect("stop did not drain the racing terminal");
    assert_eq!(result(closed)["admission_open"], false);
    wait_for_pty_close(&mut reader).await;

    let stderr = fixture.stderr();
    let shutdown_done = stderr
        .find("test terminal shutdown snapshot complete")
        .expect("stop did not finish terminal shutdown");
    let late_attached = stderr
        .find("test late PTY attached ")
        .expect("terminal was not created after shutdown");
    assert!(
        shutdown_done < late_attached,
        "terminal opened before shutdown"
    );
    let late = stderr
        .split("test late PTY attached ")
        .nth(1)
        .expect("terminal was not created after shutdown")
        .lines()
        .next()
        .unwrap();
    let pid: libc::pid_t = late
        .split("pid=")
        .nth(1)
        .expect("missing late terminal PID")
        .parse()
        .unwrap();
    let reaped = timeout(Duration::from_secs(2), async {
        while process_is_running(pid) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .is_ok();
    if !reaped {
        // SAFETY: the test owns this shell's process group and stops it when
        // proving the pre-fix leak, so a failing regression leaves no child.
        unsafe { libc::killpg(pid, libc::SIGKILL) };
    }
    assert!(reaped, "late PTY shell {pid} survived workspace stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_pty_reconnects_with_identity_and_retires_on_transport_replacement() {
    let fixture = Dispatcher::start(120).await;
    let mut first_connection = fixture.connect().await;
    let first_handle = first_connection._mux.handle();
    let (first_writer, mut first_reader) =
        first_handle.open(Channel::Pty).await.unwrap().into_split();
    first_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let (terminal_id, reattached) = opened_terminal(&mut first_reader).await;
    assert!(!reattached);
    // A browser resize during a collapsed layout may be zero-sized; it must
    // not orphan the attachment or strand the PTY without a cleanup owner.
    first_writer
        .send(b"{\"type\":\"resize\",\"cols\":0,\"rows\":0}\n")
        .await
        .unwrap();
    first_writer
        .send(b"{\"type\":\"input\",\"data\":\"printf PTY_FIRST\\n\"}\n")
        .await
        .unwrap();
    wait_for_pty(&mut first_reader, b"PTY_FIRST").await;

    // An abnormal browser close detaches but retains the PTY for a bounded
    // grace period. A new stream can claim the same identity and fences the
    // old generation before it can write again.
    let (second_writer, mut second_reader) =
        first_handle.open(Channel::Pty).await.unwrap().into_split();
    let mut reattach = serde_json::to_vec(&json!({
        "type":"open","cols":100,"rows":30,"terminal_id":terminal_id,"secrets":{}
    }))
    .unwrap();
    reattach.push(b'\n');
    second_writer.send(&reattach).await.unwrap();
    let (reattached_id, reattached) = opened_terminal(&mut second_reader).await;
    assert_eq!(reattached_id, terminal_id);
    assert!(reattached);
    // The old owner is now fenced. Its delayed packet may be rejected at the
    // mux boundary, but it must never reach the replacement shell.
    let mut stale_input = serde_json::to_vec(&json!({
        "type": "input", "data": "printf 'PTY_%s\\n' STALE\n"
    }))
    .unwrap();
    stale_input.push(b'\n');
    let _ = first_writer.send(&stale_input).await;
    wait_for_pty_close(&mut first_reader).await;
    let mut current_input = serde_json::to_vec(&json!({
        "type": "input", "data": "printf 'PTY_%s\\n' REATTACHED\n"
    }))
    .unwrap();
    current_input.push(b'\n');
    second_writer.send(&current_input).await.unwrap();
    let reattached_output = wait_for_pty(&mut second_reader, b"PTY_REATTACHED").await;
    assert!(
        !String::from_utf8_lossy(&reattached_output).contains("PTY_STALE"),
        "stale input reached the replacement shell"
    );

    let operation_id = "r2-pty-transfer";
    let prepared = result(
        first_connection
            .call(
                "admission.close",
                fixture.params(
                    true,
                    json!({"prepare_only":true,"operation_id":operation_id}),
                ),
            )
            .await,
    );
    assert_eq!(prepared["admission_open"], false);
    let (refused_writer, mut refused_reader) =
        first_handle.open(Channel::Pty).await.unwrap().into_split();
    refused_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    assert!(timeout(Duration::from_secs(5), refused_reader.recv())
        .await
        .expect("prepared admission did not reject a new PTY")
        .is_none());
    second_writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut second_reader).await;
    let closed = result(
        first_connection
            .call(
                "admission.close",
                fixture.params(true, json!({"operation_id":operation_id})),
            )
            .await,
    );
    assert_eq!(closed["admission_open"], false);
    result(
        first_connection
            .call(
                "admission.open",
                fixture.params(true, json!({"operation_id":operation_id})),
            )
            .await,
    );

    // Replacing the authenticated transport cancels its active stream tasks;
    // the PTY process must not survive that boundary.
    let retired_connection = fixture.connect().await;
    let retired_handle = retired_connection._mux.handle();
    let (retired_writer, mut retired_reader) = retired_handle
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    retired_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let _ = opened_terminal(&mut retired_reader).await;
    let retired_pids = terminal_processes(&retired_writer, &mut retired_reader).await;
    let mut replacement = fixture.connect().await;
    let _ = replacement
        .call("health", fixture.params(true, json!({})))
        .await;
    wait_for_pty_close(&mut retired_reader).await;
    assert_processes_gone(retired_pids).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_pty_reattach_replays_only_unseen_streaming_output() {
    let fixture = Dispatcher::start(120).await;
    let rpc = fixture.connect().await;
    let handle = rpc._mux.handle();
    let (first_writer, mut first_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    first_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let opened = opened_control(&mut first_reader).await;
    let terminal_id = opened["terminal_id"].as_str().unwrap().to_owned();
    assert_eq!(opened["offset"], 0);
    first_writer
        .send(
            serde_json::to_string(&json!({
                "type": "input",
                "data": "stty -echo; i=1; while [ \"$i\" -le 80 ]; do printf 'R2SEQ_%03d\\n' \"$i\"; i=$((i+1)); sleep 0.01; done\n"
            }))
            .unwrap()
            .as_bytes(),
        )
        .await
        .unwrap();
    first_writer.send(b"\n").await.unwrap();
    let first_output = wait_for_pty(&mut first_reader, b"R2SEQ_010").await;
    let consumed = first_output.len();
    let (second_writer, mut second_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    let mut request = serde_json::to_vec(&json!({
        "type": "open", "cols": 80, "rows": 24,
        "terminal_id": terminal_id, "offset": consumed, "secrets": {}
    }))
    .unwrap();
    request.push(b'\n');
    second_writer.send(&request).await.unwrap();
    let resumed = opened_control(&mut second_reader).await;
    assert_eq!(resumed["terminal_id"], terminal_id);
    assert_eq!(resumed["reattached"], true);
    assert_eq!(resumed["offset"], consumed);
    assert_eq!(resumed["reset"], false);
    let mut output = first_output;
    output.extend(wait_for_pty(&mut second_reader, b"R2SEQ_080").await);
    let text = String::from_utf8_lossy(&output);
    for sequence in 1..=80 {
        let marker = format!("R2SEQ_{sequence:03}");
        assert_eq!(
            text.matches(&marker).count(),
            1,
            "missing or duplicate {marker}"
        );
    }
    second_writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut second_reader).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_pty_replay_reports_when_requested_history_was_evicted() {
    let fixture = Dispatcher::start(120).await;
    let rpc = fixture.connect().await;
    let handle = rpc._mux.handle();
    let (first_writer, mut first_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    first_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let terminal_id = opened_control(&mut first_reader).await["terminal_id"]
        .as_str()
        .unwrap()
        .to_owned();
    first_writer
        .send(
            serde_json::to_string(&json!({
                "type": "input",
                "data": "stty -echo; printf '%05000d\\nR2GAP_%s\\n' 0 END\n"
            }))
            .unwrap()
            .as_bytes(),
        )
        .await
        .unwrap();
    first_writer.send(b"\n").await.unwrap();
    let output = wait_for_pty(&mut first_reader, b"R2GAP_END").await;
    assert!(output.len() > 4096, "test did not exceed replay capacity");

    let (second_writer, mut second_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    let mut request = serde_json::to_vec(&json!({
        "type":"open", "cols":80, "rows":24, "terminal_id":terminal_id,
        "offset":0, "secrets":{}
    }))
    .unwrap();
    request.push(b'\n');
    second_writer.send(&request).await.unwrap();
    let control = opened_control(&mut second_reader).await;
    assert_eq!(control["reset"], true);
    assert!(control["offset"].as_u64().unwrap() > 0);
    let replay = timeout(Duration::from_secs(5), second_reader.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(replay.len() <= 4096);
    assert!(replay
        .windows(b"R2GAP_END".len())
        .any(|chunk| chunk == b"R2GAP_END"));
    second_writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut second_reader).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_detached_terminal_expires_and_rejects_reattach() {
    let mut env = inherited_loader_env();
    env.push(("G5D_PTY_REATTACH_GRACE_MS".into(), "100".into()));
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let rpc = fixture.connect().await;
    let handle = rpc._mux.handle();
    let (writer, mut reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    let (terminal_id, _) = opened_terminal(&mut reader).await;
    let pids = terminal_processes(&writer, &mut reader).await;
    writer.close().await;
    wait_for_pty_close(&mut reader).await;
    assert_processes_gone(pids).await;

    let (reattach_writer, mut reattach_reader) =
        handle.open(Channel::Pty).await.unwrap().into_split();
    let mut request = serde_json::to_vec(&json!({
        "type": "open", "cols": 80, "rows": 24, "terminal_id": terminal_id,
        "secrets": {}
    }))
    .unwrap();
    request.push(b'\n');
    reattach_writer.send(&request).await.unwrap();
    let response = timeout(Duration::from_secs(5), reattach_reader.recv())
        .await
        .unwrap()
        .expect("dispatcher closed without a rejection");
    assert_eq!(
        serde_json::from_slice::<Value>(&response).unwrap()["type"],
        "rejected"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_drain_timeout_closes_only_the_replaced_connections_terminal() {
    let mut env = inherited_loader_env();
    env.extend([
        ("G5D_DISPATCH_TEST_STREAM_DRAIN_MS".into(), "200".into()),
        ("G5D_DISPATCH_TEST_STALL_PTY_TEARDOWN".into(), "1".into()),
    ]);
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let old = fixture.connect().await;
    let (old_writer, mut old_reader) = old
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    old_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    opened_terminal(&mut old_reader).await;
    let old_pids = terminal_processes(&old_writer, &mut old_reader).await;

    let replacement = fixture.connect().await;
    let (new_writer, mut new_reader) = replacement
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    new_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    opened_terminal(&mut new_reader).await;
    let new_pids = terminal_processes(&new_writer, &mut new_reader).await;
    timeout(Duration::from_secs(5), async {
        while !fixture
            .stderr()
            .contains("dispatcher stream teardown exceeded its bound")
        {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("test did not force the drain-timeout fallback");
    assert_processes_gone(old_pids).await;
    assert!(
        process_is_running(new_pids.0) && process_is_running(new_pids.1),
        "drain fallback killed a terminal on the replacement connection"
    );
    new_writer
        .send(b"{\"type\":\"input\",\"data\":\"printf NEW_TERMINAL_ALIVE\\n\"}\n")
        .await
        .unwrap();
    wait_for_pty(&mut new_reader, b"NEW_TERMINAL_ALIVE").await;
    new_writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut new_reader).await;
    assert_processes_gone(new_pids).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_drain_timeout_closes_attachment_registered_before_final_snapshot() {
    let mut env = inherited_loader_env();
    env.extend([
        ("G5D_DISPATCH_TEST_STREAM_DRAIN_MS".into(), "100".into()),
        ("G5D_DISPATCH_TEST_STALL_PTY_TEARDOWN".into(), "1".into()),
        ("G5D_DISPATCH_TEST_DELAY_ATTACH_MS".into(), "500".into()),
        (
            "G5D_DISPATCH_TEST_STALL_AFTER_ATTACH_MS".into(),
            "2000".into(),
        ),
        (
            "G5D_DISPATCH_TEST_PAUSE_BEFORE_DRAIN_ABORT_MS".into(),
            "800".into(),
        ),
    ]);
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let old = fixture.connect().await;
    let handle = old._mux.handle();
    let (first_writer, mut first_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    first_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{}}\n")
        .await
        .unwrap();
    opened_terminal(&mut first_reader).await;
    let (late_writer, mut late_reader) = handle.open(Channel::Pty).await.unwrap().into_split();
    late_writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{},\"test_hook\":\"late_attachment\"}\n")
        .await
        .unwrap();
    timeout(Duration::from_secs(5), async {
        while !fixture.stderr().contains("test delayed PTY attachment") {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("late attachment did not enter its delay");
    let replacement = fixture.connect().await;
    timeout(Duration::from_secs(5), async {
        while !fixture.stderr().contains("test late PTY attached ") {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("late attachment did not register during timeout cleanup");
    let stderr = fixture.stderr();
    assert!(stderr.contains("dispatcher stream teardown exceeded its bound"));
    let late_id = stderr
        .split("test late PTY attached ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    wait_for_pty_close(&mut late_reader).await;
    let (writer, mut reader) = replacement
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    let mut request = serde_json::to_vec(&json!({
        "type":"open", "cols":80, "rows":24, "terminal_id":late_id, "secrets":{}
    }))
    .unwrap();
    request.push(b'\n');
    writer.send(&request).await.unwrap();
    let response = timeout(Duration::from_secs(5), reader.recv())
        .await
        .unwrap()
        .expect("reattach closed without a rejection");
    assert_eq!(
        serde_json::from_slice::<Value>(&response).unwrap()["type"],
        "rejected",
        "late attachment survived the drain fallback"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_retries_failed_pty_close_during_connection_teardown() {
    let mut env = inherited_loader_env();
    env.push(("G5D_DISPATCH_TEST_FAIL_CLOSE_ONCE".into(), "1".into()));
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let first = fixture.connect().await;
    let (writer, mut reader) = first
        ._mux
        .handle()
        .open(Channel::Pty)
        .await
        .unwrap()
        .into_split();
    writer
        .send(b"{\"type\":\"open\",\"cols\":80,\"rows\":24,\"secrets\":{},\"test_hook\":\"fail_close_once\"}\n")
        .await
        .unwrap();
    opened_terminal(&mut reader).await;
    let pids = terminal_processes(&writer, &mut reader).await;
    writer.send(b"{\"type\":\"close\"}\n").await.unwrap();
    wait_for_pty_close(&mut reader).await;
    assert!(process_is_running(pids.0), "injected close did not fail");

    // The stream has exited, but its attachment entry must survive so the
    // connection replacement can retry the retiring session's close.
    let _replacement = fixture.connect().await;
    assert_processes_gone(pids).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_expired_lease_refuses_operations_and_notifications() {
    let fixture = Dispatcher::start(2).await;
    let mut rpc = fixture.connect().await;
    result(
        rpc.call("fs.watch", fixture.params(false, fixture.file("")))
            .await,
    );
    let renewal = rpc.event("lease.renew").await;
    timeout(Duration::from_secs(4), async {
        loop {
            let health = result(rpc.call("health", fixture.params(true, json!({}))).await);
            if health["lease_valid"] == false && health["admission_open"] == false {
                break;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("expired lease must close admission");
    rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"result":{
        "seq":renewal["params"]["seq"],"token":token(fixture.vm),"lease_ttl_secs":120
    }}))
    .await;
    let health = result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert_eq!(health["lease_valid"], false);
    assert_eq!(health["lease_renewal_seq"], 0);
    for method in ["fs.list", "ports.list", "admission.open"] {
        error(
            rpc.call(
                method,
                fixture.params(method == "admission.open", fixture.file("")),
            )
            .await,
            "LEASE_EXPIRED",
        );
    }
    let mut write = fixture.file("late-rpc.txt");
    write["expected_hash"] = Value::Null;
    write["content"] = json!("must not be written");
    error(
        rpc.call("fs.write", fixture.params(false, write)).await,
        "LEASE_EXPIRED",
    );
    assert!(!fixture.repo_root().join("late-rpc.txt").exists());

    std::fs::write(fixture.repo_root().join("late-native.txt"), b"native edit").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // Only notifications produced after expiry matter; unrelated host ports and
    // queued renewal requests may already be on the connection.
    assert!(timeout(Duration::from_millis(1500), async {
        loop {
            let event = rpc.recv().await;
            assert_ne!(
                event["method"], "file.changed",
                "expired file event: {event}"
            );
            assert!(
                event["method"] != "port.opened" || event["params"]["port"] != port,
                "expired port event: {event}"
            );
        }
    })
    .await
    .is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_auth_fencing_and_native_git_configuration() {
    let fixture = Dispatcher::start(120).await;
    // Consume the verified startup peer so the timed connect below must
    // authenticate a new connection while the unauthenticated peer stalls.
    let _startup_peer = fixture.connect().await;
    let slow_peer = TcpStream::connect(("127.0.0.1", fixture.port))
        .await
        .unwrap();
    sleep(Duration::from_millis(50)).await;
    let mut prompt_peer = timeout(Duration::from_secs(2), fixture.connect())
        .await
        .expect("a stalled unauthenticated peer must not block host authentication");
    result(
        prompt_peer
            .call("health", fixture.params(true, json!({})))
            .await,
    );
    drop(slow_peer);
    let mut rejected = TcpStream::connect(("127.0.0.1", fixture.port))
        .await
        .unwrap();
    rejected.write_all(&[0; 32]).await.unwrap();
    let mut byte = [0];
    assert_eq!(
        timeout(Duration::from_secs(3), rejected.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    let mut rpc = fixture.connect().await;
    let health = result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert_eq!(health["vm_id"], fixture.vm.to_string());
    assert_eq!(health["epoch"], 7);
    assert_eq!(health["max_file_bytes"], MAX_FILE_BYTES);
    result(
        rpc.call("runtime.inspect.summary", fixture.params(false, json!({})))
            .await,
    );
    let mut inspection = fixture.params(false, fixture.file(""));
    inspection["context"]
        .as_object_mut()
        .unwrap()
        .remove("actor_user_id");
    inspection["context"]["inspection_grant_id"] = json!(Uuid::new_v4());
    let summary = result(
        rpc.call("runtime.inspect.summary", inspection.clone())
            .await,
    );
    assert!(!summary.to_string().contains(&token(fixture.vm)));
    error(
        rpc.call("fs.list", inspection.clone()).await,
        "OWNER_REQUIRED",
    );
    error(
        rpc.call("admission.close", inspection).await,
        "LIFECYCLE_AUTHORITY_REQUIRED",
    );
    let mut stale = fixture.params(false, fixture.file(""));
    stale["context"]["expected_vm_id"] = json!(Uuid::new_v4());
    error(rpc.call("fs.list", stale).await, "STALE_INCARNATION");
    let mut reader = fixture.params(false, fixture.file(""));
    reader["context"]["actor_user_id"] = json!(Uuid::new_v4());
    error(rpc.call("fs.list", reader).await, "OWNER_REQUIRED");
    error(
        rpc.call("health", fixture.params(false, json!({}))).await,
        "LIFECYCLE_AUTHORITY_REQUIRED",
    );
    error(
        rpc.call("proc.exec", fixture.params(false, json!({})))
            .await,
        "method_not_found",
    );
    assert_eq!(
        git(
            &fixture.repo_root(),
            &[
                "config",
                "--file",
                fixture
                    .root
                    .path()
                    .join("var/lib/g5d/gitconfig")
                    .to_str()
                    .unwrap(),
                "user.name"
            ]
        ),
        "Fixture Owner"
    );
    assert_eq!(
        git(
            &fixture.repo_root(),
            &[
                "config",
                "--file",
                fixture
                    .root
                    .path()
                    .join("var/lib/g5d/gitconfig")
                    .to_str()
                    .unwrap(),
                "user.email"
            ]
        ),
        "fixture@example.test"
    );
    let mut reconnect = fixture.connect().await;
    rpc.assert_retired(json!({"jsonrpc":"2.0","id":"retired-probe","method":"fs.write","params":fixture.params(false,json!({"repo_id":fixture.repo,"path":"retired-peer","content":"forbidden","encoding":"utf-8","expected_hash":null}))})).await;
    assert!(!fixture.repo_root().join("retired-peer").exists());
    result(
        reconnect
            .call("health", fixture.params(true, json!({})))
            .await,
    );
}

/// G5Labs/G5-Lean#643: the host abandons a probe that sent no `runtime.ready`
/// in time and dials again with the same token. An abandoned socket that
/// finishes its handshake after the newer connection is live is closed; it
/// never retires the live connection and its in-flight replies.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn late_handshake_of_an_earlier_connection_cannot_replace_the_live_one() {
    let accepts = tempfile::tempdir().unwrap();
    let accept_log = accepts.path().join("accepts");
    let mut env = inherited_loader_env();
    env.push((
        "G5D_DISPATCH_TEST_ACCEPT_LOG".into(),
        accept_log.display().to_string(),
    ));
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            lease_ttl_secs: 120,
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let _startup_peer = fixture.connect().await;
    let accepted = || {
        std::fs::read_to_string(&accept_log)
            .map(|log| log.lines().count())
            .unwrap_or(0)
    };
    let before = accepted();
    let mut abandoned = TcpStream::connect(("127.0.0.1", fixture.port))
        .await
        .unwrap();
    // The dispatcher must accept the abandoned socket before the live one.
    timeout(Duration::from_secs(10), async {
        while accepted() <= before {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the dispatcher accepts the abandoned socket");
    let mut live = fixture.connect().await;
    result(live.call("health", fixture.params(true, json!({}))).await);
    abandoned.write_all(&fixture.listener_token).await.unwrap();
    let mut byte = [0];
    assert_eq!(
        timeout(Duration::from_secs(5), abandoned.read(&mut byte))
            .await
            .expect("the dispatcher must close a connection accepted before the live one")
            .unwrap(),
        0
    );
    // The late handshake has been handled; the live connection still answers.
    result(live.call("health", fixture.params(true, json!({}))).await);
    // A connection accepted after the live one still replaces it.
    let mut reconnect = fixture.connect().await;
    live.assert_retired(json!({"jsonrpc":"2.0","id":"retired-probe","method":"health","params":fixture.params(true,json!({}))}))
        .await;
    result(
        reconnect
            .call("health", fixture.params(true, json!({})))
            .await,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn shutdown_flush_preserves_branch_head_and_dirty_snapshot_with_idempotent_retry() {
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            git_origin: true,
            env: inherited_loader_env(),
            ..DispatcherOptions::default()
        },
    )
    .await;
    let repo = fixture.repo_root();
    std::fs::write(repo.join("base.txt"), b"base\n").unwrap();
    git(&repo, &["add", "base.txt"]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    let bare = fixture.origin.as_ref().unwrap().bare(fixture.repo);
    git(
        &repo,
        &["remote", "add", "origin-fixture", bare.to_str().unwrap()],
    );
    git(
        &repo,
        &[
            "push",
            "-q",
            "origin-fixture",
            &format!("{base}:refs/heads/shared"),
        ],
    );

    std::fs::write(repo.join("branch-only.txt"), b"branch\n").unwrap();
    git(&repo, &["add", "branch-only.txt"]);
    git(&repo, &["commit", "-q", "-m", "local branch"]);
    let local_branch = git(&repo, &["rev-parse", "refs/heads/shared"]);
    git(&repo, &["checkout", "-q", "-b", "foreign", &base]);
    std::fs::write(repo.join("foreign.txt"), b"foreign\n").unwrap();
    git(&repo, &["add", "foreign.txt"]);
    git(&repo, &["commit", "-q", "-m", "foreign head"]);
    let local_head = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("dirty.txt"), b"durable dirty bytes\n").unwrap();

    let mut rpc = fixture.connect().await;
    result(
        rpc.call("admission.close", fixture.params(true, json!({})))
            .await,
    );
    let mut params = fixture.params(true, json!({"reason":"shutdown"}));
    params["context"]["idempotency_key"] = json!("shutdown-flush-retry");
    fixture
        .origin
        .as_ref()
        .unwrap()
        .set_pending_session_files_unavailable(true);
    let receipt = result(rpc.call("workspace.flush", params.clone()).await);
    assert_eq!(receipt["repos"][0]["branch_oid"], base);
    assert_eq!(receipt["repos"][0]["local_branch_oid"], local_branch);
    assert_eq!(receipt["repos"][0]["local_head_oid"], local_head);
    assert_eq!(receipt["repos"][0]["head_ref"], "refs/heads/foreign");
    assert_eq!(receipt["repos"][0]["dirty"], true);
    let flush = receipt["flush_id"].as_str().unwrap();
    let snapshot_ref = format!("refs/g5/flush/{}/{flush}/snapshot", fixture.workspace);
    let branch_ref = format!("refs/g5/flush/{}/{flush}/branch", fixture.workspace);
    assert_eq!(git(&bare, &["rev-parse", &branch_ref]), local_branch);
    assert_eq!(
        git(&bare, &["show", &format!("{snapshot_ref}:dirty.txt")]),
        "durable dirty bytes"
    );

    let replay = result(rpc.call("workspace.flush", params).await);
    assert_eq!(replay, receipt);
    assert_eq!(fixture.origin.as_ref().unwrap().flush_count(), 1);

    let state_path = fixture.state_dir().join("workspace-flush.json");
    let mut abandoned: Value =
        serde_json::from_slice(&std::fs::read(&state_path).expect("read completed flush state"))
            .expect("parse completed flush state");
    abandoned["request_key"] = json!("abandoned-transient-capture");
    abandoned["complete"] = Value::Null;
    std::fs::write(
        &state_path,
        serde_json::to_vec(&abandoned).expect("serialize abandoned flush state"),
    )
    .expect("seed abandoned flush state");
    let mut replacement_params = fixture.params(true, json!({"reason":"shutdown"}));
    replacement_params["context"]["idempotency_key"] = json!("replacement-stop");
    let replacement = result(rpc.call("workspace.flush", replacement_params).await);
    assert_ne!(replacement["flush_id"], receipt["flush_id"]);
    assert_eq!(fixture.origin.as_ref().unwrap().flush_count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_file_preconditions_binary_limit_and_path_confinement() {
    let fixture = Dispatcher::start(120).await;
    let mut rpc = fixture.connect().await;
    let write = |path: &str, content: Value, encoding: &str, expected: Value| {
        fixture.params(false,json!({"repo_id":fixture.repo,"path":path,"content":content,"encoding":encoding,"expected_hash":expected}))
    };
    let created = result(
        rpc.call(
            "fs.write",
            write("a.txt", json!("alpha"), "utf-8", Value::Null),
        )
        .await,
    );
    let hash = created["hash"].clone();
    assert_eq!(hash, git(&fixture.repo_root(), &["hash-object", "a.txt"]));
    let conflict = error(
        rpc.call(
            "fs.write",
            write("a.txt", json!("lost"), "utf-8", json!("stale")),
        )
        .await,
        "FILE_VERSION_CONFLICT",
    );
    assert_eq!(conflict["code"], 409);
    assert_eq!(conflict["data"]["hash"], hash);
    error(
        rpc.call(
            "fs.write",
            write("a.txt", json!("lost"), "utf-8", Value::Null),
        )
        .await,
        "FILE_EXISTS",
    );
    error(
        rpc.call(
            "fs.write",
            write("missing", json!("lost"), "utf-8", hash.clone()),
        )
        .await,
        "FILE_MISSING",
    );
    let mut omitted = write("omitted", json!("lost"), "utf-8", Value::Null);
    omitted.as_object_mut().unwrap().remove("expected_hash");
    assert!(rpc.call("fs.write", omitted).await.get("error").is_some());
    assert_eq!(
        std::fs::read(fixture.repo_root().join("a.txt")).unwrap(),
        b"alpha"
    );
    let updated = result(
        rpc.call("fs.write", write("a.txt", json!("beta"), "utf-8", hash))
            .await,
    );
    let read = result(
        rpc.call("fs.read", fixture.params(false, fixture.file("a.txt")))
            .await,
    );
    assert_eq!(read["content"], "beta");
    assert_eq!(read["hash"], updated["hash"]);
    result(
        rpc.call("fs.stat", fixture.params(false, fixture.file("a.txt")))
            .await,
    );
    let listing = result(
        rpc.call("fs.list", fixture.params(false, fixture.file("")))
            .await,
    );
    assert!(listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "a.txt"));
    result(rpc.call("fs.rename",fixture.params(false,json!({"repo_id":fixture.repo,"from":"a.txt","to":"renamed.txt","expected_hash":updated["hash"]}))).await);
    result(rpc.call("fs.delete",fixture.params(false,json!({"repo_id":fixture.repo,"path":"renamed.txt","expected_hash":updated["hash"]}))).await);
    assert!(!fixture.repo_root().join("renamed.txt").exists());
    let binary = [0, 255, 128, 13, 10];
    result(
        rpc.call(
            "fs.write",
            write(
                "binary.bin",
                json!(STANDARD.encode(binary)),
                "base64",
                Value::Null,
            ),
        )
        .await,
    );
    let read = result(
        rpc.call("fs.read", fixture.params(false, fixture.file("binary.bin")))
            .await,
    );
    assert_eq!(read["encoding"], "base64");
    assert_eq!(
        STANDARD.decode(read["content"].as_str().unwrap()).unwrap(),
        binary
    );
    // A window of the file: the whole file's size and hash ride along.
    let mut windowed = fixture.file("binary.bin");
    windowed["offset"] = json!(1);
    windowed["length"] = json!(3);
    let window = result(rpc.call("fs.read", fixture.params(false, windowed)).await);
    assert_eq!(window["size"], binary.len());
    assert_eq!(window["hash"], read["hash"]);
    assert_eq!(window["offset"], 1);
    assert_eq!(window["length"], 3);
    assert_eq!(
        STANDARD
            .decode(window["content"].as_str().unwrap())
            .unwrap(),
        binary[1..4]
    );
    let outside = fixture.root.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("victim"), b"preserved").unwrap();
    std::os::unix::fs::symlink(&outside, fixture.repo_root().join("escape")).unwrap();
    for path in [
        "../outside/victim",
        "/tmp/dispatch-escape",
        ".git/config",
        "escape/victim",
        ".g5/runtime/secret",
    ] {
        assert!(
            rpc.call("fs.write", write(path, json!("bad"), "utf-8", Value::Null))
                .await
                .get("error")
                .is_some(),
            "accepted forbidden path {path}"
        );
    }
    assert_eq!(std::fs::read(outside.join("victim")).unwrap(), b"preserved");
    let cap = "x".repeat(MAX_FILE_BYTES);
    result(
        rpc.call(
            "fs.write",
            write("at-cap", json!(cap), "utf-8", Value::Null),
        )
        .await,
    );
    assert_eq!(
        std::fs::metadata(fixture.repo_root().join("at-cap"))
            .unwrap()
            .len(),
        MAX_FILE_BYTES as u64
    );
    error(
        rpc.call(
            "fs.write",
            write(
                "too-large",
                json!("x".repeat(MAX_FILE_BYTES + 1)),
                "utf-8",
                Value::Null,
            ),
        )
        .await,
        "PAYLOAD_TOO_LARGE",
    );
    assert!(!fixture.repo_root().join("too-large").exists());
    error(
        rpc.call(
            "fs.write",
            write(
                "binary-too-large",
                json!(STANDARD.encode(vec![0xff; MAX_FILE_BYTES + 1])),
                "base64",
                Value::Null,
            ),
        )
        .await,
        "PAYLOAD_TOO_LARGE",
    );
    assert!(!fixture.repo_root().join("binary-too-large").exists());
    // Concurrent create-if-absent requests share the actual mutation boundary.
    rpc.send(json!({"jsonrpc":"2.0","id":"race-a","method":"fs.write","params":write("race",json!("a"),"utf-8",Value::Null)})).await;
    rpc.send(json!({"jsonrpc":"2.0","id":"race-b","method":"fs.write","params":write("race",json!("b"),"utf-8",Value::Null)})).await;
    let mut winners = 0;
    let mut conflicts = 0;
    while winners + conflicts < 2 {
        let reply = rpc.recv().await;
        if reply["id"] == "race-a" || reply["id"] == "race-b" {
            if reply.get("result").is_some() {
                winners += 1;
            } else {
                error(reply, "FILE_EXISTS");
                conflicts += 1;
            }
        } else {
            rpc.pending.push(reply);
        }
    }
    assert_eq!((winners, conflicts), (1, 1));

    result(
        rpc.call("admission.close", fixture.params(true, json!({})))
            .await,
    );
    error(
        rpc.call(
            "fs.write",
            write("closed", json!("bad"), "utf-8", Value::Null),
        )
        .await,
        "WORKSPACE_STOPPING",
    );
    result(
        rpc.call("admission.open", fixture.params(true, json!({})))
            .await,
    );
    result(
        rpc.call(
            "fs.write",
            write("reopened", json!("ok"), "utf-8", Value::Null),
        )
        .await,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_native_watch_port_events_and_lease_binding() {
    let fixture = Dispatcher::start(8).await;
    let mut rpc = fixture.connect().await;
    result(
        rpc.call("fs.watch", fixture.params(false, fixture.file("")))
            .await,
    );
    std::fs::write(fixture.repo_root().join("native.txt"), b"native editor").unwrap();
    let changed = rpc.event("file.changed").await;
    assert_eq!(changed["params"]["vm_id"], fixture.vm.to_string());
    assert_eq!(changed["params"]["path"], "native.txt");
    result(
        rpc.call("ports.list", fixture.params(false, json!({})))
            .await,
    );
    let renewal = rpc.event("lease.renew").await;
    let seq = renewal["params"]["seq"].as_u64().unwrap();
    rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"error":{"code":503,"message":"temporary renewal refusal"}})).await;
    result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert!(!rpc
        .pending
        .iter()
        .any(|reply| reply["error"]["message"] == "INVALID_REQUEST"));
    sleep(Duration::from_millis(1500)).await;
    let retry = rpc.event("lease.renew").await;
    assert!(retry["params"]["seq"].as_u64().unwrap() > seq);
    rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"result":{"seq":seq,"token":token(Uuid::new_v4()),"lease_ttl_secs":120}})).await;
    let rejected = result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert_eq!(rejected["lease_renewal_seq"], 0);
    rpc.send(json!({"jsonrpc":"2.0","id":renewal["id"],"result":{"seq":seq,"token":token(fixture.vm),"lease_ttl_secs":120}})).await;
    let accepted = result(rpc.call("health", fixture.params(true, json!({}))).await);
    assert_eq!(accepted["lease_renewal_seq"], seq);
    assert_eq!(accepted["lease_valid"], true);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    loop {
        if rpc.event("port.opened").await["params"]["port"] == port {
            break;
        }
    }
    drop(listener);
    loop {
        if rpc.event("port.closed").await["params"]["port"] == port {
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_replays_completed_mutations_after_deadline_and_during_drain() {
    let fixture = Dispatcher::start(120).await;
    let mut rpc = fixture.connect().await;
    let mut params = fixture.params(
        false,
        json!({
            "repo_id":fixture.repo,"path":"replay.txt","content":"original",
            "encoding":"utf-8","expected_hash":null
        }),
    );
    let original_deadline = now_ms() + 1500;
    params["context"]["deadline_ms"] = json!(original_deadline);
    let saved = result(rpc.call("fs.write", params.clone()).await);
    result(
        rpc.call("admission.close", fixture.params(true, json!({})))
            .await,
    );
    std::fs::write(fixture.repo_root().join("replay.txt"), b"later native edit").unwrap();
    params["context"]["deadline_ms"] = json!(now_ms() + 120_000);
    assert_eq!(result(rpc.call("fs.write", params.clone()).await), saved);
    sleep(Duration::from_millis(
        original_deadline.saturating_sub(now_ms()) + 50,
    ))
    .await;
    params["context"]["deadline_ms"] = json!(now_ms() + 120_000);
    assert_eq!(result(rpc.call("fs.write", params.clone()).await), saved);
    params["content"] = json!("different intent");
    error(rpc.call("fs.write", params).await, "IDEMPOTENCY_KEY_REUSED");
    assert_eq!(
        std::fs::read(fixture.repo_root().join("replay.txt")).unwrap(),
        b"later native edit"
    );

    result(
        rpc.call("admission.open", fixture.params(true, json!({})))
            .await,
    );
    let saved = result(
        rpc.call(
            "fs.write",
            fixture.params(
                false,
                json!({
                    "repo_id":fixture.repo,"path":"recreated.txt","content":"same bytes","encoding":"utf-8",
                    "expected_hash":null
                }),
            ),
        )
        .await,
    );
    let mut delete = fixture.params(
        false,
        json!({
            "repo_id":fixture.repo,"path":"recreated.txt","expected_hash":saved["hash"]
        }),
    );
    let deleted = result(rpc.call("fs.delete", delete.clone()).await);
    std::fs::write(fixture.repo_root().join("recreated.txt"), b"same bytes").unwrap();
    delete["context"]["deadline_ms"] = json!(now_ms() + 120_000);
    assert_eq!(result(rpc.call("fs.delete", delete).await), deleted);
    assert_eq!(
        std::fs::read(fixture.repo_root().join("recreated.txt")).unwrap(),
        b"same bytes"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_retry_after_pre_entry_deadline_can_execute() {
    let fixture = Dispatcher::start_with_gate(120, true).await;
    let mut rpc = fixture.connect().await;
    let first = fixture.params(
        false,
        json!({
            "repo_id":fixture.repo,"path":"first","content":"first","encoding":"utf-8","expected_hash":null
        }),
    );
    rpc.send(json!({"jsonrpc":"2.0","id":"first","method":"fs.write","params":first}))
        .await;
    timeout(Duration::from_secs(5), async {
        while !fixture.root.path().join("mutation-gate.entered").exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut retry = fixture.params(
        false,
        json!({
            "repo_id":fixture.repo,"path":"retry","content":"retried","encoding":"utf-8","expected_hash":null
        }),
    );
    retry["context"]["deadline_ms"] = json!(now_ms() + 300);
    error(
        rpc.call("fs.write", retry.clone()).await,
        "DEADLINE_EXCEEDED",
    );
    assert!(!fixture.repo_root().join("retry").exists());
    std::fs::write(
        fixture.root.path().join("mutation-gate.release"),
        b"release",
    )
    .unwrap();
    retry["context"]["deadline_ms"] = json!(now_ms() + 120_000);
    let saved = result(rpc.call("fs.write", retry).await);
    assert!(saved["hash"].is_string());
    assert_eq!(
        std::fs::read(fixture.repo_root().join("retry")).unwrap(),
        b"retried"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_rpc_capacity_recovers_after_inflight_requests_complete() {
    let fixture = Dispatcher::start_with_gate(120, true).await;
    let mut rpc = fixture.connect().await;
    for index in 0..8 {
        rpc.send(json!({"jsonrpc":"2.0","id":format!("burst:{index}"),"method":"fs.write",
            "params":fixture.params(false,json!({"repo_id":fixture.repo,"path":format!("burst-{index}"),"content":"burst","encoding":"utf-8","expected_hash":null}))
        })).await;
    }
    error(
        rpc.call("health", fixture.params(true, json!({}))).await,
        "TOO_MANY_REQUESTS",
    );
    std::fs::write(
        fixture.root.path().join("mutation-gate.release"),
        b"release",
    )
    .unwrap();
    let mut completed = 0;
    while completed < 8 {
        let reply = rpc.recv().await;
        if reply["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("burst:"))
        {
            result(reply);
            completed += 1;
        }
    }
    result(rpc.call("health", fixture.params(true, json!({}))).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn dispatcher_close_drains_a_real_inflight_write_and_reconnect_preserves_outcome() {
    let fixture = Dispatcher::start_with_gate(120, true).await;
    let mut rpc = fixture.connect().await;
    let params=fixture.params(false,json!({"repo_id":fixture.repo,"path":"inflight","content":"durable outcome","encoding":"utf-8","expected_hash":null}));
    rpc.send(json!({"jsonrpc":"2.0","id":"write","method":"fs.write","params":params}))
        .await;
    timeout(Duration::from_secs(5), async {
        while !fixture.root.path().join("mutation-gate.entered").exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("write entered admitted boundary");
    rpc.send(json!({"jsonrpc":"2.0","id":"close","method":"admission.close","params":fixture.params(true,json!({}))})).await;
    timeout(Duration::from_secs(3), async {
        loop {
            let health = result(rpc.call("health", fixture.params(true, json!({}))).await);
            assert_eq!(health["inflight_mutations"], 1);
            if health["admission_open"] == false {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !rpc.pending.iter().any(|message| message["id"] == "close"),
        "close answered before write drained"
    );
    error(rpc.call("fs.write",fixture.params(false,json!({"repo_id":fixture.repo,"path":"refused","content":"no","encoding":"utf-8","expected_hash":null}))).await,"WORKSPACE_STOPPING");
    assert!(!fixture.repo_root().join("refused").exists());
    // Replacing the authenticated transport cancels its reply tasks, while the
    // already-admitted write remains owned by the runtime until it finishes.
    let mut replacement = fixture.connect().await;
    rpc.assert_retired(json!({"jsonrpc":"2.0","id":"retired-probe","method":"health","params":fixture.params(true,json!({}))})).await;
    let health = result(
        replacement
            .call("health", fixture.params(true, json!({})))
            .await,
    );
    assert_eq!(health["inflight_mutations"], 1);
    assert_eq!(health["admission_open"], false);
    replacement.send(json!({"jsonrpc":"2.0","id":"close-after-reconnect","method":"admission.close","params":fixture.params(true,json!({}))})).await;

    std::fs::write(
        fixture.root.path().join("mutation-gate.release"),
        b"release",
    )
    .unwrap();
    loop {
        let message = replacement.recv().await;
        if message["id"] == "close-after-reconnect" {
            result(message);
            break;
        }
    }
    assert_eq!(
        std::fs::read(fixture.repo_root().join("inflight")).unwrap(),
        b"durable outcome"
    );
    let mut rpc = replacement;
    result(
        rpc.call("admission.open", fixture.params(true, json!({})))
            .await,
    );
    let replay = result(rpc.call("fs.write", params).await);
    assert_eq!(
        replay["hash"],
        git(&fixture.repo_root(), &["hash-object", "inflight"])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn noninterrupt_fork_drains_admitted_code_and_docs_mutations_before_one_flush() {
    // The flush Git client calls back into the dispatcher's Tokio HTTP relay.
    // With one worker, a blocking child wait would starve that relay.
    let mut env = inherited_loader_env();
    env.push(("TOKIO_WORKER_THREADS".into(), "1".into()));
    let fixture = TestDispatcher::spawn(
        Path::new(env!("CARGO_BIN_EXE_g5d")),
        DispatcherOptions {
            mutation_gate: true,
            git_origin: true,
            repos: vec![
                RepoSpec::default(),
                RepoSpec {
                    root_path: "docs".into(),
                    ..RepoSpec::default()
                },
            ],
            env,
            ..DispatcherOptions::default()
        },
    )
    .await;
    let code_id = fixture.repos[0].0;
    let docs_id = fixture.repos[1].0;
    let code = fixture.member_root(0);
    let docs = fixture.member_root(1);
    std::fs::write(code.join("delete-me.txt"), b"old code\n").unwrap();
    std::fs::write(docs.join("rename-me.md"), b"old docs\n").unwrap();
    let delete_hash = git(&code, &["hash-object", "delete-me.txt"]);
    let rename_hash = git(&docs, &["hash-object", "rename-me.md"]);
    for (repo_id, root) in [(code_id, &code), (docs_id, &docs)] {
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "before fork race"]);
        let base = git(root, &["rev-parse", "HEAD"]);
        let bare = fixture.origin.as_ref().unwrap().bare(repo_id);
        git(
            root,
            &["remote", "add", "origin-fixture", bare.to_str().unwrap()],
        );
        git(
            root,
            &[
                "push",
                "-q",
                "origin-fixture",
                &format!("{base}:refs/heads/shared"),
            ],
        );
    }

    let mut rpc = fixture.connect().await;
    for (id, method, params) in [
        (
            "code-write",
            "fs.write",
            json!({"repo_id":code_id,"path":"new-code.txt","content":"complete code bytes\n","encoding":"utf-8","expected_hash":null}),
        ),
        (
            "docs-write",
            "fs.write",
            json!({"repo_id":docs_id,"path":"new-doc.md","content":"complete docs bytes\n","encoding":"utf-8","expected_hash":null}),
        ),
        (
            "code-delete",
            "fs.delete",
            json!({"repo_id":code_id,"path":"delete-me.txt","expected_hash":delete_hash}),
        ),
        (
            "docs-rename",
            "fs.rename",
            json!({"repo_id":docs_id,"from":"rename-me.md","to":"renamed.md","expected_hash":rename_hash}),
        ),
    ] {
        rpc.send(
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":fixture.params(false,params)}),
        )
        .await;
    }
    timeout(Duration::from_secs(5), async {
        loop {
            let health = result(rpc.call("health", fixture.params(true, json!({}))).await);
            if health["inflight_mutations"] == 4 {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("all four mutations must enter before the fork barrier");
    // Admission counts a mutation before it reaches the gate (it first waits
    // for the serial lock and a blocking thread), so the gate is awaited.
    timeout(Duration::from_secs(5), async {
        while !fixture.root.path().join("mutation-gate.entered").exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the first admitted mutation must reach the gate");
    rpc.send(json!({"jsonrpc":"2.0","id":"fork-close","method":"admission.close","params":fixture.params(true,json!({"mode":"fork","interrupt":false,"operation_id":"f7-race"}))})).await;
    timeout(Duration::from_secs(5), async {
        loop {
            let health = result(rpc.call("health", fixture.params(true, json!({}))).await);
            assert_eq!(health["inflight_mutations"], 4);
            if health["admission_open"] == false {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fork admission did not close while admitted operations were held");
    error(
        rpc.call(
            "fs.write",
            fixture.params(false,json!({"repo_id":code_id,"path":"too-late.txt","content":"excluded","encoding":"utf-8","expected_hash":null})),
        )
        .await,
        "WORKSPACE_STOPPING",
    );
    assert!(!rpc.pending.iter().any(|reply| reply["id"] == "fork-close"));
    std::fs::write(
        fixture.root.path().join("mutation-gate.release"),
        b"release",
    )
    .unwrap();
    let mut completed = std::collections::HashSet::new();
    timeout(Duration::from_secs(20), async {
        while completed.len() < 5 {
            let reply = rpc.recv().await;
            // File/port notifications may arrive between mutation replies.
            let Some(id) = reply["id"].as_str().map(str::to_owned) else {
                assert!(
                    reply["method"].as_str().is_some(),
                    "unexpected idless dispatcher response: {reply}"
                );
                rpc.pending.push(reply);
                continue;
            };
            if [
                "code-write",
                "docs-write",
                "code-delete",
                "docs-rename",
                "fork-close",
            ]
            .contains(&id.as_str())
            {
                result(reply);
                assert!(completed.insert(id.clone()), "duplicate reply for {id}");
            } else {
                rpc.pending.push(reply);
            }
        }
    })
    .await
    .expect("fork barrier did not drain admitted mutations");

    let mut flush_params = fixture.params(true, json!({"reason":"fork"}));
    flush_params["context"]["idempotency_key"] = json!("f7-race-flush");
    let receipt = result(
        timeout(
            Duration::from_secs(20),
            rpc.call("workspace.flush", flush_params),
        )
        .await
        .expect("flush Git push starved its own Tokio transport"),
    );
    let members = receipt["repos"].as_array().expect("flush member vector");
    assert_eq!(members.len(), 2, "flush lost a repository: {receipt}");
    let flush_id = receipt["flush_id"].as_str().expect("flush identity");
    for (repo_id, expected_present, expected_absent) in [
        (
            code_id,
            vec!["new-code.txt"],
            vec!["delete-me.txt", "too-late.txt"],
        ),
        (
            docs_id,
            vec!["new-doc.md", "renamed.md"],
            vec!["rename-me.md"],
        ),
    ] {
        let member = members
            .iter()
            .find(|member| member["repo_id"] == repo_id.to_string())
            .expect("flushed repository receipt");
        let bare = fixture.origin.as_ref().unwrap().bare(repo_id);
        let snapshot_ref = format!("refs/g5/flush/{}/{flush_id}/snapshot", fixture.workspace);
        assert_eq!(
            member["snapshot_oid"],
            git(&bare, &["rev-parse", &snapshot_ref])
        );
        let paths = git(&bare, &["ls-tree", "-r", "--name-only", &snapshot_ref]);
        for path in expected_present {
            assert!(
                paths.lines().any(|candidate| candidate == path),
                "missing {path}: {paths}"
            );
        }
        for path in expected_absent {
            assert!(
                !paths.lines().any(|candidate| candidate == path),
                "unexpected {path}: {paths}"
            );
        }
    }
    let code_bare = fixture.origin.as_ref().unwrap().bare(code_id);
    let docs_bare = fixture.origin.as_ref().unwrap().bare(docs_id);
    let snapshot_ref = format!("refs/g5/flush/{}/{flush_id}/snapshot", fixture.workspace);
    assert_eq!(
        git(
            &code_bare,
            &["show", &format!("{snapshot_ref}:new-code.txt")]
        ),
        "complete code bytes"
    );
    assert_eq!(
        git(&docs_bare, &["show", &format!("{snapshot_ref}:new-doc.md")]),
        "complete docs bytes"
    );
    assert_eq!(fixture.origin.as_ref().unwrap().flush_count(), 1);
    result(
        rpc.call(
            "admission.open",
            fixture.params(true, json!({"operation_id":"f7-race"})),
        )
        .await,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial(dispatcher_process)]
async fn flush_native_error_redacts_configured_secret_and_truncated_prefix() {
    for truncated in [false, true] {
        let fixture = TestDispatcher::spawn(
            Path::new(env!("CARGO_BIN_EXE_g5d")),
            DispatcherOptions {
                git_origin: true,
                env: inherited_loader_env(),
                ..DispatcherOptions::default()
            },
        )
        .await;
        let repo = fixture.repo_root();
        std::fs::write(repo.join("base.txt"), "base\n").unwrap();
        git(&repo, &["add", "base.txt"]);
        git(&repo, &["commit", "-qm", "base"]);
        let bare = fixture.origin.as_ref().unwrap().bare(fixture.repo);
        git(
            &repo,
            &[
                "push",
                "-q",
                bare.to_str().unwrap(),
                "HEAD:refs/heads/shared",
            ],
        );
        std::fs::write(repo.join(".gitattributes"), "dirty.txt filter=leak\n").unwrap();
        std::fs::write(repo.join("dirty.txt"), "dirty\n").unwrap();
        let prefix = &fixture.workspace_token[..40];
        // End bounded stderr halfway through the token. Git's own following
        // filter diagnostic must not change whether that prefix is protected.
        let mut diagnostic = if truncated {
            vec![b'x'; 64 * 1024 - prefix.len()]
        } else {
            Vec::new()
        };
        diagnostic.extend_from_slice(fixture.workspace_token.as_bytes());
        let diagnostic_path = fixture.root.path().join("filter-diagnostic");
        std::fs::write(&diagnostic_path, diagnostic).unwrap();
        let filter = format!("cat '{}' >&2; exit 1", diagnostic_path.display());
        git(&repo, &["config", "filter.leak.clean", &filter]);
        git(&repo, &["config", "filter.leak.required", "true"]);
        let mut rpc = fixture.connect().await;
        result(
            rpc.call("admission.close", fixture.params(true, json!({})))
                .await,
        );
        let mut params = fixture.params(true, json!({"reason":"shutdown"}));
        params["context"]["idempotency_key"] = json!("redaction-regression");
        let reply = rpc.call("workspace.flush", params).await;
        assert!(reply.get("error").is_some());
        let detail = reply.to_string();
        assert!(detail.contains("REDACTED"));
        assert!(!detail.contains(&fixture.workspace_token));
        assert!(!detail.contains(prefix));
        if truncated {
            assert!(detail.contains("output truncated"));
        }
        let logs = fixture.stderr();
        assert!(!logs.contains(&fixture.workspace_token));
        assert!(!logs.contains(prefix));
    }
}
