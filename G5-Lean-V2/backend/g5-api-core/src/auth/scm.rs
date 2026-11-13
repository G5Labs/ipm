//! SCM (Source Code Management) token manager — AES-256-GCM encryption,
//! authenticated clone URLs, token refresh, and validity checking.
//!
//! This module implements the `ScmTokenManager` struct, which handles four
//! distinct concerns:
//!
//! 1. **Token encryption at rest** — AES-256-GCM authenticated encryption of
//!    SCM OAuth tokens before storage in Postgres
//!    (`user_scm_connections.access_token_enc`).
//! 2. **Token decryption** — Decrypting stored tokens when needed for clone
//!    operations, API calls, or token refresh.
//! 3. **Authenticated clone URL construction** — Building provider-specific
//!    HTTPS clone URLs that embed the decrypted token.
//! 4. **Token lifecycle management** — Refreshing expiring tokens (GitLab) and
//!    checking validity of stored tokens (GitHub) via provider APIs.
//!
//! This module does NOT interact with the database directly — database
//! reads/writes for `user_scm_connections` are handled by `AuthService` (in
//! `services/identity/auth_service.rs`).
//!
//! # Security
//!
//! - Tokens (access, refresh, encryption key) are NEVER logged.
//! - Clone URLs are NEVER logged (they embed plaintext tokens).
//! - Provider API error details are logged server-side but NOT returned to
//!   the client.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::{rngs::OsRng, RngCore};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use g5_scm::GitAuth;

use crate::errors::AppError;
use g5_contracts::error::G5Error;

/// Operator endpoints drive SCM API access and Git transport trust.
/// Invalid HTTPS origins disable transport credentials without fallback.
pub fn configured_scm_origin_policy(config: &crate::config::AuthConfig) -> g5_scm::ScmOriginPolicy {
    g5_scm::ScmOriginPolicy::from_gitlab_api_url(Some(config.scm_gitlab_api_url()))
        .with_github_web_url(&config.scm_github_web_base_url)
}

// ---------------------------------------------------------------------------
// TokenResolutionError
// ---------------------------------------------------------------------------

/// Failure modes for `ScmTokenManager::token_for_user`.
///
/// The integration worker (Phase 2 of the remote-repo-tracking work) maps
/// these variants to the `error_detail` column of `branch_sync_runs` so
/// operators can distinguish "owner removed from org" from "no SCM
/// connection on file" from "ciphertext failed to decrypt".
///
/// See `auth_identity_access.security_view.md` D13/D14/I20.
#[derive(Debug, Error)]
pub enum TokenResolutionError {
    /// The acting user is no longer a current member of the workspace's
    /// org. Maps to `branch_sync_runs.error_detail = 'owner_not_org_member'`.
    #[error("user {user_id} is not a current member of org {org_id}")]
    OwnerNotOrgMember { user_id: Uuid, org_id: Uuid },

    /// No `user_scm_connections` row exists for `(user_id, provider)`.
    /// Maps to `branch_sync_runs.error_detail = 'no_scm_connection'`.
    #[error("no SCM connection found for user {user_id} provider {provider}")]
    NoScmConnection { user_id: Uuid, provider: String },

    /// Stored ciphertext failed AES-GCM decryption (key rotation gone wrong,
    /// row corruption, etc.). Maps to `error_detail = 'decrypt_failed'`.
    #[error("SCM token decrypt failed: {0}")]
    DecryptFailed(String),

    /// The stored token expired and could not be renewed (no refresh grant,
    /// no refresh configuration, or the provider refused the refresh).
    #[error("SCM token for user {user_id} provider {provider} expired")]
    TokenExpired { user_id: Uuid, provider: String },

    /// Database query failed.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

impl From<TokenResolutionError> for AppError {
    fn from(err: TokenResolutionError) -> Self {
        match err {
            TokenResolutionError::OwnerNotOrgMember { .. } => {
                AppError::from_error(G5Error::Unauthorized)
            }
            TokenResolutionError::NoScmConnection { .. }
            | TokenResolutionError::TokenExpired { .. } => {
                AppError::from_error(G5Error::Unauthorized)
            }
            TokenResolutionError::DecryptFailed(msg) => {
                AppError::from_error(G5Error::Internal { message: msg })
            }
            TokenResolutionError::Database(e) => AppError::from_error(G5Error::Internal {
                message: format!("SCM token resolution DB error: {e}"),
            }),
        }
    }
}

/// Row shape used by the stored-token read path. Private.
#[derive(Debug, sqlx::FromRow)]
struct ScmConnectionRow {
    id: Uuid,
    access_token_enc: Vec<u8>,
    refresh_token_enc: Option<Vec<u8>>,
    token_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ---------------------------------------------------------------------------
// ScmIdentity (fine-grained PAT connect)
// ---------------------------------------------------------------------------

/// Identity + expiry extracted from a validated SCM token (fine-grained PAT).
///
/// Returned by [`ScmTokenManager::validate_pat`]; `AuthService::connect_scm_pat`
/// persists it into `user_scm_connections`. See `g5/g5-api/auth/scm` D8.
#[derive(Debug, Clone)]
pub struct ScmIdentity {
    /// Provider's numeric user id, as a string (matches
    /// `user_scm_connections.provider_user_id`).
    pub provider_user_id: String,
    /// Provider login/username (e.g. GitHub `login`).
    pub login: String,
    /// Token expiry parsed from the provider's expiry header, if present.
    /// `None` for tokens with no expiry. Stored in
    /// `user_scm_connections.token_expires_at`.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ---------------------------------------------------------------------------
// ScmTokenManager
// ---------------------------------------------------------------------------

/// Manages SCM provider tokens: encryption/decryption at rest, authenticated
/// clone URL construction, token refresh, and validity checking.
///
/// Holds a pre-initialized AES-256-GCM cipher (from the 256-bit KEK) and an
/// typed provider client for candidate validation.
#[derive(Clone)]
pub struct ScmTokenManager {
    origin_policy: g5_scm::ScmOriginPolicy,
    /// Pre-initialized AES-256-GCM cipher from the Key Encryption Key (KEK).
    cipher: Aes256Gcm,
    /// HTTP client for provider API calls (token refresh, validity checks).
    providers: Result<g5_scm::providers::ProviderClients, g5_scm::providers::ProviderError>,
    /// The SCM connection OAuth client that renews expiring GitLab tokens.
    gitlab_refresh: Option<g5_scm::providers::ConnectionOAuthConfig>,
}

impl ScmTokenManager {
    /// Creates a new `ScmTokenManager`.
    ///
    /// # Arguments
    ///
    /// - `encryption_key`: A 256-bit (32-byte) AES key used as the Key
    ///   Encryption Key (KEK). Loaded from a secret manager at startup.
    /// - `providers`: Typed provider construction result for making authorized
    ///   requests to provider APIs.
    ///
    /// The `encryption_key` is consumed and stored only within the
    /// `Aes256Gcm` cipher instance — the raw key bytes are NOT retained.
    #[must_use]
    pub fn new(
        encryption_key: [u8; 32],
        providers: Result<g5_scm::providers::ProviderClients, g5_scm::providers::ProviderError>,
    ) -> Self {
        let key = aes_gcm::aead::generic_array::GenericArray::from_slice(&encryption_key);
        let cipher = Aes256Gcm::new(key);
        Self {
            cipher,
            providers,
            origin_policy: g5_scm::ScmOriginPolicy::default(),
            gitlab_refresh: None,
        }
    }

    /// Attach the deployment policy before this manager is shared by services.
    pub fn with_origin_policy(mut self, policy: g5_scm::ScmOriginPolicy) -> Self {
        self.origin_policy = policy;
        self
    }

    /// Attach the GitLab SCM connection OAuth client used to renew expired
    /// stored tokens. Without it, an expired GitLab token is not usable.
    pub fn with_gitlab_refresh(
        mut self,
        config: Option<g5_scm::providers::ConnectionOAuthConfig>,
    ) -> Self {
        self.gitlab_refresh = config;
        self
    }

    /// Trusted origins used for provider inference and transport credentials.
    pub fn origin_policy(&self) -> &g5_scm::ScmOriginPolicy {
        &self.origin_policy
    }

    /// Materialize a credential only for a destination matching that policy.
    pub fn git_auth(&self, token: String, provider: &str, git_url: &str) -> GitAuth {
        match self
            .origin_policy
            .effective_provider(Some(provider), git_url)
        {
            Some(provider) => GitAuth::scm_token_for_provider(token, provider, git_url),
            None => GitAuth::None,
        }
    }

    /// Encrypts a plaintext SCM token using AES-256-GCM.
    ///
    /// Generates a random 96-bit (12-byte) nonce using `OsRng` and encrypts
    /// the plaintext. Returns the concatenation:
    /// `nonce (12 bytes) || ciphertext || tag (16 bytes)`.
    ///
    /// # Arguments
    ///
    /// - `plaintext`: The OAuth access token or refresh token string to
    ///   encrypt.
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<u8>)`: The encrypted blob (nonce + ciphertext + auth tag).
    /// - `Err(AppError)`: If encryption fails (should never happen with a
    ///   valid key).
    ///
    /// # Security
    ///
    /// - A fresh random nonce is generated for EVERY call via `OsRng`.
    /// - The plaintext token is NOT logged.
    pub fn encrypt_token(&self, plaintext: &str) -> Result<Vec<u8>, AppError> {
        // 1. Generate random 96-bit nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 2. Encrypt plaintext (AES-256-GCM produces ciphertext + 16-byte auth tag)
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| {
                tracing::error!("SCM token encryption failed: {}", e);
                AppError::from_error(G5Error::Internal {
                    message: "SCM token encryption failed".to_string(),
                })
            })?;

        // 3. Concatenate: nonce || ciphertext_with_tag
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypts an AES-256-GCM encrypted SCM token.
    ///
    /// Expects the input format:
    /// `nonce (12 bytes) || ciphertext || tag (16 bytes)`.
    ///
    /// # Arguments
    ///
    /// - `ciphertext`: The encrypted blob as stored in
    ///   `user_scm_connections.access_token_enc`.
    ///
    /// # Returns
    ///
    /// - `Ok(String)`: The decrypted plaintext token.
    /// - `Err(AppError)`: If the ciphertext is too short, decryption fails,
    ///   or the decrypted bytes are not valid UTF-8.
    ///
    /// # Security
    ///
    /// - The decrypted plaintext token is NOT logged.
    /// - AES-GCM authentication tag verification detects any tampering.
    pub fn decrypt_token(&self, ciphertext: &[u8]) -> Result<String, AppError> {
        // 1. Validate minimum length: 12 (nonce) + 16 (tag) = 28 bytes minimum
        if ciphertext.len() < 28 {
            return Err(AppError::from_error(G5Error::Internal {
                message: "SCM token ciphertext too short".to_string(),
            }));
        }

        // 2. Split nonce and ciphertext+tag
        let (nonce_bytes, ct_and_tag) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // 3. Decrypt and verify authentication tag
        let plaintext_bytes = self.cipher.decrypt(nonce, ct_and_tag).map_err(|e| {
            tracing::error!("SCM token decryption failed: {}", e);
            AppError::from_error(G5Error::Internal {
                message: "SCM token decryption failed".to_string(),
            })
        })?;

        // 4. Convert to UTF-8 string
        String::from_utf8(plaintext_bytes).map_err(|e| {
            tracing::error!("SCM token is not valid UTF-8: {}", e);
            AppError::from_error(G5Error::Internal {
                message: "SCM token decryption produced invalid UTF-8".to_string(),
            })
        })
    }

    /// Resolves a `GitAuth::ScmToken` for a specific (user, org, provider) tuple,
    /// with org-membership safety.
    ///
    /// This is the consolidated path that background SCM consumers (the
    /// new `branch_integration_worker` from Phase 2 of the remote-repo-tracking
    /// work, the `remote_advisory_pinger` from Phase 3, etc.) MUST use per
    /// [`auth_identity_access.security_view.md` I20](../../../views/security_views/auth_identity_access.security_view.md).
    ///
    /// # Steps (Phase 0)
    ///
    /// 0. Reject an unsupported or mismatched HTTPS origin with `GitAuth::None`.
    /// 1. Verify `(org_id, user_id)` exists in `org_memberships` per security
    ///    view D14 — rejects stale workspace owners who were removed from
    ///    the org.
    /// 2. Look up `user_scm_connections WHERE user_id = $1 AND provider = $2`.
    /// 3. Decrypt the stored access-token ciphertext via `decrypt_token`.
    /// 4. Return a provider-bound `GitAuth::ScmToken`.
    ///
    /// # Phase 2 extensions (NOT IN THIS IMPLEMENTATION)
    ///
    /// - Refresh serialization per `(provider, user_id)` (security view D17).
    ///   GitLab tokens with `token_expires_at < now()` currently return
    ///   `GitAuth::ScmToken` regardless; the provider's 401 on the eventual
    ///   git operation is what catches expiry. Phase 2 adds proactive
    ///   refresh with a Postgres advisory lock per security view Q6.
    /// - Required-scope filtering. Caller MUST verify scopes at workspace
    ///   create time using `check_token_validity`.
    ///
    /// # Errors
    ///
    /// Returns a typed [`TokenResolutionError`] that the caller maps to
    /// `branch_sync_runs.error_detail`. See variant docs for the
    /// audit-row column values.
    ///
    /// # Security
    ///
    /// - The plaintext token is never logged.
    /// - The returned `GitAuth::ScmToken` has a redacting `Debug` impl per
    ///   `g5-common/src/git/auth.rs`. Callers MUST pass it to the `git2`
    ///   credentials callback via `GitAuth::configure_callbacks`; they
    ///   MUST NOT serialize it into URLs, error strings, or process
    ///   arguments.
    pub async fn token_for_user(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        org_id: Uuid,
        provider: &str,
        git_url: &str,
    ) -> Result<GitAuth, TokenResolutionError> {
        if crate::auth::effective_provider(Some(self), Some(provider), git_url).is_none() {
            return Ok(GitAuth::None);
        }
        // Step 1: Verify org membership (security view D14).
        let is_member: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM org_memberships
                WHERE org_id = $1 AND user_id = $2 AND deactivated_at IS NULL
            )
            "#,
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        if !is_member {
            return Err(TokenResolutionError::OwnerNotOrgMember { user_id, org_id });
        }

        // Steps 2-3: the one stored-token read path (refreshing GitLab).
        let token = self.access_token(pool, user_id, provider).await?;

        // Step 4: Return GitAuth::ScmToken (whose Debug impl redacts).
        Ok(self.git_auth(token, provider, git_url))
    }

    /// The one read path for a user's stored SCM access token, shared by
    /// Git transport and API callers. An expired GitLab token with a refresh
    /// grant is renewed and stored; any other expired token is refused rather
    /// than handed to a provider. A refresh is one provider exchange; failure
    /// is reported, never retried here.
    pub async fn access_token(
        &self,
        pool: &PgPool,
        user_id: Uuid,
        provider: &str,
    ) -> Result<String, TokenResolutionError> {
        let row = sqlx::query_as::<_, ScmConnectionRow>(
            r#"
            SELECT id, access_token_enc, refresh_token_enc, token_expires_at
            FROM user_scm_connections
            WHERE user_id = $1 AND provider = $2
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(provider)
        .fetch_optional(pool)
        .await?;
        let Some(conn) = row else {
            return Err(TokenResolutionError::NoScmConnection {
                user_id,
                provider: provider.to_string(),
            });
        };
        if conn
            .token_expires_at
            .is_some_and(|expires_at| expires_at < chrono::Utc::now())
        {
            let expired = || TokenResolutionError::TokenExpired {
                user_id,
                provider: provider.to_string(),
            };
            let refreshable = g5_scm::ScmProvider::from_name(provider)
                .is_some_and(|p| matches!(p, g5_scm::ScmProvider::GitLab));
            let Some(refresh_token_enc) = conn.refresh_token_enc.filter(|_| refreshable) else {
                return Err(expired());
            };
            return self
                .refresh_gitlab_token(pool, conn.id, &refresh_token_enc)
                .await
                .map_err(|reason| {
                    tracing::warn!(%user_id, provider, reason, "SCM token refresh failed; token expired");
                    expired()
                });
        }
        self.decrypt_token(&conn.access_token_enc)
            .map_err(|e| TokenResolutionError::DecryptFailed(format!("decrypt: {}", e.inner)))
    }

    /// Renew one GitLab connection and store the rotated grant. The returned
    /// error is a server-side diagnostic only.
    async fn refresh_gitlab_token(
        &self,
        pool: &PgPool,
        connection_id: Uuid,
        refresh_token_enc: &[u8],
    ) -> Result<String, String> {
        let config = self
            .gitlab_refresh
            .clone()
            .ok_or("no GitLab SCM OAuth configuration")?;
        let providers = self.providers.as_ref().map_err(ToString::to_string)?;
        let refresh_token = self
            .decrypt_token(refresh_token_enc)
            .map_err(|_| "refresh token decrypt failed".to_string())?;
        let renewed = providers
            .connection_oauth(config)
            .refresh_token(&refresh_token)
            .await
            .map_err(|error| match error {
                g5_scm::providers::TokenRefreshError::Failed(error) => error.to_string(),
                g5_scm::providers::TokenRefreshError::Rejected { .. } => {
                    "the provider refused the refresh; reconnect the account".to_string()
                }
            })?;
        let access_token_enc = self
            .encrypt_token(&renewed.access_token)
            .map_err(|_| "token encryption failed".to_string())?;
        let refresh_token_enc = renewed
            .refresh_token
            .as_deref()
            .map(|token| self.encrypt_token(token))
            .transpose()
            .map_err(|_| "token encryption failed".to_string())?;
        let expires_at = renewed
            .expires_in
            .map(|secs| chrono::Utc::now() + chrono::Duration::seconds(secs));
        sqlx::query(
            r#"
            UPDATE user_scm_connections
            SET access_token_enc = $2,
                refresh_token_enc = $3,
                token_expires_at = $4,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(connection_id)
        .bind(&access_token_enc)
        .bind(refresh_token_enc.as_deref())
        .bind(expires_at)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to store the renewed token: {error}"))?;
        Ok(renewed.access_token)
    }

    /// Validates a user-supplied fine-grained Personal Access Token (GitHub)
    /// by calling `GET {api_base_url}/user`, returning the token's identity and
    /// expiry. This is the connect-time check that replaces the SCM OAuth App
    /// code exchange (see `g5/g5-api/auth/scm` D8 and
    /// `designs/API_AND_STORAGE.md` §3.11.2).
    ///
    /// `api_base_url` is config-driven (`G5_SCM_GITHUB_API_BASE_URL`, default
    /// `https://api.github.com`) so dev/CI can point validation at a hermetic
    /// stub instead of real github.com.
    ///
    /// # Returns
    ///
    /// - `Ok(ScmIdentity)` on 2xx — `provider_user_id`, `login`, and optional
    ///   `expires_at` parsed from the `github-authentication-token-expiration`
    ///   response header.
    /// - `Err(AppError)` — `G5Error::Unauthorized` on 401 (the route maps this
    ///   to `400 invalid_token`); `G5Error::Internal` on transport/parse
    ///   failures or other non-2xx statuses.
    ///
    /// # Security
    ///
    /// - The `token` is NEVER logged.
    pub async fn validate_pat(
        &self,
        provider: &str,
        api_base_url: &str,
        token: &str,
    ) -> Result<ScmIdentity, AppError> {
        let provider =
            g5_scm::ScmProvider::from_name(provider).ok_or_else(|| G5Error::ValidationError {
                details: serde_json::json!({"field":"provider","error":"Unsupported SCM provider"}),
            })?;
        let providers = self
            .providers
            .as_ref()
            .map_err(|error| G5Error::from(error.clone()))?;
        let grant =
            g5_scm::providers::CandidateGrant::new(provider, api_base_url, token.to_owned())
                .map_err(G5Error::from)?;
        let identity = providers
            .candidate(grant)
            .validate()
            .await
            .map_err(|error| {
                if error.status == Some(401) {
                    G5Error::Unauthorized
                } else {
                    G5Error::from(error)
                }
            })?;
        Ok(ScmIdentity {
            provider_user_id: identity.provider_user_id,
            login: identity.login,
            expires_at: identity.expires_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: creates a `ScmTokenManager` with a known key for testing.
    fn test_manager() -> ScmTokenManager {
        let key = [0x42u8; 32]; // deterministic test key
        ScmTokenManager::new(key, g5_scm::providers::ProviderClients::new())
    }

    // -- Constructor tests --

    #[test]
    fn test_new_creates_manager() {
        let manager = test_manager();
        // Verify the manager was created successfully by performing a
        // round-trip encrypt/decrypt.
        let plaintext = "test_token";
        let encrypted = manager.encrypt_token(plaintext).unwrap();
        let decrypted = manager.decrypt_token(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    // -- Encryption tests --

    #[test]
    fn test_encrypt_token_returns_nonce_plus_ciphertext() {
        let manager = test_manager();
        let encrypted = manager.encrypt_token("my_secret_token").unwrap();

        // Minimum: 12 (nonce) + 1 (at least 1 byte plaintext) + 16 (tag) = 29
        assert!(
            encrypted.len() >= 29,
            "Encrypted output too short: {}",
            encrypted.len()
        );

        // Verify it starts with 12 nonce bytes (we can't predict them, but
        // the length must be at least 12 + plaintext_len + 16)
        let expected_min = 12 + "my_secret_token".len() + 16;
        assert_eq!(encrypted.len(), expected_min);
    }

    #[test]
    fn test_encrypt_token_unique_nonces() {
        let manager = test_manager();
        let enc1 = manager.encrypt_token("same_token").unwrap();
        let enc2 = manager.encrypt_token("same_token").unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(
            enc1, enc2,
            "Two encryptions of the same plaintext should differ (unique nonces)"
        );

        // But both should decrypt to the same plaintext
        let dec1 = manager.decrypt_token(&enc1).unwrap();
        let dec2 = manager.decrypt_token(&enc2).unwrap();
        assert_eq!(dec1, "same_token");
        assert_eq!(dec2, "same_token");
    }

    #[test]
    fn test_encrypt_empty_string() {
        let manager = test_manager();
        let encrypted = manager.encrypt_token("").unwrap();
        // 12 (nonce) + 0 (empty plaintext) + 16 (tag) = 28
        assert_eq!(encrypted.len(), 28);

        let decrypted = manager.decrypt_token(&encrypted).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_long_token() {
        let manager = test_manager();
        let long_token = "a".repeat(1000);
        let encrypted = manager.encrypt_token(&long_token).unwrap();
        let decrypted = manager.decrypt_token(&encrypted).unwrap();
        assert_eq!(decrypted, long_token);
    }

    // -- Decryption tests --

    #[test]
    fn test_decrypt_roundtrip() {
        let manager = test_manager();
        let original = "gho_abc123xyz789_test_token";
        let encrypted = manager.encrypt_token(original).unwrap();
        let decrypted = manager.decrypt_token(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_decrypt_too_short() {
        let manager = test_manager();

        // Less than 28 bytes should fail
        let short = vec![0u8; 27];
        let result = manager.decrypt_token(&short);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err.inner {
            G5Error::Internal { message } => {
                assert_eq!(message, "SCM token ciphertext too short");
            }
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    #[test]
    fn test_decrypt_empty_slice() {
        let manager = test_manager();
        let result = manager.decrypt_token(&[]);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err.inner {
            G5Error::Internal { message } => {
                assert_eq!(message, "SCM token ciphertext too short");
            }
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    #[test]
    fn test_decrypt_tampered_ciphertext() {
        let manager = test_manager();
        let encrypted = manager.encrypt_token("secret").unwrap();

        // Tamper with a byte in the ciphertext portion (after the nonce)
        let mut tampered = encrypted.clone();
        if tampered.len() > 13 {
            tampered[13] ^= 0xFF;
        }

        let result = manager.decrypt_token(&tampered);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err.inner {
            G5Error::Internal { message } => {
                assert_eq!(message, "SCM token decryption failed");
            }
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let manager1 = test_manager(); // key = [0x42; 32]
        let manager2 =
            ScmTokenManager::new([0x99u8; 32], g5_scm::providers::ProviderClients::new());

        let encrypted = manager1.encrypt_token("secret").unwrap();
        let result = manager2.decrypt_token(&encrypted);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match &err.inner {
            G5Error::Internal { message } => {
                assert_eq!(message, "SCM token decryption failed");
            }
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    // -- Clone URL tests --

    // -- Error mapping tests --

    #[test]
    fn test_encrypt_error_is_internal() {
        // We can't easily trigger an encryption failure with a valid key,
        // but we can verify the error mapping pattern by testing decrypt
        // failures.
        let manager = test_manager();
        let result = manager.decrypt_token(&[0u8; 10]);
        let err = result.unwrap_err();
        match &err.inner {
            G5Error::Internal { .. } => {} // expected
            other => panic!("Expected G5Error::Internal, got {:?}", other),
        }
    }

    // -- Ciphertext format verification --

    #[test]
    fn test_ciphertext_format_nonce_ciphertext_tag() {
        let manager = test_manager();
        let plaintext = "test";
        let encrypted = manager.encrypt_token(plaintext).unwrap();

        // Format: nonce (12) || ciphertext (plaintext.len()) || tag (16)
        assert_eq!(encrypted.len(), 12 + plaintext.len() + 16);

        // The first 12 bytes are the nonce
        let nonce = &encrypted[..12];
        // Nonce should not be all zeros (extremely unlikely with OsRng)
        assert!(
            nonce.iter().any(|&b| b != 0),
            "Nonce should not be all zeros"
        );
    }

    // -- TokenResolutionError tests --

    #[test]
    fn test_token_resolution_error_owner_not_member_maps_to_unauthorized() {
        let err = TokenResolutionError::OwnerNotOrgMember {
            user_id: uuid::Uuid::nil(),
            org_id: uuid::Uuid::nil(),
        };
        let app_err: AppError = err.into();
        match &app_err.inner {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[test]
    fn test_token_resolution_error_no_connection_maps_to_unauthorized() {
        let err = TokenResolutionError::NoScmConnection {
            user_id: uuid::Uuid::nil(),
            provider: "github".to_string(),
        };
        let app_err: AppError = err.into();
        match &app_err.inner {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[test]
    fn test_token_resolution_error_decrypt_maps_to_internal() {
        let err = TokenResolutionError::DecryptFailed("test".to_string());
        let app_err: AppError = err.into();
        match &app_err.inner {
            G5Error::Internal { message } => {
                assert!(message.contains("test"));
            }
            other => panic!("Expected Internal, got {:?}", other),
        }
    }

    #[test]
    fn test_token_resolution_error_display_includes_context() {
        let err = TokenResolutionError::OwnerNotOrgMember {
            user_id: uuid::Uuid::nil(),
            org_id: uuid::Uuid::nil(),
        };
        let display = format!("{}", err);
        assert!(display.contains("not a current member"));
    }

    // -- validate_pat tests (fine-grained PAT connect) --

    #[tokio::test]
    async fn test_validate_pat_success_parses_identity_and_expiry() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .and(header("authorization", "Bearer test_pat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "github-authentication-token-expiration",
                        "2099-12-31 23:59:59 UTC",
                    )
                    .set_body_json(serde_json::json!({"id": 424242, "login": "octo-dev"})),
            )
            .mount(&server)
            .await;

        let manager = test_manager();
        let identity = manager
            .validate_pat("github", &server.uri(), "test_pat")
            .await
            .expect("valid token should succeed");

        assert_eq!(identity.provider_user_id, "424242");
        assert_eq!(identity.login, "octo-dev");
        assert_eq!(
            identity
                .expires_at
                .expect("expiry header should parse")
                .to_rfc3339(),
            "2099-12-31T23:59:59+00:00"
        );
    }

    #[tokio::test]
    async fn test_validate_pat_401_maps_to_unauthorized() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let err = test_manager()
            .validate_pat("github", &server.uri(), "bad_token")
            .await
            .expect_err("401 should error");
        match &err.inner {
            G5Error::Unauthorized => {}
            other => panic!("Expected Unauthorized, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_validate_pat_missing_expiry_header_is_none() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"id": 7, "login": "no-expiry"})),
            )
            .mount(&server)
            .await;

        let identity = test_manager()
            .validate_pat("github", &server.uri(), "tok")
            .await
            .unwrap();
        assert_eq!(identity.login, "no-expiry");
        assert!(identity.expires_at.is_none());
    }
}
