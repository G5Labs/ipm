//! ACR (Azure Container Registry, `*.azurecr.io`) credential provider (D26).
//!
//! ACR speaks the OCI Distribution bearer challenge, but mints the bearer
//! through an Azure-flavored two-step token exchange rather than the standard
//! `GET <realm>?service=&scope=`:
//!
//! 1. Acquire an Azure AD (AAD) access token for `https://management.azure.com/`.
//! 2. `POST <registry>/oauth2/exchange` (AAD token → ACR refresh token).
//! 3. `POST <registry>/oauth2/token` (refresh token → ACR access token).
//!
//! `AcrCredentials` therefore overrides `mint` to run the dance, deriving all
//! endpoints from the challenge realm so the flow is testable against a mock.
//! The ACR registry-username convention for the oauth2 dance is the all-zero
//! GUID [`ACR_OAUTH_USERNAME`]; the bearer token alone rides the `/v2/`
//! `Authorization` header.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::auth::{AuthChallenge, CredentialProvider};
use super::error::OciError;

/// The ACR registry-username convention when authenticating via the AAD oauth2
/// dance (`docker login -u 00000000-0000-0000-0000-000000000000`). Documented
/// for provenance; the minted bearer is what authenticates `/v2/` requests.
#[allow(dead_code)]
const ACR_OAUTH_USERNAME: &str = "00000000-0000-0000-0000-000000000000";

/// AAD scope/resource for ACR token exchange.
const AAD_SCOPE: &str = "https://management.azure.com/.default";
const AAD_RESOURCE: &str = "https://management.azure.com/";
/// Default AAD authority base (overridable for tests).
const AAD_AUTHORITY_BASE: &str = "https://login.microsoftonline.com";
/// Default Azure IMDS base (overridable for tests).
const IMDS_BASE: &str = "http://169.254.169.254";
/// Default ACR access-token lifetime when `oauth2/token` omits `expires_in`.
const DEFAULT_TOKEN_TTL_SECS: u64 = 300;
/// Bounded timeout for the IMDS managed-identity probe.
const IMDS_TIMEOUT: Duration = Duration::from_secs(2);

/// AAD token-endpoint response (service principal / IMDS).
#[derive(Deserialize)]
struct AadToken {
    #[serde(default)]
    access_token: String,
}

/// `az account get-access-token --output json` response.
#[derive(Deserialize)]
struct AzToken {
    #[serde(rename = "accessToken", default)]
    access_token: String,
}

/// `oauth2/exchange` response (AAD → ACR refresh token).
#[derive(Deserialize)]
struct AcrRefreshToken {
    #[serde(default)]
    refresh_token: String,
}

/// `oauth2/token` response (refresh token → ACR access token).
#[derive(Deserialize)]
struct AcrAccessToken {
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
}

/// ACR credential source (D26). One `AcrClient` serves every ACR host; the
/// oauth2 endpoints are derived per-request from the challenge realm.
///
/// Non-injected tiers are gated by two independent flags so tests can opt
/// SP/IMDS in against mock endpoints while the unmockable `az` CLI tier stays
/// off. The service-principal env reads and IMDS managed-identity probe are
/// gated on `ambient`; the `az` shell-out is gated on `allow_az_shellout`.
/// Production constructors (`from_env`, `with_aad_token`) set both flags
/// `true`; the test constructor `for_tests` accepts an `ambient` arg but
/// hard-pins `allow_az_shellout = false` regardless, so a `wiremock`-driven
/// test can run SP/IMDS against mock URLs without ever reaching a developer's
/// authenticated `az`.
pub struct AcrCredentials {
    /// Tier 1: injected AAD access token from the daemon config layer.
    injected_aad: Option<String>,
    /// AAD authority base (overridable for tests).
    authority_base: String,
    /// Azure IMDS base (overridable for tests).
    imds_base: String,
    /// Whether to consult ambient host sources (service-principal env vars
    /// and IMDS managed identity). `false` under test so the suite never reads
    /// real `AZURE_TENANT_ID`/`AZURE_CLIENT_ID`/`AZURE_CLIENT_SECRET` and never
    /// probes real `169.254.169.254`. The service-principal and IMDS paths run
    /// only when this flag is `true` (see [`Self::for_tests`]).
    ambient: bool,
    /// Whether to consult the `az` CLI shell-out tier. Always `false` under
    /// test (forced off by [`Self::for_tests`]) so the suite never shells out
    /// to a developer's authenticated `az`, even when `ambient` is `true` so
    /// the SP/IMDS mock paths can run. Production sets this `true`.
    allow_az_shellout: bool,
}

impl AcrCredentials {
    /// Resolve the AAD token from the runtime cascade (service principal →
    /// IMDS → `az`).
    pub fn from_env() -> Self {
        Self {
            injected_aad: None,
            authority_base: AAD_AUTHORITY_BASE.to_string(),
            imds_base: IMDS_BASE.to_string(),
            ambient: true,
            allow_az_shellout: true,
        }
    }

    /// Inject an AAD access token (tier 1). `None` falls through to the
    /// service-principal / IMDS / `az` tiers.
    pub fn with_aad_token(injected_aad: Option<String>) -> Self {
        Self {
            injected_aad,
            authority_base: AAD_AUTHORITY_BASE.to_string(),
            imds_base: IMDS_BASE.to_string(),
            ambient: true,
            allow_az_shellout: true,
        }
    }

    /// Test-only: overridable AAD-authority + IMDS base URLs with an explicit
    /// ambient gate. Tests that want to drive the service-principal / IMDS
    /// paths against mock URLs pass `ambient = true`; tests that exercise only
    /// the injected/anonymous paths pass `ambient = false`. The `az` shell-out
    /// tier is hard-disabled here regardless of `ambient` — `wiremock` cannot
    /// stand in for the local `az` binary, so the suite must never shell out
    /// to a developer's authenticated `az`.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn for_tests(
        injected_aad: Option<String>,
        authority_base: Option<String>,
        imds_base: Option<String>,
        ambient: bool,
    ) -> Self {
        Self {
            injected_aad,
            authority_base: authority_base.unwrap_or_else(|| AAD_AUTHORITY_BASE.to_string()),
            imds_base: imds_base.unwrap_or_else(|| IMDS_BASE.to_string()),
            ambient,
            allow_az_shellout: false,
        }
    }

    /// Resolve an AAD access token (injected → service principal → IMDS →
    /// `az`), or `None` (anonymous, which private ACR rejects).
    async fn resolve_aad(&self, http: &reqwest::Client) -> Option<String> {
        if let Some(tok) = &self.injected_aad {
            return Some(tok.clone());
        }
        if self.ambient {
            if let Some(tok) = self.try_service_principal(http).await {
                return Some(tok);
            }
            if let Some(tok) = self.try_imds(http).await {
                return Some(tok);
            }
        }
        if self.allow_az_shellout {
            if let Some(tok) = try_az().await {
                return Some(tok);
            }
        }
        None
    }

    /// Tier 2: service-principal client-credentials grant against AAD.
    async fn try_service_principal(&self, http: &reqwest::Client) -> Option<String> {
        let tenant = std::env::var("AZURE_TENANT_ID")
            .ok()
            .filter(|s| !s.is_empty())?;
        let client_id = std::env::var("AZURE_CLIENT_ID")
            .ok()
            .filter(|s| !s.is_empty())?;
        let client_secret = std::env::var("AZURE_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.is_empty())?;
        let url = format!(
            "{}/{}/oauth2/v2.0/token",
            self.authority_base.trim_end_matches('/'),
            tenant
        );
        let resp = http
            .post(&url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("scope", AAD_SCOPE),
            ])
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let parsed: AadToken = resp.json().await.ok()?;
        (!parsed.access_token.is_empty()).then_some(parsed.access_token)
    }

    /// Tier 3: managed identity via Azure IMDS.
    async fn try_imds(&self, http: &reqwest::Client) -> Option<String> {
        let url = format!(
            "{}/metadata/identity/oauth2/token?api-version=2018-02-01&resource={}",
            self.imds_base.trim_end_matches('/'),
            AAD_RESOURCE
        );
        let resp = http
            .get(&url)
            .header("Metadata", "true")
            .timeout(IMDS_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let parsed: AadToken = resp.json().await.ok()?;
        (!parsed.access_token.is_empty()).then_some(parsed.access_token)
    }

    /// Step 2 of the dance: exchange the AAD token for an ACR refresh token.
    async fn exchange_refresh_token(
        &self,
        http: &reqwest::Client,
        base: &str,
        service: &str,
        aad_token: &str,
    ) -> Result<String, OciError> {
        let url = format!("{base}/oauth2/exchange");
        let resp = http
            .post(&url)
            .form(&[
                ("grant_type", "access_token"),
                ("service", service),
                ("access_token", aad_token),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OciError::AuthFailed(
                self.registry().to_string(),
                format!(
                    "ACR oauth2/exchange returned {}: {}",
                    status,
                    super::error::truncate_body(&body)
                ),
            ));
        }
        let parsed: AcrRefreshToken = resp.json().await.map_err(|e| {
            OciError::AuthFailed(
                self.registry().to_string(),
                format!("ACR oauth2/exchange response decode failed: {e}"),
            )
        })?;
        if parsed.refresh_token.is_empty() {
            return Err(OciError::AuthFailed(
                self.registry().to_string(),
                "ACR oauth2/exchange returned an empty refresh_token".to_string(),
            ));
        }
        Ok(parsed.refresh_token)
    }

    /// Step 3 of the dance: exchange the refresh token for an ACR access token.
    async fn exchange_access_token(
        &self,
        http: &reqwest::Client,
        base: &str,
        service: &str,
        scope: &str,
        refresh_token: &str,
    ) -> Result<(String, u64), OciError> {
        let url = format!("{base}/oauth2/token");
        let resp = http
            .post(&url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("service", service),
                ("scope", scope),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OciError::AuthFailed(
                self.registry().to_string(),
                format!(
                    "ACR oauth2/token returned {}: {}",
                    status,
                    super::error::truncate_body(&body)
                ),
            ));
        }
        let parsed: AcrAccessToken = resp.json().await.map_err(|e| {
            OciError::AuthFailed(
                self.registry().to_string(),
                format!("ACR oauth2/token response decode failed: {e}"),
            )
        })?;
        if parsed.access_token.is_empty() {
            return Err(OciError::AuthFailed(
                self.registry().to_string(),
                "ACR oauth2/token returned an empty access_token".to_string(),
            ));
        }
        Ok((
            parsed.access_token,
            parsed.expires_in.unwrap_or(DEFAULT_TOKEN_TTL_SECS),
        ))
    }
}

#[async_trait]
impl CredentialProvider for AcrCredentials {
    fn registry(&self) -> &str {
        "ACR (*.azurecr.io)"
    }

    async fn mint(
        &self,
        http: &reqwest::Client,
        challenge: Option<&AuthChallenge>,
        registry_host: &str,
        repository: &str,
        scope: &str,
    ) -> Result<(String, u64), OciError> {
        let challenge = challenge.ok_or_else(|| {
            OciError::AuthFailed(
                self.registry().to_string(),
                "ACR bearer challenge required to mint a token but none was presented".to_string(),
            )
        })?;
        // The ACR realm is `<base>/oauth2/token`; the exchange + token
        // endpoints are siblings under `<base>` (so tests can point them at a
        // mock server via the challenge realm).
        let base = challenge
            .realm
            .strip_suffix("/oauth2/token")
            .ok_or_else(|| {
                OciError::AuthFailed(
                    self.registry().to_string(),
                    format!(
                        "ACR challenge realm '{}' did not end with /oauth2/token",
                        challenge.realm
                    ),
                )
            })?;

        let aad = self
            .resolve_aad(http)
            .await
            .ok_or_else(|| OciError::AuthRequired {
                registry: registry_host.to_string(),
                image: repository.to_string(),
                scope: scope.to_string(),
            })?;

        let refresh = self
            .exchange_refresh_token(http, base, &challenge.service, &aad)
            .await?;
        self.exchange_access_token(http, base, &challenge.service, &challenge.scope, &refresh)
            .await
    }

    fn on_denied(&self, registry_host: &str, repository: &str, scope: &str) -> OciError {
        OciError::AuthRequired {
            registry: registry_host.to_string(),
            image: repository.to_string(),
            scope: scope.to_string(),
        }
    }
}

/// Tier 4: `az account get-access-token` shell-out (local dev). A missing
/// binary / non-zero exit / empty token → `None`.
async fn try_az() -> Option<String> {
    let out = tokio::process::Command::new("az")
        .args([
            "account",
            "get-access-token",
            "--resource",
            AAD_RESOURCE,
            "--output",
            "json",
        ])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let parsed: AzToken = serde_json::from_slice(&out.stdout).ok()?;
    (!parsed.access_token.is_empty()).then_some(parsed.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_label_is_host_family() {
        assert_eq!(AcrCredentials::from_env().registry(), "ACR (*.azurecr.io)");
    }

    #[test]
    fn on_denied_is_auth_required_with_host() {
        let provider = AcrCredentials::from_env();
        match provider.on_denied("myreg.azurecr.io", "team/img", "repository:team/img:pull") {
            OciError::AuthRequired {
                registry,
                image,
                scope,
            } => {
                assert_eq!(registry, "myreg.azurecr.io");
                assert_eq!(image, "team/img");
                assert_eq!(scope, "repository:team/img:pull");
            }
            other => panic!("expected AuthRequired, got {other:?}"),
        }
    }

    #[test]
    fn oauth_username_convention_is_all_zero_guid() {
        assert_eq!(ACR_OAUTH_USERNAME, "00000000-0000-0000-0000-000000000000");
    }
}
