//! Authentication and authorization middleware for `g5-api`.
//!
//! This is the **innermost** middleware in the Tower stack (closest to the
//! route handler), positioned after Request ID, CORS, Compression, and Rate
//! Limit middleware. Its responsibilities are:
//!
//! 1. **JWT validation** for public API routes (`/api/v1/`) — extracts
//!    `Authorization: Bearer <jwt>` headers, validates using [`JwtValidator`]
//!    from `g5-common`, and stores [`JwtClaims`] in request extensions.
//! 2. **Internal credential validation** — workspace-incarnation JWTs are
//!    accepted on the in-guest API surface, while host JWTs are confined to
//!    the host control surface.
//! 3. **Path exemption** — certain auth-related and health endpoints bypass
//!    authentication entirely.
//! 4. **Scope enforcement** — an optional per-route middleware layer
//!    ([`RequireScopes`]) that checks JWT scopes against required scopes for
//!    the endpoint.
//!
//! This middleware is the security boundary of `g5-api`. Every request that
//! reaches a route handler has been authenticated (or explicitly exempted).
//! Route handlers extract the authenticated identity via the [`AuthContext`]
//! enum stored in request extensions.

use std::borrow::Cow;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{FromRequestParts, OriginalUri};
use http::request::Parts;
use http::{header, Request, Response};
use tower::{Layer, Service};

use futures_util::future::BoxFuture;

use g5_auth::Scope;
use g5_auth::{
    has_scopes, JwtClaims, JwtValidator, PreviewAgentClaims, WorkspaceIncarnationClaims,
    SESSION_CLIENT_BROWSER,
};
use g5_contracts::error::G5Error;
use sqlx::PgPool;

use crate::auth::host_validator::{HostAuthError, HostAuthValidator};
use crate::errors::{AppError, ErrorEnvelope};
use crate::middleware::preview_host::PreviewHostOrigin;
use crate::middleware::request_id::RequestId;
use crate::services::api_key_service::ApiKeyService;

/// Issuer claim value stamped on JWT claims synthesized for API-key auth
/// (`g5k_` keys). Free discriminator between API-key traffic and browser JWT
/// sessions — consumed by the user-action audit layer (`auth_kind`) and the
/// person-session account routes, and UI-telemetry ingest.
pub const API_KEY_ISSUER: &str = "g5-api-key";

// ---------------------------------------------------------------------------
// AuthContext — authenticated identity enum
// ---------------------------------------------------------------------------

/// Represents the authenticated identity for a request.
///
/// Stored in axum request extensions by the auth middleware and extracted
/// by route handlers.
///
/// # Variants
///
/// - `User(JwtClaims)` — Person session or organization API key (distinguished
///   by [`API_KEY_ISSUER`]). Contains the validated
///   claims (sub, org, scopes, iat, exp, iss, aud, jti).
/// - `Host` — Authenticated worker host via per-host JWT (rev-7 §5.7).
///   Identifies a `worker_hosts` row by its UUID; the org is the host's
///   owning org. Carries `Scope::WorkerHost`.
#[derive(Debug, Clone)]
pub enum AuthContext {
    /// Authenticated user via JWT. Contains the validated claims.
    User(JwtClaims),

    /// Authenticated worker host via per-host JWT (`host_jwt`, rev-7 §5.7).
    ///
    /// Identifies a row in `worker_hosts` by `host_id`; the `org_id` is the
    /// host's owning organisation. Stored as typed [`uuid::Uuid`] — the
    /// middleware parses both fields out of the JWT claims (which are
    /// `String` on the wire) once, here, so handlers and the
    /// [`AuthenticatedHost`](crate::middleware::AuthenticatedHost) extractor
    /// can use them without re-parsing.
    ///
    /// The token's scope set carries `Scope::WorkerHost` — the discriminant
    /// the middleware uses to route the request here instead of
    /// [`User`](Self::User). Per `g5/g5-api/worker_hosts` [D4], a host-scoped
    /// JWT MUST NOT be accepted on a user-only endpoint, and a user JWT MUST
    /// NOT be accepted on a host-only endpoint — the path-specific
    /// [`RequireScopes`] layer performs the second half of the check.
    Host {
        host_id: uuid::Uuid,
        org_id: uuid::Uuid,
        // Host JWTs deliberately carry no OAuth-style user scopes; this
        // vector contains only the routing-discriminant `Scope::WorkerHost`
        // (and any future host-only scopes). User-facing `RequireScopes`
        // layers will therefore Forbid host requests automatically.
        scopes: Vec<Scope>,
    },
}

/// Typed workspace credential admitted only on the narrow in-guest API
/// surfaces that recheck the current incarnation against durable state.
#[derive(Debug, Clone)]
pub struct WorkspaceIncarnationContext(pub WorkspaceIncarnationClaims);

/// A workspace agent's preview credential (`g5.preview.agent-access`),
/// admitted beside a request-local [`AuthContext::User`] for its owner on the
/// one preview origin it names. The preview relay must recheck these claims
/// against durable state before relaying; the user context alone grants
/// nothing more than the owner's own preview authority.
#[derive(Debug, Clone)]
pub struct PreviewAgentContext(pub PreviewAgentClaims);

impl AuthContext {
    /// Returns the actor ID — user ID for User or host ID for Host.
    ///
    /// This is the canonical identity for audit, capability minting,
    /// and logging.
    ///
    /// Returned as `Cow<'_, str>` because [`Host`](Self::Host) stores
    /// its `host_id` as a typed `Uuid` (the wire form is a string but
    /// downstream code wants the parsed UUID): User borrows its stored
    /// `String` zero-cost, while Host allocates the
    /// hyphenated form on demand. Call `.as_deref()` to get `&str`.
    #[must_use]
    pub fn actor_id(&self) -> Option<Cow<'_, str>> {
        match self {
            AuthContext::User(claims) => Some(Cow::Borrowed(&claims.sub)),
            AuthContext::Host { host_id, .. } => Some(Cow::Owned(host_id.to_string())),
        }
    }

    /// Returns the user ID (`sub` claim) if this is a User context.
    /// Returns `None` for Host contexts.
    #[must_use]
    pub fn user_id(&self) -> Option<&str> {
        match self {
            AuthContext::User(claims) => Some(&claims.sub),
            _ => None,
        }
    }

    /// Returns the host ID if this is a Host context (rev-7 §5.7).
    ///
    /// The variant stores `host_id` as a typed `Uuid` so handlers can use it
    /// directly via [`host_id`](Self::host_id); this accessor returns the
    /// hyphenated string form for logging/audit paths.
    #[must_use]
    pub fn host_id_str(&self) -> Option<String> {
        match self {
            AuthContext::Host { host_id, .. } => Some(host_id.to_string()),
            _ => None,
        }
    }

    /// Returns the host ID as a typed [`uuid::Uuid`] if this is a Host
    /// context (rev-7 §5.7). `None` for User.
    #[must_use]
    pub fn host_id(&self) -> Option<uuid::Uuid> {
        match self {
            AuthContext::Host { host_id, .. } => Some(*host_id),
            _ => None,
        }
    }

    /// Returns the org ID from the auth context.
    ///
    /// - `User` → `claims.org`
    /// - `Host` → `org_id` (hyphenated form of the stored `Uuid`)
    ///
    /// Returned as `Cow<'_, str>` for the same reason as
    /// [`actor_id`](Self::actor_id): User stores the org as a `String`, while
    /// Host stores a typed `Uuid` and converts on demand.
    #[must_use]
    pub fn org_id(&self) -> Option<Cow<'_, str>> {
        match self {
            AuthContext::User(claims) => Some(Cow::Borrowed(&claims.org)),
            AuthContext::Host { org_id, .. } => Some(Cow::Owned(org_id.to_string())),
        }
    }

    /// Returns the org ID, or an error if not available.
    ///
    /// Convenience wrapper for handlers that require an org context.
    pub fn selected_org_id(&self) -> Result<Cow<'_, str>, g5_contracts::error::G5Error> {
        self.org_id()
            .ok_or(g5_contracts::error::G5Error::Unauthorized)
    }

    /// Returns the scopes for this context.
    #[must_use]
    pub fn scopes(&self) -> &[Scope] {
        match self {
            AuthContext::User(claims) => &claims.scopes,
            AuthContext::Host { scopes, .. } => scopes,
        }
    }

    /// Returns `true` if this is a User context.
    #[must_use]
    pub fn is_user(&self) -> bool {
        matches!(self, AuthContext::User(_))
    }

    /// Returns `true` if this is a Host context (rev-7 §5.7).
    #[must_use]
    pub fn is_host(&self) -> bool {
        matches!(self, AuthContext::Host { .. })
    }

    /// Returns the [`JwtClaims`] if this is a User context.
    /// Returns `None` for Host contexts.
    #[must_use]
    pub fn claims(&self) -> Option<&JwtClaims> {
        match self {
            AuthContext::User(claims) => Some(claims),
            _ => None,
        }
    }

    /// Returns the user ID as a UUID, or a Forbidden error.
    ///
    /// Use this in handlers that require user-only access.
    pub fn require_user_id(&self) -> Result<uuid::Uuid, g5_contracts::error::G5Error> {
        match self {
            AuthContext::User(claims) => claims.sub.parse::<uuid::Uuid>().map_err(|_| {
                g5_contracts::error::G5Error::Internal {
                    message: "Invalid user ID in JWT claims".into(),
                }
            }),
            _ => Err(g5_contracts::error::G5Error::Forbidden {
                missing_scopes: vec!["user_auth_required".into()],
            }),
        }
    }
}

/// Extracts the `user_id` from `AuthContext::User`. Returns `Unauthorized`
/// error if the auth context is a service token (service tokens don't carry
/// user identity and cannot perform user-specific operations).
fn extract_user_id(auth: &AuthContext) -> Result<uuid::Uuid, AppError> {
    let user_id_str = auth
        .user_id()
        .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;
    let user_id = uuid::Uuid::parse_str(user_id_str).map_err(|_| {
        AppError::from_error(G5Error::Internal {
            message: "Invalid user ID in JWT".into(),
        })
    })?;
    Ok(user_id)
}

/// Extracts the user ID of a person's own session, refusing organization API
/// keys with `Forbidden`. Account-wide operations and session issuance must
/// not let a key escape its issuing organization and scope ceiling.
pub fn extract_session_user_id(auth: &AuthContext) -> Result<uuid::Uuid, AppError> {
    let user_id = extract_user_id(auth)?;
    if auth
        .claims()
        .is_some_and(|claims| claims.iss.as_deref() == Some(API_KEY_ISSUER))
    {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["user_session_required".into()],
        }));
    }
    Ok(user_id)
}

/// Extracts the user ID of a browser session sent by a browser, for actions
/// reserved for the web UI (g5.security.browser-session-actions). Refuses with
/// `Forbidden` (`browser_session_required`) API keys, CLI sessions, sessions
/// issued before sessions recorded their kind, tickets and other `token_use`
/// credentials, hosts, and requests without a same-origin or same-site
/// `Sec-Fetch-Site`. Browsers set that header on every request and page
/// scripts cannot; `Origin` is not consulted, because browsers omit it on
/// same-origin reads and a relay or proxy in front of the platform changes
/// it. This attests how the session was issued and that a browser sent the
/// request, not that a person is present.
pub fn require_browser_session(
    auth: &AuthContext,
    headers: &http::HeaderMap,
) -> Result<uuid::Uuid, AppError> {
    let refused = || {
        AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["browser_session_required".into()],
        })
    };
    let user_id = extract_session_user_id(auth)?;
    let Some(claims) = auth.claims() else {
        return Err(refused());
    };
    if claims.token_use.is_some() || claims.client.as_deref() != Some(SESSION_CLIENT_BROWSER) {
        return Err(refused());
    }
    let mut sites = headers.get_all("sec-fetch-site").iter();
    let site_allowed = sites
        .next()
        .is_some_and(|site| matches!(site.to_str(), Ok("same-origin" | "same-site")))
        && sites.next().is_none();
    if !site_allowed {
        return Err(refused());
    }
    Ok(user_id)
}

// ---------------------------------------------------------------------------
// FromRequestParts for AuthContext — axum extractor
// ---------------------------------------------------------------------------

/// Axum extractor for [`AuthContext`].
///
/// Allows route handlers to extract the auth context directly:
///
/// ```ignore
/// async fn get_workspace(
///     auth: AuthContext,
///     Path(id): Path<String>,
/// ) -> ApiResult<Json<WorkspaceDTO>> {
///     let user_id = auth.user_id().ok_or(G5Error::Unauthorized)?;
///     // ...
/// }
/// ```
#[async_trait]
impl<S> FromRequestParts<S> for AuthContext
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))
    }
}

// ---------------------------------------------------------------------------
// AuthLayer — Tower Layer
// ---------------------------------------------------------------------------

/// Tower [`Layer`] for the authentication middleware.
///
/// Wraps an inner service with JWT authentication and API key validation.
/// Apply this as the innermost layer in the middleware stack (after CORS,
/// compression, rate limiting).
///
/// # Usage
///
/// ```ignore
/// let router = Router::new()
///     .route("/api/v1/workspaces", get(list_workspaces))
///     .layer(AuthLayer::new(jwt_validator));
/// ```
#[derive(Clone)]
pub struct AuthLayer {
    jwt_validator: Arc<JwtValidator>,
    /// Trusted main origin for the preview re-auth redirect ([D18]);
    /// see [`with_main_origin`](Self::with_main_origin).
    main_origin: Option<String>,
    /// System pool for API key hash lookups (pre-RLS).
    api_key_pool: Option<PgPool>,
    /// Worker-host DB-liveness validator (rev-7 §5.7). When `Some`, every
    /// host JWT is checked against `worker_hosts` (existence + org_id +
    /// status) before `AuthContext::Host` is constructed. When `None`,
    /// the middleware fails closed — every host JWT is rejected with
    /// 500 ("host_jwt validation unavailable") instead of being accepted
    /// on signature alone. Production wiring in `main.rs` always supplies
    /// `PgHostAuthValidator`; the `None` path exists only because the
    /// pre-9a `AuthLayer::new` constructor predates this field and is
    /// still used by a small number of legacy unit tests that do not
    /// present host JWTs.
    host_validator: Option<Arc<dyn HostAuthValidator>>,
}

impl AuthLayer {
    /// Creates a new `AuthLayer` with the given JWT validator.
    #[must_use]
    pub fn new(jwt_validator: Arc<JwtValidator>) -> Self {
        Self {
            jwt_validator,
            main_origin: None,
            api_key_pool: None,
            host_validator: None,
        }
    }

    /// Attach a database pool for `g5k_*` API key validation.
    #[must_use]
    pub fn with_api_key_pool(mut self, pool: PgPool) -> Self {
        self.api_key_pool = Some(pool);
        self
    }

    /// Attach a [`HostAuthValidator`] so host JWTs are checked against
    /// the live `worker_hosts` registry (rev-7 §5.7 [D5]/[D7]).
    #[must_use]
    pub fn with_host_validator(mut self, validator: Arc<dyn HostAuthValidator>) -> Self {
        self.host_validator = Some(validator);
        self
    }

    /// Attach the canonical browser-facing main origin (validated
    /// `config.server.external_url`) enabling the preview re-auth redirect
    /// (routes/preview [D18]). The origin is trusted config — NEVER derived
    /// from `Host` or forwarded headers. When absent (legacy unit tests),
    /// the redirect arm is inert and unauthenticated preview navigations
    /// keep the plain 401.
    #[must_use]
    pub fn with_main_origin(mut self, origin: String) -> Self {
        self.main_origin = Some(origin);
        self
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            jwt_validator: self.jwt_validator.clone(),
            api_key_pool: self.api_key_pool.clone(),
            host_validator: self.host_validator.clone(),
            main_origin: self.main_origin.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// AuthMiddleware<S> — Tower Service
// ---------------------------------------------------------------------------

/// Tower [`Service`] that performs authentication on incoming requests.
///
/// - Exempt paths bypass authentication entirely.
/// - `/api/internal/` routes validate a workspace-incarnation or host JWT.
/// - All other routes validate JWT tokens.
/// - Stores [`AuthContext`] in request extensions on success.
/// - Returns standard JSON error envelope on failure.
#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    jwt_validator: Arc<JwtValidator>,
    /// System pool for API key hash lookups (pre-RLS).
    api_key_pool: Option<PgPool>,
    /// Worker-host DB-liveness validator (rev-7 §5.7).
    host_validator: Option<Arc<dyn HostAuthValidator>>,
    /// Trusted main origin for the preview re-auth redirect ([D18]).
    main_origin: Option<String>,
}

impl<S> Service<Request<Body>> for AuthMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        // Clone inner service and swap to maintain readiness state.
        // This is the standard Tower middleware pattern — failing to do this
        // causes panics or incorrect behavior under load.
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        let jwt_validator = self.jwt_validator.clone();
        let api_key_pool = self.api_key_pool.clone();
        let host_validator = self.host_validator.clone();
        let main_origin = self.main_origin.clone();

        Box::pin(async move {
            // Axum resolves nested routers before route layers run and may
            // replace `Request::uri()` with the nest-local suffix. Principal
            // selection is based on the public route namespace, so always use
            // the preserved original URI when routing is present. Direct
            // middleware unit tests have no `OriginalUri` and use the request
            // URI fallback.
            let path = req
                .extensions()
                .get::<OriginalUri>()
                .map(|uri| uri.0.path().to_string())
                .unwrap_or_else(|| req.uri().path().to_string());

            // 1. Check exempt paths — pass through without auth
            if is_exempt_path(&path) {
                tracing::trace!("Skipping auth for exempt path: {}", path);
                return inner.call(req).await;
            }

            // 1b. Preview ticket-cookie carve-out (auth_middleware [I13],
            // issue G5Labs/G5#736): on `/api/v1/preview/:workspaceId/*` paths that
            // host translation marked as arriving on the preview's own host,
            // the workspace-bound `__Host-g5_preview` ticket cookie is the
            // authoritative browser auth and is honored FIRST — before
            // extracting any Authorization header. The previewed customer app
            // attaches its OWN (non-platform) bearer token to every API call;
            // validating THAT as a platform JWT would 401 the request before
            // the relay runs (the app token is foreign/HS-signed, not a G5
            // ES256 JWT). Trying the cookie first keeps every downstream
            // security layer (RequireScopes, audit, org-scoped RLS,
            // cross-tenant workspace binding via the ticket `aud`) unchanged. A
            // genuine platform Bearer token still authenticates a preview path
            // via the fall-through below when no valid cookie is present.
            // Requests on the platform path never authenticate by cookie
            // (`g5.security.mvp-exposure.preview`). `preview_path_workspace_id`
            // excludes the mint endpoint.
            let translated_workspace = req
                .extensions()
                .get::<PreviewHostOrigin>()
                .map(|origin| origin.workspace_id);
            if let Some(workspace_id) = preview_path_workspace_id(&path)
                .filter(|workspace_id| translated_workspace == Some(*workspace_id))
            {
                // A workspace agent's preview credential
                // (`g5.preview.agent-access`) is the request's only
                // credential when its header is present: an invalid one never
                // falls back to a cookie or bearer token.
                if req.headers().contains_key(g5_auth::PREVIEW_AGENT_HEADER) {
                    let Some(claims) = admit_preview_agent(&jwt_validator, workspace_id, &req)
                    else {
                        return Ok(build_error_response(G5Error::Unauthorized, &req));
                    };
                    req.extensions_mut()
                        .insert(AuthContext::User(preview_agent_user_claims(&claims)));
                    req.extensions_mut().insert(PreviewAgentContext(claims));
                    return inner.call(req).await;
                }
                for ticket in preview_cookie_tickets(&req) {
                    if let Ok(claims) = jwt_validator.validate_preview_ticket(&ticket, workspace_id)
                    {
                        tracing::debug!(
                            user_id = %claims.sub,
                            workspace_id = %workspace_id,
                            "Authenticated preview via __Host-g5_preview ticket cookie"
                        );
                        req.extensions_mut().insert(AuthContext::User(claims));
                        return inner.call(req).await;
                    }
                }
            }

            // 2. Extract Authorization: Bearer <token>
            let token = match extract_bearer_token(&req) {
                Some(t) => t,
                None => {
                    // [I13] step 5 / routes/preview [D18] (issue G5#1097
                    // piece 2): an unauthenticated browser HTML NAVIGATION on
                    // a preview path is redirected to the re-auth endpoint,
                    // which heals the session off the main-origin g5_refresh
                    // cookie. Everything else (XHR/fetch, non-GET, Bearer
                    // fall-through above, speculative loads, fresh-marker
                    // requests) keeps the standard 401 envelope.
                    if let (Some(workspace_id), Some(origin)) =
                        (preview_path_workspace_id(&path), main_origin.as_deref())
                    {
                        if let Some(redirect) = preview_reauth_redirect(
                            origin,
                            workspace_id,
                            &path,
                            req.uri().query(),
                            req.headers(),
                            req.method(),
                        ) {
                            tracing::debug!(
                                %workspace_id,
                                "Unauthenticated preview navigation → re-auth redirect"
                            );
                            return Ok(redirect);
                        }
                    }
                    // No Bearer header, and (on a preview path) no valid
                    // ticket cookie above → standard 401. Not an
                    // exemption; every failure falls through to here.
                    tracing::warn!(
                        "Authentication failed for {}: missing Authorization header",
                        path
                    );
                    return Ok(build_error_response(G5Error::Unauthorized, &req));
                }
            };

            // 4. Route to appropriate validator based on path prefix
            if path.starts_with("/api/internal/") {
                if is_workspace_incarnation_internal_route(req.method(), &path) {
                    match jwt_validator.validate_workspace_incarnation(&token) {
                        Ok(claims) => {
                            req.extensions_mut()
                                .insert(WorkspaceIncarnationContext(claims));
                            return inner.call(req).await;
                        }
                        Err(error) => {
                            tracing::warn!(
                                path,
                                %error,
                                "Workspace-incarnation authentication failed"
                            );
                            return Ok(build_error_response(error, &req));
                        }
                    }
                }

                if !is_host_only_internal_path(&path) {
                    tracing::warn!(path, "Rejected request to unknown internal API path");
                    return Ok(build_error_response(G5Error::Unauthorized, &req));
                }

                match jwt_validator.validate(&token) {
                    Ok(claims) => {
                        if !claims.scopes.contains(&Scope::WorkerHost) {
                            tracing::warn!(
                                subject = %claims.sub,
                                path,
                                "Non-host JWT used on host-only internal path"
                            );
                            return Ok(build_error_response(
                                G5Error::Forbidden {
                                    missing_scopes: vec![Scope::WorkerHost.to_string()],
                                },
                                &req,
                            ));
                        }

                        let host_id = match claims.sub.parse::<uuid::Uuid>() {
                            Ok(value) => value,
                            Err(_) => {
                                tracing::warn!(
                                    subject = %claims.sub,
                                    path,
                                    "Rejected host_jwt with non-UUID sub claim"
                                );
                                return Ok(build_error_response(G5Error::Unauthorized, &req));
                            }
                        };
                        let host_org_id = match claims.org.parse::<uuid::Uuid>() {
                            Ok(value) => value,
                            Err(_) => {
                                tracing::warn!(
                                    org = %claims.org,
                                    path,
                                    "Rejected host_jwt with non-UUID org claim"
                                );
                                return Ok(build_error_response(G5Error::Unauthorized, &req));
                            }
                        };

                        if let Some(ref validator) = host_validator {
                            match validator.validate(host_id, host_org_id).await {
                                Ok(()) => {}
                                Err(HostAuthError::NotFound) => {
                                    tracing::warn!(
                                        host_id = %host_id,
                                        path,
                                        "Rejected host_jwt: host_id not in worker_hosts"
                                    );
                                    return Ok(build_error_response(G5Error::Unauthorized, &req));
                                }
                                Err(HostAuthError::OrgMismatch) => {
                                    tracing::warn!(
                                        host_id = %host_id,
                                        org_id = %host_org_id,
                                        path,
                                        "Rejected host_jwt: org_id does not match worker_hosts row"
                                    );
                                    return Ok(build_error_response(G5Error::Unauthorized, &req));
                                }
                                Err(HostAuthError::StatusForbidden(status)) => {
                                    tracing::warn!(
                                        host_id = %host_id,
                                        host_status = %status,
                                        path,
                                        "Rejected host_jwt: host status not acceptable"
                                    );
                                    return Ok(build_error_response(G5Error::Unauthorized, &req));
                                }
                                Err(HostAuthError::Db(error)) => {
                                    tracing::error!(
                                        host_id = %host_id,
                                        path,
                                        %error,
                                        "Failed to validate host_jwt against worker_hosts"
                                    );
                                    return Ok(build_error_response(
                                        G5Error::Internal {
                                            message: "worker_hosts validation failed".into(),
                                        },
                                        &req,
                                    ));
                                }
                            }
                        } else {
                            tracing::warn!(
                                host_id = %host_id,
                                path,
                                "Rejected host_jwt: AuthLayer has no HostAuthValidator wired"
                            );
                            return Ok(build_error_response(
                                G5Error::Internal {
                                    message: "host_jwt validation unavailable".into(),
                                },
                                &req,
                            ));
                        }

                        tracing::debug!(
                            host_id = %host_id,
                            org_id = %host_org_id,
                            "Authenticated worker host via host_jwt"
                        );
                        req.extensions_mut().insert(AuthContext::Host {
                            host_id,
                            org_id: host_org_id,
                            scopes: claims.scopes,
                        });
                        inner.call(req).await
                    }
                    Err(error) => {
                        tracing::warn!(path, %error, "Host authentication failed");
                        Ok(build_error_response(error, &req))
                    }
                }
            } else {
                // 4a. API key authentication — detect `g5k_` prefix
                if token.starts_with(g5_auth::API_KEY_PREFIX) {
                    if let Some(ref pool) = api_key_pool {
                        // Extract client IP for last_used_ip tracking.
                        let remote_ip =
                            super::client_ip::ClientIp::from_extensions(req.extensions())
                                .address()
                                .map(|ip| ip.to_string());

                        match ApiKeyService::validate_api_key(pool, &token, remote_ip.as_deref())
                            .await
                        {
                            Ok(validated) => {
                                tracing::debug!(
                                    user_id = %validated.user_id,
                                    org_id = %validated.org_id,
                                    "Authenticated via API key"
                                );
                                let claims = JwtClaims {
                                    sub: validated.user_id.to_string(),
                                    org: validated.org_id.to_string(),
                                    scopes: validated.scopes,
                                    iat: 0,
                                    exp: i64::MAX,
                                    iss: Some(API_KEY_ISSUER.to_string()),
                                    aud: Some("g5".to_string()),
                                    jti: None,
                                    token_use: None,
                                    client: None,
                                };
                                req.extensions_mut().insert(AuthContext::User(claims));
                                return inner.call(req).await;
                            }
                            Err(e) => {
                                tracing::warn!("API key authentication failed for {}: {}", path, e);
                                return Ok(build_error_response(e, &req));
                            }
                        }
                    } else {
                        tracing::warn!("API key presented but no pool configured for validation");
                        return Ok(build_error_response(G5Error::Unauthorized, &req));
                    }
                }

                // JWT validation for public API endpoints
                match jwt_validator.validate(&token) {
                    Ok(claims) => {
                        // rev-7 §5.7 D4: host_jwt is for the internal
                        // host-control surface only. Rejecting it here prevents
                        // a host token from masquerading as a user.
                        if claims.scopes.contains(&Scope::WorkerHost) {
                            tracing::warn!(
                                subject = %claims.sub,
                                path,
                                "Worker host JWT used on public API path"
                            );
                            return Ok(build_error_response(G5Error::Unauthorized, &req));
                        }
                        tracing::debug!("Authenticated user {} for org {}", claims.sub, claims.org);
                        req.extensions_mut().insert(AuthContext::User(claims));
                        inner.call(req).await
                    }
                    Err(e) => {
                        tracing::warn!("Authentication failed for {}: {}", path, e);
                        Ok(build_error_response(e, &req))
                    }
                }
            }
        })
    }
}

fn is_workspace_incarnation_internal_route(method: &http::Method, path: &str) -> bool {
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    let valid_uuid = |value: &str| uuid::Uuid::parse_str(value).is_ok();

    match (method, segments.as_slice()) {
        (
            &http::Method::GET | &http::Method::PUT,
            ["api", "internal", "chat-sessions", session_id, "session-files"],
        ) => valid_uuid(session_id),
        (
            &http::Method::GET,
            ["api", "internal", "chat-sessions", session_id, "session-files", leaf],
        ) => valid_uuid(session_id) && matches!(*leaf, "manifest" | "content" | "published"),
        (
            &http::Method::POST,
            ["api", "internal", "chat-sessions", session_id, "session-files", leaf],
        ) => valid_uuid(session_id) && matches!(*leaf, "begin" | "publish"),
        (
            &http::Method::GET,
            ["api", "internal", "workspaces", workspace_id, "session-files", "pending"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::POST,
            ["api", "internal", "chat-sessions", session_id, "subscription-usage"],
        )
        | (&http::Method::POST, ["api", "internal", "chat-sessions", session_id, "codex-auth"])
        | (&http::Method::POST, ["api", "internal", "chat-sessions", session_id, "attachments"]) => {
            valid_uuid(session_id)
        }
        (&http::Method::GET, ["api", "internal", "chat-attachments", attachment_id, "content"]) => {
            valid_uuid(attachment_id)
        }
        // Reading one PR, and listing a repository's PRs (`g5.git.pr-list`).
        (
            &http::Method::GET,
            ["api", "internal", "workspaces", workspace_id, "pull-requests", "info" | "github" | "targets" | "descriptions"],
        ) => valid_uuid(workspace_id),
        // One stored PR's canonical description (`g5.git.pr-description`).
        (
            &http::Method::GET,
            ["api", "internal", "workspaces", workspace_id, "pull-requests", pull_request_id, "description"],
        ) => valid_uuid(workspace_id) && valid_uuid(pull_request_id),
        // `g5d pr create --dry-run`: a preview that writes nothing.
        (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "pull-requests", "preview"],
        ) => valid_uuid(workspace_id),
        (&http::Method::POST, ["api", "internal", "workspaces", workspace_id, "pull-requests"]) => {
            valid_uuid(workspace_id)
        }
        // The owner's agent reading a GitHub PR's attachments (`g5.attachments`).
        (
            &http::Method::GET,
            ["api", "internal", "workspaces", workspace_id, "pull-requests", "github", "attachments"]
            | ["api", "internal", "workspaces", workspace_id, "pull-requests", "github", "attachments", "content"],
        ) => valid_uuid(workspace_id),
        // The owner's agent commenting on and resolving GitHub PR review threads.
        (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "pull-requests", "github", "comments"]
            | ["api", "internal", "workspaces", workspace_id, "pull-requests", "github", "threads", "resolve"],
        ) => valid_uuid(workspace_id),
        // The owner's agent reading and linking its issues, commenting and
        // creating issues as the owner (routes/internal).
        (
            &http::Method::GET | &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "issue-links"],
        )
        | (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "issue-comments" | "issues"]
            | ["api", "internal", "workspaces", workspace_id, "issues", "attachments"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "source-control", "save"],
        ) => valid_uuid(workspace_id),
        // The owner's agent emailing its own owner (`g5.workspace.owner-email`).
        (&http::Method::POST, ["api", "internal", "workspaces", workspace_id, "owner-email"]) => {
            valid_uuid(workspace_id)
        }
        (
            &http::Method::GET,
            ["api", "internal", "workspaces", workspace_id, "issue-links", link_id],
        ) => valid_uuid(workspace_id) && valid_uuid(link_id),
        // The owner's agent reading any issue in the organization's issue
        // domains, searching them, reading their attachments, and unlinking
        // its own issues (`g5.external-issues.cli.reads`, `g5.attachments`).
        (
            &http::Method::GET,
            ["api", "internal", "workspaces", workspace_id, "issues", "view" | "search" | "attachments"]
            | ["api", "internal", "workspaces", workspace_id, "issues", "attachments", "content"],
        )
        | (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "issue-links", "unlink"],
        ) => valid_uuid(workspace_id),
        // The owner's agent reading, authoring and proposing referenced
        // questions (routes/internal, `g5.documents.question.agent-access`).
        (
            &http::Method::GET | &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "questions"],
        )
        | (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "questions", "answer"],
        ) => valid_uuid(workspace_id),
        (&http::Method::GET, ["api", "internal", "workspaces", workspace_id, "manifest"])
        | (&http::Method::POST, ["api", "internal", "workspaces", workspace_id, "files"])
        | (
            &http::Method::POST,
            ["api", "internal", "workspaces", workspace_id, "platform", "issues"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-incarnations", workspace_id, "flushes", "start"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-incarnations", workspace_id, "flushes", flush_id, "complete" | "fail"],
        ) => valid_uuid(workspace_id) && valid_uuid(flush_id),
        (&http::Method::PUT, ["api", "internal", "docs", doc_id, "content"]) => valid_uuid(doc_id),
        // Workspace-agent operations (G5-Lean#710): the handlers check the
        // incarnation and the target's agent scope.
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces" | "children" | "users"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        // A running chat turn's ontology record (g5.chat.ontology-card): its
        // start snapshot from the runner and its governing statements.
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "chat-tasks", task_id, "ontology"],
        ) => valid_uuid(workspace_id) && valid_uuid(task_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "chat-tasks", task_id, "ontology", "baseline" | "governing"],
        ) => valid_uuid(workspace_id) && valid_uuid(task_id),
        (
            &http::Method::GET | &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "sessions"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "sessions", session_id, "messages"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(session_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "start" | "stop" | "delete" | "pull-request"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "pull-requests", pull_request_id, "merge" | "close" | "refresh"],
        ) => valid_uuid(workspace_id) && valid_uuid(pull_request_id),
        // Merge when ready: an agent may only turn it off (g5.git.pr-auto-merge).
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "pull-requests", pull_request_id, "auto-merge", "disable"],
        ) => valid_uuid(workspace_id) && valid_uuid(pull_request_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "sessions", session_id, "stop" | "background-end" | "rename"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(session_id),
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "sessions", session_id, "inputs"],
        ) => valid_uuid(workspace_id) && valid_uuid(session_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "sessions", session_id, "inputs", input_id, "edit" | "withdraw" | "redeliver"],
        ) => {
            valid_uuid(workspace_id)
                && valid_uuid(target_id)
                && valid_uuid(session_id)
                && valid_uuid(input_id)
        }
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "tasks", task_id, "continue" | "cancel"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(task_id),
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "approvals" | "pull-requests"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        // The org-wide approval activity feed (G5-Lean#1099): no target
        // workspace, the readable-workspace scope is applied in the query.
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "approval-activity"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "approvals", "requests" | "reviews"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "approvals", "requests", request_id, "cancel"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(request_id),
        // A formal agent review's retained evidence; reads only (G5-Lean#1019).
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "approvals", "reviews", review_id, "report" | "transcript" | "commits"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(review_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "issue-state" | "issue-edit"],
        ) => valid_uuid(workspace_id),
        // A domain's label catalog (`g5d issue label`, g5.issue.labels).
        (
            &http::Method::GET | &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "issue-labels"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "issue-labels", "edit" | "delete"],
        ) => valid_uuid(workspace_id),
        (
            &http::Method::GET | &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "preview"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "preview", "cancel" | "credential"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "repos", repo_id, "preview-applications" | "preview-profiles"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(repo_id),
        (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "repos", repo_id, "preview-profiles"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id) && valid_uuid(repo_id),
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "sessions", session_id, "messages" | "timeline"],
        ) => valid_uuid(workspace_id) && valid_uuid(session_id),
        (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "files"],
        )
        | (
            &http::Method::GET,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "files", "content"],
        )
        | (
            &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "workspaces", target_id, "files", "write" | "delete" | "rename"],
        ) => valid_uuid(workspace_id) && valid_uuid(target_id),
        (&http::Method::POST, ["api", "internal", "workspace-agent", workspace_id, "fork"]) => {
            valid_uuid(workspace_id)
        }
        (
            &http::Method::GET | &http::Method::POST,
            ["api", "internal", "workspace-agent", workspace_id, "stop-owner-workspaces"],
        ) => valid_uuid(workspace_id),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// RequireScopes — per-route scope enforcement Layer
// ---------------------------------------------------------------------------

/// Tower [`Layer`] for per-route scope enforcement.
///
/// Applied to specific routes to enforce that the authenticated user has
/// all required scopes. Runs AFTER the auth middleware (which has already
/// stored [`AuthContext`] in extensions).
///
/// # Usage
///
/// ```ignore
/// .route(
///     "/api/v1/workspaces",
///     post(create_workspace)
///         .layer(RequireScopes::new(&["workspaces:write"])),
/// )
/// ```
///
/// # Behavior
///
/// - **User tokens** — checks that all required scopes are present.
/// - **Host tokens** — checks their host-only scopes.
/// - Returns **403 FORBIDDEN** (not 401) when scopes are insufficient.
/// - Returns **401 UNAUTHORIZED** if no `AuthContext` is found (defensive).
#[derive(Clone)]
pub struct RequireScopes {
    required_scopes: &'static [Scope],
}

impl RequireScopes {
    /// Creates a new `RequireScopes` layer with the given scope requirements.
    #[must_use]
    pub fn new(scopes: &'static [Scope]) -> Self {
        Self {
            required_scopes: scopes,
        }
    }
}

impl<S> Layer<S> for RequireScopes {
    type Service = RequireScopesMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequireScopesMiddleware {
            inner,
            required_scopes: self.required_scopes,
        }
    }
}

// ---------------------------------------------------------------------------
// RequireScopesMiddleware<S> — Tower Service
// ---------------------------------------------------------------------------

/// Tower [`Service`] that enforces scope requirements on authenticated
/// requests.
///
/// Checks that the [`AuthContext`] stored in request extensions has all
/// required scopes.
#[derive(Clone)]
pub struct RequireScopesMiddleware<S> {
    inner: S,
    required_scopes: &'static [Scope],
}

impl<S> Service<Request<Body>> for RequireScopesMiddleware<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        // Clone inner service and swap to maintain readiness state.
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        let required_scopes = self.required_scopes;

        Box::pin(async move {
            // Extract AuthContext from extensions
            let auth_context = req.extensions().get::<AuthContext>().cloned();

            match auth_context {
                // User tokens — check scopes
                Some(AuthContext::User(ref claims)) => {
                    if has_scopes(claims, required_scopes) {
                        inner.call(req).await
                    } else {
                        // 403 FORBIDDEN — valid token but insufficient scopes
                        let missing: Vec<String> = required_scopes
                            .iter()
                            .filter(|s| !claims.scopes.contains(s))
                            .map(|s| s.to_string())
                            .collect();

                        let path = req.uri().path().to_string();
                        tracing::warn!("Scope check failed for {}: missing {:?}", path, missing);

                        Ok(build_error_response(
                            G5Error::Forbidden {
                                missing_scopes: missing,
                            },
                            &req,
                        ))
                    }
                }

                // Host tokens (rev-7 §5.7) — check scopes from the Host variant.
                // A path that does NOT require Scope::WorkerHost will reject a
                // host JWT here with 403, which is the intended behaviour
                // (host JWTs cannot do user-facing work).
                Some(AuthContext::Host { ref scopes, .. }) => {
                    if required_scopes.iter().all(|s| scopes.contains(s)) {
                        inner.call(req).await
                    } else {
                        let missing: Vec<String> = required_scopes
                            .iter()
                            .filter(|s| !scopes.contains(s))
                            .map(|s| s.to_string())
                            .collect();
                        let path = req.uri().path().to_string();
                        tracing::warn!(
                            "Scope check failed for host on {}: missing {:?}",
                            path,
                            missing
                        );
                        Ok(build_error_response(
                            G5Error::Forbidden {
                                missing_scopes: missing,
                            },
                            &req,
                        ))
                    }
                }

                // No AuthContext — should not happen if auth middleware ran,
                // but handle defensively
                None => {
                    tracing::warn!(
                        "No AuthContext found in extensions for scope check on {}",
                        req.uri().path()
                    );
                    Ok(build_error_response(G5Error::Unauthorized, &req))
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Checks whether the given path is exempt from authentication.
///
/// Exempt paths bypass authentication entirely. They are hardcoded — this
/// is intentional: the set of unauthenticated endpoints is a security-critical
/// constant that should not be configurable at runtime.
///
/// # Exempt Paths
///
/// | Path Pattern | Reason |
/// |---|---|
/// | `/api/v1/auth/login/*` | Initiates OAuth flow — no token exists yet |
/// | `/api/v1/auth/callback/*` | OAuth callback — token is being created |
/// | `/api/v1/instance/health` | Load balancer health checks |
/// | `/api/v1/instance/version` | Version endpoint — public metadata |
/// | `/api/v1/instance/info` | Instance info — needed by landing page before login |
/// | `/api/v1/ws` | WebSocket upgrade — auth via query-param token |
/// | `/api/v1/workspaces/:id/terminal` | Terminal WebSocket — auth via scoped query-param ticket |
/// | `/api/v1/auth/test-token` | E2E token issuer — only when `G5_E2E_AUTH=true` |
/// | `/api/v1/auth/refresh` | Token refresh — auth via refresh token, not JWT |
/// | `/api/v1/github-app/{setup,oauth/callback,reconnect/select}` | GitHub App browser callbacks — signed state |
/// | `/api/v1/admin/licenses` | Self-hosted license provisioning — auth via admin bearer token |
/// | `/api/v1/registry/token` | Self-hosted registry exchange — auth via license key bearer token |
fn is_exempt_path(path: &str) -> bool {
    // Auth login/callback paths (with provider parameter)
    if path.starts_with("/api/v1/auth/login/") {
        return true;
    }
    if path.starts_with("/api/v1/auth/callback/") {
        return true;
    }
    // SCM OAuth callback — authentication is via the OAuth `state` parameter
    // stored in `oauth_states` table, not a JWT. The browser redirect from
    // the OAuth provider has no JWT credentials.
    if path.starts_with("/api/v1/scm/") && path.ends_with("/callback") {
        return true;
    }
    // Refresh endpoint — the refresh token (cookie or body) is the auth
    // mechanism, not a JWT. Making this non-exempt would create a
    // chicken-and-egg problem: you'd need a valid JWT to get a new JWT.
    if path == "/api/v1/auth/refresh" {
        return true;
    }
    // Preview re-auth endpoint (routes/preview [D18], issue G5#1097 piece 2)
    // — same rationale as /refresh: the g5_refresh cookie IS the credential.
    // Exact shape only: the trailing segment must be a UUID.
    if let Some(rest) = path.strip_prefix("/api/v1/auth/preview/reauth/") {
        if !rest.contains('/') && uuid::Uuid::parse_str(rest).is_ok() {
            return true;
        }
    }
    // CLI device authorization start/poll endpoints are called before a local
    // machine has a bearer token. Browser approval remains non-exempt and
    // therefore requires a validated user JWT.
    if path == "/api/v1/auth/cli/start" || path == "/api/v1/auth/cli/token" {
        return true;
    }
    // Health and version endpoints
    if path == "/healthz"
        || path == "/api/v1/instance/health"
        || path == "/api/v1/instance/version"
        || path == "/api/v1/instance/info"
    {
        return true;
    }
    // Git smart HTTP — authentication is handled by the GitAuth extractor
    // using HTTP Basic auth (x-worker-token / x-user-token).
    if path.starts_with("/git/") {
        return true;
    }
    // WebSocket — authentication is handled by the WS handler itself using
    // a query-parameter token because the WebSocket protocol does not
    // support custom headers during the HTTP upgrade handshake.
    if path == "/api/v1/ws" {
        return true;
    }
    if let Some(rest) = path.strip_prefix("/api/v1/workspaces/") {
        if let Some(workspace_id) = rest.strip_suffix("/terminal") {
            if !workspace_id.contains('/') && uuid::Uuid::parse_str(workspace_id).is_ok() {
                return true;
            }
        }
    }
    // E2E test-token endpoint — only registered when G5_E2E_AUTH=true.
    // Must be exempt because it is the endpoint that *creates* auth tokens.
    if path == "/api/v1/auth/test-token" {
        return true;
    }
    // GitHub App browser callbacks carry no G5 session. The handlers
    // authorize them with HMAC-signed state matched to a stored digest, a
    // signed PKCE cookie and GitHub's installation-access proof.
    if matches!(
        path,
        "/api/v1/github-app/setup"
            | "/api/v1/github-app/oauth/callback"
            | "/api/v1/github-app/reconnect/select"
    ) {
        return true;
    }
    // Self-hosted delivery endpoints authenticate inside their handlers:
    // `/api/v1/admin/licenses` validates the admin bearer token hash, and
    // `/api/v1/registry/token` validates the customer license key.
    if path == "/api/v1/admin/licenses" || path == "/api/v1/registry/token" {
        return true;
    }
    false
}

/// URL prefix reserved for host-only internal endpoints (rev-7 §5.7).
///
/// A `host_jwt` is only valid on a path that starts with this prefix. The
/// remaining `/api/internal/…` namespace is owned by worker session JWTs.
/// The PR-rev-7-9b provisioning surface and PR-rev-7-9c bring-up
/// endpoints register routes under this prefix; the gating is enforced
/// here so a future endpoint cannot accidentally widen the host token's
/// reach by forgetting to attach a scope layer.
pub const HOST_ONLY_INTERNAL_PREFIX: &str = "/api/internal/host/";

/// Returns `true` when the path is reserved for `host_jwt` traffic
/// (rev-7 §5.7 [D4]).
///
/// Used both to accept a host JWT on a host-only path and to reject a
/// worker session / user JWT that tried to reach one.
///
/// # Preview reverse-tunnel exception (worker_hosts [D4])
///
/// The preview reverse-tunnel attach route is registered at the *plural*
/// `/api/internal/hosts/:hostId/tunnel` (routes/internal [I27]) rather than
/// under the singular [`HOST_ONLY_INTERNAL_PREFIX`], but its carrier is the
/// host itself — the [`AuthenticatedHost`] extractor requires a
/// `Scope::WorkerHost` token, and the deployed `g5d` dials this exact plural
/// path. It therefore belongs on the host-token surface. Admitting it here (a
/// g5-api-only, hot-deployable change) is preferred over relocating the route
/// or rebuilding the worker image. The exception is deliberately bounded to
/// the tunnel carrier (`…/hosts/:hostId/tunnel`) with `ends_with("/tunnel")`
/// rather than a blanket `/api/internal/hosts/` prefix, so a future plural
/// `/hosts/` route cannot silently inherit host-token reach; the per-route
/// `:hostId == authenticated host id` tenancy check (routes/internal [I27])
/// remains enforced downstream in the extractor.
#[must_use]
pub fn is_host_only_internal_path(path: &str) -> bool {
    path.starts_with(HOST_ONLY_INTERNAL_PREFIX)
        || (path.starts_with("/api/internal/hosts/") && path.ends_with("/tunnel"))
}

/// Extracts a bearer token from the `Authorization` header.
///
/// Returns `None` if:
/// - The `Authorization` header is missing.
/// - The header value is not valid UTF-8.
/// - The header does not start with `Bearer `.
/// - The token part is empty.
///
/// # Security
///
/// Tokens are extracted ONLY from the `Authorization` header — NOT from
/// query parameters, cookies, or the request body. This is a deliberate
/// security decision: the `Authorization` header is not sent in cross-origin
/// requests by default, reducing CSRF risk.
fn extract_bearer_token(req: &Request<Body>) -> Option<String> {
    let header_value = req.headers().get(header::AUTHORIZATION)?;
    let value = header_value.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

/// Cookie carrying the workspace-bound preview session ticket, set on the
/// per-preview host by the handoff exchange (issue G5Labs/G5#935;
/// `routes/preview` [D15], `middleware/preview_host` [D2]). Host-only,
/// `Secure`, `Path=/` — workspace A's preview origin can never receive workspace B's
/// cookie, and no platform-origin variant exists
/// (`g5.security.mvp-exposure.preview`). Preview-host requests reach the
/// [I13] carve-out with the standard preview path shape because the
/// host-translation layer rewrites them before this middleware runs.
pub const PREVIEW_TICKET_COOKIE_HOST: &str = "__Host-g5_preview";

/// Returns the preview workspace id when `path` is a browser preview-proxy path
/// (`/api/v1/preview/:workspaceId`, with or without a trailing `/` or wildcard
/// remainder) — the only paths where the preview ticket cookie is an
/// acceptable credential (auth_middleware [I13]).
///
/// The mint endpoint (`/api/v1/preview/session/:workspaceId`) never matches: its
/// first segment is the literal `session`, which is not a UUID, so it always
/// requires a normal Bearer credential.
/// Query parameter carrying the [D18] re-auth loop-guard marker: its value is
/// the unix-seconds issuance timestamp. A FRESH marker (≤ 60 s) suppresses the
/// re-auth redirect so a cookie-blocked browser gets a plain 401 instead of an
/// infinite 302 loop; a stale marker is ignored so a later expiry on a URL
/// still bearing an old marker heals again. The relay strips this parameter
/// before forwarding upstream; the [D15] handoff 303 deliberately preserves it.
pub const PREVIEW_REAUTH_MARKER_PARAM: &str = "__g5_reauth";

/// Marker freshness window (seconds).
pub const PREVIEW_REAUTH_MARKER_FRESH_SECS: i64 = 60;

/// True when the query carries a FRESH `__g5_reauth` marker ([D18]).
fn has_fresh_reauth_marker(query: Option<&str>, now: i64) -> bool {
    let Some(query) = query else { return false };
    for pair in query.split('&') {
        if let Some((name, value)) = pair.split_once('=') {
            if name == PREVIEW_REAUTH_MARKER_PARAM {
                if let Ok(ts) = value.parse::<i64>() {
                    if now - ts <= PREVIEW_REAUTH_MARKER_FRESH_SECS {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Browser HTML-navigation classifier for the [D18] re-auth redirect.
///
/// - Speculative loads never qualify (`Sec-Purpose`/`Purpose: prefetch`,
///   prerender) — a prefetcher must not mint credentials.
/// - With fetch metadata present: `Sec-Fetch-Dest` must be `document` or
///   `iframe` AND `Sec-Fetch-Mode` must be `navigate`. `iframe` is deliberate:
///   an embedded Preview Browser is an iframe whose navigations must heal; a
///   cross-site framed navigation still fails closed at the endpoint because
///   `SameSite=Lax` withholds `g5_refresh` on non-top-level cross-site
///   requests.
/// - Only when BOTH `Sec-Fetch-Dest` and `Sec-Fetch-Mode` are absent (older
///   browsers) does an `Accept` containing `text/html` qualify; a present
///   non-document destination never falls through to the Accept fallback.
fn is_browser_html_navigation(headers: &http::HeaderMap) -> bool {
    let header_str = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_ascii_lowercase)
    };
    let speculative =
        |v: Option<String>| v.is_some_and(|p| p.contains("prefetch") || p.contains("prerender"));
    if speculative(header_str("sec-purpose")) || speculative(header_str("purpose")) {
        return false;
    }
    let dest = header_str("sec-fetch-dest");
    let mode = header_str("sec-fetch-mode");
    match (dest, mode) {
        (None, None) => header_str("accept").is_some_and(|a| a.contains("text/html")),
        (dest, mode) => {
            dest.is_some_and(|d| d == "document" || d == "iframe")
                && mode.is_some_and(|m| m == "navigate")
        }
    }
}

/// Builds the [D18] `302` to the preview re-auth endpoint, or `None` when the
/// request does not qualify (wrong method, not a navigation, fresh marker).
///
/// `main_origin` is trusted config (`config.server.external_url`, validated at
/// startup) — never derived from `Host`/forwarded headers. The `return`
/// parameter is the request's path+query relative to the preview root; the
/// endpoint re-sanitizes it after percent-decoding.
fn preview_reauth_redirect(
    main_origin: &str,
    workspace_id: uuid::Uuid,
    path: &str,
    query: Option<&str>,
    headers: &http::HeaderMap,
    method: &http::Method,
) -> Option<Response<Body>> {
    if method != http::Method::GET {
        return None;
    }
    if !is_browser_html_navigation(headers) {
        return None;
    }
    if has_fresh_reauth_marker(query, chrono::Utc::now().timestamp()) {
        return None;
    }
    // Path+query relative to the preview root, in the host-translated shape.
    let prefix = format!("/api/v1/preview/{workspace_id}");
    let mut rest = path.strip_prefix(&prefix).unwrap_or("/").to_string();
    if rest.is_empty() {
        rest.push('/');
    }
    if let Some(q) = query {
        if !q.is_empty() {
            rest.push('?');
            rest.push_str(q);
        }
    }
    let encoded_return: String = url::form_urlencoded::byte_serialize(rest.as_bytes()).collect();
    let location =
        format!("{main_origin}/api/v1/auth/preview/reauth/{workspace_id}?return={encoded_return}");
    Response::builder()
        .status(http::StatusCode::FOUND)
        .header(http::header::LOCATION, location)
        .header(http::header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .ok()
}

fn preview_path_workspace_id(path: &str) -> Option<uuid::Uuid> {
    let rest = path.strip_prefix("/api/v1/preview/")?;
    let segment = rest.split('/').next().unwrap_or("");
    segment.parse::<uuid::Uuid>().ok()
}

/// The preview agent credential on a request for preview `task_id`, if it is
/// valid and the request is not a browser's. Browsers always send fetch
/// metadata and page script cannot remove it, so any `Sec-Fetch-*` header
/// refuses the credential; so does more than one credential value.
fn admit_preview_agent(
    validator: &JwtValidator,
    task_id: uuid::Uuid,
    req: &Request<Body>,
) -> Option<PreviewAgentClaims> {
    let headers = req.headers();
    if headers
        .keys()
        .any(|name| name.as_str().starts_with("sec-fetch-"))
    {
        tracing::warn!(%task_id, "Preview agent credential refused from a browser request");
        return None;
    }
    let mut values = headers.get_all(g5_auth::PREVIEW_AGENT_HEADER).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        tracing::warn!(%task_id, "Preview agent credential refused: repeated header");
        return None;
    };
    let claims = validator
        .validate_preview_agent(value.to_str().ok()?.trim(), task_id)
        .map_err(|_| tracing::warn!(%task_id, "Preview agent credential invalid or expired"))
        .ok()?;
    tracing::debug!(
        %task_id,
        caller_workspace_id = %claims.workspace_id,
        "Authenticated preview via workspace agent credential"
    );
    Some(claims)
}

/// The owner's request-local identity for a preview agent credential: the
/// person preview ticket's read scopes, but its own `token_use` and audience,
/// so person-only preview paths (renewal, embed handling) never apply and
/// audit names the credential.
fn preview_agent_user_claims(claims: &PreviewAgentClaims) -> JwtClaims {
    JwtClaims {
        sub: claims.owner_user_id.to_string(),
        org: claims.org_id.to_string(),
        scopes: vec![Scope::TasksRead, Scope::WorkspacesRead],
        iat: claims.iat,
        exp: claims.exp,
        iss: claims.iss.clone(),
        aud: Some(g5_auth::preview_agent_audience(claims.task_id)),
        jti: Some(claims.jti.clone()),
        token_use: Some(g5_auth::PREVIEW_AGENT_TOKEN_USE.to_string()),
        client: None,
    }
}

/// Extracts every `__Host-g5_preview` value from the request's `Cookie`
/// header(s).
///
/// Multiple simultaneous previews mean multiple same-name cookies scoped to
/// different preview origins; the browser only attaches matching ones, but
/// all received candidates are returned so the caller can try each against
/// the path's workspace id.
fn preview_cookie_tickets(req: &Request<Body>) -> Vec<String> {
    let mut tickets = Vec::new();
    for value in req.headers().get_all(header::COOKIE) {
        // App cookies on isolated preview hosts may contain valid UTF-8.
        // HeaderValue::to_str rejects all non-ASCII bytes and would hide the
        // G5 ticket when both cookies appear in one request header.
        let Ok(raw) = std::str::from_utf8(value.as_bytes()) else {
            continue;
        };
        for pair in raw.split(';') {
            if let Some((name, val)) = pair.split_once('=') {
                let name = name.trim();
                if name == PREVIEW_TICKET_COOKIE_HOST && !val.trim().is_empty() {
                    tickets.push(val.trim().to_string());
                }
            }
        }
    }
    tickets
}

/// Constructs an error response with the standard JSON error envelope.
///
/// Extracts the request ID from request extensions (set by
/// [`RequestIdMiddleware`]) and includes it in the error response for
/// correlation.
///
/// The response follows the standard envelope format:
///
/// ```json
/// {
///   "error": {
///     "code": "UNAUTHORIZED",
///     "message": "Missing or invalid authentication token",
///     "details": {},
///     "requestId": "req_abc123"
///   }
/// }
/// ```
fn build_error_response(error: G5Error, req: &Request<Body>) -> Response<Body> {
    // Extract request_id from extensions (set by RequestIdMiddleware)
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_default();

    let status = error.status_code();
    let error_response = error.to_error_response(request_id);
    let envelope = ErrorEnvelope {
        error: error_response,
    };

    let body = serde_json::to_string(&envelope).unwrap_or_else(|_| {
        r#"{"error":{"code":"INTERNAL_ERROR","message":"An internal error occurred","details":{},"requestId":""}}"#.to_string()
    });

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Request, Response, StatusCode};
    use std::future::Future;
    use std::pin::Pin;
    use uuid::Uuid;

    // -- Test helper services --

    /// Helper service that echoes back a 200 OK with an empty body.
    #[derive(Clone)]
    #[allow(dead_code)]
    struct OkService;

    impl Service<Request<Body>> for OkService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            Box::pin(async {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::empty())
                    .unwrap())
            })
        }
    }

    /// Helper service that extracts AuthContext from extensions and returns
    /// it as a JSON body.
    #[derive(Clone)]
    #[allow(dead_code)]
    struct ExtractAuthService;

    impl Service<Request<Body>> for ExtractAuthService {
        type Response = Response<Body>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: Request<Body>) -> Self::Future {
            let auth = req.extensions().get::<AuthContext>().cloned();
            let agent = req.extensions().get::<PreviewAgentContext>().cloned();
            Box::pin(async move {
                let body_str = match auth {
                    Some(AuthContext::User(claims)) => match agent {
                        Some(PreviewAgentContext(agent)) => {
                            format!("user:{};agent:{}", claims.sub, agent.workspace_id)
                        }
                        None => format!("user:{}", claims.sub),
                    },
                    Some(AuthContext::Host { host_id, .. }) => {
                        format!("host:{}", host_id)
                    }
                    None => "none".to_string(),
                };
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from(body_str))
                    .unwrap())
            })
        }
    }

    // -- is_exempt_path tests --

    #[test]
    fn test_exempt_login_path() {
        assert!(is_exempt_path("/api/v1/auth/login/github"));
        // The handler validates provider names after this prefix exemption.
        assert!(is_exempt_path("/api/v1/auth/login/unconfigured"));
    }

    #[test]
    fn test_exempt_callback_path() {
        assert!(is_exempt_path("/api/v1/auth/callback/github"));
        assert!(is_exempt_path("/api/v1/auth/callback/unconfigured"));
    }

    #[test]
    fn test_exempt_health_path() {
        assert!(is_exempt_path("/api/v1/instance/health"));
    }

    #[test]
    fn test_exempt_version_path() {
        assert!(is_exempt_path("/api/v1/instance/version"));
    }

    #[test]
    fn test_exempt_instance_info_path() {
        assert!(is_exempt_path("/api/v1/instance/info"));
    }

    #[test]
    fn test_exempt_refresh_path() {
        assert!(is_exempt_path("/api/v1/auth/refresh"));
    }

    #[test]
    fn test_exempt_cli_device_start_and_token_paths() {
        assert!(is_exempt_path("/api/v1/auth/cli/start"));
        assert!(is_exempt_path("/api/v1/auth/cli/token"));
        assert!(!is_exempt_path("/api/v1/auth/cli/approve"));
    }

    #[test]
    fn test_exempt_github_app_callback_paths_only() {
        assert!(is_exempt_path("/api/v1/github-app/setup"));
        assert!(is_exempt_path("/api/v1/github-app/oauth/callback"));
        assert!(is_exempt_path("/api/v1/github-app/reconnect/select"));
        assert!(!is_exempt_path("/api/v1/github-app"));
        assert!(!is_exempt_path("/api/v1/github-app/setup/extra"));
        assert!(!is_exempt_path(
            "/api/v1/orgs/00000000-0000-0000-0000-000000000001/github-app/binding"
        ));
    }

    #[test]
    fn test_exempt_self_hosted_license_paths() {
        assert!(is_exempt_path("/api/v1/admin/licenses"));
        assert!(is_exempt_path("/api/v1/registry/token"));
        assert!(!is_exempt_path("/api/v1/admin"));
        assert!(!is_exempt_path("/api/v1/registry"));
    }

    #[test]
    fn test_non_exempt_paths() {
        assert!(!is_exempt_path("/api/v1/workspaces"));
        assert!(!is_exempt_path("/api/v1/jobs"));
        assert!(!is_exempt_path("/api/internal/tasks"));
        assert!(!is_exempt_path(
            "/api/internal/workspace-incarnations/00000000-0000-0000-0000-000000000001/flushes/start"
        ));
        assert!(!is_exempt_path("/"));
    }

    #[test]
    fn terminal_websocket_exemption_requires_the_exact_workspace_route() {
        let workspace_id = Uuid::new_v4();
        assert!(is_exempt_path(&format!(
            "/api/v1/workspaces/{workspace_id}/terminal"
        )));
        assert!(!is_exempt_path("/api/v1/workspaces/not-a-uuid/terminal"));
        assert!(!is_exempt_path(&format!(
            "/api/v1/workspaces/{workspace_id}/terminal/extra"
        )));
        assert!(!is_exempt_path(&format!(
            "/api/v1/workspaces/{workspace_id}"
        )));
    }

    // -- extract_bearer_token tests --

    #[test]
    fn test_extract_bearer_token_valid() {
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Bearer mytoken123")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("mytoken123".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing_header() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_bearer_token(&req), None);
    }

    #[test]
    fn test_extract_bearer_token_wrong_scheme() {
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Basic dXNlcjpwYXNz")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), None);
    }

    #[test]
    fn test_extract_bearer_token_empty_token() {
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Bearer ")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), None);
    }

    #[test]
    fn test_extract_bearer_token_no_space() {
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Bearertoken")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), None);
    }

    // -- AuthContext method tests --

    #[test]
    fn test_auth_context_user_id() {
        let claims = JwtClaims {
            sub: "user-123".to_string(),
            org: "org-456".to_string(),
            scopes: vec![Scope::WorkspacesRead],
            iat: 0,
            exp: 3600,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let ctx = AuthContext::User(claims);
        assert_eq!(ctx.user_id(), Some("user-123"));
    }

    #[test]
    fn test_auth_context_org_id() {
        let claims = JwtClaims {
            sub: "user-123".to_string(),
            org: "org-456".to_string(),
            scopes: vec![],
            iat: 0,
            exp: 3600,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let ctx = AuthContext::User(claims);
        assert_eq!(ctx.org_id().as_deref(), Some("org-456"));
    }

    #[test]
    fn test_auth_context_scopes() {
        let claims = JwtClaims {
            sub: "u".to_string(),
            org: "o".to_string(),
            scopes: vec![Scope::WorkspacesRead, Scope::WorkspacesWrite],
            iat: 0,
            exp: 3600,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let ctx = AuthContext::User(claims);
        assert_eq!(
            ctx.scopes(),
            &[Scope::WorkspacesRead, Scope::WorkspacesWrite]
        );
    }

    #[test]
    fn test_auth_context_claims() {
        let claims = JwtClaims {
            sub: "user-123".to_string(),
            org: "org-456".to_string(),
            scopes: vec![Scope::WorkspacesRead],
            iat: 100,
            exp: 3700,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let ctx = AuthContext::User(claims.clone());
        let extracted = ctx.claims().unwrap();
        assert_eq!(extracted.sub, "user-123");
        assert_eq!(extracted.org, "org-456");
    }

    #[test]
    fn test_auth_context_debug_and_clone() {
        let claims = JwtClaims {
            sub: "u".to_string(),
            org: "o".to_string(),
            scopes: vec![],
            iat: 0,
            exp: 3600,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let ctx = AuthContext::User(claims);
        let cloned = ctx.clone();
        assert!(cloned.is_user());
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("User"));
    }

    // -- build_error_response tests --

    // -- AuthContext::Host accessor tests (rev-7 §5.7) --

    #[test]
    fn host_context_actor_org_and_scope_accessors() {
        let host_id: Uuid = "11111111-1111-1111-1111-111111111111".parse().unwrap();
        let org_id: Uuid = "22222222-2222-2222-2222-222222222222".parse().unwrap();
        let ctx = AuthContext::Host {
            host_id,
            org_id,
            scopes: vec![Scope::WorkerHost],
        };
        assert!(ctx.is_host());
        assert!(!ctx.is_user());
        assert_eq!(
            ctx.actor_id().as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(
            ctx.host_id_str().as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(ctx.host_id(), Some(host_id));
        assert_eq!(
            ctx.org_id().as_deref(),
            Some("22222222-2222-2222-2222-222222222222")
        );
        assert_eq!(ctx.scopes(), &[Scope::WorkerHost]);
        // User-only accessor returns None for Host
        assert!(ctx.user_id().is_none());
        // claims() returns None for non-User
        assert!(ctx.claims().is_none());
    }

    // These auth routes retain their existing Unauthorized policy; the shared
    // AuthContext::require_user_id helper instead rejects hosts as Forbidden.
    #[test]
    fn extract_user_id_rejects_host_as_unauthorized() {
        let auth = AuthContext::Host {
            host_id: uuid::Uuid::new_v4(),
            org_id: uuid::Uuid::new_v4(),
            scopes: vec![g5_auth::Scope::WorkerHost],
        };
        let err = extract_user_id(&auth).unwrap_err();
        assert!(matches!(err.inner, G5Error::Unauthorized));
    }

    #[test]
    fn extract_session_user_id_refuses_api_keys_and_accepts_user_jwts() {
        let claims = |iss: Option<&str>| g5_auth::JwtClaims {
            sub: uuid::Uuid::new_v4().to_string(),
            org: uuid::Uuid::new_v4().to_string(),
            scopes: vec![g5_auth::Scope::OrgAdmin],
            iat: 0,
            exp: 0,
            iss: iss.map(str::to_string),
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let err =
            extract_session_user_id(&AuthContext::User(claims(Some(API_KEY_ISSUER)))).unwrap_err();
        match err.inner {
            G5Error::Forbidden { missing_scopes } => {
                assert_eq!(missing_scopes, vec!["user_session_required".to_string()])
            }
            other => panic!("Expected Forbidden, got {other:?}"),
        }
        let user = claims(Some("g5"));
        let expected = uuid::Uuid::parse_str(&user.sub).unwrap();
        assert_eq!(
            extract_session_user_id(&AuthContext::User(user)).unwrap(),
            expected
        );
        let host = AuthContext::Host {
            host_id: uuid::Uuid::new_v4(),
            org_id: uuid::Uuid::new_v4(),
            scopes: vec![g5_auth::Scope::WorkerHost],
        };
        assert!(matches!(
            extract_session_user_id(&host).unwrap_err().inner,
            G5Error::Unauthorized
        ));
    }

    #[test]
    fn extract_user_id_preserves_invalid_subject_error() {
        let claims = g5_auth::JwtClaims {
            sub: "not-a-uuid".into(),
            org: uuid::Uuid::new_v4().to_string(),
            scopes: vec![],
            iat: 0,
            exp: 0,
            iss: None,
            aud: None,
            jti: None,
            token_use: None,
            client: None,
        };
        let err = extract_user_id(&AuthContext::User(claims)).unwrap_err();
        match err.inner {
            G5Error::Internal { message } => assert_eq!(message, "Invalid user ID in JWT"),
            other => panic!("Expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn user_context_require_user_id_returns_subject_uuid() {
        let user_id = Uuid::new_v4();
        let ctx = AuthContext::User(user_claims(user_id, Uuid::new_v4()));
        assert_eq!(ctx.require_user_id().unwrap(), user_id);
    }

    #[test]
    fn user_context_require_user_id_rejects_invalid_subject() {
        let mut claims = user_claims(Uuid::new_v4(), Uuid::new_v4());
        claims.sub = "not-a-uuid".into();
        match AuthContext::User(claims).require_user_id() {
            Err(G5Error::Internal { message }) => {
                assert_eq!(message, "Invalid user ID in JWT claims");
            }
            other => panic!("Expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn host_context_require_user_id_is_forbidden() {
        let ctx = AuthContext::Host {
            host_id: "11111111-1111-1111-1111-111111111111".parse().unwrap(),
            org_id: "22222222-2222-2222-2222-222222222222".parse().unwrap(),
            scopes: vec![Scope::WorkerHost],
        };
        match ctx.require_user_id() {
            Err(G5Error::Forbidden { missing_scopes }) => {
                assert_eq!(missing_scopes, vec!["user_auth_required".to_string()]);
            }
            other => panic!("Expected Forbidden, got {:?}", other),
        }
    }

    // -- Mixed-scope rejection at middleware (rev-7 §5.7 D4) --

    // -- AuthMiddleware end-to-end routing tests (rev-7 §5.7 D4) --

    /// Test ES256 key pair, identical to the keys in `auth/jwt.rs` tests.
    const TEST_PRIVATE_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg0J63p4NbSY225Nco
6sEKek+VGL+z0Bt+/o3rkfIBFAihRANCAAQV+5QLRzUNaBAZyRaVIOWFtfZpe2ly
7GviW9bv0coSK0uGOFk/8N5UiSrpwDjuEN7DIIdAPwvDY26VHa9qhsW+
-----END PRIVATE KEY-----";

    const TEST_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEFfuUC0c1DWgQGckWlSDlhbX2aXtp
cuxr4lvW79HKEitLhjhZP/DeVIkq6cA47hDewyCHQD8Lw2NulR2vaobFvg==
-----END PUBLIC KEY-----";

    /// Sign a `JwtClaims` payload with the test ES256 key.
    fn sign(claims: &JwtClaims) -> String {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
        let key = EncodingKey::from_ec_pem(TEST_PRIVATE_KEY_PEM.as_bytes()).unwrap();
        encode(&Header::new(Algorithm::ES256), claims, &key).unwrap()
    }

    fn host_claims(host_id: Uuid, org_id: Uuid) -> JwtClaims {
        let now = chrono::Utc::now().timestamp();
        JwtClaims {
            sub: host_id.to_string(),
            org: org_id.to_string(),
            scopes: vec![Scope::WorkerHost],
            iat: now,
            exp: now + 3600,
            iss: Some("g5-api".into()),
            aud: Some("g5".into()),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: None,
            client: None,
        }
    }

    fn user_claims(user_id: Uuid, org_id: Uuid) -> JwtClaims {
        let now = chrono::Utc::now().timestamp();
        JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes: vec![Scope::WorkspacesRead],
            iat: now,
            exp: now + 3600,
            iss: Some("g5-api".into()),
            aud: Some("g5".into()),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: None,
            client: None,
        }
    }

    fn build_middleware() -> AuthMiddleware<ExtractAuthService> {
        build_middleware_with_hosts(&[])
    }

    /// Build the middleware with a `StaticHostAuthValidator` pre-seeded with
    /// the given `(host_id, org_id, status)` triples. Tests pass `&[]` to
    /// model "no hosts registered" (every `host_jwt` then 401s with NotFound).
    fn build_middleware_with_hosts(
        hosts: &[(Uuid, Uuid, &str)],
    ) -> AuthMiddleware<ExtractAuthService> {
        use crate::auth::host_validator::test_support::StaticHostAuthValidator;

        let validator =
            Arc::new(JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5")).unwrap());
        let mut host_validator = StaticHostAuthValidator::new();
        for (h, o, s) in hosts {
            host_validator = host_validator.with_host(*h, *o, s);
        }
        let layer = AuthLayer::new(validator)
            .with_host_validator(Arc::new(host_validator) as Arc<dyn HostAuthValidator>);
        layer.layer(ExtractAuthService)
    }

    async fn body_text(resp: Response<Body>) -> (StatusCode, String) {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let s = String::from_utf8_lossy(&bytes).to_string();
        (status, s)
    }

    #[tokio::test]
    async fn host_jwt_on_host_only_internal_path_lands_as_host_context() {
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "ready")]);
        let resp = mw.call(req).await.unwrap();
        let (status, body) = body_text(resp).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, format!("host:{}", host_id));
    }

    #[tokio::test]
    async fn host_jwt_on_non_host_internal_path_is_unauthorized() {
        // rev-7 §5.7 [D4]: host JWTs MUST NOT pass the internal-token gate
        // for arbitrary internal paths. A host token presented on
        // `/api/internal/usage` (or any non-host internal endpoint)
        // is a scope-creep attempt; the middleware must reject before any
        // handler runs.
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/usage")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        // Even with a registered, ready host the path itself is wrong: the
        // middleware refuses without consulting the validator.
        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "ready")]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn host_jwt_missing_in_worker_hosts_is_unauthorized() {
        // rev-7 §5.7 [D5]: DB-liveness — the JWT is signed, but the host_id
        // does not correspond to a row in worker_hosts.
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        // No hosts registered: the validator returns NotFound.
        let mut mw = build_middleware_with_hosts(&[]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn host_jwt_with_wrong_org_id_is_unauthorized() {
        // rev-7 §5.7 [D5]: the JWT carries an org claim that does not match
        // the worker_hosts row's owning org — token-reassignment / theft.
        let host_id = Uuid::new_v4();
        let row_org = Uuid::new_v4();
        let claimed_org = Uuid::new_v4();
        let token = sign(&host_claims(host_id, claimed_org));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        // Host exists but in a different org from the one in the JWT.
        let mut mw = build_middleware_with_hosts(&[(host_id, row_org, "ready")]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn host_jwt_for_terminated_host_is_unauthorized() {
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "terminated")]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn host_jwt_for_draining_host_is_accepted_for_reconciliation() {
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "draining")]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn host_jwt_for_provisioning_host_is_accepted() {
        // Foundational state-machine entry: a fresh host's first request
        // arrives before its status moves to `ready`. Accepting
        // `provisioning` avoids a chicken-and-egg deadlock at bring-up.
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "provisioning")]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn user_jwt_on_host_only_path_is_rejected() {
        // A user JWT lacks both Internal and WorkerHost scopes, so the
        // existing internal-path branch returns Forbidden. This test
        // documents the contract for the host-only surface specifically.
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&user_claims(user_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/host/status")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn host_jwt_on_public_api_path_is_rejected() {
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri("/api/v1/workspaces")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "host_jwt on public API path must be rejected"
        );
    }

    // -- Preview ticket-cookie carve-out ([I13]/[D12], issue G5Labs/G5#736) --

    fn preview_ticket_claims(user_id: Uuid, org_id: Uuid, workspace_id: Uuid) -> JwtClaims {
        let now = chrono::Utc::now().timestamp();
        JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes: vec![Scope::TasksRead, Scope::WorkspacesRead],
            iat: now,
            exp: now + 1800,
            iss: Some("g5-api".into()),
            aud: Some(g5_auth::preview_ticket_audience(workspace_id)),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: Some(g5_auth::PREVIEW_TOKEN_USE.into()),
            client: None,
        }
    }

    #[test]
    fn preview_path_workspace_id_matches_proxy_paths_only() {
        let workspace = Uuid::new_v4();
        assert_eq!(
            preview_path_workspace_id(&format!("/api/v1/preview/{workspace}")),
            Some(workspace)
        );
        assert_eq!(
            preview_path_workspace_id(&format!("/api/v1/preview/{workspace}/")),
            Some(workspace)
        );
        assert_eq!(
            preview_path_workspace_id(&format!("/api/v1/preview/{workspace}/assets/app.js")),
            Some(workspace)
        );
        // The mint endpoint's literal `session` segment is not a UUID and
        // must keep requiring a Bearer credential.
        assert_eq!(
            preview_path_workspace_id(&format!("/api/v1/preview/session/{workspace}")),
            None
        );
        // Non-preview paths never activate the carve-out.
        assert_eq!(preview_path_workspace_id("/api/v1/workspaces"), None);
        assert_eq!(preview_path_workspace_id("/api/v1/preview/"), None);
    }

    fn nav_headers(pairs: &[(&str, &str)]) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                http::HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn browser_navigation_classifier_d18() {
        // Top-level navigation and same-origin iframe navigation qualify.
        assert!(is_browser_html_navigation(&nav_headers(&[
            ("sec-fetch-dest", "document"),
            ("sec-fetch-mode", "navigate"),
        ])));
        assert!(is_browser_html_navigation(&nav_headers(&[
            ("sec-fetch-dest", "iframe"),
            ("sec-fetch-mode", "navigate"),
            ("accept", "text/html"),
        ])));
        // XHR/fetch — even when it asks for HTML — never qualifies.
        assert!(!is_browser_html_navigation(&nav_headers(&[
            ("sec-fetch-dest", "empty"),
            ("sec-fetch-mode", "cors"),
            ("accept", "text/html"),
        ])));
        // A present non-document dest never falls through to the Accept
        // fallback, and document with a non-navigate mode is rejected.
        assert!(!is_browser_html_navigation(&nav_headers(&[
            ("sec-fetch-dest", "image"),
            ("accept", "text/html"),
        ])));
        assert!(!is_browser_html_navigation(&nav_headers(&[
            ("sec-fetch-dest", "document"),
            ("sec-fetch-mode", "cors"),
        ])));
        // Speculative loads never mint credentials.
        assert!(!is_browser_html_navigation(&nav_headers(&[
            ("sec-fetch-dest", "document"),
            ("sec-fetch-mode", "navigate"),
            ("sec-purpose", "prefetch"),
        ])));
        assert!(!is_browser_html_navigation(&nav_headers(&[
            ("purpose", "prefetch"),
            ("accept", "text/html"),
        ])));
        // Fetch-metadata-absent old browsers: Accept decides.
        assert!(is_browser_html_navigation(&nav_headers(&[(
            "accept",
            "text/html,application/xhtml+xml"
        )])));
        assert!(!is_browser_html_navigation(&nav_headers(&[(
            "accept",
            "application/json"
        )])));
    }

    #[test]
    fn reauth_marker_freshness() {
        let now = 1_000_000;
        assert!(has_fresh_reauth_marker(
            Some(&format!("a=1&__g5_reauth={}", now - 10)),
            now
        ));
        // Stale markers are ignored so a later expiry heals again.
        assert!(!has_fresh_reauth_marker(
            Some(&format!("__g5_reauth={}", now - 61)),
            now
        ));
        assert!(!has_fresh_reauth_marker(Some("a=1"), now));
        assert!(!has_fresh_reauth_marker(Some("__g5_reauth=garbage"), now));
        assert!(!has_fresh_reauth_marker(None, now));
    }

    #[test]
    fn reauth_redirect_shape_and_gating() {
        let workspace = Uuid::new_v4();
        let origin = "https://dev.g5labs.ai";
        let nav = nav_headers(&[
            ("sec-fetch-dest", "document"),
            ("sec-fetch-mode", "navigate"),
        ]);
        let path = format!("/api/v1/preview/{workspace}/dashboard");
        let response = preview_reauth_redirect(
            origin,
            workspace,
            &path,
            Some("tab=1"),
            &nav,
            &http::Method::GET,
        )
        .expect("navigation must redirect");
        assert_eq!(response.status(), http::StatusCode::FOUND);
        let location = response.headers()[http::header::LOCATION].to_str().unwrap();
        assert!(
            location.starts_with(&format!(
                "{origin}/api/v1/auth/preview/reauth/{workspace}?return="
            )),
            "{location}"
        );
        // return carries the preview-root-relative path+query, encoded.
        assert!(location.contains("%2Fdashboard%3Ftab%3D1"), "{location}");
        assert_eq!(response.headers()[http::header::CACHE_CONTROL], "no-store");

        // Non-GET, XHR, and fresh-marker requests keep the 401 path.
        assert!(
            preview_reauth_redirect(origin, workspace, &path, None, &nav, &http::Method::POST)
                .is_none()
        );
        let xhr = nav_headers(&[("sec-fetch-dest", "empty"), ("sec-fetch-mode", "cors")]);
        assert!(
            preview_reauth_redirect(origin, workspace, &path, None, &xhr, &http::Method::GET)
                .is_none()
        );
        let fresh = format!("__g5_reauth={}", chrono::Utc::now().timestamp());
        assert!(preview_reauth_redirect(
            origin,
            workspace,
            &path,
            Some(&fresh),
            &nav,
            &http::Method::GET
        )
        .is_none());
    }

    #[test]
    fn reauth_endpoint_exemption_is_exact_shape() {
        let workspace = Uuid::new_v4();
        assert!(is_exempt_path(&format!(
            "/api/v1/auth/preview/reauth/{workspace}"
        )));
        assert!(!is_exempt_path("/api/v1/auth/preview/reauth/not-a-uuid"));
        assert!(!is_exempt_path(&format!(
            "/api/v1/auth/preview/reauth/{workspace}/extra"
        )));
        assert!(!is_exempt_path("/api/v1/auth/preview/reauth/"));
    }

    #[test]
    fn preview_cookie_tickets_extracts_only_host_ticket_pairs() {
        let req = Request::builder()
            .uri("/api/v1/preview/x")
            .header(header::COOKIE, "a=b; __Host-g5_preview=tok1; theme=dark")
            .header(header::COOKIE, "__Host-g5_preview=tok2")
            .body(Body::empty())
            .unwrap();
        assert_eq!(preview_cookie_tickets(&req), vec!["tok1", "tok2"]);

        let with_utf8_app_cookie = Request::builder()
            .uri("/api/v1/preview/x")
            .header(header::COOKIE, "app=café; __Host-g5_preview=tok3")
            .body(Body::empty())
            .unwrap();
        assert_eq!(preview_cookie_tickets(&with_utf8_app_cookie), vec!["tok3"]);

        // The retired platform-origin cookie name is no credential at all.
        let none = Request::builder()
            .uri("/api/v1/preview/x")
            .header(
                header::COOKIE,
                "a=b; __Host-g5_previewX=evil; __Host-g5_preview=; g5_preview=old",
            )
            .body(Body::empty())
            .unwrap();
        assert!(preview_cookie_tickets(&none).is_empty());
    }

    /// A preview agent credential from `caller`'s incarnation for `task`.
    fn preview_agent_credential(caller: Uuid, owner: Uuid, task: Uuid) -> String {
        let minter = g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api", "g5").unwrap();
        let validator = JwtValidator::new(TEST_PUBLIC_KEY_PEM, Some("g5-api"), Some("g5")).unwrap();
        let incarnation = minter
            .mint_workspace_incarnation(Uuid::new_v4(), caller, owner, Uuid::new_v4(), 1, 600)
            .unwrap();
        let incarnation = validator
            .validate_workspace_incarnation(&incarnation)
            .unwrap();
        minter
            .mint_preview_agent(&incarnation, Uuid::new_v4(), task)
            .unwrap()
    }

    #[tokio::test]
    async fn preview_agent_credential_authenticates_only_its_preview_origin_without_browser_metadata(
    ) {
        let (caller, owner, task) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let credential = preview_agent_credential(caller, owner, task);
        let request = |task_path: Uuid, origin: Option<Uuid>| {
            let mut builder = Request::builder()
                .uri(format!("/api/v1/preview/{task_path}/api/items"))
                .header(g5_auth::PREVIEW_AGENT_HEADER, credential.as_str());
            if let Some(origin) = origin {
                builder = builder.extension(translated(origin));
            }
            builder
        };

        let resp = build_middleware()
            .call(request(task, Some(task)).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, body) = body_text(resp).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, format!("user:{owner};agent:{caller}"));

        // Another task's origin, the platform path and a browser are refused.
        let other = Uuid::new_v4();
        for (what, req) in [
            ("other task", request(other, Some(other))),
            ("platform path", request(task, None)),
            (
                "browser",
                request(task, Some(task)).header("sec-fetch-mode", "cors"),
            ),
            (
                "repeated header",
                request(task, Some(task)).header(g5_auth::PREVIEW_AGENT_HEADER, "second"),
            ),
        ] {
            let resp = build_middleware()
                .call(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{what}");
        }
    }

    #[tokio::test]
    async fn invalid_preview_agent_credential_never_falls_back_to_the_cookie() {
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(
            user_id,
            Uuid::new_v4(),
            workspace_id,
        ));
        let req = Request::builder()
            .uri(format!("/api/v1/preview/{workspace_id}/"))
            .header(header::COOKIE, format!("__Host-g5_preview={ticket}"))
            .header(g5_auth::PREVIEW_AGENT_HEADER, "not-a-credential")
            .extension(translated(workspace_id))
            .body(Body::empty())
            .unwrap();
        let resp = build_middleware().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    fn translated(workspace_id: Uuid) -> PreviewHostOrigin {
        PreviewHostOrigin {
            workspace_id,
            host: format!("{workspace_id}.preview.g5.example"),
        }
    }

    #[tokio::test]
    async fn preview_cookie_authenticates_matching_preview_path() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(user_id, org_id, workspace_id));

        let req = Request::builder()
            .uri(format!("/api/v1/preview/{workspace_id}/"))
            .header(header::COOKIE, format!("__Host-g5_preview={ticket}"))
            .extension(translated(workspace_id))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        let (status, body) = body_text(resp).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            format!("user:{user_id}"),
            "cookie carve-out must insert a real scoped AuthContext::User"
        );
    }

    #[tokio::test]
    async fn preview_cookie_on_platform_path_is_unauthorized() {
        // `g5.security.mvp-exposure.preview`: a valid ticket presented on the
        // platform path (no host translation) authenticates nothing, under
        // either cookie name.
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(user_id, org_id, workspace_id));
        for name in ["__Host-g5_preview", "g5_preview"] {
            let req = Request::builder()
                .uri(format!("/api/v1/preview/{workspace_id}/"))
                .header(header::COOKIE, format!("{name}={ticket}"))
                .body(Body::empty())
                .unwrap();
            let mut mw = build_middleware();
            let resp = mw.call(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{name}");
        }
        // Translation for another workspace does not lend its cookie either.
        let req = Request::builder()
            .uri(format!("/api/v1/preview/{workspace_id}/"))
            .header(header::COOKIE, format!("__Host-g5_preview={ticket}"))
            .extension(translated(Uuid::new_v4()))
            .body(Body::empty())
            .unwrap();
        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn preview_cookie_for_other_workspace_is_unauthorized() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(user_id, org_id, Uuid::new_v4()));

        let path_job = Uuid::new_v4();
        let req = Request::builder()
            .uri(format!("/api/v1/preview/{path_job}/"))
            .header(header::COOKIE, format!("__Host-g5_preview={ticket}"))
            .extension(translated(path_job))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "workspace-A ticket presented on workspace-B's path must fail closed"
        );
    }

    #[tokio::test]
    async fn preview_cookie_outside_preview_paths_is_unauthorized() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(user_id, org_id, workspace_id));

        let req = Request::builder()
            .uri("/api/v1/workspaces")
            .header(header::COOKIE, format!("__Host-g5_preview={ticket}"))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "the preview ticket cookie is only a credential on preview proxy paths"
        );
    }

    #[tokio::test]
    async fn valid_preview_cookie_authenticates_despite_app_bearer() {
        // [I13]: on a preview path the ticket cookie is tried FIRST, so a
        // valid ticket authenticates even when the previewed app sends its OWN
        // (non-platform) Authorization bearer on every API call. Regression for
        // the login-bounce where g5-api 401'd the app's token before the relay
        // (field evidence 2026-07-07, BankWorkflow; chat f52ec430).
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(user_id, org_id, workspace_id));

        let req = Request::builder()
            .uri(format!(
                "/api/v1/preview/{workspace_id}/api/v1/approvals/pending"
            ))
            // The app's own foreign token — not a valid G5 platform JWT.
            .header(header::AUTHORIZATION, "Bearer app.hs512.token")
            .header(header::COOKIE, format!("__Host-g5_preview={ticket}"))
            .extension(translated(workspace_id))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        let (status, body) = body_text(resp).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the app's own bearer header must not block ticket cookie auth on preview paths"
        );
        assert_eq!(body, format!("user:{user_id}"));
    }

    #[tokio::test]
    async fn app_bearer_on_preview_path_without_valid_cookie_still_401s() {
        // Fail-closed: no valid ticket cookie means the preview carve-out
        // does not fire, and a non-platform bearer falls through to the
        // standard 401 — the cookie-first change does not weaken auth.
        let req = Request::builder()
            .uri(format!(
                "/api/v1/preview/{}/api/v1/approvals/pending",
                Uuid::new_v4()
            ))
            .header(header::AUTHORIZATION, "Bearer app.hs512.token")
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn preview_ticket_as_bearer_is_unauthorized() {
        // A preview ticket replayed as a Bearer credential is dead: the
        // standard validator rejects any token carrying `token_use`.
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let ticket = sign(&preview_ticket_claims(user_id, org_id, workspace_id));

        let req = Request::builder()
            .uri(format!("/api/v1/preview/{workspace_id}/"))
            .header(header::AUTHORIZATION, format!("Bearer {ticket}"))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // -- Preview reverse-tunnel plural-path exception (worker_hosts [D4],
    //    routes/internal [I27]) --

    #[test]
    fn is_host_only_internal_path_admits_tunnel_carrier_only() {
        // Singular host surface: always host-only.
        assert!(is_host_only_internal_path("/api/internal/host/status"));

        // Plural tunnel carrier route (routes/internal [I27]): admitted so the
        // host's own `host_jwt` can attach the preview reverse tunnel.
        let host_id = Uuid::new_v4();
        assert!(is_host_only_internal_path(&format!(
            "/api/internal/hosts/{host_id}/tunnel"
        )));

        // Bound is `ends_with("/tunnel")` — a future plural `/hosts/` route
        // must NOT silently inherit host-token reach.
        assert!(!is_host_only_internal_path(&format!(
            "/api/internal/hosts/{host_id}/other"
        )));
        assert!(!is_host_only_internal_path("/api/internal/hosts"));
        // A general internal path stays outside the host surface.
        assert!(!is_host_only_internal_path("/api/internal/usage"));
    }

    #[tokio::test]
    async fn host_jwt_on_tunnel_plural_path_lands_as_host_context() {
        // worker_hosts [D4]: the preview reverse-tunnel carrier route is the
        // plural `/api/internal/hosts/:hostId/tunnel`, dialed by the deployed
        // g5d with its `host_jwt`. The middleware must pass it through the
        // host-token gate and store `AuthContext::Host` so the
        // `AuthenticatedHost` extractor accepts it downstream.
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri(format!("/api/internal/hosts/{host_id}/tunnel"))
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "ready")]);
        let resp = mw.call(req).await.unwrap();
        let (status, body) = body_text(resp).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, format!("host:{}", host_id));
    }

    #[tokio::test]
    async fn user_jwt_on_tunnel_plural_path_is_rejected() {
        // A user JWT carries neither Internal nor WorkerHost scope, so the
        // tunnel carrier surface rejects it just like any other internal path.
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&user_claims(user_id, org_id));

        let req = Request::builder()
            .uri(format!("/api/internal/hosts/{user_id}/tunnel"))
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn host_jwt_on_non_tunnel_plural_path_is_unauthorized() {
        // Regression guard for the `ends_with("/tunnel")` bound: a `host_jwt`
        // on a NON-tunnel plural `/hosts/` path is still rejected at the gate,
        // even for a registered, ready host — the exception admits the tunnel
        // carrier only, never the whole plural namespace.
        let host_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&host_claims(host_id, org_id));

        let req = Request::builder()
            .uri(format!("/api/internal/hosts/{host_id}/other"))
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware_with_hosts(&[(host_id, org_id, "ready")]);
        let resp = mw.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn user_jwt_on_internal_path_is_forbidden() {
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let token = sign(&user_claims(user_id, org_id));

        let req = Request::builder()
            .uri("/api/internal/tasks/anything")
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let mut mw = build_middleware();
        let resp = mw.call(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_build_error_response_unauthorized() {
        let req = Request::builder()
            .uri("/api/v1/test")
            .body(Body::empty())
            .unwrap();

        let response = build_error_response(G5Error::Unauthorized, &req);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(content_type, "application/json");

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_obj = json.get("error").expect("missing 'error' key");
        assert_eq!(error_obj["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn test_build_error_response_with_request_id() {
        let mut req = Request::builder()
            .uri("/api/v1/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(RequestId("req_test123".to_string()));

        let response = build_error_response(G5Error::Unauthorized, &req);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_obj = json.get("error").unwrap();
        assert_eq!(error_obj["requestId"], "req_test123");
    }

    #[tokio::test]
    async fn test_build_error_response_forbidden_with_details() {
        let req = Request::builder()
            .uri("/api/v1/test")
            .body(Body::empty())
            .unwrap();

        let response = build_error_response(
            G5Error::Forbidden {
                missing_scopes: vec!["tasks:execute".to_string()],
            },
            &req,
        );
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_obj = json.get("error").unwrap();
        assert_eq!(error_obj["code"], "FORBIDDEN");
        // Details should contain missing_scopes
        let details = &error_obj["details"];
        assert!(details.get("missing_scopes").is_some());
    }

    #[test]
    fn workspace_incarnation_auth_covers_the_retained_internal_surface() {
        let id = Uuid::new_v4();
        assert!(!is_workspace_incarnation_internal_route(
            &http::Method::POST,
            &format!("/api/internal/workspaces/{id}/ontology/upsert")
        ));
        let allowed = [
            (
                http::Method::POST,
                format!("/api/internal/chat-sessions/{id}/subscription-usage"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/chat-sessions/{id}/codex-auth"),
            ),
            (
                http::Method::PUT,
                format!("/api/internal/chat-sessions/{id}/session-files"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/chat-sessions/{id}/session-files/manifest"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/chat-sessions/{id}/session-files/content"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/chat-sessions/{id}/session-files/published"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/chat-sessions/{id}/session-files/begin"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/chat-sessions/{id}/session-files/publish"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/chat-sessions/{id}/attachments"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/chat-attachments/{id}/content"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/manifest"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/session-files/pending"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/source-control/save"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/issue-links"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/issue-links/{id}"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/issue-comments"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/issue-links"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/issues"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/issues/view"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/issues/search"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/issues/attachments"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/issues/attachments/content"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/issues/attachments"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/pull-requests/github/attachments"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/pull-requests/github/attachments/content"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/issue-links/unlink"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/files"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/platform/issues"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspace-incarnations/{id}/flushes/start"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspace-incarnations/{id}/flushes/{id}/complete"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspace-incarnations/{id}/flushes/{id}/fail"),
            ),
            (
                http::Method::PUT,
                format!("/api/internal/docs/{id}/content"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/questions"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/questions"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/questions/answer"),
            ),
        ];
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/info")
        ));
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::POST,
            &format!("/api/internal/workspaces/{id}/pull-requests")
        ));
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/github")
        ));
        assert!(!is_workspace_incarnation_internal_route(
            &http::Method::POST,
            &format!("/api/internal/workspaces/{id}/pull-requests/github")
        ));
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/targets")
        ));
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::POST,
            &format!("/api/internal/workspaces/{id}/pull-requests/preview")
        ));
        assert!(!is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/preview")
        ));
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/{id}/description")
        ));
        assert!(is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/descriptions")
        ));
        assert!(!is_workspace_incarnation_internal_route(
            &http::Method::GET,
            &format!("/api/internal/workspaces/{id}/pull-requests/not-a-uuid/description")
        ));
        assert!(!is_workspace_incarnation_internal_route(
            &http::Method::POST,
            &format!("/api/internal/workspaces/{id}/pull-requests/{id}/description")
        ));
        for leaf in ["github/comments", "github/threads/resolve"] {
            assert!(is_workspace_incarnation_internal_route(
                &http::Method::POST,
                &format!("/api/internal/workspaces/{id}/pull-requests/{leaf}")
            ));
            assert!(!is_workspace_incarnation_internal_route(
                &http::Method::GET,
                &format!("/api/internal/workspaces/{id}/pull-requests/{leaf}")
            ));
        }
        for (method, path) in [
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/pull-requests"),
            ),
            (
                http::Method::POST,
                "/api/internal/workspaces/not-a-uuid/pull-requests".into(),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/pull-requests/info"),
            ),
            (
                http::Method::GET,
                "/api/internal/workspaces/not-a-uuid/pull-requests/info".into(),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/pull-requests/other"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/questions/answer"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/questions/clear"),
            ),
            (
                http::Method::POST,
                "/api/internal/workspaces/not-a-uuid/questions".into(),
            ),
        ] {
            assert!(!is_workspace_incarnation_internal_route(&method, &path));
        }
        for (method, path) in allowed {
            assert!(
                is_workspace_incarnation_internal_route(&method, &path),
                "{method} {path} must accept workspace-incarnation auth"
            );
        }

        // Workspace-agent operations (G5-Lean#710).
        let agent = format!("/api/internal/workspace-agent/{id}");
        for (method, path) in [
            (http::Method::GET, format!("{agent}/workspaces")),
            (http::Method::GET, format!("{agent}/children")),
            (http::Method::GET, format!("{agent}/workspaces/{id}")),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/sessions"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions/{id}/messages"),
            ),
            (http::Method::POST, format!("{agent}/workspaces/{id}/start")),
            (http::Method::POST, format!("{agent}/workspaces/{id}/stop")),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/delete"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/pull-request"),
            ),
            (
                http::Method::POST,
                format!("{agent}/pull-requests/{id}/merge"),
            ),
            (http::Method::GET, format!("{agent}/sessions/{id}/messages")),
            (http::Method::GET, format!("{agent}/sessions/{id}/timeline")),
            (http::Method::POST, format!("{agent}/fork")),
            (http::Method::GET, format!("{agent}/users")),
            (http::Method::GET, format!("{agent}/stop-owner-workspaces")),
            (http::Method::POST, format!("{agent}/stop-owner-workspaces")),
            (
                http::Method::POST,
                format!("{agent}/pull-requests/{id}/close"),
            ),
            (
                http::Method::POST,
                format!("{agent}/pull-requests/{id}/refresh"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions/{id}/stop"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions/{id}/background-end"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions/{id}/rename"),
            ),
            (http::Method::GET, format!("{agent}/sessions/{id}/inputs")),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions/{id}/inputs/{id}/withdraw"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/tasks/{id}/cancel"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/approvals"),
            ),
            (http::Method::GET, format!("{agent}/approval-activity")),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/approvals/reviews"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/approvals/requests/{id}/cancel"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/approvals/reviews/{id}/report"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/approvals/reviews/{id}/transcript"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/approvals/reviews/{id}/commits"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/pull-requests"),
            ),
            (http::Method::POST, format!("{agent}/issue-state")),
            (http::Method::POST, format!("{agent}/issue-edit")),
            (http::Method::GET, format!("{agent}/issue-labels")),
            (http::Method::POST, format!("{agent}/issue-labels")),
            (http::Method::POST, format!("{agent}/issue-labels/edit")),
            (http::Method::POST, format!("{agent}/issue-labels/delete")),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/preview"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/preview"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/preview/cancel"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/preview/credential"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/repos/{id}/preview-applications"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/repos/{id}/preview-profiles"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/repos/{id}/preview-profiles"),
            ),
            (http::Method::GET, format!("{agent}/workspaces/{id}/files")),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/files/content"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/files/write"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/files/delete"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/files/rename"),
            ),
        ] {
            assert!(
                is_workspace_incarnation_internal_route(&method, &path),
                "{method} {path} must accept workspace-incarnation auth"
            );
        }
        for (method, path) in [
            (http::Method::POST, format!("{agent}/workspaces")),
            (http::Method::DELETE, format!("{agent}/workspaces/{id}")),
            (http::Method::GET, format!("{agent}/workspaces/{id}/start")),
            (http::Method::GET, format!("{agent}/fork")),
            (http::Method::POST, format!("{agent}/users")),
            (
                http::Method::POST,
                format!("{agent}/sessions/{id}/messages"),
            ),
            (
                http::Method::GET,
                "/api/internal/workspace-agent/not-a-uuid/children".into(),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/not-a-uuid/stop"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/transfer"),
            ),
            (
                http::Method::PUT,
                format!("{agent}/workspaces/{id}/files/write"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/files/src/main.rs"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/approvals/decisions"),
            ),
            // Review evidence is read-only, of a named review, and never
            // the cleanup retry a person may run.
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/approvals/reviews/{id}/report"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/approvals/reviews/{id}/cleanup"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/approvals/reviews/{id}/cleanup"),
            ),
            (
                http::Method::GET,
                format!("{agent}/workspaces/{id}/approvals/reviews/not-a-uuid/report"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/tasks/{id}/pause"),
            ),
            (
                http::Method::POST,
                format!("{agent}/workspaces/{id}/sessions/{id}/inputs/{id}/delete"),
            ),
        ] {
            assert!(
                !is_workspace_incarnation_internal_route(&method, &path),
                "{method} {path} must remain outside workspace-incarnation auth"
            );
        }

        for (method, path) in [
            (http::Method::GET, format!("/api/internal/jobs/{id}/tasks")),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/review/comments"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/review/publish-findings"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/workspaces/{id}/review/rounds/{id}"),
            ),
            (
                http::Method::POST,
                format!("/api/internal/workspaces/{id}/review/suggestions/{id}/decide"),
            ),
            (http::Method::POST, format!("/api/internal/jobs/{id}/tasks")),
            (
                http::Method::POST,
                format!("/api/internal/tasks/{id}/tasks"),
            ),
            (
                http::Method::GET,
                format!("/api/internal/hosts/{id}/tunnel"),
            ),
            (http::Method::POST, "/api/internal/usage".to_string()),
        ] {
            assert!(
                !is_workspace_incarnation_internal_route(&method, &path),
                "{method} {path} must remain outside workspace-incarnation auth"
            );
        }
    }

    fn browser_headers(site: &str) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        headers.insert("sec-fetch-site", site.parse().unwrap());
        headers
    }

    fn session(client: Option<&str>) -> AuthContext {
        let mut claims = user_claims(Uuid::new_v4(), Uuid::new_v4());
        claims.client = client.map(str::to_owned);
        AuthContext::User(claims)
    }

    #[test]
    fn browser_session_guard_admits_only_a_browser_session_sent_by_a_browser() {
        let ok = browser_headers("same-origin");
        let browser = session(Some(SESSION_CLIENT_BROWSER));
        assert!(require_browser_session(&browser, &ok).is_ok());
        assert!(require_browser_session(&browser, &browser_headers("same-site")).is_ok());
        // Origin is not consulted: browsers omit it on same-origin reads and
        // a relay in front of the platform changes it.
        let mut relayed = ok.clone();
        relayed.insert(header::ORIGIN, "https://relay.example".parse().unwrap());
        assert!(require_browser_session(&browser, &relayed).is_ok());

        // CLI login, pre-kind sessions and API keys are refused.
        for refused in [
            session(Some(g5_auth::SESSION_CLIENT_CLI)),
            session(None),
            {
                let mut claims = user_claims(Uuid::new_v4(), Uuid::new_v4());
                claims.client = Some(SESSION_CLIENT_BROWSER.into());
                claims.iss = Some(API_KEY_ISSUER.into());
                AuthContext::User(claims)
            },
            {
                let mut claims = user_claims(Uuid::new_v4(), Uuid::new_v4());
                claims.client = Some(SESSION_CLIENT_BROWSER.into());
                claims.token_use = Some(g5_auth::PREVIEW_TOKEN_USE.into());
                AuthContext::User(claims)
            },
            AuthContext::Host {
                host_id: Uuid::new_v4(),
                org_id: Uuid::new_v4(),
                scopes: vec![Scope::WorkerHost],
            },
        ] {
            assert!(require_browser_session(&refused, &ok).is_err());
        }

        // A request without the browser's fetch metadata, or a cross-site
        // or user-initiated navigation, is refused.
        let mut doubled = ok.clone();
        doubled.append("sec-fetch-site", "same-origin".parse().unwrap());
        for headers in [
            http::HeaderMap::new(),
            browser_headers("cross-site"),
            browser_headers("none"),
            doubled,
        ] {
            assert!(require_browser_session(&browser, &headers).is_err());
        }
    }
}
