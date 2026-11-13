//! Microsoft Entra ID platform login: verified tenant/object identity only.
//!
//! Email claims are display hints, never account-linking or admission proof.
//! Secrets and provider response bodies are never logged.

use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm, KeyOperations, PublicKeyUse};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::errors::{AppError, G5Error};

const KEY_TTL: Duration = Duration::from_secs(3600);
const KEY_REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const CLOCK_TOLERANCE_SECS: u64 = 60;

#[must_use]
pub fn default_authorize_url(tenant_id: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/authorize")
}

#[must_use]
pub fn default_token_url(tenant_id: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token")
}

#[must_use]
pub fn default_jwks_url(tenant_id: Uuid) -> String {
    format!("https://login.microsoftonline.com/{tenant_id}/discovery/v2.0/keys")
}

/// Debug never reveals credentials returned by the code exchange.
#[derive(Clone)]
pub struct EntraTokens {
    pub access_token: String,
    pub id_token: Option<String>,
}

impl std::fmt::Debug for EntraTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntraTokens")
            .field("access_token", &"[REDACTED]")
            .field("id_token", &self.id_token.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

/// Constructed only after ID-token verification.
#[derive(Debug)]
pub struct EntraIdentity {
    pub tenant_id: Uuid,
    pub object_id: Uuid,
    pub email_hint: Option<String>,
    pub display_name: String,
}

impl EntraIdentity {
    /// Canonical, unambiguous persisted Microsoft identity; never the OIDC sub.
    #[must_use]
    pub fn oauth_id(&self) -> String {
        format!("{}:{}", self.tenant_id, self.object_id)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    id_token: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct IdTokenClaims {
    iss: String,
    aud: String,
    tid: Uuid,
    oid: Uuid,
    nonce: String,
    email: Option<String>,
    name: Option<String>,
    preferred_username: Option<String>,
    upn: Option<String>,
    unique_name: Option<String>,
}

#[derive(Default)]
struct SigningKeys {
    keys: Option<JwkSet>,
    fetched_at: Option<Instant>,
    last_attempt: Option<Instant>,
}

/// A configured directory owns one bounded, shared cache of its signing keys.
#[derive(Clone)]
pub struct EntraClient {
    http: reqwest::Client,
    tenant_id: Uuid,
    client_id: String,
    issuer: String,
    jwks_url: String,
    signing_keys: Arc<Mutex<SigningKeys>>,
}

impl EntraClient {
    #[must_use]
    pub fn new(tenant_id: Uuid, client_id: String, jwks_url: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("static Entra HTTP client configuration is valid");
        Self {
            http,
            tenant_id,
            client_id,
            issuer: format!("https://login.microsoftonline.com/{tenant_id}/v2.0"),
            jwks_url: jwks_url.unwrap_or_else(|| default_jwks_url(tenant_id)),
            signing_keys: Arc::default(),
        }
    }

    /// Exchanges an authorization code (with its PKCE verifier) for tokens.
    ///
    /// # Errors
    /// `Unauthorized` when Entra refuses the code; `Internal` on transport or
    /// malformed responses.
    pub async fn exchange_code(
        &self,
        token_url: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
        code: &str,
        verifier: &str,
    ) -> Result<EntraTokens, AppError> {
        if code.trim().is_empty() || verifier.trim().is_empty() {
            return Err(G5Error::Unauthorized.into());
        }
        let form = [
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ];
        let response = self
            .http
            .post(token_url)
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .map_err(|error| {
                tracing::error!(error = %redact_error(&error), "Microsoft token request failed");
                G5Error::Internal {
                    message: "Failed to exchange Microsoft authorization code".to_string(),
                }
            })?;
        let status = response.status();
        let parsed: TokenResponse = response.json().await.map_err(|_| {
            tracing::error!(%status, "Microsoft token response was not JSON");
            G5Error::Internal {
                message: "Failed to parse Microsoft token response".to_string(),
            }
        })?;
        if !status.is_success() || parsed.error.is_some() {
            return Err(G5Error::Unauthorized.into());
        }
        match parsed.access_token.filter(|token| !token.trim().is_empty()) {
            Some(access_token) => Ok(EntraTokens {
                access_token,
                id_token: parsed.id_token.filter(|token| !token.trim().is_empty()),
            }),
            None => {
                tracing::warn!(
                    %status,
                    "Microsoft refused the authorization code"
                );
                Err(G5Error::Unauthorized.into())
            }
        }
    }

    /// Verify all identity proof before exposing any payload claim to login.
    pub async fn validate_identity(
        &self,
        id_token: Option<&str>,
        expected_nonce: &str,
    ) -> Result<EntraIdentity, AppError> {
        let id_token = id_token.ok_or(G5Error::Unauthorized)?;
        if expected_nonce.is_empty() {
            return Err(G5Error::Unauthorized.into());
        }
        // Only the untrusted header is read to select a key from our own JWKS.
        // Neither jku/x5u nor any payload can select an endpoint or algorithm.
        let header = decode_header(id_token).map_err(|_| G5Error::Unauthorized)?;
        if header.alg != Algorithm::RS256 {
            return Err(G5Error::Unauthorized.into());
        }
        let kid = header
            .kid
            .filter(|id| !id.is_empty())
            .ok_or(G5Error::Unauthorized)?;
        let jwk = self.signing_key(&kid).await?;
        if jwk
            .common
            .key_algorithm
            .is_some_and(|alg| alg != KeyAlgorithm::RS256)
            || jwk
                .common
                .public_key_use
                .as_ref()
                .is_some_and(|usage| *usage != PublicKeyUse::Signature)
            || jwk
                .common
                .key_operations
                .as_ref()
                .is_some_and(|ops| !ops.contains(&KeyOperations::Verify))
        {
            return Err(G5Error::Unauthorized.into());
        }
        let key = DecodingKey::from_jwk(&jwk).map_err(|_| G5Error::Unauthorized)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&[&self.issuer]);
        validation.set_required_spec_claims(&["iss", "aud", "exp", "nbf"]);
        validation.validate_nbf = true;
        validation.leeway = CLOCK_TOLERANCE_SECS;
        let claims = decode::<IdTokenClaims>(id_token, &key, &validation)
            .map_err(|_| G5Error::Unauthorized)?
            .claims;
        if claims.iss != self.issuer
            || claims.aud != self.client_id
            || claims.tid != self.tenant_id
            || claims.oid.is_nil()
            || claims.nonce != expected_nonce
        {
            return Err(G5Error::Unauthorized.into());
        }
        let email_hint = [
            claims.email,
            claims.preferred_username,
            claims.upn,
            claims.unique_name,
        ]
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_lowercase())
        .find(|value| value.contains('@'));
        let display_name = claims
            .name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| email_hint.clone())
            .unwrap_or_else(|| claims.oid.to_string());
        Ok(EntraIdentity {
            tenant_id: claims.tid,
            object_id: claims.oid,
            email_hint,
            display_name,
        })
    }

    async fn signing_key(&self, kid: &str) -> Result<Jwk, AppError> {
        // Serialize the fetch as well as cache replacement. Concurrent unknown
        // kids, upstream failure and cancellation cannot create a refresh storm.
        let mut cache = self.signing_keys.lock().await;
        if cache.fetched_at.is_some_and(|at| at.elapsed() < KEY_TTL) {
            if let Some(key) = cache.keys.as_ref().and_then(|keys| keys.find(kid)) {
                return Ok(key.clone());
            }
        }
        if cache
            .last_attempt
            .is_some_and(|at| at.elapsed() < KEY_REFRESH_INTERVAL)
        {
            return Err(G5Error::Unauthorized.into());
        }
        cache.last_attempt = Some(Instant::now());
        let keys = self
            .http
            .get(&self.jwks_url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| key_fetch_error())?
            .json::<JwkSet>()
            .await
            .map_err(|_| key_fetch_error())?;
        cache.keys = Some(keys);
        cache.fetched_at = Some(Instant::now());
        cache
            .keys
            .as_ref()
            .and_then(|keys| keys.find(kid))
            .cloned()
            .ok_or_else(|| G5Error::Unauthorized.into())
    }
}

fn key_fetch_error() -> G5Error {
    G5Error::Internal {
        message: "Failed to fetch Microsoft signing keys".to_string(),
    }
}

fn redact_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_decode() {
        "decode"
    } else {
        "request"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/entra/mod.rs"
        ));
    }

    fn client(server: &MockServer) -> EntraClient {
        EntraClient::new(
            Uuid::parse_str(fixture::TENANT).unwrap(),
            fixture::CLIENT.into(),
            Some(format!("{}/keys", server.uri())),
        )
    }

    #[tokio::test]
    async fn validated_pair_does_not_require_an_email_hint() {
        let server = MockServer::start().await;
        fixture::keys(&server, &[1]).await;
        let client = client(&server);
        let mut claims = fixture::claims("nonce");
        claims.as_object_mut().unwrap().remove("email");
        let identity = client
            .validate_identity(Some(&fixture::signed(&claims, 1)), "nonce")
            .await
            .unwrap();
        assert_eq!(
            identity.oauth_id(),
            format!("{}:{}", fixture::TENANT, fixture::OBJECT)
        );
        assert!(identity.email_hint.is_none());
        claims["preferred_username"] = json!(" User@Example.COM ");
        let identity = client
            .validate_identity(Some(&fixture::signed(&claims, 1)), "nonce")
            .await
            .unwrap();
        assert_eq!(identity.email_hint.as_deref(), Some("user@example.com"));
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "keys are cached"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_signature_claims_and_missing_proof() {
        let server = MockServer::start().await;
        fixture::keys(&server, &[1]).await;
        let client = client(&server);
        let now = jsonwebtoken::get_current_timestamp();
        for (name, value) in [
            ("iss", json!("https://attacker.example/v2.0")),
            ("tid", json!("11111111-2222-4333-8444-555555555555")),
            ("aud", json!("another-client")),
            (
                "iss",
                json!([format!(
                    "https://login.microsoftonline.com/{}/v2.0",
                    fixture::TENANT
                )]),
            ),
            ("aud", json!([fixture::CLIENT, "another-client"])),
            ("exp", json!(now - 120)),
            ("nbf", json!(now + 120)),
            ("nonce", json!("other-attempt")),
            ("nonce", json!("")),
            ("oid", json!("not-an-object-id")),
            ("oid", json!(Uuid::nil().to_string())),
        ] {
            let mut claims = fixture::claims("nonce");
            claims[name] = value;
            assert!(
                client
                    .validate_identity(Some(&fixture::signed(&claims, 1)), "nonce")
                    .await
                    .is_err(),
                "{name}: {claims}"
            );
        }
        for name in ["iss", "tid", "oid", "aud", "exp", "nbf", "nonce"] {
            let mut claims = fixture::claims("nonce");
            claims.as_object_mut().unwrap().remove(name);
            assert!(
                client
                    .validate_identity(Some(&fixture::signed(&claims, 1)), "nonce")
                    .await
                    .is_err(),
                "missing {name}"
            );
        }
        let good = fixture::signed(&fixture::claims("nonce"), 1);
        let other = fixture::signed(&fixture::claims("nonce"), 2);
        let forged = format!(
            "{}.{}",
            good.rsplit_once('.').unwrap().0,
            other.rsplit_once('.').unwrap().1
        );
        assert!(client
            .validate_identity(Some(&forged), "nonce")
            .await
            .is_err());
        assert!(client.validate_identity(None, "nonce").await.is_err());
        assert!(client
            .validate_identity(Some("not-a-token"), "nonce")
            .await
            .is_err());
        assert!(client.validate_identity(Some(&good), "").await.is_err());
        assert!(client.validate_identity(Some(&good), "nonce").await.is_ok());
    }

    #[tokio::test]
    async fn rejects_algorithm_confusion_before_fetching_keys() {
        let server = MockServer::start().await;
        let client = client(&server);
        let mut header = jsonwebtoken::Header::new(Algorithm::HS256);
        header.kid = Some("key-1".into());
        let token = jsonwebtoken::encode(
            &header,
            &fixture::claims("nonce"),
            &jsonwebtoken::EncodingKey::from_secret(b"attacker-secret"),
        )
        .unwrap();
        assert!(client
            .validate_identity(Some(&token), "nonce")
            .await
            .is_err());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn key_cache_handles_rotation_expiry_and_failed_refresh() {
        let server = MockServer::start().await;
        fixture::keys(&server, &[1]).await;
        let client = client(&server);
        let first = fixture::signed(&fixture::claims("nonce"), 1);
        let rotated = fixture::signed(&fixture::claims("nonce"), 2);
        client
            .validate_identity(Some(&first), "nonce")
            .await
            .unwrap();
        server.reset().await;
        fixture::keys(&server, &[1, 2]).await;
        // Unknown keys cannot make every callback fetch. Once the bounded
        // refresh interval elapses, rotation succeeds without waiting for TTL.
        assert!(client
            .validate_identity(Some(&rotated), "nonce")
            .await
            .is_err());
        assert!(server.received_requests().await.unwrap().is_empty());
        client.signing_keys.lock().await.last_attempt = Some(Instant::now() - KEY_REFRESH_INTERVAL);
        let (a, b) = tokio::join!(
            client.validate_identity(Some(&rotated), "nonce"),
            client.validate_identity(Some(&rotated), "nonce")
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        server.reset().await;
        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;
        {
            let mut cache = client.signing_keys.lock().await;
            cache.fetched_at = Some(Instant::now() - KEY_TTL);
            cache.last_attempt = Some(Instant::now() - KEY_REFRESH_INTERVAL);
        }
        assert!(
            client
                .validate_identity(Some(&first), "nonce")
                .await
                .is_err(),
            "expired cache fails closed"
        );
        assert!(
            client
                .validate_identity(Some(&first), "nonce")
                .await
                .is_err(),
            "failed fetch is throttled"
        );
        server.verify().await;
        server.reset().await;
        fixture::keys(&server, &[2]).await;
        client.signing_keys.lock().await.last_attempt = Some(Instant::now() - KEY_REFRESH_INTERVAL);
        client
            .validate_identity(Some(&rotated), "nonce")
            .await
            .unwrap();
        assert!(
            client
                .validate_identity(Some(&first), "nonce")
                .await
                .is_err(),
            "removed keys are discarded"
        );
    }

    #[tokio::test]
    async fn cancelled_key_fetch_releases_lock_and_can_recover() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"keys": [fixture::jwk(1)]}))
                    .set_delay(Duration::from_secs(20)),
            )
            .mount(&server)
            .await;
        let client = client(&server);
        let token = fixture::signed(&fixture::claims("nonce"), 1);
        let pending = {
            let client = client.clone();
            let token = token.clone();
            tokio::spawn(async move { client.validate_identity(Some(&token), "nonce").await })
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            while server.received_requests().await.unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        assert!(client
            .validate_identity(Some(&token), "nonce")
            .await
            .is_err());
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        server.reset().await;
        fixture::keys(&server, &[1]).await;
        client.signing_keys.lock().await.last_attempt = Some(Instant::now() - KEY_REFRESH_INTERVAL);
        client
            .validate_identity(Some(&token), "nonce")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn code_exchange_requires_success_and_redacts_secrets() {
        let server = MockServer::start().await;
        let client = client(&server);
        for (status, body, succeeds) in [
            (
                200,
                json!({"access_token":"secret-access", "id_token":"secret-id"}),
                true,
            ),
            (
                400,
                json!({"access_token":"secret-access", "id_token":"secret-id"}),
                false,
            ),
            (
                200,
                json!({"error":"invalid_grant", "access_token":"secret-access"}),
                false,
            ),
            (200, json!({"access_token":""}), false),
        ] {
            server.reset().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(wiremock::matchers::body_string_contains(
                    "code_verifier=verifier",
                ))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
            let result = client
                .exchange_code(
                    &format!("{}/token", server.uri()),
                    fixture::CLIENT,
                    "secret",
                    "https://g5.example/callback",
                    "code",
                    "verifier",
                )
                .await;
            assert_eq!(result.is_ok(), succeeds);
            if let Ok(tokens) = result {
                let debug = format!("{tokens:?}");
                assert!(!debug.contains("secret-access"));
                assert!(!debug.contains("secret-id"));
            }
            server.verify().await;
        }
    }
}
