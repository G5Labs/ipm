//! Shared OCI Distribution v2 bearer-token auth for DockerHub and GHCR (D15).
//!
//! Both registries follow the same protocol: unauthenticated request → 401
//! with `Www-Authenticate: Bearer realm=...,service=...,scope=...` → token
//! request to `<realm>?service=...&scope=...` → retry with
//! `Authorization: Bearer <jwt>`. The `CredentialProvider` trait is the only
//! per-registry seam: it supplies optional Basic-auth credentials for the
//! token endpoint and maps a post-token 401 to a registry-appropriate typed
//! error. `DockerHubCredentials` reads `DOCKER_USERNAME` / `DOCKER_PASSWORD`;
//! `GhcrCredentials` resolves the GHCR priority chain (D17). Tokens are
//! cached per `(registry_host, scope)` in `TokenCache` (D19) and the mint is
//! singleflighted by `DistributionClient`'s refresh lock (D21).
//!
//! The cache is in-process only — token leakage to disk is explicitly avoided.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;

use super::error::{truncate_body, OciError};

#[derive(Clone, Debug)]
struct CachedToken {
    token: String,
    expires_at: Instant,
}

/// Composite cache key. Each `DistributionClient` already owns its own
/// `TokenCache` (per-registry), so in practice `registry_host` is constant per
/// cache instance — but composing it into the key is defense in depth: if a
/// future refactor ever shares one cache across registries, identical scope
/// strings (e.g. `repository:owner/img:pull`) for different hosts will no
/// longer collide. Repository is already encoded inside `scope` per the OCI
/// Distribution challenge format, so `(host, scope)` is the full
/// `(registry, repository, scope)` tuple requested by the review.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct TokenKey {
    registry_host: String,
    scope: String,
}

/// Per-`DistributionClient` token cache, keyed by `(registry_host, scope)`.
///
/// `std::sync::Mutex` (not `tokio::sync::Mutex`): the critical section is
/// purely synchronous (`HashMap` get/put) and is never held across `.await` —
/// tokio's docs explicitly recommend `std::sync::Mutex` for that pattern.
/// See https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html#which-kind-of-mutex.
/// If a future refactor needs to await while the lock is held (e.g. an inline
/// singleflight refresh), switch to `tokio::sync::Mutex` then.
pub struct TokenCache {
    inner: Mutex<HashMap<TokenKey, CachedToken>>,
}

impl Default for TokenCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, registry_host: &str, scope: &str) -> Option<String> {
        let map = self.inner.lock().ok()?;
        let key = TokenKey {
            registry_host: registry_host.to_string(),
            scope: scope.to_string(),
        };
        let entry = map.get(&key)?;
        if entry.expires_at > Instant::now() {
            Some(entry.token.clone())
        } else {
            None
        }
    }

    pub fn put(&self, registry_host: String, scope: String, token: String, expires_in_secs: u64) {
        // Apply a 30s safety margin so a token cached right before its
        // expiry boundary still validates on the request that follows.
        let ttl_secs = expires_in_secs.saturating_sub(30).max(30);
        let entry = CachedToken {
            token,
            expires_at: Instant::now() + Duration::from_secs(ttl_secs),
        };
        if let Ok(mut map) = self.inner.lock() {
            map.insert(
                TokenKey {
                    registry_host,
                    scope,
                },
                entry,
            );
        }
    }

    /// Remove a cached entry. Called by `DistributionClient::authed_get` when
    /// the cached token was just rejected with a 401 inside the singleflight
    /// double-check — eviction guarantees the next `put` cannot collide with a
    /// stale equal-keyed entry and that subsequent same-scope callers cannot
    /// observe the failed token before the fresh mint completes.
    ///
    /// A no-op if the key is absent. `PoisonError` is silently dropped (the
    /// cache is best-effort; a poisoned mutex is treated as a cache miss
    /// everywhere else too).
    pub fn invalidate(&self, registry_host: &str, scope: &str) {
        if let Ok(mut map) = self.inner.lock() {
            let key = TokenKey {
                registry_host: registry_host.to_string(),
                scope: scope.to_string(),
            };
            map.remove(&key);
        }
    }

    /// Test-only: directly insert an already-expired entry, bypassing the 30s
    /// TTL floor that `put` applies. Used by the wiremock expiry-refresh
    /// regression test to force a cache miss on the second authed_get without
    /// having to mock `Instant`.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn force_insert_expired(&self, registry_host: String, scope: String, token: String) {
        let entry = CachedToken {
            token,
            // 1s in the past — strictly less than Instant::now() at every
            // observation point in the get() check below.
            expires_at: Instant::now() - Duration::from_secs(1),
        };
        if let Ok(mut map) = self.inner.lock() {
            map.insert(
                TokenKey {
                    registry_host,
                    scope,
                },
                entry,
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthChallenge {
    pub realm: String,
    pub service: String,
    pub scope: String,
}

pub fn parse_challenge(header: &str) -> Result<AuthChallenge, OciError> {
    let trimmed = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or_else(|| {
            // The Www-Authenticate header is registry-controlled and can in
            // principle be arbitrarily long (or contain control chars);
            // truncate_body bounds + sanitizes so a misbehaving registry can't
            // spill a giant header into operator-facing error logs.
            OciError::AuthFailed(
                "challenge".to_string(),
                format!("not a Bearer challenge: '{}'", truncate_body(header)),
            )
        })?;

    let mut realm: Option<String> = None;
    let mut service: Option<String> = None;
    let mut scope: Option<String> = None;

    for (key, value) in split_pairs(trimmed) {
        match key.as_str() {
            "realm" => realm = Some(value),
            "service" => service = Some(value),
            "scope" => scope = Some(value),
            _ => {}
        }
    }

    Ok(AuthChallenge {
        realm: realm.ok_or_else(|| {
            OciError::AuthFailed("challenge".to_string(), "missing realm".to_string())
        })?,
        service: service.unwrap_or_default(),
        scope: scope.unwrap_or_default(),
    })
}

fn split_pairs(input: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_quotes = false;
    let mut start: usize = 0;
    for (i, c) in input.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                push_pair(&input[start..i], &mut out);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    push_pair(&input[start..], &mut out);
    out
}

fn push_pair(slice: &str, out: &mut Vec<(String, String)>) {
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some((k, v)) = trimmed.split_once('=') {
        let key = k.trim().to_string();
        let val = v.trim().trim_matches('"').to_string();
        out.push((key, val));
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    /// GHCR (and Docker's token endpoint) include `issued_at` in their token
    /// response. We model the field so the struct documents its presence, but
    /// intentionally do not consume it: TTL is sourced from `expires_in` and
    /// the cache's 30s safety margin, both of which are independent of the
    /// server clock's `issued_at`. Reading `issued_at` would require trusting
    /// the registry's clock vs. ours, which serves no purpose here.
    #[serde(default, rename = "issued_at")]
    _issued_at: Option<String>,
}

pub async fn fetch_token(
    client: &reqwest::Client,
    challenge: &AuthChallenge,
    creds: Option<(&str, &str)>,
) -> Result<(String, u64), OciError> {
    tracing::debug!(
        realm = %challenge.realm,
        service = %challenge.service,
        scope = %challenge.scope,
        authenticated = creds.is_some(),
        "Requesting OCI bearer token"
    );
    let mut req = client.get(&challenge.realm).query(&[
        ("service", challenge.service.as_str()),
        ("scope", challenge.scope.as_str()),
    ]);
    if let Some((user, pass)) = creds {
        req = req.basic_auth(user, Some(pass));
    }

    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(OciError::AuthFailed(
            challenge.realm.clone(),
            format!(
                "token endpoint returned {}: {}",
                status,
                truncate_body(&body)
            ),
        ));
    }

    let parsed: TokenResponse = resp.json().await?;
    let token = parsed.token.or(parsed.access_token).ok_or_else(|| {
        OciError::AuthFailed(
            challenge.realm.clone(),
            "token endpoint returned neither 'token' nor 'access_token'".to_string(),
        )
    })?;
    let expires_in = parsed.expires_in.unwrap_or(300);
    tracing::debug!(
        realm = %challenge.realm,
        expires_in,
        "Received OCI bearer token"
    );
    Ok((token, expires_in))
}

/// Read an environment variable, treating an empty value as unset.
///
/// Used by both the DockerHub and GHCR credential providers so that
/// `VAR=` (explicitly empty) is observationally identical to `VAR` unset —
/// an empty username/password would otherwise be silently sent to the
/// registry's token endpoint and produce a confusing 401.
fn env_opt(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

pub fn env_credentials() -> Option<(String, String)> {
    let user = env_opt("DOCKER_USERNAME")?;
    let pass = env_opt("DOCKER_PASSWORD")?;
    Some((user, pass))
}

/// Username every Google-style token-flow registry accepts when only an
/// access token is known. For GHCR / GitHub Apps installation tokens, GHCR
/// accepts any non-empty username for a classic PAT; `oauth2accesstoken` is
/// the conventional placeholder. For GAR (the second Google-issued OAuth2
/// access-token registry, see [`super::gar`]) this is the canonical
/// `docker login` username. Hoisting the constant here keeps the two
/// callers in sync.
pub(crate) const OAUTH2_ACCESS_TOKEN_USERNAME: &str = "oauth2accesstoken";

/// The `Authorization` scheme the shared `DistributionClient` applies to
/// `/v2/...` requests for a given registry (D15).
///
/// `Bearer` for the OCI-Distribution-token registries (DockerHub, GHCR, GAR,
/// ACR); `Basic` for ECR, whose `/v2/` API authenticates with a base64
/// `AWS:<password>` credential rather than a discovered bearer token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthScheme {
    Bearer,
    Basic,
}

impl AuthScheme {
    /// The `Authorization` header scheme token (`"Bearer"` / `"Basic"`).
    pub fn header_prefix(self) -> &'static str {
        match self {
            AuthScheme::Bearer => "Bearer",
            AuthScheme::Basic => "Basic",
        }
    }
}

/// Per-registry credential acquisition + denial-error mapping (D15). The shared
/// `DistributionClient` (registry.rs) is registry-agnostic; everything that
/// differs between DockerHub, GHCR, GAR, ECR, and ACR is funnelled through
/// this trait. The cached token *value* (whatever `mint` returns) is applied
/// under `scheme()`; `pre_authenticates()` controls whether the core mints it
/// eagerly (before the first request) or lazily (on a 401).
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// Registry host, used for error context.
    fn registry(&self) -> &str;

    /// The `Authorization` scheme used on `/v2/...` requests. `Bearer` for the
    /// token-flow registries (default); `Basic` for ECR.
    fn scheme(&self) -> AuthScheme {
        AuthScheme::Bearer
    }

    /// When `true`, the shared core mints + applies a token on the *first*
    /// request rather than waiting for a 401. ECR sets this because its `/v2/`
    /// 401 challenge is `Basic` (no bearer realm to fetch a token from), so the
    /// core authenticates eagerly. Default `false` (lazy bearer challenge).
    fn pre_authenticates(&self) -> bool {
        false
    }

    /// Synchronous Basic-auth `(username, password/token)` for the standard
    /// bearer *token endpoint*, or `None` for an anonymous token request.
    /// Consulted only by the default `mint`; cloud registries that resolve
    /// credentials asynchronously override `mint` instead and leave this at
    /// its `None` default.
    fn credentials(&self) -> Option<(String, String)> {
        None
    }

    /// Mint a fresh token *value* (+ `expires_in` seconds) for the image at
    /// `registry_host` / `repository` / `scope`. The returned value is cached
    /// per `(host, scope)` by the core and applied under `scheme()`. The
    /// default is the standard OCI Distribution v2 flow — `GET
    /// <realm>?service=&scope=` with the optional `credentials()` Basic pair.
    /// GAR/ECR/ACR override this because their token acquisition is
    /// asynchronous (metadata server, SigV4-signed AWS API, AAD oauth2 dance
    /// respectively); `registry_host` is the seam through which ECR derives its
    /// AWS region (the Bearer registries read everything they need from
    /// `challenge`).
    async fn mint(
        &self,
        http: &reqwest::Client,
        challenge: Option<&AuthChallenge>,
        _registry_host: &str,
        _repository: &str,
        _scope: &str,
    ) -> Result<(String, u64), OciError> {
        let challenge = challenge.ok_or_else(|| {
            OciError::AuthFailed(
                self.registry().to_string(),
                "a Bearer challenge is required to mint a token but none was presented".to_string(),
            )
        })?;
        let creds = self.credentials();
        let creds_ref = creds.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
        fetch_token(http, challenge, creds_ref).await
    }

    /// Map a `401` that persists *after* a freshly minted token into a typed
    /// error. The default treats it as a generic auth failure tagged with the
    /// provider's static `registry()` label (DockerHub is single-host);
    /// registries with a distinct "no credentials for a private image" signal
    /// override this (GHCR keeps its `ghcr.io` constant; the cloud registries
    /// use the per-request `registry_host` since one client serves every host
    /// of a family).
    fn on_denied(&self, _registry_host: &str, repository: &str, scope: &str) -> OciError {
        OciError::AuthFailed(
            self.registry().to_string(),
            format!(
                "401 returned even after a fresh token for repository '{}' (scope '{}')",
                repository, scope
            ),
        )
    }
}

/// DockerHub credential source: optional `DOCKER_USERNAME` / `DOCKER_PASSWORD`
/// for private images, anonymous otherwise. Preserves PR-rev7-5a behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct DockerHubCredentials;

impl CredentialProvider for DockerHubCredentials {
    fn credentials(&self) -> Option<(String, String)> {
        env_credentials()
    }

    fn registry(&self) -> &str {
        super::registry::DOCKERHUB_API
    }
}

/// GHCR credential source. Resolves credentials in priority order (D17):
/// injected config → `GHCR_TOKEN` (with optional `GHCR_USERNAME`) → `GITHUB_TOKEN` → anonymous.
#[derive(Debug, Default, Clone)]
pub struct GhcrCredentials {
    /// Tier 1: credentials injected by the daemon configuration layer (e.g.
    /// from a `[oci.ghcr]` config block). The OCI module exposes this
    /// injection point; parsing the config file into it is the daemon
    /// config layer's job (deferred to the isolation/daemon wire-in).
    configured: Option<(String, String)>,
}

impl GhcrCredentials {
    /// Resolve from environment only (no injected configuration).
    pub fn from_env() -> Self {
        Self { configured: None }
    }

    /// Inject configured credentials (tier 1). `None` falls through to the
    /// environment tiers.
    pub fn with_configured(configured: Option<(String, String)>) -> Self {
        Self { configured }
    }
}

impl CredentialProvider for GhcrCredentials {
    fn credentials(&self) -> Option<(String, String)> {
        if let Some(creds) = &self.configured {
            return Some(creds.clone());
        }
        resolve_ghcr_env(env_opt)
    }

    fn registry(&self) -> &str {
        super::registry::GHCR_API
    }

    fn on_denied(&self, _registry_host: &str, repository: &str, scope: &str) -> OciError {
        // GHCR is single-host; keep reporting the canonical `ghcr.io` label
        // (not the per-request host, which is the mock server under test).
        ghcr_denial(self.credentials().is_some(), repository, scope)
    }
}

/// Map a post-token GHCR `401` to a typed error based on whether any
/// credentials were available. Pure (no env / no `self`) so both branches are
/// deterministically unit-testable.
///
/// - `had_credentials == false` → `AuthRequired` (anonymous pull of a private
///   image; the operator must supply credentials).
/// - `had_credentials == true` → `AuthFailed` (credentials were offered but
///   GHCR rejected them).
fn ghcr_denial(had_credentials: bool, repository: &str, scope: &str) -> OciError {
    if had_credentials {
        OciError::AuthFailed(
            super::registry::GHCR_API.to_string(),
            format!(
                "GHCR rejected the supplied credentials for repository '{}' (scope '{}')",
                repository, scope
            ),
        )
    } else {
        OciError::AuthRequired {
            registry: super::registry::GHCR_API.to_string(),
            image: repository.to_string(),
            scope: scope.to_string(),
        }
    }
}

/// Pure GHCR environment-credential resolver (tiers 2–3 of D17), written
/// over a key→value getter so it is unit-testable without mutating the real
/// process environment.
///
/// - `GHCR_TOKEN` present → `(GHCR_USERNAME or oauth2accesstoken, GHCR_TOKEN)`
/// - else `GITHUB_TOKEN` present → `(GITHUB_ACTOR or oauth2accesstoken, GITHUB_TOKEN)`
/// - else `None` (anonymous)
pub(crate) fn resolve_ghcr_env<F>(get: F) -> Option<(String, String)>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(token) = get("GHCR_TOKEN") {
        let user = get("GHCR_USERNAME").unwrap_or_else(|| OAUTH2_ACCESS_TOKEN_USERNAME.to_string());
        return Some((user, token));
    }
    if let Some(token) = get("GITHUB_TOKEN") {
        let user = get("GITHUB_ACTOR").unwrap_or_else(|| OAUTH2_ACCESS_TOKEN_USERNAME.to_string());
        return Some((user, token));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dockerhub_challenge_with_quotes() {
        let header = r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull""#;
        let c = parse_challenge(header).expect("parses");
        assert_eq!(c.realm, "https://auth.docker.io/token");
        assert_eq!(c.service, "registry.docker.io");
        assert_eq!(c.scope, "repository:library/alpine:pull");
    }

    #[test]
    fn parse_challenge_lowercase_scheme() {
        let header = r#"bearer realm="https://auth.example/token",service="ex""#;
        let c = parse_challenge(header).expect("parses lowercase");
        assert_eq!(c.realm, "https://auth.example/token");
        assert_eq!(c.service, "ex");
        assert_eq!(c.scope, "");
    }

    #[test]
    fn parse_challenge_rejects_non_bearer() {
        let header = "Basic realm=\"x\"";
        let err = parse_challenge(header).expect_err("non-bearer must fail");
        assert!(matches!(err, OciError::AuthFailed(_, _)));
    }

    #[test]
    fn token_cache_returns_within_ttl() {
        let cache = TokenCache::new();
        cache.put("ghcr.io".into(), "sc".into(), "abc".into(), 1_000);
        assert_eq!(cache.get("ghcr.io", "sc").as_deref(), Some("abc"));
    }

    #[test]
    fn token_cache_misses_unknown() {
        let cache = TokenCache::new();
        assert!(cache.get("ghcr.io", "nothing-cached").is_none());
    }

    #[test]
    fn token_cache_applies_ttl_floor_for_short_expiry() {
        // Even a token reported with expires_in below the 30s safety margin is
        // cached for the 30s floor (not evicted instantly), so the immediately
        // following request reuses it.
        let cache = TokenCache::new();
        cache.put("ghcr.io".into(), "short".into(), "tok".into(), 0);
        assert_eq!(cache.get("ghcr.io", "short").as_deref(), Some("tok"));
        cache.put("ghcr.io".into(), "tiny".into(), "tok2".into(), 5);
        assert_eq!(cache.get("ghcr.io", "tiny").as_deref(), Some("tok2"));
    }

    #[test]
    fn token_cache_isolates_same_scope_across_registries() {
        // Same scope string for two registries must NOT cross-pollute. Each
        // DistributionClient already owns its own cache; the composite key is
        // defense in depth so a future shared cache stays correct.
        let cache = TokenCache::new();
        let scope = "repository:owner/img:pull";
        cache.put("ghcr.io".into(), scope.into(), "ghcr-token".into(), 1_000);
        cache.put(
            "registry-1.docker.io".into(),
            scope.into(),
            "dockerhub-token".into(),
            1_000,
        );
        assert_eq!(cache.get("ghcr.io", scope).as_deref(), Some("ghcr-token"));
        assert_eq!(
            cache.get("registry-1.docker.io", scope).as_deref(),
            Some("dockerhub-token")
        );
    }

    #[test]
    fn token_cache_force_insert_expired_misses() {
        // The test-only force_insert_expired bypasses the 30s floor so
        // wiremock tests can prove the refresh path runs on expiry.
        let cache = TokenCache::new();
        cache.force_insert_expired("ghcr.io".into(), "sc".into(), "stale".into());
        assert!(cache.get("ghcr.io", "sc").is_none());
    }

    // -- GHCR challenge parsing (D16) --

    #[test]
    fn parse_ghcr_challenge() {
        let header = r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:octocat/hello-world:pull""#;
        let c = parse_challenge(header).expect("parses GHCR challenge");
        assert_eq!(c.realm, "https://ghcr.io/token");
        assert_eq!(c.service, "ghcr.io");
        assert_eq!(c.scope, "repository:octocat/hello-world:pull");
    }

    #[test]
    fn parse_challenge_missing_realm_errors() {
        let header = r#"Bearer service="ghcr.io",scope="repository:owner/img:pull""#;
        let err = parse_challenge(header).expect_err("missing realm must fail");
        assert!(matches!(err, OciError::AuthFailed(_, _)));
        assert!(err.to_string().contains("missing realm"));
    }

    // -- GHCR token-response parsing (D16/D20) --

    #[test]
    fn token_response_parses_ghcr_shape() {
        // GHCR returns token + expires_in + issued_at; issued_at is ignored.
        let json = r#"{"token":"jwt-abc","expires_in":600,"issued_at":"2026-05-29T00:00:00Z"}"#;
        let parsed: TokenResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(parsed.token.as_deref(), Some("jwt-abc"));
        assert_eq!(parsed.expires_in, Some(600));
    }

    #[test]
    fn token_response_defaults_missing_expiry_at_call_site() {
        // The struct leaves expires_in as None; fetch_token applies the 300s
        // default. Here we assert the struct decodes with the field absent.
        let json = r#"{"token":"jwt-no-exp"}"#;
        let parsed: TokenResponse = serde_json::from_str(json).expect("parses");
        assert_eq!(parsed.token.as_deref(), Some("jwt-no-exp"));
        assert_eq!(parsed.expires_in, None);
        assert_eq!(parsed.expires_in.unwrap_or(300), 300);
    }

    #[test]
    fn token_response_accepts_access_token_alias() {
        let json = r#"{"access_token":"jwt-alias","expires_in":300}"#;
        let parsed: TokenResponse = serde_json::from_str(json).expect("parses");
        assert!(parsed.token.is_none());
        assert_eq!(parsed.access_token.as_deref(), Some("jwt-alias"));
    }

    // -- GHCR credential resolution (D17) --

    fn map_getter<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn ghcr_env_prefers_ghcr_username_token() {
        let creds = resolve_ghcr_env(map_getter(&[
            ("GHCR_USERNAME", "octocat"),
            ("GHCR_TOKEN", "pat-1"),
            ("GITHUB_TOKEN", "should-be-ignored"),
        ]))
        .expect("resolves");
        assert_eq!(creds, ("octocat".to_string(), "pat-1".to_string()));
    }

    #[test]
    fn ghcr_token_without_username_defaults() {
        let creds = resolve_ghcr_env(map_getter(&[("GHCR_TOKEN", "pat-2")])).expect("resolves");
        assert_eq!(
            creds,
            ("oauth2accesstoken".to_string(), "pat-2".to_string())
        );
    }

    #[test]
    fn ghcr_falls_back_to_github_token_with_actor() {
        let creds = resolve_ghcr_env(map_getter(&[
            ("GITHUB_TOKEN", "gh-tok"),
            ("GITHUB_ACTOR", "ci-bot"),
        ]))
        .expect("resolves");
        assert_eq!(creds, ("ci-bot".to_string(), "gh-tok".to_string()));
    }

    #[test]
    fn ghcr_github_token_without_actor_defaults() {
        let creds =
            resolve_ghcr_env(map_getter(&[("GITHUB_TOKEN", "gh-tok-2")])).expect("resolves");
        assert_eq!(
            creds,
            ("oauth2accesstoken".to_string(), "gh-tok-2".to_string())
        );
    }

    #[test]
    fn ghcr_anonymous_when_no_env() {
        assert!(resolve_ghcr_env(map_getter(&[])).is_none());
    }

    #[test]
    fn ghcr_configured_credentials_take_priority() {
        let provider =
            GhcrCredentials::with_configured(Some(("cfg-user".into(), "cfg-token".into())));
        // configured tier short-circuits before any env lookup.
        assert_eq!(
            provider.credentials(),
            Some(("cfg-user".to_string(), "cfg-token".to_string()))
        );
        assert_eq!(provider.registry(), "ghcr.io");
    }

    #[test]
    fn ghcr_denial_anonymous_is_auth_required() {
        match ghcr_denial(false, "owner/img", "repository:owner/img:pull") {
            OciError::AuthRequired {
                registry,
                image,
                scope,
            } => {
                assert_eq!(registry, "ghcr.io");
                assert_eq!(image, "owner/img");
                assert_eq!(scope, "repository:owner/img:pull");
            }
            other => panic!("expected AuthRequired, got {:?}", other),
        }
    }

    #[test]
    fn ghcr_denial_with_credentials_is_auth_failed() {
        match ghcr_denial(true, "owner/img", "repository:owner/img:pull") {
            OciError::AuthFailed(reg, msg) => {
                assert_eq!(reg, "ghcr.io");
                assert!(msg.contains("rejected the supplied credentials"));
            }
            other => panic!("expected AuthFailed, got {:?}", other),
        }
    }

    #[test]
    fn dockerhub_credentials_registry_host() {
        let provider = DockerHubCredentials;
        assert_eq!(provider.registry(), "registry-1.docker.io");
    }
}
