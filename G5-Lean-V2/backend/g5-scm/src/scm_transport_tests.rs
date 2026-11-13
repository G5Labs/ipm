//! Hermetic HTTPS proxy tests of the real libgit2 credential callback. The
//! listener never forwards traffic: both provider and redirect targets are local.
use crate::GitAuth;
use openssl::{
    asn1::Asn1Time,
    hash::MessageDigest,
    pkey::PKey,
    rsa::Rsa,
    ssl::{SslAcceptor, SslMethod, SslStream},
    x509::{X509NameBuilder, X509},
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn headers(stream: &mut impl Read) -> String {
    let mut bytes = Vec::new();
    let mut byte = [0];
    while stream.read_exact(&mut byte).is_ok() {
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).to_ascii_lowercase()
}

/// Connection cancellation is local to that connection, as in a real HTTPS
/// server. Only completed CONNECT and TLS handshakes reach request assertions.
/// Empty pre-TLS closures are harmless; nonempty TLS input stays strict.
fn proxy_tls(
    mut socket: TcpStream,
    acceptor: &SslAcceptor,
) -> Option<(String, SslStream<TcpStream>)> {
    let connect = headers(&mut socket);
    if !connect.ends_with("\r\n\r\n") || !connect.starts_with("connect ") {
        return None;
    }
    if socket
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .is_err()
    {
        return None;
    }
    match socket.peek(&mut [0]) {
        Ok(0) => return None,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            return None
        }
        Err(error) => panic!("fixture pre-TLS peek failed: {error}"),
        Ok(_) => {}
    }
    let tls = acceptor
        .accept(socket)
        .expect("fixture TLS handshake failed");
    Some((connect, tls))
}

fn tls_acceptor() -> Arc<SslAcceptor> {
    let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "hermetic-scm-test")
        .unwrap();
    let name = name.build();
    let mut cert = X509::builder().unwrap();
    cert.set_version(2).unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(&name).unwrap();
    cert.set_pubkey(&key).unwrap();
    cert.set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    cert.set_not_after(&Asn1Time::days_from_now(1).unwrap())
        .unwrap();
    cert.sign(&key, MessageDigest::sha256()).unwrap();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&key).unwrap();
    acceptor.set_certificate(&cert.build()).unwrap();
    Arc::new(acceptor.build())
}

#[test]
fn proxy_fixture_rejects_non_tls_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let acceptor = tls_acceptor();
    let server = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        proxy_tls(socket, &acceptor)
    });
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    client
        .write_all(b"CONNECT fixture:443 HTTP/1.1\r\nHost: fixture:443\r\n\r\n")
        .unwrap();
    assert!(headers(&mut client).starts_with("http/1.1 200 "));
    client.write_all(b"not a TLS ClientHello\r\n").unwrap();
    drop(client);
    let failure = server
        .join()
        .expect_err("bytes sent after CONNECT must complete a real TLS handshake");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("fixture TLS handshake failed"),
        "{message}"
    );
}

/// Exercise the production native Git command through real TLS, Basic auth,
/// and git-http-backend, including an operator-configured nondefault port.
#[test]
fn authenticated_native_lease_push_and_compensation_succeed() {
    for url in [
        "https://github.com/remote.git",
        "https://gitlab.internal:8443/remote.git",
        "git@github.com:remote.git",
        "ssh://git@github.com/remote.git",
    ] {
        native_lease_transport_case(url, None);
    }
}

#[derive(Clone, Copy)]
enum AbandonedProxyPhase {
    BeforeTls,
    BeforeConnect,
}

#[test]
fn authenticated_native_lease_push_recovers_after_abandoned_proxy_connections() {
    for phase in [
        AbandonedProxyPhase::BeforeTls,
        AbandonedProxyPhase::BeforeConnect,
    ] {
        for url in [
            "https://github.com/remote.git",
            "https://gitlab.internal:8443/remote.git",
        ] {
            native_lease_transport_case(url, Some(phase));
        }
    }
}

fn native_lease_transport_case(url: &str, abandoned: Option<AbandonedProxyPhase>) {
    use std::process::{Command, Stdio};
    let root = tempfile::tempdir().unwrap();
    let remote_path = root.path().join("remote.git");
    let remote = git2::Repository::init_bare(&remote_path).unwrap();
    remote
        .config()
        .unwrap()
        .set_bool("http.receivepack", true)
        .unwrap();
    let local_path = root.path().join("local.git");
    let local = git2::Repository::init_bare(&local_path).unwrap();
    local.remote("origin", url).unwrap();
    if url.starts_with("git@") || url.starts_with("ssh://") {
        // A more-specific identity mapping must win over a broad push rewrite,
        // just as it does in Git. The normalized URL is the actual transport.
        let normalized = "https://github.com/remote.git";
        let mut config = local.config().unwrap();
        config
            .set_str(
                "url.https://unused.invalid/.pushInsteadOf",
                "https://github.com/",
            )
            .unwrap();
        config
            .set_str(&format!("url.{normalized}.pushInsteadOf"), normalized)
            .unwrap();
    }
    let tree_oid = local.treebuilder(None).unwrap().write().unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.com").unwrap();
    let oid = local
        .commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "fixture",
            &local.find_tree(tree_oid).unwrap(),
            &[],
        )
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    local
        .config()
        .unwrap()
        .set_str("http.proxy", &format!("http://{address}"))
        .unwrap();
    // Only this isolated fixture trusts its self-signed TLS certificate.
    local
        .config()
        .unwrap()
        .set_bool("http.sslVerify", false)
        .unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let server_done = done.clone();
    let acceptor = tls_acceptor();
    let project_root = root.path().to_path_buf();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut authenticated_requests = 0;
        while !server_done.load(Ordering::SeqCst) && Instant::now() < deadline {
            let (socket, _) = match listener.accept() {
                Ok(socket) => socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let Some((_, mut tls)) = proxy_tls(socket, &acceptor) else {
                continue;
            };
            let request = headers(&mut tls);
            if !request.ends_with("\r\n\r\n") {
                continue;
            }
            let expected =
                openssl::base64::encode_block(b"x-access-token:member-token").to_ascii_lowercase();
            if !request.contains(&format!("authorization: basic {expected}")) {
                let _ = tls.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=git\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                let _ = tls.shutdown();
                continue;
            }
            authenticated_requests += 1;
            let mut words = request.split_whitespace();
            let method = words.next().unwrap().to_ascii_uppercase();
            let path = words.next().unwrap();
            let (path, query) = path.split_once('?').unwrap_or((path, ""));
            let content_length = request
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap_or("0")
                .trim()
                .parse::<usize>()
                .unwrap();
            let content_type = request
                .lines()
                .find_map(|line| line.strip_prefix("content-type: "))
                .unwrap_or("")
                .trim();
            let mut body = vec![0; content_length];
            tls.read_exact(&mut body).unwrap();
            let mut backend = Command::new("git")
                .arg("http-backend")
                .env("GIT_PROJECT_ROOT", &project_root)
                .env("GIT_HTTP_EXPORT_ALL", "1")
                .env("REQUEST_METHOD", method)
                .env("PATH_INFO", path)
                .env("QUERY_STRING", query)
                .env("CONTENT_TYPE", content_type)
                .env("CONTENT_LENGTH", content_length.to_string())
                .env("REMOTE_USER", "fixture")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            backend.stdin.take().unwrap().write_all(&body).unwrap();
            let output = backend.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let separator = output
                .stdout
                .windows(4)
                .position(|bytes| bytes == b"\r\n\r\n")
                .unwrap();
            let response_headers = String::from_utf8_lossy(&output.stdout[..separator]);
            assert!(!response_headers.contains("Status:"), "{response_headers}");
            let response_body = &output.stdout[separator + 4..];
            write!(
                tls,
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n{}\r\n\r\n",
                response_body.len(),
                response_headers
            )
            .unwrap();
            tls.write_all(response_body).unwrap();
            let _ = tls.shutdown();
        }
        authenticated_requests
    });
    if let Some(phase) = abandoned {
        // A cancelled TCP/proxy attempt must not destroy the server needed by
        // subsequent real authenticated push and compensation requests.
        let mut cancelled = std::net::TcpStream::connect(address).unwrap();
        cancelled
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        if matches!(phase, AbandonedProxyPhase::BeforeTls) {
            let target = url::Url::parse(url).unwrap();
            let authority = format!(
                "{}:{}",
                target.host_str().unwrap(),
                target.port_or_known_default().unwrap()
            );
            write!(
                cancelled,
                "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n\r\n"
            )
            .unwrap();
            assert!(headers(&mut cancelled).starts_with("http/1.1 200"));
        }
        cancelled.shutdown(std::net::Shutdown::Both).unwrap();
        drop(cancelled);
    }
    let policy = crate::ScmOriginPolicy::from_gitlab_api_url(Some(
        "https://gitlab.internal:8443/api/v4/user",
    ));
    let auth = if url.starts_with("git@") || url.starts_with("ssh://") {
        GitAuth::Token("member-token".into())
    } else {
        let provider = policy.provider_for_url(url).unwrap();
        GitAuth::scm_token_for_provider("member-token".into(), provider, url)
    };
    let repo = crate::GitRepo::open(&local_path).unwrap();
    let git = crate::native::NativeGit::cli(Duration::from_secs(60)).unwrap();
    let push = repo.push_create_ref(&git, "origin", &oid.to_string(), "main", &auth);
    if let Err(error) = push {
        done.store(true, Ordering::SeqCst);
        let server_result = server.join();
        panic!("native authenticated push failed: {error}; fixture result: {server_result:?}");
    }
    assert_eq!(
        remote.find_reference("refs/heads/main").unwrap().target(),
        Some(oid)
    );
    let delete = git.delete_branch_blocking(
        crate::native::BranchDelete {
            repository: local_path.clone(),
            remote: "origin".into(),
            expected_url: url.into(),
            branch: "main".into(),
            expected: oid.to_string(),
        },
        &auth,
    );
    done.store(true, Ordering::SeqCst);
    let server_result = server.join();
    delete.unwrap_or_else(|error| {
        panic!(
            "native authenticated compensation failed: {error}; fixture result: {server_result:?}"
        )
    });
    assert!(server_result.expect("TLS fixture server panicked") >= 4);
    assert_eq!(local.find_remote("origin").unwrap().url().unwrap(), url);
    assert!(remote.find_reference("refs/heads/main").is_err());
}
