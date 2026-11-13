//! Per-preview subdomain host translation (issue G5Labs/G5#935).
//!
//! Outermost layer on the composed app (`middleware/preview_host` [D1]/[D2],
//! `routes/preview` [I3]). When `G5_PREVIEW_HOST_SUFFIX` is configured,
//! requests whose `Host` is `<workspace-uuid>.<suffix>` are:
//!
//! 1. **Handoff exchange** — a request carrying the single-use
//!    `__g5_preview_ticket` query parameter is terminated here: the ticket is
//!    validated (workspace-bound, `token_use: "preview_handoff"`), its `jti` is
//!    consumed exactly once, a 30-minute preview session ticket is minted and
//!    set as the origin-scoped `__Host-g5_preview` cookie, and the browser is
//!    redirected (`303`) to the same URL minus the ticket parameter.
//! 2. **Translation** — every other preview-origin request is rewritten to
//!    the legacy route shape `/api/v1/preview/<workspace-id><original path>` and
//!    passed through the FULL normal middleware stack (request-id, CORS,
//!    compression, rate limit, auth — whose ticket-cookie carve-out then
//!    applies verbatim) and the existing preview relay. One data path, no
//!    parallel proxy.
//!
//! Non-matching hosts pass through untouched; with no suffix configured the
//! layer is a pure pass-through.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use http::{header, StatusCode, Uri};
use uuid::Uuid;

use crate::middleware::auth_middleware::PREVIEW_TICKET_COOKIE_HOST;
use crate::services::auth_service::PREVIEW_TICKET_LIFETIME_SECS;
use crate::services::AppState;

/// Query parameter carrying the single-use preview handoff ticket
/// (`routes/preview` [D15]). Namespaced so it cannot collide with a
/// previewed app's own query parameters.
pub const PREVIEW_HANDOFF_TICKET_PARAM: &str = "__g5_preview_ticket";

/// Process-local consumed-`jti` set for single-use handoff tickets
/// (`middleware/preview_host` [D2]).
///
/// Process-local is sound here, not a durability shortcut: the preview data
/// path is already pinned to the g5-api instance holding the host's tunnel
/// carrier (`routes/preview` [D7]), and hosted environments run a single API
/// replica precisely for this class of process-local preview state
/// (`deployment_model/gcloud_hosting_environment` [D7]). A restart empties
/// the set but also drops every carrier, and an unconsumed ticket dies at its
/// 60-second `exp`. Entries store the ticket `exp` and are pruned
/// opportunistically on each insert, so the set stays bounded by the number
/// of handoffs minted per minute.
pub struct ConsumedJtiStore {
    consumed: Mutex<HashMap<String, i64>>,
}

impl ConsumedJtiStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            consumed: Mutex::new(HashMap::new()),
        }
    }

    /// Records `jti` as consumed. Returns `true` when this call consumed it
    /// (first use), `false` when it was already consumed (replay).
    ///
    /// `exp` is the ticket's expiry (unix seconds); entries past expiry are
    /// pruned on each call, so replay of an expired jti is irrelevant — the
    /// signature check already rejects the expired ticket itself.
    pub fn consume(&self, jti: &str, exp: i64, now: i64) -> bool {
        let mut consumed = self
            .consumed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        consumed.retain(|_, entry_exp| *entry_exp > now);
        consumed.insert(jti.to_string(), exp).is_none()
    }
}

impl Default for ConsumedJtiStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Request extension marking a request that arrived on a per-preview
/// subdomain origin and was translated to the legacy route shape.
///
/// Consumed by `routes/preview`: suppresses the legacy-path redirect
/// (`routes/preview` [D14]) and drives the forwarded `Host` /
/// `X-Forwarded-*` shaping (`routes/preview` [D13]).
#[derive(Clone, Debug)]
pub struct PreviewHostOrigin {
    /// Workspace id parsed from the first host label.
    pub workspace_id: Uuid,
    /// The original `<workspace-id>.<suffix>` host (lowercased, port stripped).
    pub host: String,
}

/// Parses `host` as `<workspace-uuid>.<suffix>` (case-insensitive; `:port`
/// stripped). Returns the workspace id only on an exact match — the remainder
/// after the first label must equal `suffix` exactly, so an embedded-suffix
/// host such as `x.preview.dev.g5labs.ai.evil.com` never matches.
#[must_use]
pub fn parse_preview_host(host: &str, suffix: &str) -> Option<Uuid> {
    let host = host.trim().to_ascii_lowercase();
    let host = host.split(':').next().unwrap_or("");
    let (first_label, rest) = host.split_once('.')?;
    if rest != suffix {
        return None;
    }
    first_label.parse::<Uuid>().ok()
}

/// The request's host: prefer the `Host` header, fall back to the URI
/// authority (HTTP/2 `:authority`).
fn request_host(req: &Request) -> Option<String> {
    if let Some(value) = req.headers().get(header::HOST) {
        if let Ok(s) = value.to_str() {
            return Some(s.to_string());
        }
    }
    req.uri().host().map(str::to_string)
}

/// Outermost middleware entry (`axum::middleware::from_fn_with_state`).
pub async fn preview_host_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(suffix) = state.preview_host_suffix.as_deref() else {
        return next.run(req).await;
    };
    let Some(host) = request_host(&req) else {
        return next.run(req).await;
    };
    let Some(workspace_id) = parse_preview_host(&host, suffix) else {
        return next.run(req).await;
    };
    let normalized_host = {
        let lower = host.trim().to_ascii_lowercase();
        lower.split(':').next().unwrap_or("").to_string()
    };

    // 1. Handoff exchange: consume the single-use ticket, set the
    //    origin-scoped session cookie, redirect to the clean URL.
    if let Some(ticket) = extract_handoff_ticket(req.uri()) {
        return handoff_exchange(&state, workspace_id, req.uri(), &ticket).await;
    }

    // 2. Translation: rewrite to the legacy route shape and continue through
    //    the full normal stack. The auth middleware's ticket-cookie carve-out
    //    authenticates via the origin cookie; the relay strips the injected
    //    prefix back off, so the app sees the original path.
    let mut req = req;
    let path = req.uri().path();
    let mut translated = format!("/api/v1/preview/{workspace_id}{path}");
    if let Some(q) = req.uri().query() {
        translated.push('?');
        translated.push_str(q);
    }
    match rewrite_uri(req.uri(), &translated) {
        Some(uri) => *req.uri_mut() = uri,
        None => {
            return plain_response(
                StatusCode::BAD_REQUEST,
                "preview request path could not be translated",
            )
        }
    }
    req.extensions_mut().insert(PreviewHostOrigin {
        workspace_id,
        host: normalized_host,
    });
    next.run(req).await
}

/// Extracts the handoff ticket value from the query string, if present.
fn extract_handoff_ticket(uri: &Uri) -> Option<String> {
    let query = uri.query()?;
    for pair in query.split('&') {
        // The redirect preserves the original query and appends the ticket, so
        // a preceding valueless flag (e.g. `?import&__g5_preview_ticket=...`)
        // must be skipped rather than aborting the whole scan.
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        if name == PREVIEW_HANDOFF_TICKET_PARAM && !value.is_empty() {
            // Query values are percent-encoded by the client; JWTs are
            // URL-safe (base64url + '.'), so no decoding is required.
            return Some(value.to_string());
        }
    }
    None
}

/// Validates + consumes the handoff ticket, mints the session cookie, and
/// redirects to the same URL minus the ticket parameter
/// (`middleware/preview_host` [D2]).
async fn handoff_exchange(
    state: &Arc<AppState>,
    workspace_id: Uuid,
    uri: &Uri,
    ticket: &str,
) -> Response {
    // Validate: signature, expiry, token_use == "preview_handoff", workspace-bound
    // aud. Fails closed when no validator is configured.
    let Some(validator) = state.jwt_validator.as_deref() else {
        tracing::warn!(%workspace_id, "preview handoff rejected: no JWT validator configured");
        return handoff_unauthorized();
    };
    let claims = match validator.validate_preview_handoff_ticket(ticket, workspace_id) {
        Ok(claims) => claims,
        Err(_) => {
            tracing::warn!(%workspace_id, "preview handoff rejected: invalid or expired ticket");
            return handoff_unauthorized();
        }
    };

    // Single use: consume the jti exactly once (replays fail closed).
    let now = chrono::Utc::now().timestamp();
    let Some(jti) = claims.jti.as_deref() else {
        tracing::warn!(%workspace_id, "preview handoff rejected: ticket has no jti");
        return handoff_unauthorized();
    };
    if !state.preview_handoff_jtis.consume(jti, claims.exp, now) {
        tracing::warn!(%workspace_id, "preview handoff rejected: ticket already consumed (replay)");
        return handoff_unauthorized();
    }

    // Mint the 30-minute session ticket for the handoff's identity.
    let (Ok(user_id), Ok(org_id)) = (claims.sub.parse::<Uuid>(), claims.org.parse::<Uuid>()) else {
        tracing::warn!(%workspace_id, "preview handoff rejected: non-UUID sub/org claims");
        return handoff_unauthorized();
    };
    let session_ticket = match state
        .auth_service
        .mint_preview_ticket(user_id, org_id, workspace_id)
    {
        Ok(ticket) => ticket,
        Err(e) => {
            tracing::error!(%workspace_id, error = ?e, "preview handoff: session ticket mint failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Preview session could not be established. Reopen the preview from G5.",
            );
        }
    };

    let cookie = build_host_session_cookie(&session_ticket);

    let location = strip_handoff_ticket_param(uri);
    tracing::info!(%workspace_id, user_id = %user_id, "preview handoff consumed; session cookie set");
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, location)
        .header(header::SET_COOKIE, cookie)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(|_| plain_response(StatusCode::INTERNAL_SERVER_ERROR, "redirect failed"))
}

/// Builds the preview-host session cookie string (`routes/preview`
/// [D15]/[D17]) — the ONE builder shared by the handoff exchange and the
/// relay's sliding ticket renewal, so the two emission points cannot drift.
///
/// Origin-scoped: host-only (no `Domain`), `Path=/`, `__Host-` and therefore
/// `Secure`; preview hosts are HTTPS-only (startup rejects a plain-HTTP
/// platform origin with a preview suffix), so there is no insecure variant.
/// `SameSite=Lax`, not `Strict`: the handoff redirect chain originates
/// cross-site at the main origin, and Strict cookies are not attached to
/// cross-site-initiated navigations — Strict would loop the handoff.
#[must_use]
pub fn build_host_session_cookie(session_ticket: &str) -> String {
    format!(
        "{PREVIEW_TICKET_COOKIE_HOST}={session_ticket}; HttpOnly; Secure; Path=/; \
         SameSite=Lax; Max-Age={PREVIEW_TICKET_LIFETIME_SECS}"
    )
}

/// The same path + query with the `__g5_preview_ticket` pair removed.
fn strip_handoff_ticket_param(uri: &Uri) -> String {
    let path = uri.path();
    let Some(query) = uri.query() else {
        return path.to_string();
    };
    let remaining: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            pair.split_once('=')
                .is_none_or(|(name, _)| name != PREVIEW_HANDOFF_TICKET_PARAM)
        })
        .collect();
    if remaining.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", remaining.join("&"))
    }
}

/// Minimal HTML 401 for a browser navigation whose ticket is invalid,
/// expired, or replayed. A navigation (not an API client) is the consumer,
/// so JSON would render as noise.
fn handoff_unauthorized() -> Response {
    let body = "<!doctype html><html><head><title>Preview unavailable</title></head>\
                <body><p>This preview link is invalid or has expired. \
                Reopen the preview from G5.</p></body></html>";
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap_or_else(|_| plain_response(StatusCode::UNAUTHORIZED, "unauthorized"))
}

fn plain_response(status: StatusCode, message: &'static str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message))
        .expect("static response construction cannot fail")
}

/// Rebuilds the request URI with a new path-and-query, preserving scheme and
/// authority. Returns `None` on a malformed result so the caller can fail
/// closed as a 400 rather than panicking.
fn rewrite_uri(original: &Uri, path_and_query: &str) -> Option<Uri> {
    let mut parts = original.clone().into_parts();
    parts.path_and_query = Some(path_and_query.parse().ok()?);
    Uri::from_parts(parts).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUFFIX: &str = "preview.dev.g5labs.ai";

    #[test]
    fn parse_preview_host_matches_workspace_subdomains_only() {
        let workspace = Uuid::new_v4();
        assert_eq!(
            parse_preview_host(&format!("{workspace}.{SUFFIX}"), SUFFIX),
            Some(workspace)
        );
        // Case-insensitive and port-stripped.
        assert_eq!(
            parse_preview_host(&format!("{}.PREVIEW.dev.G5labs.ai:443", workspace), SUFFIX),
            Some(workspace)
        );
        // Non-UUID first label.
        assert_eq!(parse_preview_host(&format!("app.{SUFFIX}"), SUFFIX), None);
        // Bare suffix, no workspace label.
        assert_eq!(parse_preview_host(SUFFIX, SUFFIX), None);
        // Main origin.
        assert_eq!(parse_preview_host("dev.g5labs.ai", SUFFIX), None);
        // Embedded-suffix spoofing: suffix must be the exact remainder.
        assert_eq!(
            parse_preview_host(&format!("{workspace}.{SUFFIX}.evil.com"), SUFFIX),
            None
        );
        // Extra label between workspace and suffix.
        assert_eq!(
            parse_preview_host(&format!("{workspace}.x.{SUFFIX}"), SUFFIX),
            None
        );
    }

    #[test]
    fn consumed_jti_store_is_single_use_and_prunes_expired() {
        let store = ConsumedJtiStore::new();
        let now = 1_000;
        assert!(store.consume("a", now + 60, now), "first use consumes");
        assert!(!store.consume("a", now + 60, now), "replay rejected");
        // A different jti is independent.
        assert!(store.consume("b", now + 60, now));
        // After expiry the entry is pruned (the ticket itself is dead by
        // signature expiry, so re-consumption of the jti is moot — this only
        // checks the set stays bounded).
        assert!(store.consume("c", now + 1, now));
        assert!(store.consume("d", now + 60, now + 30));
        let len = store
            .consumed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        assert_eq!(len, 3, "expired entry pruned: {len}");
    }

    #[test]
    fn host_session_cookie_shape() {
        // The one shared builder for handoff exchange and [D17] renewal.
        let cookie = build_host_session_cookie("tkt");
        assert!(
            cookie.starts_with("__Host-g5_preview=tkt; HttpOnly; Secure; Path=/; "),
            "{cookie}"
        );
        assert!(cookie.contains("SameSite=Lax"), "{cookie}");
        assert!(cookie.contains("Max-Age=1800"), "{cookie}");
    }

    #[test]
    fn extract_and_strip_handoff_ticket_param() {
        let uri: Uri = "/assets/app.js?v=2&__g5_preview_ticket=tkt123&x=1"
            .parse()
            .unwrap();
        assert_eq!(extract_handoff_ticket(&uri).as_deref(), Some("tkt123"));
        assert_eq!(strip_handoff_ticket_param(&uri), "/assets/app.js?v=2&x=1");

        let only: Uri = "/?__g5_preview_ticket=tkt123".parse().unwrap();
        assert_eq!(strip_handoff_ticket_param(&only), "/");

        let none: Uri = "/index.html?a=b".parse().unwrap();
        assert_eq!(extract_handoff_ticket(&none), None);
        assert_eq!(strip_handoff_ticket_param(&none), "/index.html?a=b");
    }

    #[test]
    fn extract_handoff_ticket_skips_valueless_segments() {
        // A preserved bare flag (`?import`) before the appended ticket must not
        // abort extraction — the ticket still has to be found and stripped.
        let uri: Uri = "/chunk.js?import&__g5_preview_ticket=tkt123"
            .parse()
            .unwrap();
        assert_eq!(extract_handoff_ticket(&uri).as_deref(), Some("tkt123"));
        assert_eq!(strip_handoff_ticket_param(&uri), "/chunk.js?import");
    }

    #[test]
    fn rewrite_uri_preserves_query() {
        let uri: Uri = "/chunk-XYZ.js?import".parse().unwrap();
        let workspace = Uuid::new_v4();
        let rewritten = rewrite_uri(
            &uri,
            &format!("/api/v1/preview/{workspace}/chunk-XYZ.js?import"),
        )
        .expect("rewrite");
        assert_eq!(
            rewritten.path(),
            format!("/api/v1/preview/{workspace}/chunk-XYZ.js")
        );
        assert_eq!(rewritten.query(), Some("import"));
    }

    /// Routing-semantics regression for the [D1] outer-layer mechanism
    /// constraint (issue G5Labs/G5#1123): axum resolves the matched route
    /// BEFORE `Router::layer` middleware runs, so a URI-rewriting layer
    /// attached via `Router::layer` cannot change which route matches — the
    /// translated request falls through to the 404 fallback exactly as dev's
    /// first live subdomain environment showed. The rewrite layer must wrap
    /// the finished router as an OUTER tower service (the `main.rs` shape),
    /// where the identical rewrite demonstrably routes.
    #[tokio::test]
    async fn uri_rewrite_routes_only_when_layered_outside_the_router() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::routing::any;
        use axum::Router;
        use tower::util::MapRequestLayer;
        use tower::{Layer as _, ServiceExt as _};

        let workspace = Uuid::new_v4();

        fn preview_router() -> Router {
            Router::new().nest(
                "/api/v1/preview",
                Router::new()
                    .route("/:workspaceId/", any(|| async { "proxied" }))
                    .route("/:workspaceId/*path", any(|| async { "proxied" })),
            )
        }
        // The same translation the production middleware performs for a
        // preview-origin request (host parsing elided — this test pins the
        // layer-attachment semantics, not detection).
        let rewrite = move |mut req: Request<Body>| {
            let translated = format!("/api/v1/preview/{workspace}{}", req.uri().path());
            if let Some(uri) = rewrite_uri(req.uri(), &translated) {
                *req.uri_mut() = uri;
            }
            req
        };
        let request = || Request::builder().uri("/").body(Body::empty()).unwrap();

        // Attached via Router::layer: the route was already resolved on the
        // ORIGINAL path ("/", no match), so the rewrite is invisible to
        // routing and the request lands on the 404 fallback.
        let inner_attached = preview_router().layer(MapRequestLayer::new(rewrite));
        let resp = inner_attached.oneshot(request()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "Router::layer attachment must NOT route the rewritten URI — if \
             this ever starts passing, axum's layer-vs-routing order changed"
        );

        // Wrapped around the finished router (the main.rs shape): the rewrite
        // runs before routing and the translated URI matches the proxy route.
        let outer_wrapped = MapRequestLayer::new(rewrite).layer(preview_router());
        let resp = outer_wrapped.oneshot(request()).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "outer-wrapped rewrite must reach the preview route"
        );
    }
}
