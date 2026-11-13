//! Browser-facing preview reverse proxy (PR4 proxy carrier 4b).
//!
//! `GET|<any> /api/v1/preview/:taskId/*path` relays a browser request to a
//! workspace preview task's HTTP listener over the host's outbound tunnel. The
//! route:
//!
//! 1. authenticates the normal browser session and authorizes the preview task
//!    through its owner-scoped workspace;
//! 2. resolves the task's workspace incarnation (VM and epoch) and configured
//!    port;
//! 3. opens a logical [`Channel::Http`](g5_internal_api::stream_mux::Channel)
//!    stream to that port through `WorkspaceRuntime::open_preview_stream`,
//!    which refuses a workspace now running another incarnation;
//! 4. relays the HTTP request/response bytes (streaming, framing-aware) or, for
//!    a WebSocket upgrade, performs a raw byte passthrough after the handshake.
//!
//! The path prefix `/api/v1/preview/:taskId` is stripped before forwarding, so
//! the preview app sees `/` + the wildcard remainder + the original query.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, Request, State},
    response::Response,
    routing::{any, post},
    Router,
};
use bytes::Bytes;
use http::{header, HeaderMap, HeaderValue, StatusCode};
use http_body_util::BodyExt as _;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use uuid::Uuid;

use g5_contracts::error::G5Error;
use g5_contracts::types::TaskId;
use g5_internal_api::stream_mux::MuxStream;

use crate::db::scoped::scoped_db_for_task;
use crate::db::workspace_agent_scope::{agent_owner_db, agent_target, AgentTargetNeed};
use crate::errors::AppError;
use crate::middleware::preview_host::{build_host_session_cookie, PREVIEW_HANDOFF_TICKET_PARAM};
use crate::middleware::{
    extract_session_user_id, AuthContext, PreviewAgentContext, PreviewHostOrigin,
    PREVIEW_REAUTH_MARKER_PARAM, PREVIEW_TICKET_COOKIE_HOST,
};
use crate::routes::audited::AuditedRouter;
use crate::routes::ResponseEnvelope;
use crate::services::auth_service::PREVIEW_TICKET_LIFETIME_SECS;
use crate::services::workspace_runtime::WorkspaceActivityReason;
use crate::services::AppState;
use g5_auth::Scope;

/// Upper bound on a buffered request body forwarded upstream (uploads). The
/// preview app receives exactly these bytes; larger bodies are rejected.
const MAX_REQUEST_BODY: usize = 64 * 1024 * 1024;

/// Upper bound on the upstream response *head* (status line + headers) we buffer
/// while looking for the `\r\n\r\n` terminator — a malformed/oversized head fails
/// closed instead of buffering unbounded.
const MAX_RESPONSE_HEAD: usize = 256 * 1024;

/// Time-to-first-byte ceiling on the upstream response head. The streaming body
/// and the WebSocket passthrough are intentionally unbounded *after* the head
/// (SSE, long-poll, and live sockets are legitimately long-lived); only the
/// initial head read is capped so a preview app that accepts the connection but
/// never responds cannot pin a tunnel stream — and the g5-api request task —
/// indefinitely. Per-host concurrency is separately bounded by the mux
/// (`MAX_LIVE_STREAMS`), so this only needs to bound a single stalled exchange.
const PREVIEW_RESPONSE_HEAD_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-exchange buffer of the duplex pipe bridging the mux stream to hyper.
/// Bounded per-exchange memory ([D19]); mux flow-control backpressure still
/// governs end-to-end throughput through the awaited pump `send`.
const RELAY_DUPLEX_BUF: usize = 64 * 1024;

/// Bridge a [`MuxStream`] to a tokio duplex IO that hyper's HTTP/1 client
/// connection can drive ([D19]).
///
/// The mux deliberately exposes async `send`/`recv` rather than
/// `AsyncRead`/`AsyncWrite` (flow-control window acquisition stays out of
/// manual `poll_write` — see `stream_mux`), so the bridge is the mux's own
/// documented usage shape: two explicit pump loops, exactly like the host
/// bridge. The outbound pump owns the `MuxWriter` and performs the exchange's
/// single idempotent `close()` when the hyper side reaches EOF/shutdown or
/// either pump ends — realizing the [D11] close-timing contract: never while
/// hyper still has request/response traffic in flight (hyper only shuts the
/// transport down after the upstream message completes or the body is
/// dropped), always once the exchange is over, so no stream registry entry is
/// stranded against the mux `MAX_LIVE_STREAMS` cap.
fn bridge_mux_stream(
    stream: MuxStream,
    activity: Option<crate::services::workspace_runtime::WorkspaceActivityGuard>,
) -> tokio::io::DuplexStream {
    let (hyper_side, pump_side) = tokio::io::duplex(RELAY_DUPLEX_BUF);
    let (writer, mut reader) = stream.into_split();
    let (mut pump_rd, mut pump_wr) = tokio::io::split(pump_side);

    let inbound_activity = activity.clone();
    // Inbound: mux → hyper reads. Upstream EOF (mux Close/teardown) becomes a
    // clean shutdown so hyper sees close-delimited response end.
    tokio::spawn(async move {
        while let Some(chunk) = reader.recv().await {
            if let Some(guard) = &inbound_activity {
                if !guard.authorize().await {
                    break;
                }
            }
            if pump_wr.write_all(&chunk).await.is_err() {
                break; // hyper side gone — exchange abandoned
            }
        }
        let _ = pump_wr.shutdown().await;
    });

    // Outbound: hyper writes → mux. Owns the writer; the close on exit is the
    // exchange's close-once ([D11]).
    tokio::spawn(async move {
        let mut buf = vec![0u8; RELAY_DUPLEX_BUF];
        loop {
            match pump_rd.read(&mut buf).await {
                Ok(0) => break, // hyper shutdown or dropped the connection
                Ok(n) => {
                    if let Some(guard) = &activity {
                        if !guard.authorize().await {
                            break;
                        }
                    }
                    if writer.send(&buf[..n]).await.is_err() {
                        break; // tunnel gone
                    }
                }
                Err(_) => break,
            }
        }
        writer.close().await;
    });

    hyper_side
}

/// Frame size for the buffered upstream request body. Mux-frame-sized so
/// hyper's bounded write buffer — not a single monolithic frame — is the only
/// skew between "body fully yielded" and "body flushed to the tunnel".
const REQUEST_BODY_FRAME: usize = 16 * 1024;

/// Buffered upstream request body that signals when it has been fully yielded
/// to hyper. The [D4] time-to-first-byte deadline is armed only on that
/// signal, so a large upload pushed through a backpressured tunnel never eats
/// the response window (parity with the pre-[D19] relay, whose timer started
/// after the body `send` completed). Frames are [`REQUEST_BODY_FRAME`]-sized:
/// hyper polls the next frame only as its bounded write buffer drains, so
/// when the signal fires at most one write-buffer of bytes (≤ `max_buf_size`)
/// is still in flight under the deadline.
struct SignalOnEndBody {
    data: Bytes,
    sent: Option<tokio::sync::oneshot::Sender<()>>,
    len: u64,
}

impl SignalOnEndBody {
    fn new(data: Bytes, sent: tokio::sync::oneshot::Sender<()>) -> Self {
        let len = data.len() as u64;
        Self {
            data,
            sent: Some(sent),
            len,
        }
    }
}

impl http_body::Body for SignalOnEndBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
        if !self.data.is_empty() {
            let take = self.data.len().min(REQUEST_BODY_FRAME);
            let frame = self.data.split_to(take);
            return std::task::Poll::Ready(Some(Ok(http_body::Frame::data(frame))));
        }
        if let Some(sent) = self.sent.take() {
            let _ = sent.send(());
        }
        std::task::Poll::Ready(None)
    }

    fn is_end_stream(&self) -> bool {
        self.data.is_empty() && self.sent.is_none()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::with_exact(self.len)
    }
}

/// Map a hyper client error on the upstream exchange to the relay's
/// operator-facing error classes ([D19]: these strings are keyed on by ops
/// triage ladders and must not drift).
fn map_upstream_error(e: &hyper::Error) -> G5Error {
    if e.is_incomplete_message() {
        G5Error::ServiceUnavailable {
            message: "preview upstream closed before sending a response".into(),
        }
    } else if e.is_parse_too_large() {
        G5Error::Internal {
            message: "preview upstream response head exceeded limit".into(),
        }
    } else if e.is_parse() {
        G5Error::Internal {
            message: format!("parse preview upstream response: {e}"),
        }
    } else {
        G5Error::ServiceUnavailable {
            message: format!("preview upstream exchange failed: {e}"),
        }
    }
}

/// Send the (fully buffered) upstream request and await the response head
/// under the [D4] deadline: phase 1 waits — un-deadlined, backpressure-bound —
/// until the request body has been fully yielded to hyper (or the response
/// arrives first), then phase 2 bounds the remaining time-to-first-byte by
/// [`PREVIEW_RESPONSE_HEAD_TIMEOUT`], mapping an elapsed deadline to the same
/// 503 the pre-[D19] relay produced.
async fn send_upstream_request(
    sender: &mut hyper::client::conn::http1::SendRequest<SignalOnEndBody>,
    req: http::Request<SignalOnEndBody>,
    body_sent: tokio::sync::oneshot::Receiver<()>,
) -> Result<http::Response<hyper::body::Incoming>, AppError> {
    let resp_fut = sender.send_request(req);
    tokio::pin!(resp_fut);
    let result = tokio::select! {
        r = &mut resp_fut => r,
        _ = body_sent => {
            match tokio::time::timeout(PREVIEW_RESPONSE_HEAD_TIMEOUT, &mut resp_fut).await {
                Ok(r) => r,
                Err(_) => {
                    return Err(AppError::from_error(G5Error::ServiceUnavailable {
                        message: "preview upstream did not start responding in time".into(),
                    }))
                }
            }
        }
    };
    result.map_err(|e| AppError::from_error(map_upstream_error(&e)))
}

/// Open a hyper HTTP/1 client connection over the bridged mux stream ([D19])
/// and spawn its connection task (upgrades enabled; errors observed and
/// logged, never silently discarded).
async fn handshake_upstream<B>(
    stream: MuxStream,
    activity: Option<crate::services::workspace_runtime::WorkspaceActivityGuard>,
) -> Result<hyper::client::conn::http1::SendRequest<B>, AppError>
where
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let io = TokioIo::new(bridge_mux_stream(stream, activity));
    let (sender, conn) = hyper::client::conn::http1::Builder::new()
        .max_buf_size(MAX_RESPONSE_HEAD)
        .handshake::<_, B>(io)
        .await
        .map_err(|e| AppError::from_error(map_upstream_error(&e)))?;
    tokio::spawn(async move {
        if let Err(e) = conn.with_upgrades().await {
            tracing::warn!(error = %e, "preview relay upstream connection ended with error");
        }
    });
    Ok(sender)
}

/// Routes for the preview proxy. Mounted at `/api/v1/preview` by `build_router`.
pub fn router() -> Router<Arc<AppState>> {
    // `any(...)` is intentional: the proxy fronts the customer's full web app, so
    // every HTTP method (and WS upgrade) must pass through. Auth + scope are
    // enforced by `audited_route`'s `RequireScopes(Scope::TasksRead)` — the same
    // read scope as `GET /tasks/:id` — and the proxy additionally re-resolves the
    // target on the caller's org-scoped RLS pool (cross-tenant safe). The scope
    // admits an organization API key to RELAY; every browser-credential
    // producer behind these routes additionally passes
    // `preview_session_identity`, so a key never receives a handoff or ticket.
    AuditedRouter::new()
        // Mint endpoint deliberately OUTSIDE the `/:taskId/` subtree, which is
        // reserved in full (every method, every path) for the previewed app —
        // a mint route at `/:workspaceId/session` would shadow the app's own
        // `/session` path. The literal `session` segment can never collide
        // with a proxy target because `taskId` is a typed UUID.
        .audited_route(
            "/session/:taskId",
            post(preview_session_mint),
            &[Scope::TasksRead],
        )
        .audited_route("/:taskId", any(preview_proxy_root), &[Scope::TasksRead])
        .audited_route("/:taskId/", any(preview_proxy_root), &[Scope::TasksRead])
        .audited_route(
            "/:taskId/*path",
            any(preview_proxy_path),
            &[Scope::TasksRead],
        )
        .into_router()
}

/// Mint response body: the canonical URL the SPA should open in a new tab
/// (`routes/preview` [I2]/[D15]).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewSessionMintDto {
    preview_url: String,
}

/// The one issuance boundary for preview browser credentials
/// (`g5.workspace.preview-origin-security`, issue G5Labs/G5-Lean#661): the
/// caller's person-session identity, taken through the shared
/// [`extract_session_user_id`] guard so an organization API key — whatever
/// its scopes — is refused with `Forbidden { user_session_required }` before
/// a handoff or ticket exists. Browser tickets are a person's credential
/// (`g5.security.credential-classes`); a key that could mint one would
/// outlive its own revocation. The `__Host-g5_preview` ticket cookie carries
/// the person's own session identity and passes, so relay renewal keeps
/// working; hosts carry no user identity and stay `Unauthorized`.
///
/// All three producers — the explicit mint, the platform-path redirect and
/// the sliding renewal — call this and nothing else for their `sub`/`org`.
fn preview_session_identity(auth: &AuthContext) -> Result<(Uuid, Uuid), AppError> {
    let user_id = extract_session_user_id(auth)?;
    let org_id = auth
        .org_id()
        .and_then(|value| value.parse::<Uuid>().ok())
        .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;
    Ok((user_id, org_id))
}

/// Resolve the task-owned preview path. The task is owner-only even for reads:
/// `scoped_db_for_task` applies
/// the current workspace-owner gate before any live target or readiness fact
/// is returned.
async fn resolve_workspace_preview_target(
    state: &Arc<AppState>,
    auth: &AuthContext,
    task_id: Uuid,
    require_ready: bool,
) -> Result<crate::services::task_lifecycle::WorkspacePreviewTarget, AppError> {
    let mut db = scoped_db_for_task(state, auth, task_id, &["workspaces:write"]).await?;
    let target = state
        .task_service
        .workspace_preview_target(&mut db, task_id, require_ready)
        .await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(target)
}

/// Recheck a workspace agent's preview credential (`g5.preview.agent-access`)
/// for one request, in one owner transaction: the minting incarnation is
/// still current, the target is still its own workspace or a direct child
/// under agent scope (the other-workspaces setting included), and the task is
/// still the target's ready current preview.
async fn resolve_agent_preview_target(
    state: &Arc<AppState>,
    agent: &g5_auth::PreviewAgentClaims,
    task_id: Uuid,
) -> Result<crate::services::task_lifecycle::WorkspacePreviewTarget, AppError> {
    if agent.task_id != task_id {
        return Err(AppError::from_error(G5Error::Unauthorized));
    }
    let caller = agent.caller_incarnation();
    let mut db = agent_owner_db(state, &caller, &["workspaces:write"]).await?;
    agent_target(
        &mut db,
        &caller,
        agent.target_workspace_id,
        AgentTargetNeed::SelfOrOther,
    )
    .await?;
    let target = state
        .task_service
        .workspace_preview_target(&mut db, task_id, true)
        .await?;
    if target.workspace_id != agent.target_workspace_id {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: Vec::new(),
        }));
    }
    db.commit().await.map_err(AppError::from)?;
    Ok(target)
}

/// `POST /api/v1/preview/session/:taskId` — browser cookie handoff (issue
/// G5Labs/G5#736; `routes/preview` [I2]/[D8]/[D15]).
///
/// Called with the normal Bearer header by the SPA before it opens the
/// preview in a new tab (a top-level navigation cannot carry the
/// Authorization header). Requires the person's own session
/// ([`preview_session_identity`]; an API key is refused before anything is
/// minted), re-authorizes against the workspace's org/workspace exactly like
/// the proxy, and returns `200` with `{ previewUrl }`: the
/// per-preview host handoff URL under `G5_PREVIEW_HOST_SUFFIX` (issue
/// G5Labs/G5#935). Previews are served only from isolated preview hosts
/// (`g5.security.mvp-exposure.preview`): without a suffix the platform keeps
/// running and this endpoint answers 503 `PREVIEW_ORIGIN_UNCONFIGURED`. No
/// platform-origin preview cookie exists; the handoff exchange sets the
/// origin-scoped `__Host-g5_preview` cookie on the preview host.
async fn preview_session_mint(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
) -> Result<Response, AppError> {
    let task_id = task_id.into_uuid();
    // Only a person's own session may mint a browser ticket; the ticket
    // carries the caller's identity so downstream audit/RLS stay real.
    let (user_id, org_id) = preview_session_identity(&auth)?;

    resolve_workspace_preview_target(&state, &auth, task_id, true).await?;

    // The canonical browser entry point ([D15]): the preview host gets a
    // fresh single-use handoff ticket. No fallback serves preview content on
    // the platform origin.
    let Some(suffix) = state.preview_host_suffix.as_deref() else {
        return Err(AppError::from_error(G5Error::PreviewOriginUnconfigured));
    };
    let handoff = state
        .auth_service
        .mint_preview_handoff_ticket(user_id, org_id, task_id)?;
    let preview_url =
        format!("https://{task_id}.{suffix}/?{PREVIEW_HANDOFF_TICKET_PARAM}={handoff}");

    let body = serde_json::to_vec(&ResponseEnvelope::ok(PreviewSessionMintDto { preview_url }))
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("serialize preview session response: {e}"),
            })
        })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("build preview session response: {e}"),
            })
        })
}

async fn preview_proxy_root(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(task_id): Path<TaskId>,
    req: Request,
) -> Result<Response, AppError> {
    proxy(state, auth, task_id.into_uuid(), String::new(), req).await
}

async fn preview_proxy_path(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path((task_id, rest)): Path<(TaskId, String)>,
    req: Request,
) -> Result<Response, AppError> {
    proxy(state, auth, task_id.into_uuid(), rest, req).await
}

async fn proxy(
    state: Arc<AppState>,
    auth: AuthContext,
    workspace_id: Uuid,
    rest_path: String,
    req: Request,
) -> Result<Response, AppError> {
    // A workspace agent's preview credential (`g5.preview.agent-access`):
    // only on the isolated origin, never for a WebSocket, and rechecked
    // against the agent's scope rather than admitted as the owner's session.
    let agent = req
        .extensions()
        .get::<PreviewAgentContext>()
        .map(|context| context.0.clone());
    let agent_credential = auth.claims().is_some_and(|claims| {
        claims.token_use.as_deref() == Some(g5_auth::PREVIEW_AGENT_TOKEN_USE)
    });
    if agent_credential != agent.is_some()
        || (agent.is_some() && req.extensions().get::<PreviewHostOrigin>().is_none())
    {
        return Err(AppError::from_error(G5Error::Unauthorized));
    }
    if agent.is_some() && is_websocket_upgrade(req.headers()) {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from(
                "Workspace agent preview credentials do not open WebSocket connections",
            ))
            .map_err(|e| {
                AppError::from_error(G5Error::Internal {
                    message: format!("build preview agent WebSocket rejection: {e}"),
                })
            });
    }
    let workspace_target = match &agent {
        Some(agent) => resolve_agent_preview_target(&state, agent, workspace_id).await?,
        None => resolve_workspace_preview_target(&state, &auth, workspace_id, true).await?,
    };

    // Platform-path requests ([D14], issue G5Labs/G5#935): a request that did
    // NOT arrive via host translation never relays on the platform origin
    // (`g5.security.mvp-exposure.preview`). With a suffix configured, an
    // authenticated GET/HEAD moves to the per-preview host with a fresh
    // single-use handoff ticket and every other method is refused; without a
    // suffix previews are unavailable.
    let Some(origin) = req.extensions().get::<PreviewHostOrigin>().cloned() else {
        let Some(suffix) = state.preview_host_suffix.as_deref() else {
            return Err(AppError::from_error(G5Error::PreviewOriginUnconfigured));
        };
        let method = req.method();
        if method == http::Method::GET || method == http::Method::HEAD {
            return platform_path_redirect(&state, &auth, workspace_id, suffix, &rest_path, &req);
        }
        return Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header(header::ALLOW, "GET, HEAD")
            .body(Body::from("Use the isolated preview URL"))
            .map_err(|e| {
                AppError::from_error(G5Error::Internal {
                    message: format!("build preview method rejection: {e}"),
                })
            });
    };

    // WebSocket APIs expose responses to cross-origin scripts without CORS.
    // A sibling preview is same-site, so the browser attaches this host's
    // preview and app cookies. Require the target preview's own Origin before
    // opening an upstream stream; browsers always send one on WS handshakes.
    if is_websocket_upgrade(req.headers())
        && !same_isolated_websocket_origin(req.headers(), &origin)
    {
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from(
                "Cross-origin preview WebSocket requests are forbidden",
            ))
            .map_err(|e| {
                AppError::from_error(G5Error::Internal {
                    message: format!("build preview WebSocket origin rejection: {e}"),
                })
            });
    }

    let user_id = auth
        .user_id()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;
    let correlation_id = req
        .extensions()
        .get::<crate::middleware::request_id::RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (stream, activity) = state
        .workspace_runtime
        .open_preview_stream(
            workspace_target.workspace_id,
            workspace_target.vm_id,
            workspace_target.epoch,
            workspace_target.port,
            user_id,
        )
        .await
        .map_err(AppError::from_error)?;
    // Preview traffic intentionally renews activity, including automatic HTTP
    // requests and new WebSocket connections. Admission above and the fenced,
    // throttled write retain current owner/incarnation authorization. Existing
    // connection traffic is authorized separately and never re-recorded.
    activity
        .record(WorkspaceActivityReason::PreviewTraffic, &correlation_id)
        .await;
    let session_expires_at =
        Some(chrono::Utc::now() + chrono::Duration::seconds(PREVIEW_TICKET_LIFETIME_SECS));

    // 5. Relay. Decide the Authorization disposition once ([D9]): platform
    // credentials are always stripped; the previewed app's own Authorization
    // passes through. Fails closed to strip when no validator is configured.
    //
    // Sliding renewal ([D17]): the decision is computed here against the
    // resolved session's expiry, but the fresh ticket is minted only after
    // relay_http returned a usable upstream response. The WebSocket branch
    // never renews (browsers do not reliably process Set-Cookie on 101), and
    // the [D14] redirect returned above before this point.
    let renew_ticket = should_renew_preview_ticket(&auth, workspace_id, session_expires_at);
    let (mut parts, body) = req.into_parts();
    let strip_authorization =
        should_strip_authorization(state.jwt_validator.as_deref(), &parts.headers);
    // [D20] `Origin: null` drop: preserved by default; dropped only for a
    // ticket-authenticated, fetch-metadata-attested same-ORIGIN navigation —
    // the [D47] sandboxed cross-origin embed nulls WebKit form-POST Origins.
    // Never on WebSocket handshakes ([D5]) — the WS branch below passes
    // `false` unconditionally.
    let drop_null_origin = is_trusted_embed_navigation(&auth, workspace_id, &parts.headers);
    // [D16] anti-framing normalization target: the workspace origin from
    // startup-validated config (`server.external_url`, the same
    // canonicalization the [D18] reauth middleware uses — never derived from
    // `Host` or forwarded headers). Startup refuses a non-canonical value, so
    // a failure here is a programming error, not a mode.
    let frame_ancestors_allow = crate::middleware::canonical_main_origin(&state.external_url)
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("canonicalize workspace origin for preview anti-framing: {e}"),
            })
        })?;
    let policy = RelayPolicy {
        strip_authorization,
        origin: &origin,
        drop_null_origin,
        frame_ancestors_allow: &frame_ancestors_allow,
    };
    if is_websocket_upgrade(&parts.headers) {
        relay_websocket(&mut parts, stream, Some(activity), &rest_path, policy).await
    } else {
        let mut response =
            relay_http(parts, body, stream, Some(activity), &rest_path, policy).await?;
        if renew_ticket {
            attach_renewed_preview_ticket(&state, &auth, workspace_id, &mut response);
        }
        Ok(response)
    }
}

/// [D18] `return` sanitizer (input is already percent-decoded): a local
/// return path by the shared check (`crate::auth::return_path`: exactly one
/// leading `/`, no backslash anywhere, no control characters, ≤ 2048 chars)
/// with no `#` fragment; reserved `__g5_preview_ticket` /
/// `__g5_reauth` pairs are stripped from its query. Anything else degrades
/// to `/` — the final Location is always composed from server config plus
/// workspaceId plus this path, so an open redirect is structurally impossible.
pub(super) fn sanitize_reauth_return(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "/".to_string();
    };
    if !crate::auth::return_path::is_local_return_path(raw) || raw.contains('#') {
        return "/".to_string();
    }
    // Strip reserved parameters from the embedded query, preserving the rest.
    match raw.split_once('?') {
        None => raw.to_string(),
        Some((path, query)) => {
            let kept: Vec<&str> = query
                .split('&')
                .filter(|pair| {
                    let name = pair.split_once('=').map_or(*pair, |(n, _)| n);
                    name != PREVIEW_HANDOFF_TICKET_PARAM && name != PREVIEW_REAUTH_MARKER_PARAM
                })
                .filter(|p| !p.is_empty())
                .collect();
            if kept.is_empty() {
                path.to_string()
            } else {
                format!("{path}?{}", kept.join("&"))
            }
        }
    }
}

/// Browser-facing no-store HTML 401 for every [D18] failure path — never a
/// redirect (loop safety), never the JSON envelope (a navigation is the
/// consumer).
pub(super) fn reauth_unauthorized() -> Response {
    let body = "<!doctype html><html><head><title>Preview unavailable</title></head>\
                <body><p>Preview session expired. Reopen the preview from G5.</p>\
                </body></html>";
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Renewal window for the [D17] sliding session ticket: a cookie-validated
/// request renews only when its ticket is PAST HALF-LIFE but not yet expired
/// (`0 < exp - now <= 900`). Renewing on every request would mint needlessly;
/// the half-life threshold caps minting at ~2 tickets per 30-minute window
/// under continuous traffic.
const PREVIEW_TICKET_RENEWAL_WINDOW_SECS: i64 = PREVIEW_TICKET_LIFETIME_SECS / 2;

/// [D20] trusted-embed gate for the `Origin: null` drop. TRUE only when the
/// request (a) authenticated via THIS workspace's preview ticket (same
/// `token_use`/`aud` predicate as [D17] renewal; that credential is the
/// [D15] `__Host-g5_preview` session cookie — platform-Bearer and
/// programmatic callers fail here and keep their header), and (b) carries
/// browser fetch metadata attesting a same-ORIGIN HTML navigation:
/// `Sec-Fetch-Site: same-origin` AND `Sec-Fetch-Mode: navigate` AND
/// `Sec-Fetch-Dest: document|iframe`. Fetch metadata are forbidden headers
/// page script cannot forge, and `same-origin` is strictly stronger than
/// same-site — a sibling-preview form POST from `<workspaceA>.<suffix>` to
/// `<workspaceB>.<suffix>` arrives as `same-site` and fails the gate, preserving
/// the target app's opaque-origin CSRF signal; only a document already on
/// the TARGET preview's own origin can attest `same-origin`. Requests
/// without fetch metadata fail closed to preserving the header.
fn is_trusted_embed_navigation(
    auth: &AuthContext,
    workspace_id: Uuid,
    headers: &HeaderMap,
) -> bool {
    let Some(claims) = auth.claims() else {
        return false;
    };
    if claims.token_use.as_deref() != Some(g5_auth::PREVIEW_TOKEN_USE) {
        return false;
    }
    let audience = g5_auth::preview_ticket_audience(workspace_id);
    if claims.aud.as_deref() != Some(audience.as_str()) {
        return false;
    }
    let fetch_meta = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_ascii_lowercase())
    };
    fetch_meta("sec-fetch-site").as_deref() == Some("same-origin")
        && fetch_meta("sec-fetch-mode").as_deref() == Some("navigate")
        && matches!(
            fetch_meta("sec-fetch-dest").as_deref(),
            Some("document") | Some("iframe")
        )
}

/// The [D17] renewal decision, evaluated per relayed HTTP request.
///
/// All four conditions must hold:
/// 1. the request authenticated via the preview ticket cookie — the
///    carve-out's claims carry `token_use == "preview"` and the workspace-bound
///    audience (a platform-Bearer fall-through has no cookie to renew);
/// 2. the ticket is inside its renewal window (past half-life, not expired);
/// 3. the resolved session is unexpired (`now < expires_at` — serving stays
///    reaper-governed, only renewal is expiry-gated);
/// 4. the session row has a non-NULL `expires_at` — pre-TTL rows keep the
///    status-quo single-ticket behavior, because renewable possession with
///    no reaper backstop would be unbounded.
fn should_renew_preview_ticket(
    auth: &AuthContext,
    workspace_id: Uuid,
    session_expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    let Some(expires_at) = session_expires_at else {
        return false;
    };
    let Some(claims) = auth.claims() else {
        return false;
    };
    if claims.token_use.as_deref() != Some(g5_auth::PREVIEW_TOKEN_USE) {
        return false;
    }
    let audience = g5_auth::preview_ticket_audience(workspace_id);
    if claims.aud.as_deref() != Some(audience.as_str()) {
        return false;
    }
    let now = chrono::Utc::now();
    if now >= expires_at {
        return false;
    }
    let remaining = claims.exp - now.timestamp();
    remaining > 0 && remaining <= PREVIEW_TICKET_RENEWAL_WINDOW_SECS
}

/// [D17] renewal emission: mint a fresh session ticket for the SAME
/// `sub`/`org`/workspace and APPEND the host-scoped `Set-Cookie` after relay
/// sanitization has removed unsafe upstream cookies. `Cache-Control` is
/// forced to `private, no-store` so no intermediary ever caches a
/// credential-bearing response. The identity comes from the shared
/// [`preview_session_identity`] boundary: a renewed ticket is a fresh browser
/// credential, so a non-session identity is served without one. Renewal must
/// never fail a working request: every failure path logs and leaves the
/// response unchanged.
fn attach_renewed_preview_ticket(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    response: &mut Response,
) {
    let (user_id, org_id) = match preview_session_identity(auth) {
        Ok(identity) => identity,
        Err(e) => {
            tracing::warn!(
                %workspace_id,
                error = ?e,
                "preview ticket renewal refused for a non-session identity; serving without renewal"
            );
            return;
        }
    };
    let ticket = match state
        .auth_service
        .mint_preview_ticket(user_id, org_id, workspace_id)
    {
        Ok(ticket) => ticket,
        Err(e) => {
            tracing::warn!(
                %workspace_id,
                error = ?e,
                "preview ticket renewal mint failed; serving without renewal"
            );
            return;
        }
    };
    let cookie = build_host_session_cookie(&ticket);
    match HeaderValue::from_str(&cookie) {
        Ok(value) => {
            response.headers_mut().append(header::SET_COOKIE, value);
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
        }
        Err(e) => {
            tracing::warn!(
                %workspace_id,
                error = ?e,
                "preview renewal cookie not header-safe; serving without renewal"
            );
        }
    }
}

/// `302` from the platform path-prefix URL to the per-preview host ([D14]):
/// same path and query, plus a fresh single-use handoff ticket minted from
/// the already-authorized caller's person-session identity ([D15],
/// [`preview_session_identity`]). The caller has passed the owner and
/// readiness checks by now; an API key is refused here, before the handoff
/// is minted, with no `Location`.
fn platform_path_redirect(
    state: &Arc<AppState>,
    auth: &AuthContext,
    workspace_id: Uuid,
    suffix: &str,
    rest_path: &str,
    req: &Request,
) -> Result<Response, AppError> {
    let (user_id, org_id) = preview_session_identity(auth)?;
    let handoff = state
        .auth_service
        .mint_preview_handoff_ticket(user_id, org_id, workspace_id)?;
    let location =
        subdomain_redirect_location(workspace_id, suffix, rest_path, req.uri().query(), &handoff);
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("build preview redirect: {e}"),
            })
        })
}

/// Builds the subdomain redirect target: `https://<workspace>.<suffix>/<rest>`
/// with the original query preserved and the handoff ticket appended.
fn subdomain_redirect_location(
    workspace_id: Uuid,
    suffix: &str,
    rest_path: &str,
    query: Option<&str>,
    handoff_ticket: &str,
) -> String {
    let mut location = format!(
        "https://{workspace_id}.{suffix}/{}",
        rest_path.trim_start_matches('/')
    );
    location.push('?');
    if let Some(q) = query {
        if !q.is_empty() {
            location.push_str(q);
            location.push('&');
        }
    }
    location.push_str(PREVIEW_HANDOFF_TICKET_PARAM);
    location.push('=');
    location.push_str(handoff_ticket);
    location
}

// ---------------------------------------------------------------------------
// HTTP relay (request → upstream, streaming framing-aware response back)
// ---------------------------------------------------------------------------

/// Relay policy resolved once per request from the preview host + auth, then
/// handed to whichever relay path runs. Bundled into one value so
/// `relay_http`/`relay_websocket` stay within the argument budget (clippy
/// `too_many_arguments`, PR #1283 review P1) and the coupled inputs travel
/// together instead of as parallel loose booleans:
/// - `strip_authorization`: drop the app's platform `Authorization` ([D9]).
/// - `origin`: the per-preview host the request arrived on ([D13]); every
///   relayed request has one because the platform path never relays.
/// - `drop_null_origin`: the [D20] trusted-embed `Origin: null` drop gate is
///   open. Honored by the HTTP relay only; the WS handshake `Origin` is never
///   dropped (it passed the same-origin gate in `proxy`). `relay_websocket`
///   ignores this field and passes `false` unconditionally.
/// - `frame_ancestors_allow`: the [D16] workspace origin added to
///   `frame-ancestors`.
struct RelayPolicy<'a> {
    strip_authorization: bool,
    origin: &'a PreviewHostOrigin,
    drop_null_origin: bool,
    frame_ancestors_allow: &'a str,
}

async fn relay_http(
    parts: http::request::Parts,
    body: Body,
    stream: MuxStream,
    activity: Option<crate::services::workspace_runtime::WorkspaceActivityGuard>,
    rest_path: &str,
    policy: RelayPolicy<'_>,
) -> Result<Response, AppError> {
    // Buffer the request body (bounded) so the upstream request carries an
    // exact Content-Length ([D3]).
    let body_bytes = axum::body::to_bytes(body, MAX_REQUEST_BODY)
        .await
        .map_err(|_| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({ "body": "preview request body too large" }),
            })
        })?;

    // The exchange is driven by hyper's client connection over the bridged mux
    // stream ([D19]). Send-half close timing — after the exchange completes
    // rather than interrupting a response ([D11], registry cap) — is
    // realized by hyper's shutdown discipline plus the bridge's close-once.
    let mut sender = handshake_upstream::<SignalOnEndBody>(stream, activity).await?;
    let (body_sent_tx, body_sent_rx) = tokio::sync::oneshot::channel();
    let req = build_upstream_request(
        &parts,
        rest_path,
        true,
        policy.strip_authorization,
        policy.origin,
        policy.drop_null_origin,
    )
    .body(SignalOnEndBody::new(body_bytes, body_sent_tx))
    .map_err(|e| {
        AppError::from_error(G5Error::Internal {
            message: format!("build preview upstream request: {e}"),
        })
    })?;
    let response = send_upstream_request(&mut sender, req, body_sent_rx).await?;

    // Sanitize the response head ([D3]/[D16]) and stream the hyper-decoded
    // body straight through (never fully buffered; poll-after-EOF safe by
    // construction — [D19]).
    let (mut resp_parts, incoming) = response.into_parts();
    sanitize_relay_response_headers(&mut resp_parts.headers, false, policy.frame_ancestors_allow);
    let mut builder = Response::builder().status(resp_parts.status);
    if let Some(h) = builder.headers_mut() {
        *h = resp_parts.headers;
    }
    builder
        .body(Body::new(incoming.map_err(axum::Error::new)))
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("build preview response: {e}"),
            })
        })
}

/// Whether the request's `authorization` header(s) must be stripped before
/// the relay forwards them (routes/preview [D9]): platform credentials never
/// reach the previewed app; the app's OWN credentials pass through so header-
/// authenticated apps can log in. Bearer tokens are classified by
/// `JwtValidator::is_platform_credential` (platform JWTs and `g5k_` API keys);
/// non-Bearer schemes (Basic, an app's own API keys) are never platform
/// credentials.
///
/// The decision is all-or-nothing across EVERY `authorization` value because
/// `build_upstream_request` forwards or drops the header by name — HTTP permits
/// multiple `authorization` fields, so classifying only the first value would
/// let `Basic <app>` + `Bearer <platform>` forward the platform token. Strips
/// (all values) when ANY value is a platform credential, and fails closed to
/// strip on a non-UTF8 value or when no platform validator is configured; only
/// when every value is confidently app-owned are they all forwarded.
fn should_strip_authorization(
    validator: Option<&g5_auth::JwtValidator>,
    headers: &HeaderMap,
) -> bool {
    for value in headers.get_all(header::AUTHORIZATION) {
        let Ok(s) = value.to_str() else {
            return true; // non-UTF8 → fail closed
        };
        let mut parts = s.trim().splitn(2, ' ');
        let scheme = parts.next().unwrap_or("");
        let token = parts.next().unwrap_or("").trim();
        if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
            continue; // not a platform-capable value; check the rest
        }
        let is_platform = match validator {
            Some(v) => v.is_platform_credential(token),
            None => true, // no validator → fail closed
        };
        if is_platform {
            return true;
        }
    }
    false
}

/// Compose the typed upstream request: rewritten origin-form target and the
/// forwarded header set. The caller attaches the body; hyper derives the
/// framing (`Content-Length` from the buffered body — [D3]).
///
/// The forwarded `Host` is rewritten to `localhost` (`routes/preview` [D13])
/// — default dev servers (Vite ≥ 5.4.12/6 `server.allowedHosts`) reject
/// unrecognized hosts, and bare `localhost` is the port-forwarding shape
/// they all accept — with `X-Forwarded-Host`/`X-Forwarded-Proto` carrying
/// the real preview origin (proxy-owned: client-supplied values are dropped).
fn build_upstream_request(
    parts: &http::request::Parts,
    rest_path: &str,
    force_close: bool,
    strip_authorization: bool,
    origin: &PreviewHostOrigin,
    drop_null_origin: bool,
) -> http::request::Builder {
    let mut target = String::from("/");
    target.push_str(rest_path.trim_start_matches('/'));
    if let Some(q) = parts.uri.query() {
        // [D18]: the re-auth loop-guard marker is platform plumbing — strip
        // it before forwarding so the previewed app never sees it. All other
        // pairs (including valueless flags) pass through verbatim.
        let kept: Vec<&str> = q
            .split('&')
            .filter(|pair| {
                let name = pair.split_once('=').map_or(*pair, |(n, _)| n);
                name != PREVIEW_REAUTH_MARKER_PARAM
            })
            .filter(|p| !p.is_empty())
            .collect();
        if !kept.is_empty() {
            target.push('?');
            target.push_str(&kept.join("&"));
        }
    }

    // Headers the client's own `Connection` nominates are hop-by-hop and are
    // dropped alongside it on the force-close path ([D3]); the WS handshake
    // (force_close = false) forwards its Connection/Upgrade pair intact.
    let nominated: Vec<String> = if force_close {
        parts
            .headers
            .get_all(header::CONNECTION)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    let mut builder = http::Request::builder()
        .method(parts.method.clone())
        .uri(&target);
    let Some(headers) = builder.headers_mut() else {
        return builder; // unreachable: method/uri are valid; error surfaces at .body()
    };
    for (name, value) in parts.headers.iter() {
        let lname = name.as_str();
        // We re-issue Connection ourselves when forcing close; otherwise pass
        // the client's headers (including Upgrade/Sec-WebSocket-* for WS) intact.
        if force_close && (lname == "connection" || nominated.iter().any(|t| t == lname)) {
            continue;
        }
        // The relay owns the upstream framing: the buffered body carries an
        // exact Content-Length (or no body on the WS handshake), so the
        // client's own framing headers must never ride along — forwarding a
        // stale `Transfer-Encoding: chunked` next to an already-decoded body
        // would desynchronize the upstream parser ([D3]).
        if matches!(
            lname,
            "transfer-encoding" | "content-length" | "trailer" | "expect"
        ) {
            continue;
        }
        // [D13]: Host is rewritten below, and the X-Forwarded-* assertions
        // are proxy-owned — never client-supplied.
        if matches!(lname, "host" | "x-forwarded-host" | "x-forwarded-proto") {
            continue;
        }
        // [D20]: a literal `Origin: null` is a browser ambiguity marker
        // (sandboxed or referrer-suppressed frames — the embedded Preview
        // Browser in WebKit), not an identity;
        // previewed apps' CORS filters reject it on sign-in POSTs.
        // Preserved by default — the caller sets `drop_null_origin` only
        // for a ticket-authenticated, fetch-metadata-attested same-ORIGIN
        // navigation (`is_trusted_embed_navigation`). Same-SITE senders (a
        // sibling preview's cross-subdomain form POST), Bearer/programmatic
        // callers, and WebSocket handshakes keep the marker the target app
        // rejects forgeries with.
        if drop_null_origin && lname == "origin" && value.as_bytes() == b"null" {
            continue;
        }
        // Never forward PLATFORM credentials to the previewed customer app
        // (routes/preview [D9]): the caller decides via
        // `should_strip_authorization` whether this Authorization value is a
        // platform credential (always stripped) or the app's own login
        // header (forwarded); the `__Host-g5_preview` ticket is filtered out
        // of the Cookie header unconditionally while the app's own cookies
        // pass.
        if lname == "authorization" && strip_authorization {
            continue;
        }
        // A workspace agent's preview credential is a platform credential
        // whatever authenticated the request (`g5.preview.agent-access`).
        if lname == g5_auth::PREVIEW_AGENT_HEADER {
            continue;
        }
        if lname == "cookie" {
            if let Ok(v) = std::str::from_utf8(value.as_bytes()) {
                let filtered = strip_preview_ticket_cookie(v);
                if !filtered.is_empty() {
                    if let Ok(fv) = HeaderValue::from_str(&filtered) {
                        headers.append(header::COOKIE, fv);
                    }
                }
            }
            continue;
        }
        // Parity with the pre-[D19] relay: non-UTF-8 header values were
        // dropped by the string-composed head and stay dropped.
        if value.to_str().is_ok() {
            headers.append(name.clone(), value.clone());
        }
    }
    headers.insert(header::HOST, HeaderValue::from_static("localhost"));
    if let Ok(v) = HeaderValue::from_str(&origin.host) {
        headers.insert("x-forwarded-host", v);
    }
    headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    if force_close {
        headers.insert(header::CONNECTION, HeaderValue::from_static("close"));
    }
    builder
}

/// Sanitize an upstream response head for downstream delivery ([D3]/[D16]).
///
/// A `101 Switching Protocols` response is a WebSocket handshake: its
/// `Connection: Upgrade` and `Upgrade: websocket` headers are NOT hop-by-hop
/// noise — the browser requires them (with `Sec-WebSocket-Accept`) to
/// complete the upgrade — and a 101 carries no framing or anti-framing
/// semantics, so its protocol headers pass through. Every response loses
/// upstream `Clear-Site-Data` and keeps only host-scoped app cookies.
/// Every normal response also has its
/// hop-by-hop headers stripped (`connection`, `keep-alive`,
/// `proxy-connection`, `upgrade`, `transfer-encoding`, plus any header the
/// upstream's `Connection` nominates; hyper re-frames the downstream body
/// itself) and its anti-framing headers normalized ([D16]).
/// `frame_ancestors_allow` is the [D16] workspace-origin allowance.
fn sanitize_relay_response_headers(
    headers: &mut HeaderMap,
    is_upgrade: bool,
    frame_ancestors_allow: &str,
) {
    // Remove Domain so the app can set a session cookie only for its own
    // preview host, never for the platform or sibling previews.
    let app_cookies: Vec<HeaderValue> = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(rewrite_isolated_preview_cookie)
        .collect();
    headers.remove(header::SET_COOKIE);
    for cookie in app_cookies {
        headers.append(header::SET_COOKIE, cookie);
    }
    // Cookie clearing affects the entire registrable domain, including the
    // platform's HttpOnly refresh cookie, even from an isolated preview host.
    headers.remove("clear-site-data");
    if is_upgrade {
        return;
    }
    let nominated: Vec<String> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    for name in nominated {
        // `close`/`keep-alive` tokens are connection options, not header
        // names, but removing a header that doesn't exist is a no-op either
        // way — remove whatever the upstream nominated.
        headers.remove(name.as_str());
    }
    headers.remove(header::CONNECTION);
    headers.remove(header::UPGRADE);
    headers.remove("keep-alive");
    headers.remove("proxy-connection");
    headers.remove(header::TRANSFER_ENCODING);
    normalize_anti_framing_headers(headers, frame_ancestors_allow);
}

/// Keep a preview app's server-issued cookie on its own host. Never relay a
/// G5-owned name or an explicit Domain that could target the platform/siblings.
fn rewrite_isolated_preview_cookie(value: &HeaderValue) -> Option<HeaderValue> {
    // HeaderValue::to_str rejects non-ASCII bytes, including valid UTF-8 that
    // some apps use in cookie values. Only invalid UTF-8 fails this boundary.
    let raw = std::str::from_utf8(value.as_bytes()).ok()?;
    let mut fields = raw.split(';');
    let (name, cookie_value) = fields.next()?.split_once('=')?;
    let name = name.trim();
    let cookie_value = cookie_value.trim();
    if name.is_empty()
        || http::header::HeaderName::from_bytes(name.as_bytes()).is_err()
        || cookie_value.chars().any(char::is_control)
        || (cookie_value.starts_with('"') != cookie_value.ends_with('"'))
    {
        return None;
    }
    let lower_name = name.to_ascii_lowercase();
    let bare_name = lower_name
        .strip_prefix("__host-")
        .or_else(|| lower_name.strip_prefix("__secure-"))
        .unwrap_or(&lower_name);
    if bare_name.starts_with("g5_") {
        return None;
    }

    let mut rewritten = format!("{name}={cookie_value}");
    for field in fields {
        let attr_name = field.split_once('=').map_or(field, |(name, _)| name).trim();
        if !attr_name.eq_ignore_ascii_case("domain") {
            rewritten.push(';');
            rewritten.push_str(field);
        }
    }
    HeaderValue::from_str(&rewritten).ok()
}

/// Normalize previewed-app anti-framing headers per serving mode
/// (routes/preview [D16]): app defaults like Spring Security's
/// `X-Frame-Options: DENY` otherwise block the workspace's embedded Preview
/// Browser ("refused to connect") while top-level tabs ignore those headers.
///
/// LEGACY mode (`frame_ancestors_allow == None`): `X-Frame-Options` is
/// rewritten to `SAMEORIGIN` and CSP `frame-ancestors` directives to
/// `frame-ancestors 'self'` — the embed is same-origin there.
///
/// SUBDOMAIN mode (`Some(workspace_origin)` — the startup-validated
/// `server.external_url` origin): the workspace is a CROSS-origin ancestor
/// and XFO has no allow-origin form, so XFO is removed and CSP
/// `frame-ancestors` directives are rewritten to `frame-ancestors 'self'
/// <workspace-origin>`; when the app sent an anti-framing header but the
/// result would carry no enforced `frame-ancestors` directive (XFO-only
/// apps), one is appended so stripping XFO never REDUCES the protection the
/// app asked for (multiple enforced CSP headers combine by intersection).
///
/// BOTH modes: nothing is added when the app sent no anti-framing header,
/// and `Content-Security-Policy-Report-Only` is left untouched.
fn normalize_anti_framing_headers(headers: &mut HeaderMap, workspace_origin: &str) {
    let had_xfo = headers.contains_key(header::X_FRAME_OPTIONS);
    // XFO cannot express "self plus this one cross-origin ancestor"; the CSP
    // directive below is the only admission mechanism.
    headers.remove(header::X_FRAME_OPTIONS);
    let target = format!("frame-ancestors 'self' {workspace_origin}");
    let csp_values: Vec<HeaderValue> = headers
        .get_all(header::CONTENT_SECURITY_POLICY)
        .iter()
        .cloned()
        .collect();
    // A directive matches only when its NAME token is exactly
    // `frame-ancestors` — prefix matching would rewrite unrelated directives
    // (e.g. a hypothetical `frame-ancestors-report-uri`) into an ENFORCED
    // frame-ancestors policy the app never sent, violating [D16]'s
    // nothing-added contract. Headers without a real match stay
    // byte-identical.
    let is_frame_ancestors = |directive: &str| {
        directive
            .split_ascii_whitespace()
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("frame-ancestors"))
    };
    let mut any_frame_ancestors = false;
    if !csp_values.is_empty() {
        let rewritten: Vec<HeaderValue> = csp_values
            .into_iter()
            .map(|value| {
                let Ok(text) = value.to_str() else {
                    return value;
                };
                // A single enforced CSP header VALUE may carry multiple
                // comma-separated policies (HTTP field-line merging; browsers
                // split on the comma exactly like separate headers). Policy
                // boundaries are preserved: `frame-ancestors` directives are
                // rewritten WITHIN their own policy only, and sibling
                // comma-separated policies are never replaced or dropped —
                // the former semicolon-only split either dropped an unrelated
                // enforced policy or left `frame-ancestors` unrewritten
                // depending on ordering ([D16], PR #1283 review P0).
                if !text
                    .split(',')
                    .any(|policy| policy.split(';').any(|d| is_frame_ancestors(d.trim())))
                {
                    return value;
                }
                any_frame_ancestors = true;
                let new_text = text
                    .split(',')
                    .map(|policy| {
                        policy
                            .split(';')
                            .map(str::trim)
                            .filter(|d| !d.is_empty())
                            .map(|d| {
                                if is_frame_ancestors(d) {
                                    target.as_str()
                                } else {
                                    d
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .filter(|policy| !policy.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                HeaderValue::from_str(&new_text).unwrap_or(value)
            })
            .collect();
        headers.remove(header::CONTENT_SECURITY_POLICY);
        for value in rewritten {
            headers.append(header::CONTENT_SECURITY_POLICY, value);
        }
    }
    // XFO-only app: the removed XFO must be REPLACED by the CSP directive, or
    // the response would leave the relay with less anti-framing protection
    // than the app sent ([D16]).
    if had_xfo && !any_frame_ancestors {
        if let Ok(value) = HeaderValue::from_str(&target) {
            headers.append(header::CONTENT_SECURITY_POLICY, value);
        }
    }
}

// ---------------------------------------------------------------------------
// WebSocket relay (raw upgrade passthrough)
// ---------------------------------------------------------------------------

/// Relay a WebSocket upgrade by forwarding the handshake over the tunnel and,
/// once the browser connection upgrades, piping raw bytes both directions. The
/// app speaks raw WebSocket over its TCP listener, so this is a transparent byte
/// pipe — g5-api never interprets WS frames.
async fn relay_websocket(
    parts: &mut http::request::Parts,
    stream: MuxStream,
    activity: Option<crate::services::workspace_runtime::WorkspaceActivityGuard>,
    rest_path: &str,
    policy: RelayPolicy<'_>,
) -> Result<Response, AppError> {
    // Forward the upgrade request over a hyper client connection with
    // upgrades enabled ([D19]); handshake headers preserved, no forced close.
    let mut sender = handshake_upstream::<http_body_util::Empty<Bytes>>(stream, activity).await?;
    let req = build_upstream_request(
        parts,
        rest_path,
        false,
        policy.strip_authorization,
        policy.origin,
        // [D5]/[D20]: WS handshake headers pass through untouched; the
        // handshake passed the same-origin gate in `proxy`.
        // `policy.drop_null_origin` is deliberately ignored here.
        false,
    )
    .body(http_body_util::Empty::new())
    .map_err(|e| {
        AppError::from_error(G5Error::Internal {
            message: format!("build preview upstream request: {e}"),
        })
    })?;
    // The handshake has no request body; the [D4] deadline bounds the head.
    let mut response =
        match tokio::time::timeout(PREVIEW_RESPONSE_HEAD_TIMEOUT, sender.send_request(req)).await {
            Ok(r) => r.map_err(|e| AppError::from_error(map_upstream_error(&e)))?,
            Err(_) => {
                return Err(AppError::from_error(G5Error::ServiceUnavailable {
                    message: "preview upstream did not start responding in time".into(),
                }))
            }
        };

    // Only wire the byte pipe when the upstream actually switched protocols.
    if response.status() == StatusCode::SWITCHING_PROTOCOLS {
        let upstream_upgrade = hyper::upgrade::on(&mut response);
        if let Some(browser_upgrade) = parts.extensions.remove::<hyper::upgrade::OnUpgrade>() {
            tokio::spawn(async move {
                let (browser, upstream) = match tokio::join!(browser_upgrade, upstream_upgrade) {
                    (Ok(b), Ok(u)) => (b, u),
                    (b, u) => {
                        tracing::warn!(
                            browser_err = ?b.err(),
                            upstream_err = ?u.err(),
                            "preview WS upgrade failed"
                        );
                        return;
                    }
                };
                let mut browser = TokioIo::new(browser);
                let mut upstream = TokioIo::new(upstream);
                // Transparent byte pipe — g5-api never interprets WS frames.
                // When either side ends, dropping the upstream IO releases the
                // hyper connection and the mux bridge performs its close-once.
                if let Err(e) = tokio::io::copy_bidirectional(&mut browser, &mut upstream).await {
                    tracing::debug!(error = %e, "preview WS pipe ended with error");
                }
            });
        } else {
            tracing::warn!("preview WS upgrade requested but no OnUpgrade extension present");
        }
        // Keep the protocol handshake intact while applying the cookie policy.
        sanitize_relay_response_headers(response.headers_mut(), true, policy.frame_ancestors_allow);
        let mut builder = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
        if let Some(h) = builder.headers_mut() {
            *h = response.headers().clone();
        }
        return builder.body(Body::empty()).map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("build preview WS response: {e}"),
            })
        });
    }

    // Non-101: the app declined the upgrade (401/404/...); relay the full
    // response INCLUDING its body as a normal HTTP response ([D5]).
    let (mut resp_parts, incoming) = response.into_parts();
    sanitize_relay_response_headers(&mut resp_parts.headers, false, policy.frame_ancestors_allow);
    let mut builder = Response::builder().status(resp_parts.status);
    if let Some(h) = builder.headers_mut() {
        *h = resp_parts.headers;
    }
    builder
        .body(Body::new(incoming.map_err(axum::Error::new)))
        .map_err(|e| {
            AppError::from_error(G5Error::Internal {
                message: format!("build preview WS response: {e}"),
            })
        })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Removes `__Host-g5_preview` name/value pairs from a `Cookie` header value
/// while preserving every other pair (the previewed app may legitimately use
/// its own cookies under the preview origin).
/// Returns the re-joined header value, empty when the ticket was the only
/// cookie.
fn strip_preview_ticket_cookie(cookie_header: &str) -> String {
    cookie_header
        .split(';')
        .map(str::trim)
        .filter(|pair| {
            !pair.is_empty()
                && pair
                    .split_once('=')
                    .map(|(name, _)| name.trim() != PREVIEW_TICKET_COOKIE_HOST)
                    .unwrap_or(true)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Whether the request is a WebSocket upgrade (`Upgrade: websocket` +
/// `Connection: upgrade`, case-insensitive).
fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    let upgrade_ws = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    let conn_upgrade = headers
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    upgrade_ws && conn_upgrade
}

fn same_isolated_websocket_origin(headers: &HeaderMap, preview: &PreviewHostOrigin) -> bool {
    // Suffix-mode startup requires HTTPS and generated preview URLs have no
    // explicit port. Host translation records this lowercased host before
    // rewriting the route; do not use forwarded headers or the rewritten URI.
    let expected = format!("https://{}", preview.host);
    let mut origins = headers.get_all(header::ORIGIN).iter();
    matches!(origins.next(), Some(value) if value.as_bytes() == expected.as_bytes())
        && origins.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Method;

    /// The preview host every relayed request arrives on in these tests.
    static TEST_ORIGIN: std::sync::LazyLock<PreviewHostOrigin> =
        std::sync::LazyLock::new(|| PreviewHostOrigin {
            workspace_id: Uuid::nil(),
            host: "abc.preview.g5.dev".to_string(),
        });
    const WORKSPACE_ORIGIN: &str = "https://dev.g5labs.ai";

    fn parts_with(method: Method, uri: &str, headers: &[(&str, &str)]) -> http::request::Parts {
        let mut b = http::Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(()).unwrap().into_parts().0
    }

    /// Render the typed upstream request as the wire-shaped head string the
    /// pre-[D19] tests asserted on (request line + `name: value` lines), so
    /// the policy assertions ([D9]/[D13]/[D18]) carry over unchanged. Header
    /// names are lowercase (http::HeaderName canonical form).
    fn build_head(
        parts: &http::request::Parts,
        rest_path: &str,
        force_close: bool,
        strip_authorization: bool,
        origin: &PreviewHostOrigin,
    ) -> String {
        build_head_with_null_origin_drop(
            parts,
            rest_path,
            force_close,
            strip_authorization,
            origin,
            false,
        )
    }

    fn build_head_with_null_origin_drop(
        parts: &http::request::Parts,
        rest_path: &str,
        force_close: bool,
        strip_authorization: bool,
        origin: &PreviewHostOrigin,
        drop_null_origin: bool,
    ) -> String {
        let req = build_upstream_request(
            parts,
            rest_path,
            force_close,
            strip_authorization,
            origin,
            drop_null_origin,
        )
        .body(())
        .unwrap();
        let mut s = format!("{} {} HTTP/1.1\r\n", req.method(), req.uri());
        for (name, value) in req.headers() {
            s.push_str(name.as_str());
            s.push_str(": ");
            s.push_str(value.to_str().unwrap_or("<non-utf8>"));
            s.push_str("\r\n");
        }
        s.push_str("\r\n");
        s
    }

    #[test]
    fn request_head_rewrites_path_and_forces_close() {
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/assets/app.js?v=2",
            &[
                ("host", "g5.dev"),
                ("accept", "*/*"),
                ("connection", "keep-alive"),
            ],
        );
        let head = build_head(&parts, "assets/app.js", true, true, &TEST_ORIGIN);
        // The query comes from the original URI; the path is the wildcard remainder.
        assert!(
            head.starts_with("GET /assets/app.js?v=2 HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(head.to_ascii_lowercase().contains("host: localhost\r\n"));
        assert!(head.contains("connection: close\r\n"));
        // The client's keep-alive Connection must not leak through.
        assert_eq!(head.to_ascii_lowercase().matches("connection:").count(), 1);
        assert!(head.ends_with("\r\n\r\n"));
    }

    #[test]
    fn request_head_strips_caller_credentials() {
        // routes/preview [D9]: the previewed customer app must never receive
        // the caller's Bearer JWT or the preview ticket cookie, while the
        // app's own cookies pass through.
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/",
            &[
                ("host", "g5.dev"),
                ("authorization", "Bearer caller.jwt.secret"),
                (
                    "cookie",
                    "sid=app-session; __Host-g5_preview=ticket123; theme=dark",
                ),
            ],
        );
        let head = build_head(&parts, "", true, true, &TEST_ORIGIN);
        let lower = head.to_ascii_lowercase();
        assert!(!lower.contains("authorization"), "{head}");
        assert!(!head.contains("g5_preview"), "{head}");
        assert!(
            lower.contains("cookie: sid=app-session; theme=dark\r\n"),
            "{head}"
        );
    }

    #[test]
    fn request_head_drops_null_origin_only_when_gated_forwards_real_origin() {
        // routes/preview [D20]: with the trusted-embed gate satisfied a
        // literal `Origin: null` (WebKit inside the embedded Preview Browser)
        // is dropped so previewed apps' CORS filters see the non-CORS
        // no-Origin shape; real origins forward even when gated.
        let parts = parts_with(
            Method::POST,
            "/api/v1/preview/abc/auth/authorize/callback",
            &[("host", "g5.dev"), ("origin", "null")],
        );
        let head = build_head_with_null_origin_drop(
            &parts,
            "auth/authorize/callback",
            true,
            true,
            &TEST_ORIGIN,
            true,
        );
        assert!(!head.contains("origin:"), "{head}");

        let parts = parts_with(
            Method::POST,
            "/api/v1/preview/abc/auth/authorize/callback",
            &[("host", "g5.dev"), ("origin", "https://g5.dev")],
        );
        let head = build_head_with_null_origin_drop(
            &parts,
            "auth/authorize/callback",
            true,
            true,
            &TEST_ORIGIN,
            true,
        );
        assert!(head.contains("origin: https://g5.dev\r\n"), "{head}");
    }

    #[test]
    fn request_head_subdomain_forwards_null_origin() {
        // routes/preview [D20]: on the SUBDOMAIN path an UNGATED request
        // keeps the marker. Sibling previews are same-site, so their
        // no-referrer form POSTs arrive `Sec-Fetch-Site: same-site`, fail
        // the same-ORIGIN gate leg, and the proxy passes
        // drop_null_origin = false — `Origin: null` stays the target app's
        // CSRF signal.
        let origin = PreviewHostOrigin {
            workspace_id: Uuid::new_v4(),
            host: "abc.preview.g5.dev".to_string(),
        };
        let parts = parts_with(
            Method::POST,
            "/api/v1/preview/abc/auth/authorize/callback",
            &[("host", "abc.preview.g5.dev"), ("origin", "null")],
        );
        let head = build_head(&parts, "auth/authorize/callback", true, true, &origin);
        assert!(head.contains("origin: null\r\n"), "{head}");
    }

    #[test]
    fn request_head_subdomain_drops_null_origin_when_gated() {
        // routes/preview [D20] (supervisor decision 8de289df): the gate now
        // admits BOTH serving modes — inside the [D47] sandboxed cross-origin
        // embed, WebKit nulls the previewed app's own in-frame form-POST
        // `Origin` even with `allow-same-origin`, and only a document already
        // on the TARGET preview's origin can attest
        // `Sec-Fetch-Site: same-origin`. With the gate satisfied the marker
        // is dropped.
        let origin = PreviewHostOrigin {
            workspace_id: Uuid::new_v4(),
            host: "abc.preview.g5.dev".to_string(),
        };
        let parts = parts_with(
            Method::POST,
            "/api/v1/preview/abc/auth/authorize/callback",
            &[("host", "abc.preview.g5.dev"), ("origin", "null")],
        );
        let head = build_head_with_null_origin_drop(
            &parts,
            "auth/authorize/callback",
            true,
            true,
            &origin,
            true,
        );
        assert!(!head.contains("origin:"), "{head}");
    }

    #[test]
    fn request_head_websocket_handshake_preserves_null_origin() {
        // The WS handshake Origin passes through for the upstream app to
        // evaluate; `proxy` checks it against the preview host before this
        // request builder can run.
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/ws",
            &[
                ("host", "g5.dev"),
                ("origin", "null"),
                ("connection", "upgrade"),
                ("upgrade", "websocket"),
            ],
        );
        let head = build_head(&parts, "ws", false, true, &TEST_ORIGIN);
        assert!(head.contains("origin: null\r\n"), "{head}");
    }

    #[test]
    fn isolated_websocket_origin_requires_one_exact_preview_origin() {
        let preview = PreviewHostOrigin {
            workspace_id: Uuid::new_v4(),
            host: "abc.preview.g5.dev".into(),
        };
        for origins in [
            vec![],
            vec!["null"],
            vec!["https://other.preview.g5.dev"],
            vec!["https://abc.preview.g5.dev.evil"],
            vec!["http://abc.preview.g5.dev"],
            vec!["https://abc.preview.g5.dev:444"],
            vec!["https://abc.preview.g5.dev", "https://abc.preview.g5.dev"],
        ] {
            let mut headers = HeaderMap::new();
            for origin in origins {
                headers.append(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
            }
            assert!(!same_isolated_websocket_origin(&headers, &preview));
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://abc.preview.g5.dev"),
        );
        assert!(same_isolated_websocket_origin(&headers, &preview));
    }

    #[test]
    fn trusted_embed_gate_requires_ticket_and_same_origin_navigation() {
        // routes/preview [D20] gate legs: ticket credential for THIS workspace +
        // fetch-metadata-attested same-ORIGIN navigation. Everything else
        // fails closed to preserving the header.
        let workspace = Uuid::new_v4();
        let auth = preview_cookie_auth(workspace, 900);
        let embed = &[
            ("sec-fetch-site", "same-origin"),
            ("sec-fetch-mode", "navigate"),
            ("sec-fetch-dest", "iframe"),
        ];
        let headers = |hs: &[(&str, &str)]| parts_with(Method::POST, "/p", hs).headers;

        // The embed's form-POST navigation passes (iframe dest); a top-level
        // same-origin tab (document dest) passes too.
        assert!(is_trusted_embed_navigation(
            &auth,
            workspace,
            &headers(embed)
        ));
        assert!(is_trusted_embed_navigation(
            &auth,
            workspace,
            &headers(&[
                ("sec-fetch-site", "same-origin"),
                ("sec-fetch-mode", "navigate"),
                ("sec-fetch-dest", "document"),
            ]),
        ));

        // Cross-mode sibling preview: same-SITE is not same-origin.
        assert!(!is_trusted_embed_navigation(
            &auth,
            workspace,
            &headers(&[
                ("sec-fetch-site", "same-site"),
                ("sec-fetch-mode", "navigate"),
                ("sec-fetch-dest", "document"),
            ]),
        ));
        // Subresource/XHR shapes and missing metadata fail closed.
        assert!(!is_trusted_embed_navigation(
            &auth,
            workspace,
            &headers(&[
                ("sec-fetch-site", "same-origin"),
                ("sec-fetch-mode", "cors"),
                ("sec-fetch-dest", "empty"),
            ]),
        ));
        assert!(!is_trusted_embed_navigation(
            &auth,
            workspace,
            &headers(&[])
        ));

        // Wrong workspace's ticket (aud mismatch) and non-ticket auth fail.
        let other_workspace_auth = preview_cookie_auth(Uuid::new_v4(), 900);
        assert!(!is_trusted_embed_navigation(
            &other_workspace_auth,
            workspace,
            &headers(embed),
        ));
        let bearer = AuthContext::User(g5_auth::JwtClaims {
            token_use: None,
            ..match preview_cookie_auth(workspace, 900) {
                AuthContext::User(c) => c,
                _ => unreachable!(),
            }
        });
        assert!(!is_trusted_embed_navigation(
            &bearer,
            workspace,
            &headers(embed)
        ));
    }

    #[test]
    fn request_head_forwards_app_authorization_when_not_platform() {
        // routes/preview [D9] selective stripping: when the classifier says
        // the Authorization is the previewed app's OWN credential, it must be
        // forwarded — while the preview ticket cookie stays filtered
        // unconditionally.
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/api/v1/me",
            &[
                ("host", "g5.dev"),
                ("authorization", "Bearer app.issued.token"),
                (
                    "cookie",
                    "sid=app-session; __Host-g5_preview=ticket123; theme=dark",
                ),
            ],
        );
        let head = build_head(&parts, "api/v1/me", true, false, &TEST_ORIGIN);
        let lower = head.to_ascii_lowercase();
        assert!(
            lower.contains("authorization: bearer app.issued.token\r\n"),
            "{head}"
        );
        assert!(!head.contains("g5_preview"), "{head}");
        assert!(
            lower.contains("cookie: sid=app-session; theme=dark\r\n"),
            "{head}"
        );
    }

    #[test]
    fn request_head_subdomain_rewrites_host_and_sets_forwarded() {
        // routes/preview [D13]: a subdomain-origin request has its forwarded
        // Host rewritten to bare `localhost` (default dev servers accept it),
        // with X-Forwarded-Host/Proto carrying the real origin; the client's
        // own Host / X-Forwarded-* values are dropped as proxy-owned.
        let workspace = Uuid::new_v4();
        let origin = PreviewHostOrigin {
            workspace_id: workspace,
            host: format!("{workspace}.preview.dev.g5labs.ai"),
        };
        let parts = parts_with(
            Method::GET,
            "/chunk-XYZ.js?import",
            &[
                ("host", &format!("{workspace}.preview.dev.g5labs.ai")),
                ("x-forwarded-proto", "http"),
                ("x-forwarded-host", "spoofed.example.com"),
            ],
        );
        let head = build_head(&parts, "chunk-XYZ.js", true, true, &origin);
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains("host: localhost\r\n"), "{head}");
        assert!(
            lower.contains(&format!(
                "x-forwarded-host: {workspace}.preview.dev.g5labs.ai\r\n"
            )),
            "{head}"
        );
        assert!(lower.contains("x-forwarded-proto: https\r\n"), "{head}");
        // The client's spoofed forwarded values must not survive.
        assert!(!lower.contains("spoofed.example.com"), "{head}");
        // Exactly one Host line (the request-line has no "host:", and
        // "x-forwarded-host:" is matched separately below).
        assert_eq!(lower.matches("\r\nhost: ").count(), 1, "{head}");
        assert_eq!(lower.matches("x-forwarded-proto:").count(), 1, "{head}");
    }

    #[test]
    fn request_head_strips_host_variant_preview_cookie() {
        // The session cookie `__Host-g5_preview` is filtered out of the
        // forwarded Cookie header; every other pair passes.
        let parts = parts_with(
            Method::GET,
            "/",
            &[("cookie", "sid=app; __Host-g5_preview=tkt; theme=dark")],
        );
        let head = build_head(&parts, "", true, true, &TEST_ORIGIN);
        assert!(!head.contains("g5_preview"), "{head}");
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains("cookie: sid=app; theme=dark\r\n"), "{head}");
    }

    #[test]
    fn subdomain_redirect_location_preserves_query_and_appends_ticket() {
        let workspace = Uuid::new_v4();
        let loc = subdomain_redirect_location(
            workspace,
            "preview.dev.g5labs.ai",
            "assets/app.js",
            Some("v=2"),
            "handoff.jwt",
        );
        assert_eq!(
            loc,
            format!(
                "https://{workspace}.preview.dev.g5labs.ai/assets/app.js?v=2&__g5_preview_ticket=handoff.jwt"
            )
        );
        // No query → just the ticket param.
        let root = subdomain_redirect_location(workspace, "preview.dev.g5labs.ai", "", None, "h");
        assert_eq!(
            root,
            format!("https://{workspace}.preview.dev.g5labs.ai/?__g5_preview_ticket=h")
        );
    }

    #[test]
    fn authorization_disposition_scheme_and_fail_closed_rules() {
        // [D9]: only Bearer tokens can be platform credentials; non-Bearer
        // schemes and absent headers are never stripped. With no validator
        // configured, Bearer strips unconditionally (fail closed).
        let headers = |v: Option<&str>| {
            let mut h = HeaderMap::new();
            if let Some(v) = v {
                h.insert(header::AUTHORIZATION, HeaderValue::from_str(v).unwrap());
            }
            h
        };
        let rejecting = g5_auth::JwtValidator::reject_all();
        for validator in [None, Some(&rejecting)] {
            assert!(!should_strip_authorization(validator, &headers(None)));
            assert!(!should_strip_authorization(
                validator,
                &headers(Some("Basic dXNlcjpwYXNz"))
            ));
            assert!(!should_strip_authorization(
                validator,
                &headers(Some("Bearer "))
            ));
            assert!(should_strip_authorization(
                validator,
                &headers(Some("Bearer any.token.value"))
            ));
            assert!(should_strip_authorization(
                validator,
                &headers(Some("bearer lowercase-scheme"))
            ));
            // A platform API key is a platform credential by prefix.
            assert!(should_strip_authorization(
                validator,
                &headers(Some("Bearer g5k_0123456789abcdef0123456789abcdef"))
            ));
            // Non-UTF8 header value: fail closed to strip.
            let mut h = HeaderMap::new();
            h.insert(
                header::AUTHORIZATION,
                HeaderValue::from_bytes(b"Bearer \xFFtoken").unwrap(),
            );
            assert!(should_strip_authorization(validator, &h));
        }
    }

    #[test]
    fn multi_valued_authorization_strips_if_any_value_is_platform() {
        // [D9]: HTTP allows multiple `authorization` fields. Classifying only
        // the first value would let `Basic <app>` + `Bearer <platform>`
        // forward the platform token — the decision must cover every value.
        // (No validator here → any Bearer classifies as platform, fail closed,
        // which is exactly what a smuggled platform token would do.)
        let mut smuggled = HeaderMap::new();
        smuggled.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic app-login"),
        );
        smuggled.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer platform.token.value"),
        );
        assert!(should_strip_authorization(None, &smuggled));

        // And the strip is applied to ALL authorization values, not just the
        // classified one, so nothing leaks downstream.
        let parts = {
            let mut b = http::Request::builder()
                .method(Method::GET)
                .uri("/api/v1/preview/abc/")
                .header("host", "g5.dev");
            b = b.header("authorization", "Basic app-login");
            b = b.header("authorization", "Bearer platform.token.value");
            b.body(()).unwrap().into_parts().0
        };
        let head = build_head(&parts, "", true, true, &TEST_ORIGIN);
        assert!(
            !head.to_ascii_lowercase().contains("authorization"),
            "no authorization value may survive when strip is decided: {head}"
        );

        // Multiple app-owned values (all non-Bearer / non-platform) forward.
        let mut app_only = HeaderMap::new();
        app_only.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic app-one"),
        );
        app_only.append(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic app-two"),
        );
        assert!(!should_strip_authorization(None, &app_only));
    }

    #[test]
    fn request_head_drops_cookie_header_when_only_ticket_present() {
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/",
            &[("cookie", "__Host-g5_preview=ticket123")],
        );
        let head = build_head(&parts, "", true, true, &TEST_ORIGIN);
        assert!(!head.to_ascii_lowercase().contains("cookie:"), "{head}");
    }

    #[test]
    fn request_head_preserves_utf8_app_cookie_without_forwarding_ticket() {
        let mut parts = parts_with(Method::GET, "/api/v1/preview/abc/", &[]);
        parts.headers.append(
            header::COOKIE,
            HeaderValue::from_str("__Host-g5_preview=ticket123; app=café").unwrap(),
        );
        let request = build_upstream_request(&parts, "", true, true, &TEST_ORIGIN, false)
            .body(())
            .unwrap();
        assert_eq!(
            request.headers().get(header::COOKIE).unwrap().as_bytes(),
            "app=café".as_bytes()
        );
    }

    #[test]
    fn strip_preview_ticket_cookie_preserves_other_pairs() {
        assert_eq!(
            strip_preview_ticket_cookie("a=1; __Host-g5_preview=t; b=2"),
            "a=1; b=2"
        );
        assert_eq!(strip_preview_ticket_cookie("__Host-g5_preview=t"), "");
        // A name that merely starts with the ticket name is NOT stripped, and
        // the retired platform-origin name is an ordinary app cookie now.
        assert_eq!(
            strip_preview_ticket_cookie("__Host-g5_previewx=keep; g5_preview=app"),
            "__Host-g5_previewx=keep; g5_preview=app"
        );
        assert_eq!(strip_preview_ticket_cookie("plainflag"), "plainflag");
    }

    #[test]
    fn anti_framing_xfo_replaced_by_workspace_frame_ancestors() {
        let mut headers = HeaderMap::new();
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        normalize_anti_framing_headers(&mut headers, WORKSPACE_ORIGIN);
        assert!(headers.get(header::X_FRAME_OPTIONS).is_none());
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "frame-ancestors 'self' https://dev.g5labs.ai"
        );
    }

    #[test]
    fn anti_framing_nothing_added_when_absent() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        normalize_anti_framing_headers(&mut headers, WORKSPACE_ORIGIN);
        assert!(headers.get(header::X_FRAME_OPTIONS).is_none());
        assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    }

    #[test]
    fn anti_framing_csp_frame_ancestors_rewritten_others_preserved() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'; img-src *"),
        );
        normalize_anti_framing_headers(&mut headers, WORKSPACE_ORIGIN);
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'; frame-ancestors 'self' https://dev.g5labs.ai; img-src *"
        );
    }

    #[test]
    fn anti_framing_csp_without_frame_ancestors_untouched() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'; img-src *"),
        );
        normalize_anti_framing_headers(&mut headers, WORKSPACE_ORIGIN);
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'; img-src *"
        );
    }

    #[test]
    fn anti_framing_prefix_named_directive_not_rewritten() {
        // Exact directive-name match only: a directive that merely STARTS
        // with `frame-ancestors` must not be rewritten into an enforced
        // frame-ancestors policy the app never sent ([D16] nothing-added).
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self';frame-ancestors-report-uri /csp"),
        );
        normalize_anti_framing_headers(&mut headers, WORKSPACE_ORIGIN);
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self';frame-ancestors-report-uri /csp"
        );
    }

    #[test]
    fn anti_framing_101_handshake_headers_untouched() {
        // D16/D5: a WebSocket handshake's headers pass through verbatim — the
        // sanitize pass (hop-by-hop strip + anti-framing normalization)
        // applies to normal HTTP responses only.
        let mut headers = HeaderMap::new();
        headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        headers.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
        headers.insert("sec-websocket-accept", HeaderValue::from_static("abc"));
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        sanitize_relay_response_headers(&mut headers, true, WORKSPACE_ORIGIN);
        assert_eq!(headers.get(header::UPGRADE).unwrap(), "websocket");
        assert_eq!(headers.get(header::CONNECTION).unwrap(), "Upgrade");
        assert_eq!(headers.get("sec-websocket-accept").unwrap(), "abc");
        assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    }

    #[test]
    fn anti_framing_report_only_csp_untouched() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY_REPORT_ONLY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        );
        normalize_anti_framing_headers(&mut headers, WORKSPACE_ORIGIN);
        assert_eq!(
            headers
                .get(header::CONTENT_SECURITY_POLICY_REPORT_ONLY)
                .unwrap(),
            "frame-ancestors 'none'"
        );
    }

    const WS_ORIGIN: &str = WORKSPACE_ORIGIN;

    #[test]
    fn anti_framing_subdomain_xfo_only_replaced_by_workspace_csp() {
        // [D16] subdomain mode: XFO has no allow-origin form — it is removed
        // and REPLACED by the workspace-admitting CSP directive, so an
        // XFO-only app (Spring's DENY default) never leaves the relay with
        // less anti-framing protection than it asked for.
        let mut headers = HeaderMap::new();
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert!(headers.get(header::X_FRAME_OPTIONS).is_none());
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "frame-ancestors 'self' https://dev.g5labs.ai"
        );
    }

    #[test]
    fn anti_framing_subdomain_csp_frame_ancestors_rewritten_others_preserved() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'; img-src *"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'; frame-ancestors 'self' https://dev.g5labs.ai; img-src *"
        );
        // Only the one rewritten CSP header — no extra appended value.
        assert_eq!(
            headers
                .get_all(header::CONTENT_SECURITY_POLICY)
                .iter()
                .count(),
            1
        );
    }

    #[test]
    fn anti_framing_subdomain_xfo_and_csp_no_duplicate_append() {
        // App sent BOTH XFO and a CSP frame-ancestors directive: XFO is
        // removed, the directive rewritten, and no second CSP value appended.
        let mut headers = HeaderMap::new();
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert!(headers.get(header::X_FRAME_OPTIONS).is_none());
        let values: Vec<_> = headers
            .get_all(header::CONTENT_SECURITY_POLICY)
            .iter()
            .collect();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], "frame-ancestors 'self' https://dev.g5labs.ai");
    }

    #[test]
    fn anti_framing_subdomain_nothing_added_when_absent() {
        // [D16] nothing-added contract holds in subdomain mode: an app that
        // permits framing keeps permitting it.
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert!(headers.get(header::X_FRAME_OPTIONS).is_none());
        assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    }

    #[test]
    fn anti_framing_subdomain_csp_without_frame_ancestors_untouched_no_append() {
        // A CSP with no frame-ancestors directive (and no XFO) is not an
        // anti-framing header — byte-identical passthrough, nothing appended.
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'; img-src *"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'; img-src *"
        );
        assert_eq!(
            headers
                .get_all(header::CONTENT_SECURITY_POLICY)
                .iter()
                .count(),
            1
        );
    }

    #[test]
    fn anti_framing_subdomain_report_only_untouched() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY_REPORT_ONLY,
            HeaderValue::from_static("frame-ancestors 'none'"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert_eq!(
            headers
                .get(header::CONTENT_SECURITY_POLICY_REPORT_ONLY)
                .unwrap(),
            "frame-ancestors 'none'"
        );
        assert!(headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    }

    #[test]
    fn anti_framing_subdomain_multiple_csp_headers_rewrite_only_matching() {
        // Multiple enforced CSP headers combine by intersection: only the
        // value carrying a frame-ancestors directive is rewritten; the other
        // stays byte-identical and nothing extra is appended.
        let mut headers = HeaderMap::new();
        headers.append(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'"),
        );
        headers.append(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none'; img-src *"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        let values: Vec<_> = headers
            .get_all(header::CONTENT_SECURITY_POLICY)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect();
        assert_eq!(
            values,
            vec![
                "default-src 'self'".to_string(),
                "frame-ancestors 'self' https://dev.g5labs.ai; img-src *".to_string(),
            ]
        );
    }

    #[test]
    fn anti_framing_comma_policies_preserved_frame_ancestors_first() {
        // [D16] P0 regression (PR #1283 review): one enforced CSP header
        // VALUE may carry multiple comma-separated policies. With
        // `frame-ancestors` in the FIRST policy, the sibling policy must
        // survive the rewrite (the old semicolon-only split replaced the
        // whole value, dropping `default-src 'none'`).
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("frame-ancestors 'none', default-src 'none'"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "frame-ancestors 'self' https://dev.g5labs.ai, default-src 'none'"
        );
    }

    #[test]
    fn anti_framing_comma_policies_rewritten_frame_ancestors_last() {
        // [D16] P0 regression (PR #1283 review): with `frame-ancestors` in a
        // LATER comma-separated policy it must still be rewritten (the old
        // split keyed the whole value on its first directive name and left
        // the blocking 'none' in place) — and no duplicate CSP value is
        // appended even when XFO was also present.
        let mut headers = HeaderMap::new();
        headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self', frame-ancestors 'none'"),
        );
        normalize_anti_framing_headers(&mut headers, WS_ORIGIN);
        assert!(headers.get(header::X_FRAME_OPTIONS).is_none());
        let values: Vec<_> = headers
            .get_all(header::CONTENT_SECURITY_POLICY)
            .iter()
            .collect();
        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0],
            "default-src 'self', frame-ancestors 'self' https://dev.g5labs.ai"
        );
    }

    /// A cookie-minted preview AuthContext whose ticket expires `exp_offset_secs`
    /// from now (matches what the [I13] carve-out inserts).
    fn preview_cookie_auth(workspace_id: Uuid, exp_offset_secs: i64) -> AuthContext {
        let now = chrono::Utc::now().timestamp();
        AuthContext::User(g5_auth::JwtClaims {
            sub: Uuid::new_v4().to_string(),
            org: Uuid::new_v4().to_string(),
            scopes: vec![Scope::TasksRead, Scope::WorkspacesRead],
            iat: now - (PREVIEW_TICKET_LIFETIME_SECS - exp_offset_secs),
            exp: now + exp_offset_secs,
            iss: Some("g5-api".to_string()),
            aud: Some(g5_auth::preview_ticket_audience(workspace_id)),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: Some(g5_auth::PREVIEW_TOKEN_USE.to_string()),
            client: None,
        })
    }

    fn live_session_expiry() -> Option<chrono::DateTime<chrono::Utc>> {
        Some(chrono::Utc::now() + chrono::Duration::hours(1))
    }

    #[test]
    fn renewal_fires_inside_window_against_live_session() {
        let workspace = Uuid::new_v4();
        // Past half-life (600s remaining), session live for another hour.
        let auth = preview_cookie_auth(workspace, 600);
        assert!(should_renew_preview_ticket(
            &auth,
            workspace,
            live_session_expiry()
        ));
        // At (about) the 900s boundary — inclusive. Constructed at 898s so
        // wall-clock drift between construction and check stays inside the
        // window rather than flaking across it.
        let auth = preview_cookie_auth(workspace, 898);
        assert!(should_renew_preview_ticket(
            &auth,
            workspace,
            live_session_expiry()
        ));
    }

    #[test]
    fn no_renewal_for_fresh_ticket() {
        let workspace = Uuid::new_v4();
        // Fresh ticket far above the window (drift-safe margin above 900).
        let auth = preview_cookie_auth(workspace, 1700);
        assert!(!should_renew_preview_ticket(
            &auth,
            workspace,
            live_session_expiry()
        ));
    }

    #[test]
    fn no_renewal_for_expired_or_zero_remaining_ticket() {
        let workspace = Uuid::new_v4();
        let expired = preview_cookie_auth(workspace, -10);
        assert!(!should_renew_preview_ticket(
            &expired,
            workspace,
            live_session_expiry()
        ));
        let zero = preview_cookie_auth(workspace, 0);
        assert!(!should_renew_preview_ticket(
            &zero,
            workspace,
            live_session_expiry()
        ));
    }

    #[test]
    fn no_renewal_for_wrong_workspace_audience() {
        let workspace = Uuid::new_v4();
        let other_workspace = Uuid::new_v4();
        let auth = preview_cookie_auth(other_workspace, 600);
        assert!(!should_renew_preview_ticket(
            &auth,
            workspace,
            live_session_expiry()
        ));
    }

    #[test]
    fn no_renewal_for_platform_bearer_fall_through() {
        let workspace = Uuid::new_v4();
        // A platform user JWT has no token_use and no preview audience —
        // exactly what the Bearer fall-through inserts.
        let AuthContext::User(mut claims) = preview_cookie_auth(workspace, 600) else {
            unreachable!()
        };
        claims.token_use = None;
        claims.aud = None;
        let auth = AuthContext::User(claims);
        assert!(!should_renew_preview_ticket(
            &auth,
            workspace,
            live_session_expiry()
        ));
    }

    #[test]
    fn no_renewal_for_null_expiry_legacy_session() {
        let workspace = Uuid::new_v4();
        let auth = preview_cookie_auth(workspace, 600);
        assert!(!should_renew_preview_ticket(&auth, workspace, None));
    }

    #[test]
    fn no_renewal_for_expired_session() {
        let workspace = Uuid::new_v4();
        let auth = preview_cookie_auth(workspace, 600);
        let expired_session = Some(chrono::Utc::now() - chrono::Duration::seconds(30));
        assert!(!should_renew_preview_ticket(
            &auth,
            workspace,
            expired_session
        ));
    }

    /// The shared issuance boundary (G5Labs/G5-Lean#661): a person's session
    /// and the preview ticket cookie it exchanged into yield their own
    /// `sub`/`org`; an organization API key — even one with every scope — is
    /// refused with `user_session_required`, and a host carries no user.
    #[test]
    fn preview_session_identity_admits_person_credentials_only() {
        use crate::middleware::auth_middleware::API_KEY_ISSUER;

        let workspace = Uuid::new_v4();
        let cookie = preview_cookie_auth(workspace, 600);
        let AuthContext::User(cookie_jwt) = &cookie else {
            unreachable!()
        };
        assert_eq!(
            preview_session_identity(&cookie).unwrap(),
            (
                cookie_jwt.sub.parse().unwrap(),
                cookie_jwt.org.parse().unwrap()
            )
        );

        let AuthContext::User(mut session_jwt) = preview_cookie_auth(workspace, 600) else {
            unreachable!()
        };
        session_jwt.token_use = None;
        session_jwt.aud = Some("g5".into());
        let session = AuthContext::User(session_jwt.clone());
        assert_eq!(
            preview_session_identity(&session).unwrap(),
            (
                session_jwt.sub.parse().unwrap(),
                session_jwt.org.parse().unwrap()
            )
        );

        for scopes in [vec![Scope::TasksRead], Scope::all().to_vec()] {
            let mut key_jwt = session_jwt.clone();
            key_jwt.iss = Some(API_KEY_ISSUER.into());
            key_jwt.scopes = scopes;
            let refused = preview_session_identity(&AuthContext::User(key_jwt)).unwrap_err();
            match refused.inner {
                G5Error::Forbidden { missing_scopes } => {
                    assert_eq!(missing_scopes, vec!["user_session_required".to_string()]);
                }
                other => panic!("API key must be refused as a non-session: {other:?}"),
            }
        }

        let host = AuthContext::Host {
            host_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            scopes: vec![Scope::WorkerHost],
        };
        assert!(matches!(
            preview_session_identity(&host).unwrap_err().inner,
            G5Error::Unauthorized
        ));
    }

    #[test]
    fn reauth_return_sanitizer_d18() {
        // Legitimate paths round-trip.
        assert_eq!(
            sanitize_reauth_return(Some("/dashboard?tab=1")),
            "/dashboard?tab=1"
        );
        assert_eq!(sanitize_reauth_return(Some("/")), "/");
        // Reserved platform params are stripped; the rest is preserved.
        assert_eq!(
            sanitize_reauth_return(Some("/x?a=1&__g5_reauth=99&__g5_preview_ticket=t&b=2")),
            "/x?a=1&b=2"
        );
        assert_eq!(sanitize_reauth_return(Some("/x?__g5_reauth=99")), "/x");
        // Open-redirect and header-injection shapes degrade to "/".
        assert_eq!(sanitize_reauth_return(Some("//evil.com/")), "/");
        assert_eq!(sanitize_reauth_return(Some("/\\evil")), "/");
        assert_eq!(sanitize_reauth_return(Some("https://evil.com/")), "/");
        assert_eq!(sanitize_reauth_return(Some("/a#frag")), "/");
        assert_eq!(sanitize_reauth_return(Some("/a\r\nSet-Cookie: x=y")), "/");
        assert_eq!(sanitize_reauth_return(Some("")), "/");
        assert_eq!(sanitize_reauth_return(None), "/");
        let oversize = format!("/{}", "a".repeat(2100));
        assert_eq!(sanitize_reauth_return(Some(&oversize)), "/");
    }

    #[test]
    fn upstream_request_head_strips_reauth_marker() {
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/page?tab=1&__g5_reauth=1752380000&x=2",
            &[],
        );
        let head = build_head(&parts, "page", true, true, &TEST_ORIGIN);
        assert!(
            head.starts_with("GET /page?tab=1&x=2 HTTP/1.1\r\n"),
            "{head}"
        );

        // Marker-only query: the upstream target has no query at all.
        let parts = parts_with(Method::GET, "/api/v1/preview/abc/?__g5_reauth=1", &[]);
        let head = build_head(&parts, "", true, true, &TEST_ORIGIN);
        assert!(head.starts_with("GET / HTTP/1.1\r\n"), "{head}");
    }

    #[test]
    fn request_head_root_path_is_slash() {
        let parts = parts_with(Method::POST, "/api/v1/preview/abc", &[]);
        let head = build_head(&parts, "", true, true, &TEST_ORIGIN);
        assert!(head.starts_with("POST / HTTP/1.1\r\n"), "{head}");
    }

    #[test]
    fn request_head_preserves_upgrade_headers_when_not_forcing_close() {
        let parts = parts_with(
            Method::GET,
            "/api/v1/preview/abc/ws",
            &[
                ("connection", "Upgrade"),
                ("upgrade", "websocket"),
                ("sec-websocket-key", "dGhlIHNhbXBsZQ=="),
            ],
        );
        let head = build_head(&parts, "ws", false, true, &TEST_ORIGIN);
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains("connection: upgrade\r\n"));
        assert!(lower.contains("upgrade: websocket\r\n"));
        assert!(lower.contains("sec-websocket-key:"));
    }

    #[test]
    fn detects_websocket_upgrade() {
        let mut h = HeaderMap::new();
        h.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        h.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
        assert!(is_websocket_upgrade(&h));

        let mut plain = HeaderMap::new();
        plain.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
        assert!(!is_websocket_upgrade(&plain));
    }

    // -----------------------------------------------------------------------
    // Relay exchange tests ([D19]): drive the FULL hyper-over-mux relay
    // against a scripted upstream on an in-process mux pair.
    // -----------------------------------------------------------------------

    /// In-process mux pair: returns (relay-side stream, upstream-side stream,
    /// keep-alive handles for both muxes).
    async fn mux_pair() -> (
        MuxStream,
        MuxStream,
        g5_internal_api::stream_mux::Mux,
        g5_internal_api::stream_mux::Mux,
    ) {
        let (a, b) = tokio::io::duplex(256 * 1024);
        let api = g5_internal_api::stream_mux::Mux::new(a, true);
        let mut host = g5_internal_api::stream_mux::Mux::new(b, false);
        let client = api
            .open(g5_internal_api::stream_mux::Channel::Http)
            .await
            .unwrap();
        let accepted = host.accept().await.unwrap();
        (client, accepted, api, host)
    }

    fn get_parts(path: &str) -> http::request::Parts {
        parts_with(Method::GET, path, &[("host", "g5.dev")])
    }

    /// Read one full HTTP/1.1 request (head + Content-Length body) from the
    /// upstream side of the mux. Returns the raw bytes.
    async fn read_full_request(reader: &mut g5_internal_api::stream_mux::MuxReader) -> Vec<u8> {
        fn head_end(buf: &[u8]) -> Option<usize> {
            buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
        }
        let mut buf: Vec<u8> = Vec::new();
        let (mut end, mut want) = (None, 0usize);
        loop {
            if end.is_none() {
                if let Some(e) = head_end(&buf) {
                    let head = String::from_utf8_lossy(&buf[..e]).to_ascii_lowercase();
                    want = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    end = Some(e);
                }
            }
            if let Some(e) = end {
                if buf.len() >= e + want {
                    return buf;
                }
            }
            match reader.recv().await {
                Some(c) => buf.extend_from_slice(&c),
                None => return buf,
            }
        }
    }

    #[tokio::test]
    async fn relay_serves_content_length_body_and_closes_after_completion() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        let upstream = tokio::spawn(async move {
            let req = read_full_request(&mut r).await;
            assert!(req.starts_with(b"GET /index.html HTTP/1.1\r\n"));
            w.send(b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: 5\r\nconnection: close\r\nset-cookie: g5_refresh=evil; Path=/; Domain=g5.example\r\nset-cookie: g5_preview=evil; Path=/\r\nclear-site-data: \"cookies\", \"storage\"\r\n\r\nhello")
                .await
                .unwrap();
            // Do NOT close: the RELAY must close its send half once the
            // upstream message completes ([D11] registry-cap obligation).
            let eof = r.recv().await;
            assert!(eof.is_none(), "expected relay close, got {eof:?}");
        });

        let parts = get_parts("/api/v1/preview/abc/index.html");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "index.html",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        assert!(resp.headers().get(header::SET_COOKIE).is_none());
        assert!(resp.headers().get("clear-site-data").is_none());
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html"
        );
        // Hop-by-hop stripped on the way down.
        assert!(resp.headers().get(header::CONNECTION).is_none());
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"hello");
        tokio::time::timeout(std::time::Duration::from_secs(5), upstream)
            .await
            .expect("upstream must see the relay close after message completion")
            .unwrap();
    }

    /// [D16] production wiring (PR #1283 review): the workspace-origin
    /// allowance travels from the relay entry through
    /// `sanitize_relay_response_headers` — an upstream response carrying app
    /// anti-framing headers leaves `relay_http` with the subdomain-mode
    /// normalization applied (XFO stripped, `frame-ancestors` rewritten to
    /// admit the workspace), not only in the normalization unit tests.
    #[tokio::test]
    async fn relay_http_applies_workspace_origin_anti_framing_normalization() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        let upstream = tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            w.send(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/html\r\n\
                  x-frame-options: DENY\r\n\
                  content-security-policy: default-src 'self'; frame-ancestors 'none'\r\n\
                  content-length: 2\r\n\r\nok",
            )
            .await
            .unwrap();
            let _ = r.recv().await;
        });

        let parts = get_parts("/api/v1/preview/abc/page");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "page",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().get(header::X_FRAME_OPTIONS).is_none());
        assert_eq!(
            resp.headers().get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'self'; frame-ancestors 'self' https://dev.g5labs.ai"
        );
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"ok");
        tokio::time::timeout(std::time::Duration::from_secs(5), upstream)
            .await
            .expect("upstream must observe the relay close")
            .unwrap();
    }

    /// [D11] rewritten for [D19]: the mux send half stays OPEN while the
    /// upstream response is incomplete (a premature half-close is propagated
    /// through the bridge and can truncate the response), and closes once
    /// the upstream message is complete.
    #[tokio::test(start_paused = true)]
    async fn send_half_stays_open_until_upstream_response_completes() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        let (head_sent_tx, head_sent_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel::<()>();
        let upstream = tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            w.send(b"HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\nbo")
                .await
                .unwrap();
            head_sent_tx.send(()).unwrap();
            // While the response is INCOMPLETE the relay must not close.
            let premature = tokio::time::timeout(std::time::Duration::from_secs(5), r.recv()).await;
            assert!(
                premature.is_err(),
                "send half must stay open while the upstream response is incomplete: {premature:?}"
            );
            finish_rx.await.unwrap();
            w.send(b"dy").await.unwrap();
            let eof = tokio::time::timeout(std::time::Duration::from_secs(60), r.recv())
                .await
                .expect("relay must close after the upstream message completes");
            assert!(eof.is_none(), "expected EOF, got {eof:?}");
        });

        let parts = get_parts("/api/v1/preview/abc/slow");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "slow",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        head_sent_rx.await.unwrap();
        // Let the upstream's premature-close probe run its 5 paused seconds.
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        finish_tx.send(()).unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"body");
        upstream.await.unwrap();
    }

    /// Dropping the downstream response (client disconnect) must close the
    /// mux send half — an abandoned exchange must not strand a registry entry
    /// against MAX_LIVE_STREAMS ([D11]).
    #[tokio::test]
    async fn dropping_downstream_response_closes_mux() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        let upstream = tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            // Head + first fragment of a never-finished 1 GiB body.
            w.send(b"HTTP/1.1 200 OK\r\ncontent-length: 1073741824\r\n\r\nfragment")
                .await
                .unwrap();
            let eof = tokio::time::timeout(std::time::Duration::from_secs(5), r.recv())
                .await
                .expect("relay must close when the downstream response is dropped");
            assert!(eof.is_none(), "expected EOF, got {eof:?}");
        });

        let parts = get_parts("/api/v1/preview/abc/big");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "big",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        drop(resp); // client walked away mid-body
        upstream.await.unwrap();
    }

    #[tokio::test]
    async fn upstream_close_before_response_maps_to_503() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            w.close().await; // upstream dies without responding
        });

        let parts = get_parts("/api/v1/preview/abc/dead");
        let err = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "dead",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .expect_err("dead upstream must fail");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("preview upstream closed before sending a response"),
            "operator-facing 503 class must be preserved: {msg}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn unresponsive_upstream_times_out_with_503() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (_w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            // Accept the request, never respond, never close.
            std::future::pending::<()>().await;
        });

        let parts = get_parts("/api/v1/preview/abc/hang");
        let err = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "hang",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .expect_err("unresponsive upstream must time out");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("preview upstream did not start responding in time"),
            "operator-facing TTFB 503 class must be preserved: {msg}"
        );
    }

    #[tokio::test]
    async fn relay_close_delimited_body_flows_to_eof() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            // HTTP/1.1 with neither Content-Length nor chunked: body runs to EOF.
            w.send(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\nclose-")
                .await
                .unwrap();
            w.send(b"delimited").await.unwrap();
            w.close().await;
        });

        let parts = get_parts("/api/v1/preview/abc/stream");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "stream",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"close-delimited");
    }

    #[tokio::test]
    async fn relay_chunked_body_decoded_and_te_stripped() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            w.send(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\ncontent-type: text/event-stream\r\n\r\n")
                .await
                .unwrap();
            // Chunks split oddly across mux frames (decoder must reassemble).
            w.send(b"5\r\nhel").await.unwrap();
            w.send(b"lo\r\n2\r\nhi\r\n0\r\n\r\n").await.unwrap();
            let _ = r.recv().await;
        });

        let parts = get_parts("/api/v1/preview/abc/sse");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "sse",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        assert!(
            resp.headers().get(header::TRANSFER_ENCODING).is_none(),
            "hop-by-hop transfer-encoding must be stripped downstream"
        );
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"hellohi");
    }

    /// A HEAD response completes at the head even when it advertises a
    /// Content-Length and the upstream connection stays open — the pre-[D19]
    /// relay would have waited for entity bytes that never come ([D3]).
    #[tokio::test]
    async fn head_response_completes_at_head_on_persistent_upstream() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let req = read_full_request(&mut r).await;
            assert!(req.starts_with(b"HEAD "));
            w.send(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\n")
                .await
                .unwrap();
            // Stay open: completion must come from HEAD semantics, not EOF.
            let _ = r.recv().await;
        });

        let parts = parts_with(
            Method::HEAD,
            "/api/v1/preview/abc/index.html",
            &[("host", "g5.dev")],
        );
        let resp = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            relay_http(
                parts,
                Body::empty(),
                client,
                None,
                "index.html",
                RelayPolicy {
                    strip_authorization: true,
                    origin: &TEST_ORIGIN,
                    drop_null_origin: false,
                    frame_ancestors_allow: WORKSPACE_ORIGIN,
                },
            ),
        )
        .await
        .expect("HEAD must complete at the response head")
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert!(body.is_empty());
    }

    /// Informational responses are consumed, never relayed as final ([D3]) —
    /// the pre-[D19] head parser would have returned the 103 to the browser.
    #[tokio::test]
    async fn informational_103_is_consumed_and_final_200_relayed() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            w.send(b"HTTP/1.1 103 Early Hints\r\nlink: </style.css>; rel=preload\r\n\r\n")
                .await
                .unwrap();
            w.send(b"HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\ndone")
                .await
                .unwrap();
            let _ = r.recv().await;
        });

        let parts = get_parts("/api/v1/preview/abc/hints");
        let resp = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "hints",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"done");
    }

    /// Strict parsing stays strict ([D19]: no hyper leniency flags) — a
    /// non-HTTP upstream maps to an explicit relay error, never a panic.
    #[tokio::test]
    async fn malformed_upstream_response_maps_to_parse_error() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            let _ = read_full_request(&mut r).await;
            w.send(b"NOT-HTTP GARBAGE\r\n\r\n").await.unwrap();
            w.close().await;
        });

        let parts = get_parts("/api/v1/preview/abc/garbage");
        let err = relay_http(
            parts,
            Body::empty(),
            client,
            None,
            "garbage",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .expect_err("garbage upstream must fail");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("parse preview upstream response")
                || msg.contains("preview upstream exchange failed"),
            "parse failures must map to an explicit relay error: {msg}"
        );
    }

    /// [D4] preserved under [D19]: the 30 s deadline arms only after the
    /// request body has been fully yielded — a large upload crawling through
    /// a backpressured tunnel (upstream drains slower than the mux window)
    /// must not eat the response window.
    #[tokio::test(start_paused = true)]
    async fn backpressured_upload_does_not_consume_ttfb_deadline() {
        let (client, accepted, _m1, _m2) = mux_pair().await;
        let (w, mut r) = accepted.into_split();
        tokio::spawn(async move {
            // Drain the request SLOWLY: >30s of virtual time elapses while the
            // body is still being pushed through the mux window (100 ms per
            // 16 KiB frame × a 20 MiB body ≈ 128 s total, while the trailing
            // write-buffer skew after the body-sent signal stays ≪ 30 s).
            let mut got = 0usize;
            let mut head_seen = false;
            let mut want = usize::MAX;
            let mut buf: Vec<u8> = Vec::new();
            while got < want {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                match r.recv().await {
                    Some(c) => {
                        got += c.len();
                        if !head_seen {
                            buf.extend_from_slice(&c);
                            if let Some(e) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                head_seen = true;
                                let head =
                                    String::from_utf8_lossy(&buf[..e + 4]).to_ascii_lowercase();
                                let cl: usize = head
                                    .lines()
                                    .find_map(|l| l.strip_prefix("content-length:"))
                                    .and_then(|v| v.trim().parse().ok())
                                    .unwrap_or(0);
                                want = e + 4 + cl;
                                got = buf.len();
                            }
                        }
                    }
                    None => break,
                }
            }
            w.send(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .await
                .unwrap();
            let _ = r.recv().await;
        });

        // 20 MiB body: far beyond the 256 KiB mux stream window, so the send
        // genuinely backpressures until the upstream drains — total transfer
        // time greatly exceeds the 30 s deadline that must NOT fire.
        let upload = vec![b'x'; 20 * 1024 * 1024];
        let parts = parts_with(
            Method::POST,
            "/api/v1/preview/abc/upload",
            &[("host", "g5.dev")],
        );
        let resp = relay_http(
            parts,
            Body::from(upload),
            client,
            None,
            "upload",
            RelayPolicy {
                strip_authorization: true,
                origin: &TEST_ORIGIN,
                drop_null_origin: false,
                frame_ancestors_allow: WORKSPACE_ORIGIN,
            },
        )
        .await
        .expect("slow upload must not trip the TTFB deadline");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// [D5]: a non-101 answer to an upgrade request is a normal HTTP response
    /// and must reach the browser WITH its body (the pre-[D19] path returned
    /// an empty body).
    #[tokio::test]
    async fn non_101_ws_upgrade_response_relays_body() {
        {
            let (client, accepted, _m1, _m2) = mux_pair().await;
            let (w, mut r) = accepted.into_split();
            tokio::spawn(async move {
                let req = read_full_request(&mut r).await;
                let head = String::from_utf8_lossy(&req).to_ascii_lowercase();
                assert!(head.contains("upgrade: websocket"), "{head}");
                w.send(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 6\r\nset-cookie: g5_refresh=evil; Path=/\r\nset-cookie: sid=evil; Domain=g5.example\r\nclear-site-data: \"cookies\"\r\n\r\ndenied")
                .await
                .unwrap();
                let _ = r.recv().await;
            });

            let mut parts = parts_with(
                Method::GET,
                "/api/v1/preview/abc/ws",
                &[
                    ("host", "g5.dev"),
                    ("connection", "Upgrade"),
                    ("upgrade", "websocket"),
                    ("sec-websocket-key", "dGhlIHNhbXBsZQ=="),
                ],
            );
            let resp = relay_websocket(
                &mut parts,
                client,
                None,
                "ws",
                RelayPolicy {
                    strip_authorization: true,
                    origin: &TEST_ORIGIN,
                    drop_null_origin: false,
                    frame_ancestors_allow: WORKSPACE_ORIGIN,
                },
            )
            .await
            .unwrap();
            let cookies: Vec<_> = resp.headers().get_all(header::SET_COOKIE).iter().collect();
            assert_eq!(cookies.len(), 1);
            assert_eq!(cookies[0], "sid=evil");
            assert!(resp.headers().get("clear-site-data").is_none());
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
            let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
            assert_eq!(&body[..], b"denied");
        }
    }

    /// [D5]: a 101 handshake passes through with its Connection / Upgrade /
    /// Sec-WebSocket-Accept headers intact.
    #[tokio::test]
    async fn ws_101_response_preserves_handshake_headers() {
        {
            let (client, accepted, _m1, _m2) = mux_pair().await;
            let (w, mut r) = accepted.into_split();
            tokio::spawn(async move {
                let _ = read_full_request(&mut r).await;
                w.send(b"HTTP/1.1 101 Switching Protocols\r\nconnection: Upgrade\r\nupgrade: websocket\r\nsec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\nset-cookie: g5_refresh=evil; Path=/\r\nset-cookie: sid=evil; Domain=g5.example\r\nclear-site-data: \"cookies\"\r\n\r\n")
                .await
                .unwrap();
                let _ = r.recv().await;
            });

            // No browser OnUpgrade extension in this test — the relay must still
            // answer 101 with the handshake headers (and log the missing pipe).
            let mut parts = parts_with(
                Method::GET,
                "/api/v1/preview/abc/ws",
                &[
                    ("host", "g5.dev"),
                    ("connection", "Upgrade"),
                    ("upgrade", "websocket"),
                    ("sec-websocket-key", "dGhlIHNhbXBsZQ=="),
                ],
            );
            let resp = relay_websocket(
                &mut parts,
                client,
                None,
                "ws",
                RelayPolicy {
                    strip_authorization: true,
                    origin: &TEST_ORIGIN,
                    drop_null_origin: false,
                    frame_ancestors_allow: WORKSPACE_ORIGIN,
                },
            )
            .await
            .unwrap();
            let cookies: Vec<_> = resp.headers().get_all(header::SET_COOKIE).iter().collect();
            assert_eq!(cookies.len(), 1);
            assert_eq!(cookies[0], "sid=evil");
            assert!(resp.headers().get("clear-site-data").is_none());
            assert_eq!(resp.status(), StatusCode::SWITCHING_PROTOCOLS);
            assert_eq!(resp.headers().get(header::UPGRADE).unwrap(), "websocket");
            assert_eq!(resp.headers().get(header::CONNECTION).unwrap(), "Upgrade");
            assert_eq!(
                resp.headers().get("sec-websocket-accept").unwrap(),
                "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
            );
        }
    }

    #[test]
    fn sanitize_strips_connection_nominated_headers() {
        // Hop-by-hop sanitization honors Connection tokens ([D3]): a header
        // named by the upstream's Connection must be removed too.
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONNECTION,
            HeaderValue::from_static("close, x-app-hop"),
        );
        headers.insert("x-app-hop", HeaderValue::from_static("secret"));
        headers.insert("x-app-end-to-end", HeaderValue::from_static("keep"));
        sanitize_relay_response_headers(&mut headers, false, WORKSPACE_ORIGIN);
        assert!(headers.get(header::CONNECTION).is_none());
        assert!(headers.get("x-app-hop").is_none());
        assert_eq!(headers.get("x-app-end-to-end").unwrap(), "keep");
    }

    #[test]
    fn isolated_cookie_rewrite_keeps_multiple_host_only_cookies_and_other_attributes() {
        let mut headers = HeaderMap::new();
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static(
                "sid=\"a=b\";  DoMaIn =g5.example; Path=/; DOMAIN=preview.g5.example; HttpOnly; Secure; SameSite=None; Partitioned",
            ),
        );
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static(
                "next=ok; Expires=Wed, 21 Oct 2030 07:28:00 GMT; Max-Age=3600",
            ),
        );
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_str("utf8=café; DOMAIN=g5.example; Path=/").unwrap(),
        );
        sanitize_relay_response_headers(&mut headers, false, WORKSPACE_ORIGIN);
        let cookies: Vec<_> = headers.get_all(header::SET_COOKIE).iter().collect();
        assert_eq!(cookies.len(), 3);
        assert_eq!(
            cookies[0],
            "sid=\"a=b\"; Path=/; HttpOnly; Secure; SameSite=None; Partitioned"
        );
        assert_eq!(
            cookies[1],
            "next=ok; Expires=Wed, 21 Oct 2030 07:28:00 GMT; Max-Age=3600"
        );
        assert_eq!(
            cookies[2],
            HeaderValue::from_str("utf8=café; Path=/").unwrap()
        );
    }

    #[test]
    fn isolated_cookie_rewrite_drops_reserved_and_malformed_headers() {
        let mut headers = HeaderMap::new();
        for cookie in [
            "g5_refresh=evil",
            "G5_REFRESH=evil",
            "__Host-g5_refresh=evil; Secure; Path=/",
            "__Host-g5_github_app_pkce=evil; Secure; Path=/",
            "g5_preview=evil",
            "__Host-g5_preview=evil; Secure; Path=/",
            "__Secure-g5_x=evil; Secure",
            "broken",
            "=empty-name",
            "bad name=value",
            "unclosed=\"value",
        ] {
            headers.append(header::SET_COOKIE, HeaderValue::from_str(cookie).unwrap());
        }
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_bytes(b"nonutf8=\xff").unwrap(),
        );
        headers.append("clear-site-data", HeaderValue::from_static("\"cookies\""));
        sanitize_relay_response_headers(&mut headers, true, WORKSPACE_ORIGIN);
        assert!(headers.get(header::SET_COOKIE).is_none());
        assert!(headers.get("clear-site-data").is_none());
    }

    #[test]
    fn request_strips_client_framing_and_nominated_headers() {
        // [D3]: the relay owns the upstream framing — client framing headers
        // and Connection-nominated hop-by-hop headers never ride along.
        let parts = parts_with(
            Method::POST,
            "/api/v1/preview/abc/upload",
            &[
                ("host", "g5.dev"),
                ("transfer-encoding", "chunked"),
                ("content-length", "999"),
                ("trailer", "x-checksum"),
                ("expect", "100-continue"),
                ("connection", "keep-alive, x-client-hop"),
                ("x-client-hop", "secret"),
                ("x-end-to-end", "keep"),
            ],
        );
        let head = build_head(&parts, "upload", true, true, &TEST_ORIGIN);
        let lower = head.to_ascii_lowercase();
        assert!(!lower.contains("transfer-encoding"), "{head}");
        assert!(!lower.contains("content-length"), "{head}");
        assert!(!lower.contains("trailer"), "{head}");
        assert!(!lower.contains("expect"), "{head}");
        assert!(!lower.contains("x-client-hop"), "{head}");
        assert!(lower.contains("x-end-to-end: keep\r\n"), "{head}");
        assert!(lower.contains("connection: close\r\n"), "{head}");
    }

    /// [D19] under the REAL serving stack: axum/hyper over real TCP with the
    /// production `CompressionLayer`, whose gzip body wrapper polls the inner
    /// body again after end-of-stream — the exact re-poller behind the #807
    /// production panic. hyper's `Incoming` must tolerate it on both framings.
    #[tokio::test]
    async fn real_hyper_stack_serves_relay_bodies_to_completion() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // >32 bytes + text/html so the production DefaultPredicate compresses,
        // exactly like a relayed Vite index.html.
        const PAGE: &[u8] = b"<html><body>preview page body, long enough to compress</body></html>";

        async fn relayed(chunked: bool) -> Response {
            let (client, accepted, m1, m2) = mux_pair().await;
            let (w, mut r) = accepted.into_split();
            tokio::spawn(async move {
                let _ = read_full_request(&mut r).await;
                if chunked {
                    let payload = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ntransfer-encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                        PAGE.len(),
                        std::str::from_utf8(PAGE).unwrap()
                    );
                    w.send(payload.as_bytes()).await.unwrap();
                } else {
                    let payload = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\n\r\n{}",
                        PAGE.len(),
                        std::str::from_utf8(PAGE).unwrap()
                    );
                    w.send(payload.as_bytes()).await.unwrap();
                }
                let _ = r.recv().await;
            });
            // Keep the muxes alive for the response body's lifetime.
            std::mem::forget((m1, m2));
            let parts = super::tests::get_parts("/api/v1/preview/abc/page");
            relay_http(
                parts,
                Body::empty(),
                client,
                None,
                "page",
                RelayPolicy {
                    strip_authorization: true,
                    origin: &TEST_ORIGIN,
                    drop_null_origin: false,
                    frame_ancestors_allow: WORKSPACE_ORIGIN,
                },
            )
            .await
            .unwrap()
        }

        // Both route handlers are deliberately capture-free `||` closures: the
        // route_auth_static scope audit scans route files for `get(<ident>`
        // registrations, and a `get(move ...)` or named handler here would be
        // flagged as an API route without a scope contract.
        let app = axum::Router::new()
            .route(
                "/len",
                axum::routing::get(|| async { relayed(false).await }),
            )
            .route(
                "/chunked",
                axum::routing::get(|| async { relayed(true).await }),
            )
            // The production middleware whose body wrapper drives post-EOF polls.
            .layer(crate::middleware::compression::build_compression_layer());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        for path in ["/len", "/chunked"] {
            let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
            c.write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nhost: t\r\naccept-encoding: gzip\r\nconnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            let mut buf = Vec::new();
            let read = c.read_to_end(&mut buf).await;
            let text = String::from_utf8_lossy(&buf);
            assert!(
                read.is_ok(),
                "{path}: connection aborted mid-response (server task panicked?): {read:?}"
            );
            assert!(text.starts_with("HTTP/1.1 200"), "{path}: {text}");
            assert!(
                text.to_ascii_lowercase().contains("content-encoding: gzip"),
                "{path}: compression did not engage — experiment not exercising the prod path: {text}"
            );
            assert!(
                buf.ends_with(b"0\r\n\r\n"),
                "{path}: response truncated — missing terminal chunk (server task died mid-body)"
            );
        }
    }
}
