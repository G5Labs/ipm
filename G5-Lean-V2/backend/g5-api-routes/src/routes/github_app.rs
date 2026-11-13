//! Organization binding to the G5-owned GitHub App.
//!
//! Organization-admin routes are nested at `/api/v1/orgs/:orgId/github-app`.
//! The browser callbacks GitHub redirects to are nested at
//! `/api/v1/github-app` and are unauthenticated by design: they carry no G5
//! session, and are authorized by HMAC-signed connection state, the stored
//! state digest, a signed PKCE cookie, and GitHub's proof that the OAuth user
//! can access the installation.
//!
//! Flow: an admin creates a connection attempt and is sent to GitHub's
//! installation page with signed state. GitHub redirects to `/setup` with the
//! installation id; G5 binds that id into a signed OAuth state and starts
//! GitHub App OAuth with PKCE. `/oauth/callback` exchanges the code, verifies
//! the user's access to that exact installation, discards the user token,
//! re-checks that the attempt's creator is still an org admin, and appends
//! binding version N+1 with empty coverage. Reconnect skips installation and
//! discovers the App installations the OAuth user can access.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::scoped::{
    scoped_db_for_org, scoped_db_for_system_org, scoped_db_for_user_actor_in_org,
};
use crate::errors::AppError;
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::routes::audited::AuditedRouter;
use crate::routes::cookies::credential_cookie;
use crate::routes::orgs::OrgPathParams;
use crate::routes::ResponseEnvelope;
use crate::services::github_app::{
    connection_state_digest, new_pkce_cookie, oauth_authorize_url, oauth_state_for_installation,
    oauth_state_for_reconnect_discovery, oauth_state_for_reconnect_selection, parse_oauth_state,
    parse_pkce_cookie, pkce_code_challenge, pkce_cookie_name, unix_now,
    validated_installation_management_url, verify_connection_state, verify_user_connection_state,
    ConnectionAttemptRef, GithubAppAccessibleInstallation, GithubAppClient, GithubAppOAuthState,
};
use crate::services::github_app_binding::{
    append_coverage_change, append_disconnect, append_installation_refresh,
    append_verified_binding, create_connection_attempt, is_org_admin, load_binding_status,
    load_latest_binding, load_pending_attempt, org_slug, validate_coverage_selection,
    GithubAppBindingStatus, PendingConnectionAttempt,
};
use crate::services::github_user_grants::{
    complete_connection, delete_grant, grant_status, org_uses_github, pending_attempt_return_path,
    start_connection, UserGrantStatus,
};
use crate::services::{AppState, PermissionService};
use g5_auth::Scope;
use g5_contracts::error::G5Error;
use g5_contracts::types::enums::OrgRole;

const RECORDED_BINDING_NOTICE: &str = "Recorded binding — not live verified";
const ORG_ADMIN_SCOPE: &[&str] = &["org:admin"];

// ─── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppBindingStatusResponseDTO {
    pub notice: &'static str,
    pub binding: Option<GithubAppBindingDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppBindingDTO {
    pub version: i64,
    pub disposition: String,
    pub provenance: String,
    pub installation_id: i64,
    pub account_login: String,
    pub permissions: serde_json::Value,
    pub recorded_at: DateTime<Utc>,
    pub covered_repositories: Vec<GithubAppCoveredRepositoryDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppCoveredRepositoryDTO {
    pub github_repo_id: i64,
    pub full_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppRedirectDTO {
    pub redirect_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppInstallationRepositoriesDTO {
    pub binding_version: i64,
    pub repositories: Vec<GithubAppInstallationRepositoryDTO>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppInstallationRepositoryDTO {
    pub github_repo_id: i64,
    pub full_name: String,
    pub private: bool,
    pub covered: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAppManagementUrlDTO {
    pub management_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateGithubAppCoverageRequest {
    pub binding_version: i64,
    /// GitHub numeric repository ids; an empty list removes all coverage.
    pub github_repo_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisconnectGithubAppRequest {
    pub binding_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefreshGithubAppBindingRequest {
    pub binding_version: i64,
}

#[derive(Debug, Deserialize)]
pub struct GithubAppSetupQuery {
    pub state: Option<String>,
    pub installation_id: Option<String>,
    pub setup_action: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GithubAppOAuthCallbackQuery {
    pub state: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GithubAppReconnectSelectionQuery {
    pub state: Option<String>,
}

impl From<GithubAppBindingStatus> for GithubAppBindingDTO {
    fn from(status: GithubAppBindingStatus) -> Self {
        let binding = status.binding;
        Self {
            version: binding.version,
            disposition: binding.disposition,
            provenance: binding.provenance,
            installation_id: binding.installation_id,
            account_login: binding.account_login,
            permissions: binding.permissions,
            recorded_at: binding.created_at,
            covered_repositories: status
                .covered_repositories
                .into_iter()
                .map(|repository| GithubAppCoveredRepositoryDTO {
                    github_repo_id: repository.github_repo_id,
                    full_name: repository.full_name,
                })
                .collect(),
        }
    }
}

fn binding_status_response(
    status: Option<GithubAppBindingStatus>,
) -> GithubAppBindingStatusResponseDTO {
    GithubAppBindingStatusResponseDTO {
        notice: RECORDED_BINDING_NOTICE,
        binding: status.map(GithubAppBindingDTO::from),
    }
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Organization-admin routes nested at `/api/v1/orgs/:orgId/github-app`.
pub fn router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .audited_route("/binding", get(get_github_app_binding), &[Scope::OrgAdmin])
        .audited_route(
            "/connection-attempt",
            post(create_github_app_connection_attempt),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/reconnect-attempt",
            post(create_github_app_reconnect_attempt),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/binding/repositories",
            get(list_github_app_installation_repositories),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/binding/coverage",
            put(update_github_app_coverage),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/binding/refresh",
            post(refresh_github_app_binding),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/binding/disconnect",
            post(disconnect_github_app),
            &[Scope::OrgAdmin],
        )
        .audited_route(
            "/binding/management-url",
            get(get_github_app_management_url),
            &[Scope::OrgAdmin],
        )
        .into_router()
}

/// GitHub browser callbacks nested at `/api/v1/github-app`.
pub fn callback_router() -> Router<Arc<AppState>> {
    AuditedRouter::new()
        .exempt_route(
            "/setup",
            get(github_app_setup_callback),
            "GitHub App setup redirect; authorized by HMAC-signed attempt state matched to its stored digest",
        )
        .exempt_route(
            "/oauth/callback",
            get(github_app_oauth_callback),
            "GitHub App OAuth callback; authorized by signed state, stored digest, signed PKCE cookie and GitHub installation access",
        )
        .exempt_route(
            "/reconnect/select",
            get(github_app_reconnect_selection),
            "GitHub App reconnect choice; authorized by signed selection state matched to its stored digest",
        )
        .exempt_route(
            "/user-connection",
            get(get_user_connection),
            "the person's own GitHub connection status; person session required in handler, and an after-login offer read also requires membership of the named org",
        )
        .exempt_route(
            "/user-connection",
            post(start_user_connection),
            "the person starts authorizing the G5 GitHub App for themselves; person session required in handler",
        )
        .exempt_route(
            "/user-connection/disconnect",
            post(disconnect_user_connection),
            "the person forgets their own GitHub connection; person session required in handler",
        )
        .into_router()
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn github_app_client(state: &AppState) -> Result<&GithubAppClient, AppError> {
    state.github_app.as_deref().ok_or_else(|| {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "github app integration is not configured".to_string(),
        })
    })
}

/// A non-member receives the non-enumerating `404` projection.
pub(crate) fn mask_foreign_org_access(error: AppError) -> AppError {
    match &error.inner {
        G5Error::Forbidden { missing_scopes }
            if missing_scopes.iter().any(|scope| scope == "org_membership") =>
        {
            AppError::from_error(G5Error::NotFound {
                resource: "organization".to_string(),
            })
        }
        _ => error,
    }
}

/// Open the path org's tenant transaction and require that the caller is an
/// admin of that org. The route's `OrgAdmin` scope describes the
/// JWT's selected org, which need not be the path org, so the role is checked
/// against the path org here.
pub(crate) async fn org_admin_db(
    state: &Arc<AppState>,
    auth: &AuthContext,
    org_id: Uuid,
) -> Result<(g5_authz::ScopedDb, Uuid), AppError> {
    let actor = auth.require_user_id().map_err(AppError::from_error)?;
    let db = scoped_db_for_org(state, auth, org_id, ORG_ADMIN_SCOPE)
        .await
        .map_err(mask_foreign_org_access)?;
    if !is_org_admin(&state.system_pool, org_id, actor).await? {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["org:admin".to_string()],
        }));
    }
    Ok((db, actor))
}

fn with_pkce_cookie(mut response: Response, state: &AppState, cookie: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    if state.auth_service.cookie_secure() {
        let legacy = format!(
            "{}=; HttpOnly; Secure; SameSite=Lax; Path=/api/v1/github-app; Max-Age=0",
            pkce_cookie_name(false)
        );
        if let Ok(value) = HeaderValue::from_str(&legacy) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

fn pkce_set_cookie(state: &AppState, value: &str) -> String {
    let secure = if state.auth_service.cookie_secure() {
        "; Secure"
    } else {
        ""
    };
    let path = if state.auth_service.cookie_secure() {
        "/"
    } else {
        "/api/v1/github-app"
    };
    format!(
        "{}={value}; HttpOnly{secure}; SameSite=Lax; Path={path}; Max-Age=600",
        pkce_cookie_name(state.auth_service.cookie_secure())
    )
}

fn pkce_clear_cookie(state: &AppState) -> String {
    let secure = if state.auth_service.cookie_secure() {
        "; Secure"
    } else {
        ""
    };
    let path = if state.auth_service.cookie_secure() {
        "/"
    } else {
        "/api/v1/github-app"
    };
    format!(
        "{}=; HttpOnly{secure}; SameSite=Lax; Path={path}; Max-Age=0",
        pkce_cookie_name(state.auth_service.cookie_secure())
    )
}

/// The PKCE verifier from this browser's cookie when it was issued for
/// `attempt`; a cookie for another attempt (or none) cannot continue it.
fn attempt_pkce_verifier(
    state: &AppState,
    headers: &HeaderMap,
    attempt: &ConnectionAttemptRef,
) -> Option<String> {
    let cfg = state.github_app.as_deref()?.config();
    credential_cookie(
        headers,
        pkce_cookie_name(state.auth_service.cookie_secure()),
    )
    .and_then(|cookie| parse_pkce_cookie(cfg, cookie, &attempt.state_digest))
}

// ─── A person's GitHub connection ────────────────────────────────────────────

const USER_GRANT_STATE_PREFIX: &str = "g5gu1.";

fn user_token_manager(state: &AppState) -> Result<&crate::auth::ScmTokenManager, AppError> {
    state.scm_token_manager.as_deref().ok_or_else(|| {
        AppError::from_error(G5Error::ServiceUnavailable {
            message: "GitHub connections are not configured on this deployment".to_string(),
        })
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartUserConnectionRequest {
    /// Relative G5 path to return to after authorizing.
    pub return_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConnectionQuery {
    /// The organization the person is entering after login. When present, the
    /// response also says whether to offer the connection there.
    pub after_login_org_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConnectionDTO {
    #[serde(flatten)]
    pub status: UserGrantStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offer_after_login: Option<bool>,
}

/// `GET /api/v1/github-app/user-connection[?afterLoginOrgId=…]`
async fn get_user_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<UserConnectionQuery>,
) -> Result<Json<ResponseEnvelope<UserConnectionDTO>>, AppError> {
    let user_id = extract_session_user_id(&auth)?;
    let status = grant_status(&state.system_pool, user_id).await?;
    let offer_after_login = match query.after_login_org_id {
        None => None,
        // Nothing to offer, so no organization read.
        Some(_) if state.github_app.is_none() || status.connected => Some(false),
        Some(org_id) => {
            let mut db = scoped_db_for_org(&state, &auth, org_id, &[])
                .await
                .map_err(mask_foreign_org_access)?;
            Some(
                org_offers_connection(
                    &mut db,
                    user_id,
                    org_id,
                    state.repository_credentials.origin_policy(),
                )
                .await?,
            )
        }
    };
    Ok(Json(ResponseEnvelope::ok(UserConnectionDTO {
        status,
        offer_after_login,
    })))
}

/// Whether a disconnected person is offered the shared connection after login
/// (`g5.settings.github-connection`): they are more than a viewer of the
/// organization they are entering and it uses GitHub. Only the login landing
/// asks (`afterLoginOrgId`), so other status reads pay nothing for the
/// organization read; `db` is that organization's membership-verified tenant
/// scope, so a non-member has already received the non-enumerating `404`.
async fn org_offers_connection(
    db: &mut g5_authz::ScopedDb,
    user_id: Uuid,
    org_id: Uuid,
    origin_policy: &g5_scm::ScmOriginPolicy,
) -> Result<bool, AppError> {
    let role =
        PermissionService::require_active_member_on_conn(db.conn(), user_id, org_id, false).await?;
    if role == OrgRole::Viewer {
        return Ok(false);
    }
    Ok(org_uses_github(db.conn(), org_id, origin_policy).await?)
}

/// `POST /api/v1/github-app/user-connection`
///
/// Start authorizing the G5 GitHub App for the caller and return GitHub's
/// authorization URL. The PKCE verifier is set as an HttpOnly cookie bound
/// to this attempt.
async fn start_user_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(request): Json<StartUserConnectionRequest>,
) -> Result<Response, AppError> {
    let user_id = extract_session_user_id(&auth)?;
    let client = github_app_client(&state)?;
    user_token_manager(&state)?;
    let cfg = client.config();
    let oauth_state =
        start_connection(&state.system_pool, client, user_id, &request.return_path).await?;
    let (verifier, cookie) = new_pkce_cookie(cfg, &connection_state_digest(&oauth_state));
    let redirect_url = oauth_authorize_url(cfg, &oauth_state, &pkce_code_challenge(&verifier));
    let response = (
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(GithubAppRedirectDTO { redirect_url })),
    )
        .into_response();
    Ok(with_pkce_cookie(
        response,
        &state,
        &pkce_set_cookie(&state, &cookie),
    ))
}

/// `POST /api/v1/github-app/user-connection/disconnect`
async fn disconnect_user_connection(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<ResponseEnvelope<UserGrantStatus>>, AppError> {
    let user_id = extract_session_user_id(&auth)?;
    delete_grant(&state.system_pool, user_id).await?;
    let status = grant_status(&state.system_pool, user_id).await?;
    Ok(Json(ResponseEnvelope::ok(status)))
}

fn with_query_flag(path: &str, key: &str, value: &str) -> String {
    let separator = if path.contains('?') { '&' } else { '?' };
    format!("{path}{separator}{key}={value}")
}

/// Complete a person's "Connect GitHub": verify the signed state, its stored
/// attempt and this browser's PKCE cookie, exchange the code, and keep the
/// token encrypted. The browser returns to the page that started it.
async fn user_connection_callback(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    code: Option<String>,
    raw_state: &str,
) -> Response {
    let attempt = state
        .github_app
        .as_deref()
        .and_then(|client| verify_user_connection_state(client.config(), raw_state, unix_now()));
    let return_path = match &attempt {
        Some(attempt) => pending_attempt_return_path(&state.system_pool, attempt)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let result = async {
        let (Some(attempt), Some(_)) = (&attempt, &return_path) else {
            return Err(callback_invalid());
        };
        let client = github_app_client(state)?;
        let token_manager = user_token_manager(state)?;
        let verifier = credential_cookie(headers, pkce_cookie_name(state.auth_service.cookie_secure()))
            .and_then(|cookie| parse_pkce_cookie(client.config(), cookie, &attempt.state_digest))
            .ok_or_else(callback_invalid)?;
        let code = code
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(callback_invalid)?;
        let tokens = client.exchange_user_grant_code(&code, &verifier).await?;
        let github_user = client.authenticated_user(&tokens.access_token).await?;
        complete_connection(
            &state.system_pool,
            token_manager,
            attempt,
            &tokens,
            &github_user,
        )
        .await?
        .ok_or_else(callback_invalid)?;
        tracing::info!(user_id = %attempt.user_id, github_login = %github_user.login, "github user connection stored");
        Ok::<_, AppError>(())
    }
    .await;
    let base = return_path.unwrap_or_else(|| "/".to_string());
    let location = match result {
        Ok(()) => with_query_flag(&base, "githubConnection", "connected"),
        Err(error) => {
            tracing::warn!(error = %error.inner, "github user connection callback rejected");
            with_query_flag(&base, "githubConnection", "failed")
        }
    };
    with_pkce_cookie(
        Redirect::to(&location).into_response(),
        state,
        &pkce_clear_cookie(state),
    )
}

// ─── Organization-admin handlers ─────────────────────────────────────────────

/// `GET /api/v1/orgs/:orgId/github-app/binding`
///
/// The latest recorded snapshot. The notice is contractual: durable state
/// alone is not a live GitHub verification.
async fn get_github_app_binding(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<GithubAppBindingStatusResponseDTO>>, AppError> {
    github_app_client(&state)?;
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    let status = load_binding_status(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(binding_status_response(status))))
}

/// `POST /api/v1/orgs/:orgId/github-app/connection-attempt`
///
/// Create a ten-minute, single-use attempt and return GitHub's installation
/// page carrying its signed state. The attempt's PKCE verifier is set as an
/// HttpOnly cookie bound to the attempt, so only this browser can complete
/// the setup and OAuth callbacks.
async fn create_github_app_connection_attempt(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Response, AppError> {
    let client = github_app_client(&state)?;
    let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
    let attempt = create_connection_attempt(&mut db, client.config(), params.org_id, actor).await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(org_id = %params.org_id, attempt_id = %attempt.attempt_id, "github app connection attempt created");
    let (_, cookie) = new_pkce_cookie(client.config(), &connection_state_digest(attempt.state()));
    let response = (
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(GithubAppRedirectDTO {
            redirect_url: attempt.installation_url(client.config()),
        })),
    )
        .into_response();
    Ok(with_pkce_cookie(
        response,
        &state,
        &pkce_set_cookie(&state, &cookie),
    ))
}

/// `POST /api/v1/orgs/:orgId/github-app/reconnect-attempt`
///
/// Create an attempt for an App already installed on GitHub and return the
/// OAuth discovery URL. The PKCE verifier is set as an HttpOnly cookie bound
/// to the attempt.
async fn create_github_app_reconnect_attempt(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Response, AppError> {
    let client = github_app_client(&state)?;
    let cfg = client.config();
    let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
    let attempt = create_connection_attempt(&mut db, cfg, params.org_id, actor).await?;
    db.commit().await.map_err(AppError::from)?;
    let (verifier, cookie) = new_pkce_cookie(cfg, &connection_state_digest(attempt.state()));
    let oauth_state = oauth_state_for_reconnect_discovery(cfg, attempt.state());
    let redirect_url = oauth_authorize_url(cfg, &oauth_state, &pkce_code_challenge(&verifier));
    let response = (
        StatusCode::CREATED,
        Json(ResponseEnvelope::ok(GithubAppRedirectDTO { redirect_url })),
    )
        .into_response();
    Ok(with_pkce_cookie(
        response,
        &state,
        &pkce_set_cookie(&state, &cookie),
    ))
}

/// `GET /api/v1/orgs/:orgId/github-app/binding/repositories`
///
/// The live list of repositories the active installation can access, marked
/// with the latest snapshot's coverage. No transaction is held during the
/// GitHub request.
async fn list_github_app_installation_repositories(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<GithubAppInstallationRepositoriesDTO>>, AppError> {
    let client = github_app_client(&state)?;
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    let status = load_binding_status(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let status = status
        .filter(|status| status.binding.is_active())
        .ok_or_else(no_active_binding)?;
    let covered: BTreeSet<i64> = status
        .covered_repositories
        .iter()
        .map(|repository| repository.github_repo_id)
        .collect();
    let mut repositories = client
        .installation_repositories(status.binding.github_installation_id())
        .await?;
    repositories.sort_by(|left, right| {
        left.full_name
            .to_ascii_lowercase()
            .cmp(&right.full_name.to_ascii_lowercase())
    });
    Ok(Json(ResponseEnvelope::ok(
        GithubAppInstallationRepositoriesDTO {
            binding_version: status.binding.version,
            repositories: repositories
                .into_iter()
                .map(|repository| GithubAppInstallationRepositoryDTO {
                    covered: covered.contains(&repository.github_repo_id),
                    github_repo_id: repository.github_repo_id,
                    full_name: repository.full_name,
                    private: repository.private,
                })
                .collect(),
        },
    )))
}

/// `PUT /api/v1/orgs/:orgId/github-app/binding/coverage`
///
/// Replace coverage with exactly `githubRepoIds`, each re-verified against the
/// installation's live repository list, as snapshot version N+1 that also
/// records GitHub's current installation permissions and account. The
/// version check makes concurrent edits fail with `409` instead of
/// overwriting.
async fn update_github_app_coverage(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(request): Json<UpdateGithubAppCoverageRequest>,
) -> Result<Json<ResponseEnvelope<GithubAppBindingStatusResponseDTO>>, AppError> {
    let client = github_app_client(&state)?;
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    let binding = load_latest_binding(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let binding = binding
        .filter(|binding| binding.is_active())
        .ok_or_else(no_active_binding)?;
    if binding.version != request.binding_version {
        return Err(AppError::from_error(G5Error::Conflict {
            reason: "github app binding changed; reload and retry".to_string(),
        }));
    }
    let installation_id = binding.github_installation_id();
    let (accessible, installation) = tokio::try_join!(
        client.installation_repositories(installation_id),
        client.installation_metadata(installation_id),
    )?;
    let coverage = validate_coverage_selection(&request.github_repo_ids, &accessible)
        .map_err(G5Error::from)?;

    let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
    append_coverage_change(
        &mut db,
        params.org_id,
        actor,
        request.binding_version,
        &installation,
        &coverage,
    )
    .await?;
    let status = load_binding_status(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    Ok(Json(ResponseEnvelope::ok(binding_status_response(status))))
}

/// `POST /api/v1/orgs/:orgId/github-app/binding/refresh`
///
/// Re-read the active installation from GitHub and, when its permissions or
/// account changed (for example after an owner accepted new permissions),
/// append snapshot version N+1 with the same coverage. An unchanged
/// installation returns the current status without a new snapshot. No
/// transaction is held during the GitHub request.
async fn refresh_github_app_binding(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(request): Json<RefreshGithubAppBindingRequest>,
) -> Result<Json<ResponseEnvelope<GithubAppBindingStatusResponseDTO>>, AppError> {
    let client = github_app_client(&state)?;
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    let binding = load_latest_binding(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let binding = binding
        .filter(|binding| binding.is_active())
        .ok_or_else(no_active_binding)?;
    let installation = client
        .installation_metadata(binding.github_installation_id())
        .await?;

    let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
    let refreshed = append_installation_refresh(
        &mut db,
        params.org_id,
        actor,
        request.binding_version,
        &installation,
    )
    .await?;
    let status = load_binding_status(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    if let Some(binding) = refreshed {
        tracing::info!(
            org_id = %params.org_id,
            version = binding.version,
            "github app binding refreshed from installation"
        );
    }
    Ok(Json(ResponseEnvelope::ok(binding_status_response(status))))
}

/// `POST /api/v1/orgs/:orgId/github-app/binding/disconnect`
///
/// Append a disconnected snapshot. No GitHub request is made, so revocation
/// works while GitHub is unavailable; the GitHub installation is unchanged.
async fn disconnect_github_app(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Json(request): Json<DisconnectGithubAppRequest>,
) -> Result<Json<ResponseEnvelope<GithubAppBindingStatusResponseDTO>>, AppError> {
    github_app_client(&state)?;
    let (mut db, actor) = org_admin_db(&state, &auth, params.org_id).await?;
    append_disconnect(&mut db, params.org_id, actor, request.binding_version).await?;
    let status = load_binding_status(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(org_id = %params.org_id, "github app binding disconnected");
    Ok(Json(ResponseEnvelope::ok(binding_status_response(status))))
}

/// `GET /api/v1/orgs/:orgId/github-app/binding/management-url`
///
/// GitHub's settings page for the recorded installation, validated against
/// the configured GitHub web origin. Navigation only.
async fn get_github_app_management_url(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
) -> Result<Json<ResponseEnvelope<GithubAppManagementUrlDTO>>, AppError> {
    let client = github_app_client(&state)?;
    let (mut db, _) = org_admin_db(&state, &auth, params.org_id).await?;
    let binding = load_latest_binding(&mut db, params.org_id).await?;
    db.commit().await.map_err(AppError::from)?;
    let binding = binding.ok_or_else(no_active_binding)?;
    let installation_id = binding.github_installation_id();
    let installation = client.installation_metadata(installation_id).await?;
    let management_url = installation
        .html_url
        .as_deref()
        .and_then(|url| {
            validated_installation_management_url(client.config(), url, installation_id)
        })
        .ok_or_else(|| {
            AppError::from_error(G5Error::ServiceUnavailable {
                message: "github installation settings URL was unavailable".to_string(),
            })
        })?;
    Ok(Json(ResponseEnvelope::ok(GithubAppManagementUrlDTO {
        management_url,
    })))
}

fn no_active_binding() -> AppError {
    AppError::from_error(G5Error::Conflict {
        reason: "github app binding is not active".to_string(),
    })
}

// ─── Browser callbacks ───────────────────────────────────────────────────────

/// Load the attempt a verified state names, under that org's RLS scope, and
/// require it to match the state's digest and still be pending.
async fn pending_attempt(
    state: &Arc<AppState>,
    attempt: &ConnectionAttemptRef,
) -> Result<PendingConnectionAttempt, AppError> {
    let mut db = scoped_db_for_system_org(state, attempt.org_id, ORG_ADMIN_SCOPE).await?;
    let pending = load_pending_attempt(&mut db, attempt).await?;
    db.commit().await.map_err(AppError::from)?;
    pending.ok_or_else(callback_invalid)
}

fn callback_invalid() -> AppError {
    AppError::from_error(G5Error::Unauthorized)
}

async fn org_settings_path(state: &AppState, org_id: Uuid, outcome: &str) -> String {
    match org_slug(&state.system_pool, org_id).await.ok().flatten() {
        Some(slug) => {
            let slug: String = url::form_urlencoded::byte_serialize(slug.as_bytes()).collect();
            format!("/{slug}/settings/github-app?githubAppConnection={outcome}")
        }
        None => format!("/?githubAppConnection={outcome}"),
    }
}

/// Redirect a failed callback. When the state verified, the org is known
/// and the admin returns to its settings page; otherwise to the root.
async fn callback_failure(state: &AppState, org_id: Option<Uuid>) -> Response {
    let location = match org_id {
        Some(org_id) => org_settings_path(state, org_id, "failed").await,
        None => "/?githubAppConnection=failed".to_string(),
    };
    Redirect::to(&location).into_response()
}

/// `GET /api/v1/github-app/setup`
///
/// GitHub's post-installation redirect. The untrusted `installation_id` is
/// bound into a signed OAuth state only after the attempt state verifies,
/// matches a pending attempt, and the browser presents that attempt's PKCE
/// cookie; then GitHub App OAuth begins with the cookie's verifier.
async fn github_app_setup_callback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<GithubAppSetupQuery>,
) -> Response {
    let installation_id = query
        .installation_id
        .as_deref()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|id| *id > 0);
    let raw_state = query.state.filter(|value| !value.trim().is_empty());
    // GitHub sends a state-less `setup_action=update` after settings changes
    // made on GitHub. It can never create a binding.
    if raw_state.is_none() && query.setup_action.as_deref() == Some("update") {
        return Redirect::to("/?githubAppConnection=updated").into_response();
    }
    let Some(client) = state.github_app.as_deref() else {
        return callback_failure(&state, None).await;
    };
    let cfg = client.config();
    let Some(raw_state) = raw_state else {
        return callback_failure(&state, None).await;
    };
    let Some(attempt) = verify_connection_state(cfg, &raw_state, unix_now()) else {
        return callback_failure(&state, None).await;
    };
    let org_id = Some(attempt.org_id);
    let Some(installation_id) = installation_id else {
        return callback_failure(&state, org_id).await;
    };
    if pending_attempt(&state, &attempt).await.is_err() {
        return callback_failure(&state, org_id).await;
    }
    let Some(verifier) = attempt_pkce_verifier(&state, &headers, &attempt) else {
        return callback_failure(&state, org_id).await;
    };
    let Some(oauth_state) = oauth_state_for_installation(cfg, &raw_state, installation_id) else {
        return callback_failure(&state, org_id).await;
    };
    let location = oauth_authorize_url(cfg, &oauth_state, &pkce_code_challenge(&verifier));
    Redirect::to(&location).into_response()
}

/// `GET /api/v1/github-app/reconnect/select`
///
/// Start a fresh OAuth proof for the installation chosen on the reconnect
/// page. The choice is signed but is not itself proof of access, and it
/// requires the attempt's PKCE cookie; the verifier is rotated because the
/// discovery exchange already used it.
async fn github_app_reconnect_selection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<GithubAppReconnectSelectionQuery>,
) -> Response {
    let Some(client) = state.github_app.as_deref() else {
        return callback_failure(&state, None).await;
    };
    let cfg = client.config();
    let Some(selection_state) = query.state else {
        return callback_failure(&state, None).await;
    };
    let Some(GithubAppOAuthState::ReconnectSelection { attempt, .. }) =
        parse_oauth_state(cfg, &selection_state, unix_now())
    else {
        return callback_failure(&state, None).await;
    };
    if pending_attempt(&state, &attempt).await.is_err()
        || attempt_pkce_verifier(&state, &headers, &attempt).is_none()
    {
        return callback_failure(&state, Some(attempt.org_id)).await;
    }
    let (verifier, cookie) = new_pkce_cookie(cfg, &attempt.state_digest);
    let location = oauth_authorize_url(cfg, &selection_state, &pkce_code_challenge(&verifier));
    with_pkce_cookie(
        Redirect::to(&location).into_response(),
        &state,
        &pkce_set_cookie(&state, &cookie),
    )
}

enum CallbackOutcome {
    Connected(Uuid),
    Choose(Vec<ReconnectChoice>),
}

struct ReconnectChoice {
    account_login: String,
    selection_state: String,
}

/// `GET /api/v1/github-app/oauth/callback`
///
/// Complete the OAuth bridge. Failures are generic and secret-free; the PKCE
/// cookie is always cleared.
async fn github_app_oauth_callback(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<GithubAppOAuthCallbackQuery>,
) -> Response {
    // A person's "Connect GitHub" returns through the same callback URL.
    if let Some(raw_state) = query
        .state
        .as_deref()
        .filter(|value| value.starts_with(USER_GRANT_STATE_PREFIX))
    {
        return user_connection_callback(&state, &headers, query.code, raw_state).await;
    }
    let parsed = state.github_app.as_deref().and_then(|client| {
        query
            .state
            .as_deref()
            .and_then(|value| parse_oauth_state(client.config(), value, unix_now()))
    });
    let org_id = parsed.as_ref().map(|parsed| parsed.attempt().org_id);
    let result = match parsed {
        Some(parsed) => complete_oauth_callback(&state, &headers, query.code, parsed).await,
        None => Err(callback_invalid()),
    };
    let response = match result {
        Ok(CallbackOutcome::Connected(org_id)) => {
            Redirect::to(&org_settings_path(&state, org_id, "connected").await).into_response()
        }
        // The attempt's PKCE cookie must survive until the chosen
        // installation's selection request.
        Ok(CallbackOutcome::Choose(choices)) => return reconnect_choice_page(&choices),
        Err(error) => {
            tracing::warn!(org_id = ?org_id, error = %error.inner, "github app OAuth callback rejected");
            callback_failure(&state, org_id).await
        }
    };
    with_pkce_cookie(response, &state, &pkce_clear_cookie(&state))
}

async fn complete_oauth_callback(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    code: Option<String>,
    oauth_state: GithubAppOAuthState,
) -> Result<CallbackOutcome, AppError> {
    let client = github_app_client(state)?;
    // The attempt is checked before any provider work; the binding append
    // re-checks it under the org lock and consumes it.
    let attempt = pending_attempt(state, oauth_state.attempt()).await?;
    let verifier = attempt_pkce_verifier(state, headers, oauth_state.attempt())
        .ok_or_else(callback_invalid)?;
    let code = code
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(callback_invalid)?;
    let user_token = client.exchange_oauth_code(&code, &verifier).await?;

    let installation_id = match &oauth_state {
        GithubAppOAuthState::Installation {
            installation_id, ..
        }
        | GithubAppOAuthState::ReconnectSelection {
            installation_id, ..
        } => *installation_id,
        GithubAppOAuthState::ReconnectDiscovery {
            attempt: attempt_ref,
        } => {
            let installations = client.user_accessible_installations(&user_token).await?;
            match installations.as_slice() {
                [] => return Err(callback_invalid()),
                [only] => only.installation_id,
                _ => {
                    // The user token must not outlive this request; the
                    // chosen installation gets a fresh OAuth proof.
                    drop(user_token);
                    let choices = installations
                        .into_iter()
                        .filter_map(|installation| {
                            reconnect_choice(client, attempt_ref, installation)
                        })
                        .collect();
                    return Ok(CallbackOutcome::Choose(choices));
                }
            }
        }
    };
    client
        .verify_user_installation_access(&user_token, installation_id)
        .await?;
    drop(user_token);

    let installation = client.installation_metadata(installation_id).await?;
    // Minting an installation token proves the installation is usable (not
    // suspended) before it is recorded as active.
    client.installation_repositories(installation_id).await?;

    if !is_org_admin(&state.system_pool, attempt.org_id, attempt.created_by).await? {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["org:admin".to_string()],
        }));
    }
    let mut db =
        scoped_db_for_user_actor_in_org(state, attempt.created_by, attempt.org_id, ORG_ADMIN_SCOPE)
            .await?;
    let binding = append_verified_binding(&mut db, &attempt, &installation).await?;
    db.commit().await.map_err(AppError::from)?;
    tracing::info!(
        org_id = %attempt.org_id,
        version = binding.version,
        installation_id = binding.installation_id,
        "github app binding recorded"
    );
    Ok(CallbackOutcome::Connected(attempt.org_id))
}

fn reconnect_choice(
    client: &GithubAppClient,
    attempt: &ConnectionAttemptRef,
    installation: GithubAppAccessibleInstallation,
) -> Option<ReconnectChoice> {
    Some(ReconnectChoice {
        selection_state: oauth_state_for_reconnect_selection(
            client.config(),
            attempt,
            installation.installation_id,
        )?,
        account_login: installation.account_login,
    })
}

fn reconnect_selection_href(selection_state: &str) -> String {
    let encoded: String =
        url::form_urlencoded::byte_serialize(selection_state.as_bytes()).collect();
    format!("/api/v1/github-app/reconnect/select?state={encoded}")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn reconnect_choice_page(choices: &[ReconnectChoice]) -> Response {
    let items = choices
        .iter()
        .map(|choice| {
            format!(
                "<li><a href=\"{}\">Connect @{}</a></li>",
                html_escape(&reconnect_selection_href(&choice.selection_state)),
                html_escape(&choice.account_login),
            )
        })
        .collect::<String>();
    let mut response = (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Choose a GitHub App installation</title></head><body><main><h1>Choose a GitHub App installation</h1><p>GitHub confirmed that you can access these installations. Choose the one G5 should connect.</p><ul>{items}</ul></main></body></html>"
        ),
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_page_escapes_provider_values() {
        let response = reconnect_choice_page(&[ReconnectChoice {
            account_login: "<script>".to_string(),
            selection_state: "s\"x".to_string(),
        }]);
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
    }
}
