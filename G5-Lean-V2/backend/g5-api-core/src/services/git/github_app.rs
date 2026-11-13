//! Client for the single G5-owned GitHub App that organizations install.
//!
//! Responsibilities:
//! - App JWT signing and installation access tokens narrowed to explicit
//!   `repository_ids` and `permissions`, cached per narrowed request.
//! - The administrator verification bridge: GitHub App OAuth with PKCE, used
//!   only to prove that the installing user can access the installation. The
//!   user token is discarded immediately and never stored.
//! - Installation metadata and repository inventory reads.
//! - HMAC-signed connection-attempt state, OAuth state and PKCE cookie values.
//!
//! The App private key, OAuth client secret and state-signing secret are
//! deployment configuration ([`GithubAppConfig`]) and are redacted in `Debug`.
//! GitHub error text is scrubbed of URLs and credential-shaped tokens before
//! it reaches an error or log.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use g5_contracts::error::G5Error;
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

pub use crate::config::GithubAppConfig;
use crate::services::identity::github_app_auth::{
    redact_message, sign_app_jwt, InstallationTokenCache,
};

/// Lifetime of a connection attempt (and of its signed state).
pub const CONNECTION_ATTEMPT_TTL: Duration = Duration::from_secs(10 * 60);

const STATE_VERSION: &str = "g5ga1";
const SELECTION_STATE_VERSION: &str = "g5ga1s";
/// State of a person's "Connect GitHub" (GitHub App user authorization).
const USER_GRANT_STATE_VERSION: &str = "g5gu1";
const PKCE_VERSION: &str = "g5gp2";
const PKCE_COOKIE: &str = "g5_github_app_pkce";

// ---------------------------------------------------------------------------
// Signed connection state
// ---------------------------------------------------------------------------

/// A verified reference to a connection attempt, recovered from signed state.
///
/// The signature proves G5 issued the state for this org/attempt; callers must
/// still load the attempt under the org's RLS scope, compare `state_digest`
/// with the stored digest, and check that it is unexpired and unconsumed.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionAttemptRef {
    pub org_id: Uuid,
    pub attempt_id: Uuid,
    /// Unix seconds at which the attempt expires.
    pub expires_at: i64,
    /// SHA-256 of the complete attempt state string.
    pub state_digest: [u8; 32],
}

impl std::fmt::Debug for ConnectionAttemptRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionAttemptRef")
            .field("org_id", &self.org_id)
            .field("attempt_id", &self.attempt_id)
            .field("expires_at", &self.expires_at)
            .field("state_digest", &"[REDACTED]")
            .finish()
    }
}

/// The authenticated state carried through the GitHub OAuth callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubAppOAuthState {
    /// Fresh installation: GitHub returned `installation_id` to the setup URL.
    Installation {
        attempt: ConnectionAttemptRef,
        installation_id: u64,
    },
    /// Reconnect an App already installed on GitHub: discover which
    /// installations of this App the user can access.
    ReconnectDiscovery { attempt: ConnectionAttemptRef },
    /// The user chose one installation on the reconnect chooser page.
    ReconnectSelection {
        attempt: ConnectionAttemptRef,
        installation_id: u64,
    },
}

impl GithubAppOAuthState {
    #[must_use]
    pub fn attempt(&self) -> &ConnectionAttemptRef {
        match self {
            Self::Installation { attempt, .. }
            | Self::ReconnectDiscovery { attempt }
            | Self::ReconnectSelection { attempt, .. } => attempt,
        }
    }
}

fn hmac_sign(cfg: &GithubAppConfig, domain: &str, message: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(cfg.connection_state_secret.as_bytes())
        .expect("HMAC accepts a key of any length");
    mac.update(domain.as_bytes());
    mac.update(b"\n");
    mac.update(message.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn hmac_verify(cfg: &GithubAppConfig, domain: &str, message: &str, signature: &str) -> bool {
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(cfg.connection_state_secret.as_bytes()) else {
        return false;
    };
    mac.update(domain.as_bytes());
    mac.update(b"\n");
    mac.update(message.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

fn signature(cfg: &GithubAppConfig, domain: &str, message: &str) -> String {
    URL_SAFE_NO_PAD.encode(hmac_sign(cfg, domain, message))
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Current Unix time in seconds.
#[must_use]
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// SHA-256 digest stored with the attempt; the raw state is never stored.
#[must_use]
pub fn connection_state_digest(state: &str) -> [u8; 32] {
    Sha256::digest(state.as_bytes()).into()
}

/// Issue the signed state for a new connection attempt:
/// `g5ga1.<org>.<attempt>.<expires>.<nonce>.<hmac>`.
#[must_use]
pub fn new_connection_state(
    cfg: &GithubAppConfig,
    org_id: Uuid,
    attempt_id: Uuid,
    expires_at: i64,
) -> String {
    let body = format!(
        "{STATE_VERSION}.{org_id}.{attempt_id}.{expires_at}.{}",
        random_token()
    );
    let sig = signature(cfg, "attempt", &body);
    format!("{body}.{sig}")
}

/// Verify a connection-attempt state's signature and expiry.
#[must_use]
pub fn verify_connection_state(
    cfg: &GithubAppConfig,
    state: &str,
    now: i64,
) -> Option<ConnectionAttemptRef> {
    let parts: Vec<&str> = state.split('.').collect();
    let [version, org, attempt, expires, nonce, sig] = parts.as_slice() else {
        return None;
    };
    if *version != STATE_VERSION || nonce.len() < 43 {
        return None;
    }
    let body = format!("{version}.{org}.{attempt}.{expires}.{nonce}");
    if !hmac_verify(cfg, "attempt", &body, sig) {
        return None;
    }
    let expires_at: i64 = expires.parse().ok()?;
    if expires_at <= now {
        return None;
    }
    Some(ConnectionAttemptRef {
        org_id: Uuid::parse_str(org).ok()?,
        attempt_id: Uuid::parse_str(attempt).ok()?,
        expires_at,
        state_digest: connection_state_digest(state),
    })
}

/// A verified "Connect GitHub" callback state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserConnectionAttemptRef {
    pub user_id: Uuid,
    pub attempt_id: Uuid,
    pub expires_at: i64,
    pub state_digest: [u8; 32],
}

/// Issue the signed state for a person's GitHub App user authorization:
/// `g5gu1.<user>.<attempt>.<expires>.<nonce>.<hmac>`. Its HMAC domain differs
/// from organization binding states, so neither can stand in for the other.
#[must_use]
pub fn new_user_connection_state(
    cfg: &GithubAppConfig,
    user_id: Uuid,
    attempt_id: Uuid,
    expires_at: i64,
) -> String {
    let body = format!(
        "{USER_GRANT_STATE_VERSION}.{user_id}.{attempt_id}.{expires_at}.{}",
        random_token()
    );
    let sig = signature(cfg, "user-grant", &body);
    format!("{body}.{sig}")
}

/// Verify a "Connect GitHub" state's signature and expiry.
#[must_use]
pub fn verify_user_connection_state(
    cfg: &GithubAppConfig,
    state: &str,
    now: i64,
) -> Option<UserConnectionAttemptRef> {
    let parts: Vec<&str> = state.split('.').collect();
    let [version, user, attempt, expires, nonce, sig] = parts.as_slice() else {
        return None;
    };
    if *version != USER_GRANT_STATE_VERSION || nonce.len() < 43 {
        return None;
    }
    let body = format!("{version}.{user}.{attempt}.{expires}.{nonce}");
    if !hmac_verify(cfg, "user-grant", &body, sig) {
        return None;
    }
    let expires_at: i64 = expires.parse().ok()?;
    if expires_at <= now {
        return None;
    }
    Some(UserConnectionAttemptRef {
        user_id: Uuid::parse_str(user).ok()?,
        attempt_id: Uuid::parse_str(attempt).ok()?,
        expires_at,
        state_digest: connection_state_digest(state),
    })
}

/// Bind GitHub's untrusted setup `installation_id` to the attempt state before
/// starting OAuth: `<attempt state>.i.<installation>.<hmac>`.
#[must_use]
pub fn oauth_state_for_installation(
    cfg: &GithubAppConfig,
    attempt_state: &str,
    installation_id: u64,
) -> Option<String> {
    if installation_id == 0 {
        return None;
    }
    let body = format!("{attempt_state}.i.{installation_id}");
    let sig = signature(cfg, "installation", &body);
    Some(format!("{body}.{sig}"))
}

/// OAuth state for reconnect discovery: `<attempt state>.r.<hmac>`.
#[must_use]
pub fn oauth_state_for_reconnect_discovery(cfg: &GithubAppConfig, attempt_state: &str) -> String {
    let body = format!("{attempt_state}.r");
    let sig = signature(cfg, "reconnect", &body);
    format!("{body}.{sig}")
}

/// Selection state for one installation on the reconnect chooser page. It
/// carries the attempt's digest rather than the raw attempt state:
/// `g5ga1s.<org>.<attempt>.<expires>.<digest>.<installation>.<hmac>`.
#[must_use]
pub fn oauth_state_for_reconnect_selection(
    cfg: &GithubAppConfig,
    attempt: &ConnectionAttemptRef,
    installation_id: u64,
) -> Option<String> {
    if installation_id == 0 {
        return None;
    }
    let body = format!(
        "{SELECTION_STATE_VERSION}.{}.{}.{}.{}.{installation_id}",
        attempt.org_id,
        attempt.attempt_id,
        attempt.expires_at,
        URL_SAFE_NO_PAD.encode(attempt.state_digest),
    );
    let sig = signature(cfg, "select", &body);
    Some(format!("{body}.{sig}"))
}

fn parse_installation_id(value: &str) -> Option<u64> {
    value.parse::<u64>().ok().filter(|id| *id > 0)
}

/// Verify any OAuth state form without revealing which component was invalid.
#[must_use]
pub fn parse_oauth_state(
    cfg: &GithubAppConfig,
    state: &str,
    now: i64,
) -> Option<GithubAppOAuthState> {
    if state.starts_with(&format!("{SELECTION_STATE_VERSION}.")) {
        let parts: Vec<&str> = state.split('.').collect();
        let [_, org, attempt, expires, digest, installation, sig] = parts.as_slice() else {
            return None;
        };
        let body = parts[..6].join(".");
        if !hmac_verify(cfg, "select", &body, sig) {
            return None;
        }
        let expires_at: i64 = expires.parse().ok()?;
        if expires_at <= now {
            return None;
        }
        let state_digest: [u8; 32] = URL_SAFE_NO_PAD.decode(digest).ok()?.try_into().ok()?;
        return Some(GithubAppOAuthState::ReconnectSelection {
            attempt: ConnectionAttemptRef {
                org_id: Uuid::parse_str(org).ok()?,
                attempt_id: Uuid::parse_str(attempt).ok()?,
                expires_at,
                state_digest,
            },
            installation_id: parse_installation_id(installation)?,
        });
    }

    let (body, sig) = state.rsplit_once('.')?;
    if let Some(attempt_state) = body.strip_suffix(".r") {
        if !hmac_verify(cfg, "reconnect", body, sig) {
            return None;
        }
        let attempt = verify_connection_state(cfg, attempt_state, now)?;
        return Some(GithubAppOAuthState::ReconnectDiscovery { attempt });
    }
    let (prefix, installation) = body.rsplit_once('.')?;
    let attempt_state = prefix.strip_suffix(".i")?;
    if !hmac_verify(cfg, "installation", body, sig) {
        return None;
    }
    let attempt = verify_connection_state(cfg, attempt_state, now)?;
    Some(GithubAppOAuthState::Installation {
        attempt,
        installation_id: parse_installation_id(installation)?,
    })
}

/// PKCE verifier bound to one connection attempt, signed before it is placed
/// in a short-lived HttpOnly cookie on the administrator's browser when the
/// attempt is created: `g5gp2.<verifier>.<attempt state digest>.<hmac>`.
/// Every callback requires this cookie for the same attempt, so a setup or
/// OAuth link forwarded to another browser cannot complete the attempt.
/// Returns `(verifier, cookie value)`.
#[must_use]
pub fn new_pkce_cookie(cfg: &GithubAppConfig, attempt_digest: &[u8; 32]) -> (String, String) {
    let verifier = random_token();
    let body = format!(
        "{PKCE_VERSION}.{verifier}.{}",
        URL_SAFE_NO_PAD.encode(attempt_digest)
    );
    let sig = signature(cfg, "pkce", &body);
    (verifier, format!("{body}.{sig}"))
}

/// The verifier from a PKCE cookie signed for the attempt whose state digest
/// is `attempt_digest`; `None` for any other attempt, tampering or version.
#[must_use]
pub fn parse_pkce_cookie(
    cfg: &GithubAppConfig,
    value: &str,
    attempt_digest: &[u8; 32],
) -> Option<String> {
    let parts: Vec<&str> = value.split('.').collect();
    let [version, verifier, digest, sig] = parts.as_slice() else {
        return None;
    };
    if *version != PKCE_VERSION || verifier.len() < 43 {
        return None;
    }
    if !hmac_verify(cfg, "pkce", &format!("{version}.{verifier}.{digest}"), sig) {
        return None;
    }
    let bound: [u8; 32] = URL_SAFE_NO_PAD.decode(digest).ok()?.try_into().ok()?;
    bool::from(bound.ct_eq(attempt_digest)).then(|| verifier.to_string())
}

#[must_use]
pub fn pkce_code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[must_use]
pub const fn pkce_cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-g5_github_app_pkce"
    } else {
        PKCE_COOKIE
    }
}

/// GitHub installation page for a new attempt.
#[must_use]
pub fn installation_url(cfg: &GithubAppConfig, attempt_state: &str) -> String {
    let mut url = Url::parse(&format!(
        "{}/apps/{}/installations/new",
        cfg.web_base_url, cfg.app_slug
    ))
    .expect("validated GitHub web base URL");
    url.query_pairs_mut().append_pair("state", attempt_state);
    url.into()
}

/// GitHub OAuth authorize page with PKCE.
#[must_use]
pub fn oauth_authorize_url(cfg: &GithubAppConfig, state: &str, code_challenge: &str) -> String {
    let mut url = Url::parse(&format!("{}/login/oauth/authorize", cfg.web_base_url))
        .expect("validated GitHub web base URL");
    url.query_pairs_mut()
        .append_pair("client_id", &cfg.oauth_client_id)
        .append_pair("redirect_uri", &cfg.oauth_callback_url)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "select_account");
    url.into()
}

/// Accept only a GitHub installation settings URL on the configured web
/// origin that names the requested installation, so a provider payload cannot
/// become an open redirect.
#[must_use]
pub fn validated_installation_management_url(
    cfg: &GithubAppConfig,
    candidate: &str,
    installation_id: u64,
) -> Option<String> {
    let candidate = Url::parse(candidate).ok()?;
    let web_base = Url::parse(&cfg.web_base_url).ok()?;
    let installation_id = installation_id.to_string();
    let same_origin = candidate.scheme() == web_base.scheme()
        && candidate.host_str() == web_base.host_str()
        && candidate.port_or_known_default() == web_base.port_or_known_default();
    let names_installation = candidate
        .path_segments()?
        .any(|segment| segment == installation_id);
    (same_origin
        && candidate.username().is_empty()
        && candidate.password().is_none()
        && candidate.fragment().is_none()
        && names_installation)
        .then(|| candidate.into())
}

// ---------------------------------------------------------------------------
// Installation token narrowing
// ---------------------------------------------------------------------------

use g5_scm::providers::{
    AppGrant, AppUserGrant, ConnectionOAuthConfig, InstallationGrant, ProviderClients,
    ProviderEndpoints, ProviderError, RepositoryCapability, RepositoryClient, RepositoryGrant,
    RepositoryIdentity,
};
pub use g5_scm::providers::{
    GithubAppAccessibleInstallation, GithubAppInstallation, GithubAppRepository,
    GithubAuthenticatedUser, GithubUserTokens, InstallationTokenRequest,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TokenCacheKey {
    installation_id: u64,
    request: InstallationTokenRequest,
}

// ---------------------------------------------------------------------------
// Provider records
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// GitHub App client. One per process; installation tokens are cached per
/// `(installation, repository_ids, permissions)`.
pub struct GithubAppClient {
    providers: ProviderClients,
    cfg: GithubAppConfig,
    tokens: InstallationTokenCache<TokenCacheKey>,
    #[cfg(feature = "test-helpers")]
    oauth_web_base_url: Option<String>,
}

impl std::fmt::Debug for GithubAppClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubAppClient")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl GithubAppClient {
    pub fn provider_clients() -> Result<ProviderClients, ProviderError> {
        ProviderClients::new()
    }

    pub fn new(providers: ProviderClients, cfg: GithubAppConfig) -> Self {
        Self {
            providers,
            cfg,
            tokens: InstallationTokenCache::new(),
            #[cfg(feature = "test-helpers")]
            oauth_web_base_url: None,
        }
    }
    /// Keep the repository's HTTPS identity while serving OAuth from a local fixture.
    #[cfg(feature = "test-helpers")]
    pub fn with_oauth_web_url_for_test(mut self, url: String) -> Self {
        self.oauth_web_base_url = Some(url);
        self
    }
    pub fn config(&self) -> &GithubAppConfig {
        &self.cfg
    }
    pub(crate) fn endpoints(&self) -> Result<ProviderEndpoints, G5Error> {
        ProviderEndpoints::new(
            g5_scm::ScmProvider::GitHub,
            &self.cfg.api_base_url,
            &self.cfg.web_base_url,
        )
        .map_err(Into::into)
    }
    fn app_protocol(&self) -> Result<g5_scm::providers::AppClient, G5Error> {
        Ok(self
            .providers
            .app(AppGrant::new(self.endpoints()?, self.mint_app_jwt()?)))
    }
    fn user_protocol(&self, token: &str) -> Result<g5_scm::providers::AppUserClient, G5Error> {
        Ok(self.providers.app_user(AppUserGrant::new(
            self.endpoints()?,
            self.cfg.app_id.clone(),
            token.to_owned(),
        )))
    }
    fn oauth_protocol(&self) -> Result<g5_scm::providers::ConnectionOAuthClient, G5Error> {
        let endpoints = self.endpoints()?;
        #[cfg(feature = "test-helpers")]
        let endpoints = match &self.oauth_web_base_url {
            Some(web) => {
                ProviderEndpoints::new(g5_scm::ScmProvider::GitHub, &self.cfg.api_base_url, web)?
            }
            None => endpoints,
        };
        Ok(self
            .providers
            .connection_oauth(ConnectionOAuthConfig::github_app(
                &endpoints,
                self.cfg.oauth_client_id.clone(),
                self.cfg.oauth_client_secret.clone(),
                self.cfg.oauth_callback_url.clone(),
            )?))
    }
    pub(crate) fn repository_protocol(
        &self,
        token: &str,
        repository: RepositoryIdentity,
        capability: RepositoryCapability,
    ) -> Result<RepositoryClient, ProviderError> {
        let endpoints = ProviderEndpoints::new(
            g5_scm::ScmProvider::GitHub,
            &self.cfg.api_base_url,
            &self.cfg.web_base_url,
        )?;
        Ok(self.providers.repository(RepositoryGrant::new(
            endpoints,
            repository,
            capability,
            token.to_owned(),
        )?))
    }

    /// Mint, or reuse a cached, installation token narrowed to `request`.
    pub async fn installation_token(
        &self,
        installation_id: u64,
        request: &InstallationTokenRequest,
    ) -> Result<String, G5Error> {
        let key = TokenCacheKey {
            installation_id,
            request: request.clone(),
        };
        if let Some(token) = self.tokens.get(&key).await {
            return Ok(token);
        }
        let issued = self
            .app_protocol()?
            .installation_token(installation_id, request)
            .await?;
        self.tokens.insert(key, &issued).await;
        Ok(issued.token)
    }

    pub async fn exchange_oauth_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<String, G5Error> {
        Ok(self
            .oauth_protocol()?
            .exchange_code(code, Some(code_verifier))
            .await?
            .access_token)
    }
    pub async fn exchange_user_grant_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<GithubUserTokens, G5Error> {
        self.oauth_protocol()?
            .exchange_code(code, Some(code_verifier))
            .await
            .map(app_token_lifetimes)
            .map_err(Into::into)
    }
    pub async fn refresh_user_token(
        &self,
        refresh_token: &str,
    ) -> Result<GithubUserTokens, GithubTokenRefreshError> {
        self.oauth_protocol()
            .map_err(GithubTokenRefreshError::Failed)?
            .refresh_token(refresh_token)
            .await
            .map(app_token_lifetimes)
            .map_err(|error| match error {
                g5_scm::providers::TokenRefreshError::Rejected { code } => {
                    GithubTokenRefreshError::Rejected { code }
                }
                g5_scm::providers::TokenRefreshError::Failed(error) => {
                    GithubTokenRefreshError::Failed(error.into())
                }
            })
    }
    pub async fn authenticated_user(
        &self,
        user_token: &str,
    ) -> Result<GithubAuthenticatedUser, G5Error> {
        self.user_protocol(user_token)?
            .authenticated_user()
            .await
            .map_err(Into::into)
    }
    pub async fn verify_user_repository_access(
        &self,
        user_token: &str,
        repository_full_name: &str,
        repository_id: u64,
    ) -> Result<(), GithubUserRepositoryAccessError> {
        use GithubUserRepositoryAccessError as E;
        self.user_protocol(user_token)
            .map_err(E::Failed)?
            .verify_repository_access(repository_full_name, repository_id)
            .await
            .map_err(|error| match error {
                g5_scm::providers::RepositoryAccessError::ConnectionRequired => {
                    E::ConnectionRequired
                }
                g5_scm::providers::RepositoryAccessError::AccessDenied => E::AccessDenied,
                g5_scm::providers::RepositoryAccessError::RepositoryChanged => E::RepositoryChanged,
                g5_scm::providers::RepositoryAccessError::Failed(error) => E::Failed(error.into()),
            })
    }
    pub async fn verify_user_installation_access(
        &self,
        user_token: &str,
        installation_id: u64,
    ) -> Result<(), G5Error> {
        self.user_protocol(user_token)?
            .verify_installation_access(installation_id)
            .await
            .map_err(Into::into)
    }
    pub async fn user_accessible_installations(
        &self,
        user_token: &str,
    ) -> Result<Vec<GithubAppAccessibleInstallation>, G5Error> {
        self.user_protocol(user_token)?
            .accessible_installations()
            .await
            .map_err(Into::into)
    }
    pub async fn installation_metadata(
        &self,
        installation_id: u64,
    ) -> Result<GithubAppInstallation, G5Error> {
        self.app_protocol()?
            .installation_metadata(installation_id)
            .await
            .map_err(Into::into)
    }
    pub async fn installation_repositories(
        &self,
        installation_id: u64,
    ) -> Result<Vec<GithubAppRepository>, G5Error> {
        // Selection and cache policy stay here; issuance and inventory are separate primitives.
        let request = InstallationTokenRequest {
            repository_ids: BTreeSet::new(),
            permissions: BTreeMap::from([("metadata".to_string(), "read".to_string())]),
            ..InstallationTokenRequest::default()
        };
        let token = self.installation_token(installation_id, &request).await?;
        self.providers
            .installation(InstallationGrant::new(
                self.endpoints()?,
                installation_id,
                token,
            ))
            .repositories()
            .await
            .map_err(Into::into)
    }

    fn mint_app_jwt(&self) -> Result<String, G5Error> {
        sign_app_jwt(&self.cfg.app_id, &self.cfg.private_key_pem).map_err(github_error)
    }
}

fn app_token_lifetimes(mut tokens: GithubUserTokens) -> GithubUserTokens {
    tokens.expires_in = tokens.expires_in.filter(|seconds| *seconds > 0);
    tokens.refresh_token_expires_in = tokens
        .refresh_token_expires_in
        .filter(|seconds| *seconds > 0);
    tokens
}

/// Why a user's GitHub authorization cannot access a repository.
#[derive(Debug)]
pub enum GithubUserRepositoryAccessError {
    ConnectionRequired,
    AccessDenied,
    RepositoryChanged,
    Failed(G5Error),
}

/// Why GitHub did not renew a user access token.
#[derive(Debug)]
pub enum GithubTokenRefreshError {
    /// GitHub answered with an OAuth `error` code (e.g. `bad_refresh_token`
    /// for a revoked or superseded grant, `incorrect_client_credentials` for
    /// a deployment misconfiguration).
    Rejected { code: Option<String> },
    /// The request failed or the answer could not be read.
    Failed(G5Error),
}

/// A redacted provider failure. It is `ServiceUnavailable` so request paths
/// report a retryable upstream failure without leaking credentials or URLs.
fn github_error(message: impl AsRef<str>) -> G5Error {
    G5Error::ServiceUnavailable {
        message: format!(
            "github app request failed: {}",
            redact_message(message.as_ref())
        ),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, body_string_contains, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Test-only RSA key (not a real credential).
    pub(crate) const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCh0zHjS9lHvZUa\nr5FnzZ+3ylJKauPP+oybsYej5xgurEOlabdXd5GIDMySwfZt06pNLoWTGIG6gKlp\nFVnm1aalXPK4U++cKpF0n5DZmuMPzqQhjYku+0LwkGS6UhkaJ5hazznrrTYW34eX\nGqwR58wIYCdhfB7yL/TzlNNOUJqZ156MsPty87azqT7tebw+CxDSfJx4GM88xJS/\n/8k5j4qaqlsaKjkcO4puKYeY5uym/M2gJqH69zpDcllsbnvjCNvEvZTvZpH84SyU\n7r63FdmYMVQwQynylTls9lhDIP7fSt9/GIHPLG99aCrabSg9271BW5gmurXz4hIp\nU9JwgmVZAgMBAAECggEAFem93ywVs6fB6ZetpnNLNf2RMB+SUsb4RFsBZZn1Bdjw\n3hTXLWJ1FAm7qAHY511audHbZ6RRaNpkVmT8RAxZYRoZiglfsHN5N+QlnrnhFnJi\nemKqmXCPy3iaEzrryEVlVaKrErVlViqVt0n21APh9p5ipspCfIQu9M8J4Bh9HujD\nDLOsz5v0wBEZSgMfkr7RO0/UOZBTqihgZMpG1ljUeE1w+MJW/HjhgqITow2ObkYT\nz5PQciLbwAT++C3dgUYnx4I7OShskHnFkizZHZceiByOBUnNs3VwKnfuQJ2XDXoB\nXZuunzkXboaHnjpIz/UOQ/XpehF7rfFCJWDhb7v2qQKBgQDW3WvQwyOYqJqu5SBG\nbhM+U7pRTbmeIvoPbGdB669RdBNXiSd2QfiWGw6tZtcBju1HciFHBjlPrTxtEkeW\n2wNf02fpJo6E+6PCfpZisGoAD54qVlvE49MaWPosfnSf7QOL9LgmnMmgpwKFtdcE\ndmIFHWueT9o3osn2uPA0zOfzlQKBgQDAzkYeSfZFaxPU0HnWkWcZWnQh50XfSr4k\n4yHOaGGTvW6MbH8N8io0VcVMtpQNFI43ZR1P0pfQKq1veEIbL95Wb/FHII//8K/G\nmaSqPuMASEmZwIYcDbOM21svhFmIYfxbus8BagodRK3YHzz85rwTH1TMBFcpXYsW\nam53rGs5tQKBgQDSTfNDaFylBRIx8U4/4WmwbZ9GCP+PSsja483Fdfwpi6WK3lHj\nq/q2YVFiY/A6ND0G2yy7KdpapEMqtuOfQkrxNijUycphWzQUIHbtraJy4NZ8tf4o\nnp13gnOLrVJVXlVEXYkJMhkfx5fua+dDFOQoAX3efGYoYouvk6B47b5yaQKBgHxI\nOvCpUoM626Sx4efLAH9aTEPSdbrSzjCu7IrFxChc0pbYFSpjMItdwnmJs3OfDXFt\nz3A12sBH0lZpWYt21sY3EHt95PThLB1Dyt4m/WKgNjlS3d/VgSjJ+URu9tuv9eEX\nUPqjZPOq4rjOohyPiyp0Zs+/+INklhST9LNVxCSFAoGAbTkP6swjzdAxW3ljzIAw\nZ9XR1bxnsYkNJzab9uvOHMB97nox9WqF3JV8Bdw1vZxwGpktzdF3ynBdlwLGw/9L\nMGqEAL0pDl8Xk+4CwsVAAabY92hHGM2i32SMSuOsYtdANKUDOpG63X/8BMPlPwzL\n+QNvFWc2w8Q0w3YIYjBeR80=\n-----END PRIVATE KEY-----\n";

    pub(crate) fn test_cfg(base_url: &str) -> GithubAppConfig {
        GithubAppConfig {
            app_id: "123456".to_string(),
            private_key_pem: TEST_KEY_PEM.to_string(),
            app_slug: "g5-test-app".to_string(),
            oauth_client_id: "Iv1.test-client".to_string(),
            oauth_client_secret: "oauth-client-secret".to_string(),
            oauth_callback_url: "https://g5.example.test/api/v1/github-app/oauth/callback"
                .to_string(),
            connection_state_secret: "state-signing-secret-at-least-thirty-two-bytes".to_string(),
            api_base_url: base_url.to_string(),
            web_base_url: base_url.to_string(),
        }
    }

    fn other_secret_cfg() -> GithubAppConfig {
        let mut cfg = test_cfg("https://github.example.test");
        cfg.connection_state_secret = "a-different-signing-secret-of-32-bytes-or-more".to_string();
        cfg
    }

    const NOW: i64 = 1_800_000_000;

    fn issued_state(cfg: &GithubAppConfig) -> (String, Uuid, Uuid) {
        let org = Uuid::new_v4();
        let attempt = Uuid::new_v4();
        (
            new_connection_state(cfg, org, attempt, NOW + 600),
            org,
            attempt,
        )
    }

    fn replace_segment(state: &str, index: usize, value: &str) -> String {
        let mut parts: Vec<String> = state.split('.').map(ToOwned::to_owned).collect();
        parts[index] = value.to_string();
        parts.join(".")
    }

    #[test]
    fn connection_state_round_trips_and_binds_digest() {
        let cfg = test_cfg("https://github.example.test");
        let (state, org, attempt) = issued_state(&cfg);
        let verified = verify_connection_state(&cfg, &state, NOW).expect("valid state");
        assert_eq!(verified.org_id, org);
        assert_eq!(verified.attempt_id, attempt);
        assert_eq!(verified.expires_at, NOW + 600);
        assert_eq!(verified.state_digest, connection_state_digest(&state));
        // Two states for the same attempt never share a digest (random nonce).
        let again = new_connection_state(&cfg, org, attempt, NOW + 600);
        assert_ne!(connection_state_digest(&again), verified.state_digest);
        assert!(!format!("{verified:?}").contains(&URL_SAFE_NO_PAD.encode(verified.state_digest)));
    }

    #[test]
    fn connection_state_rejects_tampering_expiry_and_foreign_keys() {
        let cfg = test_cfg("https://github.example.test");
        let (state, _, _) = issued_state(&cfg);

        // Moving the state to another org or attempt breaks the signature.
        let wrong_org = replace_segment(&state, 1, &Uuid::new_v4().to_string());
        assert!(verify_connection_state(&cfg, &wrong_org, NOW).is_none());
        let wrong_attempt = replace_segment(&state, 2, &Uuid::new_v4().to_string());
        assert!(verify_connection_state(&cfg, &wrong_attempt, NOW).is_none());
        // Extending expiry breaks the signature.
        let extended = replace_segment(&state, 3, &(NOW + 86_400).to_string());
        assert!(verify_connection_state(&cfg, &extended, NOW).is_none());
        // Flipped signature, truncated and extended forms are rejected.
        let bad_sig = replace_segment(&state, 5, &URL_SAFE_NO_PAD.encode([0_u8; 32]));
        assert!(verify_connection_state(&cfg, &bad_sig, NOW).is_none());
        assert!(verify_connection_state(&cfg, &format!("{state}.x"), NOW).is_none());
        assert!(verify_connection_state(&cfg, &state[..state.len() - 2], NOW).is_none());
        // A state signed with another deployment's secret is rejected.
        assert!(verify_connection_state(&other_secret_cfg(), &state, NOW).is_none());
        // Expired at and after the deadline.
        assert!(verify_connection_state(&cfg, &state, NOW + 600).is_none());
        assert!(verify_connection_state(&cfg, &state, NOW + 601).is_none());
    }

    #[test]
    fn oauth_states_are_domain_separated_and_tamper_evident() {
        let cfg = test_cfg("https://github.example.test");
        let (state, org, attempt) = issued_state(&cfg);
        let attempt_ref = verify_connection_state(&cfg, &state, NOW).unwrap();

        let installation = oauth_state_for_installation(&cfg, &state, 99).unwrap();
        assert_eq!(
            parse_oauth_state(&cfg, &installation, NOW),
            Some(GithubAppOAuthState::Installation {
                attempt: attempt_ref.clone(),
                installation_id: 99,
            })
        );
        // The installation id is bound to the signature.
        let swapped = installation.replace(".i.99.", ".i.100.");
        assert!(parse_oauth_state(&cfg, &swapped, NOW).is_none());
        assert!(oauth_state_for_installation(&cfg, &state, 0).is_none());
        // The raw attempt state is not itself an OAuth state.
        assert!(parse_oauth_state(&cfg, &state, NOW).is_none());

        let discovery = oauth_state_for_reconnect_discovery(&cfg, &state);
        assert_eq!(
            parse_oauth_state(&cfg, &discovery, NOW),
            Some(GithubAppOAuthState::ReconnectDiscovery {
                attempt: attempt_ref.clone()
            })
        );
        // A discovery signature cannot be replayed as an installation state.
        let (body, sig) = discovery.rsplit_once('.').unwrap();
        let forged = format!("{}.i.99.{sig}", body.strip_suffix(".r").unwrap());
        assert!(parse_oauth_state(&cfg, &forged, NOW).is_none());

        let selection = oauth_state_for_reconnect_selection(&cfg, &attempt_ref, 42).unwrap();
        assert!(!selection.contains(&state));
        let parsed = parse_oauth_state(&cfg, &selection, NOW).unwrap();
        assert_eq!(
            parsed,
            GithubAppOAuthState::ReconnectSelection {
                attempt: attempt_ref.clone(),
                installation_id: 42,
            }
        );
        assert_eq!(parsed.attempt().org_id, org);
        assert_eq!(parsed.attempt().attempt_id, attempt);
        let swapped = selection.replace(".42.", ".43.");
        assert!(parse_oauth_state(&cfg, &swapped, NOW).is_none());

        // Every form expires with the underlying attempt.
        for value in [&installation, &discovery, &selection] {
            assert!(parse_oauth_state(&cfg, value, NOW + 600).is_none());
            assert!(parse_oauth_state(&other_secret_cfg(), value, NOW).is_none());
        }
    }

    #[test]
    fn pkce_cookie_is_signed_and_bound_to_its_attempt() {
        let cfg = test_cfg("https://github.example.test");
        let attempt = connection_state_digest("attempt-a");
        let other_attempt = connection_state_digest("attempt-b");
        let (verifier, cookie) = new_pkce_cookie(&cfg, &attempt);
        assert_eq!(
            parse_pkce_cookie(&cfg, &cookie, &attempt),
            Some(verifier.clone())
        );
        assert!(parse_pkce_cookie(&cfg, &cookie, &other_attempt).is_none());
        assert!(parse_pkce_cookie(&cfg, &format!("{cookie}x"), &attempt).is_none());
        assert!(parse_pkce_cookie(&other_secret_cfg(), &cookie, &attempt).is_none());
        // Rebinding the digest without re-signing is rejected.
        let parts: Vec<&str> = cookie.split('.').collect();
        let rebound = format!(
            "{}.{}.{}.{}",
            parts[0],
            parts[1],
            URL_SAFE_NO_PAD.encode(other_attempt),
            parts[3]
        );
        assert!(parse_pkce_cookie(&cfg, &rebound, &other_attempt).is_none());
        assert_eq!(pkce_code_challenge(&verifier).len(), 43);
    }

    #[test]
    fn urls_are_built_from_configuration() {
        let cfg = test_cfg("https://github.example.test");
        let url = installation_url(&cfg, "g5ga1.a.b");
        assert_eq!(
            url,
            "https://github.example.test/apps/g5-test-app/installations/new?state=g5ga1.a.b"
        );
        let authorize = oauth_authorize_url(&cfg, "state", "challenge");
        assert!(authorize.starts_with("https://github.example.test/login/oauth/authorize?"));
        assert!(authorize.contains("code_challenge_method=S256"));
        assert!(!authorize.contains("oauth-client-secret"));

        assert_eq!(
            validated_installation_management_url(
                &cfg,
                "https://github.example.test/organizations/acme/settings/installations/11",
                11
            )
            .as_deref(),
            Some("https://github.example.test/organizations/acme/settings/installations/11")
        );
        for candidate in [
            "https://attacker.test/settings/installations/11",
            "https://github.example.test/settings/installations/12",
            "https://user:pw@github.example.test/settings/installations/11",
        ] {
            assert!(validated_installation_management_url(&cfg, candidate, 11).is_none());
        }
    }

    #[test]
    fn debug_redacts_secrets() {
        let cfg = test_cfg("https://github.example.test");
        let client = GithubAppClient::new(ProviderClients::new().unwrap(), cfg);
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("BEGIN PRIVATE KEY"));
        assert!(!rendered.contains("oauth-client-secret"));
        assert!(!rendered.contains("state-signing-secret"));
    }

    #[test]
    fn token_request_body_narrows_repositories_and_permissions() {
        let request = InstallationTokenRequest {
            repository_ids: BTreeSet::from([42, 7]),
            permissions: BTreeMap::from([("issues".to_string(), "write".to_string())]),
            ..InstallationTokenRequest::default()
        };
        assert_eq!(
            request.body(),
            json!({"repository_ids": [7, 42], "permissions": {"issues": "write"}})
        );
        assert_eq!(InstallationTokenRequest::default().body(), json!({}));
        // Report delivery narrows by name through the same issuance request.
        assert_eq!(
            InstallationTokenRequest::report_delivery("g5labs/platform-bugs")
                .unwrap()
                .body(),
            json!({"repositories": ["platform-bugs"], "permissions": {"issues": "write", "contents": "write"}})
        );
    }

    #[test]
    fn sign_app_jwt_produces_rs256_token_with_expected_claims() {
        use crate::services::identity::github_app_auth::sign_app_jwt_at;
        let token = sign_app_jwt_at("123456", TEST_KEY_PEM, 1_000_000).expect("signs");
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let decode = |seg: &str| URL_SAFE_NO_PAD.decode(seg).expect("base64url segment");
        let header: serde_json::Value = serde_json::from_slice(&decode(parts[0])).unwrap();
        assert_eq!(header["alg"], "RS256");
        let claims: serde_json::Value = serde_json::from_slice(&decode(parts[1])).unwrap();
        assert_eq!(claims["iss"], "123456");
        assert_eq!(claims["iat"], 999_940);
        assert_eq!(claims["exp"], 1_000_540);
        assert!(sign_app_jwt_at(
            "1",
            "-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----",
            1
        )
        .is_err());
    }

    #[tokio::test]
    async fn installation_token_sends_narrowed_body_and_caches_per_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/app/installations/77/access_tokens"))
            .and(body_json(
                json!({"repository_ids": [42], "permissions": {"issues": "read"}}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "token": "ghs_read",
                "expires_at": "2999-01-01T00:00:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/app/installations/77/access_tokens"))
            .and(body_json(
                json!({"repository_ids": [42], "permissions": {"issues": "write"}}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token": "ghs_write"})))
            .expect(1)
            .mount(&server)
            .await;

        let client = GithubAppClient::new(ProviderClients::new().unwrap(), test_cfg(&server.uri()));
        let read = InstallationTokenRequest {
            repository_ids: BTreeSet::from([42]),
            permissions: BTreeMap::from([("issues".to_string(), "read".to_string())]),
            ..InstallationTokenRequest::default()
        };
        let write = InstallationTokenRequest {
            permissions: BTreeMap::from([("issues".to_string(), "write".to_string())]),
            ..read.clone()
        };
        assert_eq!(
            client.installation_token(77, &read).await.unwrap(),
            "ghs_read"
        );
        assert_eq!(
            client.installation_token(77, &read).await.unwrap(),
            "ghs_read"
        );
        assert_eq!(
            client.installation_token(77, &write).await.unwrap(),
            "ghs_write"
        );
        assert_eq!(
            client.installation_token(77, &write).await.unwrap(),
            "ghs_write"
        );
    }

    #[tokio::test]
    async fn installation_token_failure_is_redacted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/app/installations/2/access_tokens"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                "Bad credentials at https://x-access-token:ghs_leak@api.github.com/app",
            ))
            .mount(&server)
            .await;
        let client = GithubAppClient::new(ProviderClients::new().unwrap(), test_cfg(&server.uri()));
        let err = client
            .installation_token(2, &InstallationTokenRequest::default())
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("401"));
        assert!(!msg.contains("ghs_leak"));
    }

    #[tokio::test]
    async fn oauth_exchange_verification_metadata_and_inventory() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains("code_verifier=verifier-value"))
            .and(body_string_contains("code=good-code"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"access_token": "ghu_user"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(body_string_contains("code=bad-code"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"error": "bad_verification_code"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/installations/11/repositories"))
            .and(header("authorization", "Bearer ghu_user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"repositories": []})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/installations/12/repositories"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/app/installations/11"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 11,
                "account": {"id": 44, "login": "acme"},
                "permissions": {"issues": "write", "metadata": "read"},
                "html_url": "https://github.example.test/organizations/acme/settings/installations/11"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/app/installations/11/access_tokens"))
            .and(body_json(json!({"permissions": {"metadata": "read"}})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"token": "ghs_meta"})))
            .mount(&server)
            .await;
        let page_one: Vec<serde_json::Value> = (1..=100)
            .map(|id| json!({"id": id, "full_name": format!("acme/r{id}"), "private": true}))
            .collect();
        Mock::given(method("GET"))
            .and(path("/installation/repositories"))
            .and(query_param("page", "1"))
            .and(header("authorization", "Bearer ghs_meta"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "link",
                        "<https://api.github.example.test/installation/repositories?page=2>; rel=\"next\"",
                    )
                    .set_body_json(json!({"repositories": page_one})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/installation/repositories"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "repositories": [{"id": 101, "full_name": "acme/last", "private": false}]
            })))
            .mount(&server)
            .await;

        let client = GithubAppClient::new(ProviderClients::new().unwrap(), test_cfg(&server.uri()));
        let token = client
            .exchange_oauth_code("good-code", "verifier-value")
            .await
            .unwrap();
        assert_eq!(token, "ghu_user");
        assert!(client
            .exchange_oauth_code("bad-code", "verifier-value")
            .await
            .is_err());
        client
            .verify_user_installation_access(&token, 11)
            .await
            .unwrap();
        assert!(client
            .verify_user_installation_access(&token, 12)
            .await
            .is_err());

        let installation = client.installation_metadata(11).await.unwrap();
        assert_eq!(installation.account_login, "acme");
        assert_eq!(
            installation.permissions.get("issues").map(String::as_str),
            Some("write")
        );
        assert!(client.installation_metadata(12).await.is_err());

        let repositories = client.installation_repositories(11).await.unwrap();
        assert_eq!(repositories.len(), 101);
        assert_eq!(repositories[100].full_name, "acme/last");
    }

    #[tokio::test]
    async fn reconnect_discovery_filters_to_configured_app() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user/installations"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "installations": [
                    {"id": 55, "app_id": 999999, "account": {"id": 1, "login": "other-app"}},
                    {"id": 12, "app_id": 123456, "account": {"id": 2, "login": "zeta"}},
                    {"id": 11, "app_id": 123456, "account": {"id": 3, "login": "alpha"}}
                ]
            })))
            .mount(&server)
            .await;
        let client = GithubAppClient::new(ProviderClients::new().unwrap(), test_cfg(&server.uri()));
        let installations = client.user_accessible_installations("ghu").await.unwrap();
        assert_eq!(
            installations,
            vec![
                GithubAppAccessibleInstallation {
                    installation_id: 11,
                    account_login: "alpha".to_string()
                },
                GithubAppAccessibleInstallation {
                    installation_id: 12,
                    account_login: "zeta".to_string()
                },
            ]
        );
    }
}
