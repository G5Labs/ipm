//! Auth service — OAuth login flows, JWT minting, token refresh, SCM token
//! management, and WebSocket ticket minting.
//!
//! This is the main authentication and authorization service for `g5-api`,
//! responsible for:
//!
//! 1. **Identity OAuth flows** — Initiating and completing OAuth login with
//!    GitHub or Microsoft Entra ID to establish user identity.
//! 2. **JWT minting** — Creating ES256-signed JWTs with user claims and
//!    effective scopes.
//! 3. **Token refresh** — Validating opaque refresh tokens and issuing new JWTs.
//! 4. **SCM connection management** — Separate OAuth flows for connecting
//!    source-code providers with broader repo-access scopes.
//! 5. **SCM token encryption/decryption** — AES-256-GCM envelope encryption
//!    for storing SCM tokens at rest.
//! 6. **Repo listing and import** — Fetching accessible repos from SCM
//!    providers and importing them into G5 orgs.
//! 7. **WebSocket ticket minting** — Short-lived (5-minute) JWTs for WebSocket
//!    upgrade authentication.
//! 8. **Background SCM token refresh** — Proactively refreshing expiring
//!    GitLab OAuth tokens.
//!
//! This service is the ONLY component that mints JWTs. The `g5-common/auth`
//! module provides shared validation logic consumed by both `g5-api` and `g5d`,
//! but minting is exclusively `g5-api`'s responsibility via this service.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use chrono::Utc;
use g5_scm::providers::{
    AccountGrant, LoginClient, LoginConfig, LoginUser, OAuthTokens, ProviderClients, ProviderError,
};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use g5_auth::{
    preview_ticket_audience, websocket_ticket_audience, JwtClaims, Scope,
    PREVIEW_HANDOFF_TOKEN_USE, PREVIEW_TOKEN_USE, SESSION_CLIENT_BROWSER, SESSION_CLIENT_CLI,
    WEBSOCKET_TOKEN_USE,
};
use g5_authz::{AuthzService, DbCapability, ScopedDb};
use g5_contracts::types::enums::OrgRole;
use g5_contracts::types::ids::{OrgId, SessionId, UserId};

use crate::auth::oauth_attempt::{attempt_verifier, OAuthAttempt, OAUTH_ATTEMPT_TTL_SECS};
use crate::auth::return_path::is_local_return_path;
use crate::config::AuthConfig;
use crate::db::scoped::authorize_org_scopes;
use crate::errors::{AppError, G5Error};
use crate::middleware::{extract_session_user_id, AuthContext};
use crate::services::org_service::{CreateOrgRequest, OrgService, OrganizationCreated};

/// Lifetime of a browser preview ticket in seconds (30 minutes; issue
/// G5Labs/G5#736, `routes/preview` [D8]). Long enough that the previewed
/// app's own asset/XHR sub-requests keep working through a normal preview
/// session (5 minutes would cut them off mid-session), short enough to bound
/// the replay window; the SPA re-mints on every "Open preview" click. Also
/// used as the `g5_preview` cookie `Max-Age`.
pub const PREVIEW_TICKET_LIFETIME_SECS: i64 = 1800;

/// Lifetime of a single-use preview HANDOFF ticket in seconds (issue
/// G5Labs/G5#935, `routes/preview` [D15]). The ticket rides the
/// `__g5_preview_ticket` URL parameter of the per-preview subdomain entry
/// URL and only has to survive one `window.open` navigation; it is consumed
/// exactly once (keyed by `jti`) and exchanged for the 30-minute session
/// cookie, so a lingering URL (history, logs) dies within a minute.
pub const PREVIEW_HANDOFF_TICKET_LIFETIME_SECS: i64 = 60;

// ---------------------------------------------------------------------------
// Configuration Types
// ---------------------------------------------------------------------------

/// OAuth provider configuration for login or SCM connection.
///
/// Two separate config maps exist in `AuthService`:
/// - `oauth_configs` — Login OAuth with minimal identity-only scopes.
/// - `scm_oauth_configs` — SCM connection OAuth with broader repo-access scopes.
#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    pub client_id: String,
    pub client_secret: String,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

// ---------------------------------------------------------------------------
// DTO Types — Public API Surface
// ---------------------------------------------------------------------------

/// OAuth redirect response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthRedirect {
    pub url: String,
    pub state: String,
    /// Value of the attempt cookie the route sets on the initiating browser
    /// (`crate::auth::oauth_attempt`); never part of a response body.
    #[serde(skip)]
    pub attempt_cookie: String,
}

/// Authentication tokens returned after login or refresh.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokens {
    pub jwt: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user_id: Uuid,
}

/// Outcome of an OAuth login callback.
///
/// Admission (`g5.user.admission`) can refuse a login, and the `callback`
/// route needs the attempted verified email to render a diagnosable denied
/// page. Carrying that email through `G5Error::Forbidden` would pollute a
/// shared error used across the API and risk the address leaking into generic
/// error-logging paths, so a refused admission is a successful, typed outcome
/// rather than an `Err`. Genuine failures (invalid state, ambiguous linkage,
/// provider/internal) still return `Err(AppError)`.
#[derive(Debug, Clone)]
pub enum CallbackOutcome {
    /// The login was admitted; the session credentials were minted.
    Authenticated(AuthTokens),
    /// The identity has neither a pending invitation nor a membership.
    /// `email` is the attempted verified address the visitor just proved
    /// control of — surfaced only on the denied page, never through error
    /// logging.
    AdmissionDenied { email: String, provider: String },
}

/// Mask an email address for structured logs: keep the first character of the
/// local part and the full domain, redacting the rest (e.g. `alice@corp.com`
/// -> `a***@corp.com`).
///
/// Admission denials are diagnosed by domain and rough identity, so the
/// initial-plus-domain form preserves the operator's triage signal ("which
/// domain, which rough identity got denied") without writing the raw personal
/// address to log storage (CWE-532). The exact address is still delivered to
/// the visitor's own browser via the denied-page cookie. Malformed inputs with
/// no local part or no domain collapse to a fully redacted `***`.
fn mask_email_for_log(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) if !local.is_empty() && !domain.is_empty() => {
            let first = local.chars().next().unwrap_or('*');
            format!("{first}***@{domain}")
        }
        _ => "***".to_string(),
    }
}

/// A freshly provisioned org's Base docs+code repositories that the route
/// layer must materialize post-commit through the canonical finalizer.
///
/// `issue_test_token` provisions the org's DB rows through the shared
/// `OrgService::create_org` cascade but, holding no repository-storage handle,
/// does not run either repository's git bootstrap itself (auth_service [D3]).
/// It returns these targets so the route layer can complete materialization via
/// the shared docs+code bootstrap helpers, keeping repo storage out of
/// `AuthService`.
#[derive(Debug, Clone)]
pub struct TestTokenBaseRepoBootstrap {
    pub org_id: Uuid,
    pub base_workspace_id: Uuid,
    pub docs_repo_id: Uuid,
    pub code_repo_id: Uuid,
    pub code_repo_name: String,
}

/// A newly-created test org whose Base workspace can seed deterministic
/// fixture workspaces through the same checked fork service as production.
#[derive(Debug, Clone)]
pub struct TestTokenFixtureForkTarget {
    pub org_id: Uuid,
    pub base_workspace_id: Uuid,
}

/// Result of the E2E/dev test-token seed+mint path: the minted tokens plus the
/// per-org Base docs+code materialization targets the route layer must finish.
#[derive(Debug, Clone)]
pub struct TestTokenProvisioning {
    pub tokens: AuthTokens,
    pub base_repo_bootstraps: Vec<TestTokenBaseRepoBootstrap>,
    pub fixture_fork_targets: Vec<TestTokenFixtureForkTarget>,
}

/// Identity resolved from a refresh token WITHOUT side effects
/// (auth_service [D22]; see [`AuthService::resolve_refresh_identity`]).
#[derive(Debug, Clone)]
pub struct RefreshIdentity {
    pub user_id: Uuid,
    pub org_id: Uuid,
    pub scopes: Vec<Scope>,
    /// How the session was issued (`browser`, `cli`), or `None` for a session
    /// issued before sessions recorded it (g5.security.browser-session-actions).
    pub client: Option<String>,
}

/// Pending CLI device authorization start response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub expires_in: i64,
    pub interval: i64,
}

/// Browser-side approval result for a CLI device request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDeviceApproval {
    pub status: String,
}

/// CLI-side poll result for a device authorization request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliDevicePoll {
    pub status: String,
    pub tokens: Option<AuthTokens>,
    pub org_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
}

/// Result of connecting an SCM provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmConnectionResult {
    pub provider: String,
    pub provider_username: String,
    pub scopes: Vec<String>,
    pub connected_at: String,
    pub return_to: String,
}

/// SCM repo list response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScmRepoListResponse {
    pub repos: Vec<ScmRepoDTO>,
    pub next_cursor: Option<String>,
}

/// A repo from an SCM provider.
#[derive(Debug, Clone, Serialize)]
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
    pub imported_repo_id: Option<Uuid>,
}

/// Request to import repos from an SCM provider.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRepoSpec {
    pub provider_repo_id: String,
    pub full_name: String,
    pub clone_url: String,
    pub default_branch: String,
}

/// Imported repo DTO.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDTO {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub slug: String,
    pub git_url: String,
    pub default_branch: String,
    pub provider: Option<String>,
    pub provider_repo_id: Option<String>,
    pub provider_full_name: Option<String>,
    pub created_at: String,
}

/// Result of background SCM token refresh.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshScmResult {
    pub refreshed: i32,
    pub failed: i32,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Internal helper types
// ---------------------------------------------------------------------------

/// Platform login provider name for Microsoft Entra ID.
const MICROSOFT_PROVIDER: &str = "microsoft";

/// Tokens from a platform login code exchange. Only Entra returns an ID
/// token, which must be validated before any identity claims are used.
struct LoginTokens {
    access_token: String,
    id_token: Option<String>,
}

/// Parsed OAuth user info (provider-agnostic).
#[derive(Debug)]
struct OAuthUserInfo {
    oauth_id: String,
    email: String,
    verified_emails: Vec<String>,
    display_name: String,
    avatar_url: Option<String>,
    provider_username: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum GitHubEmailMode {
    LoginVerifiedSet,
    SkipEmailLookup,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ImportedRepoStatusRow {
    provider_repo_id: Option<String>,
    id: Uuid,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ExistingProviderRepoRow {
    id: Uuid,
    org_id: Uuid,
    name: String,
    slug: String,
    git_url: String,
    default_branch: String,
    provider: Option<String>,
    provider_repo_id: Option<String>,
    provider_full_name: Option<String>,
    created_at: chrono::DateTime<Utc>,
}

impl From<ExistingProviderRepoRow> for RepoDTO {
    fn from(row: ExistingProviderRepoRow) -> Self {
        RepoDTO {
            id: row.id,
            org_id: row.org_id,
            name: row.name,
            slug: row.slug,
            git_url: row.git_url,
            default_branch: row.default_branch,
            provider: row.provider,
            provider_repo_id: row.provider_repo_id,
            provider_full_name: row.provider_full_name,
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// AuthService
// ---------------------------------------------------------------------------

/// Authentication and authorization service.
///
/// Handles OAuth login flows, JWT minting, token refresh, SCM token management,
/// and WebSocket ticket minting. This is the ONLY component that holds the
/// ES256 private key and can mint JWTs.
pub struct AuthService {
    db: PgPool,
    tenant_pool: PgPool,
    authz: Arc<dyn AuthzService>,
    providers: Result<ProviderClients, ProviderError>,
    jwt_encoding_key: Option<EncodingKey>,
    #[allow(dead_code)]
    jwt_decoding_key: Option<DecodingKey>,
    /// The one stored SCM token owner: encryption at rest and the read path.
    scm_tokens: Option<crate::auth::ScmTokenManager>,
    oauth_configs: HashMap<String, OAuthProviderConfig>,
    scm_oauth_configs: HashMap<String, OAuthProviderConfig>,
    /// Entra ID login protocol (identity-only; never an SCM provider).
    entra: Option<crate::auth::entra::EntraClient>,
    scm_origin_policy: g5_scm::ScmOriginPolicy,
    /// What later contexts add to the organizations the E2E test-token path
    /// creates (`OrgService::with_created_hook`); installed by the
    /// composition root.
    org_created_hooks: Vec<Arc<dyn OrganizationCreated>>,
    access_token_ttl_secs: i64,
    refresh_token_ttl_secs: i64,
    /// Absolute refresh-token lifetime from issue (`refresh_tokens.created_at`).
    refresh_token_absolute_ttl_secs: i64,
    jwt_issuer: String,
    jwt_audience: String,
    /// Whether to include the `Secure` flag on auth cookies.
    /// `false` allows cookies over plain HTTP (CI / local dev).
    cookie_secure: bool,
    /// Effective wire name of the httpOnly refresh-token cookie.
    refresh_cookie_name: String,
    /// Former host-only name, retained only to expire the pre-cutover cookie.
    legacy_refresh_cookie_name: Option<String>,
    /// GitHub API base URL for fine-grained PAT validation (config-driven;
    /// real api.github.com in prod, a hermetic stub in dev/CI).
    scm_github_api_base_url: String,
    /// Operator-configured API root retained independently of OAuth enablement.
    scm_gitlab_api_root: String,
}

fn refresh_cookie_names(configured: &str, secure: bool) -> (String, Option<String>) {
    if secure && !configured.starts_with("__Host-") {
        (
            format!("__Host-{configured}"),
            (!configured.to_ascii_lowercase().starts_with("__host-"))
                .then(|| configured.to_string()),
        )
    } else {
        (configured.to_string(), None)
    }
}

impl AuthService {
    /// Create a new `AuthService` from the database pool and auth configuration.
    ///
    /// This constructor is called by `AppState::new` with `(db.clone(), config.auth.clone())`.
    /// It builds the internal OAuth config maps and parses JWT keys from the config.
    ///
    /// If `jwt_secret` is `None` (M0 mode), JWT minting methods will return errors.
    /// If OAuth providers are not configured, OAuth methods will return validation errors.
    pub fn new(
        db: PgPool,
        tenant_pool: PgPool,
        authz: Arc<dyn AuthzService>,
        auth_config: AuthConfig,
        external_url: &str,
    ) -> Self {
        let (refresh_cookie_name, legacy_refresh_cookie_name) =
            refresh_cookie_names(&auth_config.refresh_cookie_name, auth_config.cookie_secure);
        let providers = ProviderClients::new();

        // Build redirect URIs from external_url.
        let login_redirect_base = format!(
            "{}/api/v1/auth/callback",
            external_url.trim_end_matches('/')
        );
        let scm_redirect_base = format!("{}/api/v1/scm", external_url.trim_end_matches('/'));

        // Parse JWT keys from the secret (PEM-encoded ES256 key pair).
        // In M0 mode, jwt_secret is None and minting is disabled.
        let (jwt_encoding_key, jwt_decoding_key) = match &auth_config.jwt_secret {
            Some(secret) => {
                // Attempt to parse as ES256 PEM key pair.
                // The secret may contain the private key PEM; the public key
                // is derived or provided separately. For simplicity, we try
                // to parse as EC PEM. If that fails, fall back to HMAC for
                // development/testing.
                let encoding = EncodingKey::from_ec_pem(secret.as_bytes())
                    .map_err(|e| warn!("Failed to parse JWT encoding key from G5_JWT_SECRET: {e}"))
                    .ok();

                // The private key PEM cannot be used for DecodingKey — try the
                // separate G5_JWT_PUBLIC_KEY env var first, then fall back to
                // parsing the secret as a public key (which only works if the
                // PEM happens to contain the public key).
                let decoding = auth_config
                    .jwt_public_key
                    .as_ref()
                    .and_then(|pk| {
                        DecodingKey::from_ec_pem(pk.as_bytes())
                            .map_err(|e| {
                                warn!(
                                    "Failed to parse JWT decoding key from G5_JWT_PUBLIC_KEY: {e}"
                                )
                            })
                            .ok()
                    })
                    .or_else(|| DecodingKey::from_ec_pem(secret.as_bytes()).ok());

                if encoding.is_none() {
                    warn!("JWT encoding key is NOT available — token minting will fail");
                }

                (encoding, decoding)
            }
            None => (None, None),
        };

        // Parse encryption key from hex string.
        let encryption_key = auth_config.scm_encryption_key.as_ref().and_then(|hex_key| {
            let bytes = hex::decode(hex_key).ok()?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                Some(key)
            } else {
                warn!("SCM encryption key must be 32 bytes (64 hex chars)");
                None
            }
        });

        // Build OAuth config maps from AuthConfig.
        let mut oauth_configs = HashMap::new();
        let mut scm_oauth_configs = HashMap::new();

        // GitHub login OAuth
        if let Some(ref gh) = auth_config.github {
            let authorize_url = gh
                .authorize_url
                .clone()
                .unwrap_or_else(|| "https://github.com/login/oauth/authorize".to_string());
            let token_url = gh
                .token_url
                .clone()
                .unwrap_or_else(|| "https://github.com/login/oauth/access_token".to_string());
            let userinfo_url = gh
                .userinfo_url
                .clone()
                .unwrap_or_else(|| "https://api.github.com/user".to_string());

            oauth_configs.insert(
                "github".to_string(),
                OAuthProviderConfig {
                    client_id: gh.client_id.clone(),
                    client_secret: gh.client_secret.clone(),
                    authorize_url: authorize_url.clone(),
                    token_url: token_url.clone(),
                    userinfo_url: userinfo_url.clone(),
                    scopes: vec!["user:email".to_string(), "read:user".to_string()],
                    redirect_uri: format!("{}/github", login_redirect_base),
                },
            );
        }

        // Require a concrete directory even when configuration is constructed
        // directly, rather than through CLI normalization.
        let mut entra = None;
        if let (Some(ms), Some(tenant)) = (
            auth_config.microsoft.as_ref(),
            auth_config
                .microsoft_tenant_id
                .as_deref()
                .and_then(|id| Uuid::parse_str(id).ok())
                .filter(|id| !id.is_nil()),
        ) {
            let tenant_id = tenant.to_string();
            entra = Some(crate::auth::entra::EntraClient::new(
                tenant,
                ms.client_id.clone(),
                auth_config.microsoft_jwks_url.clone(),
            ));
            let authorize_url = ms
                .authorize_url
                .clone()
                .unwrap_or_else(|| crate::auth::entra::default_authorize_url(&tenant_id));
            let token_url = ms
                .token_url
                .clone()
                .unwrap_or_else(|| crate::auth::entra::default_token_url(&tenant_id));

            oauth_configs.insert(
                MICROSOFT_PROVIDER.to_string(),
                OAuthProviderConfig {
                    client_id: ms.client_id.clone(),
                    client_secret: ms.client_secret.clone(),
                    authorize_url,
                    token_url,
                    userinfo_url: String::new(),
                    scopes: vec![
                        "openid".to_string(),
                        "email".to_string(),
                        "profile".to_string(),
                    ],
                    redirect_uri: format!("{}/microsoft", login_redirect_base),
                },
            );
        }

        // GitHub SCM connect uses a fine-grained PAT (POST /scm/github/connect
        // -> AuthService::connect_scm_pat), not OAuth. GitHub is intentionally
        // absent from scm_oauth_configs; only GitLab SCM uses OAuth (below).

        if let Some(gitlab) = gitlab_scm_oauth_config(&auth_config, &scm_redirect_base) {
            scm_oauth_configs.insert("gitlab".to_string(), gitlab);
        }
        let scm_tokens = encryption_key.map(|key| {
            crate::auth::ScmTokenManager::new(key, providers.clone())
                .with_origin_policy(crate::auth::configured_scm_origin_policy(&auth_config))
                .with_gitlab_refresh(
                    scm_oauth_configs
                        .get("gitlab")
                        .and_then(connection_refresh_config),
                )
        });

        let access_token_ttl_secs = auth_config.access_token_ttl.as_secs() as i64;
        let refresh_token_ttl_secs = auth_config.refresh_token_ttl.as_secs() as i64;
        let refresh_token_absolute_ttl_secs =
            auth_config.refresh_token_absolute_ttl.as_secs() as i64;
        let gitlab_userinfo_url = auth_config.scm_gitlab_api_url().trim_end_matches('/');
        let scm_gitlab_api_root = gitlab_userinfo_url
            .strip_suffix("/user")
            .unwrap_or(gitlab_userinfo_url)
            .to_string();

        Self {
            db,
            tenant_pool,
            authz,
            providers,
            jwt_encoding_key,
            jwt_decoding_key,
            scm_tokens,
            oauth_configs,
            scm_oauth_configs,
            entra,
            scm_origin_policy: crate::auth::configured_scm_origin_policy(&auth_config),
            org_created_hooks: Vec::new(),
            access_token_ttl_secs,
            refresh_token_ttl_secs,
            refresh_token_absolute_ttl_secs,
            jwt_issuer: auth_config.jwt_issuer.clone(),
            jwt_audience: auth_config.jwt_audience.clone(),
            cookie_secure: auth_config.cookie_secure,
            refresh_cookie_name,
            legacy_refresh_cookie_name,
            scm_github_api_base_url: auth_config.scm_github_api_base_url.clone(),
            scm_gitlab_api_root,
        }
    }

    /// Composition supplies the repository instance independently of login.
    pub(crate) fn with_repository_integration(
        mut self,
        policy: g5_scm::ScmOriginPolicy,
        github_api: String,
    ) -> Self {
        self.scm_tokens = self
            .scm_tokens
            .map(|tokens| tokens.with_origin_policy(policy.clone()));
        self.scm_origin_policy = policy;
        self.scm_github_api_base_url = github_api;
        self
    }

    /// Run `hook` for every organization the E2E test-token path creates
    /// (`OrgService::with_created_hook`).
    #[must_use]
    pub fn with_org_created_hook(mut self, hook: Arc<dyn OrganizationCreated>) -> Self {
        self.org_created_hooks.push(hook);
        self
    }

    /// The GitLab SCM connection OAuth client that renews stored tokens, for
    /// composition of the shared token manager.
    pub(crate) fn gitlab_connection_refresh(
        auth_config: &AuthConfig,
        external_url: &str,
    ) -> Option<g5_scm::providers::ConnectionOAuthConfig> {
        let scm_redirect_base = format!("{}/api/v1/scm", external_url.trim_end_matches('/'));
        gitlab_scm_oauth_config(auth_config, &scm_redirect_base)
            .as_ref()
            .and_then(connection_refresh_config)
    }

    /// Returns the configured refresh token TTL in seconds.
    ///
    /// Used by the HTTP callback handler to set the `Max-Age` on the
    /// httpOnly refresh-token cookie.
    #[must_use]
    pub fn refresh_token_ttl_secs(&self) -> i64 {
        self.refresh_token_ttl_secs
    }

    /// Expiry of a refresh token issued now: the idle lifetime, never past
    /// the absolute one.
    fn new_refresh_expiry(&self) -> chrono::DateTime<Utc> {
        Utc::now()
            + chrono::Duration::seconds(
                self.refresh_token_ttl_secs
                    .min(self.refresh_token_absolute_ttl_secs),
            )
    }

    /// Whether auth cookies should include the `Secure` flag.
    ///
    /// Returns `true` in production (HTTPS).  Set `G5_COOKIE_SECURE=false`
    /// for CI / local development where the edge/client is plain HTTP.
    #[must_use]
    pub fn cookie_secure(&self) -> bool {
        self.cookie_secure
    }

    /// The only accepted refresh-cookie name, host-prefixed in secure deployments.
    #[must_use]
    pub fn refresh_cookie_name(&self) -> &str {
        &self.refresh_cookie_name
    }

    /// A legacy name may be expired, never used to authenticate a secure request.
    #[must_use]
    pub fn legacy_refresh_cookie_name(&self) -> Option<&str> {
        self.legacy_refresh_cookie_name.as_deref()
    }

    /// Issue a fresh user-scoped JWT + refresh token pair for an existing user.
    ///
    /// This powers browser OAuth refresh and approved one-time CLI device
    /// authorization. Worker bootstrap flows must not call this method when
    /// producing downloadable setup material because downloads are not allowed
    /// to contain refresh-capable auth material.
    pub async fn issue_user_tokens(
        &self,
        user_id: Uuid,
        org_id: Option<Uuid>,
        client: &str,
    ) -> Result<AuthTokens, AppError> {
        let target_org_id = org_id.unwrap_or_else(Uuid::nil);

        if !target_org_id.is_nil() {
            let is_member = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM org_memberships
                    WHERE deactivated_at IS NULL AND user_id = $1 AND org_id = $2
                )
                "#,
            )
            .bind(user_id)
            .bind(target_org_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to verify org membership: {e}"),
            })?;

            if !is_member {
                return Err(G5Error::Forbidden {
                    missing_scopes: vec!["org_membership".to_string()],
                }
                .into());
            }
        }

        let scopes = self
            .resolve_effective_scopes(user_id, target_org_id)
            .await?;

        let jwt = self.mint_jwt(
            user_id,
            target_org_id,
            scopes,
            self.access_token_ttl_secs,
            Some(client),
        )?;

        let refresh_token = generate_random_hex(32);
        let refresh_expires_at = self.new_refresh_expiry();
        let org_id_for_db: Option<Uuid> = if target_org_id.is_nil() {
            None
        } else {
            Some(target_org_id)
        };

        let stored = refresh_token_hash(&refresh_token);
        sqlx::query!(
            "INSERT INTO refresh_tokens (token_hash, user_id, org_id, expires_at, client_kind) VALUES ($1, $2, $3, $4, $5)",
            &stored,
            user_id,
            org_id_for_db,
            refresh_expires_at,
            client,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to store refresh token: {e}"),
        })?;

        Ok(AuthTokens {
            jwt,
            refresh_token,
            expires_in: self.access_token_ttl_secs,
            user_id,
        })
    }

    /// Issue a short-lived access JWT without storing a refresh token.
    ///
    /// Server-side smart-HTTP clone helpers use this only for scoped workspace
    /// operations. The token is never written into downloadable material and
    /// does not create a refresh-token row.
    pub async fn issue_user_access_jwt(
        &self,
        user_id: Uuid,
        org_id: Option<Uuid>,
    ) -> Result<String, AppError> {
        let target_org_id = org_id.unwrap_or_else(Uuid::nil);

        if !target_org_id.is_nil() {
            let is_member = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM org_memberships
                    WHERE deactivated_at IS NULL AND user_id = $1 AND org_id = $2
                )
                "#,
            )
            .bind(user_id)
            .bind(target_org_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to verify org membership: {e}"),
            })?;

            if !is_member {
                return Err(G5Error::Forbidden {
                    missing_scopes: vec!["org_membership".to_string()],
                }
                .into());
            }
        }

        let scopes = self
            .resolve_effective_scopes(user_id, target_org_id)
            .await?;
        self.mint_jwt(
            user_id,
            target_org_id,
            scopes,
            self.access_token_ttl_secs,
            None,
        )
    }

    // -----------------------------------------------------------------------
    // OAuth Login Flow
    // -----------------------------------------------------------------------

    /// Generates an OAuth authorize URL for the given provider.
    ///
    /// The state parameter is cryptographically random and stored server-side
    /// with a 10-minute TTL, and the attempt is bound to the initiating browser
    /// by the returned attempt cookie and its PKCE challenge
    /// (`crate::auth::oauth_attempt`).
    pub async fn initiate_oauth(&self, provider: &str) -> Result<OAuthRedirect, AppError> {
        let config = self
            .oauth_configs
            .get(provider)
            .ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "provider",
                    "error": format!("Unknown OAuth provider: {}", provider)
                }),
            })?;

        // Generate cryptographically random 256-bit state.
        let state = generate_random_hex(32);
        let attempt = OAuthAttempt::new(&state);

        // Store state in database with 10-minute TTL.
        let expires_at = Utc::now() + chrono::Duration::seconds(OAUTH_ATTEMPT_TTL_SECS);
        sqlx::query!(
            r#"
            INSERT INTO oauth_states (state, provider, user_id, expires_at)
            VALUES ($1, $2, NULL, $3)
            ON CONFLICT (state) DO NOTHING
            "#,
            &state,
            provider,
            expires_at,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to store OAuth state: {}", e),
        })?;

        let mut url = authorize_url(config, &state, &attempt);
        if provider == MICROSOFT_PROVIDER {
            // The persisted random state is also the nonce. Its atomic deletion
            // owns replay prevention; there is no second nonce lifecycle.
            url.push_str(&format!("&nonce={}", percent_encode(&state)));
        }

        info!(provider = %provider, "OAuth login initiated");

        Ok(OAuthRedirect {
            url,
            state,
            attempt_cookie: attempt.cookie_value().to_string(),
        })
    }

    /// Exchanges an OAuth authorization code for tokens, upserts the user,
    /// checks for pending org invitations, and mints a JWT.
    ///
    /// Returns [`CallbackOutcome::Authenticated`] on success, or
    /// [`CallbackOutcome::AdmissionDenied`] (not an `Err`) when the identity
    /// has neither a pending invitation nor a membership, so the route can
    /// render a diagnosable denied page.
    ///
    /// `attempt_cookie` is this browser's login attempt cookie. Without the
    /// one issued for `state` the callback is refused before the state is
    /// consumed or the code exchanged (SEC-M35, issue #456).
    pub async fn handle_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        attempt_cookie: Option<&str>,
    ) -> Result<CallbackOutcome, AppError> {
        // Step 0: Only the browser that started this attempt may finish it.
        let verifier = attempt_verifier(attempt_cookie, state).ok_or(G5Error::Unauthorized)?;

        // Step 1: Validate and consume the state parameter.
        let state_row = sqlx::query!(
            r#"
            DELETE FROM oauth_states
            WHERE state = $1 AND expires_at > now()
            RETURNING provider, user_id
            "#,
            state,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to validate OAuth state: {}", e),
        })?;

        let state_row = state_row.ok_or(G5Error::Unauthorized)?;

        if state_row.provider != provider || state_row.user_id.is_some() {
            return Err(G5Error::Unauthorized.into());
        }

        let config = self
            .oauth_configs
            .get(provider)
            .ok_or(G5Error::Unauthorized)?;

        // Step 2: Exchange code for access token.
        let token_response = self
            .exchange_code_for_token(provider, config, code, &verifier)
            .await?;

        // Step 3: Microsoft has identity proof only; its email is never proof.
        let user_info = if provider == MICROSOFT_PROVIDER {
            let identity = self
                .entra
                .as_ref()
                .ok_or(G5Error::Unauthorized)?
                .validate_identity(token_response.id_token.as_deref(), state)
                .await?;
            OAuthUserInfo {
                oauth_id: identity.oauth_id(),
                email: identity.email_hint.unwrap_or_default(),
                verified_emails: Vec::new(),
                display_name: identity.display_name,
                avatar_url: None,
                provider_username: None,
            }
        } else {
            self.fetch_user_info(
                provider,
                &token_response.access_token,
                config,
                GitHubEmailMode::LoginVerifiedSet,
            )
            .await?
        };
        let login_emails = &user_info.verified_emails;

        // Step 3.5: Admission (`g5.user.admission`). A pending org invitation
        // governs *first* admission and an existing org membership governs
        // *re-entry*; there is no deployment allowlist. Organizations exist
        // only through platform operators, so an identity with neither has
        // nothing to use.
        {
            let has_pending_invitation = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM org_invitations
                    WHERE lower(email) = ANY($1::text[])
                      AND status = 'pending'
                      AND expires_at > now()
                )
                "#,
            )
            .bind(login_emails)
            .fetch_one(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                // A database failure must not be silently mapped to "no
                // invitation": that would deny a legitimately invited user on a
                // transient fault. Surface it as an error distinct from a
                // successful no-invitation result.
                message: format!("Failed to check pending org invitation: {e}"),
            })?;

            // Issue #1517: the first successful login consumes the invitation
            // (accepts it + creates the membership row), so without this check
            // an invited member is locked out after exactly one login. Resolve
            // the identity by the same proof set account-linking uses below —
            // the OAuth provider identity or any verified email. This admits the
            // global account; active-organization selection below still excludes
            // retained inactive memberships.
            let has_existing_membership = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM org_memberships m
                    JOIN users u ON u.id = m.user_id
                    WHERE u.oauth_provider IS DISTINCT FROM 'service'
                      AND ((u.oauth_provider = $2 AND u.oauth_id = $3)
                        OR lower(u.email) = ANY($1::text[]))
                )
                "#,
            )
            .bind(login_emails)
            .bind(provider)
            .bind(&user_info.oauth_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                // This query decides re-entry for every existing member, so a
                // database failure mapped to `false` would lock a legitimate
                // member out with a user-visible admission denial. Propagate it
                // so the fault is distinguishable from a genuine no-membership.
                message: format!("Failed to check existing org membership: {e}"),
            })?;

            // Log a masked identity (first initial + full domain) so operators
            // can triage an admission decision without the raw personal
            // address landing in log storage (CWE-532). The exact address is
            // surfaced to the visitor's own browser on the denied page.
            let masked_email = mask_email_for_log(&user_info.email);

            if !has_pending_invitation && !has_existing_membership {
                warn!(
                    email_masked = %masked_email,
                    verified_email_count = login_emails.len(),
                    provider = %provider,
                    "Login refused: no pending invitation or existing membership"
                );
                // Not an error: return the attempted display hint so the
                // callback route can name it on the denied page (issue #1517).
                // A Microsoft hint conveys no proof of email ownership.
                return Ok(CallbackOutcome::AdmissionDenied {
                    email: user_info.email.clone(),
                    provider: provider.to_string(),
                });
            }

            info!(
                email_masked = %masked_email,
                verified_email_count = login_emails.len(),
                provider = %provider,
                admitted_by = if has_existing_membership {
                    "existing_membership"
                } else {
                    "pending_invitation"
                },
                "Login admitted by an existing org membership or pending invitation"
            );
        }

        // Step 4-7: Upsert user and auto-accept invitations in a transaction.
        let mut tx = self.db.begin().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to begin transaction: {}", e),
        })?;

        // Step 5-6: Upsert user.
        //
        // The users table has two unique constraints: (email) and
        // (oauth_provider, oauth_id). GitHub users can prove ownership of
        // multiple verified emails, so account linking must consider the
        // provider identity and every verified email as one proof set.
        // Lock the complete candidate set in a stable order. A competing
        // relink cannot turn a checked row into a service account before update.
        let candidates: Vec<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id, oauth_provider, oauth_id FROM users
             WHERE (oauth_provider = $1 AND oauth_id = $2)
                OR lower(email) = ANY($3::text[])
             ORDER BY id FOR UPDATE",
        )
        .bind(provider)
        .bind(&user_info.oauth_id)
        .bind(login_emails)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to look up account-linking candidates: {e}"),
        })?;
        if candidates
            .iter()
            .any(|(_, provider, _)| provider.as_deref() == Some("service"))
        {
            return Err(G5Error::Unauthorized.into());
        }
        if candidates.len() > 1 {
            return Err(G5Error::Conflict {
                reason: "GitHub verified emails match multiple G5 accounts".to_string(),
            }
            .into());
        }
        let existing_user_id = candidates.first().map(|row| row.0);
        let oauth_user_id = candidates
            .first()
            .filter(|(_, stored_provider, stored_id)| {
                stored_provider.as_deref() == Some(provider)
                    && stored_id.as_deref() == Some(user_info.oauth_id.as_str())
            })
            .map(|row| row.0);
        // Do not permit future admission changes or a concurrent relink to
        // introduce Microsoft onboarding through this shared upsert path.
        if provider == MICROSOFT_PROVIDER && oauth_user_id.is_none() {
            return Ok(CallbackOutcome::AdmissionDenied {
                email: user_info.email.clone(),
                provider: provider.to_string(),
            });
        }

        if provider == MICROSOFT_PROVIDER {
            // Bind admission to the locked account, not a precheck that could
            // have observed this pair on a different row before a relink.
            // Keep one membership through commit; inactive membership still
            // admits the account, while organization access is resolved later.
            let member: Option<Uuid> = sqlx::query_scalar(
                "SELECT user_id FROM org_memberships WHERE user_id = $1
                 ORDER BY org_id LIMIT 1 FOR KEY SHARE",
            )
            .bind(oauth_user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to verify Microsoft account membership: {e}"),
            })?;
            if member.is_none() {
                return Ok(CallbackOutcome::AdmissionDenied {
                    email: user_info.email.clone(),
                    provider: provider.to_string(),
                });
            }
        }

        let user_id: Uuid = if let Some(uid) = existing_user_id {
            // User exists — update OAuth credentials + profile info.
            let should_update_email = provider != MICROSOFT_PROVIDER && oauth_user_id == Some(uid);
            sqlx::query(
                "UPDATE users
                 SET oauth_provider = $1,
                     oauth_id = $2,
                     email = CASE WHEN $6 THEN $3 ELSE email END,
                     display_name = $4,
                     avatar_url = $5
                 WHERE id = $7",
            )
            .bind(provider)
            .bind(&user_info.oauth_id)
            .bind(&user_info.email)
            .bind(&user_info.display_name)
            .bind(user_info.avatar_url.as_deref())
            .bind(should_update_email)
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                if is_unique_violation(&e) {
                    return G5Error::Conflict {
                        reason: "Email already associated with another account".to_string(),
                    };
                }
                G5Error::Internal {
                    message: format!(
                        "Failed to update OAuth credentials for existing user: {}",
                        e
                    ),
                }
            })?;
            uid
        } else {
            // New user — insert with ON CONFLICT for the (oauth_provider, oauth_id) case.
            let row: Uuid = sqlx::query_scalar(
                "INSERT INTO users (id, email, display_name, avatar_url, oauth_provider, oauth_id)
                 VALUES (gen_random_uuid(), $1, $2, $3, $4, $5)
                 ON CONFLICT (oauth_provider, oauth_id) DO UPDATE
                 SET email = EXCLUDED.email,
                     display_name = EXCLUDED.display_name,
                     avatar_url = EXCLUDED.avatar_url
                 RETURNING id",
            )
            .bind(&user_info.email)
            .bind(&user_info.display_name)
            .bind(user_info.avatar_url.as_deref())
            .bind(provider)
            .bind(&user_info.oauth_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                if is_unique_violation(&e) {
                    return G5Error::Conflict {
                        reason: "Email already associated with another account".to_string(),
                    };
                }
                G5Error::Internal {
                    message: format!("Database error during user upsert: {}", e),
                }
            })?;
            row
        };

        // Step 7: Accept pending invitations addressed to a verified email
        // against their current rows (`g5.organization.invitation-authority`):
        // the organization service locks each one and claims it only if it is
        // still pending, so a revoke that committed first grants nothing and
        // a later one finds the row accepted.
        let accepted_invitations =
            OrgService::accept_pending_invitations_on_conn(&mut tx, user_id, login_emails).await?;

        tx.commit().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to commit transaction: {}", e),
        })?;

        if !accepted_invitations.is_empty() {
            info!(
                user_id = %user_id,
                count = accepted_invitations.len(),
                "Auto-accepted pending org invitations"
            );
        }

        // Step 8: Determine active org.
        let org_id_opt = sqlx::query_scalar::<_, Uuid>(
            r#"
                SELECT org_id
                FROM org_memberships
                WHERE deactivated_at IS NULL AND user_id = $1
                ORDER BY created_at DESC
                LIMIT 1
                "#,
        )
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to query org memberships: {}", e),
        })?;

        // org_id is None when the user has no org memberships (first login).
        let org_id = org_id_opt.unwrap_or(Uuid::nil());

        // Step 9: Resolve effective scopes.
        let scopes = self.resolve_effective_scopes(user_id, org_id).await?;

        // Step 10: Mint JWT for a browser session (g5.security.browser-session-actions).
        let jwt = self.mint_jwt(
            user_id,
            org_id,
            scopes,
            self.access_token_ttl_secs,
            Some(SESSION_CLIENT_BROWSER),
        )?;

        // Step 11-12: Generate and store refresh token.
        // org_id is stored as NULL when the user has no org, which is
        // allowed by migration 019 (nullable org_id on refresh_tokens).
        let refresh_token = generate_random_hex(32);
        let refresh_expires_at = self.new_refresh_expiry();

        let stored = refresh_token_hash(&refresh_token);
        sqlx::query!(
            "INSERT INTO refresh_tokens (token_hash, user_id, org_id, expires_at, client_kind) VALUES ($1, $2, $3, $4, $5)",
            &stored,
            user_id,
            org_id_opt as Option<Uuid>,
            refresh_expires_at,
            SESSION_CLIENT_BROWSER,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to store refresh token: {}", e),
        })?;

        info!(user_id = %user_id, provider = %provider, "User logged in via OAuth");
        // auth_service [D28] (issue #1552): /api/v1/auth/* is auth-exempt, so
        // the user-action audit middleware never sees the login — emit the
        // audit event here, at the one site that establishes a session.
        tracing::info!(
            target: "g5_audit",
            event = "user.login",
            user_id = %user_id,
            org_id = %org_id_opt.map(|o| o.to_string()).unwrap_or_default(),
            provider = %provider,
            "User logged in"
        );

        Ok(CallbackOutcome::Authenticated(AuthTokens {
            jwt,
            refresh_token,
            expires_in: self.access_token_ttl_secs,
            user_id,
        }))
    }

    // -----------------------------------------------------------------------
    // Token Refresh
    // -----------------------------------------------------------------------

    /// Validates a refresh token and mints a new JWT.
    ///
    /// Optionally accepts an `org_id` to switch the active org context.
    /// Read-only refresh-token identity resolution (auth_service [D22],
    /// issue G5Labs/G5#1097 piece 2): the validate-only arms of
    /// [`refresh_token`](Self::refresh_token) — expiry-checked token lookup,
    /// org re-resolution (§38.1100) or explicit `requested_org_id` membership
    /// check, and effective-scope resolution — with NO side effects: no JWT
    /// mint, no `expires_at` sliding-window extension. The preview re-auth
    /// endpoint (routes/preview [D18]) runs on a `SameSite=Lax`-attached,
    /// cross-site-triggerable GET navigation and must not mutate auth state;
    /// `refresh_token` shares this helper so org-resolution parity between
    /// the two paths is structural.
    pub async fn resolve_refresh_identity(
        &self,
        refresh_token: &str,
        requested_org_id: Option<Uuid>,
    ) -> Result<RefreshIdentity, AppError> {
        // Look up the refresh token, checking both its idle expiry and its
        // absolute lifetime from issue (issue #459): renewal slides the
        // former but never the latter.
        let (user_id, stored_org_id, client): (Uuid, Option<Uuid>, Option<String>) =
            sqlx::query_as(
                r#"
            SELECT user_id, org_id, client_kind FROM refresh_tokens
            WHERE token_hash = $1
              AND expires_at > now()
              AND created_at > now() - make_interval(secs => $2)
            "#,
            )
            .bind(refresh_token_hash(refresh_token))
            .bind(self.refresh_token_absolute_ttl_secs as f64)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to look up refresh token: {}", e),
            })?
            .ok_or(G5Error::Unauthorized)?;

        // If an org was explicitly requested, verify membership.
        // The stored org_id is nullable since migration 019.
        let target_org_id = if let Some(requested_org_id) = requested_org_id {
            let is_member = sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!"
                FROM org_memberships
                WHERE deactivated_at IS NULL AND user_id = $1 AND org_id = $2
                "#,
                user_id,
                requested_org_id,
            )
            .fetch_one(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to verify org membership: {}", e),
            })?;

            if is_member.count == 0 {
                return Err(G5Error::Forbidden {
                    missing_scopes: vec!["org_membership".to_string()],
                }
                .into());
            }
            requested_org_id
        } else {
            // §38.1100 — Re-resolve org from current memberships instead of
            // blindly trusting the stored refresh token org (which may be stale
            // if the user joined/left orgs since login).
            if let Some(stored) = stored_org_id {
                // Verify the stored org membership still exists
                let still_member = sqlx::query!(
                    r#"SELECT COUNT(*) as "count!" FROM org_memberships WHERE deactivated_at IS NULL AND user_id = $1 AND org_id = $2"#,
                    user_id,
                    stored,
                )
                .fetch_one(&self.db)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to verify stored org membership: {e}"),
                })?;

                if still_member.count > 0 {
                    stored
                } else {
                    // Stored org is stale — pick newest membership
                    sqlx::query_scalar!(
                        r#"SELECT org_id FROM org_memberships WHERE deactivated_at IS NULL AND user_id = $1 ORDER BY created_at DESC LIMIT 1"#,
                        user_id,
                    )
                    .fetch_optional(&self.db)
                    .await
                    .map_err(|e| G5Error::Internal {
                        message: format!("Failed to re-resolve org membership: {e}"),
                    })?
                    .unwrap_or(Uuid::nil())
                }
            } else {
                // No stored org — check if user now has memberships
                sqlx::query_scalar!(
                    r#"SELECT org_id FROM org_memberships WHERE deactivated_at IS NULL AND user_id = $1 ORDER BY created_at DESC LIMIT 1"#,
                    user_id,
                )
                .fetch_optional(&self.db)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve org membership: {e}"),
                })?
                .unwrap_or(Uuid::nil())
            }
        };

        // Resolve effective scopes.
        let scopes = self
            .resolve_effective_scopes(user_id, target_org_id)
            .await?;

        Ok(RefreshIdentity {
            user_id,
            org_id: target_org_id,
            scopes,
            client,
        })
    }

    pub async fn refresh_token(
        &self,
        refresh_token: &str,
        org_id: Option<Uuid>,
    ) -> Result<AuthTokens, AppError> {
        // Steps 1-5: shared read-only identity resolution (auth_service [D22]).
        let identity = self.resolve_refresh_identity(refresh_token, org_id).await?;
        let user_id = identity.user_id;
        let target_org_id = identity.org_id;
        let scopes = identity.scopes;

        // Step 6: Mint new JWT, keeping the session's kind.
        let jwt = self.mint_jwt(
            user_id,
            target_org_id,
            scopes,
            self.access_token_ttl_secs,
            identity.client.as_deref(),
        )?;

        // Step 7: Extend the idle expiry (sliding window), never past the
        // absolute lifetime counted from issue (issue #459). The same token
        // stays in use: refresh does not rotate it.
        let new_expires_at = Utc::now() + chrono::Duration::seconds(self.refresh_token_ttl_secs);
        // Store org_id as NULL if the user still has no org.
        let org_id_for_update = if target_org_id.is_nil() {
            None
        } else {
            Some(target_org_id)
        };
        sqlx::query(
            r#"
            UPDATE refresh_tokens
            SET expires_at = LEAST($2, created_at + make_interval(secs => $4)),
                org_id = $3
            WHERE token_hash = $1
            "#,
        )
        .bind(refresh_token_hash(refresh_token))
        .bind(new_expires_at)
        .bind(org_id_for_update)
        .bind(self.refresh_token_absolute_ttl_secs as f64)
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to update refresh token: {}", e),
        })?;

        info!(user_id = %user_id, "JWT refreshed");
        // auth_service [D28] (issue #1552): session continuation event — a
        // coarse presence heartbeat (one per tab per 50 minutes).
        tracing::info!(
            target: "g5_audit",
            event = "user.session_refresh",
            user_id = %user_id,
            org_id = %org_id_for_update.map(|o| o.to_string()).unwrap_or_default(),
            "User session refreshed"
        );

        Ok(AuthTokens {
            jwt,
            refresh_token: refresh_token.to_string(),
            expires_in: self.access_token_ttl_secs,
            user_id,
        })
    }

    // -----------------------------------------------------------------------
    // CLI Device Authorization
    // -----------------------------------------------------------------------

    /// Starts a short-lived CLI device authorization request for secretless
    /// worker bootstrap. A request may be workspace-scoped or org-scoped.
    pub async fn start_cli_device_authorization(
        &self,
        workspace_id: Option<Uuid>,
        requested_org_id: Option<Uuid>,
        device_name: Option<String>,
    ) -> Result<CliDeviceStart, AppError> {
        let org_id: Uuid = if let Some(workspace_id) = workspace_id {
            let org_id: Uuid = sqlx::query_scalar("SELECT cli_device_workspace_org($1)")
                .bind(workspace_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to resolve CLI auth workspace: {e}"),
                })?
                .flatten()
                .ok_or_else(|| G5Error::NotFound {
                    resource: format!("workspace {workspace_id}"),
                })?;

            if let Some(requested_org_id) = requested_org_id {
                if requested_org_id != org_id {
                    return Err(G5Error::ValidationError {
                        details: serde_json::json!({
                            "orgId": "does not own requested workspaceId",
                            "workspaceId": workspace_id.to_string(),
                        }),
                    }
                    .into());
                }
            }

            org_id
        } else {
            let org_id = requested_org_id.ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({
                    "orgId": "orgId is required when workspaceId is omitted",
                }),
            })?;
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM orgs WHERE id = $1)")
                    .bind(org_id)
                    .fetch_one(&self.db)
                    .await
                    .map_err(|e| G5Error::Internal {
                        message: format!("Failed to validate CLI auth org: {e}"),
                    })?;
            if !exists {
                return Err(G5Error::NotFound {
                    resource: format!("org {org_id}"),
                }
                .into());
            }
            org_id
        };

        let device_code = generate_random_hex(32);
        let device_code_hash = hash_cli_device_code(&device_code);
        let user_code = generate_cli_user_code();
        let expires_in = 600;
        let interval = 2;
        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in);

        sqlx::query(
            r#"
            INSERT INTO cli_device_authorizations (
                device_code_hash, user_code, workspace_id, org_id, device_name, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&device_code_hash)
        .bind(&user_code)
        .bind(workspace_id)
        .bind(org_id)
        .bind(device_name.as_deref())
        .bind(expires_at)
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to create CLI device authorization: {e}"),
        })?;

        Ok(CliDeviceStart {
            device_code,
            user_code,
            expires_in,
            interval,
        })
    }

    /// Approves or denies a pending CLI device authorization request.
    pub async fn approve_cli_device_authorization(
        &self,
        user_id: Uuid,
        user_code: &str,
        approve: bool,
    ) -> Result<CliDeviceApproval, AppError> {
        #[derive(sqlx::FromRow)]
        struct PendingDeviceRow {
            id: Uuid,
            org_id: Uuid,
            workspace_id: Option<Uuid>,
            status: String,
            expires_at: chrono::DateTime<Utc>,
        }

        let normalized_code = normalize_cli_user_code(user_code);
        let row = sqlx::query_as::<_, PendingDeviceRow>(
            r#"
            SELECT id, org_id, workspace_id, status, expires_at
            FROM cli_device_authorizations
            WHERE user_code = $1
            "#,
        )
        .bind(&normalized_code)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to look up CLI device authorization: {e}"),
        })?
        .ok_or(G5Error::Unauthorized)?;

        if row.expires_at <= Utc::now() {
            sqlx::query("UPDATE cli_device_authorizations SET status = 'expired' WHERE id = $1")
                .bind(row.id)
                .execute(&self.db)
                .await
                .ok();
            return Ok(CliDeviceApproval {
                status: "expired".to_string(),
            });
        }

        if row.status != "pending" {
            return Ok(CliDeviceApproval { status: row.status });
        }

        let is_member: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM org_memberships
                WHERE deactivated_at IS NULL AND user_id = $1 AND org_id = $2
            )
            "#,
        )
        .bind(user_id)
        .bind(row.org_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to verify CLI auth org membership: {e}"),
        })?;

        if !is_member {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["org_membership".to_string()],
            }
            .into());
        }

        if let Some(workspace_id) = row.workspace_id {
            let capability = self
                .authz
                .mint_db_capability(
                    UserId::from_uuid(user_id),
                    &[OrgId::from_uuid(row.org_id)],
                    &["*".to_string()],
                )
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to mint CLI approval DB capability: {e}"),
                })?;

            let mut scoped_db = ScopedDb::from_capability(&self.tenant_pool, capability)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to initialize scoped CLI approval lookup: {e}"),
                })?;

            crate::db::scoped::ensure_user_workspace_access(
                &self.db,
                &mut scoped_db,
                user_id,
                row.org_id,
                workspace_id,
            )
            .await?;

            scoped_db.rollback().await.map_err(|e| G5Error::Internal {
                message: format!("Failed to clean up scoped CLI approval lookup: {e}"),
            })?;
        }

        let next_status = if approve { "approved" } else { "denied" };
        sqlx::query(
            r#"
            UPDATE cli_device_authorizations
            SET status = $2, user_id = $3, approved_at = CASE WHEN $2 = 'approved' THEN now() ELSE approved_at END
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(row.id)
        .bind(next_status)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to approve CLI device authorization: {e}"),
        })?;

        Ok(CliDeviceApproval {
            status: next_status.to_string(),
        })
    }

    /// Polls a CLI device authorization request and mints tokens after approval.
    pub async fn poll_cli_device_authorization(
        &self,
        device_code: &str,
    ) -> Result<CliDevicePoll, AppError> {
        #[derive(sqlx::FromRow)]
        struct DeviceRow {
            id: Uuid,
            org_id: Uuid,
            workspace_id: Option<Uuid>,
            user_id: Option<Uuid>,
            status: String,
            expires_at: chrono::DateTime<Utc>,
        }

        let device_code_hash = hash_cli_device_code(device_code);
        let row = sqlx::query_as::<_, DeviceRow>(
            r#"
            SELECT id, org_id, workspace_id, user_id, status, expires_at
            FROM cli_device_authorizations
            WHERE device_code_hash = $1
            "#,
        )
        .bind(&device_code_hash)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to poll CLI device authorization: {e}"),
        })?
        .ok_or(G5Error::Unauthorized)?;

        if row.expires_at <= Utc::now() && row.status != "consumed" {
            sqlx::query(
                "UPDATE cli_device_authorizations SET status = 'expired' WHERE id = $1 AND status != 'consumed'",
            )
            .bind(row.id)
            .execute(&self.db)
            .await
            .ok();
            return Ok(CliDevicePoll {
                status: "expired".to_string(),
                tokens: None,
                org_id: Some(row.org_id),
                workspace_id: row.workspace_id,
            });
        }

        match row.status.as_str() {
            "pending" | "denied" | "expired" | "consumed" => Ok(CliDevicePoll {
                status: row.status,
                tokens: None,
                org_id: Some(row.org_id),
                workspace_id: row.workspace_id,
            }),
            "approved" => {
                let user_id = row.user_id.ok_or_else(|| G5Error::Internal {
                    message: "Approved CLI device authorization is missing user_id".to_string(),
                })?;

                let result = sqlx::query(
                    r#"
                    UPDATE cli_device_authorizations
                    SET status = 'consumed', consumed_at = now()
                    WHERE id = $1 AND status = 'approved' AND consumed_at IS NULL
                    "#,
                )
                .bind(row.id)
                .execute(&self.db)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to consume CLI device authorization: {e}"),
                })?;

                if result.rows_affected() != 1 {
                    return Ok(CliDevicePoll {
                        status: "consumed".to_string(),
                        tokens: None,
                        org_id: Some(row.org_id),
                        workspace_id: row.workspace_id,
                    });
                }

                let tokens = self
                    .issue_user_tokens(user_id, Some(row.org_id), SESSION_CLIENT_CLI)
                    .await?;
                Ok(CliDevicePoll {
                    status: "approved".to_string(),
                    tokens: Some(tokens),
                    org_id: Some(row.org_id),
                    workspace_id: row.workspace_id,
                })
            }
            _ => Err(G5Error::Internal {
                message: format!("Unknown CLI device authorization status: {}", row.status),
            }
            .into()),
        }
    }

    // -----------------------------------------------------------------------
    // Logout
    // -----------------------------------------------------------------------

    /// Revokes a refresh token by deleting it from storage.
    ///
    /// Idempotent — succeeds even if the token doesn't exist.
    pub async fn logout(&self, refresh_token: &str) -> Result<(), AppError> {
        let stored = refresh_token_hash(refresh_token);
        sqlx::query!(
            r#"DELETE FROM refresh_tokens WHERE token_hash = $1"#,
            &stored,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to delete refresh token: {}", e),
        })?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // E2E Test Token (requires G5_E2E_AUTH=true)
    // -----------------------------------------------------------------------

    /// Issues a JWT for E2E testing by upserting a test user with the given
    /// email. This bypasses the full OAuth flow and is **only** available when
    /// the `G5_E2E_AUTH=true` environment variable is set.
    ///
    /// Route registration and this service both require the explicit opt-in.
    /// The service checks it before any database access or provisioning.
    ///
    /// Returns the minted tokens along with Base docs+code materialization
    /// targets for orgs created by this invocation. This service layer has no
    /// repository-storage handle (auth_service [D3]), so it does NOT run the
    /// repositories' git bootstrap; the route layer completes materialization
    /// post-commit via the shared docs+code bootstrap helpers. Existing
    /// orgs never receive a target, preventing token reissuance from repairing
    /// their Base repositories or authority rows.
    pub async fn issue_test_token(&self, email: &str) -> Result<TestTokenProvisioning, AppError> {
        if !g5_contracts::env::is_env_flag_enabled("G5_E2E_AUTH") {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["E2E test token issuance is disabled".to_string()],
            }
            .into());
        }

        // Upsert a test user — use "e2e" as the OAuth provider and the email
        // as the OAuth ID so we get deterministic, conflict-free records.
        //
        // Edge case: the same email may already exist via a different OAuth
        // provider (e.g. "github") if the OAuth stub login ran first.  We
        // handle both unique constraints (email *and* oauth_provider+oauth_id)
        // by first looking up by email, falling back to the INSERT…ON CONFLICT.
        let existing_id: Option<Uuid> =
            sqlx::query_scalar!(r#"SELECT id FROM users WHERE email = $1"#, email)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to look up E2E test user: {}", e),
                })?;

        let user_id: Uuid = if let Some(id) = existing_id {
            id
        } else {
            sqlx::query_scalar!(
                r#"
                INSERT INTO users (id, email, display_name, avatar_url, oauth_provider, oauth_id)
                VALUES (gen_random_uuid(), $1, $2, NULL, 'e2e', $3)
                ON CONFLICT (oauth_provider, oauth_id) DO UPDATE
                SET email = EXCLUDED.email,
                    display_name = EXCLUDED.display_name
                RETURNING id
                "#,
                email,
                format!("E2E Test ({})", email),
                email,
            )
            .fetch_one(&self.db)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to upsert E2E test user: {}", e),
            })?
        };

        // Provision the two membership orgs (g5labs, g5labs-member) through the
        // SHARED OrgService::create_org cascade so each Base workspace is born
        // docs+code — exactly like a production org. This replaces the old
        // parallel raw-SQL base provisioning that produced a docs-only Base and
        // broke the docs+code invariant for `just dev-up` (G5_E2E_AUTH). The
        // negative g5labs-unassigned tenant has only its dedicated fixture
        // administrator, never a normal persona or workspace. Even negative
        // fixtures preserve administrator continuity.
        sqlx::query(
            r#"WITH admin_user AS (
                 INSERT INTO users (email, display_name)
                 VALUES ('e2e-unassigned-admin@g5.invalid', 'Unassigned fixture administrator')
                 ON CONFLICT (email) DO UPDATE SET display_name = EXCLUDED.display_name
                 RETURNING id
             ), tenant_org AS (
                 INSERT INTO orgs (id, name, slug)
                 VALUES (gen_random_uuid(), 'G5 Labs Unassigned', 'g5labs-unassigned')
                 ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
                 RETURNING id
             )
             INSERT INTO org_memberships (org_id, user_id, role)
             SELECT tenant_org.id, admin_user.id, 'admin' FROM tenant_org, admin_user
             ON CONFLICT (org_id, user_id) DO NOTHING"#,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to upsert unassigned E2E org: {}", e),
        })?;

        let org_service = OrgService::new().with_created_hooks(self.org_created_hooks.clone());
        let mut membership_org_ids: Vec<Uuid> = Vec::new();
        // Capture bootstrap targets only from this invocation's successful
        // create_org cascades. Existing or race-won fixture orgs must not be
        // inspected or repaired during token reissuance.
        let mut base_repo_bootstraps: Vec<TestTokenBaseRepoBootstrap> = Vec::new();
        for (org_name, org_slug) in [("G5 Labs", "g5labs"), ("G5 Labs Member", "g5labs-member")] {
            // Idempotent: run the create_org cascade only when the org does not
            // exist yet (E2E tests mint per-test users repeatedly and in
            // parallel). A concurrent creator surfaces as a slug Conflict, which
            // we resolve by re-fetching.
            let existing_org: Option<Uuid> =
                sqlx::query_scalar::<_, Uuid>("SELECT id FROM orgs WHERE slug = $1")
                    .bind(org_slug)
                    .fetch_optional(&self.db)
                    .await
                    .map_err(|e| G5Error::Internal {
                        message: format!("Failed to look up E2E org '{}': {}", org_slug, e),
                    })?;

            let (seeded_org_id, needs_admin_membership) = if let Some(id) = existing_org {
                (id, true)
            } else {
                // The full durable cascade uses the system-pool transaction;
                // create_org activates bounded RLS context on this same
                // connection after staging owner membership.
                let mut create_db = ScopedDb::unscoped(
                    &self.db,
                    DbCapability {
                        org_ids: vec![],
                        actor_id: UserId::from_uuid(user_id),
                        scopes: vec![],
                        session_id: SessionId::new(),
                        ttl_secs: 3600,
                    },
                )
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to open E2E create_org session: {}", e),
                })?;

                match org_service
                    .create_org(
                        self.authz.as_ref(),
                        &mut create_db,
                        user_id,
                        CreateOrgRequest {
                            name: org_name.to_string(),
                            slug: Some(org_slug.to_string()),
                            kind: Default::default(),
                        },
                    )
                    .await
                {
                    Ok(resp) => {
                        create_db.commit().await.map_err(|e| G5Error::Internal {
                            message: format!("Failed to commit E2E create_org: {}", e),
                        })?;
                        // Every E2E persona shares these orgs and owns what it
                        // creates, so the suite would exhaust the per-owner
                        // limits (`g5.organization.owner-limits`) long before
                        // the organization quota it already runs under. Lift
                        // them once, at creation; a later Admin change stays.
                        for (key, value) in [
                            (
                                crate::services::org_admin_settings::MAX_RUNNING_VMS_PER_OWNER_KEY,
                                100,
                            ),
                            (
                                crate::services::org_admin_settings::MAX_WORKSPACES_PER_OWNER_KEY,
                                1000,
                            ),
                        ] {
                            sqlx::query(
                                "INSERT INTO configs (org_id, scope, scope_id, key, value)
                                 VALUES ($1, 'org', $1, $2, to_jsonb($3::int))
                                 ON CONFLICT (org_id, scope, scope_id, key) DO NOTHING",
                            )
                            .bind(resp.org.id)
                            .bind(key)
                            .bind(value)
                            .execute(&self.db)
                            .await
                            .map_err(|e| G5Error::Internal {
                                message: format!("Failed to lift E2E owner limits: {}", e),
                            })?;
                        }
                        base_repo_bootstraps.push(TestTokenBaseRepoBootstrap {
                            org_id: resp.org.id,
                            base_workspace_id: resp.base_workspace.id,
                            docs_repo_id: resp.docs_repo.id,
                            code_repo_id: resp.code_repo.id,
                            code_repo_name: resp.code_repo.name,
                        });
                        // create_org already staged this user's Admin membership in
                        // the committed transaction, so no post-commit upsert is
                        // necessary on the system pool.
                        (resp.org.id, false)
                    }
                    Err(G5Error::Conflict { .. }) => {
                        // Lost the create race to a parallel token issuance.
                        create_db.rollback().await.map_err(|e| G5Error::Internal {
                            message: format!(
                                "Failed to roll back raced E2E create_org '{}': {}",
                                org_slug, e
                            ),
                        })?;
                        (
                            sqlx::query_scalar::<_, Uuid>("SELECT id FROM orgs WHERE slug = $1")
                                .bind(org_slug)
                                .fetch_one(&self.db)
                                .await
                                .map_err(|e| G5Error::Internal {
                                    message: format!(
                                        "Failed to resolve raced E2E org '{}': {}",
                                        org_slug, e
                                    ),
                                })?,
                            true,
                        )
                    }
                    Err(e) => return Err(e.into()),
                }
            };

            if needs_admin_membership {
                // Parallel users share existing fixture orgs. A freshly created
                // org already has this user as Admin inside the committed cascade.
                // Only a missing membership defaults to Admin; an existing one
                // keeps its role so a dev persona seeded as member (or a test's
                // deliberate role change) survives token reissuance.
                sqlx::query!(
                    "INSERT INTO org_memberships (org_id, user_id, role) \
                     VALUES ($1, $2, 'admin') \
                     ON CONFLICT (org_id, user_id) DO NOTHING",
                    seeded_org_id,
                    user_id,
                )
                .execute(&self.db)
                .await
                .map_err(|e| G5Error::Internal {
                    message: format!("Failed to upsert E2E org membership: {}", e),
                })?;
            }

            membership_org_ids.push(seeded_org_id);
        }

        let org_id = membership_org_ids[0];
        // Only orgs created by this invocation need fixture forks. Their Base
        // repositories are materialized by the route before it runs these
        // targets. Existing fixture orgs are intentionally never repaired by
        // token reissuance.
        let fixture_fork_targets = base_repo_bootstraps
            .iter()
            .map(|target| TestTokenFixtureForkTarget {
                org_id: target.org_id,
                base_workspace_id: target.base_workspace_id,
            })
            .collect();

        let scopes = self.resolve_effective_scopes(user_id, org_id).await?;

        // Mint JWT. E2E sessions stand in for the browser's.
        let jwt = self.mint_jwt(
            user_id,
            org_id,
            scopes,
            self.access_token_ttl_secs,
            Some(SESSION_CLIENT_BROWSER),
        )?;

        // Create refresh token.  Store NULL for org_id when the user has no
        // org (org_id is nil).  Migration 019 made the column nullable.
        let refresh_token = generate_random_hex(32);
        let refresh_expires_at = self.new_refresh_expiry();

        let org_id_for_db: Option<Uuid> = if org_id.is_nil() { None } else { Some(org_id) };
        let stored = refresh_token_hash(&refresh_token);
        sqlx::query!(
            "INSERT INTO refresh_tokens (token_hash, user_id, org_id, expires_at, client_kind) VALUES ($1, $2, $3, $4, $5)",
            &stored,
            user_id,
            org_id_for_db,
            refresh_expires_at,
            SESSION_CLIENT_BROWSER,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to store E2E refresh token: {}", e),
        })?;

        info!(user_id = %user_id, email = %email, "E2E test token issued");

        Ok(TestTokenProvisioning {
            tokens: AuthTokens {
                jwt,
                refresh_token,
                expires_in: self.access_token_ttl_secs,
                user_id,
            },
            base_repo_bootstraps,
            fixture_fork_targets,
        })
    }

    // -----------------------------------------------------------------------
    // SCM Connection Flow
    // -----------------------------------------------------------------------

    /// Generates an SCM OAuth authorize URL with broader repo-access scopes,
    /// bound to the initiating browser like a login attempt.
    pub async fn initiate_scm_connect(
        &self,
        provider: &str,
        user_id: Uuid,
        return_to: Option<&str>,
    ) -> Result<OAuthRedirect, AppError> {
        let config =
            self.scm_oauth_configs
                .get(provider)
                .ok_or_else(|| G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "provider",
                        "error": format!("Unknown SCM provider: {}", provider)
                    }),
                })?;

        let state = build_scm_oauth_state(return_to);
        let attempt = OAuthAttempt::new(&state);
        let expires_at = Utc::now() + chrono::Duration::seconds(OAUTH_ATTEMPT_TTL_SECS);

        // Store state with user_id association.
        sqlx::query!(
            r#"
            INSERT INTO oauth_states (state, provider, user_id, expires_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (state) DO NOTHING
            "#,
            &state,
            provider,
            user_id,
            expires_at,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to store OAuth state: {}", e),
        })?;

        let url = authorize_url(config, &state, &attempt);

        info!(user_id = %user_id, provider = %provider, "SCM connection initiated");

        Ok(OAuthRedirect {
            url,
            state,
            attempt_cookie: attempt.cookie_value().to_string(),
        })
    }

    /// Exchanges the SCM OAuth code for tokens, encrypts them, and upserts
    /// into `user_scm_connections`.
    ///
    /// The state row names the G5 user who started the attempt; only that
    /// user's browser, presenting the attempt cookie issued for `state`, may
    /// store a provider credential under it. Anything else is refused before
    /// the state is consumed or the code exchanged (SEC-M35, issue #449).
    pub async fn handle_scm_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        attempt_cookie: Option<&str>,
    ) -> Result<ScmConnectionResult, AppError> {
        let verifier = attempt_verifier(attempt_cookie, state).ok_or(G5Error::Unauthorized)?;

        // Validate state and extract user_id.
        let state_row = sqlx::query!(
            r#"
            DELETE FROM oauth_states
            WHERE state = $1 AND expires_at > now()
            RETURNING provider, user_id
            "#,
            state,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to validate OAuth state: {}", e),
        })?;

        let state_row = state_row.ok_or(G5Error::Unauthorized)?;
        let user_id = state_row.user_id.ok_or(G5Error::Unauthorized)?;

        if state_row.provider != provider {
            return Err(G5Error::Unauthorized.into());
        }

        let config = self
            .scm_oauth_configs
            .get(provider)
            .ok_or(G5Error::Unauthorized)?;

        // Exchange code for tokens.
        let token_response = self
            .connection_protocol(provider, config)?
            .exchange_code(code, Some(&verifier))
            .await
            .map_err(G5Error::from)?;

        // Fetch provider user info.
        let user_info = self
            .fetch_user_info(
                provider,
                &token_response.access_token,
                config,
                GitHubEmailMode::SkipEmailLookup,
            )
            .await?;

        // Encrypt the access token.
        let access_token_enc = self.encrypt_token(&token_response.access_token)?;

        // Encrypt refresh token if present.
        let refresh_token_enc = match &token_response.refresh_token {
            Some(rt) => Some(self.encrypt_token(rt)?),
            None => None,
        };

        // Calculate token expiry.
        let token_expires_at = token_response
            .expires_in
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

        let scopes_vec: Vec<String> = config.scopes.clone();
        let connected_at = Utc::now();

        // UPSERT into user_scm_connections.
        sqlx::query!(
            r#"
            INSERT INTO user_scm_connections
                (id, user_id, provider, provider_user_id, provider_username,
                 access_token_enc, refresh_token_enc, token_expires_at, scopes)
            VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (user_id, provider) DO UPDATE SET
                access_token_enc = EXCLUDED.access_token_enc,
                refresh_token_enc = EXCLUDED.refresh_token_enc,
                token_expires_at = EXCLUDED.token_expires_at,
                scopes = EXCLUDED.scopes,
                provider_user_id = EXCLUDED.provider_user_id,
                provider_username = EXCLUDED.provider_username,
                updated_at = now()
            "#,
            user_id,
            provider,
            user_info.oauth_id,
            user_info.provider_username.as_deref(),
            &access_token_enc,
            refresh_token_enc.as_deref(),
            token_expires_at,
            &scopes_vec,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to upsert SCM connection: {}", e),
        })?;

        info!(
            user_id = %user_id,
            provider = %provider,
            "SCM provider connected"
        );

        Ok(ScmConnectionResult {
            provider: provider.to_string(),
            provider_username: user_info
                .provider_username
                .unwrap_or(user_info.display_name),
            scopes: scopes_vec,
            connected_at: connected_at.to_rfc3339(),
            return_to: scm_return_to_from_state(state),
        })
    }

    /// Connects a GitHub repo-access credential from a user-supplied
    /// fine-grained Personal Access Token: validates the token against the
    /// configured GitHub API base URL, then upserts an encrypted
    /// `auth_method='pat'` row into `user_scm_connections`.
    ///
    /// Replaces the SCM OAuth App code-exchange for GitHub (GitLab keeps the
    /// OAuth flow). See `g5/g5-api/auth` D12 / `auth/scm` D8 and
    /// `designs/API_AND_STORAGE.md` §3.11.2.
    ///
    /// # Errors
    ///
    /// - `ValidationError` (`unsupported_provider`) if `provider != "github"`.
    /// - `Unauthorized` if the token is rejected (mapped by the route to
    ///   `invalid_token`).
    /// - `Internal` if no encryption key is configured.
    ///
    /// # Security
    ///
    /// - The `token` is never logged.
    pub async fn connect_scm_pat(
        &self,
        user_id: Uuid,
        provider: &str,
        token: &str,
    ) -> Result<ScmConnectionResult, AppError> {
        if provider != "github" {
            return Err(AppError::from_error(G5Error::ValidationError {
                details: serde_json::json!({
                    "code": "unsupported_provider",
                    "message": format!(
                        "PAT connect is only supported for GitHub, not '{provider}'"
                    ),
                }),
            }));
        }

        // Validate the PAT against the configured GitHub API base URL.
        let manager = self.scm_tokens()?;
        let identity = match manager
            .validate_pat(provider, &self.scm_github_api_base_url, token)
            .await
        {
            Ok(identity) => identity,
            // Map a token rejection (401) to a 400-class validation error. A raw
            // 401 would make the frontend api client treat it as an expired JWT
            // and trigger a session refresh / logout. (g5/g5-api/auth/scm D8.)
            Err(e) if matches!(e.inner, G5Error::Unauthorized) => {
                return Err(AppError::from_error(G5Error::ValidationError {
                    details: serde_json::json!({
                        "code": "invalid_token",
                        "field": "token",
                        "message": "GitHub rejected the token. Provide a valid, \
                                    unexpired fine-grained PAT with Contents: \
                                    Read and write on the target repositories.",
                    }),
                }));
            }
            Err(e) => return Err(e),
        };

        let provider_user_id = identity.provider_user_id;
        let login = identity.login;
        let expires_at = identity.expires_at;

        // Encrypt and upsert. PATs carry no refresh token and an empty scope
        // set; `auth_method='pat'` keeps the GitLab-only refresh loop away.
        let access_token_enc = self.encrypt_token(token)?;
        let connected_at = Utc::now();
        let empty_scopes: Vec<String> = Vec::new();

        sqlx::query!(
            r#"
            INSERT INTO user_scm_connections
                (id, user_id, provider, provider_user_id, provider_username,
                 access_token_enc, refresh_token_enc, token_expires_at, scopes, auth_method)
            VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, NULL, $6, $7, 'pat')
            ON CONFLICT (user_id, provider) DO UPDATE SET
                access_token_enc = EXCLUDED.access_token_enc,
                refresh_token_enc = NULL,
                token_expires_at = EXCLUDED.token_expires_at,
                scopes = EXCLUDED.scopes,
                provider_user_id = EXCLUDED.provider_user_id,
                provider_username = EXCLUDED.provider_username,
                auth_method = 'pat',
                updated_at = now()
            "#,
            user_id,
            provider,
            provider_user_id,
            Some(login.as_str()),
            &access_token_enc,
            expires_at,
            &empty_scopes,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to upsert SCM connection: {}", e),
        })?;

        info!(
            user_id = %user_id,
            provider = %provider,
            login = %login,
            "SCM provider connected via PAT"
        );

        Ok(ScmConnectionResult {
            provider: provider.to_string(),
            provider_username: login,
            scopes: Vec::new(),
            connected_at: connected_at.to_rfc3339(),
            return_to: String::new(),
        })
    }

    /// Deletes the SCM connection for the given user and provider.
    pub async fn disconnect_scm(&self, user_id: Uuid, provider: &str) -> Result<(), AppError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM user_scm_connections
            WHERE user_id = $1 AND provider = $2
            "#,
            user_id,
            provider,
        )
        .execute(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to delete SCM connection: {}", e),
        })?;

        if result.rows_affected() == 0 {
            return Err(G5Error::NotFound {
                resource: format!("SCM connection for provider {}", provider),
            }
            .into());
        }

        info!(user_id = %user_id, provider = %provider, "SCM provider disconnected");

        Ok(())
    }

    // -----------------------------------------------------------------------
    // SCM Repo Operations
    // -----------------------------------------------------------------------

    /// Lists repositories for a person session with current target-org read access.
    pub async fn list_scm_repos(
        &self,
        auth: &AuthContext,
        provider: &str,
        org_id: Uuid,
        cursor: Option<String>,
    ) -> Result<ScmRepoListResponse, AppError> {
        let user_id = extract_session_user_id(auth)?;
        // Authorize before using the account credential, then release the
        // transaction before provider I/O. Recheck when reading import status.
        let db = self
            .scoped_repo_db_for_org(auth, org_id, &["workspaces:read"])
            .await?;
        db.commit().await.map_err(AppError::from)?;
        let access_token = self.get_scm_token(user_id, provider).await?;

        self.list_account_repos(auth, provider, &access_token, org_id, cursor)
            .await
    }

    async fn scoped_repo_db_for_org(
        &self,
        auth: &AuthContext,
        org_id: Uuid,
        scopes: &[&str],
    ) -> Result<ScopedDb, AppError> {
        let capability = self
            .authz
            .mint_db_capability(
                UserId::from_uuid(extract_session_user_id(auth)?),
                &[OrgId::from_uuid(org_id)],
                &scopes
                    .iter()
                    .map(|scope| (*scope).to_string())
                    .collect::<Vec<_>>(),
            )
            .await
            .map_err(AppError::from)?;
        let mut db = ScopedDb::from_capability(&self.tenant_pool, capability)
            .await
            .map_err(AppError::from)?;
        authorize_org_scopes(&mut db, auth, org_id, None, scopes).await?;
        Ok(db)
    }

    /// Creates `repos` rows in the caller's authorized transaction. Membership
    /// and capability scope labels alone do not grant write authority.
    pub async fn import_repos(
        &self,
        db: &mut ScopedDb,
        provider: &str,
        org_id: Uuid,
        repos: Vec<ImportRepoSpec>,
    ) -> Result<Vec<RepoDTO>, AppError> {
        let user_id = db.actor_id().into_uuid();
        crate::services::PermissionService::require_user_scopes_on_conn(
            db.conn(),
            user_id,
            org_id,
            None,
            &["workspaces:write"],
        )
        .await
        .map_err(AppError::from)?;

        // Validate the whole batch before idempotent lookup or any insert. Never
        // trust clone URLs simply because the caller labels them with a provider.
        for repo in &repos {
            if self
                .scm_origin_policy
                .effective_provider(Some(provider), &repo.clone_url)
                .is_none()
            {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "code": "SCM_CLONE_URL_INVALID",
                        "message": "Clone URL must use the provider's configured HTTPS origin without userinfo."
                    }),
                }.into());
            }
        }

        let mut imported = Vec::new();
        for repo_spec in &repos {
            let existing = sqlx::query_as::<_, ExistingProviderRepoRow>(
                r#"
                SELECT id, org_id, name, slug, git_url, default_branch,
                       provider, provider_repo_id, provider_full_name, created_at
                FROM repos
                WHERE org_id = $1 AND provider = $2 AND provider_repo_id = $3
                "#,
            )
            .bind(org_id)
            .bind(provider)
            .bind(&repo_spec.provider_repo_id)
            .fetch_all(db.conn())
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to check existing repo: {}", e),
            })?;

            if let Some(row) = existing.first() {
                imported.push(row.clone().into());
                continue;
            }

            // Derive slug from full_name.
            let slug = repo_spec.full_name.replace('/', "-").to_lowercase();
            let name = repo_spec
                .full_name
                .rsplit('/')
                .next()
                .unwrap_or(&repo_spec.full_name)
                .to_string();

            let row = match sqlx::query_as::<_, ExistingProviderRepoRow>(
                r#"
                INSERT INTO repos
                    (id, org_id, name, slug, git_url, default_branch,
                     provider, provider_repo_id, provider_full_name)
                VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING id, org_id, name, slug, git_url, default_branch,
                          provider, provider_repo_id, provider_full_name, created_at
                "#,
            )
            .bind(org_id)
            .bind(&name)
            .bind(&slug)
            .bind(&repo_spec.clone_url)
            .bind(&repo_spec.default_branch)
            .bind(provider)
            .bind(&repo_spec.provider_repo_id)
            .bind(&repo_spec.full_name)
            .fetch_one(db.conn())
            .await
            {
                Ok(row) => row,
                Err(e) if is_unique_violation(&e) => {
                    let existing = sqlx::query_as::<_, ExistingProviderRepoRow>(
                        r#"
                        SELECT id, org_id, name, slug, git_url, default_branch,
                               provider, provider_repo_id, provider_full_name, created_at
                        FROM repos
                        WHERE org_id = $1 AND provider = $2 AND provider_repo_id = $3
                        "#,
                    )
                    .bind(org_id)
                    .bind(provider)
                    .bind(&repo_spec.provider_repo_id)
                    .fetch_all(db.conn())
                    .await
                    .map_err(|db_err| G5Error::Internal {
                        message: format!(
                            "Failed to resolve repo after insert conflict: {}",
                            db_err
                        ),
                    })?;

                    if let Some(row) = existing.into_iter().next() {
                        row
                    } else {
                        return Err(G5Error::Conflict {
                            reason: "Repository import conflicted with another repo in this org"
                                .to_string(),
                        }
                        .into());
                    }
                }
                Err(e) => {
                    return Err(G5Error::Internal {
                        message: format!("Failed to insert repo: {}", e),
                    }
                    .into())
                }
            };

            imported.push(row.into());
        }

        info!(
            user_id = %user_id,
            provider = %provider,
            count = imported.len(),
            "Repos imported"
        );

        Ok(imported)
    }

    // -----------------------------------------------------------------------
    // SCM Token Management
    // -----------------------------------------------------------------------

    /// The user's SCM access token through the shared stored-token read path
    /// (expired GitLab tokens are renewed there; other expired tokens refuse).
    pub async fn get_scm_token(&self, user_id: Uuid, provider: &str) -> Result<String, AppError> {
        self.scm_tokens()?
            .access_token(&self.db, user_id, provider)
            .await
            .map_err(|error| match error {
                crate::auth::TokenResolutionError::NoScmConnection { .. } => G5Error::NotFound {
                    resource: format!("SCM connection for provider {}", provider),
                }
                .into(),
                error => AppError::from(error),
            })
    }

    // -----------------------------------------------------------------------
    // WebSocket Ticket
    // -----------------------------------------------------------------------

    /// Mints a short-lived (5-minute) JWT for WebSocket upgrade authentication.
    pub async fn mint_ws_ticket(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<String, AppError> {
        let org_ids = self
            .authz
            .user_org_ids(UserId::from_uuid(user_id))
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to resolve user org memberships: {}", e),
            })?;

        if org_ids.is_empty() {
            return Err(G5Error::Forbidden {
                missing_scopes: vec!["workspace_access".to_string()],
            }
            .into());
        }

        let capability = self
            .authz
            .mint_db_capability(UserId::from_uuid(user_id), &org_ids, &["*".to_string()])
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to mint WebSocket DB capability: {}", e),
            })?;

        let mut scoped_db = ScopedDb::from_capability(&self.tenant_pool, capability)
            .await
            .map_err(|e| G5Error::Internal {
                message: format!("Failed to initialize scoped WebSocket lookup: {}", e),
            })?;

        let workspace_org_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT org_id
            FROM workspaces
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(workspace_id)
        .fetch_optional(scoped_db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to fetch workspace through scoped DB: {}", e),
        })?;

        let org_id = workspace_org_id.ok_or_else(|| G5Error::NotFound {
            resource: format!("workspace {}", workspace_id),
        })?;

        crate::db::scoped::ensure_user_workspace_access(
            &self.db,
            &mut scoped_db,
            user_id,
            org_id,
            workspace_id,
        )
        .await?;

        scoped_db.rollback().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to clean up scoped WebSocket lookup: {}", e),
        })?;

        let scopes = self.resolve_effective_scopes(user_id, org_id).await?;

        let ticket = self.mint_ws_jwt(user_id, org_id, scopes, Some(workspace_id))?;

        info!(
            user_id = %user_id,
            workspace_id = %workspace_id,
            "WebSocket ticket minted"
        );

        Ok(ticket)
    }

    /// Mints a short-lived JWT for WebSocket upgrade without workspace context.
    ///
    /// Used when the client needs a WS ticket before a workspace is selected.
    /// The ticket carries the caller's JWT scopes and a 5-minute lifetime.
    pub async fn mint_ws_ticket_general(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        scopes: Vec<Scope>,
    ) -> Result<String, AppError> {
        let ticket = self.mint_ws_jwt(user_id, org_id, scopes, None)?;
        info!(user_id = %user_id, "WebSocket ticket (general) minted");
        Ok(ticket)
    }

    // -----------------------------------------------------------------------
    // Preview Ticket
    // -----------------------------------------------------------------------

    /// Mints a short-lived, workspace-bound preview ticket JWT carried by the
    /// `g5_preview` cookie (issue G5Labs/G5#736; `routes/preview` [D8]).
    ///
    /// The ticket carries the caller's real `sub`/`org` so audit and RLS org
    /// scoping downstream stay real, the reduced read-only scope set the
    /// preview proxy path enforces, `token_use: "preview"` (which the
    /// standard `JwtValidator::validate` rejects, so the ticket is dead as a
    /// Bearer credential everywhere else), and the workspace-bound audience
    /// `preview:<workspaceId>` (a ticket for workspace A presented on workspace B's path fails
    /// closed).
    pub fn mint_preview_ticket(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<String, AppError> {
        let encoding_key = self
            .jwt_encoding_key
            .as_ref()
            .ok_or_else(|| G5Error::Internal {
                message: "JWT signing key not configured".to_string(),
            })?;

        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes: vec![Scope::TasksRead, Scope::WorkspacesRead],
            iat: now,
            exp: now + PREVIEW_TICKET_LIFETIME_SECS,
            iss: Some(self.jwt_issuer.clone()),
            aud: Some(preview_ticket_audience(workspace_id)),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: Some(PREVIEW_TOKEN_USE.to_string()),
            client: None,
        };

        let header = Header::new(Algorithm::ES256);
        let ticket = encode(&header, &claims, encoding_key).map_err(|e| {
            error!("Preview ticket JWT encoding failed: {}", e);
            G5Error::Internal {
                message: "Failed to encode preview ticket".to_string(),
            }
        })?;

        info!(user_id = %user_id, workspace_id = %workspace_id, "Preview ticket minted");
        Ok(ticket)
    }

    /// Mints a single-use, workspace-bound preview HANDOFF ticket (issue
    /// G5Labs/G5#935; `routes/preview` [D15], auth_service [D21]).
    ///
    /// Claims are identical to [`mint_preview_ticket`](Self::mint_preview_ticket)
    /// except `token_use: "preview_handoff"` — accepted ONLY by
    /// `JwtValidator::validate_preview_handoff_ticket`, so a URL-leaked
    /// handoff ticket can never be replayed as a session cookie or Bearer —
    /// and the 60-second [`PREVIEW_HANDOFF_TICKET_LIFETIME_SECS`] expiry. The
    /// `jti` is the single-use consumption key; the host-translation layer
    /// records it on exchange and rejects replays. Like `mint_preview_ticket`,
    /// this does not authorize — callers have already re-authorized via
    /// `scoped_db_for_job`.
    pub fn mint_preview_handoff_ticket(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<String, AppError> {
        let encoding_key = self
            .jwt_encoding_key
            .as_ref()
            .ok_or_else(|| G5Error::Internal {
                message: "JWT signing key not configured".to_string(),
            })?;

        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes: vec![Scope::TasksRead, Scope::WorkspacesRead],
            iat: now,
            exp: now + PREVIEW_HANDOFF_TICKET_LIFETIME_SECS,
            iss: Some(self.jwt_issuer.clone()),
            aud: Some(preview_ticket_audience(workspace_id)),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: Some(PREVIEW_HANDOFF_TOKEN_USE.to_string()),
            client: None,
        };

        let header = Header::new(Algorithm::ES256);
        let ticket = encode(&header, &claims, encoding_key).map_err(|e| {
            error!("Preview handoff ticket JWT encoding failed: {}", e);
            G5Error::Internal {
                message: "Failed to encode preview handoff ticket".to_string(),
            }
        })?;

        info!(user_id = %user_id, workspace_id = %workspace_id, "Preview handoff ticket minted");
        Ok(ticket)
    }

    // -----------------------------------------------------------------------
    // SCM Connection Listing
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Internal: JWT Minting
    // -----------------------------------------------------------------------

    /// Internal method that creates and signs a JWT.
    fn mint_jwt(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        scopes: Vec<Scope>,
        lifetime_secs: i64,
        client: Option<&str>,
    ) -> Result<String, AppError> {
        let encoding_key = self
            .jwt_encoding_key
            .as_ref()
            .ok_or_else(|| G5Error::Internal {
                message: "JWT signing key not configured".to_string(),
            })?;

        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes,
            iat: now,
            exp: now + lifetime_secs,
            iss: Some(self.jwt_issuer.clone()),
            aud: Some(self.jwt_audience.clone()),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: None,
            client: client.map(str::to_owned),
        };

        let header = Header::new(Algorithm::ES256);
        let token = encode(&header, &claims, encoding_key).map_err(|e| {
            error!("JWT encoding failed: {}", e);
            G5Error::Internal {
                message: "Failed to encode JWT".to_string(),
            }
        })?;

        Ok(token)
    }

    fn mint_ws_jwt(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        scopes: Vec<Scope>,
        workspace_id: Option<Uuid>,
    ) -> Result<String, AppError> {
        let encoding_key = self
            .jwt_encoding_key
            .as_ref()
            .ok_or_else(|| G5Error::Internal {
                message: "JWT signing key not configured".to_string(),
            })?;
        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes,
            iat: now,
            exp: now + 300,
            iss: Some(self.jwt_issuer.clone()),
            aud: Some(websocket_ticket_audience(workspace_id)),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: Some(WEBSOCKET_TOKEN_USE.to_string()),
            client: None,
        };
        encode(&Header::new(Algorithm::ES256), &claims, encoding_key).map_err(|error| {
            error!(%error, "Failed to encode WebSocket ticket");
            AppError::from_error(G5Error::Internal {
                message: "Failed to encode WebSocket ticket".to_string(),
            })
        })
    }

    // -----------------------------------------------------------------------
    // Internal: AES-256-GCM Encryption/Decryption
    // -----------------------------------------------------------------------

    fn scm_tokens(&self) -> Result<&crate::auth::ScmTokenManager, AppError> {
        self.scm_tokens.as_ref().ok_or_else(|| {
            G5Error::Internal {
                message: "SCM encryption key not configured".to_string(),
            }
            .into()
        })
    }

    /// Stored tokens use the shared manager's AES-256-GCM format
    /// (`nonce || ciphertext || tag`); existing rows remain readable.
    fn encrypt_token(&self, plaintext: &str) -> Result<Vec<u8>, AppError> {
        self.scm_tokens()?.encrypt_token(plaintext)
    }

    #[cfg(test)]
    fn decrypt_token(&self, ciphertext: &[u8]) -> Result<String, AppError> {
        self.scm_tokens()?.decrypt_token(ciphertext)
    }

    // -----------------------------------------------------------------------
    // Internal: OAuth Helpers
    // -----------------------------------------------------------------------

    fn providers(&self) -> Result<&ProviderClients, AppError> {
        self.providers
            .as_ref()
            .map_err(|error| G5Error::from(error.clone()).into())
    }
    fn selected_provider(provider: &str) -> Result<g5_scm::ScmProvider, AppError> {
        g5_scm::ScmProvider::from_name(provider).ok_or_else(|| G5Error::ValidationError {
            details: serde_json::json!({"field":"provider", "error":format!("Unsupported SCM provider: {provider}")})
        }.into())
    }
    fn login_protocol(
        &self,
        provider: &str,
        config: &OAuthProviderConfig,
    ) -> Result<LoginClient, AppError> {
        let config = LoginConfig::new(
            Self::selected_provider(provider)?,
            &config.token_url,
            &config.userinfo_url,
            config.client_id.clone(),
            config.client_secret.clone(),
            config.redirect_uri.clone(),
        )
        .map_err(G5Error::from)?;
        Ok(self.providers()?.login(config))
    }
    fn connection_protocol(
        &self,
        provider: &str,
        config: &OAuthProviderConfig,
    ) -> Result<g5_scm::providers::ConnectionOAuthClient, AppError> {
        let config = g5_scm::providers::ConnectionOAuthConfig::new(
            Self::selected_provider(provider)?,
            &config.token_url,
            &config.userinfo_url,
            config.client_id.clone(),
            config.client_secret.clone(),
            config.redirect_uri.clone(),
        )
        .map_err(G5Error::from)?;
        Ok(self.providers()?.connection_oauth(config))
    }
    fn account_protocol(
        &self,
        provider: &str,
        token: &str,
    ) -> Result<g5_scm::providers::AccountClient, AppError> {
        // Endpoint selection is composition; protocol dispatch belongs to SCM.
        let provider = Self::selected_provider(provider)?;
        let api_base = match provider {
            g5_scm::ScmProvider::GitHub | g5_scm::ScmProvider::GitHubInstance { .. } => {
                &self.scm_github_api_base_url
            }
            g5_scm::ScmProvider::GitLab | g5_scm::ScmProvider::GitLabInstance { .. } => {
                &self.scm_gitlab_api_root
            }
        };
        Ok(self.providers()?.account(
            AccountGrant::new(provider, api_base, token.to_owned()).map_err(G5Error::from)?,
        ))
    }
    async fn exchange_code_for_token(
        &self,
        provider: &str,
        config: &OAuthProviderConfig,
        code: &str,
        verifier: &str,
    ) -> Result<LoginTokens, AppError> {
        if provider == MICROSOFT_PROVIDER {
            let tokens = self
                .entra
                .as_ref()
                .ok_or(G5Error::Unauthorized)?
                .exchange_code(
                    &config.token_url,
                    &config.client_id,
                    &config.client_secret,
                    &config.redirect_uri,
                    code,
                    verifier,
                )
                .await?;
            return Ok(LoginTokens {
                access_token: tokens.access_token,
                id_token: tokens.id_token,
            });
        }
        let tokens: OAuthTokens = self
            .login_protocol(provider, config)?
            .exchange_code(code, Some(verifier))
            .await
            .map_err(G5Error::from)?;
        Ok(LoginTokens {
            access_token: tokens.access_token,
            id_token: None,
        })
    }
    async fn fetch_user_info(
        &self,
        provider: &str,
        access_token: &str,
        config: &OAuthProviderConfig,
        github_email_mode: GitHubEmailMode,
    ) -> Result<OAuthUserInfo, AppError> {
        let client = self.login_protocol(provider, config)?;
        let identity = match github_email_mode {
            GitHubEmailMode::LoginVerifiedSet => client.user_info(access_token).await,
            GitHubEmailMode::SkipEmailLookup => {
                self.connection_protocol(provider, config)?
                    .user_info(access_token)
                    .await
            }
        }
        .map_err(G5Error::from)?;
        match identity {
            LoginUser::GitHub(user) => {
                let (email, verified_emails) = match github_email_mode {
                    GitHubEmailMode::LoginVerifiedSet => {
                        self.fetch_github_login_emails(&client, access_token, user.email.as_deref())
                            .await?
                    }
                    GitHubEmailMode::SkipEmailLookup => (
                        user.email
                            .as_deref()
                            .map(normalize_email)
                            .unwrap_or_else(|| format!("{}@users.noreply.github.com", user.id)),
                        Vec::new(),
                    ),
                };
                Ok(OAuthUserInfo {
                    oauth_id: user.id.to_string(),
                    email,
                    verified_emails,
                    display_name: user.name.unwrap_or_else(|| user.login.clone()),
                    avatar_url: user.avatar_url,
                    provider_username: Some(user.login),
                })
            }
            LoginUser::GitLab(user) => {
                let email = user.email.ok_or_else(|| G5Error::Internal {
                    message: "No email found on GitLab account".into(),
                })?;
                let email = normalize_email(&email);
                Ok(OAuthUserInfo {
                    oauth_id: user.id.to_string(),
                    verified_emails: single_verified_email(&email),
                    email,
                    display_name: user.name.unwrap_or_else(|| user.username.clone()),
                    avatar_url: user.avatar_url,
                    provider_username: Some(user.username),
                })
            }
        }
    }
    async fn fetch_github_login_emails(
        &self,
        client: &LoginClient,
        access_token: &str,
        public_email: Option<&str>,
    ) -> Result<(String, Vec<String>), AppError> {
        let emails = client
            .github_emails(access_token)
            .await
            .map_err(G5Error::from)?;
        let mut verified_emails = Vec::new();
        let mut primary_email = None;

        for github_email in emails {
            if !github_email.verified {
                continue;
            }

            let normalized = normalize_email(&github_email.email);
            if github_email.primary && !normalized.is_empty() {
                primary_email = Some(normalized.clone());
            }
            push_unique_email(&mut verified_emails, normalized);
        }

        let verified_public_email = public_email
            .map(normalize_email)
            .filter(|email| verified_emails.iter().any(|verified| verified == email));

        let canonical_email = primary_email
            .or(verified_public_email)
            .or_else(|| verified_emails.first().cloned())
            .ok_or_else(|| G5Error::Internal {
                message: "No verified email found on GitHub account".to_string(),
            })?;

        let mut ordered_emails = Vec::new();
        push_unique_email(&mut ordered_emails, canonical_email.clone());
        for email in verified_emails {
            push_unique_email(&mut ordered_emails, email);
        }

        Ok((canonical_email, ordered_emails))
    }

    // -----------------------------------------------------------------------
    // Internal: SCM Repo Listing
    // -----------------------------------------------------------------------

    async fn list_account_repos(
        &self,
        auth: &AuthContext,
        provider: &str,
        access_token: &str,
        org_id: Uuid,
        cursor: Option<String>,
    ) -> Result<ScmRepoListResponse, AppError> {
        let page = cursor
            .as_deref()
            .and_then(|c| c.parse::<u32>().ok())
            .unwrap_or(1);
        let page = self
            .account_protocol(provider, access_token)?
            .repositories(page)
            .await
            .map_err(account_provider_error)?;
        let provider_repo_ids: Vec<String> = page
            .repositories
            .iter()
            .map(|repo| repo.provider_repo_id.clone())
            .collect();
        let imported_map = self
            .imported_repo_map_for_org(auth, provider, org_id, &provider_repo_ids)
            .await?;
        let repos = page
            .repositories
            .into_iter()
            .map(|r| {
                let imported_id = imported_map.get(&r.provider_repo_id).copied();
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
                    already_imported: imported_id.is_some(),
                    imported_repo_id: imported_id,
                }
            })
            .collect();
        Ok(ScmRepoListResponse {
            repos,
            next_cursor: page.next_page.map(|p| p.to_string()),
        })
    }

    async fn imported_repo_map_for_org(
        &self,
        auth: &AuthContext,
        provider: &str,
        org_id: Uuid,
        provider_repo_ids: &[String],
    ) -> Result<HashMap<String, Uuid>, AppError> {
        if provider_repo_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut db = self
            .scoped_repo_db_for_org(auth, org_id, &["workspaces:read"])
            .await?;

        let imported_repos = sqlx::query_as::<_, ImportedRepoStatusRow>(
            r#"
            SELECT provider_repo_id, id
            FROM repos
            WHERE provider = $1 AND org_id = $2 AND provider_repo_id = ANY($3)
            "#,
        )
        .bind(provider)
        .bind(org_id)
        .bind(provider_repo_ids)
        .fetch_all(db.conn())
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to check imported repos: {}", e),
        })?;

        db.rollback().await.map_err(|e| G5Error::Internal {
            message: format!("Failed to clean up imported-repo scoped session: {}", e),
        })?;

        Ok(imported_repos
            .into_iter()
            .filter_map(|row| {
                row.provider_repo_id
                    .map(|provider_repo_id| (provider_repo_id, row.id))
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Internal: Scope Resolution
    // -----------------------------------------------------------------------

    /// Resolves effective scopes for a user in an org.
    ///
    /// Combines role-based default scopes with any user-specific grants.
    pub(crate) async fn resolve_effective_scopes(
        &self,
        user_id: Uuid,
        org_id: Uuid,
    ) -> Result<Vec<Scope>, AppError> {
        // If org_id is nil (user has no org), return empty scopes.
        if org_id.is_nil() {
            return Ok(Vec::new());
        }

        // Get the user's role in the org.
        let membership = sqlx::query!(
            r#"
            SELECT role
            FROM org_memberships
            WHERE deactivated_at IS NULL AND user_id = $1 AND org_id = $2
            "#,
            user_id,
            org_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to query org membership: {}", e),
        })?;

        let role = match membership {
            Some(m) => m.role,
            None => return Ok(Vec::new()),
        };

        // Parse the DB role string into the typed OrgRole enum.
        let org_role = role.parse::<OrgRole>().map_err(|_| G5Error::Internal {
            message: format!("Invalid role in org_memberships: {role:?}"),
        })?;

        // Get role-based default scopes.
        let mut scopes: Vec<Scope> = Self::default_scopes_for_role(org_role);

        // Get org-level default scopes.
        let org_scopes = sqlx::query!(
            r#"
            SELECT scope
            FROM org_default_scopes
            WHERE org_id = $1
            "#,
            org_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to query org default scopes: {}", e),
        })?;

        // Additive grants raise a viewer only to read-only scopes
        // (`g5.organization.viewer-ceiling`), in issued tokens as in current
        // authority: the same rule `PermissionService` applies.
        let within_ceiling =
            |s: &Scope| crate::services::PermissionService::scope_within_role_ceiling(org_role, s);

        for row in org_scopes {
            if let Ok(s) = row.scope.parse::<Scope>() {
                if !scopes.contains(&s) && within_ceiling(&s) {
                    scopes.push(s);
                }
            } else {
                tracing::warn!(scope = %row.scope, "Unknown scope in org_default_scopes, skipping");
            }
        }

        // Get user-specific scope grants.
        let user_grants = sqlx::query!(
            r#"
            SELECT scope
            FROM user_scope_grants
            WHERE user_id = $1 AND org_id = $2
              AND (expires_at IS NULL OR expires_at > now())
            "#,
            user_id,
            org_id,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| G5Error::Internal {
            message: format!("Failed to query user scope grants: {}", e),
        })?;

        for row in user_grants {
            if let Ok(s) = row.scope.parse::<Scope>() {
                if !scopes.contains(&s) && within_ceiling(&s) {
                    scopes.push(s);
                }
            } else {
                tracing::warn!(scope = %row.scope, "Unknown scope in user_scope_grants, skipping");
            }
        }

        scopes.sort_by_key(|s| s.as_str());
        scopes.dedup();

        Ok(scopes)
    }

    /// Returns default scopes for a given org role, matching the predefined
    /// role→scope mappings from API_AND_STORAGE.md §2.12.
    pub(crate) fn default_scopes_for_role(role: OrgRole) -> Vec<Scope> {
        match role {
            OrgRole::Admin => vec![
                Scope::WorkspacesRead,
                Scope::WorkspacesWrite,
                Scope::WorkspacesAdmin,
                Scope::TasksRead,
                Scope::TasksExecute,
                Scope::SessionsRead,
                Scope::SessionsWrite,
                Scope::WorkersAdmin,
                Scope::WorkersRead,
                Scope::BillingAdmin,
                Scope::OrgRead,
                Scope::OrgAdmin,
                Scope::UsersRead,
                Scope::SettingsRead,
                Scope::SettingsWrite,
            ],
            OrgRole::Member => vec![
                Scope::WorkspacesRead,
                Scope::WorkspacesWrite,
                Scope::TasksRead,
                Scope::TasksExecute,
                Scope::SessionsRead,
                Scope::SessionsWrite,
                Scope::WorkersRead,
                Scope::OrgRead,
                Scope::UsersRead,
                Scope::SettingsRead,
            ],
            OrgRole::Viewer => vec![
                Scope::WorkspacesRead,
                Scope::TasksRead,
                Scope::SessionsRead,
                Scope::WorkersRead,
                Scope::OrgRead,
                Scope::UsersRead,
            ],
            _ => Vec::new(),
        }
    }

    /// List organizations/groups from an SCM provider using the user's stored
    /// SCM access token.
    pub async fn list_scm_orgs(
        &self,
        user_id: Uuid,
        provider: &str,
    ) -> Result<Vec<g5_api_foundation::contracts::auth::ScmOrgDTO>, AppError> {
        let access_token = self.get_scm_token(user_id, provider).await?;

        let organizations = self
            .account_protocol(provider, &access_token)?
            .organizations()
            .await
            .map_err(account_provider_error)?;
        Ok(organizations
            .into_iter()
            .map(|o| g5_api_foundation::contracts::auth::ScmOrgDTO {
                id: o.id,
                name: o.name,
                avatar_url: o.avatar_url,
            })
            .collect())
    }

    /// Get the list of enabled OAuth login providers.
    #[must_use]
    pub fn enabled_providers(&self) -> Vec<String> {
        ["github", MICROSOFT_PROVIDER]
            .into_iter()
            .filter(|provider| self.oauth_configs.contains_key(*provider))
            .map(str::to_string)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Utility Functions
// ---------------------------------------------------------------------------

fn account_provider_error(error: ProviderError) -> AppError {
    if error.status == Some(401) {
        G5Error::Unauthorized.into()
    } else {
        G5Error::from(error).into()
    }
}

/// The stored form of a refresh credential (`g5.user.session-authority`):
/// the hex SHA-256 of the bearer, so a read of `refresh_tokens` cannot replay
/// it. The plaintext is returned once to the client that logged in.
#[must_use]
pub fn refresh_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Generates a cryptographically random hex string of the given byte length.
///
/// Uses `OsRng` for cryptographic security.
fn generate_random_hex(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Authorize URL for an attempt, with its S256 PKCE challenge.
fn authorize_url(config: &OAuthProviderConfig, state: &str, attempt: &OAuthAttempt) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&scope={}&state={}&response_type=code&code_challenge={}&code_challenge_method=S256",
        config.authorize_url,
        percent_encode(&config.client_id),
        percent_encode(&config.redirect_uri),
        percent_encode(&config.scopes.join(" ")),
        percent_encode(state),
        attempt.code_challenge(),
    )
}

/// Where an SCM connection returns when nothing safe was asked for.
pub const SCM_DEFAULT_RETURN_TO: &str = "/settings";

fn build_scm_oauth_state(return_to: Option<&str>) -> String {
    let nonce = generate_random_hex(32);
    let return_to = return_to
        .and_then(sanitize_scm_return_to)
        .unwrap_or(SCM_DEFAULT_RETURN_TO);
    let encoded_return_to =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(return_to.as_bytes());
    format!("{nonce}.{encoded_return_to}")
}

/// The return path carried by an SCM connection state, re-checked on the way
/// out, or [`SCM_DEFAULT_RETURN_TO`]. A state taken from a request is
/// attacker-chosen until its attempt is verified, so callers use this only for
/// an attempt this browser started (SEC-M36).
#[must_use]
pub fn scm_return_to_from_state(state: &str) -> String {
    state
        .rsplit_once('.')
        .and_then(|(_, encoded)| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(encoded)
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|candidate| sanitize_scm_return_to(&candidate).map(str::to_string))
        .unwrap_or_else(|| SCM_DEFAULT_RETURN_TO.to_string())
}

fn sanitize_scm_return_to(return_to: &str) -> Option<&str> {
    let candidate = return_to.trim();
    is_local_return_path(candidate).then_some(candidate)
}

fn hash_cli_device_code(device_code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(device_code.as_bytes());
    hex::encode(hasher.finalize())
}

fn generate_cli_user_code() -> String {
    let compact = generate_random_hex(4).to_uppercase();
    format!("{}-{}", &compact[..4], &compact[4..])
}

fn normalize_cli_user_code(input: &str) -> String {
    let compact: String = input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    if compact.len() == 8 {
        format!("{}-{}", &compact[..4], &compact[4..])
    } else {
        input.trim().to_ascii_uppercase()
    }
}

/// Percent-encodes a string for use in URL query parameters.
///
/// Encodes all characters except unreserved characters per RFC 3986.
fn percent_encode(input: &str) -> String {
    url::form_urlencoded::byte_serialize(input.as_bytes()).collect()
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn push_unique_email(emails: &mut Vec<String>, email: String) {
    if !email.is_empty() && !emails.iter().any(|existing| existing == &email) {
        emails.push(email);
    }
}

fn single_verified_email(email: &str) -> Vec<String> {
    let normalized = normalize_email(email);
    if normalized.is_empty() {
        Vec::new()
    } else {
        vec![normalized]
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(ref db_err) = e {
        db_err.code().is_some_and(|code| code == "23505")
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// GitLab SCM connection OAuth (repository-access scopes and callback),
/// distinct from GitLab login.
fn gitlab_scm_oauth_config(
    auth_config: &AuthConfig,
    scm_redirect_base: &str,
) -> Option<OAuthProviderConfig> {
    let gl = auth_config.scm_gitlab.as_ref()?;
    Some(OAuthProviderConfig {
        client_id: gl.client_id.clone(),
        client_secret: gl.client_secret.clone(),
        authorize_url: gl
            .authorize_url
            .clone()
            .unwrap_or_else(|| "https://gitlab.com/oauth/authorize".to_string()),
        token_url: gl
            .token_url
            .clone()
            .unwrap_or_else(|| "https://gitlab.com/oauth/token".to_string()),
        userinfo_url: auth_config.scm_gitlab_api_url().to_string(),
        scopes: vec![
            "read_repository".to_string(),
            "write_repository".to_string(),
            "read_api".to_string(),
        ],
        redirect_uri: format!("{}/gitlab/callback", scm_redirect_base),
    })
}

fn connection_refresh_config(
    config: &OAuthProviderConfig,
) -> Option<g5_scm::providers::ConnectionOAuthConfig> {
    g5_scm::providers::ConnectionOAuthConfig::new(
        g5_scm::ScmProvider::GitLab,
        &config.token_url,
        &config.userinfo_url,
        config.client_id.clone(),
        config.client_secret.clone(),
        config.redirect_uri.clone(),
    )
    .map_err(|error| warn!(%error, "GitLab SCM token refresh disabled: invalid OAuth endpoints"))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_cookie_names_keep_default_and_custom_names() {
        for base in ["g5_refresh", "preview_refresh", "g5_github_app_pkce_custom"] {
            let protected = format!("__Host-{base}");
            assert_eq!(
                refresh_cookie_names(base, true),
                (protected.clone(), Some(base.to_string()))
            );
            assert_eq!(refresh_cookie_names(&protected, true), (protected, None));
            assert_eq!(refresh_cookie_names(base, false), (base.to_string(), None));
        }
    }

    /// Build a test-only `AuthService` without a real database connection.
    /// Uses `PgPool::connect_lazy` with a dummy URL so the pool is valid but
    /// never actually connects. Only suitable for tests that don't hit the DB.
    fn test_auth_service(encryption_key: Option<[u8; 32]>) -> AuthService {
        let db = PgPool::connect_lazy("postgres://invalid@localhost/nonexistent")
            .expect("connect_lazy should not fail");
        AuthService {
            db: db.clone(),
            tenant_pool: db,
            authz: Arc::new(crate::stubs::StubAuthz),
            providers: ProviderClients::new(),
            jwt_encoding_key: None,
            jwt_decoding_key: None,
            scm_tokens: encryption_key
                .map(|key| crate::auth::ScmTokenManager::new(key, ProviderClients::new())),
            oauth_configs: HashMap::new(),
            scm_oauth_configs: HashMap::new(),
            entra: None,
            scm_origin_policy: g5_scm::ScmOriginPolicy::default(),
            org_created_hooks: Vec::new(),
            access_token_ttl_secs: 3600,
            refresh_token_ttl_secs: 2592000,
            refresh_token_absolute_ttl_secs: 7776000,
            jwt_issuer: "g5-api".to_string(),
            jwt_audience: "g5".to_string(),
            cookie_secure: true,
            refresh_cookie_name: "__Host-g5_refresh".to_string(),
            legacy_refresh_cookie_name: Some("g5_refresh".to_string()),
            scm_github_api_base_url: "https://api.github.com".to_string(),
            scm_gitlab_api_root: "https://gitlab.com/api/v4".to_string(),
        }
    }

    #[tokio::test]
    #[serial_test::serial(e2e_auth)]
    async fn test_token_requires_explicit_opt_in_before_database_access() {
        struct RestoreFlag(Option<std::ffi::OsString>);
        impl Drop for RestoreFlag {
            fn drop(&mut self) {
                match self.0.take() {
                    Some(value) => std::env::set_var("G5_E2E_AUTH", value),
                    None => std::env::remove_var("G5_E2E_AUTH"),
                }
            }
        }
        let _restore = RestoreFlag(std::env::var_os("G5_E2E_AUTH"));
        let service = test_auth_service(None);
        // A closed pool makes any attempted database access fail immediately.
        // The email is allowed, so only the service's opt-in gate can forbid it.
        service.db.close().await;
        for value in [
            None,
            Some(""),
            Some("false"),
            Some("1"),
            Some("yes"),
            Some("TRUE"),
            Some(" true "),
        ] {
            match value {
                Some(value) => std::env::set_var("G5_E2E_AUTH", value),
                None => std::env::remove_var("G5_E2E_AUTH"),
            }
            let error = service
                .issue_test_token("demo@g5labs.ai")
                .await
                .unwrap_err();
            assert!(
                matches!(error.inner, G5Error::Forbidden { .. }),
                "{value:?} must forbid issuance before database access: {error:?}"
            );
        }
    }

    // -- DTO Serialization Tests --

    #[test]
    fn test_oauth_redirect_serialization() {
        let redirect = OAuthRedirect {
            url: "https://github.com/login/oauth/authorize?client_id=test".to_string(),
            state: "abc123".to_string(),
            attempt_cookie: "g5oa1.digest.verifier".to_string(),
        };
        let json = serde_json::to_value(&redirect).unwrap();
        assert!(json.get("url").is_some());
        assert!(json.get("state").is_some());
        assert!(json.get("attemptCookie").is_none());
    }

    #[test]
    fn test_auth_tokens_serialization() {
        let tokens = AuthTokens {
            jwt: "eyJ...".to_string(),
            refresh_token: "deadbeef".to_string(),
            expires_in: 3600,
            user_id: Uuid::nil(),
        };
        let json = serde_json::to_value(&tokens).unwrap();
        assert_eq!(json["expiresIn"], 3600);
        assert!(json.get("refreshToken").is_some());
        assert!(json.get("userId").is_some());
        // Verify camelCase
        assert!(json.get("expires_in").is_none());
        assert!(json.get("refresh_token").is_none());
        assert!(json.get("user_id").is_none());
    }

    #[test]
    fn test_scm_connection_result_serialization() {
        let result = ScmConnectionResult {
            provider: "github".to_string(),
            provider_username: "octocat".to_string(),
            scopes: vec!["repo".to_string(), "read:org".to_string()],
            connected_at: "2024-01-01T00:00:00+00:00".to_string(),
            return_to: "/g5labs/base?settings=scm".to_string(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("providerUsername").is_some());
        assert!(json.get("connectedAt").is_some());
        assert!(json.get("returnTo").is_some());
        assert!(json.get("provider_username").is_none());
        assert!(json.get("connected_at").is_none());
    }

    #[test]
    fn test_scm_repo_dto_serialization() {
        let repo = ScmRepoDTO {
            provider_repo_id: "12345".to_string(),
            full_name: "octocat/hello-world".to_string(),
            name: "hello-world".to_string(),
            owner: "octocat".to_string(),
            default_branch: "main".to_string(),
            is_private: false,
            clone_url: "https://github.com/octocat/hello-world.git".to_string(),
            description: Some("A test repo".to_string()),
            language: Some("Rust".to_string()),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            already_imported: true,
            imported_repo_id: Some(Uuid::nil()),
        };
        let json = serde_json::to_value(&repo).unwrap();
        assert!(json.get("providerRepoId").is_some());
        assert!(json.get("fullName").is_some());
        assert!(json.get("defaultBranch").is_some());
        assert!(json.get("isPrivate").is_some());
        assert!(json.get("cloneUrl").is_some());
        assert!(json.get("alreadyImported").is_some());
        assert!(json.get("importedRepoId").is_some());
        assert!(json.get("updatedAt").is_some());
        // No snake_case
        assert!(json.get("provider_repo_id").is_none());
        assert!(json.get("full_name").is_none());
        assert!(json.get("default_branch").is_none());
    }

    #[test]
    fn test_import_repo_spec_deserialization() {
        let json = r#"{
            "providerRepoId": "12345",
            "fullName": "octocat/hello-world",
            "cloneUrl": "https://github.com/octocat/hello-world.git",
            "defaultBranch": "main"
        }"#;
        let spec: ImportRepoSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.provider_repo_id, "12345");
        assert_eq!(spec.full_name, "octocat/hello-world");
        assert_eq!(spec.clone_url, "https://github.com/octocat/hello-world.git");
        assert_eq!(spec.default_branch, "main");
    }

    #[test]
    fn test_repo_dto_serialization() {
        let dto = RepoDTO {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            name: "hello-world".to_string(),
            slug: "octocat-hello-world".to_string(),
            git_url: "https://github.com/octocat/hello-world.git".to_string(),
            default_branch: "main".to_string(),
            provider: Some("github".to_string()),
            provider_repo_id: Some("12345".to_string()),
            provider_full_name: Some("octocat/hello-world".to_string()),
            created_at: "2024-01-01T00:00:00+00:00".to_string(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("orgId").is_some());
        assert!(json.get("gitUrl").is_some());
        assert!(json.get("defaultBranch").is_some());
        assert!(json.get("providerRepoId").is_some());
        assert!(json.get("providerFullName").is_some());
        assert!(json.get("createdAt").is_some());
        // No snake_case
        assert!(json.get("org_id").is_none());
        assert!(json.get("git_url").is_none());
    }

    #[test]
    fn test_refresh_scm_result_serialization() {
        let result = RefreshScmResult {
            refreshed: 3,
            failed: 1,
            errors: vec!["connection timeout".to_string()],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["refreshed"], 3);
        assert_eq!(json["failed"], 1);
        assert_eq!(json["errors"][0], "connection timeout");
    }

    // -- Utility Tests --

    #[test]
    fn test_generate_random_hex() {
        let hex1 = generate_random_hex(32);
        let hex2 = generate_random_hex(32);
        assert_eq!(hex1.len(), 64); // 32 bytes = 64 hex chars
        assert_ne!(hex1, hex2); // Should be different (with overwhelming probability)
    }

    #[test]
    fn test_generate_random_hex_length() {
        assert_eq!(generate_random_hex(16).len(), 32);
        assert_eq!(generate_random_hex(1).len(), 2);
        assert_eq!(generate_random_hex(0).len(), 0);
    }

    #[test]
    fn test_mask_email_for_log_keeps_initial_and_domain() {
        assert_eq!(mask_email_for_log("alice@corp.com"), "a***@corp.com");
        // A single-character local part still redacts to the initial + domain.
        assert_eq!(mask_email_for_log("a@corp.com"), "a***@corp.com");
        // Subdomains and plus-addressing are preserved on the domain side.
        assert_eq!(
            mask_email_for_log("bob.smith+tag@mail.example.co.uk"),
            "b***@mail.example.co.uk"
        );
    }

    #[test]
    fn test_mask_email_for_log_fully_redacts_malformed_input() {
        // No local part, no domain, or no `@` must never leak partial PII.
        assert_eq!(mask_email_for_log(""), "***");
        assert_eq!(mask_email_for_log("no-at-sign"), "***");
        assert_eq!(mask_email_for_log("@corp.com"), "***");
        assert_eq!(mask_email_for_log("alice@"), "***");
    }

    // -- Encryption/Decryption Tests --

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        // Create a service with a known encryption key.
        let key = [0x42u8; 32];
        let service = test_auth_service(Some(key));

        let plaintext = "ghp_test_token_12345";
        let encrypted = service.encrypt_token(plaintext).unwrap();

        // Encrypted data should be nonce (12) + ciphertext + tag (16)
        assert!(encrypted.len() > 12 + 16);
        assert_ne!(&encrypted[12..], plaintext.as_bytes());

        let decrypted = service.decrypt_token(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_encrypt_unique_nonces() {
        let key = [0x42u8; 32];
        let service = test_auth_service(Some(key));

        let enc1 = service.encrypt_token("token").unwrap();
        let enc2 = service.encrypt_token("token").unwrap();

        // Nonces (first 12 bytes) should differ.
        assert_ne!(&enc1[..12], &enc2[..12]);
    }

    #[tokio::test]
    async fn test_decrypt_invalid_ciphertext() {
        let key = [0x42u8; 32];
        let service = test_auth_service(Some(key));

        // Too short.
        let result = service.decrypt_token(&[0u8; 5]);
        assert!(result.is_err());

        // Valid length but garbage data.
        let result = service.decrypt_token(&[0u8; 40]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_encrypt_without_key() {
        let service = test_auth_service(None);

        let result = service.encrypt_token("token");
        assert!(result.is_err());
    }

    // -- Role-Based Scope Tests --

    #[tokio::test]
    async fn test_default_scopes_admin() {
        let scopes = AuthService::default_scopes_for_role(OrgRole::Admin);
        assert!(scopes.contains(&Scope::WorkspacesAdmin));
        assert!(scopes.contains(&Scope::OrgAdmin));
        assert!(scopes.contains(&Scope::SettingsWrite));
    }

    #[tokio::test]
    async fn test_default_scopes_member() {
        let scopes = AuthService::default_scopes_for_role(OrgRole::Member);
        assert!(scopes.contains(&Scope::WorkspacesWrite));
        assert!(scopes.contains(&Scope::TasksExecute));
        assert!(!scopes.contains(&Scope::WorkspacesAdmin));
        assert!(!scopes.contains(&Scope::OrgAdmin));
    }

    #[tokio::test]
    async fn test_default_scopes_viewer() {
        let scopes = AuthService::default_scopes_for_role(OrgRole::Viewer);
        assert!(scopes.contains(&Scope::WorkspacesRead));
        assert!(!scopes.contains(&Scope::WorkspacesWrite));
        assert!(!scopes.contains(&Scope::TasksExecute));
    }

    // -- OAuth Config Tests --

    #[test]
    fn test_oauth_provider_config() {
        let config = OAuthProviderConfig {
            client_id: "test-id".to_string(),
            client_secret: "test-secret".to_string(),
            authorize_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            userinfo_url: "https://example.com/userinfo".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            redirect_uri: "https://example.com/callback".to_string(),
        };
        assert_eq!(config.client_id, "test-id");
        assert_eq!(config.scopes.len(), 2);
    }

    #[test]
    fn test_scm_repo_list_response_serialization() {
        let response = ScmRepoListResponse {
            repos: vec![],
            next_cursor: Some("2".to_string()),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert!(json.get("repos").is_some());
        assert!(json.get("nextCursor").is_some());
        assert!(json.get("next_cursor").is_none());
    }
}
