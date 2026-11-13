//! Auth and SCM route handlers under two path prefixes.
//!
//! This module implements the HTTP route handlers for the Authentication API
//! (API_AND_STORAGE.md §3.11) and SCM Connection API. Endpoints span 2 path prefixes:
//!
//! - `/api/v1/auth` — OAuth login, callback, JWT refresh, logout, scope check
//! - `/api/v1/scm` — SCM provider connect, callback, disconnect, repo listing, import, org listing
//!
//! All handlers follow the thin handler pattern: extract request parameters,
//! delegate to `AuthService`, and format the response using `ResponseEnvelope`.
//! No business logic resides in the handlers.

use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::{rejection::BytesRejection, FromRequest, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use g5_auth::Scope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::oauth_attempt::{attempt_verifier, OAuthFlow};
use crate::db::scoped::scoped_db_for_org;
use crate::db::scoped::scoped_db_for_system_org;
use crate::errors::AppError;
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::routes::audited::AuditedRouter;
use crate::routes::browser_origin::{
    normalize_browser_origin, resolve_browser_origin_from_cli_metadata,
};
use crate::routes::ResponseEnvelope;
use crate::services::AppState;
use g5_contracts::error::G5Error;

pub use g5_api_foundation::contracts::auth::ScmOrgDTO;

// ─── Request DTOs ────────────────────────────────────────────────────────────

/// OAuth callback query parameters (shared between login and SCM callbacks).
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
    pub code: String,
    pub state: String,
}

/// Optional return target for SCM OAuth initiation.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmConnectParams {
    #[serde(alias = "return_to")]
    pub return_to: Option<String>,
}

/// Refresh token request body.
///
/// Both fields are optional: the refresh token may come from an httpOnly
/// cookie instead of the body, and `org_id` is only needed for org switching.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenRequest {
    #[serde(default)]
    pub refresh_token: String,
    pub org_id: Option<String>,
}

/// CLI logout request body. Browser logout uses the httpOnly refresh cookie.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutRequest {
    pub refresh_token: String,
}

/// WebSocket ticket request body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsTicketRequest {
    pub workspace_id: Option<String>,
}

/// CLI device authorization start request body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CliDeviceStartRequest {
    pub workspace_id: Option<String>,
    pub org_id: Option<String>,
    pub device_name: Option<String>,
    pub browser_origin: Option<String>,
}

fn cli_device_verification_base_url(
    configured_external_url: &str,
    headers: &HeaderMap,
    browser_origin: Option<&str>,
) -> Result<String, G5Error> {
    if browser_origin.is_some_and(|value| !value.trim().is_empty()) {
        return resolve_browser_origin_from_cli_metadata(browser_origin, configured_external_url);
    }

    let configured = configured_external_url.trim_end_matches('/').to_string();
    if !is_local_browser_origin(&configured) {
        return Ok(configured);
    }

    Ok(forwarded_host(headers)
        .map(|host| {
            let proto = forwarded_proto(headers).unwrap_or("http");
            format!("{proto}://{host}")
        })
        .unwrap_or(configured))
}

fn is_local_browser_origin(value: &str) -> bool {
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .map(|host| {
            matches!(
                host.as_str(),
                "localhost" | "127.0.0.1" | "::1" | "client" | "edge"
            )
        })
        .unwrap_or(false)
}

fn forwarded_proto(headers: &HeaderMap) -> Option<&'static str> {
    let raw = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())?
        .split(',')
        .next()?
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "https" => Some("https"),
        "http" => Some("http"),
        _ => None,
    }
}

fn forwarded_host(headers: &HeaderMap) -> Option<String> {
    for name in ["x-forwarded-host", "host"] {
        let raw = headers.get(name).and_then(|value| value.to_str().ok())?;
        let host = raw.split(',').next()?.trim();
        if is_safe_host_header(host) {
            return Some(host.to_string());
        }
    }
    None
}

fn is_safe_host_header(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && !host.contains('/')
        && !host.contains('\\')
        && !host.chars().any(char::is_whitespace)
}

/// Browser approval request body for a pending CLI device authorization.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceApproveRequest {
    pub user_code: String,
    #[serde(default = "default_cli_approve")]
    pub approve: bool,
}

/// CLI polling request body for a pending device authorization.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceTokenRequest {
    pub device_code: String,
}

/// WebSocket ticket response body.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsTicketResponse {
    pub ticket: String,
}

/// SCM repo list query parameters.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmRepoListParams {
    pub cursor: Option<String>,
    pub org_id: Option<String>,
}

/// Import repos request body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReposRequest {
    pub org_id: String,
    pub repos: Vec<ImportRepoItem>,
}

/// Single repo to import.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRepoItem {
    pub provider_repo_id: String,
    pub full_name: String,
    pub clone_url: String,
    pub default_branch: String,
}

// ─── Response DTOs ───────────────────────────────────────────────────────────

/// OAuth redirect response — returned by login and SCM connect endpoints.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthRedirectDTO {
    pub url: String,
    pub state: String,
}

/// Authentication tokens — returned by callback and refresh endpoints.
///
/// `refresh_token` is omitted when the exchange was authenticated by the
/// httpOnly `g5_refresh` cookie: the browser keeps that credential in the
/// cookie jar, so script never needs, and must never receive, a copy
/// (`g5.security.login.refresh-exchange`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokensDTO {
    pub jwt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub user_id: String,
}

/// CLI device authorization start response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: i64,
    pub interval: i64,
}

/// CLI device authorization approval response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceApproveResponse {
    pub status: String,
}

/// CLI device authorization polling response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceTokenResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// SCM connection result — returned by SCM callback.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmConnectionResultDTO {
    pub provider: String,
    pub provider_username: String,
    pub scopes: Vec<String>,
    pub connected_at: String,
    pub return_to: String,
}

/// Request body for `POST /api/v1/scm/:provider/connect` (fine-grained PAT).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmConnectRequest {
    pub token: String,
}

/// SCM repo list response — returned by GET /scm/:provider/repos.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmRepoListDTO {
    pub repos: Vec<ScmRepoDTO>,
    pub next_cursor: Option<String>,
}

/// A single repo from an SCM provider.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmRepoDTO {
    pub provider_repo_id: String,
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub default_branch: String,
    pub is_private: bool,
    pub clone_url: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub updated_at: String,
    pub already_imported: bool,
    pub imported_repo_id: Option<String>,
}

/// Import repos response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReposResponse {
    pub imported: Vec<ImportedRepoDTO>,
}

/// An imported repo DTO.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedRepoDTO {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub slug: String,
    pub git_url: String,
    pub default_branch: String,
    pub provider: Option<String>,
    pub provider_repo_id: Option<String>,
    pub provider_full_name: Option<String>,
    pub created_at: String,
}

// ─── From Implementations ────────────────────────────────────────────────────

impl From<crate::services::auth_service::OAuthRedirect> for OAuthRedirectDTO {
    fn from(r: crate::services::auth_service::OAuthRedirect) -> Self {
        OAuthRedirectDTO {
            url: r.url,
            state: r.state,
        }
    }
}

impl From<crate::services::auth_service::AuthTokens> for AuthTokensDTO {
    fn from(t: crate::services::auth_service::AuthTokens) -> Self {
        AuthTokensDTO {
            jwt: t.jwt,
            refresh_token: Some(t.refresh_token),
            expires_in: t.expires_in,
            user_id: t.user_id.to_string(),
        }
    }
}

impl From<crate::services::auth_service::ScmConnectionResult> for ScmConnectionResultDTO {
    fn from(r: crate::services::auth_service::ScmConnectionResult) -> Self {
        ScmConnectionResultDTO {
            provider: r.provider,
            provider_username: r.provider_username,
            scopes: r.scopes,
            connected_at: r.connected_at,
            return_to: r.return_to,
        }
    }
}

impl From<crate::services::auth_service::ScmRepoListResponse> for ScmRepoListDTO {
    fn from(r: crate::services::auth_service::ScmRepoListResponse) -> Self {
        ScmRepoListDTO {
            repos: r.repos.into_iter().map(ScmRepoDTO::from).collect(),
            next_cursor: r.next_cursor,
        }
    }
}

impl From<crate::services::auth_service::ScmRepoDTO> for ScmRepoDTO {
    fn from(r: crate::services::auth_service::ScmRepoDTO) -> Self {
        ScmRepoDTO {
            provider_repo_id: r.provider_repo_id,
            full_name: r.full_name,
            name: r.name,
            owner: r.owner,
            default_branch: r.default_branch,
            is_private: r.is_private,
            clone_url: r.clone_url,
            description: r.description,
            language: r.language,
            updated_at: r.updated_at,
            already_imported: r.already_imported,
            imported_repo_id: r.imported_repo_id.map(|id| id.to_string()),
        }
    }
}

impl From<crate::services::auth_service::RepoDTO> for ImportedRepoDTO {
    fn from(r: crate::services::auth_service::RepoDTO) -> Self {
        ImportedRepoDTO {
            id: r.id.to_string(),
            org_id: r.org_id.to_string(),
            name: r.name,
            slug: r.slug,
            git_url: r.git_url,
            default_branch: r.default_branch,
            provider: r.provider,
            provider_repo_id: r.provider_repo_id,
            provider_full_name: r.provider_full_name,
            created_at: r.created_at,
        }
    }
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Auth routes nested under `/api/v1/auth`.
///
/// Endpoints:
/// - `GET /login/:provider` — initiate OAuth login
/// - `GET /callback/:provider` — handle OAuth callback
/// - `POST /refresh` — refresh JWT
/// - `POST /logout` — revoke refresh token
/// - `POST /test-token` — *E2E only* issue a test JWT (requires G5_E2E_AUTH=true)
use crate::db::scoped::scoped_db_for_task;
use crate::middleware::preview_host::PREVIEW_HANDOFF_TICKET_PARAM;
use crate::middleware::PREVIEW_REAUTH_MARKER_PARAM;
use crate::routes::cookies::credential_cookie;
use crate::routes::preview::{reauth_unauthorized, sanitize_reauth_return};
use axum::http::StatusCode;
use g5_contracts::types::ids::TaskId;

/// `GET /api/v1/auth/preview/reauth/:taskId?return=<path+query>` — the [D18]
/// expired-cookie re-auth endpoint (issue G5Labs/G5#1097 piece 2).
///
/// Receives the refresh cookie (`Path=/` in secure deployments,
/// `/api/v1/auth` in plain-HTTP development);
/// auth-middleware-exempt like `/refresh` (the refresh cookie IS the
/// credential). Authenticates via the READ-ONLY refresh-identity resolver
/// (auth_service [D22] — no JWT mint, no expiry extension: this is a
/// `SameSite=Lax`-attached, cross-site-triggerable GET), authorizes with the
/// [I2] mint guard, then redirects back to the preview with a fresh
/// credential and a timestamped `__g5_reauth` loop-guard marker. Every
/// failure path is a browser-facing no-store HTML 401, never a redirect.
async fn preview_reauth(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<TaskId>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    req: Request,
) -> Response {
    let task_id = task_id.into_uuid();

    // (1) Sanitize `return` (already percent-decoded by the Query extractor).
    let return_path = sanitize_reauth_return(params.get("return").map(String::as_str));

    // (2) The g5_refresh cookie is the credential; absent → HTML 401.
    let Some(refresh_token) =
        credential_cookie(req.headers(), state.auth_service.refresh_cookie_name())
    else {
        tracing::debug!(%task_id, "preview re-auth: no g5_refresh cookie");
        return reauth_unauthorized();
    };

    // (3) Read-only identity resolution (auth_service [D22]).
    let Ok(identity) = state
        .auth_service
        .resolve_refresh_identity(refresh_token, None)
        .await
    else {
        tracing::debug!(%task_id, "preview re-auth: refresh identity resolution failed");
        return reauth_unauthorized();
    };

    // (4) Authorize exactly like the mint endpoint ([I2]): org-scoped RLS
    // task resolution from the SERVER-resolved claims; read-only → roll back.
    let now = chrono::Utc::now().timestamp();
    let auth = AuthContext::User(g5_auth::JwtClaims {
        sub: identity.user_id.to_string(),
        org: identity.org_id.to_string(),
        scopes: identity.scopes,
        iat: now,
        exp: now + 60,
        iss: None,
        aud: None,
        jti: None,
        token_use: None,
        client: None,
    });
    match scoped_db_for_task(&state, &auth, task_id, &["workspaces:read"]).await {
        Ok(db) => {
            if db.rollback().await.is_err() {
                tracing::warn!(%task_id, "preview re-auth: authorization rollback failed");
                return reauth_unauthorized();
            }
        }
        Err(e) => {
            tracing::debug!(%task_id, error = ?e, "preview re-auth: task authorization failed");
            return reauth_unauthorized();
        }
    }

    // (5) Mint a fresh handoff ticket and redirect back to the preview host
    // with a fresh marker. Previews are served only from isolated preview
    // hosts (`g5.security.mvp-exposure.preview`): without a suffix there is
    // nothing to heal into, and the caller learns why.
    let Some(suffix) = state.preview_host_suffix.as_deref() else {
        return crate::errors::AppError::from_error(G5Error::PreviewOriginUnconfigured)
            .into_response();
    };
    let marker = format!("{PREVIEW_REAUTH_MARKER_PARAM}={now}");
    let separator = if return_path.contains('?') { '&' } else { '?' };
    let Ok(handoff) =
        state
            .auth_service
            .mint_preview_handoff_ticket(identity.user_id, identity.org_id, task_id)
    else {
        tracing::warn!(%task_id, "preview re-auth: handoff ticket mint failed");
        return reauth_unauthorized();
    };
    let location = format!(
        "https://{task_id}.{suffix}{return_path}{separator}{PREVIEW_HANDOFF_TICKET_PARAM}={handoff}&{marker}"
    );

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(|_| reauth_unauthorized())
}

pub fn router() -> Router<Arc<AppState>> {
    let r = AuditedRouter::new()
        .exempt_route(
            "/login/:provider",
            get(login),
            "OAuth login initiation — no token exists yet",
        )
        .exempt_route(
            "/callback/:provider",
            get(callback),
            "OAuth callback — no token exists yet",
        )
        .exempt_route(
            "/refresh",
            post(refresh_token),
            "Token refresh — uses refresh token, no scopes required",
        )
        .exempt_route(
            "/cli/start",
            post(cli_device_start),
            "CLI device authorization start — local machine has no token yet",
        )
        .exempt_route(
            "/cli/token",
            post(cli_device_token),
            "CLI device authorization poll — device code is the one-time proof",
        )
        .exempt_route(
            "/cli/approve",
            post(cli_device_approve),
            "CLI device authorization approval — human user session checked by handler",
        )
        .exempt_route(
            "/logout",
            post(logout),
            "Session teardown — any authenticated user",
        )
        .exempt_route(
            "/ws-ticket",
            post(ws_ticket),
            "WebSocket ticket — human user session checked by handler, no scope needed",
        )
        .exempt_route(
            "/preview/reauth/:taskId",
            get(preview_reauth),
            "Preview re-auth — the g5_refresh cookie is the credential; \
             mounted under /api/v1/auth for the cookie path scope, contract \
             owned by routes/preview [D18]",
        );

    // Conditionally register the test-token endpoint.
    // Only available when G5_E2E_AUTH=true is set in the environment.
    if std::env::var("G5_E2E_AUTH").unwrap_or_default() == "true" {
        tracing::warn!("E2E test-token endpoint is ENABLED — do not use in production");
        r.exempt_route(
            "/test-token",
            post(issue_test_token),
            "E2E test endpoint — dev only",
        )
        .into_router()
    } else {
        r.into_router()
    }
}

/// SCM connection routes nested under `/api/v1/scm`.
///
/// Endpoints:
/// - `GET /:provider/connect` — initiate SCM OAuth flow
/// - `GET /:provider/callback` — handle SCM OAuth callback
/// - `DELETE /:provider/disconnect` — disconnect SCM provider
/// - `GET /:provider/repos` — list SCM repos
/// - `POST /:provider/repos/import` — import repos from SCM
/// - `GET /:provider/orgs` — list SCM orgs
pub fn scm_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/:provider/connect",
            get(scm_connect).post(scm_connect_pat),
            "Account SCM credentials — person session required in handler",
        )
        .exempt_route(
            "/:provider/callback",
            get(scm_callback),
            "SCM OAuth callback — single-use state and browser attempt cookie",
        )
        .exempt_route(
            "/:provider/disconnect",
            delete(scm_disconnect),
            "Account SCM credentials — person session required in handler",
        )
        .audited_route(
            "/:provider/repos",
            get(list_scm_repos),
            &[Scope::WorkspacesRead],
        )
        .audited_route(
            "/:provider/repos/import",
            post(import_scm_repos),
            &[Scope::WorkspacesWrite],
        )
        .exempt_route(
            "/:provider/orgs",
            get(list_scm_orgs),
            "Account SCM credentials — person session required in handler",
        )
        .into_router()
}

// ─── E2E Test Auth ───────────────────────────────────────────────────────────

/// Request body for the E2E test-token endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestTokenRequest {
    pub email: Option<String>,
}

/// `POST /api/v1/auth/test-token`
///
/// Issues a JWT for E2E testing without requiring an OAuth provider.
/// **Only available when `G5_E2E_AUTH=true` is set in the environment.**
///
/// The endpoint creates or upserts a test user and returns valid tokens
/// that can be used by Playwright or Rust E2E tests to authenticate.
async fn issue_test_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TestTokenRequest>,
) -> Result<Json<ResponseEnvelope<AuthTokensDTO>>, AppError> {
    let email = body.email.unwrap_or_else(|| "e2e-test@g5.dev".to_string());
    tracing::info!(email = %email, "Issuing E2E test token");

    let tokens = provision_test_token(&state, &email).await?;

    let dto: AuthTokensDTO = tokens.into();
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// Dev/E2E test-token provisioning orchestration (routes/auth [D14]).
///
/// Mints the fixture user's tokens through `AuthService::issue_test_token`
/// (which provisions each membership org's DB rows via the shared
/// `OrgService::create_org` cascade) and then — because the service holds no
/// repository-storage handle — completes the post-commit Base docs+code
/// materialization for every org the token path provisioned via the shared
/// repository bootstrap helpers. This keeps repo storage out of `AuthService`
/// while guaranteeing the dev/E2E Base is born with a complete materialized
/// authority cut rather than an unborn or parent-only member.
///
/// Exposed at crate/module scope (not just as a route handler) so integration
/// tests can exercise the exact same orchestration the endpoint runs.
pub async fn provision_test_token(
    state: &Arc<AppState>,
    email: &str,
) -> Result<crate::services::auth_service::AuthTokens, AppError> {
    let provisioning = state.auth_service.issue_test_token(email).await?;

    for target in &provisioning.base_repo_bootstraps {
        crate::routes::orgs::bootstrap_base_docs_repo(
            state,
            target.org_id,
            target.base_workspace_id,
            target.docs_repo_id,
        )
        .await?;
        crate::routes::orgs::bootstrap_base_code_repo(
            state,
            target.org_id,
            target.base_workspace_id,
            target.code_repo_id,
            &target.code_repo_name,
        )
        .await?;
    }

    // Seed the retained demo workspaces through the sole production branch
    // creator. This happens only after both Base repositories are materialized,
    // so the fork captures a complete docs+code source vector and publishes a
    // stopped workspace with real refs in every member repository.
    for target in &provisioning.fixture_fork_targets {
        let db = scoped_db_for_system_org(state, target.org_id, &["workspaces:write"]).await?;
        let scope = db.reopener();
        db.rollback().await.map_err(AppError::from)?;
        let fork_service = state.workspace_fork_service.scoped(scope);
        for name in ["demo", "e2e-test"] {
            fork_service
                .fork_workspace(
                    target.org_id,
                    target.base_workspace_id,
                    provisioning.tokens.user_id,
                    crate::services::workspace_fork_service::ForkWorkspaceRequest {
                        name: name.to_string(),
                        description: None,
                        idempotency_key: format!(
                            "e2e-test-token:{}:{name}",
                            target.base_workspace_id
                        ),
                        read_only: false,
                        merge_target_workspace_id: None,
                        chat_session_ids: Vec::new(),
                        source: crate::services::workspace_fork_service::ForkSource::Durable,
                        interrupt: false,
                        issue: None,
                        start_chat: None,
                        uniquify_name: false,
                        reverification: None,
                    },
                )
                .await?;
        }
    }

    // The fixture operator persona also operates the platform
    // (`g5.organization.platform-operator`), so dev and E2E stacks can reach
    // the operator console and create organizations through it.
    crate::routes::platform::ensure_e2e_operator(state, email, provisioning.tokens.user_id).await?;

    Ok(provisioning.tokens)
}

// ─── Auth Handlers ───────────────────────────────────────────────────────────

/// `GET /api/v1/auth/login/:provider`
///
/// Initiates an OAuth login flow by redirecting the browser to the
/// authorization URL for the specified enabled provider.
/// Returns an HTTP redirect so the browser follows the OAuth flow
/// automatically, carrying the attempt cookie that binds the flow to this
/// browser (SEC-M35, issue #456). This endpoint is **unauthenticated**.
async fn login(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Response, AppError> {
    tracing::debug!(provider = %provider, "Auth endpoint called: login");
    let redirect = state.auth_service.initiate_oauth(&provider).await?;
    tracing::info!(provider = %provider, url = %redirect.url, "OAuth login initiated — redirecting browser");
    let cookie =
        OAuthFlow::Login.set_cookie(&redirect.attempt_cookie, state.auth_service.cookie_secure());
    with_set_cookie(Redirect::to(&redirect.url).into_response(), &cookie)
}

/// This browser's attempt cookie for `flow`.
fn oauth_attempt_cookie<'a>(
    state: &AppState,
    headers: &'a HeaderMap,
    flow: OAuthFlow,
) -> Option<&'a str> {
    credential_cookie(
        headers,
        flow.cookie_name(state.auth_service.cookie_secure()),
    )
}

/// Append a `Set-Cookie` header, keeping any already set.
fn with_set_cookie(mut response: Response, cookie: &str) -> Result<Response, AppError> {
    let value = HeaderValue::from_str(cookie).map_err(|_| {
        AppError::from_error(G5Error::Internal {
            message: "Failed to build Set-Cookie header".to_string(),
        })
    })?;
    response.headers_mut().append(header::SET_COOKIE, value);
    Ok(response)
}

// `GET /api/v1/auth/callback/:provider`
//
// Handles the OAuth callback from the identity provider. Exchanges the
// authorization code for tokens, upserts the user, auto-accepts pending
// org invitations, mints a JWT, and sets the refresh token as an httpOnly
// cookie. The browser is then redirected back to the SPA login route so the
// client can pick up the session via `POST /auth/refresh`.
// This endpoint is **unauthenticated**.

/// Build the refresh-token Set-Cookie value under AuthService's effective name.
/// Secure names carry `__Host-`, which requires Path=/ and no Domain.
///
/// When `secure` is `true` (production over HTTPS), the `Secure` flag is
/// included so browsers only send the cookie on encrypted connections.
/// In CI / local dev over plain HTTP, `secure` is `false` and the flag is
/// omitted — otherwise browsers silently discard the cookie.
fn build_refresh_cookie(name: &str, token: &str, max_age_secs: i64, secure: bool) -> String {
    if secure {
        format!("{name}={token}; HttpOnly; Secure; Path=/; SameSite=Lax; Max-Age={max_age_secs}")
    } else {
        format!("{name}={token}; HttpOnly; Path=/api/v1/auth; SameSite=Lax; Max-Age={max_age_secs}")
    }
}

/// Expire the former host-only cookie without ever accepting its credential.
fn expire_legacy_refresh_cookie(
    response: Response,
    state: &AppState,
) -> Result<Response, AppError> {
    match state.auth_service.legacy_refresh_cookie_name() {
        Some(name) => with_set_cookie(
            response,
            &format!("{name}=; HttpOnly; Secure; Path=/api/v1/auth; SameSite=Lax; Max-Age=0"),
        ),
        None => Ok(response),
    }
}

fn oauth_callback_error_redirect(error: &AppError) -> Option<&'static str> {
    match &error.inner {
        G5Error::Forbidden { .. } => Some("/login/denied"),
        _ => None,
    }
}

/// Build the short-lived `g5_login_denied` Set-Cookie value that carries the
/// attempted verified email to the denied page (issue #1517).
///
/// Unlike `g5_refresh` this cookie is intentionally **not** `HttpOnly`: the SPA
/// reads it to name the rejected address, then clears it. It is scoped to
/// `/login` (covering `/login/denied`), lives only 5 minutes, and carries the
/// percent-encoded email so the address never travels in the URL — keeping it
/// out of Referer headers, browser history, and access logs. `Secure` is gated
/// on the same flag as `g5_refresh`.
fn build_denied_context_cookie(email: &str, secure: bool) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(email.as_bytes()).collect();
    if secure {
        format!("g5_login_denied={encoded}; Secure; Path=/login; SameSite=Lax; Max-Age=300")
    } else {
        format!("g5_login_denied={encoded}; Path=/login; SameSite=Lax; Max-Age=300")
    }
}

/// Build the matching-path expiration cookie for a previously rendered denied
/// login context. If the SPA did not get a chance to clear the earlier cookie,
/// it could otherwise render one visitor's denied email after a different
/// visitor's successful or generic-forbidden OAuth callback.
fn clear_denied_context_cookie(secure: bool) -> String {
    if secure {
        "g5_login_denied=; Secure; Path=/login; SameSite=Lax; Max-Age=0".to_string()
    } else {
        "g5_login_denied=; Path=/login; SameSite=Lax; Max-Age=0".to_string()
    }
}

fn scm_callback_redirect(return_to: &str, key: &str, value: &str) -> String {
    let (base, fragment) = return_to
        .split_once('#')
        .map(|(base, fragment)| (base, Some(fragment)))
        .unwrap_or((return_to, None));
    let separator = if base.contains('?') { '&' } else { '?' };
    let suffix = fragment
        .map(|fragment| format!("#{fragment}"))
        .unwrap_or_default();
    format!(
        "{base}{separator}{key}={}{}",
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>(),
        suffix
    )
}

/// Where a failed SCM callback sends the browser. The state is taken from
/// the request, so its return path is followed only when this browser holds
/// the attempt cookie issued for it; any other request, such as an
/// unauthenticated crafted link, goes to the fixed settings page (SEC-M36).
fn scm_callback_error_redirect(error: &AppError, state: &str, attempt_bound: bool) -> String {
    let message = error.inner.to_string();
    let reason = if message.contains("Failed to parse GitHub emails") {
        "github_email_lookup_failed"
    } else if message.contains("OAuth token exchange failed") {
        "oauth_token_exchange_failed"
    } else if message.contains("SCM encryption key not configured") {
        "scm_encryption_key_missing"
    } else if matches!(error.inner, G5Error::Unauthorized) {
        "oauth_state_invalid"
    } else {
        "connection_failed"
    };
    let return_to = if attempt_bound {
        crate::services::auth_service::scm_return_to_from_state(state)
    } else {
        crate::services::auth_service::SCM_DEFAULT_RETURN_TO.to_string()
    };
    scm_callback_redirect(&return_to, "scm_error", reason)
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Query(params): Query<OAuthCallbackParams>,
) -> Result<Response, AppError> {
    tracing::debug!(provider = %provider, "Auth endpoint called: callback");
    let attempt_cookie = oauth_attempt_cookie(&state, &headers, OAuthFlow::Login);
    // Every redirect below follows a consumed attempt, so it also expires
    // that attempt's cookie. A refused attempt answers with an error and sets
    // no cookie at all.
    let clear_attempt = OAuthFlow::Login.clear_cookie(state.auth_service.cookie_secure());
    let outcome =
        match state
            .auth_service
            .handle_callback(&provider, &params.code, &params.state, attempt_cookie)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(path) = oauth_callback_error_redirect(&error) {
                    tracing::info!(
                        provider = %provider,
                        redirect = %path,
                        "OAuth callback denied — redirecting browser to public access page"
                    );
                    // This error has no verified email to display. Clear any old
                    // context so the denied page cannot show a prior visitor's identity.
                    let cookie = clear_denied_context_cookie(state.auth_service.cookie_secure());
                    let mut response = Redirect::to(path).into_response();
                    response.headers_mut().insert(
                    header::SET_COOKIE,
                    HeaderValue::from_str(&cookie).map_err(|_| {
                        crate::errors::AppError::from_error(g5_contracts::error::G5Error::Internal {
                            message: "Failed to build denied-context expiration Set-Cookie header"
                                .to_string(),
                        })
                    })?,
                );
                    return with_set_cookie(response, &clear_attempt);
                }
                return Err(error);
            }
        };

    let tokens = match outcome {
        crate::services::auth_service::CallbackOutcome::Authenticated(tokens) => tokens,
        crate::services::auth_service::CallbackOutcome::AdmissionDenied { email, provider } => {
            // Deliberately does NOT log the raw email (the service already
            // logged a masked identity + verified count); the full address
            // travels only in the browser-scoped context cookie, never in logs
            // or the URL. (issue #1517)
            tracing::info!(
                provider = %provider,
                "OAuth callback refused admission — redirecting to denied page with context"
            );
            let cookie = build_denied_context_cookie(&email, state.auth_service.cookie_secure());
            let mut response = Redirect::to("/login/denied").into_response();
            response.headers_mut().insert(
                header::SET_COOKIE,
                HeaderValue::from_str(&cookie).map_err(|_| {
                    crate::errors::AppError::from_error(g5_contracts::error::G5Error::Internal {
                        message: "Failed to build denied-context Set-Cookie header".to_string(),
                    })
                })?,
            );
            // This redirect carries an email-bearing Set-Cookie; mark it
            // non-cacheable so no shared or browser cache can retain or replay
            // the address to another identity (CWE-525). (issue #1517)
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            return with_set_cookie(response, &clear_attempt);
        }
    };

    // A login may have accepted invitations; new members get their personal
    // workspace (g5.personal.lifecycle) without waiting for the reconciler.
    // It runs beside the redirect, which never waits on its Git work; `/me`
    // ensures the same workspace if Home asks first.
    {
        let state = state.clone();
        let user_id = tokens.user_id;
        tokio::spawn(async move {
            crate::services::personal_workspace::ensure_after_login(&state, user_id).await;
        });
    }

    let refresh_ttl = state.auth_service.refresh_token_ttl_secs();

    // Secure refresh cookies are host-bound; HTTP development retains its scoped path.
    let cookie = build_refresh_cookie(
        state.auth_service.refresh_cookie_name(),
        &tokens.refresh_token,
        refresh_ttl,
        state.auth_service.cookie_secure(),
    );

    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| {
            crate::errors::AppError::from_error(g5_contracts::error::G5Error::Internal {
                message: "Failed to build Set-Cookie header".to_string(),
            })
        })?,
    );
    // Keep the new refresh cookie while clearing a prior visitor's denied-page
    // context; replacing this header would silently discard the new session.
    let denied_context_expiration = clear_denied_context_cookie(state.auth_service.cookie_secure());
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&denied_context_expiration).map_err(|_| {
            crate::errors::AppError::from_error(g5_contracts::error::G5Error::Internal {
                message: "Failed to build denied-context expiration Set-Cookie header".to_string(),
            })
        })?,
    );

    tracing::info!(provider = %provider, "OAuth callback completed — setting cookie and redirecting to SPA");
    with_set_cookie(
        expire_legacy_refresh_cookie(response, &state)?,
        &clear_attempt,
    )
}

/// `POST /api/v1/auth/refresh`
///
/// Refreshes a JWT using a refresh token. Optionally accepts an `orgId`
/// to switch the active org context (for org switching without re-authentication).
///
/// The refresh token may be provided in one of two ways:
/// 1. **JSON body** — `{ "refreshToken": "...", "orgId": "..." }`
/// 2. **httpOnly cookie** — `g5_refresh` cookie (set by the OAuth callback)
///
/// Cookie-based refresh is the primary mechanism for the browser OAuth flow:
/// after the OAuth callback sets the cookie, the SPA calls this endpoint
/// with `credentials: 'include'` and no body to bootstrap the JWT.
///
/// The cookie exchange is CSRF-sensitive (`g5.security.login.refresh-exchange`):
/// the browser attaches `g5_refresh` ambiently, so a page on another origin,
/// including a same-site preview host, could otherwise drive it. A request
/// that names a browser origin must name a trusted one, and a cookie-sourced
/// exchange returns no refresh credential to script.
async fn refresh_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<RefreshTokenRequest>>,
) -> Result<Response, AppError> {
    tracing::debug!("Auth endpoint called: refresh_token");

    // Decompose the optional body into its parts before matching.
    let (body_token, body_org_id) = match body {
        Some(Json(b)) => (b.refresh_token, b.org_id),
        None => (String::new(), None),
    };

    // Prefer the body's refresh token; fall back to the httpOnly cookie.
    let from_cookie = body_token.is_empty();
    let token = if !from_cookie {
        body_token
    } else {
        if !is_trusted_browser_request(&headers, &state.trusted_browser_origins) {
            tracing::warn!(
                origin = ?headers.get(header::ORIGIN),
                sec_fetch_site = ?headers.get("sec-fetch-site"),
                "Refused g5_refresh cookie exchange from an untrusted browser origin"
            );
            return Ok((
                StatusCode::FORBIDDEN,
                "Cookie refresh from an untrusted browser origin is forbidden",
            )
                .into_response());
        }
        credential_cookie(&headers, state.auth_service.refresh_cookie_name())
            .ok_or(G5Error::Unauthorized)?
            .to_owned()
    };

    let org_id = body_org_id
        .as_deref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({"orgId": "Invalid UUID format"}),
                })
            })
        })
        .transpose()?;

    let tokens = state.auth_service.refresh_token(&token, org_id).await?;

    // Re-set the cookie for the same refresh token: refresh renews its idle
    // expiry within the absolute lifetime but does not rotate it (issue #459).
    let refresh_ttl = state.auth_service.refresh_token_ttl_secs();
    let cookie = build_refresh_cookie(
        state.auth_service.refresh_cookie_name(),
        &tokens.refresh_token,
        refresh_ttl,
        state.auth_service.cookie_secure(),
    );

    let mut dto: AuthTokensDTO = tokens.into();
    if from_cookie {
        // The cookie jar already holds the credential; script gets the JWT only.
        dto.refresh_token = None;
    }
    let mut response = Json(ResponseEnvelope::ok(dto)).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static("")),
    );

    tracing::info!("JWT refreshed");
    expire_legacy_refresh_cookie(response, &state)
}

/// Whether a request may exchange the ambient `g5_refresh` cookie.
///
/// Browsers send `Origin` on every `POST`, so a present `Origin` must be one
/// of the trusted browser origins (platform origin plus CORS allow-list);
/// `null`, malformed, duplicate and foreign origins fail closed. `Sec-Fetch-Site`
/// must be absent or one of `same-origin`, `same-site` (the SPA dev server
/// on another port) or `none`; `cross-site` and unknown values fail closed.
/// A request with neither header is a non-browser client, which cannot be
/// driven cross-origin and keeps working.
pub(crate) fn is_trusted_browser_request(headers: &HeaderMap, trusted_origins: &[String]) -> bool {
    let mut sites = headers.get_all("sec-fetch-site").iter();
    if let Some(site) = sites.next() {
        let allowed = matches!(site.to_str(), Ok("same-origin" | "same-site" | "none"));
        if !allowed || sites.next().is_some() {
            return false;
        }
    }
    let mut origins = headers.get_all(header::ORIGIN).iter();
    match origins.next() {
        None => true,
        Some(origin) => {
            origins.next().is_none()
                && origin
                    .to_str()
                    .ok()
                    .and_then(|value| normalize_browser_origin(value).ok())
                    .is_some_and(|origin| trusted_origins.contains(&origin))
        }
    }
}

/// `POST /api/v1/auth/cli/start`
///
/// Starts a short-lived device authorization for a local `g5d auth login`.
/// This endpoint is unauthenticated because it is called before the local
/// machine has any bearer credential. The returned device code is stored only
/// as a hash server-side and can be exchanged exactly once after browser
/// approval.
async fn cli_device_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CliDeviceStartRequest>,
) -> Result<Json<ResponseEnvelope<CliDeviceStartResponse>>, AppError> {
    tracing::debug!("Auth endpoint called: cli_device_start");
    let workspace_id = body
        .workspace_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            Uuid::parse_str(value).map_err(|_| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({"workspaceId": "Invalid UUID format"}),
                })
            })
        })
        .transpose()?;
    let org_id = body
        .org_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            Uuid::parse_str(value).map_err(|_| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({"orgId": "Invalid UUID format"}),
                })
            })
        })
        .transpose()?;

    if workspace_id.is_none() && org_id.is_none() {
        return Err(AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({
                "workspaceId": "workspaceId or orgId is required",
                "orgId": "workspaceId or orgId is required",
            }),
        }));
    }

    let started = state
        .auth_service
        .start_cli_device_authorization(workspace_id, org_id, body.device_name)
        .await?;
    let verification_base_url = cli_device_verification_base_url(
        &state.external_url,
        &headers,
        body.browser_origin.as_deref(),
    )
    .map_err(AppError::from_error)?;
    let verification_uri = format!(
        "{}/cli/authorize",
        verification_base_url.trim_end_matches('/')
    );
    let verification_uri_complete = format!(
        "{}?userCode={}",
        verification_uri,
        url::form_urlencoded::byte_serialize(started.user_code.as_bytes()).collect::<String>()
    );

    Ok(Json(ResponseEnvelope::ok(CliDeviceStartResponse {
        device_code: started.device_code,
        user_code: started.user_code,
        verification_uri,
        verification_uri_complete,
        expires_in: started.expires_in,
        interval: started.interval,
    })))
}

/// `POST /api/v1/auth/cli/approve`
///
/// Approves or denies a pending CLI device authorization. The browser must
/// already be authenticated with a user session (never an API key); the
/// service verifies that the approving user is a member of the target
/// workspace's org before the device can poll tokens.
async fn cli_device_approve(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CliDeviceApproveRequest>,
) -> Result<Json<ResponseEnvelope<CliDeviceApproveResponse>>, AppError> {
    tracing::debug!("Auth endpoint called: cli_device_approve");
    let user_id = extract_session_user_id(&auth)?;
    let result = state
        .auth_service
        .approve_cli_device_authorization(user_id, &body.user_code, body.approve)
        .await?;

    Ok(Json(ResponseEnvelope::ok(CliDeviceApproveResponse {
        status: result.status,
    })))
}

/// `POST /api/v1/auth/cli/token`
///
/// Polls a pending device authorization. Tokens are returned only after a
/// browser-authenticated user approves the matching user code; the request is
/// then consumed and subsequent polls return `consumed`.
async fn cli_device_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CliDeviceTokenRequest>,
) -> Result<Json<ResponseEnvelope<CliDeviceTokenResponse>>, AppError> {
    tracing::debug!("Auth endpoint called: cli_device_token");
    let result = state
        .auth_service
        .poll_cli_device_authorization(&body.device_code)
        .await?;
    let response = match result.tokens {
        Some(tokens) => CliDeviceTokenResponse {
            status: result.status,
            jwt: Some(tokens.jwt),
            refresh_token: Some(tokens.refresh_token),
            expires_in: Some(tokens.expires_in),
            user_id: Some(tokens.user_id.to_string()),
            org_id: result.org_id.map(|id| id.to_string()),
            workspace_id: result.workspace_id.map(|id| id.to_string()),
        },
        None => CliDeviceTokenResponse {
            status: result.status,
            jwt: None,
            refresh_token: None,
            expires_in: None,
            user_id: None,
            org_id: result.org_id.map(|id| id.to_string()),
            workspace_id: result.workspace_id.map(|id| id.to_string()),
        },
    };

    Ok(Json(ResponseEnvelope::ok(response)))
}

/// `POST /api/v1/auth/logout`
///
/// Revokes a refresh token, ending the user's session. Idempotent —
/// succeeds even if the refresh token doesn't exist or is already revoked.
/// Also clears the `g5_refresh` httpOnly cookie.
async fn logout(
    State(state): State<Arc<AppState>>,
    _auth: AuthContext,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, AppError> {
    tracing::debug!("Auth endpoint called: logout");
    // Read through the bounded Bytes extractor first: a missing media type
    // does not imply an absent body. Non-empty CLI payloads must still pass
    // the normal JSON media-type and syntax checks before any revocation.
    let body = match body {
        Ok(body) if body.is_empty() => None,
        Ok(body) => {
            let mut request = Request::new(Body::from(body));
            *request.headers_mut() = headers.clone();
            match Json::<LogoutRequest>::from_request(request, &state).await {
                Ok(Json(body)) => Some(body),
                Err(rejection) => return Ok(rejection.into_response()),
            }
        }
        Err(rejection) => return Ok(rejection.into_response()),
    };
    let body_token = body
        .map(|body| body.refresh_token)
        .filter(|token| !token.is_empty());
    if let Some(token) = &body_token {
        state.auth_service.logout(token).await?;
    }
    // The browser cannot read its httpOnly token to put it in JSON. Revoke
    // the cookie credential as well if a CLI-style body accompanies it,
    // since the response clears that browser session's cookie either way.
    if let Some(token) = credential_cookie(&headers, state.auth_service.refresh_cookie_name())
        .filter(|token| Some(*token) != body_token.as_deref())
    {
        state.auth_service.logout(token).await?;
    }

    // Clear the refresh-token cookie.
    let clear_cookie = build_refresh_cookie(
        state.auth_service.refresh_cookie_name(),
        "",
        0,
        state.auth_service.cookie_secure(),
    );

    let mut response = Json(ResponseEnvelope::ok(())).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_cookie).unwrap_or_else(|_| HeaderValue::from_static("")),
    );

    tracing::info!("User logged out");
    expire_legacy_refresh_cookie(response, &state)
}

// ─── WebSocket Ticket ────────────────────────────────────────────────────────

/// `POST /api/v1/auth/ws-ticket`
///
/// Mints a short-lived JWT for WebSocket upgrade authentication.
/// If `workspaceId` is provided, validates workspace access.
/// Otherwise mints a general short-lived ticket for the authenticated user.
/// API keys are refused: tickets are user sessions.
async fn ws_ticket(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<WsTicketRequest>,
) -> Result<Json<ResponseEnvelope<WsTicketResponse>>, AppError> {
    tracing::debug!("Auth endpoint called: ws_ticket");
    let user_id = extract_session_user_id(&auth)?;

    let ticket = if let Some(ref ws_id) = body.workspace_id {
        let workspace_id = Uuid::parse_str(ws_id).map_err(|_| {
            AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({"workspaceId": "Invalid UUID format"}),
            })
        })?;
        state
            .auth_service
            .mint_ws_ticket(user_id, workspace_id)
            .await?
    } else {
        // No workspace — mint a general short-lived ticket.
        let org_id = extract_org_id(&auth)?;
        let scopes = auth.scopes().to_vec();
        state
            .auth_service
            .mint_ws_ticket_general(user_id, org_id, scopes)
            .await?
    };

    Ok(Json(ResponseEnvelope::ok(WsTicketResponse { ticket })))
}

// ─── SCM Handlers ────────────────────────────────────────────────────────────

/// `GET /api/v1/scm/:provider/connect`
///
/// Initiates an SCM OAuth flow (broader scopes for repo access).
/// Returns the authorization URL and sets the attempt cookie that binds the
/// flow to this browser: the URL alone, forwarded to anyone else, cannot
/// connect their provider account to this user (SEC-M35, issue #449).
async fn scm_connect(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
    Query(params): Query<ScmConnectParams>,
) -> Result<Response, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: scm_connect");
    let user_id = extract_session_user_id(&auth)?;
    let redirect = state
        .auth_service
        .initiate_scm_connect(&provider, user_id, params.return_to.as_deref())
        .await?;
    let cookie = OAuthFlow::ScmConnect
        .set_cookie(&redirect.attempt_cookie, state.auth_service.cookie_secure());
    let dto: OAuthRedirectDTO = redirect.into();
    with_set_cookie(Json(ResponseEnvelope::ok(dto)).into_response(), &cookie)
}

/// `POST /api/v1/scm/:provider/connect`
///
/// Connects GitHub repo access by validating and storing a user-supplied
/// fine-grained Personal Access Token. Replaces the SCM OAuth App redirect for
/// GitHub; GitLab still uses the `GET` OAuth flow above.
async fn scm_connect_pat(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
    Json(body): Json<ScmConnectRequest>,
) -> Result<Json<ResponseEnvelope<ScmConnectionResultDTO>>, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: scm_connect_pat");
    let user_id = extract_session_user_id(&auth)?;
    let result = state
        .auth_service
        .connect_scm_pat(user_id, &provider, &body.token)
        .await?;
    let dto: ScmConnectionResultDTO = result.into();
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/scm/:provider/callback`
///
/// Handles the SCM OAuth callback from the browser redirect. This endpoint
/// is auth-exempt because the browser redirect from the OAuth provider carries
/// no JWT — instead, the `state` parameter is validated against the
/// `oauth_states` table to identify the user, and the browser must present
/// the attempt cookie `scm_connect` set for that state.
///
/// On success, redirects the browser back to the SPA route that initiated
/// the connection.
/// On failure, redirects with an error query parameter. Either way the
/// attempt cookie is expired.
async fn scm_callback(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    Query(params): Query<OAuthCallbackParams>,
) -> Result<Response, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: scm_callback");
    let attempt_cookie = oauth_attempt_cookie(&state, &headers, OAuthFlow::ScmConnect);
    let location = match state
        .auth_service
        .handle_scm_callback(&provider, &params.code, &params.state, attempt_cookie)
        .await
    {
        Ok(result) => {
            tracing::info!(provider = %provider, "SCM provider connected");
            scm_callback_redirect(&result.return_to, "scm_connected", "true")
        }
        Err(e) => {
            tracing::error!(provider = %provider, error = %e, "SCM callback failed");
            let attempt_bound = attempt_verifier(attempt_cookie, &params.state).is_some();
            scm_callback_error_redirect(&e, &params.state, attempt_bound)
        }
    };
    with_set_cookie(
        Redirect::to(&location).into_response(),
        &OAuthFlow::ScmConnect.clear_cookie(state.auth_service.cookie_secure()),
    )
}

/// `DELETE /api/v1/scm/:provider/disconnect`
///
/// Revokes and deletes the SCM connection for the given provider.
async fn scm_disconnect(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
) -> Result<Json<ResponseEnvelope<()>>, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: scm_disconnect");
    let user_id = extract_session_user_id(&auth)?;
    state
        .auth_service
        .disconnect_scm(user_id, &provider)
        .await?;
    tracing::info!(provider = %provider, "SCM provider disconnected");
    Ok(Json(ResponseEnvelope::ok(())))
}

/// `GET /api/v1/scm/:provider/repos`
///
/// Lists repositories the user can access on the SCM provider.
async fn list_scm_repos(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
    Query(params): Query<ScmRepoListParams>,
) -> Result<Json<ResponseEnvelope<ScmRepoListDTO>>, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: list_scm_repos");
    extract_session_user_id(&auth)?;
    let org_id = params
        .org_id
        .as_deref()
        .map(|id| {
            Uuid::parse_str(id).map_err(|_| {
                AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({"orgId": "Invalid UUID format"}),
                })
            })
        })
        .transpose()?
        .unwrap_or(extract_org_id(&auth)?);
    let result = state
        .auth_service
        .list_scm_repos(&auth, &provider, org_id, params.cursor)
        .await?;
    let dto: ScmRepoListDTO = result.into();
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `POST /api/v1/scm/:provider/repos/import`
///
/// Imports selected repositories from the SCM provider into a G5 organization.
/// Idempotent — already-imported repos are skipped.
async fn import_scm_repos(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
    Json(body): Json<ImportReposRequest>,
) -> Result<Json<ResponseEnvelope<ImportReposResponse>>, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: import_scm_repos");
    extract_session_user_id(&auth)?;

    let org_id = Uuid::parse_str(&body.org_id).map_err(|_| {
        AppError::from_error(G5Error::ValidationError {
            details: serde_json::json!({"orgId": "Invalid UUID format"}),
        })
    })?;

    let repos: Vec<crate::services::auth_service::ImportRepoSpec> = body
        .repos
        .into_iter()
        .map(|r| crate::services::auth_service::ImportRepoSpec {
            provider_repo_id: r.provider_repo_id,
            full_name: r.full_name,
            clone_url: r.clone_url,
            default_branch: r.default_branch,
        })
        .collect();

    let mut db = scoped_db_for_org(&state, &auth, org_id, &["workspaces:write"]).await?;
    let imported = state
        .auth_service
        .import_repos(&mut db, &provider, org_id, repos)
        .await?;
    db.commit().await.map_err(AppError::from)?;

    let dto = ImportReposResponse {
        imported: imported.into_iter().map(ImportedRepoDTO::from).collect(),
    };
    tracing::info!(provider = %provider, count = dto.imported.len(), "Repos imported from SCM");
    Ok(Json(ResponseEnvelope::ok(dto)))
}

/// `GET /api/v1/scm/:provider/orgs`
///
/// Lists organizations/groups the user belongs to on the SCM provider.
async fn list_scm_orgs(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(provider): Path<String>,
) -> Result<Json<ResponseEnvelope<Vec<ScmOrgDTO>>>, AppError> {
    tracing::debug!(provider = %provider, "SCM endpoint called: list_scm_orgs");
    let user_id = extract_session_user_id(&auth)?;
    let orgs = state.auth_service.list_scm_orgs(user_id, &provider).await?;
    Ok(Json(ResponseEnvelope::ok(orgs)))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn default_cli_approve() -> bool {
    true
}

/// Extracts the `org_id` from `AuthContext::User`. Returns `Unauthorized`
/// error if the auth context is a service token.
fn extract_org_id(auth: &AuthContext) -> Result<Uuid, AppError> {
    let org_id_str = auth
        .org_id()
        .ok_or_else(|| AppError::from_error(G5Error::Unauthorized))?;
    let org_id = Uuid::parse_str(&org_id_str).map_err(|_| {
        AppError::from_error(G5Error::Internal {
            message: "Invalid org ID in JWT".into(),
        })
    })?;
    Ok(org_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_browser_request_admits_only_trusted_origins() {
        let trusted = vec![
            "https://app.example.com".to_string(),
            "http://localhost:5173".to_string(),
        ];
        let headers = |pairs: &[(&str, &str)]| {
            let mut headers = HeaderMap::new();
            for (name, value) in pairs {
                headers.append(
                    http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    HeaderValue::from_str(value).unwrap(),
                );
            }
            headers
        };
        // Non-browser clients send neither header.
        assert!(is_trusted_browser_request(&headers(&[]), &trusted));
        // The SPA on the platform origin, and the dev server on another port.
        assert!(is_trusted_browser_request(
            &headers(&[
                ("origin", "https://app.example.com"),
                ("sec-fetch-site", "same-origin")
            ]),
            &trusted
        ));
        assert!(is_trusted_browser_request(
            &headers(&[
                ("origin", "HTTPS://App.Example.com/"),
                ("sec-fetch-site", "same-origin")
            ]),
            &trusted
        ));
        assert!(is_trusted_browser_request(
            &headers(&[
                ("origin", "http://localhost:5173"),
                ("sec-fetch-site", "same-site")
            ]),
            &trusted
        ));
        assert!(is_trusted_browser_request(
            &headers(&[
                ("origin", "https://app.example.com"),
                ("sec-fetch-site", "none")
            ]),
            &trusted
        ));
        // A sibling preview host is same-site but not trusted.
        assert!(!is_trusted_browser_request(
            &headers(&[
                ("origin", "https://job.preview.example.com"),
                ("sec-fetch-site", "same-site")
            ]),
            &trusted
        ));
        for refused in [
            vec![("origin", "https://evil.example")],
            vec![("origin", "null")],
            vec![("origin", "not an origin")],
            vec![("origin", "https://app.example.com/path")],
            vec![
                ("origin", "https://app.example.com"),
                ("origin", "https://evil.example"),
            ],
            vec![
                ("origin", "https://app.example.com"),
                ("sec-fetch-site", "cross-site"),
            ],
            vec![
                ("origin", "https://app.example.com"),
                ("sec-fetch-site", "bogus"),
            ],
            vec![
                ("origin", "https://app.example.com"),
                ("sec-fetch-site", "same-origin"),
                ("sec-fetch-site", "cross-site"),
            ],
            vec![("sec-fetch-site", "cross-site")],
        ] {
            assert!(
                !is_trusted_browser_request(&headers(&refused), &trusted),
                "{refused:?} must be refused"
            );
        }
    }

    #[test]
    fn cli_device_verification_url_uses_forwarded_public_host_when_configured_local() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("g5.example.com"),
        );

        assert_eq!(
            cli_device_verification_base_url("http://localhost:3000", &headers, None).unwrap(),
            "https://g5.example.com"
        );
    }

    #[test]
    fn cli_device_verification_url_keeps_nonlocal_configured_external_url() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("attacker.example.com"),
        );

        assert_eq!(
            cli_device_verification_base_url("https://app.example.com/", &headers, None).unwrap(),
            "https://app.example.com"
        );
    }

    #[test]
    fn cli_device_verification_url_prefers_cli_browser_origin_metadata() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("api.example.com"),
        );

        assert_eq!(
            cli_device_verification_base_url(
                "http://localhost:3000",
                &headers,
                Some("https://app.example.com/")
            )
            .unwrap(),
            "https://app.example.com"
        );
    }

    #[test]
    fn cli_device_start_request_accepts_worker_workspace_org_target() {
        let parsed: CliDeviceStartRequest = serde_json::from_value(serde_json::json!({
            "workspaceId": "019dd0b5-cc93-7771-bc4d-000000000001",
            "orgId": "019dd0b5-cc93-7771-bc4d-000000000002",
            "deviceName": "worker laptop",
            "browserOrigin": "https://app.example.com",
        }))
        .expect("workspace/org target should deserialize");

        assert_eq!(
            parsed.workspace_id.as_deref(),
            Some("019dd0b5-cc93-7771-bc4d-000000000001")
        );
        assert_eq!(
            parsed.org_id.as_deref(),
            Some("019dd0b5-cc93-7771-bc4d-000000000002")
        );
        assert_eq!(
            parsed.browser_origin.as_deref(),
            Some("https://app.example.com")
        );
    }

    #[test]
    fn cli_device_start_request_accepts_worker_org_only_target() {
        let parsed: CliDeviceStartRequest = serde_json::from_value(serde_json::json!({
            "orgId": "019dd0b5-cc93-7771-bc4d-000000000002",
            "deviceName": "worker laptop",
        }))
        .expect("org-only target should deserialize");

        assert_eq!(parsed.workspace_id.as_deref(), None);
        assert_eq!(
            parsed.org_id.as_deref(),
            Some("019dd0b5-cc93-7771-bc4d-000000000002")
        );
    }

    #[test]
    fn cli_device_start_request_allows_shape_without_scope_for_handler_validation() {
        let parsed: CliDeviceStartRequest = serde_json::from_value(serde_json::json!({
            "deviceName": "worker laptop",
        }))
        .expect("missing scope is handler validation, not serde validation");

        assert!(parsed.workspace_id.is_none());
        assert!(parsed.org_id.is_none());
    }

    #[test]
    fn cli_device_start_request_rejects_legacy_daemon_target_fields() {
        let err = serde_json::from_value::<CliDeviceStartRequest>(serde_json::json!({
            "workspaceId": "019dd0b5-cc93-7771-bc4d-000000000001",
            "daemonId": "legacy-daemon",
            "sessionScope": "legacy-session",
        }))
        .expect_err("legacy daemon target fields must be rejected");

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn cli_device_token_response_can_return_org_scope() {
        let serialized = serde_json::to_value(CliDeviceTokenResponse {
            status: "approved".to_string(),
            jwt: Some("jwt".to_string()),
            refresh_token: Some("refresh".to_string()),
            expires_in: Some(3600),
            user_id: Some("019dd0b5-cc93-7771-bc4d-000000000003".to_string()),
            org_id: Some("019dd0b5-cc93-7771-bc4d-000000000002".to_string()),
            workspace_id: None,
        })
        .expect("serialize token response");

        assert_eq!(
            serialized["orgId"],
            serde_json::json!("019dd0b5-cc93-7771-bc4d-000000000002")
        );
        assert_eq!(serialized.get("workspaceId"), None);
        assert_eq!(serialized.get("daemonId"), None);
        assert_eq!(serialized.get("sessionScope"), None);
    }

    #[test]
    fn cli_device_token_response_can_return_authoritative_org_and_workspace() {
        let serialized = serde_json::to_value(CliDeviceTokenResponse {
            status: "approved".to_string(),
            jwt: Some("jwt".to_string()),
            refresh_token: Some("refresh".to_string()),
            expires_in: Some(3600),
            user_id: Some("019dd0b5-cc93-7771-bc4d-000000000003".to_string()),
            org_id: Some("019dd0b5-cc93-7771-bc4d-000000000002".to_string()),
            workspace_id: Some("019dd0b5-cc93-7771-bc4d-000000000001".to_string()),
        })
        .expect("serialize token response");

        assert_eq!(
            serialized["orgId"],
            serde_json::json!("019dd0b5-cc93-7771-bc4d-000000000002")
        );
        assert_eq!(
            serialized["workspaceId"],
            serde_json::json!("019dd0b5-cc93-7771-bc4d-000000000001")
        );
        assert_eq!(serialized.get("daemonId"), None);
        assert_eq!(serialized.get("sessionScope"), None);
    }

    #[test]
    fn oauth_callback_error_redirects_forbidden_to_denied_page() {
        let error = AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["auth:allowlist".to_string()],
        });
        assert_eq!(oauth_callback_error_redirect(&error), Some("/login/denied"));
    }

    #[test]
    fn oauth_callback_error_does_not_redirect_internal_errors() {
        let error = AppError::from_error(G5Error::Internal {
            message: "boom".to_string(),
        });
        assert_eq!(oauth_callback_error_redirect(&error), None);
    }

    /// SEC-M36: a failed SCM callback follows the state's return path only for
    /// an attempt this browser started, and never a path the shared check
    /// refuses.
    #[test]
    fn scm_callback_error_redirect_follows_only_bound_local_paths() {
        use base64::Engine as _;
        let state = |path: &str| {
            format!(
                "nonce.{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(path)
            )
        };
        let error = AppError::from_error(G5Error::Unauthorized);
        let local = state("/acme/base?settings=scm");
        assert_eq!(
            scm_callback_error_redirect(&error, &local, true),
            "/acme/base?settings=scm&scm_error=oauth_state_invalid"
        );
        assert_eq!(
            scm_callback_error_redirect(&error, &local, false),
            "/settings?scm_error=oauth_state_invalid"
        );
        for off_origin in [
            "/\\attacker.example/path",
            "//attacker.example",
            "https://a.example",
        ] {
            assert_eq!(
                scm_callback_error_redirect(&error, &state(off_origin), true),
                "/settings?scm_error=oauth_state_invalid",
                "{off_origin}"
            );
        }
    }
}
