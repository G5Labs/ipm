//! User-action audit middleware for the g5-api HTTP processing pipeline.
//!
//! This is the **innermost** Tower middleware layer in the stack:
//! Request ID → CORS → Compression → Rate Limit → Auth → User-Action Audit →
//! Handler (issue G5Labs/G5#1552; `system_invariants/observability_audit` [D5]).
//!
//! For every in-scope completed request by an authenticated end user
//! (`AuthContext::User` only — never worker/host machine identities), it
//! emits exactly one structured INFO event on the `g5_audit` tracing target:
//! all mutating requests (non-GET/HEAD/OPTIONS) plus a curated read
//! allowlist ("workspace opened", "preview opened"). Machine traffic, the
//! preview relay's sub-resource fan-out, and the UI-telemetry ingest route
//! never emit.
//!
//! The layer is telemetry-only: it never rejects, rewrites, or delays a
//! request, performs no I/O, and holds no state. It runs inside the
//! request-id span, so every event carries `request_id` correlation.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::MatchedPath;
use http::{Method, Request, Response, StatusCode};
use tower::{Layer, Service};
use uuid::Uuid;

use g5_auth::{PREVIEW_AGENT_TOKEN_USE, PREVIEW_TOKEN_USE};

use crate::middleware::auth_middleware::{AuthContext, API_KEY_ISSUER};
use crate::middleware::client_ip::ClientIp;

// ---------------------------------------------------------------------------
// Layer / Middleware
// ---------------------------------------------------------------------------

/// Tower [`Layer`] for the user-action audit middleware.
///
/// Apply as the FIRST `.layer()` call in `build_middleware_stack` so it is
/// the innermost layer: it must observe the `AuthContext` the auth layer
/// inserted and the handler's response status.
#[derive(Debug, Clone)]
pub struct UserActionAuditLayer;

impl<S> Layer<S> for UserActionAuditLayer {
    type Service = UserActionAuditMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        UserActionAuditMiddleware { inner }
    }
}

/// Tower [`Service`] that emits one `g5_audit` `user_action.http` event per
/// in-scope authenticated user action.
#[derive(Debug, Clone)]
pub struct UserActionAuditMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for UserActionAuditMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Capture everything BEFORE handing the request to the inner
        // service; only the response status is read afterwards.
        let candidate = AuditCandidate::from_request(&req);

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let response = inner.call(req).await?;
            if let Some(candidate) = candidate {
                candidate.emit(response.status());
            }
            Ok(response)
        })
    }
}

// ---------------------------------------------------------------------------
// Audit candidate — scope rules and field derivation
// ---------------------------------------------------------------------------

/// Pre-computed audit event data for a request that is in scope.
struct AuditCandidate {
    user_id: String,
    org_id: String,
    auth_kind: &'static str,
    /// Route template from `MatchedPath`, when the router provided one;
    /// `None` renders the bounded `<unmatched>` action ([D3]).
    matched_template: Option<String>,
    method: Method,
    path: String,
    workspace_id: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    client_ip: String,
}

impl AuditCandidate {
    /// Returns `Some` iff this request must produce a `user_action.http`
    /// event (`middleware/audit_log` [D2]): authenticated end user, mutating
    /// method or read-allowlisted route, and not on the exclusion list.
    fn from_request(req: &Request<Body>) -> Option<Self> {
        let claims = match req.extensions().get::<AuthContext>() {
            Some(AuthContext::User(claims)) => claims,
            _ => return None,
        };

        let path = req.uri().path();
        if !is_in_scope(req.method(), path) {
            return None;
        }

        let auth_kind = if claims.token_use.as_deref() == Some(PREVIEW_TOKEN_USE) {
            "preview_ticket"
        } else if claims.token_use.as_deref() == Some(PREVIEW_AGENT_TOKEN_USE) {
            // A workspace agent's preview credential acts for the owner but
            // is not the owner's session (`g5.preview.agent-access`).
            "preview_agent"
        } else if claims.iss.as_deref() == Some(API_KEY_ISSUER) {
            "api_key"
        } else {
            "jwt"
        };

        let (resource_type, resource_id) = derive_resource(path);

        Some(Self {
            user_id: claims.sub.clone(),
            org_id: claims.org.clone(),
            auth_kind,
            matched_template: req
                .extensions()
                .get::<MatchedPath>()
                .map(|m| m.as_str().to_string()),
            method: req.method().clone(),
            path: path.to_string(),
            workspace_id: derive_workspace_id(path),
            resource_type,
            resource_id,
            client_ip: ClientIp::from_extensions(req.extensions()).to_string(),
        })
    }

    /// `action` value ([D3]): the `MatchedPath` route template when the
    /// router provided one, else the bounded `<unmatched>` constant — there
    /// is deliberately NO raw-path-derived fallback, so an authenticated
    /// caller probing arbitrary paths cannot mint unbounded or slug-bearing
    /// action cardinality. The tests pin that MatchedPath is present for
    /// every routed request.
    fn action(&self) -> String {
        let template = self.matched_template.as_deref().unwrap_or("<unmatched>");
        truncate_field(&format!("{} {}", self.method, template))
    }

    /// Emits the `g5_audit` event. Always INFO regardless of outcome — the
    /// stream is filtered by target, and `outcome`/`status` carry the
    /// outcome class ([D1]); the API error boundary already logs 4xx/5xx at
    /// warn/error.
    fn emit(&self, status: StatusCode) {
        let outcome = if status.is_server_error() {
            "server_error"
        } else if status.is_client_error() {
            "client_error"
        } else {
            "success"
        };
        tracing::info!(
            target: "g5_audit",
            event = "user_action.http",
            user_id = %self.user_id,
            org_id = %self.org_id,
            auth_kind = %self.auth_kind,
            action = %self.action(),
            method = %self.method,
            path = %truncate_field(&self.path),
            workspace_id = %self.workspace_id.as_deref().unwrap_or(""),
            resource_type = %self.resource_type.as_deref().unwrap_or(""),
            resource_id = %self.resource_id.as_deref().unwrap_or(""),
            status = status.as_u16(),
            outcome = %outcome,
            client_ip = %self.client_ip,
            "User action"
        );
    }
}

/// Scope rules ([D2]): mutating methods are in scope, GET only via the read
/// allowlist, minus the exclusions. HEAD/OPTIONS are never in scope.
///
/// The UUID-gated preview relay subtree is special-cased in full: the root
/// is registered `any(...)` and everything except a root GET ("preview
/// opened") — non-GET root requests included — is the previewed app's own
/// proxied traffic, not a G5 user action (codex reviews 2026-07-29,
/// findings 4/impl-3). The sibling mint route
/// `POST /api/v1/preview/session/:workspaceId` has no UUID after `preview` and
/// therefore emits normally.
fn is_in_scope(method: &Method, path: &str) -> bool {
    if path == "/api/v1/telemetry" || path.starts_with("/api/v1/telemetry/") {
        // The UI-telemetry ingest re-emits its payload as `ui.*` events; a
        // wrapper event per flush would double-count.
        return false;
    }
    if let Some(rest) = path.strip_prefix("/api/v1/preview/") {
        let mut segments = rest.split('/');
        let first = segments.next().unwrap_or("");
        if Uuid::parse_str(first).is_ok() {
            let is_root = segments.all(|s| s.is_empty());
            return *method == Method::GET && is_root;
        }
    }
    match *method {
        Method::GET => is_read_allowlisted(path),
        Method::HEAD | Method::OPTIONS => false,
        _ => true,
    }
}

/// Read allowlist ([D2]): "workspace opened" (the preview-root "preview
/// opened" case is handled inside [`is_in_scope`]) and every cross-tenant
/// platform read (`g5.organization.platform-operator.tenant-boundary`).
fn is_read_allowlisted(path: &str) -> bool {
    if let Some(rest) = path.strip_prefix("/api/v1/workspaces/") {
        return Uuid::parse_str(rest).is_ok();
    }
    path.starts_with("/api/v1/platform/")
}

/// Bounds emitted string fields to 200 chars ([D3]) so a hostile long path
/// cannot balloon the log line.
fn truncate_field(value: &str) -> String {
    if value.len() <= 200 {
        return value.to_string();
    }
    let mut end = 200;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// `workspace_id` is the UUID segment immediately following a `workspaces`
/// segment ([D4]).
fn derive_workspace_id(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').collect();
    segments.windows(2).find_map(|pair| {
        if pair[0] == "workspaces" && Uuid::parse_str(pair[1]).is_ok() {
            Some(pair[1].to_string())
        } else {
            None
        }
    })
}

/// (`resource_type`, `resource_id`) is the LAST (collection, UUID) adjacent
/// segment pair in the path ([D4]). Collection names are recorded verbatim.
fn derive_resource(path: &str) -> (Option<String>, Option<String>) {
    let segments: Vec<&str> = path.split('/').collect();
    let last = segments.windows(2).rev().find(|pair| {
        !pair[0].is_empty() && Uuid::parse_str(pair[0]).is_err() && Uuid::parse_str(pair[1]).is_ok()
    });
    match last {
        Some(pair) => (Some(pair[0].to_string()), Some(pair[1].to_string())),
        None => (None, None),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::test_capture::{captured_g5_audit_events, CapturedEvent};
    use axum::routing::{any, get, post};
    use axum::Router;
    use g5_auth::{JwtClaims, Scope};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;

    const USER_ID: &str = "6f0c2f7e-8f5a-4d2b-9c3e-2b1a4d5e6f70";
    const ORG_ID: &str = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    const WS_ID: &str = "11111111-2222-4333-8444-555555555555";
    const PREVIEW_WORKSPACE_ID: &str = "99999999-8888-4777-a666-555555555555";

    fn user_claims() -> JwtClaims {
        JwtClaims {
            sub: USER_ID.to_string(),
            org: ORG_ID.to_string(),
            scopes: vec![Scope::WorkspacesRead, Scope::WorkspacesWrite],
            iat: 0,
            exp: i64::MAX,
            iss: Some("g5-api".to_string()),
            aud: Some("g5".to_string()),
            jti: None,
            token_use: None,
            client: None,
        }
    }

    /// Real nested router (mirrors build_router's `.nest()` shape) with the
    /// audit layer applied via `Router::layer`, exactly like production.
    fn test_router() -> Router {
        async fn ok() -> &'static str {
            "ok"
        }
        async fn not_found() -> (StatusCode, &'static str) {
            (StatusCode::NOT_FOUND, "nope")
        }
        Router::new()
            .nest(
                "/api/v1/workspaces",
                Router::new()
                    .route("/:workspaceId", get(ok).delete(ok))
                    .route("/:workspaceId/tasks", post(ok).get(ok))
                    .route("/:workspaceId/missing", post(not_found)),
            )
            .nest(
                "/api/v1/preview",
                Router::new()
                    .route("/session/:workspaceId", post(ok))
                    .route("/:workspaceId", any(ok))
                    .route("/:workspaceId/", any(ok))
                    .route("/:workspaceId/*path", any(ok)),
            )
            .nest(
                "/api/v1/telemetry",
                Router::new().route("/ui-events", post(ok)),
            )
            .nest("/api/v1/platform", Router::new().route("/orgs", get(ok)))
            .layer(UserActionAuditLayer)
    }

    async fn run(
        method: Method,
        path: &str,
        auth: Option<AuthContext>,
    ) -> (StatusCode, Vec<CapturedEvent>) {
        let (subscriber, events) = crate::middleware::test_capture::capture_subscriber();
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(auth) = auth {
            builder = builder.extension(auth);
        }
        let req = builder.body(Body::empty()).unwrap();
        let response = async { test_router().oneshot(req).await.unwrap() }
            .with_subscriber(subscriber)
            .await;
        (response.status(), captured_g5_audit_events(&events))
    }

    fn user() -> Option<AuthContext> {
        Some(AuthContext::User(user_claims()))
    }

    #[tokio::test]
    async fn mutating_post_emits_one_event_with_template_action() {
        let (status, events) = run(
            Method::POST,
            &format!("/api/v1/workspaces/{WS_ID}/tasks"),
            user(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.len(), 1, "exactly one audit event: {events:?}");
        let e = &events[0];
        assert_eq!(e.field("event"), Some("user_action.http"));
        assert_eq!(e.field("user_id"), Some(USER_ID));
        assert_eq!(e.field("org_id"), Some(ORG_ID));
        assert_eq!(e.field("auth_kind"), Some("jwt"));
        assert_eq!(e.field("workspace_id"), Some(WS_ID));
        assert_eq!(e.field("resource_type"), Some("workspaces"));
        assert_eq!(e.field("resource_id"), Some(WS_ID));
        assert_eq!(e.field("outcome"), Some("success"));
        assert_eq!(e.field("status"), Some("200"));
        // PINNED ([D3], codex impl finding 4): MatchedPath must be present
        // in Router::layer middleware over the nested production router
        // shape — the action is the exact route template, not the sanitizer
        // fallback and never the raw path.
        assert_eq!(
            e.field("action"),
            Some("POST /api/v1/workspaces/:workspaceId/tasks"),
            "action must be the MatchedPath route template"
        );
    }

    #[tokio::test]
    async fn get_poll_route_emits_nothing() {
        let (status, events) = run(
            Method::GET,
            &format!("/api/v1/workspaces/{WS_ID}/tasks"),
            user(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(events.is_empty(), "poll GET must not emit: {events:?}");
    }

    #[tokio::test]
    async fn workspace_open_get_is_allowlisted() {
        let (status, events) =
            run(Method::GET, &format!("/api/v1/workspaces/{WS_ID}"), user()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].field("workspace_id"), Some(WS_ID));
    }

    #[tokio::test]
    async fn platform_read_is_allowlisted() {
        // Every cross-tenant platform read is attributable
        // (g5.organization.platform-operator.tenant-boundary).
        let (status, events) = run(Method::GET, "/api/v1/platform/orgs", user()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.len(), 1, "platform read must emit: {events:?}");
        assert_eq!(events[0].field("action"), Some("GET /api/v1/platform/orgs"));
    }

    #[tokio::test]
    async fn preview_root_get_is_allowlisted() {
        let (status, events) = run(
            Method::GET,
            &format!("/api/v1/preview/{PREVIEW_WORKSPACE_ID}"),
            user(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.len(), 1, "preview root open must emit: {events:?}");
    }

    #[tokio::test]
    async fn preview_subresources_emit_nothing_any_method() {
        for method in [Method::GET, Method::POST] {
            let (status, events) = run(
                method.clone(),
                &format!("/api/v1/preview/{PREVIEW_WORKSPACE_ID}/assets/app.js"),
                user(),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(
                events.is_empty(),
                "preview sub-resource must not emit for {method}: {events:?}"
            );
        }
    }

    #[tokio::test]
    async fn preview_root_non_get_emits_nothing() {
        // codex impl review finding 3: the root is registered any(...) —
        // a POST/PUT to the UUID root is the previewed app's own proxied
        // traffic, not a G5 user action.
        for method in [Method::POST, Method::PUT, Method::DELETE] {
            let (status, events) = run(
                method.clone(),
                &format!("/api/v1/preview/{PREVIEW_WORKSPACE_ID}"),
                user(),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(
                events.is_empty(),
                "non-GET preview root must not emit for {method}: {events:?}"
            );
        }
    }

    #[tokio::test]
    async fn unmatched_route_uses_bounded_action() {
        // codex impl review finding 4: an authenticated caller probing
        // arbitrary paths must not mint unbounded action cardinality.
        let long_slug = "x".repeat(400);
        let (status, events) = run(
            Method::POST,
            &format!("/api/v1/definitely-not-a-route/{long_slug}"),
            user(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e.field("action"), Some("POST <unmatched>"));
        assert!(
            e.field("path").unwrap().len() <= 200,
            "path field must be truncated"
        );
    }

    #[tokio::test]
    async fn preview_session_mint_emits() {
        // Regression for codex review finding 4: the mint route is a real
        // user action and must NOT be swallowed by the relay exclusion.
        let (status, events) = run(
            Method::POST,
            &format!("/api/v1/preview/session/{PREVIEW_WORKSPACE_ID}"),
            user(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(events.len(), 1, "mint must emit: {events:?}");
        assert_eq!(events[0].field("resource_id"), Some(PREVIEW_WORKSPACE_ID));
        assert_eq!(events[0].field("resource_type"), Some("session"));
    }

    #[tokio::test]
    async fn telemetry_ingest_emits_nothing() {
        let (status, events) = run(Method::POST, "/api/v1/telemetry/ui-events", user()).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            events.is_empty(),
            "telemetry ingest must not emit: {events:?}"
        );
    }

    #[tokio::test]
    async fn host_and_missing_auth_emit_nothing() {
        let path = format!("/api/v1/workspaces/{WS_ID}/tasks");
        let host = AuthContext::Host {
            host_id: Uuid::parse_str(PREVIEW_WORKSPACE_ID).unwrap(),
            org_id: Uuid::parse_str(ORG_ID).unwrap(),
            scopes: vec![Scope::WorkerHost],
        };
        let (_, events) = run(Method::POST, &path, Some(host)).await;
        assert!(events.is_empty(), "host auth must not emit: {events:?}");

        let (_, events) = run(Method::POST, &path, None).await;
        assert!(
            events.is_empty(),
            "unauthenticated must not emit: {events:?}"
        );
    }

    #[tokio::test]
    async fn outcome_classes_follow_status() {
        let (status, events) = run(
            Method::POST,
            &format!("/api/v1/workspaces/{WS_ID}/missing"),
            user(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].field("outcome"), Some("client_error"));
        assert_eq!(events[0].field("status"), Some("404"));
        // D1: audit events stay INFO for every outcome class.
        assert_eq!(events[0].level, "INFO");
    }

    #[tokio::test]
    async fn api_key_preview_ticket_and_preview_agent_auth_kinds() {
        let mut claims = user_claims();
        claims.iss = Some(API_KEY_ISSUER.to_string());
        let (_, events) = run(
            Method::POST,
            &format!("/api/v1/workspaces/{WS_ID}/tasks"),
            Some(AuthContext::User(claims)),
        )
        .await;
        assert_eq!(events.len(), 1, "api-key action must emit once: {events:?}");
        assert_eq!(events[0].field("auth_kind"), Some("api_key"));

        let mut claims = user_claims();
        claims.token_use = Some(PREVIEW_TOKEN_USE.to_string());
        let (_, events) = run(
            Method::GET,
            &format!("/api/v1/preview/{PREVIEW_WORKSPACE_ID}"),
            Some(AuthContext::User(claims)),
        )
        .await;
        assert_eq!(
            events.len(),
            1,
            "preview-ticket open must emit once: {events:?}"
        );
        assert_eq!(events[0].field("auth_kind"), Some("preview_ticket"));

        let mut claims = user_claims();
        claims.token_use = Some(PREVIEW_AGENT_TOKEN_USE.to_string());
        let (_, events) = run(
            Method::GET,
            &format!("/api/v1/preview/{PREVIEW_WORKSPACE_ID}"),
            Some(AuthContext::User(claims)),
        )
        .await;
        assert_eq!(
            events.len(),
            1,
            "agent preview open must emit once: {events:?}"
        );
        assert_eq!(events[0].field("auth_kind"), Some("preview_agent"));
    }

    #[test]
    fn resource_derivation_takes_last_collection_uuid_pair() {
        let (rt, rid) = derive_resource(&format!(
            "/api/v1/workspaces/{WS_ID}/tasks/{PREVIEW_WORKSPACE_ID}"
        ));
        assert_eq!(rt.as_deref(), Some("tasks"));
        assert_eq!(rid.as_deref(), Some(PREVIEW_WORKSPACE_ID));

        let (rt, rid) = derive_resource("/api/v1/orgs");
        assert!(rt.is_none() && rid.is_none());
    }
}
