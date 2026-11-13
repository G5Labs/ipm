//! GCP Compute Engine + Networking implementation of [`CloudProvider`]
//! (PR-rev-7-9c).
//!
//! Talks to the GCP REST APIs directly via `reqwest` rather than pulling a
//! `google-cloud-compute` SDK crate (ontology [I8]): the workspace has no
//! Google SDK vendored, the surface is ~6 endpoints, and `reqwest` +
//! `serde_json` + `jsonwebtoken` already cover it — including the three auth
//! paths (service-account JWT-bearer, GCE metadata server / workload identity,
//! and the ADC `authorized_user` refresh-token path for local dev).
//!
//! This provider is never unit-tested against a live API — offline tests use
//! [`super::MockCloudProvider`]. The pure helpers (status mapping, resource
//! naming, zone derivation, `from_env` gating) ARE unit-tested here; the live
//! REST path is covered by the rev-7 acceptance gate, not CI.
//!
//! Contract: `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md`
//! [D18], [D19], [I8].

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use g5_contracts::error::G5Error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

use super::{
    host_instance_name, BootImageIdentity, CloudError, CloudInstanceRef, CloudProvider,
    FoundInstance, InstanceState, ListedInstance, ProvisioningActor, VpcRef,
};
use crate::services::self_hosted_license_service::{MintedRegistryToken, RegistryTokenIssuer};

// ─── Constants / env keys ────────────────────────────────────────────────────

const COMPUTE_BASE: &str = "https://compute.googleapis.com/compute/v1";
const IAM_CREDENTIALS_BASE: &str = "https://iamcredentials.googleapis.com/v1";
const OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

/// Env: GCP project id. Required — when absent, [`GcpCloudProvider::from_env`]
/// returns `None` (cloud disabled, 9b DB-only behavior preserved).
const ENV_PROJECT: &str = "GCP_PROJECT_ID";
/// Env: alternative project id key (GCP convention).
const ENV_PROJECT_ALT: &str = "GOOGLE_CLOUD_PROJECT";
/// Env: path to a service-account key JSON (GCP convention).
const ENV_CREDS: &str = "GOOGLE_APPLICATION_CREDENTIALS";
/// Env: the worker boot image self-link/family.
const ENV_WORKER_IMAGE: &str = "G5_WORKER_IMAGE";
/// Env: zone suffix appended to the region (e.g. `a` → `{region}-a`).
const ENV_ZONE_SUFFIX: &str = "G5_GCP_ZONE_SUFFIX";
/// Env: when set (any value), open tcp:22 to `g5-worker`-tagged instances.
const ENV_ALLOW_SSH: &str = "G5_WORKER_ALLOW_SSH";
const DEFAULT_ZONE_SUFFIX: &str = "a";
/// Explicit outer-host identity. Lean deployments must use their dedicated SA.
const ENV_WORKER_SERVICE_ACCOUNT: &str = "G5_WORKER_SERVICE_ACCOUNT";
const MAX_RETRIES: u32 = 5;
const OP_POLL_INTERVAL: Duration = Duration::from_secs(2);
const OP_POLL_MAX: Duration = Duration::from_secs(120);

/// Per-region /20 subnet CIDR table (ontology [D18]).
///
/// The per-org VPC is global, but each subnet inside it is regional and GCP
/// requires subnet primary ranges to be unique within a VPC regardless of
/// region. A fixed `subnet_cidr` shared across regions collides the moment
/// an org provisions a 2nd region (opus48 round-2 B-2). The fix is a
/// deterministic region → CIDR mapping so every region gets its own /20.
///
/// /20 gives 4094 usable hosts per region per org, which is the rev-7 V1
/// host budget with room (`aux_files/future_implementation/microvm_completion_plan.md`
/// §5.7 caps fleets at ~100 hosts per org). The list is the same set as
/// `g5_contracts::types::worker_host::WORKER_HOST_REGION_ALLOWLIST` so a
/// rejected region never reaches the lookup.
const REGION_SUBNET_CIDR_TABLE: &[(&str, &str)] = &[
    ("us-central1", "10.128.0.0/20"),
    ("us-east1", "10.129.0.0/20"),
    ("us-east4", "10.130.0.0/20"),
    ("us-west1", "10.131.0.0/20"),
    ("us-west2", "10.132.0.0/20"),
    ("europe-west1", "10.133.0.0/20"),
    ("europe-west4", "10.134.0.0/20"),
    ("asia-east1", "10.135.0.0/20"),
    ("asia-northeast1", "10.136.0.0/20"),
];

/// CIDR summary covering every entry of [`REGION_SUBNET_CIDR_TABLE`]
/// (`10.128.0.0/12` = `10.128.0.0`-`10.143.255.255`). Used as the
/// `allow-internal` firewall source range so intra-VPC traffic between
/// regional subnets works without enumerating each /20.
const INTERNAL_SUMMARY_CIDR: &str = "10.128.0.0/12";

/// Look up the regional subnet CIDR for `region`. `None` for off-table
/// values — the caller maps it to [`CloudError::InvalidConfig`].
fn region_subnet_cidr(region: &str) -> Option<&'static str> {
    REGION_SUBNET_CIDR_TABLE
        .iter()
        .find_map(|(r, cidr)| (*r == region).then_some(*cidr))
}

// ─── Token acquisition ───────────────────────────────────────────────────────

/// Where access tokens come from. Resolved once at construction.
#[derive(Debug, Clone)]
enum TokenSource {
    /// 2-legged service-account JWT-bearer flow (RFC 7523).
    ServiceAccount {
        client_email: String,
        private_key_pem: String,
        token_uri: String,
    },
    /// ADC `authorized_user` (gcloud auth application-default login) — local dev.
    AuthorizedUser {
        client_id: String,
        client_secret: String,
        refresh_token: String,
        token_uri: String,
    },
    /// GCE metadata server (in-cluster workload identity).
    Metadata,
}

/// A cached bearer token + its computed expiry.
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

/// Claims for the service-account assertion JWT.
#[derive(Serialize)]
struct SaAssertionClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─── Provider ────────────────────────────────────────────────────────────────

/// GCP Compute Engine + Networking [`CloudProvider`].
pub struct GcpCloudProvider {
    project_id: String,
    worker_image: String,
    worker_service_account: String,
    zone_suffix: String,
    allow_ssh: bool,
    http: reqwest::Client,
    token_source: TokenSource,
    token_cache: Mutex<Option<CachedToken>>,
    #[cfg(test)]
    compute_base_override: Option<String>,
}

impl GcpCloudProvider {
    /// Build a provider from the environment, returning `None` (cloud disabled)
    /// when GCP is not resolvable — i.e. no project id, or no credential source.
    /// Never panics and never blocks startup on GCP reachability (ontology
    /// [D24]).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        // Resolve a credential source first; it may also carry the project id.
        let (token_source, project_from_creds) = match resolve_token_source() {
            Some(pair) => pair,
            None => {
                tracing::warn!(
                    "GCP credentials not found ({ENV_CREDS} unset/unreadable, no ADC file); \
                     cloud provider disabled"
                );
                return None;
            }
        };

        let project_id = std::env::var(ENV_PROJECT)
            .ok()
            .or_else(|| std::env::var(ENV_PROJECT_ALT).ok())
            .or(project_from_creds)
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());

        let Some(project_id) = project_id else {
            tracing::warn!(
                "GCP project id not set ({ENV_PROJECT}/{ENV_PROJECT_ALT} unset and not in \
                 credentials); cloud provider disabled"
            );
            return None;
        };

        // GCP project IDs match `[a-z][a-z0-9-]{5,29}` per Google's rules. The
        // value flows into `format!`-built REST URLs as a path segment
        // (`projects/{project_id}/...`); validating at construction closes
        // the same URL-shape vector the `region`/`instance_type` allowlists
        // close at the request boundary (codex round-2 nit promoted).
        if !is_valid_gcp_project_id(&project_id) {
            tracing::warn!(
                project_id = %project_id,
                "GCP project id is malformed (expected /^[a-z][a-z0-9-]{{5,29}}$/); \
                 cloud provider disabled"
            );
            return None;
        }

        let configured_host_account = std::env::var(ENV_WORKER_SERVICE_ACCOUNT).ok();
        let Some(worker_service_account) =
            worker_host_service_account(&project_id, configured_host_account.as_deref())
        else {
            tracing::warn!(
                "{ENV_WORKER_SERVICE_ACCOUNT} must name the dedicated host identity in this project; cloud provider disabled"
            );
            return None;
        };
        let worker_image = std::env::var(ENV_WORKER_IMAGE)
            .ok()
            .unwrap_or_else(|| format!("projects/{project_id}/global/images/family/g5-worker"));
        let zone_suffix = match std::env::var(ENV_ZONE_SUFFIX).ok() {
            Some(raw) => match sanitize_zone_suffix(&raw) {
                Some(s) => s,
                None => {
                    tracing::warn!(
                        raw = %raw,
                        "{ENV_ZONE_SUFFIX} is malformed (expected /^[a-z0-9]+$/); \
                         disabling cloud provider so requests fail closed",
                    );
                    return None;
                }
            },
            None => DEFAULT_ZONE_SUFFIX.to_string(),
        };
        let allow_ssh = std::env::var(ENV_ALLOW_SSH).is_ok();

        // No provider-wide boot-disk size: each host's disk is sized from its
        // own VM budget and passed to `provision_instance` (ontology
        // `g5.runtime.placement.host-disk`), so no configuration can separate
        // what the Add host dialog shows from what the server applies.
        tracing::info!(
            %project_id,
            worker_image = %worker_image,
            "GCP cloud provider initialized"
        );

        Some(Self {
            project_id,
            worker_image,
            worker_service_account,
            zone_suffix,
            allow_ssh,
            http: reqwest::Client::new(),
            token_source,
            token_cache: Mutex::new(None),
            #[cfg(test)]
            compute_base_override: None,
        })
    }

    // ── Token handling ──

    /// Return a valid bearer token, refreshing if the cached one is missing or
    /// within 60s of expiry.
    ///
    /// Concurrency: the mutex guards the *cached value* and is released before
    /// the network round-trip to the token endpoint. Without this two-phase
    /// pattern, every concurrent caller would queue behind a single in-flight
    /// refresh — under load the queue can dwarf the refresh itself. We
    /// double-check the cache after the network fetch returns so a parallel
    /// refresher who wins the race is the one whose value sticks.
    async fn access_token(&self) -> Result<String, CloudError> {
        // Fast path: cached token still good for ≥60s.
        {
            let cache = self.token_cache.lock().await;
            if let Some(tok) = cache.as_ref() {
                if tok.expires_at > Instant::now() + Duration::from_secs(60) {
                    return Ok(tok.access_token.clone());
                }
            }
        }
        // Slow path: fetch a fresh token WITHOUT holding the lock, so
        // concurrent callers can also race in (whichever finishes first wins
        // the double-check below).
        let (access_token, expires_in) = self.fetch_token().await?;
        let expires_at = Instant::now() + Duration::from_secs(expires_in.max(60));
        let mut cache = self.token_cache.lock().await;
        // Double-check: another caller may have stored a fresher token while
        // we were on the network. Prefer the newer expiry, drop the older.
        if let Some(existing) = cache.as_ref() {
            if existing.expires_at >= expires_at {
                return Ok(existing.access_token.clone());
            }
        }
        *cache = Some(CachedToken {
            access_token: access_token.clone(),
            expires_at,
        });
        Ok(access_token)
    }

    async fn fetch_token(&self) -> Result<(String, u64), CloudError> {
        match &self.token_source {
            TokenSource::ServiceAccount {
                client_email,
                private_key_pem,
                token_uri,
            } => {
                let assertion = build_sa_assertion(client_email, private_key_pem, token_uri)?;
                let resp = self
                    .http
                    .post(token_uri)
                    .form(&[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                        ("assertion", assertion.as_str()),
                    ])
                    .send()
                    .await
                    .map_err(|e| CloudError::Auth {
                        message: format!("token request failed: {e}"),
                    })?;
                parse_token_response(resp).await
            }
            TokenSource::AuthorizedUser {
                client_id,
                client_secret,
                refresh_token,
                token_uri,
            } => {
                let resp = self
                    .http
                    .post(token_uri)
                    .form(&[
                        ("grant_type", "refresh_token"),
                        ("client_id", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", refresh_token.as_str()),
                    ])
                    .send()
                    .await
                    .map_err(|e| CloudError::Auth {
                        message: format!("token refresh failed: {e}"),
                    })?;
                parse_token_response(resp).await
            }
            TokenSource::Metadata => {
                let resp = self
                    .http
                    .get(METADATA_TOKEN_URL)
                    .header("Metadata-Flavor", "Google")
                    .send()
                    .await
                    .map_err(|e| CloudError::Auth {
                        message: format!("metadata token request failed: {e}"),
                    })?;
                parse_token_response(resp).await
            }
        }
    }

    // ── HTTP core ──

    /// Issue an authenticated request with bounded retry on transient errors
    /// (429/500/502/503 + transport errors). Returns the final status + parsed
    /// JSON body (`Value::Null` when empty). 4xx (e.g. 404/409) are returned to
    /// the caller, not retried.
    async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), CloudError> {
        #[cfg(test)]
        let test_url = self.compute_base_override.as_ref().and_then(|base| {
            url.strip_prefix(COMPUTE_BASE)
                .map(|path| format!("{base}{path}"))
        });
        #[cfg(test)]
        let url = test_url.as_deref().unwrap_or(url);

        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let token = self.access_token().await?;
            let mut req = self.http.request(method.clone(), url).bearer_auth(&token);
            if let Some(b) = body {
                req = req.json(b);
            }
            let send_result = req.send().await;

            let resp = match send_result {
                Ok(resp) => resp,
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        let backoff = Duration::from_millis(200 * (1u64 << attempt.min(5)));
                        tracing::warn!(%url, attempt, error = %e, "GCP transport error; retrying");
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    return Err(CloudError::Transport {
                        message: format!("{method} {url}: {e}"),
                    });
                }
            };

            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            // 403 carrying a documented rate-limit reason retries like a 429
            // (issue #627 S20) — it is throughput pressure, not a denial. The
            // SAME structured check as api_error's classification.
            let is_transient = matches!(status, 429 | 500 | 502 | 503)
                || (status == 403
                    && serde_json::from_str::<Value>(&text)
                        .map(|body| is_rate_limited_403_body(&body))
                        .unwrap_or(false));

            if is_transient && attempt < MAX_RETRIES {
                let backoff = Duration::from_millis(200 * (1u64 << attempt.min(5)));
                tracing::warn!(%url, attempt, status, "GCP transient error; retrying");
                tokio::time::sleep(backoff).await;
                continue;
            }

            let value: Value = if text.trim().is_empty() {
                Value::Null
            } else {
                match serde_json::from_str::<Value>(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        // Don't fail the request — many GCP 2xx surfaces (e.g.
                        // operation polls) tolerate `Value::Null` as
                        // "completed, nothing to read". But silently swallowing
                        // a malformed body hides real provider misbehavior and
                        // burns hours during incident response, so warn with
                        // bounded context (no creds in body; bearer is header-
                        // only).
                        let snippet_len = text.len().min(200);
                        tracing::warn!(
                            %url,
                            status,
                            body_len = text.len(),
                            error = %e,
                            body_prefix = %&text[..snippet_len],
                            "GCP response body was not valid JSON; treating as null"
                        );
                        Value::Null
                    }
                }
            };

            tracing::debug!(%method, %url, status, "GCP API call");
            return Ok((status, value));
        }
    }

    /// Mint a short-lived OAuth access token for another service account via
    /// IAMCredentials `generateAccessToken`.
    ///
    /// The caller's credential must have `roles/iam.serviceAccountTokenCreator`
    /// on `service_account_email`. The returned token is suitable for Docker
    /// login to Artifact Registry with username `oauth2accesstoken`.
    pub async fn generate_service_account_access_token(
        &self,
        service_account_email: &str,
        lifetime: Duration,
    ) -> Result<(String, u64), CloudError> {
        let lifetime_seconds = lifetime.as_secs().clamp(60, 3600);
        let encoded_email: String =
            url::form_urlencoded::byte_serialize(service_account_email.as_bytes()).collect();
        let url = format!(
            "{IAM_CREDENTIALS_BASE}/projects/-/serviceAccounts/{encoded_email}:generateAccessToken"
        );
        let body = json!({
            "scope": [OAUTH_SCOPE],
            "lifetime": format!("{lifetime_seconds}s"),
        });

        let (status, response) = self
            .request(reqwest::Method::POST, &url, Some(&body))
            .await?;
        if !(200..=299).contains(&status) {
            return Err(api_error(status, &response, &url));
        }

        let access_token = response
            .get("accessToken")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|token| !token.is_empty())
            .ok_or_else(|| CloudError::Unexpected {
                message: "IAMCredentials response missing accessToken".to_string(),
            })?;

        Ok((access_token, lifetime_seconds))
    }

    /// Resolve an image *family* or specific image ref to its concrete immutable
    /// identity ([D56], issue #1424). GCE's `GET .../global/images/family/{family}`
    /// returns the family's current head image and `GET .../global/images/{name}`
    /// returns that image — both carry `name` + `selfLink`. `image_ref` is the
    /// `G5_WORKER_IMAGE` value (a bare `projects/...` path by default, or an
    /// absolute `COMPUTE_BASE` URL).
    ///
    /// The ref is origin-validated by [`validate_compute_image_ref`] BEFORE any
    /// request ([D57]): this URL is fetched with the provider bearer token
    /// attached, so an unvalidated operator-supplied value is a credential-
    /// disclosure (SSRF) sink.
    async fn resolve_image_identity(
        &self,
        image_ref: &str,
    ) -> Result<BootImageIdentity, CloudError> {
        let url = validate_compute_image_ref(image_ref)?;
        let img = self
            .get_optional(&url)
            .await?
            .ok_or_else(|| CloudError::NotFound {
                resource: image_ref.to_string(),
            })?;
        let image_id = img
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let self_link = img
            .get("selfLink")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if image_id.is_empty() || self_link.is_empty() {
            return Err(CloudError::Unexpected {
                message: format!("boot image resource for {image_ref} missing name/selfLink"),
            });
        }
        Ok(BootImageIdentity {
            image_id,
            self_link,
        })
    }

    /// GET returning `Ok(None)` on 404, `Ok(Some(body))` on 2xx, error otherwise.
    async fn get_optional(&self, url: &str) -> Result<Option<Value>, CloudError> {
        let (status, body) = self.request(reqwest::Method::GET, url, None).await?;
        match status {
            200..=299 => Ok(Some(body)),
            404 => Ok(None),
            _ => Err(api_error(status, &body, url)),
        }
    }

    /// POST a create; treat 409 (alreadyExists) as success-with-no-operation.
    /// Returns the parsed operation body on 2xx, `None` on 409.
    async fn post_create(&self, url: &str, body: &Value) -> Result<Option<Value>, CloudError> {
        let (status, resp) = self.request(reqwest::Method::POST, url, Some(body)).await?;
        match status {
            200..=299 => Ok(Some(resp)),
            409 => Ok(None),
            _ => Err(api_error(status, &resp, url)),
        }
    }

    /// Poll a long-running operation (global/regional/zonal) to DONE via its
    /// `selfLink`.
    async fn poll_operation(&self, op: &Value, what: &str) -> Result<(), CloudError> {
        let self_link = op.get("selfLink").and_then(Value::as_str);
        let Some(self_link) = self_link else {
            // No operation to poll (e.g. synchronous create) — nothing to wait on.
            return Ok(());
        };
        let deadline = tokio::time::Instant::now() + OP_POLL_MAX;
        loop {
            let (status, body) = self.request(reqwest::Method::GET, self_link, None).await?;
            if !(200..=299).contains(&status) {
                return Err(api_error(status, &body, self_link));
            }
            let op_status = body.get("status").and_then(Value::as_str).unwrap_or("");
            if op_status == "DONE" {
                if let Some(err) = body.get("error") {
                    if !err.is_null() {
                        return Err(CloudError::Api {
                            status: 0,
                            message: format!("operation {what} failed: {err}"),
                        });
                    }
                }
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(CloudError::Timeout {
                    operation: what.to_string(),
                });
            }
            tokio::time::sleep(OP_POLL_INTERVAL).await;
        }
    }

    // ── URL builders ──

    fn networks_url(&self) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/global/networks",
            self.project_id
        )
    }
    fn network_url(&self, name: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/global/networks/{name}",
            self.project_id
        )
    }
    fn firewalls_url(&self) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/global/firewalls",
            self.project_id
        )
    }
    fn subnetworks_url(&self, region: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/regions/{region}/subnetworks",
            self.project_id
        )
    }
    fn subnetwork_url(&self, region: &str, name: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/regions/{region}/subnetworks/{name}",
            self.project_id
        )
    }
    fn routers_url(&self, region: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/regions/{region}/routers",
            self.project_id
        )
    }
    fn router_url(&self, region: &str, name: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/regions/{region}/routers/{name}",
            self.project_id
        )
    }
    fn instances_url(&self, zone: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/zones/{zone}/instances",
            self.project_id
        )
    }
    fn instance_url(&self, zone: &str, name: &str) -> String {
        format!(
            "{COMPUTE_BASE}/projects/{}/zones/{zone}/instances/{name}",
            self.project_id
        )
    }

    fn zone_for(&self, region: &str) -> String {
        format!("{region}-{}", self.zone_suffix)
    }

    /// List the region's zones that are currently `UP`, for the multi-zone
    /// stockout fallback ([D59]). Same selection as Terraform's
    /// `google_compute_zones{status="UP"}` data source. GCE zone names are
    /// `{region}-{suffix}`, so filter `zones.list` by the region-name prefix and
    /// `status == "UP"`. Sorted by name for a deterministic fallback sequence.
    async fn list_up_zones_in_region(&self, region: &str) -> Result<Vec<String>, CloudError> {
        let url = format!("{COMPUTE_BASE}/projects/{}/zones", self.project_id);
        let (status, body) = self.request(reqwest::Method::GET, &url, None).await?;
        if !(200..=299).contains(&status) {
            return Err(api_error(status, &body, &url));
        }
        let prefix = format!("{region}-");
        let mut zones: Vec<String> = body
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|z| {
                        let name = z.get("name").and_then(Value::as_str)?;
                        let up = z.get("status").and_then(Value::as_str) == Some("UP");
                        (up && name.starts_with(&prefix)).then(|| name.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
        zones.sort();
        Ok(zones)
    }
}

// ─── Pure helpers (unit-tested) ──────────────────────────────────────────────

/// The per-org VPC network name (ontology [D18]).
fn network_name(org_id: Uuid) -> String {
    format!("g5-org-{org_id}")
}

/// Validate a GCP project id against Google's documented rule
/// (`^[a-z][a-z0-9-]{5,29}$`). The string flows into REST URL path segments
/// (`projects/{project_id}/...`), so off-shape values must be rejected at
/// construction.
fn is_valid_gcp_project_id(project_id: &str) -> bool {
    let len = project_id.len();
    if !(6..=30).contains(&len) {
        return false;
    }
    let mut chars = project_id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Validate a `G5_WORKER_IMAGE` boot-image reference and return the Compute API
/// URL to fetch it ([D57]).
///
/// The returned URL is requested with the provider OAuth bearer token attached,
/// so an operator-supplied ref is a credential-disclosure (SSRF) sink and must
/// fail closed here, before any outbound call. Exactly two shapes are accepted:
///
/// 1. a relative Compute image resource path (optionally `/`-prefixed), which is
///    normalized onto [`COMPUTE_BASE`] — the default/documented form; and
/// 2. an absolute `https://` URL whose origin AND base path are exactly
///    [`COMPUTE_BASE`]'s.
///
/// Everything else — any other origin, an `http://` downgrade, credentials in
/// the URL, a query/fragment, or a path outside the images namespace — is
/// rejected with [`CloudError::InvalidConfig`] and issues no request.
fn validate_compute_image_ref(image_ref: &str) -> Result<String, CloudError> {
    let reject = |why: &str| CloudError::InvalidConfig {
        message: format!(
            "invalid {ENV_WORKER_IMAGE} boot-image reference {image_ref:?}: {why}. \
Expected projects/{{project}}/global/images/{{image}}, \
projects/{{project}}/global/images/family/{{family}}, or the same path \
absolute under {COMPUTE_BASE}"
        ),
    };

    // Only treat the ref as absolute when it actually carries a scheme; a bare
    // `projects/...` path can never contain "://" and stays on the relative path.
    let resource = if image_ref.contains("://") {
        let url =
            Url::parse(image_ref).map_err(|err| reject(&format!("unparseable URL ({err})")))?;
        let base = Url::parse(COMPUTE_BASE).expect("COMPUTE_BASE is a valid URL");

        if url.scheme() != "https" {
            return Err(reject("only https is accepted (no scheme downgrade)"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(reject("credentials in URL are not accepted"));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(reject("query strings and fragments are not accepted"));
        }
        // Compare host AND port so neither a lookalike host
        // (`compute.googleapis.com.attacker.example`) nor an alternate port can
        // pass as the Compute API origin.
        if url.host_str() != base.host_str()
            || url.port_or_known_default() != base.port_or_known_default()
        {
            return Err(reject("origin is not the Compute API"));
        }
        // `base.path()` is "/compute/v1"; require the ref to sit under it.
        let prefix = format!("{}/", base.path());
        match url.path().strip_prefix(&prefix) {
            Some(rest) => rest.to_string(),
            None => return Err(reject("path is not under the Compute API base path")),
        }
    } else {
        image_ref.trim_start_matches('/').to_string()
    };

    validate_image_resource_path(&resource).map_err(reject)?;
    Ok(format!("{COMPUTE_BASE}/{resource}"))
}

/// Shape-check a Compute image resource path ([D57]): either
/// `projects/{project}/global/images/{image}` or
/// `projects/{project}/global/images/family/{family}`, with each variable
/// segment non-empty, not a traversal token, and drawn from `[A-Za-z0-9._-]`.
fn validate_image_resource_path(resource: &str) -> Result<(), &'static str> {
    let segments: Vec<&str> = resource.split('/').collect();
    let names: [&str; 2] = match segments.as_slice() {
        ["projects", project, "global", "images", image] => [project, image],
        ["projects", project, "global", "images", "family", family] => [project, family],
        _ => return Err("not a Compute image resource path"),
    };

    for name in names {
        if name.is_empty() {
            return Err("empty project/image/family segment");
        }
        if name == "." || name == ".." {
            return Err("path traversal segment");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err("project/image/family segment outside [A-Za-z0-9._-]");
        }
    }
    Ok(())
}

/// Public wrapper for `sanitize_zone_suffix` so siblings in `cloud/mod.rs`
/// (specifically `gcp_zone_for_region`, used by the orphan-reconciler) can
/// share the exact same sanitization rule without exporting the private
/// helper at module scope.
#[doc(hidden)]
pub fn sanitize_zone_suffix_pub(raw: &str) -> Option<String> {
    sanitize_zone_suffix(raw)
}

/// Sanitize a `G5_GCP_ZONE_SUFFIX` value. GCP zones match `^[a-z0-9-]+$` and
/// the suffix is concatenated onto `{region}-`, so a leading or trailing `-`
/// produces an invalid zone (`us-central1--a`, `us-central1-`). We strip both
/// ends, then require the remainder to be `^[a-z0-9]+$` so the operator can
/// pass any of `a`, `-a`, `-a-` and get the same result, while obviously
/// malformed values like `; rm -rf /` are rejected.
fn sanitize_zone_suffix(raw: &str) -> Option<String> {
    let trimmed: String = raw.trim().trim_matches('-').chars().collect();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return None;
    }
    Some(trimmed)
}

/// Reduce a free-form string (an email, in practice) to a legal GCE label
/// value: lowercase letters, digits, `_`, `-`, ≤63 chars — `@` and `.` are
/// rejected by the API, so `pgndvd@gmail.com` becomes `pgndvd-gmail-com`.
/// Returns an empty string when nothing survives; the caller omits the label
/// then (ontology [D44]).
fn sanitize_gce_label_value(raw: &str) -> String {
    const GCE_LABEL_VALUE_MAX: usize = 63;
    let mut out = String::with_capacity(GCE_LABEL_VALUE_MAX);
    for c in raw.chars() {
        let mapped = match c {
            'A'..='Z' => c.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' | '_' => c,
            '-' | '@' | '.' => '-',
            _ => continue,
        };
        if mapped == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(mapped);
        if out.len() == GCE_LABEL_VALUE_MAX {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// 3rd-gen machine series (C3/C3D/H3/Z3/M3) require the gVNIC network driver
/// — GCE rejects `nicType=GVNIC` unless the image's `guestOsFeatures` includes
/// `GVNIC`, and rejects the default VirtIO-net path on 3rd-gen instances.
/// Older series (N2/N2D/C2) accept the GCE default and don't need this. We
/// gate on the slug prefix rather than resolving the image's features at
/// provision time (latency + propagation window) — see ontology [D41].
fn requires_gvnic(instance_type: &str) -> bool {
    // `WORKER_HOST_INSTANCE_TYPE_ALLOWLIST` currently restricts the 3rd-gen
    // family to C3, so checking that one prefix is sufficient. Add C3D/H3/Z3/M3
    // if the allowlist ever extends.
    instance_type.starts_with("c3-")
}

/// 3rd-gen machine series do NOT support GCE's default boot disk type
/// (`pd-standard`). For C3 we explicitly set `pd-balanced` — the SSD-backed,
/// lower-cost tier supported across every currently-allowlisted C3 SKU. Older
/// families return `None` to keep the GCE default (no regression from the
/// rev-7-9c shape). See ontology [D41].
fn boot_disk_type_for(instance_type: &str, zone: &str) -> Option<String> {
    if instance_type.starts_with("c3-") {
        Some(format!("zones/{zone}/diskTypes/pd-balanced"))
    } else {
        None
    }
}

/// Build the boot disk's `initializeParams` for an instance insert.
///
/// `boot_disk_gb` is the per-host size the caller resolved from the host's VM
/// budget or its stored override (ontology `g5.runtime.placement.host-disk`) —
/// there is no provider-wide default, because the disk is shared by all of the
/// host's VMs and a host that cannot hold one scratch allowance per slot
/// can leave its free slots unusable (issue #344).
///
/// `diskSizeGb` is GCE's `int64`-format field, so it must be a JSON **string**,
/// and it is interpreted as base-2 GB — the unit used by the sizing policy.
/// A 3rd-gen series also needs an explicit non-`pd-standard` type;
/// older families keep the GCE default (ontology [D41]).
fn boot_disk_init(source_image: &str, boot_disk_gb: u32, instance_type: &str, zone: &str) -> Value {
    let mut init = json!({
        "sourceImage": source_image,
        "diskSizeGb": boot_disk_gb.to_string(),
    });
    if let Some(disk_type) = boot_disk_type_for(instance_type, zone) {
        init["diskType"] = json!(disk_type);
    }
    init
}

/// Resolve the host account, requiring each Lean project's dedicated identity.
/// Unconfigured unrelated/local deployments preserve their historical behavior.
fn worker_host_service_account(project_id: &str, configured: Option<&str>) -> Option<String> {
    let lean_environment = match project_id {
        "g5-qa-1ccc" => Some("qa"),
        "g5lean-staging-1ccc" => Some("staging"),
        "g5lean-prod-1ccc" => Some("prod"),
        _ => None,
    };
    let Some(email) = configured else {
        return lean_environment.is_none().then(|| "default".to_string());
    };
    let suffix = format!("@{project_id}.iam.gserviceaccount.com");
    let account = email.strip_suffix(&suffix)?;
    if !(6..=30).contains(&account.len())
        || !account.as_bytes()[0].is_ascii_lowercase()
        || !account.as_bytes().last()?.is_ascii_alphanumeric()
        || !account
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return None;
    }
    if lean_environment.is_some_and(|environment| account != format!("g5-lean-{environment}-host"))
    {
        return None;
    }
    Some(email.to_string())
}

/// Build the Compute API attachment for the selected outer-host identity.
fn worker_host_service_accounts(email: &str) -> Value {
    json!([{
        "email": email,
        "scopes": [OAUTH_SCOPE],
    }])
}

/// Persisted `cloud_instance_id` encodes the zone so `get`/`terminate` can
/// address the zonal resource from the single DB column ([D19]).
fn encode_instance_id(zone: &str, name: &str) -> String {
    format!("{zone}/{name}")
}

/// Inverse of [`encode_instance_id`].
fn decode_instance_id(cloud_instance_id: &str) -> Result<(&str, &str), CloudError> {
    cloud_instance_id
        .split_once('/')
        .filter(|(z, n)| !z.is_empty() && !n.is_empty())
        .ok_or_else(|| CloudError::Unexpected {
            message: format!(
                "malformed cloud_instance_id (expected zone/name): {cloud_instance_id}"
            ),
        })
}

/// Map a GCE instance `status` string onto [`InstanceState`] (ontology [D19]).
fn map_gce_status(status: &str) -> InstanceState {
    match status {
        "PROVISIONING" | "STAGING" => InstanceState::Provisioning,
        "RUNNING" => InstanceState::Running,
        "STOPPING" | "SUSPENDING" | "REPAIRING" => InstanceState::Stopping,
        "TERMINATED" | "SUSPENDED" | "STOPPED" => InstanceState::Terminated,
        _ => InstanceState::Unknown,
    }
}

/// Build the fallback zone order for `provision_instance`'s multi-zone stockout
/// retry ([D59]): the configured primary zone FIRST (index 0, so the caller
/// re-indexes uniformly from the start), then every other UP zone in the region
/// in the given (sorted) order. The primary is de-duplicated so it is never
/// retried twice. If `up_zones` is empty (or lists only the primary — e.g. the
/// primary is the sole UP zone), the result is just `[primary]`, so the caller
/// gives up after the original target with no extra attempts.
fn ordered_fallback_zones(primary: &str, up_zones: &[String]) -> Vec<String> {
    let mut ordered = vec![primary.to_string()];
    ordered.extend(up_zones.iter().filter(|z| z.as_str() != primary).cloned());
    ordered
}

/// The region's REAL zone set for read/cleanup paths (issue #627 S33,
/// `g5.runtime.host.cloud-linkage`): the configured primary first when it
/// actually exists in the region's UP set, and only actual UP zones otherwise.
/// Unlike [`ordered_fallback_zones`] this never injects a preference that is
/// not a real zone (`us-east1` has no `-a`), so an invalid configured suffix
/// degrades to the region's valid zones instead of failing every request
/// against a zone GCE does not know.
fn ordered_up_zones(primary: &str, up_zones: &[String]) -> Vec<String> {
    if up_zones.iter().any(|z| z == primary) {
        ordered_fallback_zones(primary, up_zones)
    } else {
        up_zones.to_vec()
    }
}

/// True when a string carries one of GCP's documented rate-limit markers
/// (`rateLimitExceeded` / `userRateLimitExceeded` reason codes, or the "Rate
/// Limit Exceeded" message, see the Compute operations-quota docs).
fn is_rate_limited_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("ratelimitexceeded") || lower.contains("rate limit exceeded")
}

/// True when a 403 BODY carries a documented rate-limit reason/message —
/// throughput limits, not IAM denials (issue #627 S20): they must retry like
/// a 429 and never be reported as `AuthDenied`. The ONE structured check both
/// `request()`'s retry decision and `api_error`'s classification use, so the
/// two can never disagree (CodeRabbit on PR #637).
fn is_rate_limited_403_body(body: &Value) -> bool {
    let err = body.get("error");
    err.and_then(|e| e.get("errors"))
        .and_then(Value::as_array)
        .is_some_and(|errors| {
            errors.iter().any(|e| {
                e.get("reason")
                    .and_then(Value::as_str)
                    .is_some_and(is_rate_limited_text)
            })
        })
        || err
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .is_some_and(is_rate_limited_text)
}

fn api_error(status: u16, body: &Value, url: &str) -> CloudError {
    let message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("GCP request to {url} returned status {status}"));
    // GCP returns 403 for documented rate limits too; classify by the provider
    // reason, not the HTTP status alone (issue #627 S20,
    // `g5.runtime.host.cloud-linkage`). A rate-limited 403 is a retryable API
    // condition, never an IAM denial. Shares the request-core's retry check.
    let rate_limited = status == 403 && is_rate_limited_403_body(body);
    match status {
        404 => CloudError::NotFound {
            resource: url.to_string(),
        },
        403 if rate_limited => CloudError::Api { status, message },
        // 401/403 → dedicated auth-denied variant so logs/audit can
        // distinguish bad credentials / missing IAM grants from ordinary API
        // failures (codex round-2 important). Emitted at error level by the
        // caller via `g5_audit` `worker_host.cloud_auth_denied`.
        401 | 403 => CloudError::AuthDenied {
            status,
            scope: url.to_string(),
            detail: message,
        },
        _ => CloudError::Api { status, message },
    }
}

fn build_sa_assertion(
    client_email: &str,
    private_key_pem: &str,
    token_uri: &str,
) -> Result<String, CloudError> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let now = unix_now();
    let claims = SaAssertionClaims {
        iss: client_email,
        scope: OAUTH_SCOPE,
        aud: token_uri,
        iat: now,
        exp: now + 3600,
    };
    let key =
        EncodingKey::from_rsa_pem(private_key_pem.as_bytes()).map_err(|e| CloudError::Auth {
            message: format!("invalid service-account private key: {e}"),
        })?;
    encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(|e| CloudError::Auth {
        message: format!("failed to sign SA assertion: {e}"),
    })
}

async fn parse_token_response(resp: reqwest::Response) -> Result<(String, u64), CloudError> {
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..=299).contains(&status) {
        // OAuth error bodies can be large, multi-line, and occasionally include
        // upstream context we do not want in audit logs. Cap to 200 chars and
        // strip newlines/whitespace runs so the error string stays single-line
        // and bounded (codex round-2 nit).
        let detail = sanitize_token_error_body(&text);
        return Err(match status {
            // 401 and 403 from the token endpoint = the SA assertion /
            // refresh token is rejected or the caller is not authorized.
            // Both are auth-denied conditions, not generic network/transport
            // errors. Ontology D17 requires 401/403 from any GCP surface —
            // including token refresh — to land in the dedicated AuthDenied
            // bucket (codex round-3 fix).
            401 | 403 => CloudError::AuthDenied {
                status,
                scope: "token_endpoint".to_string(),
                detail,
            },
            _ => CloudError::Auth {
                message: format!("token endpoint returned {status}: {detail}"),
            },
        });
    }
    let parsed: TokenResponse = serde_json::from_str(&text).map_err(|e| CloudError::Auth {
        message: format!("malformed token response: {e}"),
    })?;
    Ok((parsed.access_token, parsed.expires_in))
}

/// Cap and sanitize a token-endpoint error body so it cannot blow up logs
/// or smuggle multi-line context. Bounded length and one-line.
fn sanitize_token_error_body(body: &str) -> String {
    let trimmed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.len() <= 200 {
        trimmed
    } else {
        let mut s = trimmed;
        s.truncate(200);
        s.push_str("…[truncated]");
        s
    }
}

/// Resolve a [`TokenSource`] (and any embedded project id) from the
/// environment: explicit SA key → ADC well-known file → metadata server.
fn resolve_token_source() -> Option<(TokenSource, Option<String>)> {
    if let Ok(path) = std::env::var(ENV_CREDS) {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Some(pair) = token_source_from_json(&contents) {
                return Some(pair);
            }
        }
    }
    if let Some(path) = adc_well_known_path() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Some(pair) = token_source_from_json(&contents) {
                return Some(pair);
            }
        }
    }
    // In-cluster fallback: assume a metadata server is reachable. The caller
    // still requires a project id, so this only activates with an explicit
    // GCP_PROJECT_ID/GOOGLE_CLOUD_PROJECT.
    if std::env::var(ENV_PROJECT).is_ok() || std::env::var(ENV_PROJECT_ALT).is_ok() {
        return Some((TokenSource::Metadata, None));
    }
    None
}

/// Parse a GCP credentials JSON (`service_account` or `authorized_user`).
fn token_source_from_json(contents: &str) -> Option<(TokenSource, Option<String>)> {
    let v: Value = serde_json::from_str(contents).ok()?;
    let cred_type = v.get("type").and_then(Value::as_str).unwrap_or("");
    let project = v
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    match cred_type {
        "service_account" => {
            let client_email = v.get("client_email").and_then(Value::as_str)?.to_string();
            let private_key_pem = v.get("private_key").and_then(Value::as_str)?.to_string();
            let token_uri = v
                .get("token_uri")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_TOKEN_URI)
                .to_string();
            Some((
                TokenSource::ServiceAccount {
                    client_email,
                    private_key_pem,
                    token_uri,
                },
                project,
            ))
        }
        "authorized_user" => {
            let client_id = v.get("client_id").and_then(Value::as_str)?.to_string();
            let client_secret = v.get("client_secret").and_then(Value::as_str)?.to_string();
            let refresh_token = v.get("refresh_token").and_then(Value::as_str)?.to_string();
            Some((
                TokenSource::AuthorizedUser {
                    client_id,
                    client_secret,
                    refresh_token,
                    token_uri: DEFAULT_TOKEN_URI.to_string(),
                },
                project,
            ))
        }
        _ => None,
    }
}

/// The ADC well-known credentials path (`~/.config/gcloud/...`).
fn adc_well_known_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(std::path::Path::new(&home).join(".config/gcloud/application_default_credentials.json"))
}

// ─── CloudProvider impl ──────────────────────────────────────────────────────

#[async_trait]
impl CloudProvider for GcpCloudProvider {
    async fn ensure_vpc_for_org(&self, org_id: Uuid) -> Result<VpcRef, CloudError> {
        let net_name = network_name(org_id);
        // 1. Network (create if missing; treat 409 as already-exists).
        let existing = self.get_optional(&self.network_url(&net_name)).await?;
        if existing.is_none() {
            tracing::info!(%org_id, network = %net_name, "creating per-org VPC network");
            let body = json!({
                "name": net_name,
                "autoCreateSubnetworks": false,
                "description": format!("G5 per-org VPC for org {org_id} (no peering)"),
            });
            if let Some(op) = self.post_create(&self.networks_url(), &body).await? {
                self.poll_operation(&op, "create network").await?;
            }
        }

        // The org-wide network does not pin a region; the subnet/router do.
        // `ensure_vpc_for_org` ensures the global network exists. The regional
        // subnet + router/NAT + firewalls are ensured here too so a subsequent
        // instance insert has everything it needs. Region is taken from the
        // network's first subnet's region at provision time; but since the
        // trait gives us org_id only, we ensure region-scoped resources lazily
        // in `provision_instance` (which has the region). Here we ensure the
        // global firewalls that are region-independent.
        self.ensure_firewalls(org_id, &net_name).await?;

        // Re-read for the authoritative self-link.
        let net = self
            .get_optional(&self.network_url(&net_name))
            .await?
            .ok_or_else(|| CloudError::Unexpected {
                message: format!("network {net_name} missing immediately after ensure"),
            })?;
        let network_self_link = net
            .get("selfLink")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        Ok(VpcRef {
            vpc_id: net_name,
            // Region is resolved per-instance; ensure_vpc returns the network
            // handle. The caller passes the concrete region into
            // provision_instance, which ensures the regional subnet/router.
            region: String::new(),
            network_self_link,
            subnetwork_self_link: None,
        })
    }

    /// Provision a host with its named boot disk and configured identity, retrying eligible zone failures.
    #[allow(clippy::too_many_arguments)]
    async fn provision_instance(
        &self,
        host_id: Uuid,
        org_slug: &str,
        created_by: Option<&ProvisioningActor>,
        vpc_ref: &VpcRef,
        instance_type: &str,
        region: &str,
        image: &str,
        startup_script: &str,
        boot_disk_gb: u32,
    ) -> Result<CloudInstanceRef, CloudError> {
        let net_name = &vpc_ref.vpc_id;
        // Ensure region-scoped network plumbing (subnet + router/NAT) exists.
        let subnet_self_link = self.ensure_subnetwork(net_name, region).await?;
        self.ensure_router_nat(net_name, region).await?;

        let primary_zone = self.zone_for(region);
        let mut zone = primary_zone.clone();
        let name = host_instance_name(host_id, org_slug);
        let machine_type = format!("zones/{zone}/machineTypes/{instance_type}");
        let org_tag = net_name.clone(); // g5-org-{org_id}
                                        // Derive the org id from the network name (`g5-org-{uuid}`) so we can
                                        // tag both labels per ontology [D19] without changing the trait shape.
                                        // The VPC name is built from a uuid string in `network_name`, so the
                                        // strip+parse is total; a malformed name is treated as "no org label"
                                        // rather than failing the insert (defensive only — `ensure_vpc_for_org`
                                        // built this name moments earlier).
        let mut labels = match net_name
            .strip_prefix("g5-org-")
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            Some(org_uuid) => json!({
                "g5_host_id": host_id.to_string().replace('-', ""),
                "g5_org_id": org_uuid.to_string().replace('-', ""),
            }),
            None => json!({
                "g5_host_id": host_id.to_string().replace('-', ""),
            }),
        };
        // Creator-provenance labels (ontology [D44]): the UUID label is the
        // stable join key back to `users`; the email label is the
        // human-readable one and is absent when the email lookup failed or
        // sanitizes to empty (GCE label values reject `@` and `.`).
        if let Some(actor) = created_by {
            labels["g5_created_by_id"] = json!(actor.user_id.to_string().replace('-', ""));
            if let Some(email_label) = actor
                .email
                .as_deref()
                .map(sanitize_gce_label_value)
                .filter(|v| !v.is_empty())
            {
                labels["g5_created_by"] = json!(email_label);
            }
        }

        // 3rd-gen machine series (C3/C3D/H3/Z3/M3) require BOTH a gVNIC NIC and
        // a non-pd-standard boot disk; older families (N2/N2D/C2) inherit the
        // GCE defaults. See ontology [D41] and the Codex/CodeRabbit review on
        // PR #325 for the gating-rule rationale; gating logic lives in the
        // unit-tested predicates [`requires_gvnic`] and [`boot_disk_type_for`].
        // Explicit per-host boot-disk size (ontology
        // `g5.runtime.placement.host-disk`, issues #707 and #344): without it
        // the disk defaults to the source image's size (~30 GB), and a single
        // provider-wide size strands the free slots of a large host. The
        // caller resolved `boot_disk_gb` from this host's VM budget; see
        // [`boot_disk_init`] for the units.
        //
        // Resolve-and-pin the outer boot image ([D56], issue #1424): resolve the
        // requested `image` (a `g5-worker` family ref by default) to its concrete
        // immutable image and pin `sourceImage` to that `selfLink`, so the row
        // records exactly what booted (`boot_image_id`) and a family advance
        // between resolve and insert cannot drift the record. Best-effort: on any
        // resolver error, fall back to the raw family ref and leave
        // `boot_image_id` None (the host converges on a later rotation) — a
        // boot-image lookup must never block provisioning ([D19]/[D24] posture).
        let (source_image, boot_image_id) = match self.resolve_image_identity(image).await {
            Ok(identity) => (identity.self_link, Some(identity.image_id)),
            Err(e) => {
                tracing::warn!(
                    %host_id,
                    worker_image = image,
                    error = %e,
                    "boot-image resolution failed; provisioning from the raw family ref (boot_image_id unset)"
                );
                (image.to_string(), None)
            }
        };
        let mut boot_init = boot_disk_init(&source_image, boot_disk_gb, instance_type, &zone);
        // IAM restricts boot-disk creation to the same g5-host- prefix as VMs.
        boot_init["diskName"] = json!(name);

        let mut network_interface = json!({
            "network": vpc_ref.network_self_link,
            "subnetwork": subnet_self_link,
            // No accessConfigs → no external IP; egress via Cloud NAT.
        });
        if requires_gvnic(instance_type) {
            network_interface["nicType"] = json!("GVNIC");
        }

        let mut body = json!({
            "name": name,
            "machineType": machine_type,
            "disks": [{
                "boot": true,
                "autoDelete": true,
                "initializeParams": boot_init,
            }],
            "networkInterfaces": [network_interface],
            "metadata": {
                "items": [{ "key": "startup-script", "value": startup_script }],
            },
            "tags": { "items": ["g5-worker", org_tag] },
            "labels": labels,
            // Attach the configured dedicated SA to the OUTER host so g5d can use
            // the metadata-server token when pulling private GAR runtime images.
            // This does not inject GCP credentials into the no-identity guest
            // microVM; it only restores parity with persistent worker-host VMs.
            "serviceAccounts": worker_host_service_accounts(&self.worker_service_account),
            // Cloud Hypervisor requires KVM. Without this flag the guest has no
            // `/dev/kvm` and the supervisor fail-closes in
            // `IsolationManager::new` with "No isolation backend available".
            // The persistent `module "worker_host"` already sets the
            // equivalent `advanced_machine_features.enable_nested_virtualization
            // = true`; the API path must match (caught live on us-east1-b in
            // the PR #277 monitor session, chat intent C3). The allowlist
            // (`WORKER_HOST_INSTANCE_TYPE_ALLOWLIST`) restricts `instance_type`
            // to nested-virt-capable families (N2/C2/C3 — see [D41]), so
            // this is always safe to request.
            "advancedMachineFeatures": {
                "enableNestedVirtualization": true,
            },
        });

        // Multi-zone fallback ([D59]): a stockout or invalid configured zone
        // can re-target the insert at the region's other UP zones. Discover
        // zones lazily on the first eligible failure, so the happy path never
        // pays that call. Other errors fail fast. When EVERY UP zone is
        // exhausted the error propagates and the route handler
        // classifies it as `zone_capacity_exhausted` ([D34]). The accepted
        // instance's zone is persisted even if its operation is still pending.
        let mut candidate_zones = vec![primary_zone.clone()];
        let mut attempt = 0usize;
        loop {
            zone = candidate_zones[attempt].clone();
            body["machineType"] = json!(format!("zones/{zone}/machineTypes/{instance_type}"));
            if let Some(disk_type) = boot_disk_type_for(instance_type, &zone) {
                body["disks"][0]["initializeParams"]["diskType"] = json!(disk_type);
            }

            tracing::info!(%host_id, %zone, %instance_type, "provisioning GCE instance");
            // The two insert phases are judged SEPARATELY (issue #627 Codex
            // review P2): only a failure of the CREATE REQUEST itself may
            // re-target another zone. Once GCE has ACCEPTED the insert, any
            // later uncertainty — poll timeout, or the operation object
            // reading back 404 — keeps the deterministic linkage for
            // reconciliation (`g5.runtime.host.cloud-linkage`: never a second
            // insert for the same identity in a sibling zone).
            let insert = match self.post_create(&self.instances_url(&zone), &body).await {
                Ok(op) => {
                    let op = op.ok_or_else(|| CloudError::Api {
                        status: 409,
                        message: format!("instance {name} already exists"),
                    })?;
                    match self.poll_operation(&op, "insert instance").await {
                        Ok(()) => break,
                        Err(CloudError::Timeout { .. }) => {
                            // The insert was accepted: preserve its deterministic
                            // reference even while the operation is pending. Existing
                            // reconciliation owns registration or eventual failure
                            // (g5.architecture.host_insert_acceptance, issue #400).
                            tracing::warn!(
                                %host_id, %zone, instance_name = %name,
                                "GCE insert still pending at poll cap; retaining instance linkage for reconciliation"
                            );
                            break;
                        }
                        Err(CloudError::NotFound { .. }) => {
                            // Accepted insert whose operation read back 404:
                            // uncertainty about the OPERATION object, not
                            // evidence against the zone or the instance.
                            tracing::warn!(
                                %host_id, %zone, instance_name = %name,
                                "GCE insert accepted but its operation read back 404; retaining instance linkage for reconciliation"
                            );
                            break;
                        }
                        // An op that COMPLETED with a stockout error created
                        // nothing — the zone walk below stays safe.
                        Err(e) if super::is_zone_capacity_exhausted(&e) => Err(e),
                        Err(e) => return Err(e),
                    }
                }
                // Request-level failure: judged for zone fallback below.
                Err(e) => Err(e),
            };

            match insert {
                Ok(()) => break,
                // GCE also returns an ambiguous 403 for unknown zones (#1000).
                // Only the initial request may recover this way, and only if
                // zone discovery supplies alternatives but excludes the
                // configured preference. Otherwise retain the original IAM
                // error, including when discovery itself fails.
                Err(e @ CloudError::AuthDenied { status: 403, .. }) if attempt == 0 => {
                    let up = match self.list_up_zones_in_region(region).await {
                        Ok(up) => up,
                        Err(list_err) => {
                            tracing::warn!(
                                %host_id, %zone, error = %list_err,
                                "could not list region zones after insert denial; retaining the original authorization failure"
                            );
                            return Err(e);
                        }
                    };
                    if up.is_empty() || up.contains(&primary_zone) {
                        return Err(e);
                    }
                    tracing::warn!(
                        %host_id, %zone,
                        "worker-host insert denied for a configured zone outside the region's UP set; trying available zones"
                    );
                    candidate_zones = ordered_fallback_zones(&primary_zone, &up);
                    attempt = 1;
                }
                // Stockout AND unknown-zone both walk the region's real UP
                // zones: an invalid configured zone preference (`us-east1` has
                // no `-a` — issue #627 S33) may surface as a 404 on the CREATE
                // REQUEST and must degrade to the region's valid zones instead
                // of failing closed (`g5.runtime.host.cloud-linkage`).
                Err(e)
                    if super::is_zone_capacity_exhausted(&e)
                        || matches!(e, CloudError::NotFound { .. }) =>
                {
                    if super::is_zone_capacity_exhausted(&e) {
                        tracing::warn!(
                            %host_id, %zone,
                            "worker-host zone stockout (ZONE_RESOURCE_POOL_EXHAUSTED); trying next zone"
                        );
                    } else {
                        tracing::warn!(
                            %host_id, %zone,
                            "worker-host insert target not found (invalid/unknown zone?); trying the region's real zones"
                        );
                    }
                    // Expand candidates on the FIRST stockout/not-found only.
                    if candidate_zones.len() == 1 {
                        match self.list_up_zones_in_region(region).await {
                            Ok(up) => candidate_zones = ordered_fallback_zones(&primary_zone, &up),
                            Err(list_err) => {
                                // Return the LISTING error, not the stockout: we
                                // never got to check the fallback zones, so this
                                // is not "no capacity in any zone". A non-stockout
                                // error stays unclassified ([D34]) and the
                                // reconciler retries, rather than being pinned as
                                // `zone_capacity_exhausted` on unverified grounds.
                                tracing::warn!(
                                    %host_id, error = %list_err,
                                    "could not list region zones for stockout fallback; returning the listing error"
                                );
                                return Err(list_err);
                            }
                        }
                    }
                    attempt += 1;
                    if attempt >= candidate_zones.len() {
                        return Err(e);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Read back IPs (best-effort — not required by 9c).
        let (internal_ip, external_ip) =
            match self.get_optional(&self.instance_url(&zone, &name)).await {
                Ok(Some(inst)) => extract_ips(&inst),
                _ => (None, None),
            };

        Ok(CloudInstanceRef {
            cloud_instance_id: encode_instance_id(&zone, &name),
            internal_ip,
            external_ip,
            zone: Some(zone),
            boot_image_id,
        })
    }

    async fn terminate_instance(&self, cloud_instance_id: &str) -> Result<(), CloudError> {
        let (zone, name) = decode_instance_id(cloud_instance_id)?;
        let (status, body) = self
            .request(
                reqwest::Method::DELETE,
                &self.instance_url(zone, name),
                None,
            )
            .await?;
        match status {
            200..=299 => {
                match self.poll_operation(&body, "delete instance").await {
                    Ok(()) => Ok(()),
                    // A delete operation that times out is PENDING, and an
                    // operation-level "already gone" error is SUCCESSFUL
                    // cleanup (issue #627 S34/S35,
                    // `g5.runtime.host.cloud-linkage`): confirm absence with
                    // one GET before deciding. 404 ⇒ the instance is gone ⇒
                    // success (this also resolves competing deletes); still
                    // present or unreadable ⇒ keep the original error so the
                    // caller retries instead of declaring unverified success.
                    Err(poll_err) => {
                        match self.get_optional(&self.instance_url(zone, name)).await {
                            Ok(None) => Ok(()),
                            Ok(Some(_)) | Err(_) => Err(poll_err),
                        }
                    }
                }
            }
            // Already gone — terminate is idempotent / best-effort.
            404 => Ok(()),
            _ => Err(api_error(status, &body, &self.instance_url(zone, name))),
        }
    }

    async fn get_instance_status(
        &self,
        cloud_instance_id: &str,
    ) -> Result<InstanceState, CloudError> {
        let (zone, name) = decode_instance_id(cloud_instance_id)?;
        match self.get_optional(&self.instance_url(zone, name)).await? {
            Some(inst) => {
                let status = inst.get("status").and_then(Value::as_str).unwrap_or("");
                Ok(map_gce_status(status))
            }
            // 404 → the instance no longer exists.
            None => Ok(InstanceState::Terminated),
        }
    }

    async fn get_instance_by_name(
        &self,
        region: &str,
        name: &str,
    ) -> Result<Option<FoundInstance>, CloudError> {
        // Search the primary zone first, then the region's other UP zones — the
        // SAME primary-first set `provision_instance` provisions into ([D59]). A
        // stockout fallback can place the instance in a non-primary zone, so a
        // primary-only lookup would 404 and the orphan-reconciler ([D39]) could
        // miss (and leak) a live host, or mis-count it toward the terminate
        // threshold. The zone that answers is reported on `FoundInstance.zone`
        // so attach/repoint callers encode the true linkage ([D61]) instead of
        // re-deriving the configured primary.
        //
        // The scan is ALIVE-PREFERRED ([D61], Codex design-review blocker 3):
        // a non-alive hit (e.g. a TERMINATED primary-zone object left behind
        // by a failed/stopped attempt) must not short-circuit the search and
        // mask a live fallback VM in a sibling zone — a live hit anywhere
        // wins; when nothing alive exists, selection priority is
        // transient-existing (Stopping/Unknown) > Terminated > absent (Codex
        // round-2 finding 3) — an instance that still EXISTS somewhere must
        // never read as confirmed gone. The UP-zone list is fetched LAZILY —
        // only when the primary zone yields no LIVE hit — so the common
        // healthy-primary case pays no extra call. If the zones list can't be
        // fetched we return the error (fail safe: the reconciler treats `Err`
        // as "no change, retry" rather than "gone").
        let primary_zone = self.zone_for(region);
        let mut first_transient: Option<FoundInstance> = None;
        let mut first_terminated: Option<FoundInstance> = None;

        if let Some(inst) = self
            .get_optional(&self.instance_url(&primary_zone, name))
            .await?
        {
            let status = inst.get("status").and_then(Value::as_str).unwrap_or("");
            let state = map_gce_status(status);
            let found = FoundInstance {
                state,
                zone: primary_zone.clone(),
            };
            if matches!(state, InstanceState::Running | InstanceState::Provisioning) {
                return Ok(Some(found));
            }
            if matches!(state, InstanceState::Terminated) {
                first_terminated.get_or_insert(found);
            } else {
                first_transient.get_or_insert(found);
            }
        }

        let up_zones = self.list_up_zones_in_region(region).await?;
        for zone in ordered_fallback_zones(&primary_zone, &up_zones)
            .into_iter()
            .skip(1)
        {
            if let Some(inst) = self.get_optional(&self.instance_url(&zone, name)).await? {
                let status = inst.get("status").and_then(Value::as_str).unwrap_or("");
                let state = map_gce_status(status);
                let found = FoundInstance { state, zone };
                if matches!(state, InstanceState::Running | InstanceState::Provisioning) {
                    return Ok(Some(found));
                }
                if matches!(state, InstanceState::Terminated) {
                    first_terminated.get_or_insert(found);
                } else {
                    first_transient.get_or_insert(found);
                }
            }
        }

        // Nothing ALIVE anywhere: a transient hit (Stopping/Unknown) outranks
        // a terminated one — the caller must not treat "an instance still
        // exists somewhere" as confirmed gone. Else the first terminated hit,
        // else a full 404 — the orphan-reconciler counts the latter two
        // toward abandonment ([D39]).
        Ok(first_transient.or(first_terminated))
    }

    async fn list_instances_by_prefix(
        &self,
        region: &str,
        prefix: &str,
    ) -> Result<Vec<ListedInstance>, CloudError> {
        // Stale-host detector cloud-orphan sweep ([D65], issue #1464): list
        // every instance matching the prefix across the SAME primary-first
        // UP-zone set provisioning can place instances in ([D59]), so the
        // search space equals the leak space. A zone-listing failure is
        // returned (fail safe — the sweep skips the region this pass) rather
        // than treated as an empty fleet. The preferred zone is included only
        // when it is a real UP zone — an invalid configured suffix must not
        // abort the whole region's cleanup (issue #627 S33).
        let primary_zone = self.zone_for(region);
        let up_zones = self.list_up_zones_in_region(region).await?;
        // `name eq "<prefix>.*"` — GCE's legacy regexp-style list filter; the
        // prefix is a name fragment ([a-z0-9-] only, [D44] shape), so the
        // manual %-encoding below covers every character that can appear.
        let filter = format!("name%20eq%20%22{prefix}.*%22");
        let mut out: Vec<ListedInstance> = Vec::new();
        for zone in ordered_up_zones(&primary_zone, &up_zones) {
            let mut page_token: Option<String> = None;
            // Bounded pagination: the worker fleet is tiny (tens of
            // instances); 10 pages x 500 is a defensive ceiling, not a quota.
            for _ in 0..10 {
                let url = match &page_token {
                    Some(token) => {
                        // nextPageToken is opaque — percent-encode so `+`/`/`/`=`
                        // survive the round-trip.
                        let encoded: String =
                            url::form_urlencoded::byte_serialize(token.as_bytes()).collect();
                        format!(
                            "{}?filter={filter}&maxResults=500&pageToken={encoded}",
                            self.instances_url(&zone)
                        )
                    }
                    None => format!(
                        "{}?filter={filter}&maxResults=500",
                        self.instances_url(&zone)
                    ),
                };
                let (status, body) = self.request(reqwest::Method::GET, &url, None).await?;
                if !(200..=299).contains(&status) {
                    return Err(api_error(status, &body, &url));
                }
                if let Some(items) = body.get("items").and_then(Value::as_array) {
                    for inst in items {
                        let name = inst
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if name.is_empty() || !name.starts_with(prefix) {
                            continue;
                        }
                        let state = map_gce_status(
                            inst.get("status").and_then(Value::as_str).unwrap_or(""),
                        );
                        out.push(ListedInstance {
                            name,
                            zone: zone.clone(),
                            state,
                        });
                    }
                }
                page_token = body
                    .get("nextPageToken")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if page_token.is_none() {
                    break;
                }
            }
        }
        Ok(out)
    }

    fn worker_image(&self) -> &str {
        &self.worker_image
    }

    async fn resolve_boot_image(&self) -> Result<BootImageIdentity, CloudError> {
        self.resolve_image_identity(&self.worker_image).await
    }
}

impl GcpCloudProvider {
    /// Ensure the firewall rules for the org network exist (ontology [D18]):
    /// allow-internal, and (optionally) allow-ssh. GCP's implied deny-all
    /// ingress already provides default-deny — no explicit deny rule is created.
    async fn ensure_firewalls(&self, org_id: Uuid, net_name: &str) -> Result<(), CloudError> {
        let allow_internal = format!("{net_name}-allow-internal");
        if self
            .get_optional(&format!("{}/{allow_internal}", self.firewalls_url()))
            .await?
            .is_none()
        {
            // Use the summary range that covers EVERY region's /20 (ontology
            // [D18]). A single fixed `/20` would only authorize one region's
            // subnet; a per-region rule would either explode rule count or
            // require an update on every new region. The summary `/12` is
            // contained to the per-org VPC (firewalls are network-scoped),
            // so cross-org leakage is impossible regardless of the wider
            // CIDR.
            let body = json!({
                "name": allow_internal,
                "network": format!("global/networks/{net_name}"),
                "direction": "INGRESS",
                "priority": 1000,
                "sourceRanges": [INTERNAL_SUMMARY_CIDR],
                "allowed": [
                    { "IPProtocol": "tcp" },
                    { "IPProtocol": "udp" },
                    { "IPProtocol": "icmp" },
                ],
                "description": format!("Allow intra-VPC traffic for org {org_id}"),
            });
            if let Some(op) = self.post_create(&self.firewalls_url(), &body).await? {
                self.poll_operation(&op, "create allow-internal firewall")
                    .await?;
            }
        }

        if self.allow_ssh {
            let allow_ssh = format!("{net_name}-allow-ssh");
            // IAP TCP-forwarding range — the documented Google-owned CIDR that
            // originates IAP-tunneled SSH/RDP connections
            // (https://cloud.google.com/iap/docs/using-tcp-forwarding). Three
            // layers of access control gate SSH: (1) only IAP-originated
            // traffic matches this range; (2) IAP itself requires
            // `roles/iap.tunnelResourceAccessor` on the project / instance;
            // (3) SSH still needs a valid key via OS Login or instance
            // metadata. The previous `0.0.0.0/0` was wrong: it's a public-
            // internet allow, only safe-by-accident because instances have no
            // external IP. Caught in the PR #277 monitor session, chat
            // intent C3.
            const IAP_SOURCE_RANGE: &str = "35.235.240.0/20";
            let desired_body = json!({
                "name": allow_ssh,
                "network": format!("global/networks/{net_name}"),
                "direction": "INGRESS",
                "priority": 1000,
                "sourceRanges": [IAP_SOURCE_RANGE],
                "targetTags": ["g5-worker"],
                "allowed": [{ "IPProtocol": "tcp", "ports": ["22"] }],
                "description": "Operator IAP SSH access (G5_WORKER_ALLOW_SSH)",
            });
            let existing = self
                .get_optional(&format!("{}/{allow_ssh}", self.firewalls_url()))
                .await?;
            match existing {
                None => {
                    if let Some(op) = self
                        .post_create(&self.firewalls_url(), &desired_body)
                        .await?
                    {
                        self.poll_operation(&op, "create allow-ssh firewall")
                            .await?;
                    }
                }
                Some(existing) => {
                    // Patch existing rules whose `sourceRanges` doesn't match
                    // the IAP-only target — catches the wider `0.0.0.0/0`
                    // rules created by the pre-PR-#280 code (codex P2 on PR
                    // #280). Idempotent: rules already at IAP-only are
                    // skipped without an extra API call.
                    let needs_update = existing
                        .get("sourceRanges")
                        .and_then(Value::as_array)
                        .map(|arr| arr.len() != 1 || arr[0].as_str() != Some(IAP_SOURCE_RANGE))
                        .unwrap_or(true);
                    if needs_update {
                        tracing::warn!(
                            firewall = %allow_ssh,
                            "patching SSH firewall rule to IAP-only source range",
                        );
                        let (status, body) = self
                            .request(
                                reqwest::Method::PATCH,
                                &format!("{}/{allow_ssh}", self.firewalls_url()),
                                Some(&desired_body),
                            )
                            .await?;
                        if !(200..=299).contains(&status) {
                            return Err(api_error(
                                status,
                                &body,
                                &format!("{}/{allow_ssh}", self.firewalls_url()),
                            ));
                        }
                        // The PATCH response is itself a long-running operation
                        // body — poll it to DONE so subsequent ensure passes
                        // see the new state.
                        self.poll_operation(&body, "patch allow-ssh firewall")
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Ensure the regional subnetwork exists; returns its self-link.
    ///
    /// Each region maps to its own /20 via [`REGION_SUBNET_CIDR_TABLE`] so two
    /// regions in the same org never collide on `ipCidrRange` (subnet
    /// primary ranges must be unique within a VPC; ontology [D18], opus48
    /// round-2 B-2). The route boundary already rejects off-allowlist
    /// regions (`WORKER_HOST_REGION_ALLOWLIST` in `g5_contracts`), so reaching
    /// this branch with `None` is a configuration drift between the two
    /// tables — we surface it as `InvalidConfig` rather than fail later in
    /// the GCP request.
    async fn ensure_subnetwork(&self, net_name: &str, region: &str) -> Result<String, CloudError> {
        let cidr = region_subnet_cidr(region).ok_or_else(|| CloudError::InvalidConfig {
            message: format!(
                "region {region} has no entry in REGION_SUBNET_CIDR_TABLE; \
                 keep g5_contracts WORKER_HOST_REGION_ALLOWLIST and the CIDR table in sync"
            ),
        })?;
        let sub_name = format!("{net_name}-{region}");
        if self
            .get_optional(&self.subnetwork_url(region, &sub_name))
            .await?
            .is_none()
        {
            let body = json!({
                "name": sub_name,
                "network": format!("global/networks/{net_name}"),
                "ipCidrRange": cidr,
                "region": region,
                "privateIpGoogleAccess": true,
            });
            if let Some(op) = self
                .post_create(&self.subnetworks_url(region), &body)
                .await?
            {
                self.poll_operation(&op, "create subnetwork").await?;
            }
        }
        let sub = self
            .get_optional(&self.subnetwork_url(region, &sub_name))
            .await?
            .ok_or_else(|| CloudError::Unexpected {
                message: format!("subnetwork {sub_name} missing immediately after ensure"),
            })?;
        Ok(sub
            .get("selfLink")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
    }

    /// Ensure a Cloud Router + NAT for outbound egress (ontology [D18]).
    async fn ensure_router_nat(&self, net_name: &str, region: &str) -> Result<(), CloudError> {
        let router_name = format!("{net_name}-{region}-router");
        if self
            .get_optional(&self.router_url(region, &router_name))
            .await?
            .is_some()
        {
            return Ok(());
        }
        let body = json!({
            "name": router_name,
            "network": format!("global/networks/{net_name}"),
            "region": region,
            "nats": [{
                "name": format!("{net_name}-{region}-nat"),
                "natIpAllocateOption": "AUTO_ONLY",
                "sourceSubnetworkIpRangesToNat": "ALL_SUBNETWORKS_ALL_IP_RANGES",
            }],
        });
        if let Some(op) = self.post_create(&self.routers_url(region), &body).await? {
            self.poll_operation(&op, "create router+nat").await?;
        }
        Ok(())
    }
}

/// Extract the primary internal + external IPs from an instance resource.
fn extract_ips(inst: &Value) -> (Option<String>, Option<String>) {
    let nic = inst
        .get("networkInterfaces")
        .and_then(Value::as_array)
        .and_then(|a| a.first());
    let internal = nic
        .and_then(|n| n.get("networkIP"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let external = nic
        .and_then(|n| n.get("accessConfigs"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|c| c.get("natIP"))
        .and_then(Value::as_str)
        .map(str::to_string);
    (internal, external)
}

// ─── Tests (pure helpers only — no live GCP) ─────────────────────────────────

/// The self-hosted license service's registry token issuer, when the
/// deployment has GCP credentials; `None` disables registry pull tokens.
pub fn registry_token_issuer_from_env() -> Option<Arc<dyn RegistryTokenIssuer>> {
    GcpCloudProvider::from_env().map(|provider| {
        Arc::new(GcpRegistryTokenIssuer { provider }) as Arc<dyn RegistryTokenIssuer>
    })
}

/// The registry pull token comes from the deployment's own GCP identity.
struct GcpRegistryTokenIssuer {
    provider: GcpCloudProvider,
}

#[async_trait]
impl RegistryTokenIssuer for GcpRegistryTokenIssuer {
    async fn mint_access_token(
        &self,
        service_account_email: &str,
        lifetime: Duration,
    ) -> Result<MintedRegistryToken, G5Error> {
        let (access_token, expires_in) = self
            .provider
            .generate_service_account_access_token(service_account_email, lifetime)
            .await
            .map_err(gcp_token_error)?;

        Ok(MintedRegistryToken {
            access_token,
            expires_in,
        })
    }
}

fn gcp_token_error(error: CloudError) -> G5Error {
    G5Error::ServiceUnavailable {
        message: format!("failed to mint registry pull token: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use g5_contracts::types::worker_host::WorkerHostBootDiskSizing;

    #[test]
    fn gce_status_mapping() {
        assert_eq!(map_gce_status("PROVISIONING"), InstanceState::Provisioning);
        assert_eq!(map_gce_status("STAGING"), InstanceState::Provisioning);
        assert_eq!(map_gce_status("RUNNING"), InstanceState::Running);
        assert_eq!(map_gce_status("STOPPING"), InstanceState::Stopping);
        assert_eq!(map_gce_status("TERMINATED"), InstanceState::Terminated);
        assert_eq!(map_gce_status("SUSPENDED"), InstanceState::Terminated);
        assert_eq!(map_gce_status("WAT"), InstanceState::Unknown);
    }

    #[test]
    fn network_name_uses_org_uuid() {
        let org = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(
            network_name(org),
            "g5-org-11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn requires_gvnic_gates_on_c3_prefix_only() {
        // C3 — all allowlisted SKUs — must require gVNIC (D41 + Codex P1 #2).
        assert!(requires_gvnic("c3-standard-4"));
        assert!(requires_gvnic("c3-standard-8"));
        assert!(requires_gvnic("c3-standard-22"));
        assert!(requires_gvnic("c3-standard-44"));
        // Older 1st/2nd-gen families: GCE default NIC is fine — setting GVNIC
        // here would regress provisions against the currently-deployed image
        // (which lacks the GVNIC guestOsFeature tag).
        assert!(!requires_gvnic("n2-standard-8"));
        assert!(!requires_gvnic("n2d-standard-16"));
        assert!(!requires_gvnic("c2-standard-30"));
        // A made-up unrelated prefix must not trip the gate.
        assert!(!requires_gvnic("e2-standard-4"));
    }

    #[test]
    fn is_zone_capacity_exhausted_detects_stockout_only() {
        use crate::cloud::is_zone_capacity_exhausted;
        // Canonical GCP reason (embedded in the operation error JSON) — stockout.
        assert!(is_zone_capacity_exhausted(&CloudError::Api {
            status: 0,
            message: "operation insert instance failed: {\"errors\":[{\"code\":\"ZONE_RESOURCE_POOL_EXHAUSTED\"}]}".into(),
        }));
        // `_WITH_DETAILS` variant and mixed casing still classify.
        assert!(is_zone_capacity_exhausted(&CloudError::Api {
            status: 503,
            message: "Zone_Resource_Pool_Exhausted_With_Details: ...".into(),
        }));
        // Human phrase GCP folds into error.message only.
        assert!(is_zone_capacity_exhausted(&CloudError::Api {
            status: 503,
            message: "The zone 'us-central1-b' does not have enough resources".into(),
        }));
        // A different Api error is NOT a stockout — must fail fast, not retry zones.
        assert!(!is_zone_capacity_exhausted(&CloudError::Api {
            status: 500,
            message: "internal error".into(),
        }));
        // Non-Api variants are never "no capacity".
        assert!(!is_zone_capacity_exhausted(&CloudError::Timeout {
            operation: "insert instance".into(),
        }));
    }

    /// Exercise host provisioning against a local operation response and optional zone stockout.
    async fn provision_with_mock_operation(
        operation: Value,
        stockout_first: bool,
    ) -> Result<CloudInstanceRef, CloudError> {
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let image = "projects/test-project/global/images/g5-worker-pinned";
        Mock::given(method("GET"))
            .and(path_regex("/regions/us-central1/subnetworks/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "selfLink": "projects/test-project/regions/us-central1/subnetworks/test",
            })))
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex("/regions/us-central1/routers/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{image}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "g5-worker-pinned",
                "selfLink": format!("{COMPUTE_BASE}/{image}"),
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {"name": "us-central1-a", "status": "UP", "region": "regions/us-central1"},
                    {"name": "us-central1-b", "status": "UP", "region": "regions/us-central1"},
                ],
            })))
            .expect(u64::from(stockout_first))
            .mount(&server)
            .await;

        for zone in ["us-central1-a", "us-central1-b"] {
            let attempted = zone == "us-central1-a" || stockout_first;
            let op_path = format!("/projects/test-project/zones/{zone}/operations/insert");
            Mock::given(method("POST"))
                .and(path(format!(
                    "/projects/test-project/zones/{zone}/instances"
                )))
                .and(wiremock::matchers::body_partial_json(json!({
                    "serviceAccounts": [{"email": "g5-test-host@test-project.iam.gserviceaccount.com", "scopes": [OAUTH_SCOPE]}],
                    "disks": [{"boot": true, "autoDelete": true, "initializeParams": {
                        "diskName": host_instance_name(Uuid::nil(), "test-org"),
                        "sourceImage": format!("{COMPUTE_BASE}/{image}"),
                        "diskSizeGb": "100", "diskType": format!("zones/{zone}/diskTypes/pd-balanced"),
                    }}],
                })))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "status": "RUNNING",
                    "selfLink": format!("{COMPUTE_BASE}{op_path}"),
                })))
                .expect(u64::from(attempted))
                .mount(&server)
                .await;
            let body = if stockout_first && zone == "us-central1-a" {
                json!({"status": "DONE", "error": {
                    "errors": [{"code": "ZONE_RESOURCE_POOL_EXHAUSTED"}],
                }})
            } else {
                operation.clone()
            };
            Mock::given(method("GET"))
                .and(path(op_path))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
        }
        // A pending instance may not have IPs yet; linkage must still survive.
        Mock::given(method("GET"))
            .and(path_regex("/zones/us-central1-[ab]/instances/.*"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let provider = GcpCloudProvider {
            project_id: "test-project".to_string(),
            worker_image: image.to_string(),
            worker_service_account: "g5-test-host@test-project.iam.gserviceaccount.com".to_string(),
            zone_suffix: "a".to_string(),
            allow_ssh: false,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            token_source: TokenSource::Metadata,
            token_cache: Mutex::new(Some(CachedToken {
                access_token: "test-token".to_string(),
                expires_at: Instant::now() + Duration::from_secs(3600),
            })),
            compute_base_override: Some(server.uri()),
        };
        let vpc = VpcRef {
            vpc_id: network_name(Uuid::nil()),
            region: "us-central1".to_string(),
            network_self_link: "global/networks/test".to_string(),
            subnetwork_self_link: None,
        };
        provider
            .provision_instance(
                Uuid::nil(),
                "test-org",
                None,
                &vpc,
                "c3-standard-4",
                "us-central1",
                image,
                "#!/bin/sh",
                100,
            )
            .await
    }

    #[tokio::test(start_paused = true)]
    async fn provision_instance_retains_linkage_when_accepted_insert_poll_times_out() {
        // Exercise both the primary insert and a pending fallback after a
        // confirmed stockout. Pending operations must never start another VM.
        for stockout_first in [false, true] {
            let start = tokio::time::Instant::now();
            let instance =
                provision_with_mock_operation(json!({"status": "RUNNING"}), stockout_first)
                    .await
                    .expect("accepted insert retains its reference at the poll cap");
            assert!(start.elapsed() >= OP_POLL_MAX);
            let zone = if stockout_first {
                "us-central1-b"
            } else {
                "us-central1-a"
            };
            assert_eq!(
                instance.cloud_instance_id,
                encode_instance_id(zone, &host_instance_name(Uuid::nil(), "test-org"))
            );
            assert_eq!(instance.zone.as_deref(), Some(zone));
            assert_eq!(instance.boot_image_id.as_deref(), Some("g5-worker-pinned"));
            assert_eq!(instance.internal_ip, None);
            assert_eq!(instance.external_ip, None);
        }
    }

    #[tokio::test]
    async fn provision_instance_preserves_completed_insert_errors() {
        let error = provision_with_mock_operation(
            json!({"status": "DONE", "error": {"errors": [{"code": "QUOTA_EXCEEDED"}]}}),
            false,
        )
        .await
        .expect_err("a completed operation failure must still fail provisioning");
        assert!(
            matches!(error, CloudError::Api { message, .. } if message.contains("QUOTA_EXCEEDED"))
        );
    }

    #[test]
    fn ordered_fallback_zones_primary_first_then_rest_deduped() {
        let up = vec![
            "us-central1-a".to_string(),
            "us-central1-b".to_string(),
            "us-central1-c".to_string(),
            "us-central1-f".to_string(),
        ];
        // Primary stays at index 0; the remaining UP zones follow, primary de-duped.
        assert_eq!(
            ordered_fallback_zones("us-central1-b", &up),
            vec![
                "us-central1-b",
                "us-central1-a",
                "us-central1-c",
                "us-central1-f"
            ]
        );
        // Empty UP list → just the primary (caller gives up after the original).
        assert_eq!(
            ordered_fallback_zones("us-central1-b", &[]),
            vec!["us-central1-b"]
        );
        // A primary absent from the UP list is still tried first, then the rest.
        assert_eq!(
            ordered_fallback_zones("us-central1-x", &["us-central1-a".to_string()]),
            vec!["us-central1-x", "us-central1-a"]
        );
    }

    #[test]
    fn ordered_up_zones_includes_preference_only_when_it_is_real() {
        let up = vec![
            "us-east1-b".to_string(),
            "us-east1-c".to_string(),
            "us-east1-d".to_string(),
        ];
        // us-east1 has no `-a` zone (issue #627 S33): an invalid configured
        // preference must NOT be injected into read/cleanup paths.
        assert_eq!(
            ordered_up_zones("us-east1-a", &up),
            vec!["us-east1-b", "us-east1-c", "us-east1-d"]
        );
        // A real preference stays first.
        assert_eq!(
            ordered_up_zones("us-east1-c", &up),
            vec!["us-east1-c", "us-east1-b", "us-east1-d"]
        );
    }

    #[test]
    fn api_error_classifies_rate_limited_403_as_retryable_api_error() {
        // Documented rate-limit 403s (issue #627 S20) are throughput limits,
        // not IAM denials: they must not land in AuthDenied.
        let body = json!({"error": {
            "code": 403,
            "message": "Rate Limit Exceeded",
            "errors": [{"reason": "rateLimitExceeded", "domain": "usageLimits"}],
        }});
        assert!(matches!(
            api_error(403, &body, "https://example/compute"),
            CloudError::Api { status: 403, .. }
        ));

        // A permission 403 keeps the dedicated auth-denied classification.
        let denied = json!({"error": {
            "code": 403,
            "message": "Required 'compute.instances.create' permission",
            "errors": [{"reason": "forbidden"}],
        }});
        assert!(matches!(
            api_error(403, &denied, "https://example/compute"),
            CloudError::AuthDenied { status: 403, .. }
        ));

        // 401 stays auth-denied regardless of body.
        assert!(matches!(
            api_error(401, &json!({}), "https://example/compute"),
            CloudError::AuthDenied { status: 401, .. }
        ));
    }

    /// Exercise host deletion against controlled operation and post-delete instance responses.
    async fn terminate_with_mock(
        op_body: Value,
        instance_after_delete: Option<Value>,
    ) -> Result<(), CloudError> {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let instance_path = "/projects/test-project/zones/us-central1-a/instances/g5-host-x";
        let op_path = "/projects/test-project/zones/us-central1-a/operations/delete";
        Mock::given(method("DELETE"))
            .and(path(instance_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "RUNNING",
                "selfLink": format!("{COMPUTE_BASE}{op_path}"),
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(op_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(op_body))
            .mount(&server)
            .await;
        match instance_after_delete {
            Some(instance) => {
                Mock::given(method("GET"))
                    .and(path(instance_path))
                    .respond_with(ResponseTemplate::new(200).set_body_json(instance))
                    .mount(&server)
                    .await;
            }
            None => {
                Mock::given(method("GET"))
                    .and(path(instance_path))
                    .respond_with(ResponseTemplate::new(404))
                    .mount(&server)
                    .await;
            }
        }

        let provider = GcpCloudProvider {
            project_id: "test-project".to_string(),
            worker_image: "projects/test-project/global/images/family/g5-worker".to_string(),
            worker_service_account: "g5-test-host@test-project.iam.gserviceaccount.com".to_string(),
            zone_suffix: "a".to_string(),
            allow_ssh: false,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            token_source: TokenSource::Metadata,
            token_cache: Mutex::new(Some(CachedToken {
                access_token: "test-token".to_string(),
                expires_at: Instant::now() + Duration::from_secs(3600),
            })),
            compute_base_override: Some(server.uri()),
        };
        provider.terminate_instance("us-central1-a/g5-host-x").await
    }

    #[tokio::test]
    async fn terminate_treats_operation_level_already_gone_as_success() {
        // The delete operation reports the resource already gone (issue #627
        // S35); the confirming GET 404s ⇒ cleanup succeeded. Without this,
        // cleanup_retired can never confirm `cloud_deleted_at` and the
        // automatic-resource bound stays held forever.
        let op = json!({"status": "DONE", "error": {
            "errors": [{"code": "RESOURCE_NOT_FOUND",
                        "message": "The resource ... was not found"}],
        }});
        terminate_with_mock(op, None)
            .await
            .expect("absence-confirmed delete reports success");
    }

    #[tokio::test]
    async fn terminate_keeps_failure_when_instance_still_present() {
        // The operation failed and the instance is STILL there: the original
        // error must propagate — unverified success would strand the VM
        // (issue #627 S34: unknown provider state stays uncertain).
        let op = json!({"status": "DONE", "error": {
            "errors": [{"code": "INTERNAL_ERROR", "message": "backend error"}],
        }});
        let err = terminate_with_mock(op, Some(json!({"status": "RUNNING"})))
            .await
            .expect_err("a failed delete with a live instance stays an error");
        assert!(matches!(err, CloudError::Api { .. }));
    }

    /// Retain accepted host linkage when operation polling loses the operation resource.
    #[tokio::test]
    async fn provision_instance_keeps_accepted_insert_when_operation_reads_back_404() {
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Issue #627 Codex review P2: once GCE ACCEPTED the insert, a 404 on
        // the returned operation object is uncertainty about the operation —
        // never a license to insert the same identity into a sibling zone
        // (that duplicated VMs). The linkage is retained for reconciliation
        // and the zone walk must not start (no zones.list call).
        let server = MockServer::start().await;
        let image = "projects/test-project/global/images/g5-worker-pinned";
        Mock::given(method("GET"))
            .and(path_regex("/regions/us-central1/subnetworks/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "selfLink": "projects/test-project/regions/us-central1/subnetworks/test",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex("/regions/us-central1/routers/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{image}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "g5-worker-pinned",
                "selfLink": format!("{COMPUTE_BASE}/{image}"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {"name": "us-central1-a", "status": "UP", "region": "regions/us-central1"},
                    {"name": "us-central1-b", "status": "UP", "region": "regions/us-central1"},
                ],
            })))
            .expect(0)
            .mount(&server)
            .await;
        let op_path = "/projects/test-project/zones/us-central1-a/operations/insert";
        Mock::given(method("POST"))
            .and(path("/projects/test-project/zones/us-central1-a/instances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "RUNNING",
                "selfLink": format!("{COMPUTE_BASE}{op_path}"),
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(op_path))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "error": {"code": 404, "message": "operation not found"},
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/projects/test-project/zones/us-central1-b/instances"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex("/zones/us-central1-[ab]/instances/.*"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let provider = GcpCloudProvider {
            project_id: "test-project".to_string(),
            worker_image: image.to_string(),
            worker_service_account: "g5-test-host@test-project.iam.gserviceaccount.com".to_string(),
            zone_suffix: "a".to_string(),
            allow_ssh: false,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            token_source: TokenSource::Metadata,
            token_cache: Mutex::new(Some(CachedToken {
                access_token: "test-token".to_string(),
                expires_at: Instant::now() + Duration::from_secs(3600),
            })),
            compute_base_override: Some(server.uri()),
        };
        let vpc = VpcRef {
            vpc_id: network_name(Uuid::nil()),
            region: "us-central1".to_string(),
            network_self_link: "global/networks/test".to_string(),
            subnetwork_self_link: None,
        };
        let instance = provider
            .provision_instance(
                Uuid::nil(),
                "test-org",
                None,
                &vpc,
                "c3-standard-4",
                "us-central1",
                image,
                "#!/bin/sh",
                100,
            )
            .await
            .expect("accepted insert retains its reference when the op reads back 404");
        assert_eq!(
            instance.cloud_instance_id,
            encode_instance_id(
                "us-central1-a",
                &host_instance_name(Uuid::nil(), "test-org")
            )
        );
    }

    /// Exercise zone selection through the provider's HTTP interface.
    async fn provision_with_mock_zone_responses(
        primary_zone: &str,
        zone_list_response: wiremock::ResponseTemplate,
        insert_responses: &[(&str, wiremock::ResponseTemplate)],
        expected_list_calls: u64,
    ) -> Result<CloudInstanceRef, CloudError> {
        use wiremock::matchers::{body_partial_json, method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let image = "projects/test-project/global/images/g5-worker-pinned";
        Mock::given(method("GET"))
            .and(path_regex("/regions/us-east1/subnetworks/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "selfLink": "projects/test-project/regions/us-east1/subnetworks/test",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex("/regions/us-east1/routers/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{image}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "g5-worker-pinned",
                "selfLink": format!("{COMPUTE_BASE}/{image}"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones"))
            .respond_with(zone_list_response)
            .expect(expected_list_calls)
            .mount(&server)
            .await;
        for (zone, response) in insert_responses {
            Mock::given(method("POST"))
                .and(path(format!("/projects/test-project/zones/{zone}/instances")))
                .and(body_partial_json(json!({
                    "name": host_instance_name(Uuid::nil(), "test-org"),
                    "machineType": format!("zones/{zone}/machineTypes/c3-standard-4"),
                    "disks": [{"boot": true, "autoDelete": true, "initializeParams": {
                        "diskName": host_instance_name(Uuid::nil(), "test-org"),
                        "sourceImage": format!("{COMPUTE_BASE}/{image}"),
                        "diskSizeGb": "100", "diskType": format!("zones/{zone}/diskTypes/pd-balanced"),
                    }}],
                })))
                .respond_with(response.clone())
                .expect(1)
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path_regex("/zones/us-east1-[bcd]/operations/insert"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "DONE"})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex("/zones/us-east1-[bcd]/instances/.*"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let provider = GcpCloudProvider {
            project_id: "test-project".to_string(),
            worker_image: image.to_string(),
            worker_service_account: "g5-test-host@test-project.iam.gserviceaccount.com".to_string(),
            zone_suffix: primary_zone.strip_prefix("us-east1-").unwrap().to_string(),
            allow_ssh: false,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            token_source: TokenSource::Metadata,
            token_cache: Mutex::new(Some(CachedToken {
                access_token: "test-token".to_string(),
                expires_at: Instant::now() + Duration::from_secs(3600),
            })),
            compute_base_override: Some(server.uri()),
        };
        let vpc = VpcRef {
            vpc_id: network_name(Uuid::nil()),
            region: "us-east1".to_string(),
            network_self_link: "global/networks/test".to_string(),
            subnetwork_self_link: None,
        };
        let result = provider
            .provision_instance(
                Uuid::nil(),
                "test-org",
                None,
                &vpc,
                "c3-standard-4",
                "us-east1",
                image,
                "#!/bin/sh",
                100,
            )
            .await;
        let requests = server.received_requests().await.unwrap();
        let inserts: Vec<_> = requests
            .iter()
            .filter(|request| request.method == reqwest::Method::POST)
            .map(|request| request.url.path().to_string())
            .collect();
        let expected: Vec<_> = insert_responses
            .iter()
            .map(|(zone, _)| format!("/projects/test-project/zones/{zone}/instances"))
            .collect();
        assert_eq!(
            inserts, expected,
            "must attempt only the expected zones, in order"
        );
        result
    }

    fn mock_east1_up_zones() -> wiremock::ResponseTemplate {
        wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {"name": "us-east1-d", "status": "UP"},
                {"name": "us-east1-c", "status": "UP"},
                {"name": "us-east1-b", "status": "UP"},
            ],
        }))
    }

    fn mock_zone_denied(zone: &str) -> wiremock::ResponseTemplate {
        wiremock::ResponseTemplate::new(403).set_body_json(json!({
            "error": {"code": 403, "message": format!(
                "Permission denied on 'locations/{zone}' (or it may not exist)"
            )},
        }))
    }

    fn assert_zone_auth_denied(error: CloudError, zone: &str) {
        match error {
            CloudError::AuthDenied {
                status,
                scope,
                detail,
            } => {
                assert_eq!(status, 403);
                assert_eq!(
                    scope,
                    format!("{COMPUTE_BASE}/projects/test-project/zones/{zone}/instances")
                );
                assert_eq!(
                    detail,
                    format!("Permission denied on 'locations/{zone}' (or it may not exist)")
                );
            }
            other => panic!("expected the original zone's AuthDenied, got {other:?}"),
        }
    }

    /// Both provider responses for an unknown preferred zone must reach real zones.
    #[tokio::test]
    async fn provision_instance_falls_back_to_real_zones_when_primary_is_invalid() {
        use wiremock::ResponseTemplate;

        for primary_response in [
            ResponseTemplate::new(404).set_body_json(json!({
                "error": {"code": 404, "message": "The resource 'zones/us-east1-a' was not found"},
            })),
            mock_zone_denied("us-east1-a"),
        ] {
            let instance = provision_with_mock_zone_responses(
                "us-east1-a",
                mock_east1_up_zones(),
                &[
                    ("us-east1-a", primary_response),
                    ("us-east1-b", ResponseTemplate::new(200).set_body_json(json!({
                        "status": "RUNNING",
                        "selfLink": format!("{COMPUTE_BASE}/projects/test-project/zones/us-east1-b/operations/insert"),
                    }))),
                ],
                1,
            ).await.expect("provisioning falls back to the region's real zones");
            assert_eq!(
                instance.cloud_instance_id,
                encode_instance_id("us-east1-b", &host_instance_name(Uuid::nil(), "test-org"))
            );
            assert_eq!(instance.zone.as_deref(), Some("us-east1-b"));
            assert_eq!(instance.boot_image_id.as_deref(), Some("g5-worker-pinned"));
        }
    }

    #[tokio::test]
    async fn provision_instance_preserves_403_for_real_primary_zone() {
        let error = provision_with_mock_zone_responses(
            "us-east1-b",
            mock_east1_up_zones(),
            &[("us-east1-b", mock_zone_denied("us-east1-b"))],
            1,
        )
        .await
        .expect_err("a real zone's denial must remain an IAM failure");
        assert_zone_auth_denied(error, "us-east1-b");
    }

    #[tokio::test]
    async fn provision_instance_preserves_403_without_available_fallback_zones() {
        use wiremock::ResponseTemplate;

        for zone_list_response in [
            ResponseTemplate::new(403)
                .set_body_json(json!({"error": {"message": "zones.list denied"}})),
            ResponseTemplate::new(404),
            ResponseTemplate::new(200).set_body_json(json!({"items": []})),
            ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {"name": "us-east1-b", "status": "DOWN"},
                    {"name": "us-west1-b", "status": "UP"},
                ],
            })),
        ] {
            let error = provision_with_mock_zone_responses(
                "us-east1-a",
                zone_list_response,
                &[("us-east1-a", mock_zone_denied("us-east1-a"))],
                1,
            )
            .await
            .expect_err("uncertain zone availability must preserve the original denial");
            assert_zone_auth_denied(error, "us-east1-a");
        }
    }

    #[tokio::test]
    async fn provision_instance_preserves_403_from_fallback_zone() {
        let error = provision_with_mock_zone_responses(
            "us-east1-a",
            mock_east1_up_zones(),
            &[
                ("us-east1-a", mock_zone_denied("us-east1-a")),
                ("us-east1-b", mock_zone_denied("us-east1-b")),
            ],
            1,
        )
        .await
        .expect_err("a fallback zone's denial must stop the zone walk");
        assert_zone_auth_denied(error, "us-east1-b");
    }

    #[tokio::test]
    async fn provision_instance_preserves_401_without_zone_discovery() {
        let error = provision_with_mock_zone_responses(
            "us-east1-a",
            mock_east1_up_zones(),
            &[("us-east1-a", wiremock::ResponseTemplate::new(401))],
            0,
        )
        .await
        .expect_err("invalid credentials must not trigger zone discovery");
        assert!(matches!(error, CloudError::AuthDenied { status: 401, .. }));
    }

    /// Keep host cleanup discovery across valid zones when the preferred zone is invalid.
    #[tokio::test]
    async fn list_instances_by_prefix_sweeps_real_zones_despite_invalid_primary() {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Cleanup must inspect valid zones even when the preferred zone is
        // invalid (issue #627 S33): before the fix, the unconditional
        // primary-zone listing 404ed and aborted the whole region's sweep.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {"name": "us-east1-b", "status": "UP", "region": "regions/us-east1"},
                    {"name": "us-east1-c", "status": "UP", "region": "regions/us-east1"},
                ],
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones/us-east1-b/instances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{"name": "g5-host-orphan", "status": "RUNNING"}],
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones/us-east1-c/instances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        // The invalid primary must receive NO listing request at all: an
        // unmatched wiremock request would 404 and fail the sweep.
        Mock::given(method("GET"))
            .and(path("/projects/test-project/zones/us-east1-a/instances"))
            .and(query_param("maxResults", "500"))
            .respond_with(ResponseTemplate::new(404))
            .expect(0)
            .mount(&server)
            .await;
        let provider = GcpCloudProvider {
            project_id: "test-project".to_string(),
            worker_image: "projects/test-project/global/images/family/g5-worker".to_string(),
            worker_service_account: "g5-test-host@test-project.iam.gserviceaccount.com".to_string(),
            zone_suffix: "a".to_string(),
            allow_ssh: false,
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            token_source: TokenSource::Metadata,
            token_cache: Mutex::new(Some(CachedToken {
                access_token: "test-token".to_string(),
                expires_at: Instant::now() + Duration::from_secs(3600),
            })),
            compute_base_override: Some(server.uri()),
        };
        let listed = provider
            .list_instances_by_prefix("us-east1", "g5-host-")
            .await
            .expect("the sweep lists the region's real zones");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "g5-host-orphan");
        assert_eq!(listed[0].zone, "us-east1-b");
    }

    #[test]
    fn boot_disk_type_for_pins_c3_to_pd_balanced_only() {
        // C3 needs an explicit non-pd-standard diskType; pd-balanced is the
        // supported tier across every allowlisted C3 SKU.
        assert_eq!(
            boot_disk_type_for("c3-standard-8", "us-central1-a").as_deref(),
            Some("zones/us-central1-a/diskTypes/pd-balanced")
        );
        assert_eq!(
            boot_disk_type_for("c3-standard-44", "europe-west1-a").as_deref(),
            Some("zones/europe-west1-a/diskTypes/pd-balanced")
        );
        // Older families must return None so GCE keeps the default — no
        // regression from rev-7-9c shape.
        assert_eq!(boot_disk_type_for("n2-standard-8", "us-central1-a"), None);
        assert_eq!(boot_disk_type_for("c2-standard-16", "us-east1-b"), None);
    }

    /// The GCE insert body carries THIS host's size, so two hosts with
    /// different slot counts get different disks (issue #344). `diskSizeGb` is
    /// an `int64`-format field, hence the JSON string.
    #[test]
    fn boot_disk_init_renders_the_per_host_size_for_each_slot_count() {
        let sizing = WorkerHostBootDiskSizing::default();

        // c3-standard-22 at its default budget: 5 slots → 520 GB.
        let five = boot_disk_init(
            "projects/p/global/images/g5-worker-1",
            sizing.default_gb(5),
            "c3-standard-22",
            "us-central1-a",
        );
        assert_eq!(five["diskSizeGb"], json!("520"));
        assert_eq!(
            five["sourceImage"],
            json!("projects/p/global/images/g5-worker-1")
        );
        assert_eq!(
            five["diskType"],
            json!("zones/us-central1-a/diskTypes/pd-balanced")
        );

        // c3-standard-44 at its default budget: 11 slots → 1000 GB. Same code
        // path, different disk — the whole point of the change.
        let eleven = boot_disk_init(
            "projects/p/global/images/g5-worker-1",
            sizing.default_gb(11),
            "c3-standard-44",
            "us-central1-a",
        );
        assert_eq!(eleven["diskSizeGb"], json!("1000"));
        assert_ne!(five["diskSizeGb"], eleven["diskSizeGb"]);

        // Older families keep GCE's default disk type but still get a size.
        let n2 = boot_disk_init("img", sizing.default_gb(2), "n2-standard-8", "us-east1-b");
        assert_eq!(n2["diskSizeGb"], json!("280"));
        assert!(n2.get("diskType").is_none());
    }

    /// Render the configured host account with the required cloud-platform OAuth scope.
    #[test]
    fn worker_host_service_accounts_attach_configured_host_sa() {
        let service_accounts =
            worker_host_service_accounts("g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com");
        assert_eq!(
            service_accounts,
            json!([{
                "email": "g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com",
                "scopes": ["https://www.googleapis.com/auth/cloud-platform"],
            }])
        );
    }

    /// Reject invalid hosted identities while preserving defaults for unrelated projects.
    #[test]
    fn worker_host_identity_rejects_wrong_environment_and_preserves_unrelated_defaults() {
        assert_eq!(
            worker_host_service_account("local-project", None).as_deref(),
            Some("default")
        );
        for (project, environment) in [
            ("g5-qa-1ccc", "qa"),
            ("g5lean-staging-1ccc", "staging"),
            ("g5lean-prod-1ccc", "prod"),
        ] {
            let email = format!("g5-lean-{environment}-host@{project}.iam.gserviceaccount.com");
            assert_eq!(
                worker_host_service_account(project, Some(&email)),
                Some(email)
            );
            for invalid in [
                None,
                Some(""),
                Some("default"),
                Some("g5-lean-qa-host@g5-qa-1ccc.iam.gserviceaccount.com "),
                Some("g5-lean-qa-api@g5-qa-1ccc.iam.gserviceaccount.com"),
                Some("123-compute@developer.gserviceaccount.com"),
            ] {
                assert!(worker_host_service_account(project, invalid).is_none());
            }
        }
        assert!(worker_host_service_account(
            "g5lean-prod-1ccc",
            Some("g5-lean-staging-host@g5lean-staging-1ccc.iam.gserviceaccount.com")
        )
        .is_none());
    }

    #[test]
    fn instance_id_round_trips() {
        let id = encode_instance_id("us-central1-a", "g5-host-abc");
        assert_eq!(id, "us-central1-a/g5-host-abc");
        let (zone, name) = decode_instance_id(&id).unwrap();
        assert_eq!(zone, "us-central1-a");
        assert_eq!(name, "g5-host-abc");
    }

    #[test]
    fn label_value_sanitizes_email_shapes() {
        assert_eq!(
            sanitize_gce_label_value("pgndvd@gmail.com"),
            "pgndvd-gmail-com"
        );
        assert_eq!(
            sanitize_gce_label_value("First.Last+tag@Example.org"),
            "first-lasttag-example-org"
        );
        // Underscores are legal label chars and survive unmapped.
        assert_eq!(sanitize_gce_label_value("a_b@c.d"), "a_b-c-d");
        assert_eq!(sanitize_gce_label_value("@@@"), "");
        let long = format!("{}@example.com", "x".repeat(80));
        assert!(sanitize_gce_label_value(&long).len() <= 63);
    }

    #[test]
    fn decode_rejects_malformed_instance_id() {
        assert!(decode_instance_id("no-slash").is_err());
        assert!(decode_instance_id("/name").is_err());
        assert!(decode_instance_id("zone/").is_err());
    }

    #[test]
    fn api_error_maps_404_to_not_found() {
        let body = json!({ "error": { "message": "nope" } });
        assert!(matches!(
            api_error(404, &body, "u"),
            CloudError::NotFound { .. }
        ));
        assert!(matches!(
            api_error(500, &body, "u"),
            CloudError::Api { status: 500, .. }
        ));
    }

    #[test]
    fn api_error_maps_401_and_403_to_auth_denied() {
        let body = json!({ "error": { "message": "permission denied" } });
        let url = "https://compute.googleapis.com/compute/v1/projects/p/global/networks";
        match api_error(401, &body, url) {
            CloudError::AuthDenied {
                status,
                scope,
                detail,
            } => {
                assert_eq!(status, 401);
                assert_eq!(scope, url);
                assert!(detail.contains("permission denied"));
            }
            other => panic!("expected AuthDenied, got {other:?}"),
        }
        assert!(matches!(
            api_error(403, &body, url),
            CloudError::AuthDenied { status: 403, .. }
        ));
        // 5xx still maps to generic Api.
        assert!(matches!(
            api_error(500, &body, url),
            CloudError::Api { status: 500, .. }
        ));
    }

    #[test]
    fn project_id_validator_accepts_canonical_values() {
        assert!(is_valid_gcp_project_id("g5-prod-01"));
        assert!(is_valid_gcp_project_id("abcdef"));
        assert!(is_valid_gcp_project_id("a23456"));
    }

    #[test]
    fn project_id_validator_rejects_malformed_values() {
        // Too short / too long.
        assert!(!is_valid_gcp_project_id("abc"));
        assert!(!is_valid_gcp_project_id(&"a".repeat(31)));
        // Leading digit / hyphen / uppercase / underscore.
        assert!(!is_valid_gcp_project_id("1project"));
        assert!(!is_valid_gcp_project_id("-project"));
        assert!(!is_valid_gcp_project_id("Project"));
        assert!(!is_valid_gcp_project_id("my_proj"));
        // Path-injection shape.
        assert!(!is_valid_gcp_project_id("my-proj/global"));
        assert!(!is_valid_gcp_project_id("../etc/passwd"));
    }

    #[test]
    fn region_subnet_cidr_table_matches_allowlist() {
        use g5_contracts::types::worker_host::WORKER_HOST_REGION_ALLOWLIST;
        // Drift between the route's region allowlist and the per-region CIDR
        // table would surface as InvalidConfig at runtime — make that a
        // compile-time-ish regression instead.
        for region in WORKER_HOST_REGION_ALLOWLIST {
            assert!(
                region_subnet_cidr(region).is_some(),
                "region {region} in allowlist must have a CIDR entry"
            );
        }
        for (region, _) in REGION_SUBNET_CIDR_TABLE {
            assert!(
                WORKER_HOST_REGION_ALLOWLIST.contains(region),
                "CIDR table entry for {region} must be in the allowlist"
            );
        }
    }

    #[test]
    fn region_subnet_cidrs_are_distinct() {
        // Two regions in the same org sharing a CIDR would collide on
        // subnet `ipCidrRange` within the VPC (the original B-2 finding).
        let mut seen = std::collections::HashSet::new();
        for (_, cidr) in REGION_SUBNET_CIDR_TABLE {
            assert!(seen.insert(*cidr), "CIDR {cidr} duplicated in table");
        }
    }

    #[test]
    fn sanitize_token_error_body_caps_and_one_lines() {
        // Newlines collapse, no double spaces, total bounded at 200 chars.
        let body = "line1\nline2  with   spaces\n  line3";
        assert_eq!(
            sanitize_token_error_body(body),
            "line1 line2 with spaces line3"
        );

        let huge = "x".repeat(500);
        let out = sanitize_token_error_body(&huge);
        assert!(out.starts_with(&"x".repeat(200)));
        assert!(out.contains("[truncated]"));
        assert!(out.len() <= 220, "output stays bounded (got {})", out.len());
    }

    #[tokio::test]
    async fn parse_token_response_maps_401_to_auth_denied() {
        // 401 at the token endpoint = SA assertion / refresh token rejected.
        // Must land in AuthDenied so the worker-host route can fail closed
        // on credential outages instead of a generic Auth bucket.
        let http_resp = http::Response::builder()
            .status(401)
            .body("invalid_grant: assertion expired")
            .expect("build http response");
        let resp = reqwest::Response::from(http_resp);
        match parse_token_response(resp).await {
            Err(CloudError::AuthDenied {
                status,
                scope,
                detail,
            }) => {
                assert_eq!(status, 401);
                assert_eq!(scope, "token_endpoint");
                assert!(detail.contains("invalid_grant"));
            }
            other => panic!("expected AuthDenied for 401, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_token_response_maps_403_to_auth_denied() {
        // 403 at the token endpoint = caller is not authorized to mint a
        // token for the requested scope (e.g. IAM policy revoked). Ontology
        // D17 requires HTTP 401/403 from any GCP surface — including token
        // refresh — to land in AuthDenied, not the generic Auth bucket
        // (codex round-3 fix).
        let http_resp = http::Response::builder()
            .status(403)
            .body("permission_denied: caller lacks token-mint role")
            .expect("build http response");
        let resp = reqwest::Response::from(http_resp);
        match parse_token_response(resp).await {
            Err(CloudError::AuthDenied {
                status,
                scope,
                detail,
            }) => {
                assert_eq!(status, 403);
                assert_eq!(scope, "token_endpoint");
                assert!(detail.contains("permission_denied"));
            }
            other => panic!("expected AuthDenied for 403, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_token_response_maps_other_4xx_to_generic_auth() {
        // 400 / 5xx from the token endpoint stay in the generic Auth bucket
        // — AuthDenied is reserved for "credentials rejected" (401/403).
        let http_resp = http::Response::builder()
            .status(400)
            .body("invalid_request: missing assertion")
            .expect("build http response");
        let resp = reqwest::Response::from(http_resp);
        assert!(matches!(
            parse_token_response(resp).await,
            Err(CloudError::Auth { .. })
        ));
    }

    #[tokio::test]
    async fn ensure_subnetwork_rejects_unknown_region() {
        // The route boundary now allowlists region, but the provider still
        // defends in depth: a region with no CIDR entry must surface as
        // InvalidConfig before any GCP call is issued. Driving the helper
        // directly here proves the early-return — we never reach the
        // network round-trip in `self.get_optional`.
        assert!(region_subnet_cidr("mars-northwest1").is_none());
    }

    #[test]
    fn extract_ips_reads_internal_and_external() {
        let inst = json!({
            "networkInterfaces": [{
                "networkIP": "10.128.0.5",
                "accessConfigs": [{ "natIP": "34.1.2.3" }],
            }],
        });
        let (internal, external) = extract_ips(&inst);
        assert_eq!(internal.as_deref(), Some("10.128.0.5"));
        assert_eq!(external.as_deref(), Some("34.1.2.3"));

        let no_ext = json!({ "networkInterfaces": [{ "networkIP": "10.128.0.6" }] });
        let (internal, external) = extract_ips(&no_ext);
        assert_eq!(internal.as_deref(), Some("10.128.0.6"));
        assert_eq!(external, None);
    }

    #[test]
    fn token_source_parses_service_account() {
        let json = r#"{
            "type": "service_account",
            "client_email": "sa@proj.iam.gserviceaccount.com",
            "private_key": "-----BEGIN PRIVATE KEY-----\nMII...\n-----END PRIVATE KEY-----\n",
            "token_uri": "https://oauth2.googleapis.com/token",
            "project_id": "my-proj"
        }"#;
        let (source, project) = token_source_from_json(json).expect("parses SA");
        assert_eq!(project.as_deref(), Some("my-proj"));
        match source {
            TokenSource::ServiceAccount { client_email, .. } => {
                assert_eq!(client_email, "sa@proj.iam.gserviceaccount.com");
            }
            other => panic!("expected ServiceAccount, got {other:?}"),
        }
    }

    #[test]
    fn token_source_parses_authorized_user() {
        let json = r#"{
            "type": "authorized_user",
            "client_id": "cid",
            "client_secret": "csecret",
            "refresh_token": "rtoken"
        }"#;
        let (source, _) = token_source_from_json(json).expect("parses authorized_user");
        assert!(matches!(source, TokenSource::AuthorizedUser { .. }));
    }

    #[test]
    fn token_source_rejects_unknown_type() {
        assert!(token_source_from_json(r#"{"type":"banana"}"#).is_none());
    }

    #[test]
    fn zone_suffix_strips_dashes_and_rejects_garbage() {
        // Common operator-supplied forms all normalize to the bare suffix.
        assert_eq!(sanitize_zone_suffix("a").as_deref(), Some("a"));
        assert_eq!(sanitize_zone_suffix("-a").as_deref(), Some("a"));
        assert_eq!(sanitize_zone_suffix("-a-").as_deref(), Some("a"));
        assert_eq!(sanitize_zone_suffix("--a--").as_deref(), Some("a"));
        assert_eq!(sanitize_zone_suffix("b1").as_deref(), Some("b1"));
        assert_eq!(sanitize_zone_suffix("  c  ").as_deref(), Some("c"));

        // Empty / whitespace / dashes-only → None.
        assert_eq!(sanitize_zone_suffix(""), None);
        assert_eq!(sanitize_zone_suffix("   "), None);
        assert_eq!(sanitize_zone_suffix("-"), None);
        assert_eq!(sanitize_zone_suffix("--"), None);

        // Uppercase / hyphens-in-middle / shell metacharacters → None.
        assert_eq!(sanitize_zone_suffix("A"), None);
        assert_eq!(sanitize_zone_suffix("a-b"), None);
        assert_eq!(sanitize_zone_suffix("a; rm"), None);
        assert_eq!(sanitize_zone_suffix("a/b"), None);
    }

    #[test]
    fn compute_image_ref_accepts_relative_and_canonical_absolute_forms() {
        // The documented default: a bare family path, with and without a
        // leading slash, normalizes onto COMPUTE_BASE ([D57] acceptance 1).
        let family = "projects/g5-prod/global/images/family/g5-worker";
        assert_eq!(
            validate_compute_image_ref(family).unwrap(),
            format!("{COMPUTE_BASE}/{family}")
        );
        assert_eq!(
            validate_compute_image_ref(&format!("/{family}")).unwrap(),
            format!("{COMPUTE_BASE}/{family}")
        );

        // A specific image, not a family.
        let image = "projects/g5-prod/global/images/g5-worker-v20260723-1046-9749e00";
        assert_eq!(
            validate_compute_image_ref(image).unwrap(),
            format!("{COMPUTE_BASE}/{image}")
        );

        // Absolute under the canonical origin is accepted unchanged
        // ([D57] acceptance 2).
        let absolute = format!("{COMPUTE_BASE}/{image}");
        assert_eq!(validate_compute_image_ref(&absolute).unwrap(), absolute);
    }

    /// [D57] acceptance 3 — every rejected shape fails closed with
    /// `InvalidConfig`. These run without a provider/HTTP client at all, which
    /// is the point: validation precedes `get_optional`/`request`, so a
    /// rejected ref can never reach the bearer-token-attaching request path.
    #[test]
    fn compute_image_ref_rejects_foreign_origins_and_off_shape_paths() {
        let image = "projects/g5-prod/global/images/g5-worker-v1";
        for bad in [
            // Hostile origin — the original token-exfiltration vector.
            "https://attacker.example/compute/v1/projects/p/global/images/i".to_string(),
            // Scheme downgrade, even on the real Compute host.
            format!("http://compute.googleapis.com/compute/v1/{image}"),
            // Suffix/prefix lookalike hosts must not pass an origin check.
            format!("https://compute.googleapis.com.attacker.example/compute/v1/{image}"),
            format!("https://notcompute.googleapis.com/compute/v1/{image}"),
            // Credentials in URL.
            format!("https://user:pass@compute.googleapis.com/compute/v1/{image}"),
            // Query / fragment smuggling.
            format!("{COMPUTE_BASE}/{image}?alt=media"),
            format!("{COMPUTE_BASE}/{image}#frag"),
            // Right origin, wrong namespace / outside the base path.
            format!("{COMPUTE_BASE}/projects/g5-prod/global/networks/n"),
            "https://compute.googleapis.com/token".to_string(),
            // Traversal, relative and absolute.
            "projects/../global/images/i".to_string(),
            "projects/g5-prod/global/images/..".to_string(),
            // Off-shape relative paths.
            "".to_string(),
            "g5-worker".to_string(),
            "projects/g5-prod/global/images".to_string(),
            "projects//global/images/i".to_string(),
            // Charset escape in a name segment.
            "projects/g5-prod/global/images/i j".to_string(),
        ] {
            let err = validate_compute_image_ref(&bad)
                .expect_err(&format!("{bad:?} must be rejected, never requested"));
            assert!(
                matches!(err, CloudError::InvalidConfig { .. }),
                "{bad:?} rejected with {err:?}, expected InvalidConfig"
            );
        }
    }
}
