//! Token-isolated internal Git transport for modules and native workspace Git.
//!
//! Repository Git never receives a platform credential (design v9 §5.3, P4.2a
//! Native Git boundary): hooks, filters and provider descendants would inherit
//! it. Instead the dispatcher serves a loopback smart-HTTP relay. Native Git
//! receives one member-scoped route for the incarnation; a module operation
//! takes a temporary [`TransportLease`] for one configured member. Its random
//! 256-bit path segment exists only for the lifetime of the lease. The relay accepts exactly
//! the three smart-HTTP endpoints, forwards them to the member's configured
//! internal Git URL with the current workspace token obtained through the
//! installed credential helper, follows no redirects and never reaches any
//! other destination. Git itself sees a plain unauthenticated loopback URL.
//!
//! This is the typed authenticated network boundary P4.2a deferred; P4.3a's
//! flush reuses it.

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use futures_util::Stream;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::git::GitRuntime;

/// Upper bound on one request body relayed upstream (a push pack).
const MAX_REQUEST_BYTES: usize = 512 * 1024 * 1024;
/// Whole-request bound for one relayed Git exchange.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Bound for receiving one inbound request body from the local git client.
const INBOUND_BODY_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Aggregate bytes the transport may hold in spooled request bodies at once,
/// across every concurrent exchange: a request the budget cannot admit is
/// refused before it costs any storage.
const SPOOL_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type ResponseBody = BoxBody<Bytes, BoxError>;

struct LeaseEntry {
    repo_id: String,
    git_url: String,
    /// Cancelled when the lease is dropped: every exchange admitted under
    /// it — even one still receiving its body — stops without forwarding.
    revoked: CancellationToken,
    /// A completed push under this lease wakes [`GitTransport::pushes`].
    observed: bool,
}

/// Source of the current workspace token; the lease rotates it.
pub type TokenSource = Arc<dyn Fn() -> String + Send + Sync>;

pub struct GitTransport {
    port: u16,
    leases: Arc<Mutex<HashMap<String, LeaseEntry>>>,
    git: Arc<GitRuntime>,
    token: TokenSource,
    client: reqwest::Client,
    spool: Arc<SpoolBudget>,
    pushes: Arc<tokio::sync::Notify>,
}

/// The shared budget for spooled request bodies.
struct SpoolBudget {
    limit: u64,
    used: std::sync::atomic::AtomicU64,
}

impl SpoolBudget {
    fn reserve(&self, bytes: u64) -> bool {
        use std::sync::atomic::Ordering;
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            let Some(next) = used.checked_add(bytes) else {
                return false;
            };
            if next > self.limit {
                return false;
            }
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(current) => used = current,
            }
        }
    }

    fn release(&self, bytes: u64) {
        self.used
            .fetch_sub(bytes, std::sync::atomic::Ordering::AcqRel);
    }
}

/// Bytes one exchange holds of the budget; released when the exchange ends.
struct SpoolReservation {
    budget: Arc<SpoolBudget>,
    bytes: u64,
}

impl SpoolReservation {
    fn grow(&mut self, bytes: u64) -> bool {
        if self.budget.reserve(bytes) {
            self.bytes += bytes;
            true
        } else {
            false
        }
    }
}

impl Drop for SpoolReservation {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

/// One operation's addressable relay URL. Dropping it revokes the path.
pub struct TransportLease {
    url: String,
    secret: String,
    leases: Arc<Mutex<HashMap<String, LeaseEntry>>>,
}

impl TransportLease {
    /// The URL to hand to `git` in place of the internal origin.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The origin this lease forwards to.
    #[cfg(test)]
    pub(crate) fn upstream(&self) -> String {
        self.leases.lock().unwrap()[&self.secret].git_url.clone()
    }
}

impl Drop for TransportLease {
    fn drop(&mut self) {
        if let Some(entry) = self.leases.lock().unwrap().remove(&self.secret) {
            entry.revoked.cancel();
        }
    }
}

/// The fixed guest loopback port of the internal Git transport door.
///
/// The platform's internal Git door shares a stable port contract with the
/// dispatcher. Guest loopback is private to the VM; external access goes through
/// the existing admitted forwarding map and its authorization.
pub const GIT_DOOR_GUEST_PORT: u16 = 7101;

impl GitTransport {
    pub async fn start(
        git: Arc<GitRuntime>,
        token: TokenSource,
        bind_port: u16,
    ) -> anyhow::Result<Arc<Self>> {
        Self::start_with_spool_budget(git, token, SPOOL_BUDGET_BYTES, bind_port).await
    }

    async fn start_with_spool_budget(
        git: Arc<GitRuntime>,
        token: TokenSource,
        spool_budget: u64,
        bind_port: u16,
    ) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(("127.0.0.1", bind_port)).await?;
        let port = listener.local_addr()?.port();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(UPSTREAM_TIMEOUT)
            .build()?;
        let transport = Arc::new(Self {
            port,
            leases: Arc::default(),
            git,
            token,
            client,
            spool: Arc::new(SpoolBudget {
                limit: spool_budget,
                used: std::sync::atomic::AtomicU64::new(0),
            }),
            pushes: Arc::new(tokio::sync::Notify::new()),
        });
        tokio::spawn({
            let transport = transport.clone();
            async move {
                loop {
                    let Ok((socket, peer)) = listener.accept().await else {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    };
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let transport = transport.clone();
                    tokio::spawn(async move {
                        let service = hyper::service::service_fn(move |request| {
                            let transport = transport.clone();
                            async move { Ok::<_, Infallible>(transport.handle(request).await) }
                        });
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(socket), service)
                            .await;
                    });
                }
            }
        });
        Ok(transport)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Woken (with a stored permit, so none is missed) whenever a push under
    /// an observed lease completes upstream (`g5.personal.memory-sync`).
    pub fn pushes(&self) -> Arc<tokio::sync::Notify> {
        self.pushes.clone()
    }

    /// Lease a relay path for one member operation.
    pub fn lease(&self, repo_id: &str, git_url: &str) -> TransportLease {
        self.lease_with(repo_id, git_url, false)
    }

    /// Lease a relay path whose completed pushes are code pushes of a
    /// workspace repository: they wake [`Self::pushes`].
    pub fn observed_lease(&self, repo_id: &str, git_url: &str) -> TransportLease {
        self.lease_with(repo_id, git_url, true)
    }

    fn lease_with(&self, repo_id: &str, git_url: &str, observed: bool) -> TransportLease {
        let secret = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.leases.lock().unwrap().insert(
            secret.clone(),
            LeaseEntry {
                repo_id: repo_id.to_owned(),
                git_url: git_url.trim_end_matches('/').to_owned(),
                revoked: CancellationToken::new(),
                observed,
            },
        );
        TransportLease {
            url: format!("http://127.0.0.1:{}/{secret}/git/{repo_id}", self.port),
            secret,
            leases: self.leases.clone(),
        }
    }

    async fn handle(self: Arc<Self>, request: Request<Incoming>) -> Response<ResponseBody> {
        let method = request.method().clone();
        let path = request.uri().path().to_owned();
        let query = request.uri().query().map(str::to_owned);
        // /<secret>/git/<repo_id>/<endpoint>
        let mut segments = path.trim_start_matches('/').splitn(4, '/');
        let (secret, git, repo_id, endpoint) = (
            segments.next().unwrap_or_default(),
            segments.next().unwrap_or_default(),
            segments.next().unwrap_or_default(),
            segments.next().unwrap_or_default(),
        );
        let Some((git_url, revoked, observed)) =
            self.leases.lock().unwrap().get(secret).and_then(|lease| {
                (git == "git" && lease.repo_id == repo_id)
                    .then(|| (lease.git_url.clone(), lease.revoked.clone(), lease.observed))
            })
        else {
            return reply(StatusCode::NOT_FOUND, "no such transport lease");
        };
        let allowed = match (method.as_str(), endpoint) {
            ("GET", "info/refs") => query
                .as_deref()
                .is_some_and(|q| q == "service=git-upload-pack" || q == "service=git-receive-pack"),
            ("POST", "git-upload-pack") | ("POST", "git-receive-pack") => query.is_none(),
            _ => false,
        };
        if !allowed {
            return reply(StatusCode::NOT_FOUND, "not a smart-HTTP endpoint");
        }
        let observed_push = observed && method_is_receive_pack(&method, endpoint);
        let mut upstream = format!("{git_url}/{endpoint}");
        if let Some(query) = &query {
            upstream.push('?');
            upstream.push_str(query);
        }
        let headers = request.headers().clone();
        // The body is received under the lease — spooled to a private,
        // unlinked temporary file rather than held in memory, so a full-size
        // pack never costs the dispatcher its resident memory — and
        // revocation or the inbound bound abandons the exchange before any
        // credential is obtained.
        let spooled = tokio::select! {
            spooled = tokio::time::timeout(
                INBOUND_BODY_TIMEOUT,
                spool_body(request.into_body(), self.spool.clone()),
            ) => spooled,
            _ = revoked.cancelled() => return reply(StatusCode::GONE, "transport lease revoked"),
        };
        // The reservation rides the request-body stream below, so it lives
        // exactly as long as the spool can still be read — `send()` may
        // return the response head before the body reached EOF.
        let (spool, length, reservation) = match spooled {
            Ok(Ok(spooled)) => spooled,
            Ok(Err(SpoolError::TooLarge)) => {
                return reply(StatusCode::PAYLOAD_TOO_LARGE, "request body too large")
            }
            Ok(Err(SpoolError::Budget)) => {
                return reply(
                    StatusCode::INSUFFICIENT_STORAGE,
                    "transport spool budget exhausted",
                )
            }
            Ok(Err(SpoolError::Client(error))) => {
                tracing::warn!(%error, "internal git transport inbound body failed");
                return reply(StatusCode::BAD_REQUEST, "request body failed");
            }
            Ok(Err(SpoolError::Io(error))) => {
                tracing::warn!(%error, "internal git transport could not spool a request body");
                return reply(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "request body could not be spooled",
                );
            }
            Err(_) => return reply(StatusCode::REQUEST_TIMEOUT, "request body timed out"),
        };
        let body = reqwest::Body::wrap_stream(spooled_stream(spool, reservation));
        // Revalidate after the body: the lease must still be live, and only
        // then is a credential minted for exactly this forward.
        if revoked.is_cancelled() || !self.leases.lock().unwrap().contains_key(secret) {
            return reply(StatusCode::GONE, "transport lease revoked");
        }
        let Ok(Some(credential)) = self.git.credential_for_url(&(self.token)(), &git_url) else {
            return reply(StatusCode::FORBIDDEN, "no credential for this member");
        };
        let mut builder = self
            .client
            .request(method, &upstream)
            .basic_auth(credential.username, Some(credential.password));
        for name in [
            "content-type",
            "accept",
            "content-encoding",
            "accept-encoding",
            "git-protocol",
        ] {
            if let Some(value) = headers.get(name) {
                builder = builder.header(name, value);
            }
        }
        // The spooled length is exact, so upstream sees a sized body.
        builder = builder.header("content-length", length);
        let response = tokio::select! {
            sent = builder.body(body).send() => match sent {
                Ok(response) => response,
                Err(error) => {
                    // Include the DNS/connect cause without logging the member URL.
                    tracing::warn!(error = ?error.without_url(), "internal git transport upstream failure");
                    return reply(StatusCode::BAD_GATEWAY, "upstream unavailable");
                }
            },
            _ = revoked.cancelled() => return reply(StatusCode::GONE, "transport lease revoked"),
        };
        let status = response.status();
        // Wake observers once the push's report has fully arrived. A ref Git
        // refused inside a successful exchange still counts; observers only
        // act on their own state.
        let pushed = (observed_push && status.is_success()).then(|| self.pushes.clone());
        let mut out = Response::builder().status(status.as_u16());
        for name in [
            "content-type",
            "content-encoding",
            "cache-control",
            "pragma",
            "expires",
        ] {
            if let Some(value) = response.headers().get(name) {
                out = out.header(name, value);
            }
        }
        // The upstream body is relayed only while the lease lives: revocation
        // fails the stream, which makes hyper abort the client's exchange
        // rather than complete it as a shorter-but-valid response.
        let mut upstream_body = Box::pin(response.bytes_stream());
        let mut cancelled = Box::pin(revoked.cancelled_owned());
        let mut done = false;
        let stream = futures_util::stream::poll_fn(move |cx| {
            use std::task::Poll;
            if done {
                return Poll::Ready(None);
            }
            if cancelled.as_mut().poll(cx).is_ready() {
                done = true;
                let error: BoxError = "transport lease revoked".into();
                return Poll::Ready(Some(Err(error)));
            }
            match upstream_body.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
                Poll::Ready(Some(Err(error))) => {
                    done = true;
                    let error: BoxError = error.into();
                    Poll::Ready(Some(Err(error)))
                }
                Poll::Ready(None) => {
                    done = true;
                    if let Some(pushes) = &pushed {
                        pushes.notify_one();
                    }
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            }
        });
        out.body(BoxBody::new(StreamBody::new(stream)))
            .unwrap_or_else(|_| reply(StatusCode::BAD_GATEWAY, "invalid upstream response"))
    }
}

fn method_is_receive_pack(method: &hyper::Method, endpoint: &str) -> bool {
    method == hyper::Method::POST && endpoint == "git-receive-pack"
}

/// The spooled file as a request-body stream that owns its budget
/// reservation: the bytes are released only when the stream is dropped,
/// never while hyper may still poll it.
fn spooled_stream(
    spool: tokio::fs::File,
    reservation: SpoolReservation,
) -> impl Stream<Item = std::io::Result<Bytes>> + Send + 'static {
    use futures_util::StreamExt;
    tokio_util::io::ReaderStream::new(spool).map(move |chunk| {
        let _held = &reservation;
        chunk
    })
}

enum SpoolError {
    TooLarge,
    Budget,
    Client(BoxError),
    Io(std::io::Error),
}

/// Receive an inbound body into a private, unlinked temporary file, bounded
/// by `MAX_REQUEST_BYTES` for the request and by the shared budget across
/// requests (reserved frame by frame, so a refused request never holds more
/// than it was admitted), and hand it back rewound with its exact length.
async fn spool_body(
    body: Incoming,
    budget: Arc<SpoolBudget>,
) -> Result<(tokio::fs::File, u64, SpoolReservation), SpoolError> {
    use std::io::SeekFrom;
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};
    let mut body = http_body_util::Limited::new(body, MAX_REQUEST_BYTES);
    let mut file = tokio::fs::File::from_std(tempfile::tempfile().map_err(SpoolError::Io)?);
    let mut reservation = SpoolReservation { budget, bytes: 0 };
    let mut length = 0u64;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| {
            if error
                .downcast_ref::<http_body_util::LengthLimitError>()
                .is_some()
            {
                SpoolError::TooLarge
            } else {
                SpoolError::Client(error)
            }
        })?;
        if let Ok(data) = frame.into_data() {
            if !reservation.grow(data.len() as u64) {
                return Err(SpoolError::Budget);
            }
            file.write_all(&data).await.map_err(SpoolError::Io)?;
            length += data.len() as u64;
        }
    }
    file.flush().await.map_err(SpoolError::Io)?;
    file.seek(SeekFrom::Start(0))
        .await
        .map_err(SpoolError::Io)?;
    Ok((file, length, reservation))
}

fn reply(status: StatusCode, text: &str) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain")
        .body(
            Full::new(Bytes::from(text.to_owned()))
                .map_err(|never| match never {})
                .boxed(),
        )
        .expect("static response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DispatchConfig, OwnerGitIdentity, RepositoryLayout, RuntimePaths, WorkspaceLayout,
    };
    use crate::files::ObjectFormat;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn fixture() -> (tempfile::TempDir, Arc<GitTransport>, String, TcpListener) {
        fixture_with_budget(SPOOL_BUDGET_BYTES).await
    }

    async fn fixture_with_budget(
        spool_budget: u64,
    ) -> (tempfile::TempDir, Arc<GitTransport>, String, TcpListener) {
        fixture_with(spool_budget, false).await
    }

    /// The member string is `<repo_id>|<relay upstream>`. A guest fixture
    /// persists an unreachable loopback origin and maps it to the upstream
    /// through `door_url`, as `DispatchConfig` loading does in a microVM.
    async fn fixture_with(
        spool_budget: u64,
        guest_door: bool,
    ) -> (tempfile::TempDir, Arc<GitTransport>, String, TcpListener) {
        // An upstream that must never be contacted by a revoked exchange.
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_url = format!("http://127.0.0.1:{}", upstream.local_addr().unwrap().port());
        let temp = tempfile::tempdir().unwrap();
        let paths = RuntimePaths::under_test_root(temp.path());
        std::fs::create_dir_all(paths.workspace_root.join("code")).unwrap();
        let repo_id = uuid::Uuid::new_v4();
        let upstream_url = format!("{api_url}/git/{repo_id}");
        let (git_url, door_url) = if guest_door {
            (
                format!("http://localhost:17500/git/{repo_id}"),
                Some(upstream_url.clone()),
            )
        } else {
            (upstream_url.clone(), None)
        };
        let config = DispatchConfig {
            workspace_id: uuid::Uuid::new_v4(),
            vm_id: uuid::Uuid::new_v4(),
            owner_user_id: uuid::Uuid::new_v4(),
            epoch: 1,
            workspace_token: "secret-token".into(),
            listener_token: [1; 32],
            api_url: api_url.parse().unwrap(),
            dispatch_port: 9000,
            lease_ttl_secs: 60,
            owner_git: OwnerGitIdentity {
                name: "Owner Name".into(),
                email: "owner@example.test".into(),
            },
            paths,
            layout: WorkspaceLayout {
                repos: vec![RepositoryLayout {
                    repo_id,
                    root_path: "code".into(),
                    role: None,
                    git_url,
                    branch: "shared".into(),
                    branch_oid: "a".repeat(40),
                    upstream_branch: Some("main".into()),
                    object_format: ObjectFormat::Sha1,
                    follow_branch: false,
                    door_url,
                }],
                restore: None,
                references: Vec::new(),
            },
            module_uid: None,
            module_gid: None,
            git_door_port: 0,
            pty_reattach_grace: std::time::Duration::from_secs(15),
            process_mode: false,
        };
        let git = Arc::new(crate::git::GitRuntime::install(&config).unwrap());
        let transport = GitTransport::start_with_spool_budget(
            git,
            Arc::new(|| "secret-token".to_owned()),
            spool_budget,
            0,
        )
        .await
        .unwrap();
        (
            temp,
            transport,
            format!("{repo_id}|{upstream_url}"),
            upstream,
        )
    }

    /// A microVM guest (G5Labs/G5-Lean#1020): the checkout's origin is the
    /// persisted loopback URL, which nothing on this host serves, and the
    /// relay forwards to the door-mapped upstream. Ordinary `git` in the
    /// checkout must reach that upstream through the relay, credentialed.
    #[tokio::test]
    async fn a_guest_checkout_reaches_its_door_mapped_upstream_through_the_relay() {
        let (temp, transport, member, upstream) = fixture_with(SPOOL_BUDGET_BYTES, true).await;
        let (repo_id, upstream_url) = member.split_once('|').unwrap();
        let persisted = format!("http://localhost:17500/git/{repo_id}");
        let leases = transport.git.configure_native_remotes(&transport).unwrap();
        assert_eq!(leases[0].upstream(), upstream_url);
        let checkout = RuntimePaths::under_test_root(temp.path())
            .workspace_root
            .join("code");
        for args in [
            &["init", "--quiet"][..],
            &["remote", "add", "origin", &persisted],
        ] {
            let output = transport
                .git
                .provider_command("git")
                .current_dir(&checkout)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let mut child = transport
            .git
            .provider_command("git")
            .current_dir(&checkout)
            .args(["ls-remote", "origin"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut socket = tokio::time::timeout(Duration::from_secs(30), next_request(&upstream))
            .await
            .expect("git reaches the door-mapped upstream through the relay");
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        drop(socket);
        let _ = child.kill();
        let _ = child.wait();
        let request = String::from_utf8_lossy(&request);
        assert!(
            request.starts_with(&format!(
                "GET /git/{repo_id}/info/refs?service=git-upload-pack HTTP/1.1\r\n"
            )),
            "{request}"
        );
        use base64::Engine as _;
        let credential =
            base64::engine::general_purpose::STANDARD.encode("x-access-token:secret-token");
        assert!(
            request.to_ascii_lowercase().contains(&format!(
                "authorization: basic {}",
                credential.to_ascii_lowercase()
            )),
            "{request}"
        );
    }

    /// The next connection to the fake upstream that carries a request. The
    /// test host's loopback is shared, and another test's dispatcher probes
    /// every listening port by connecting and closing without a byte (its
    /// `port-connectivity` diagnostic). So a connection counts only once it
    /// has sent something; it is peeked, not read.
    async fn next_request(upstream: &TcpListener) -> tokio::net::TcpStream {
        loop {
            let (socket, _) = upstream.accept().await.unwrap();
            let mut byte = [0u8; 1];
            if socket.peek(&mut byte).await.is_ok_and(|read| read > 0) {
                return socket;
            }
        }
    }

    async fn raw(port: u16, request: &str) -> String {
        let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        let _ =
            tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut response)).await;
        String::from_utf8_lossy(&response).into_owned()
    }

    #[tokio::test]
    async fn the_spool_reservation_lives_as_long_as_the_body_stream() {
        use std::sync::atomic::Ordering;
        let budget = Arc::new(SpoolBudget {
            limit: 100,
            used: std::sync::atomic::AtomicU64::new(0),
        });
        let mut reservation = SpoolReservation {
            budget: budget.clone(),
            bytes: 0,
        };
        assert!(reservation.grow(40));
        assert!(!reservation.grow(61), "the budget is a hard bound");
        let file = tokio::fs::File::from_std(tempfile::tempfile().unwrap());
        let stream = spooled_stream(file, reservation);
        // Response headers arriving before the body reached EOF do not
        // release anything: only dropping the stream does.
        assert_eq!(budget.used.load(Ordering::Acquire), 40);
        drop(stream);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn concurrent_uploads_share_the_spool_budget() {
        // A body over the whole budget is refused before it reaches upstream.
        {
            let (_temp, transport, member, upstream) = fixture_with_budget(16).await;
            let (repo_id, git_url) = member.split_once('|').unwrap();
            let lease = transport.lease(repo_id, git_url);
            let path = lease.url().split_once("127.0.0.1").unwrap().1;
            let path = path[path.find('/').unwrap()..].to_owned();
            let response = raw(
                transport.port(),
                &format!(
                    "POST {path}/git-receive-pack HTTP/1.1\r\nHost: relay\r\nContent-Type: application/x-git-receive-pack-request\r\nContent-Length: 32\r\n\r\n{}",
                    "0".repeat(32)
                ),
            )
            .await;
            assert!(response.starts_with("HTTP/1.1 507"), "{response}");
            assert!(
                tokio::time::timeout(Duration::from_millis(200), next_request(&upstream))
                    .await
                    .is_err(),
                "a refused body must never reach the upstream"
            );
        }

        // An admitted body holds its bytes for exactly as long as its spool
        // exists: until the upstream has taken the whole stream. A body far
        // larger than the loopback socket buffers keeps that transfer (and
        // the reservation) open while the upstream does not read; a second
        // body that would exceed the budget is refused meanwhile, and
        // admitted once the first exchange ends and its spool is dropped.
        const HELD: usize = 32 * 1024 * 1024;
        let (_temp, transport, member, upstream) = fixture_with_budget(HELD as u64 + 1024).await;
        let (repo_id, git_url) = member.split_once('|').unwrap();
        let lease = transport.lease(repo_id, git_url);
        let path = lease.url().split_once("127.0.0.1").unwrap().1;
        let path = path[path.find('/').unwrap()..].to_owned();
        let port = transport.port();
        let post = |len: usize| {
            format!(
                "POST {path}/git-receive-pack HTTP/1.1\r\nHost: relay\r\nContent-Type: application/x-git-receive-pack-request\r\nContent-Length: {len}\r\n\r\n{}",
                "0".repeat(len)
            )
        };
        let mut first = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        first.write_all(post(HELD).as_bytes()).await.unwrap();
        let held = tokio::time::timeout(Duration::from_secs(20), next_request(&upstream))
            .await
            .expect("the admitted body reaches upstream");
        // A small body: hyper drains it in one read, so the refusal is
        // answered on a cleanly closed connection.
        let response = raw(port, &post(4096)).await;
        assert!(response.starts_with("HTTP/1.1 507"), "{response}");
        drop(held);
        let mut ended = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(20), first.read_to_end(&mut ended)).await;
        drop(first);
        // The upstream failure ends the first exchange; its spool is dropped
        // as hyper lets go of the request body, which is asynchronous to the
        // reply, so the release is awaited with a bound.
        let mut admitted = false;
        for _ in 0..100 {
            let mut third = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap();
            third.write_all(post(4096).as_bytes()).await.unwrap();
            if tokio::time::timeout(Duration::from_millis(500), next_request(&upstream))
                .await
                .is_ok()
            {
                admitted = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(admitted, "the budget is released when the exchange ends");
    }

    #[tokio::test]
    async fn revoked_lease_abandons_an_exchange_mid_body_and_refuses_new_ones() {
        let (_temp, transport, member, upstream) = fixture().await;
        let (repo_id, git_url) = member.split_once('|').unwrap();
        let lease = transport.lease(repo_id, git_url);
        let path = lease.url().split_once("127.0.0.1").unwrap().1;
        let path = path[path.find('/').unwrap()..].to_owned();
        let port = transport.port();
        // Start a receive-pack upload but stop after part of the body.
        let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        socket
            .write_all(
                format!("POST {path}/git-receive-pack HTTP/1.1\r\nHost: relay\r\nContent-Type: application/x-git-receive-pack-request\r\nContent-Length: 12\r\n\r\n0000")
                    .as_bytes(),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(lease);
        // The remainder arrives after revocation.
        let _ = socket.write_all(b"00000000").await;
        let mut response = Vec::new();
        let _ =
            tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut response)).await;
        let response = String::from_utf8_lossy(&response);
        assert!(response.starts_with("HTTP/1.1 410"), "{response}");
        // Nothing reached the upstream, and the path is gone for good.
        assert!(
            tokio::time::timeout(Duration::from_millis(200), next_request(&upstream))
                .await
                .is_err(),
            "a revoked exchange must never reach the upstream"
        );
        let refused = raw(
            port,
            &format!(
                "GET {path}/info/refs?service=git-upload-pack HTTP/1.1\r\nHost: relay\r\n\r\n"
            ),
        )
        .await;
        assert!(refused.starts_with("HTTP/1.1 404"), "{refused}");
        // A live lease still refuses anything but the three smart-HTTP endpoints.
        let lease = transport.lease(repo_id, git_url);
        let path = lease.url().split_once("127.0.0.1").unwrap().1;
        let path = path[path.find('/').unwrap()..].to_owned();
        for bad in [
            format!("GET {path}/HEAD HTTP/1.1\r\nHost: relay\r\n\r\n"),
            format!("GET {path}/info/refs HTTP/1.1\r\nHost: relay\r\n\r\n"),
            format!("DELETE {path}/git-receive-pack HTTP/1.1\r\nHost: relay\r\n\r\n"),
            format!("GET {}/git/other/info/refs?service=git-upload-pack HTTP/1.1\r\nHost: relay\r\n\r\n", path.rsplit_once("/git/").unwrap().0),
        ] {
            let response = raw(port, &bad).await;
            assert!(response.starts_with("HTTP/1.1 404"), "{bad} -> {response}");
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(200), next_request(&upstream))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn only_completed_pushes_under_observed_leases_wake_observers() {
        let (_temp, transport, member, upstream) = fixture().await;
        let (repo_id, git_url) = member.split_once('|').unwrap();
        let status = Arc::new(std::sync::atomic::AtomicU16::new(200));
        tokio::spawn({
            let status = status.clone();
            async move {
                loop {
                    let mut socket = next_request(&upstream).await;
                    let mut head = vec![0u8; 4096];
                    let _ = socket.read(&mut head).await;
                    let code = status.load(std::sync::atomic::Ordering::SeqCst);
                    let _ = socket
                        .write_all(format!("HTTP/1.1 {code} Status\r\nContent-Type: application/x-git-receive-pack-result\r\nConnection: close\r\nContent-Length: 4\r\n\r\n0000").as_bytes())
                        .await;
                    let _ = socket.shutdown().await;
                }
            }
        });
        let pushes = transport.pushes();
        let exchange = |lease: &TransportLease, endpoint: &str| {
            let path = lease.url().split_once("127.0.0.1").unwrap().1;
            let path = path[path.find('/').unwrap()..].to_owned();
            let request = format!("POST {path}/{endpoint} HTTP/1.1\r\nHost: relay\r\nConnection: close\r\nContent-Length: 4\r\n\r\n0000");
            let port = transport.port();
            async move { raw(port, &request).await }
        };
        let woke = || {
            let pushes = pushes.clone();
            async move {
                tokio::time::timeout(Duration::from_millis(300), pushes.notified())
                    .await
                    .is_ok()
            }
        };
        let plain = transport.lease(repo_id, git_url);
        assert!(exchange(&plain, "git-receive-pack")
            .await
            .starts_with("HTTP/1.1 200"));
        assert!(
            !woke().await,
            "an unobserved lease's push is not a code push"
        );
        let observed = transport.observed_lease(repo_id, git_url);
        assert!(exchange(&observed, "git-upload-pack")
            .await
            .starts_with("HTTP/1.1 200"));
        assert!(!woke().await, "a fetch is not a push");
        status.store(403, std::sync::atomic::Ordering::SeqCst);
        assert!(exchange(&observed, "git-receive-pack")
            .await
            .starts_with("HTTP/1.1 403"));
        assert!(!woke().await, "a refused exchange is not a push");
        status.store(200, std::sync::atomic::Ordering::SeqCst);
        assert!(exchange(&observed, "git-receive-pack")
            .await
            .starts_with("HTTP/1.1 200"));
        assert!(woke().await, "a completed push wakes the observer");
        // Two pushes before the observer looks leave one wake-up, not two.
        exchange(&observed, "git-receive-pack").await;
        exchange(&observed, "git-receive-pack").await;
        assert!(woke().await);
        assert!(!woke().await);
    }

    #[tokio::test]
    async fn revoking_a_lease_ends_a_response_that_is_still_streaming() {
        let (_temp, transport, member, upstream) = fixture().await;
        let (repo_id, git_url) = member.split_once('|').unwrap();
        // An upstream that answers the headers and one chunk, then stalls.
        tokio::spawn(async move {
            let mut socket = next_request(&upstream).await;
            let mut head = vec![0u8; 4096];
            let _ = socket.read(&mut head).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-git-receive-pack-result\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n0000\r\n")
                .await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        let lease = transport.lease(repo_id, git_url);
        let path = lease.url().split_once("127.0.0.1").unwrap().1;
        let path = path[path.find('/').unwrap()..].to_owned();
        let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", transport.port()))
            .await
            .unwrap();
        socket
            .write_all(
                format!("POST {path}/git-receive-pack HTTP/1.1\r\nHost: relay\r\nContent-Type: application/x-git-receive-pack-request\r\nContent-Length: 4\r\n\r\n0000")
                    .as_bytes(),
            )
            .await
            .unwrap();
        // Headers and the first chunk arrive while the lease lives.
        let mut received = Vec::new();
        let mut chunk = vec![0u8; 4096];
        let first = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut chunk))
            .await
            .expect("relayed headers arrive")
            .unwrap();
        received.extend_from_slice(&chunk[..first]);
        assert!(
            String::from_utf8_lossy(&received).starts_with("HTTP/1.1 200"),
            "{received:?}"
        );
        // Revocation ends the relayed body: the client sees the exchange end
        // instead of a stream that outlives the operation.
        drop(lease);
        let ended = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        })
        .await;
        assert!(
            ended.is_ok(),
            "a revoked lease must end the relayed response"
        );
    }
}
