//! Worker-host runtime-label resolution (PR2).
//!
//! Resolves OCI **image-config** labels for a host's digest-pinned
//! `base_image_ref` and filters them to the G5-owned `com.g5labs.runtime.*`
//! namespace. Runtime labels are **informational** metadata only — never a
//! scheduler-placement, security-enforcement, or registry-credential channel
//! (ontology [D43]).
//!
//! Contract: `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md`
//! [D43]. The trait + concrete impl + test ctor mirror the `CloudProvider`
//! precedent ([D17]) so integration tests can point the resolver at a wiremock
//! registry. There is NO g5-api → g5d production dependency: g5d's OCI module
//! may fetch image config for runtime execution metadata such as `config.Env`,
//! but `.config.Labels` control-plane extraction remains API-local. The digest
//! grammar is reused from `g5_contracts::types::worker_host::validate_base_image_ref`
//! (no parser fork).

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde_json::{Map, Value};

use g5_contracts::types::worker_host::validate_base_image_ref;

// ─── Constants (bounds + namespace, ontology [D43]) ──────────────────────────

/// Per-request read timeout for each registry HTTP call.
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
/// Per-request connect timeout for each registry HTTP call.
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Whole-resolve wall-clock guard (covers manifest + token + index + config).
const RESOLVE_WALL_CLOCK: Duration = Duration::from_secs(15);
/// User-agent presented to registries.
const USER_AGENT: &str = "g5-api-runtime-label-resolver/1";

/// Max manifest / index body accepted (streaming accumulation cap).
const MANIFEST_BODY_CAP: usize = 256 * 1024;
/// Max config-blob body accepted (streaming accumulation cap).
const CONFIG_BLOB_CAP: usize = 1024 * 1024;
/// Max persisted runtime-label entries after filtering.
const MAX_PERSISTED_LABELS: usize = 64;
/// Max serialized size (bytes) of the persisted runtime-label object.
const MAX_PERSISTED_LABELS_BYTES: usize = 16 * 1024;

/// The schema label key (extracted to `runtime_schema`, never stored in labels).
const SCHEMA_KEY: &str = "com.g5labs.runtime.schema";
/// The only schema value supported in PR2.
const SUPPORTED_SCHEMA: &str = "v1";

/// Allowlist of stored runtime-label keys (excludes the schema key, which is
/// extracted separately). Everything else — non-G5, unknown `com.g5labs.runtime.*`,
/// and ALL `org.opencontainers.image.*` — is dropped (ontology [D43]).
const STORED_LABEL_ALLOWLIST: &[&str] = &[
    "com.g5labs.runtime.base_image",
    "com.g5labs.runtime.workload.class",
    "com.g5labs.runtime.kernel.features",
    "com.g5labs.runtime.testcontainers.enabled",
    "com.g5labs.runtime.devices",
    "com.g5labs.runtime.isolation",
    "com.g5labs.runtime.resources.cpu_min",
    "com.g5labs.runtime.resources.mem_min_mib",
];

/// Accept header covering the four manifest media types we resolve.
const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, \
     application/vnd.docker.distribution.manifest.v2+json, \
     application/vnd.oci.image.index.v1+json, \
     application/vnd.docker.distribution.manifest.list.v2+json";

// ─── Public types ────────────────────────────────────────────────────────────

/// The resolved, filtered runtime labels for a base image.
///
/// `schema` is the value of `com.g5labs.runtime.schema` (`Some("v1")`), or
/// `None` when the schema key was absent/blank. Images without a supported
/// schema have not opted into G5 runtime metadata, so `labels` is `{}` unless
/// `schema == Some("v1")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRuntimeLabels {
    pub schema: Option<String>,
    pub labels: Value,
}

/// Typed, **secret-free** resolver error. Never carries registry response
/// bodies or tokens (ontology [D43]). The handler maps each variant to a 422
/// (operator-input / contract-incompatibility) or 503 (registry/network/auth)
/// using EXISTING `G5Error` variants — see [`RuntimeLabelError::is_validation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeLabelError {
    // ── Validation (operator-input / contract-incompatibility → 422) ──
    /// The ref is not a fully-qualified, fetchable digest-pinned registry ref.
    UnqualifiedRef,
    /// `com.g5labs.runtime.schema` carried an unsupported value.
    UnsupportedSchema { found: String },
    /// The config blob was undecodable, or an allowlisted key held a non-string
    /// value ("malformed/incompatible label metadata").
    LabelsUnreadable,
    /// The filtered label set exceeded the persisted-size caps.
    LabelsTooLarge,

    // ── Invariant breach (stored ref is corrupt → 500) ──
    /// The stored `base_image_ref` failed `validate_base_image_ref` (digest
    /// grammar / not digest-pinned). PR1 validates the ref at provision, so this
    /// can only mean DB corruption or a PR1/PR2 contract drift — it is NOT an
    /// operator-fixable input error and must surface as a 500, not a 422.
    InvalidStoredRef,

    // ── Registry / network / auth (upstream condition → 503, fail loudly) ──
    /// Registry returned 404 for the manifest or config blob.
    RegistryNotFound,
    /// Registry rejected the anonymous request (401/403 after token, or a
    /// non-Bearer / unknown auth challenge).
    RegistryAuth,
    /// The image index had no `linux/amd64` child.
    NoMatchingPlatform,
    /// Registry returned a 5xx or otherwise unexpected status.
    RegistryUnavailable,
    /// A registry response exceeded its streaming size cap (manifest/config blob).
    ResponseTooLarge,
    /// Transport/DNS error or whole-resolve wall-clock timeout.
    Transport,
}

impl RuntimeLabelError {
    /// `true` when this is an operator-input / contract-incompatibility error
    /// the handler must surface as a 422; `false` for registry/network/auth
    /// conditions the handler surfaces as a 503 (ontology [D43]).
    #[must_use]
    pub fn is_validation(&self) -> bool {
        matches!(
            self,
            RuntimeLabelError::UnqualifiedRef
                | RuntimeLabelError::UnsupportedSchema { .. }
                | RuntimeLabelError::LabelsUnreadable
                | RuntimeLabelError::LabelsTooLarge
        )
    }

    /// `true` when this is an internal invariant breach the handler must surface
    /// as a 500 (a stored ref that PR1 should have rejected at provision).
    #[must_use]
    pub fn is_internal(&self) -> bool {
        matches!(self, RuntimeLabelError::InvalidStoredRef)
    }

    /// Machine-readable code for the 422 validation detail body. Only meaningful
    /// for validation variants; registry variants share a generic 503 message.
    #[must_use]
    pub fn machine_code(&self) -> &'static str {
        match self {
            RuntimeLabelError::UnqualifiedRef => "unqualified_base_image_ref",
            RuntimeLabelError::UnsupportedSchema { .. } => "unsupported_runtime_schema",
            RuntimeLabelError::LabelsUnreadable => "labels_unreadable",
            RuntimeLabelError::LabelsTooLarge => "labels_too_large",
            RuntimeLabelError::InvalidStoredRef => "invalid_stored_base_image_ref",
            // Registry/network variants are not surfaced via the validation code.
            _ => "runtime_label_resolution_failed",
        }
    }

    /// A short, secret-free operator-facing message for the 503 body.
    #[must_use]
    pub fn service_message(&self) -> &'static str {
        match self {
            RuntimeLabelError::RegistryNotFound => {
                "registry returned 404 for the base image manifest or config"
            }
            RuntimeLabelError::RegistryAuth => {
                "registry rejected the anonymous pull (authentication required)"
            }
            RuntimeLabelError::NoMatchingPlatform => "base image has no linux/amd64 manifest",
            RuntimeLabelError::ResponseTooLarge => "registry response exceeded the allowed size",
            RuntimeLabelError::Transport => "could not reach the image registry",
            _ => "image registry is temporarily unavailable",
        }
    }
}

// ─── Trait + concrete OCI resolver ───────────────────────────────────────────

/// Resolves runtime labels for a digest-pinned base image ref.
#[async_trait::async_trait]
pub trait RuntimeLabelResolver: Send + Sync {
    async fn resolve(
        &self,
        base_image_ref: &str,
    ) -> Result<ResolvedRuntimeLabels, RuntimeLabelError>;
}

/// Parts of an accepted ref: the registry host, the repository path, and the
/// `sha256:<64hex>` digest. Produced only after the ref passes both
/// `validate_base_image_ref` (digest grammar) and the fully-qualified-host gate.
struct RefParts {
    host: String,
    repository: String,
    digest: String,
}

/// Concrete anonymous OCI Distribution v2 resolver.
///
/// `scheme` is `"https"` in production and `"http"` for tests (so a wiremock /
/// `127.0.0.1:PORT` registry is reachable). The client carries bounded
/// timeouts and a user-agent ([D43]).
pub struct OciRuntimeLabelResolver {
    http: reqwest::Client,
    scheme: &'static str,
    allow_insecure_loopback_hosts: bool,
}

impl OciRuntimeLabelResolver {
    /// Production resolver (`https`).
    #[must_use]
    pub fn https() -> Self {
        Self::with_scheme(Self::build_client(), "https", false)
    }

    /// Test resolver over `http` with a caller-supplied client (the integration
    /// tests build one pointed at wiremock / a loopback registry). This also
    /// permits loopback registry hosts and `http://127.0.0.1:PORT` bearer realms
    /// for wiremock only.
    #[must_use]
    pub fn for_tests(client: reqwest::Client) -> Self {
        Self::with_scheme(client, "http", true)
    }

    fn with_scheme(
        http: reqwest::Client,
        scheme: &'static str,
        allow_insecure_loopback_hosts: bool,
    ) -> Self {
        Self {
            http,
            scheme,
            allow_insecure_loopback_hosts,
        }
    }

    /// Build the bounded reqwest client used in production. The options are
    /// static and valid, so the build never fails in practice; if it ever did we
    /// fail closed (panic at construction) rather than silently fall back to an
    /// UNBOUNDED default client, which would break the mandatory bounded-fetch
    /// safety invariant ([D43]).
    fn build_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .dns_resolver(Arc::new(FilteringDnsResolver))
            .user_agent(USER_AGENT)
            .build()
            .expect("bounded OCI resolver HTTP client builds from static config")
    }

    /// Split + validate the ref into fetchable parts (ontology [D43] acceptance).
    ///
    /// Qualification is checked FIRST: structural shape (`host/repo@digest`), a
    /// real registry host (contains `.` or `:`, or is `localhost`), and the
    /// `docker.io` shorthand refusal — all of which are operator-fixable inputs
    /// (a ref PR1 may have stored) and map to a 422 `UnqualifiedRef`. ONLY once
    /// the ref is fully qualified and digest-pinned do we confirm the digest
    /// grammar via `validate_base_image_ref`; a failure there is an invariant
    /// breach (PR1 validated the ref at provision) and maps to a 500
    /// `InvalidStoredRef`, not a 422. Docker-shorthand / `docker.io/library/...`
    /// normalization candidates are refused rather than rewritten.
    fn parse_ref(base_image_ref: &str) -> Result<RefParts, RuntimeLabelError> {
        let trimmed = base_image_ref.trim();
        // Reject scheme-prefixed refs (`https://host/...`): the resolver supplies
        // the scheme itself and fetches the host verbatim, so a scheme in the ref
        // is malformed operator input ⇒ 422 (not a downstream transport 503).
        if trimmed.contains("://") {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }
        // Name is everything before the LAST '@'. No `@digest` ⇒ tag-only / not
        // digest-pinned ⇒ operator-fixable 422.
        let (name, _) = trimmed
            .rsplit_once('@')
            .ok_or(RuntimeLabelError::UnqualifiedRef)?;
        // A second '@' before the digest (e.g. `user:pass@host/repo@sha256:..`)
        // would smuggle userinfo/credentials into the verbatim registry URL. The
        // resolver is anonymous-only and fetches the host verbatim ([D43]), so
        // refuse any credential-bearing ref outright ⇒ 422.
        if name.contains('@') {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }
        // `host/repository` — a bare name (no '/') is a shorthand ⇒ 422.
        let (host, repository) = name
            .split_once('/')
            .ok_or(RuntimeLabelError::UnqualifiedRef)?;
        if repository.is_empty() || repository.starts_with('/') {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }
        // `docker.io` is the Docker Hub *shorthand*, not a fetchable Distribution
        // v2 endpoint (the real host is `registry-1.docker.io`). We accept only
        // refs the resolver can fetch verbatim, so refuse the shorthand rather
        // than silently rewriting it — that normalization is exactly the g5d
        // parser fork this resolver avoids.
        if host.eq_ignore_ascii_case("docker.io") {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }
        let host_is_registry = host == "localhost" || host.contains('.') || host.contains(':');
        if !host_is_registry {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }
        // The host is qualified and the ref is digest-pinned; confirm the digest
        // grammar last. A failure here on an otherwise-qualified stored ref is an
        // invariant breach (PR1 validated it at provision) ⇒ 500, not 422.
        let digest = validate_base_image_ref(base_image_ref)
            .map_err(|_| RuntimeLabelError::InvalidStoredRef)?;
        Ok(RefParts {
            host: host.to_string(),
            repository: repository.to_string(),
            digest,
        })
    }

    /// Run the full anonymous Distribution v2 flow for an accepted ref.
    async fn resolve_inner(
        &self,
        parts: &RefParts,
    ) -> Result<ResolvedRuntimeLabels, RuntimeLabelError> {
        self.validate_registry_host(&parts.host).await?;

        // (1) Fetch the manifest at the pinned digest.
        let manifest_bytes = self
            .fetch_manifest(&parts.host, &parts.repository, &parts.digest)
            .await?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| RuntimeLabelError::RegistryUnavailable)?;

        // (3) Resolve an image index / manifest list to its linux/amd64 child.
        let image_manifest = if is_index(&manifest) {
            let child_digest =
                select_amd64_child(&manifest).ok_or(RuntimeLabelError::NoMatchingPlatform)?;
            let child_bytes = self
                .fetch_manifest(&parts.host, &parts.repository, &child_digest)
                .await?;
            serde_json::from_slice::<Value>(&child_bytes)
                .map_err(|_| RuntimeLabelError::RegistryUnavailable)?
        } else {
            manifest
        };

        // (4) Take the CONFIG descriptor digest (not a layer) and fetch the blob.
        let config_digest = image_manifest
            .get("config")
            .and_then(|c| c.get("digest"))
            .and_then(Value::as_str)
            // A manifest with no config descriptor is not an image we can read
            // labels from; treat as an unexpected/unavailable registry response.
            .ok_or(RuntimeLabelError::RegistryUnavailable)?
            .to_string();
        let config_bytes = self
            .fetch_blob(&parts.host, &parts.repository, &config_digest)
            .await?;

        // (5) Parse config JSON and read `.config.Labels`. A bad config blob is
        // operator-facing "incompatible label metadata" (422), not a registry
        // availability problem.
        let config: Value = serde_json::from_slice(&config_bytes)
            .map_err(|_| RuntimeLabelError::LabelsUnreadable)?;
        let labels = extract_config_labels(&config)?;

        // (6) Filter to the allowlist + extract/validate the schema.
        filter_and_extract(&labels)
    }

    /// GET a manifest (with the anonymous-bearer retry) and return its bytes,
    /// streamed and capped at [`MANIFEST_BODY_CAP`].
    async fn fetch_manifest(
        &self,
        host: &str,
        repository: &str,
        reference: &str,
    ) -> Result<Vec<u8>, RuntimeLabelError> {
        let url = format!(
            "{}://{host}/v2/{repository}/manifests/{reference}",
            self.scheme
        );
        let resp = self
            .get_with_anonymous_bearer(&url, Some(MANIFEST_ACCEPT))
            .await?;
        read_body_capped(resp, MANIFEST_BODY_CAP).await
    }

    /// GET a blob (with the anonymous-bearer retry) and return its bytes,
    /// streamed and capped at [`CONFIG_BLOB_CAP`].
    async fn fetch_blob(
        &self,
        host: &str,
        repository: &str,
        digest: &str,
    ) -> Result<Vec<u8>, RuntimeLabelError> {
        let url = format!("{}://{host}/v2/{repository}/blobs/{digest}", self.scheme);
        let resp = self.get_with_anonymous_bearer(&url, None).await?;
        read_body_capped(resp, CONFIG_BLOB_CAP).await
    }

    /// GET `url`, performing the anonymous bearer-token challenge exchange on a
    /// 401 with a `WWW-Authenticate: Bearer realm=…` header. NO credentials are
    /// ever sent — only the anonymous `service`+`scope` token exchange.
    async fn get_with_anonymous_bearer(
        &self,
        url: &str,
        accept: Option<&str>,
    ) -> Result<reqwest::Response, RuntimeLabelError> {
        let first = self.send_get(url, accept, None).await?;
        if first.status() != reqwest::StatusCode::UNAUTHORIZED {
            return status_or_error(first);
        }

        // Anonymous bearer challenge ONLY.
        let challenge = first
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let challenge = challenge.ok_or(RuntimeLabelError::RegistryAuth)?;
        let parsed = parse_bearer_challenge(&challenge).ok_or(RuntimeLabelError::RegistryAuth)?;

        let token = self.fetch_anonymous_token(&parsed).await?;
        let retried = self.send_get(url, accept, Some(&token)).await?;
        status_or_error(retried)
    }

    /// Send one GET with optional Accept and bearer token.
    async fn send_get(
        &self,
        url: &str,
        accept: Option<&str>,
        bearer: Option<&str>,
    ) -> Result<reqwest::Response, RuntimeLabelError> {
        let mut req = self.http.get(url);
        if let Some(accept) = accept {
            req = req.header(reqwest::header::ACCEPT, accept);
        }
        if let Some(bearer) = bearer {
            req = req.header(reqwest::header::AUTHORIZATION, format!("Bearer {bearer}"));
        }
        req.send().await.map_err(|_| RuntimeLabelError::Transport)
    }

    /// Exchange the anonymous bearer challenge for a token (no credentials).
    async fn fetch_anonymous_token(
        &self,
        challenge: &BearerChallenge,
    ) -> Result<String, RuntimeLabelError> {
        let realm = self.validate_bearer_realm(&challenge.realm).await?;
        let mut req = self.http.get(realm);
        let mut query: Vec<(&str, &str)> = Vec::new();
        if let Some(service) = &challenge.service {
            query.push(("service", service));
        }
        if let Some(scope) = &challenge.scope {
            query.push(("scope", scope));
        }
        if !query.is_empty() {
            req = req.query(&query);
        }
        let resp = req.send().await.map_err(|_| RuntimeLabelError::Transport)?;
        if !resp.status().is_success() {
            return Err(RuntimeLabelError::RegistryAuth);
        }
        let body = read_body_capped(resp, MANIFEST_BODY_CAP).await?;
        let json: Value =
            serde_json::from_slice(&body).map_err(|_| RuntimeLabelError::RegistryAuth)?;
        // Registries return the anonymous token as either `token` or `access_token`.
        json.get("token")
            .or_else(|| json.get("access_token"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(RuntimeLabelError::RegistryAuth)
    }

    /// Validate the anonymous-token realm before sending a request. Production
    /// realms must be HTTPS, credential-free, and must not target localhost,
    /// private, link-local, multicast, documentation, benchmark, or internal
    /// addresses. The test resolver allows only loopback HTTP realms so
    /// wiremock can exercise the challenge flow without live network access.
    async fn validate_bearer_realm(&self, realm: &str) -> Result<reqwest::Url, RuntimeLabelError> {
        let url = reqwest::Url::parse(realm).map_err(|_| RuntimeLabelError::RegistryAuth)?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(RuntimeLabelError::RegistryAuth);
        }
        if !matches!(url.scheme(), "https" | "http") {
            return Err(RuntimeLabelError::RegistryAuth);
        }
        if url.scheme() == "http" && !self.allow_insecure_loopback_hosts {
            return Err(RuntimeLabelError::RegistryAuth);
        }

        let host = url.host_str().ok_or(RuntimeLabelError::RegistryAuth)?;
        let direct_ip = parse_realm_ip(host);
        if self.allow_insecure_loopback_hosts && is_loopback_realm_host(host, direct_ip) {
            return Ok(url);
        }

        if is_internal_realm_hostname(host) {
            return Err(RuntimeLabelError::RegistryAuth);
        }

        if let Some(ip) = direct_ip {
            return if is_forbidden_realm_ip(ip) {
                Err(RuntimeLabelError::RegistryAuth)
            } else {
                Ok(url)
            };
        }

        if url.scheme() != "https" {
            return Err(RuntimeLabelError::RegistryAuth);
        }

        let port = url
            .port_or_known_default()
            .ok_or(RuntimeLabelError::RegistryAuth)?;
        let resolved = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| RuntimeLabelError::Transport)?;
        let mut saw_addr = false;
        for addr in resolved {
            saw_addr = true;
            if is_forbidden_realm_ip(addr.ip()) {
                return Err(RuntimeLabelError::RegistryAuth);
            }
        }
        if !saw_addr {
            return Err(RuntimeLabelError::Transport);
        }

        Ok(url)
    }

    /// Validate the registry host before the first Distribution v2 request.
    /// Production rejects localhost/private/internal targets whether supplied as
    /// a direct IP or reached by DNS. The test resolver allows loopback so the
    /// wiremock integration suite can exercise the real HTTP flow.
    async fn validate_registry_host(&self, host: &str) -> Result<(), RuntimeLabelError> {
        let url = reqwest::Url::parse(&format!("{}://{host}/", self.scheme))
            .map_err(|_| RuntimeLabelError::UnqualifiedRef)?;
        let host = url.host_str().ok_or(RuntimeLabelError::UnqualifiedRef)?;
        let direct_ip = parse_realm_ip(host);
        if self.allow_insecure_loopback_hosts && is_loopback_realm_host(host, direct_ip) {
            return Ok(());
        }

        if is_internal_realm_hostname(host) {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }

        if let Some(ip) = direct_ip {
            return if is_forbidden_realm_ip(ip) {
                Err(RuntimeLabelError::UnqualifiedRef)
            } else {
                Ok(())
            };
        }

        if self.scheme != "https" {
            return Err(RuntimeLabelError::UnqualifiedRef);
        }

        let port = url
            .port_or_known_default()
            .ok_or(RuntimeLabelError::UnqualifiedRef)?;
        let resolved = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| RuntimeLabelError::Transport)?;
        let mut saw_addr = false;
        for addr in resolved {
            saw_addr = true;
            if is_forbidden_realm_ip(addr.ip()) {
                return Err(RuntimeLabelError::UnqualifiedRef);
            }
        }
        if !saw_addr {
            return Err(RuntimeLabelError::Transport);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl RuntimeLabelResolver for OciRuntimeLabelResolver {
    async fn resolve(
        &self,
        base_image_ref: &str,
    ) -> Result<ResolvedRuntimeLabels, RuntimeLabelError> {
        let parts = Self::parse_ref(base_image_ref)?;
        // Whole-resolve wall-clock guard: a malicious/slow registry must not be
        // able to hang the request beyond this budget (ontology [D43]).
        match tokio::time::timeout(RESOLVE_WALL_CLOCK, self.resolve_inner(&parts)).await {
            Ok(result) => result,
            Err(_) => Err(RuntimeLabelError::Transport),
        }
    }
}

// ─── Free helpers (pure; unit-tested) ────────────────────────────────────────

/// Parsed anonymous bearer-token challenge (`WWW-Authenticate: Bearer …`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct BearerChallenge {
    realm: String,
    service: Option<String>,
    scope: Option<String>,
}

/// Parse a `WWW-Authenticate: Bearer realm="…",service="…"[,scope="…"]` header.
/// Returns `None` when the scheme is not `Bearer` or `realm` is missing.
fn parse_bearer_challenge(header: &str) -> Option<BearerChallenge> {
    let rest = header.trim();
    let rest = rest
        .strip_prefix("Bearer ")
        .or_else(|| rest.strip_prefix("bearer "))?;

    let mut realm = None;
    let mut service = None;
    let mut scope = None;
    for part in split_challenge_params(rest)? {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "realm" => realm = Some(value),
            "service" => service = Some(value),
            "scope" => scope = Some(value),
            _ => {}
        }
    }
    Some(BearerChallenge {
        realm: realm?,
        service,
        scope,
    })
}

/// Split challenge params on commas that are not inside a quoted value.
fn split_challenge_params(input: &str) -> Option<Vec<String>> {
    let mut params = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                params.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if in_quotes {
        return None;
    }
    if !current.trim().is_empty() {
        params.push(current);
    }
    Some(params)
}

#[derive(Debug)]
struct FilteringDnsResolver;

impl Resolve for FilteringDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            if is_internal_realm_hostname(&host) {
                return Err(unsafe_dns_target_error());
            }

            if let Some(ip) = parse_realm_ip(&host) {
                if is_forbidden_realm_ip(ip) {
                    return Err(unsafe_dns_target_error());
                }
                let addrs: Addrs = Box::new(std::iter::once(SocketAddr::new(ip, 0)));
                return Ok(addrs);
            }

            let resolved = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|err| Box::new(err) as Box<dyn std::error::Error + Send + Sync>)?;
            let mut safe = Vec::new();
            for addr in resolved {
                let ip = normalize_realm_ip(addr.ip());
                if is_forbidden_realm_ip(ip) {
                    return Err(unsafe_dns_target_error());
                }
                safe.push(SocketAddr::new(ip, addr.port()));
            }
            if safe.is_empty() {
                return Err(unsafe_dns_target_error());
            }

            let addrs: Addrs = Box::new(safe.into_iter());
            Ok(addrs)
        })
    }
}

fn unsafe_dns_target_error() -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "unsafe registry DNS target",
    ))
}

fn is_loopback_realm_host(host: &str, direct_ip: Option<IpAddr>) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || direct_ip.is_some_and(|ip| normalize_realm_ip(ip).is_loopback())
}

fn parse_realm_ip(host: &str) -> Option<IpAddr> {
    host.parse::<IpAddr>()
        .ok()
        .or_else(|| {
            host.strip_prefix('[')
                .and_then(|stripped| stripped.strip_suffix(']'))
                .and_then(|stripped| stripped.parse::<IpAddr>().ok())
        })
        .map(normalize_realm_ip)
}

fn normalize_realm_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(ip) => IpAddr::V4(ip),
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
    }
}

fn is_internal_realm_hostname(host: &str) -> bool {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
}

fn is_forbidden_realm_ip(ip: IpAddr) -> bool {
    match normalize_realm_ip(ip) {
        IpAddr::V4(ip) => is_forbidden_realm_ipv4(ip),
        IpAddr::V6(ip) => is_forbidden_realm_ipv6(ip),
    }
}

fn is_forbidden_realm_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || octets[0] == 0
        || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 0b0100_0000)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || octets[0] >= 240
}

fn is_forbidden_realm_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
}

/// `true` when the manifest JSON is an OCI image index / docker manifest list.
fn is_index(manifest: &Value) -> bool {
    // Indexes carry a `manifests` array of child descriptors; image manifests
    // carry a `config` descriptor instead.
    if let Some(media) = manifest.get("mediaType").and_then(Value::as_str) {
        if media.contains("image.index") || media.contains("manifest.list") {
            return true;
        }
    }
    manifest.get("manifests").is_some() && manifest.get("config").is_none()
}

/// Select the `linux/amd64` child digest from an image index / manifest list.
fn select_amd64_child(index: &Value) -> Option<String> {
    let manifests = index.get("manifests")?.as_array()?;
    for entry in manifests {
        let platform = entry.get("platform");
        let os = platform
            .and_then(|p| p.get("os"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let arch = platform
            .and_then(|p| p.get("architecture"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if os == "linux" && arch == "amd64" {
            if let Some(digest) = entry.get("digest").and_then(Value::as_str) {
                return Some(digest.to_string());
            }
        }
    }
    None
}

/// Read `.config.Labels` from a parsed image config blob.
///
/// A missing `.config` or `.config.Labels` is the success path (no labels). A
/// present-but-non-object `.config.Labels` is malformed metadata (422).
fn extract_config_labels(config: &Value) -> Result<Map<String, Value>, RuntimeLabelError> {
    let Some(config_obj) = config.get("config") else {
        return Ok(Map::new());
    };
    let Some(labels) = config_obj.get("Labels") else {
        return Ok(Map::new());
    };
    if labels.is_null() {
        return Ok(Map::new());
    }
    labels
        .as_object()
        .cloned()
        .ok_or(RuntimeLabelError::LabelsUnreadable)
}

/// Filter raw image labels to the G5 allowlist, extract/validate the schema, and
/// enforce the persisted-size caps (ontology [D43]).
fn filter_and_extract(
    labels: &Map<String, Value>,
) -> Result<ResolvedRuntimeLabels, RuntimeLabelError> {
    // Schema: absent/blank → None; "v1" → accept; anything else → 422.
    let schema = match labels.get(SCHEMA_KEY) {
        None => None,
        Some(value) => {
            let raw = value.as_str().ok_or(RuntimeLabelError::LabelsUnreadable)?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed == SUPPORTED_SCHEMA {
                Some(SUPPORTED_SCHEMA.to_string())
            } else {
                return Err(RuntimeLabelError::UnsupportedSchema {
                    found: trimmed.to_string(),
                });
            }
        }
    };

    // Images without a schema are treated as not opting into G5 runtime
    // metadata. Succeed with NULL schema and an empty labels object, even if
    // allowlisted-looking keys are present.
    if schema.is_none() {
        return Ok(ResolvedRuntimeLabels {
            schema,
            labels: Value::Object(Map::new()),
        });
    }

    // Stored labels: with schema=v1, keep only allowlisted keys with string
    // values (verbatim).
    let mut stored = Map::new();
    for key in STORED_LABEL_ALLOWLIST {
        if let Some(value) = labels.get(*key) {
            let string_value = value.as_str().ok_or(RuntimeLabelError::LabelsUnreadable)?;
            stored.insert((*key).to_string(), Value::String(string_value.to_string()));
        }
    }

    // Post-filter cap (entries + serialized bytes).
    if stored.len() > MAX_PERSISTED_LABELS {
        return Err(RuntimeLabelError::LabelsTooLarge);
    }
    let labels_value = Value::Object(stored);
    let serialized =
        serde_json::to_vec(&labels_value).map_err(|_| RuntimeLabelError::LabelsUnreadable)?;
    if serialized.len() > MAX_PERSISTED_LABELS_BYTES {
        return Err(RuntimeLabelError::LabelsTooLarge);
    }

    Ok(ResolvedRuntimeLabels {
        schema,
        labels: labels_value,
    })
}

/// Map a non-401 registry response to either its body-bearing `Ok` or a typed
/// registry error. 2xx passes through; 404 / 401 / 403 / 5xx / other map to the
/// matching `RuntimeLabelError` (ontology [D43] error mapping).
fn status_or_error(resp: reqwest::Response) -> Result<reqwest::Response, RuntimeLabelError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    Err(match status {
        reqwest::StatusCode::NOT_FOUND => RuntimeLabelError::RegistryNotFound,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            RuntimeLabelError::RegistryAuth
        }
        s if s.is_server_error() => RuntimeLabelError::RegistryUnavailable,
        _ => RuntimeLabelError::RegistryUnavailable,
    })
}

/// Read a response body via streaming accumulation, aborting once the running
/// total exceeds `cap`. Content-Length is NOT trusted (ontology [D43]).
async fn read_body_capped(
    resp: reqwest::Response,
    cap: usize,
) -> Result<Vec<u8>, RuntimeLabelError> {
    let mut acc = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| RuntimeLabelError::Transport)?;
        if acc.len() + chunk.len() > cap {
            return Err(RuntimeLabelError::ResponseTooLarge);
        }
        acc.extend_from_slice(&chunk);
    }
    Ok(acc)
}

// ─── Unit tests (exempt from grounding) ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map_from(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn filter_keeps_only_allowlisted_keys() {
        let labels = map_from(json!({
            "com.g5labs.runtime.schema": "v1",
            "com.g5labs.runtime.base_image": "ubuntu:24.04",
            "com.g5labs.runtime.workload.class": "build",
            "com.g5labs.runtime.unknown_key": "drop-me",
            "org.opencontainers.image.title": "drop-me-too",
            "random.vendor.key": "nope",
        }));
        let resolved = filter_and_extract(&labels).expect("valid");
        assert_eq!(resolved.schema.as_deref(), Some("v1"));
        let stored = resolved.labels.as_object().unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(stored["com.g5labs.runtime.base_image"], "ubuntu:24.04");
        assert_eq!(stored["com.g5labs.runtime.workload.class"], "build");
        // The schema key is extracted, never stored in labels.
        assert!(!stored.contains_key("com.g5labs.runtime.schema"));
        // Non-G5, unknown G5, and OCI-reserved keys are dropped.
        assert!(!stored.contains_key("com.g5labs.runtime.unknown_key"));
        assert!(!stored.contains_key("org.opencontainers.image.title"));
        assert!(!stored.contains_key("random.vendor.key"));
    }

    #[test]
    fn schema_absent_yields_none_and_empty_object() {
        let labels = map_from(json!({ "random.vendor.key": "x" }));
        let resolved = filter_and_extract(&labels).expect("valid");
        assert_eq!(resolved.schema, None);
        assert_eq!(resolved.labels, json!({}));
    }

    #[test]
    fn schema_absent_drops_allowlisted_labels() {
        // Without `com.g5labs.runtime.schema`, the image has not opted into G5
        // runtime metadata. Persist NULL schema and an empty labels object even
        // when allowlisted-looking keys are present.
        let labels = map_from(json!({
            "com.g5labs.runtime.workload.class": "build",
            "com.g5labs.runtime.isolation": "vm",
            "org.opencontainers.image.title": "drop-me",
        }));
        let resolved = filter_and_extract(&labels).expect("valid");
        assert_eq!(resolved.schema, None);
        assert_eq!(resolved.labels, json!({}));
    }

    #[test]
    fn schema_v1_accepted() {
        let labels = map_from(json!({ "com.g5labs.runtime.schema": "v1" }));
        let resolved = filter_and_extract(&labels).expect("valid");
        assert_eq!(resolved.schema.as_deref(), Some("v1"));
    }

    #[test]
    fn schema_unknown_rejected() {
        let labels = map_from(json!({ "com.g5labs.runtime.schema": "v2" }));
        let err = filter_and_extract(&labels).expect_err("unknown schema rejected");
        assert!(matches!(err, RuntimeLabelError::UnsupportedSchema { .. }));
        assert!(err.is_validation());
    }

    #[test]
    fn schema_empty_or_whitespace_treated_absent() {
        let blank = map_from(json!({ "com.g5labs.runtime.schema": "   " }));
        assert_eq!(filter_and_extract(&blank).unwrap().schema, None);
        let empty = map_from(json!({ "com.g5labs.runtime.schema": "" }));
        assert_eq!(filter_and_extract(&empty).unwrap().schema, None);
    }

    #[test]
    fn non_string_allowlisted_value_rejected() {
        let labels = map_from(json!({
            "com.g5labs.runtime.schema": "v1",
            "com.g5labs.runtime.base_image": 123,
        }));
        let err = filter_and_extract(&labels).expect_err("non-string value rejected");
        assert_eq!(err, RuntimeLabelError::LabelsUnreadable);
        assert!(err.is_validation());
    }

    #[test]
    fn non_string_schema_value_rejected() {
        let labels = map_from(json!({ "com.g5labs.runtime.schema": ["v1"] }));
        let err = filter_and_extract(&labels).expect_err("non-string schema rejected");
        assert_eq!(err, RuntimeLabelError::LabelsUnreadable);
    }

    #[test]
    fn oversized_label_set_rejected() {
        // Build a single allowlisted label with a value over the 16 KiB cap.
        let big = "x".repeat(MAX_PERSISTED_LABELS_BYTES + 100);
        let labels = map_from(json!({
            "com.g5labs.runtime.schema": "v1",
            "com.g5labs.runtime.base_image": big
        }));
        let err = filter_and_extract(&labels).expect_err("oversized rejected");
        assert_eq!(err, RuntimeLabelError::LabelsTooLarge);
        assert!(err.is_validation());
    }

    #[test]
    fn bearer_challenge_parse_realm_service_scope() {
        let header = r#"Bearer realm="https://auth.example.com/token",service="registry.example.com",scope="repository:acme/app:pull""#;
        let parsed = parse_bearer_challenge(header).expect("parses");
        assert_eq!(parsed.realm, "https://auth.example.com/token");
        assert_eq!(parsed.service.as_deref(), Some("registry.example.com"));
        assert_eq!(parsed.scope.as_deref(), Some("repository:acme/app:pull"));
    }

    #[test]
    fn bearer_challenge_realm_only() {
        let parsed =
            parse_bearer_challenge(r#"Bearer realm="https://auth/token""#).expect("parses");
        assert_eq!(parsed.realm, "https://auth/token");
        assert_eq!(parsed.service, None);
        assert_eq!(parsed.scope, None);
    }

    #[test]
    fn non_bearer_challenge_rejected() {
        assert!(parse_bearer_challenge(r#"Basic realm="x""#).is_none());
        // Bearer scheme but missing realm → None.
        assert!(parse_bearer_challenge(r#"Bearer service="x""#).is_none());
    }

    #[test]
    fn malformed_bearer_challenge_unclosed_quote_rejected() {
        assert!(
            parse_bearer_challenge(
                r#"Bearer realm="https://auth.example.com/token,service="registry.example.com""#
            )
            .is_none(),
            "unclosed quoted realm must fail closed"
        );
    }

    #[tokio::test]
    async fn bearer_realm_validation_rejects_insecure_or_local_production_realms() {
        let resolver = OciRuntimeLabelResolver::https();
        assert_eq!(
            resolver
                .validate_bearer_realm("http://auth.example.com/token")
                .await
                .err(),
            Some(RuntimeLabelError::RegistryAuth)
        );
        assert_eq!(
            resolver
                .validate_bearer_realm("https://127.0.0.1/token")
                .await
                .err(),
            Some(RuntimeLabelError::RegistryAuth)
        );
        assert_eq!(
            resolver
                .validate_bearer_realm("https://[::ffff:127.0.0.1]/token")
                .await
                .err(),
            Some(RuntimeLabelError::RegistryAuth)
        );
        assert_eq!(
            resolver
                .validate_bearer_realm("https://localhost/token")
                .await
                .err(),
            Some(RuntimeLabelError::RegistryAuth)
        );
        assert_eq!(
            resolver
                .validate_bearer_realm("https://user:pass@auth.example.com/token")
                .await
                .err(),
            Some(RuntimeLabelError::RegistryAuth)
        );
    }

    #[tokio::test]
    async fn bearer_realm_validation_allows_loopback_for_tests_only() {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("test client builds");
        let resolver = OciRuntimeLabelResolver::for_tests(client);
        assert!(resolver
            .validate_bearer_realm("http://127.0.0.1:5000/token")
            .await
            .is_ok());
        assert!(resolver
            .validate_bearer_realm("http://localhost:5000/token")
            .await
            .is_ok());
        assert_eq!(
            resolver
                .validate_bearer_realm("http://10.0.0.1/token")
                .await
                .err(),
            Some(RuntimeLabelError::RegistryAuth)
        );
    }

    #[tokio::test]
    async fn registry_host_validation_rejects_local_or_private_production_hosts() {
        let resolver = OciRuntimeLabelResolver::https();
        assert_eq!(
            resolver
                .validate_registry_host("127.0.0.1:5000")
                .await
                .err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        assert_eq!(
            resolver
                .validate_registry_host("localhost:5000")
                .await
                .err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        assert_eq!(
            resolver.validate_registry_host("10.0.0.1:5000").await.err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        assert_eq!(
            resolver.validate_registry_host("[::1]:5000").await.err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        assert_eq!(
            resolver
                .validate_registry_host("[::ffff:127.0.0.1]:5000")
                .await
                .err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
    }

    #[tokio::test]
    async fn registry_host_validation_allows_loopback_for_tests_only() {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("test client builds");
        let resolver = OciRuntimeLabelResolver::for_tests(client);
        assert!(resolver
            .validate_registry_host("127.0.0.1:5000")
            .await
            .is_ok());
        assert!(resolver
            .validate_registry_host("localhost:5000")
            .await
            .is_ok());
        assert_eq!(
            resolver.validate_registry_host("10.0.0.1:5000").await.err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
    }

    #[tokio::test]
    async fn production_dns_resolver_rejects_unsafe_actual_targets() {
        let resolver = FilteringDnsResolver;
        assert!(resolver
            .resolve("127.0.0.1".parse().expect("valid resolver name"))
            .await
            .is_err());
        assert!(resolver
            .resolve("localhost".parse().expect("valid resolver name"))
            .await
            .is_err());

        let addrs: Vec<_> = resolver
            .resolve("8.8.8.8".parse().expect("valid resolver name"))
            .await
            .expect("public literal allowed")
            .collect();
        assert_eq!(addrs, vec![SocketAddr::new("8.8.8.8".parse().unwrap(), 0)]);
    }

    #[test]
    fn ref_acceptance_rejects_unqualified_or_tag_only() {
        let hex = "a".repeat(64);
        // No '@digest' → tag-only / not digest-pinned → operator-fixable 422.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref("registry.example.com/x:1.2.3").err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        // Bare name (no '/') → shorthand, no registry host → 422.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref(&format!("alpine@sha256:{hex}")).err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        // org/app shorthand — host segment `myorg` has no '.'/':' and is not
        // `localhost` → 422.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref(&format!("myorg/app@sha256:{hex}")).err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        // `docker.io` is the Docker Hub shorthand (real endpoint is
        // `registry-1.docker.io`); refuse it rather than rewrite it → 422.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref(&format!("docker.io/library/x@sha256:{hex}")).err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        // Hostnames are case-insensitive: the shorthand is rejected regardless of case.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref(&format!("Docker.IO/library/x@sha256:{hex}")).err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        // Scheme-prefixed ref is malformed operator input → 422, not a 503.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref(&format!(
                "https://registry.example.com/acme/app@sha256:{hex}"
            ))
            .err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
        // Credential-bearing ref (userinfo before the host) must never be
        // accepted — it would smuggle creds into the verbatim registry URL.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref(&format!(
                "user:pass@registry.example.com/acme/app@sha256:{hex}"
            ))
            .err(),
            Some(RuntimeLabelError::UnqualifiedRef)
        );
    }

    #[test]
    fn ref_grammar_breach_is_internal_not_validation() {
        // A fully-qualified, digest-pinned ref whose digest grammar is malformed
        // is an invariant breach (PR1 validates at provision with the same
        // grammar, so a stored bad digest means DB drift) → 500, not 422.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref("registry.example.com/x@sha256:nothex").err(),
            Some(RuntimeLabelError::InvalidStoredRef)
        );
        // Too-short digest on an otherwise-qualified ref.
        assert_eq!(
            OciRuntimeLabelResolver::parse_ref("registry.example.com/x@sha256:abcd").err(),
            Some(RuntimeLabelError::InvalidStoredRef)
        );
        assert!(RuntimeLabelError::InvalidStoredRef.is_internal());
        assert!(!RuntimeLabelError::InvalidStoredRef.is_validation());
    }

    #[test]
    fn ref_acceptance_accepts_fully_qualified() {
        let hex = "a".repeat(64);
        let parsed =
            OciRuntimeLabelResolver::parse_ref(&format!("registry.example.com/x@sha256:{hex}"))
                .expect("dotted host accepted");
        assert_eq!(parsed.host, "registry.example.com");
        assert_eq!(parsed.repository, "x");
        assert_eq!(parsed.digest, format!("sha256:{hex}"));

        let loopback =
            OciRuntimeLabelResolver::parse_ref(&format!("127.0.0.1:5000/acme/app@sha256:{hex}"))
                .expect("host:port accepted");
        assert_eq!(loopback.host, "127.0.0.1:5000");
        assert_eq!(loopback.repository, "acme/app");
    }

    #[test]
    fn extract_config_labels_from_fixture() {
        // A realistic Docker image config blob with a `.config.Labels` map.
        let config = json!({
            "architecture": "amd64",
            "os": "linux",
            "config": {
                "Env": ["PATH=/usr/bin"],
                "Labels": {
                    "com.g5labs.runtime.schema": "v1",
                    "com.g5labs.runtime.base_image": "ubuntu@sha256:deadbeef",
                    "com.g5labs.runtime.testcontainers.enabled": "true",
                    "org.opencontainers.image.source": "https://example.com/repo"
                }
            }
        });
        let labels = extract_config_labels(&config).expect("labels read");
        let resolved = filter_and_extract(&labels).expect("filtered");
        assert_eq!(resolved.schema.as_deref(), Some("v1"));
        let stored = resolved.labels.as_object().unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(
            stored["com.g5labs.runtime.base_image"],
            "ubuntu@sha256:deadbeef"
        );
        assert_eq!(stored["com.g5labs.runtime.testcontainers.enabled"], "true");
    }

    #[test]
    fn extract_config_labels_missing_config_is_empty() {
        assert!(extract_config_labels(&json!({})).unwrap().is_empty());
        assert!(extract_config_labels(&json!({ "config": {} }))
            .unwrap()
            .is_empty());
        assert!(
            extract_config_labels(&json!({ "config": { "Labels": null } }))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn extract_config_labels_non_object_is_unreadable() {
        let err = extract_config_labels(&json!({ "config": { "Labels": "nope" } }))
            .expect_err("non-object labels rejected");
        assert_eq!(err, RuntimeLabelError::LabelsUnreadable);
    }

    #[test]
    fn index_detection_and_amd64_selection() {
        let index = json!({
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                { "digest": "sha256:arm", "platform": { "os": "linux", "architecture": "arm64" } },
                { "digest": "sha256:amd", "platform": { "os": "linux", "architecture": "amd64" } }
            ]
        });
        assert!(is_index(&index));
        assert_eq!(select_amd64_child(&index).as_deref(), Some("sha256:amd"));

        let image = json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfg" }
        });
        assert!(!is_index(&image));

        let no_match = json!({
            "manifests": [
                { "digest": "sha256:arm", "platform": { "os": "linux", "architecture": "arm64" } }
            ]
        });
        assert_eq!(select_amd64_child(&no_match), None);
    }

    #[test]
    fn error_classification_validation_vs_service() {
        assert!(RuntimeLabelError::UnqualifiedRef.is_validation());
        assert!(RuntimeLabelError::LabelsTooLarge.is_validation());
        assert!(!RuntimeLabelError::RegistryNotFound.is_validation());
        assert!(!RuntimeLabelError::Transport.is_validation());
        assert!(!RuntimeLabelError::RegistryAuth.is_validation());
    }
}
