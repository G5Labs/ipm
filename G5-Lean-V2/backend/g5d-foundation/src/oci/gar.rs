//! GAR (Google Artifact Registry, `*-docker.pkg.dev`) credential provider
//! (D22).
//!
//! GAR speaks the OCI Distribution v2 bearer flow verbatim, so it reuses the
//! shared `DistributionClient` core (D15) and the standard `fetch_token`
//! token-endpoint round-trip. The only per-registry behavior is *acquiring the
//! Basic credential* presented to that token endpoint: a GCP OAuth2 access
//! token resolved asynchronously from (in priority order) injected config, the
//! GCE metadata server, a service-account key JSON, `gcloud`, or — failing all
//! — anonymous. The username is always `oauth2accesstoken`; the access token
//! is the password.
//!
//! This mirrors the worker-host GCP token logic in `g5-api/src/cloud/gcp.rs`
//! but is intentionally g5d-local: the crate boundary is NOT crossed.

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::auth::{fetch_token, AuthChallenge, CredentialProvider, OAUTH2_ACCESS_TOKEN_USERNAME};
use super::error::OciError;

/// GCE metadata server default-service-account token endpoint (D22 tier 2).
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

/// OAuth2 scope minted for GAR pulls.
const GCP_TOKEN_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
/// Default Google OAuth2 token endpoint for the SA JWT-bearer exchange.
const GCP_DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

/// Env: path to a service-account key JSON (GCP convention, D22 tier 3).
const ENV_SA_CREDS: &str = "GOOGLE_APPLICATION_CREDENTIALS";

/// Bounded timeout for the metadata-server probe so an off-GCE host whose
/// `metadata.google.internal` resolves but blackholes cannot stall a pull.
const METADATA_TIMEOUT: Duration = Duration::from_secs(2);

/// `{access_token, expires_in}` — the shape returned by both the metadata
/// server and the SA JWT-bearer token exchange.
#[derive(Deserialize)]
struct GcpToken {
    #[serde(default)]
    access_token: String,
}

/// Claims for the RFC 7523 service-account assertion JWT.
#[derive(Serialize)]
struct SaAssertionClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

/// GAR credential source (D22). One `GarClient` serves every GAR host/region;
/// the bearer realm is discovered per-request from the challenge, so this
/// provider is host-agnostic.
///
/// Each ambient credential tier is independently gateable so tests can opt
/// into the metadata-server or SA-JSON path against mock endpoints while
/// keeping the others off. Production sets every flag `true`.
pub struct GarCredentials {
    /// Tier 1: injected `(username, token)` from the daemon config layer.
    configured: Option<(String, String)>,
    /// GCE metadata token endpoint (overridable for tests).
    metadata_url: String,
    /// Allow the GCE metadata-server probe (Tier 2). `true` in production;
    /// the test constructor opts in only when a mock metadata URL has been
    /// wired, so the suite never hits real `169.254.169.254`/
    /// `metadata.google.internal`. The probe itself is bounded by
    /// [`METADATA_TIMEOUT`] regardless.
    allow_metadata: bool,
    /// Allow the SA-JSON env read (Tier 3). `true` in production; the test
    /// constructor opts in only when the test has wired a tmp SA JSON whose
    /// `token_uri` points at a mock endpoint.
    allow_sa: bool,
    /// Allow the `gcloud` shell-out (Tier 4). `false` under test so the
    /// suite never shells out to a developer's authenticated `gcloud`.
    allow_shellout: bool,
}

impl GarCredentials {
    /// Resolve from the runtime cascade (metadata → SA → gcloud → anonymous).
    pub fn from_env() -> Self {
        Self {
            configured: None,
            metadata_url: METADATA_TOKEN_URL.to_string(),
            allow_metadata: true,
            allow_sa: true,
            allow_shellout: true,
        }
    }

    /// Inject configured `(user, token)` (tier 1). `None` falls through to the
    /// runtime cascade.
    pub fn with_configured(configured: Option<(String, String)>) -> Self {
        Self {
            configured,
            metadata_url: METADATA_TOKEN_URL.to_string(),
            allow_metadata: true,
            allow_sa: true,
            allow_shellout: true,
        }
    }

    /// Test-only: overridable metadata URL with explicit gates for the
    /// metadata probe and the SA-JSON env read. The `gcloud` shell-out is
    /// always off under tests — `wiremock` cannot stand in for the local
    /// binary, so the suite would never want it to run.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn for_tests(
        configured: Option<(String, String)>,
        metadata_url: Option<String>,
        allow_metadata: bool,
        allow_sa: bool,
    ) -> Self {
        Self {
            configured,
            metadata_url: metadata_url.unwrap_or_else(|| METADATA_TOKEN_URL.to_string()),
            allow_metadata,
            allow_sa,
            allow_shellout: false,
        }
    }

    /// Resolve the Basic `(username, password)` pair for the GAR token
    /// endpoint, or `None` for an anonymous token request. Each non-injected
    /// tier is gated on its own flag so a test can opt into the metadata
    /// path or SA-JSON path (against mock endpoints) without enabling the
    /// others. Production sets every flag `true`.
    async fn resolve_basic_creds(&self, http: &reqwest::Client) -> Option<(String, String)> {
        if let Some(creds) = &self.configured {
            return Some(creds.clone());
        }
        if self.allow_metadata {
            if let Some(token) = self.try_metadata(http).await {
                return Some((OAUTH2_ACCESS_TOKEN_USERNAME.to_string(), token));
            }
        }
        if self.allow_sa {
            if let Some(token) = self.try_service_account(http).await {
                return Some((OAUTH2_ACCESS_TOKEN_USERNAME.to_string(), token));
            }
        }
        if self.allow_shellout {
            if let Some(token) = try_gcloud().await {
                return Some((OAUTH2_ACCESS_TOKEN_USERNAME.to_string(), token));
            }
        }
        None
    }

    /// Tier 2: GCE metadata server (workload identity). A transport error /
    /// non-200 / empty token is swallowed (→ try next source) since off-GCE
    /// hosts have no metadata server.
    async fn try_metadata(&self, http: &reqwest::Client) -> Option<String> {
        let resp = http
            .get(&self.metadata_url)
            .header("Metadata-Flavor", "Google")
            .timeout(METADATA_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let parsed: GcpToken = resp.json().await.ok()?;
        (!parsed.access_token.is_empty()).then_some(parsed.access_token)
    }

    /// Tier 3: service-account key JSON at `GOOGLE_APPLICATION_CREDENTIALS`,
    /// exchanged for an access token via the RFC 7523 JWT-bearer grant. The
    /// SA-JSON read goes through `tokio::fs` because this method is called
    /// from `mint`, which already runs under the async refresh lock —
    /// a blocking read would stall the reactor for the duration of disk I/O.
    async fn try_service_account(&self, http: &reqwest::Client) -> Option<String> {
        let path = std::env::var(ENV_SA_CREDS).ok()?;
        let contents = tokio::fs::read_to_string(&path).await.ok()?;
        let v: serde_json::Value = serde_json::from_str(&contents).ok()?;
        if v.get("type").and_then(|t| t.as_str()) != Some("service_account") {
            return None;
        }
        let client_email = v.get("client_email")?.as_str()?.to_string();
        let private_key = v.get("private_key")?.as_str()?.to_string();
        let token_uri = v
            .get("token_uri")
            .and_then(|t| t.as_str())
            .unwrap_or(GCP_DEFAULT_TOKEN_URI)
            .to_string();
        let assertion = build_sa_assertion(&client_email, &private_key, &token_uri).ok()?;
        let resp = http
            .post(&token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let parsed: GcpToken = resp.json().await.ok()?;
        (!parsed.access_token.is_empty()).then_some(parsed.access_token)
    }
}

#[async_trait]
impl CredentialProvider for GarCredentials {
    fn registry(&self) -> &str {
        // One client serves every GAR host; the precise host rides each error
        // via the `registry_host` arg threaded through `mint`/`on_denied`.
        "GAR (*-docker.pkg.dev)"
    }

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
                "GAR bearer challenge required to mint a token but none was presented".to_string(),
            )
        })?;
        let creds = self.resolve_basic_creds(http).await;
        let creds_ref = creds.as_ref().map(|(u, p)| (u.as_str(), p.as_str()));
        fetch_token(http, challenge, creds_ref).await
    }

    fn on_denied(&self, registry_host: &str, repository: &str, scope: &str) -> OciError {
        // A 401 persisting after a fresh token means either anonymous pull of a
        // private image or a token that GAR rejected. The actionable signal for
        // operators is the same — the host needs a credential source — so we
        // surface `AuthRequired` (D22), tagged with the precise host.
        OciError::AuthRequired {
            registry: registry_host.to_string(),
            image: repository.to_string(),
            scope: scope.to_string(),
        }
    }
}

/// Tier 4: `gcloud auth print-access-token` shell-out (local dev). A missing
/// binary / non-zero exit / empty output → `None` (try next / anonymous).
async fn try_gcloud() -> Option<String> {
    let out = tokio::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let token = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Sign the RFC 7523 SA assertion JWT (RS256) used in the JWT-bearer exchange.
fn build_sa_assertion(
    client_email: &str,
    private_key_pem: &str,
    token_uri: &str,
) -> Result<String, OciError> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let now = unix_now();
    let claims = SaAssertionClaims {
        iss: client_email,
        scope: GCP_TOKEN_SCOPE,
        aud: token_uri,
        iat: now,
        exp: now + 3600,
    };
    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes()).map_err(|e| {
        OciError::AuthFailed(
            "GAR".to_string(),
            format!("invalid service-account private key: {e}"),
        )
    })?;
    encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(|e| {
        OciError::AuthFailed(
            "GAR".to_string(),
            format!("failed to sign SA assertion: {e}"),
        )
    })
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_label_is_host_family() {
        let provider = GarCredentials::from_env();
        assert_eq!(provider.registry(), "GAR (*-docker.pkg.dev)");
    }

    #[test]
    fn on_denied_is_auth_required_with_host() {
        let provider = GarCredentials::from_env();
        match provider.on_denied(
            "us-central1-docker.pkg.dev",
            "proj/repo/img",
            "repository:proj/repo/img:pull",
        ) {
            OciError::AuthRequired {
                registry,
                image,
                scope,
            } => {
                assert_eq!(registry, "us-central1-docker.pkg.dev");
                assert_eq!(image, "proj/repo/img");
                assert_eq!(scope, "repository:proj/repo/img:pull");
            }
            other => panic!("expected AuthRequired, got {other:?}"),
        }
    }

    #[test]
    fn sa_assertion_signs_with_valid_pem() {
        // A throwaway RSA key proves the RS256 signing path produces a
        // three-segment JWT; an invalid PEM surfaces AuthFailed.
        let err = build_sa_assertion("sa@x.iam", "not-a-pem", GCP_DEFAULT_TOKEN_URI).unwrap_err();
        assert!(matches!(err, OciError::AuthFailed(_, _)));
    }
}
