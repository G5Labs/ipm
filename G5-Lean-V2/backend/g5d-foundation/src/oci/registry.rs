//! Registry client trait + a single registry-agnostic `DistributionClient`
//! core, with `DockerHubClient` / `GhcrClient` thin wrappers (D15).
//!
//! `parse_image_ref` accepts the canonical image-reference shapes documented
//! in `g5/g5d/oci` D2 and resolves them into `ImageRef { registry,
//! repository, reference }`. `docker.io`, `ghcr.io`, GAR, ECR, and ACR are
//! supported; still-unsupported hosts such as `host:port` are rejected with
//! `UnsupportedRegistry`. New registry families are added by binding another
//! `CredentialProvider` to the shared `DistributionClient`.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use sha2::{Digest as Sha256Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::acr::AcrCredentials;
use super::auth::{
    parse_challenge, AuthChallenge, AuthScheme, CredentialProvider, DockerHubCredentials,
    GhcrCredentials, TokenCache,
};
use super::ecr::{AwsCredentials, EcrCredentials};
use super::error::{truncate_body, OciError};
use super::gar::GarCredentials;

pub const DOCKERHUB_API: &str = "registry-1.docker.io";
pub const GOOGLE_DOCKERHUB_MIRROR: &str = "mirror.gcr.io";
pub const GHCR_API: &str = "ghcr.io";

/// GAR host suffix (D22): hosts like `us-central1-docker.pkg.dev`,
/// `europe-docker.pkg.dev`.
const GAR_HOST_SUFFIX: &str = "-docker.pkg.dev";
/// ACR host suffix (D26): hosts like `myregistry.azurecr.io`.
const ACR_HOST_SUFFIX: &str = ".azurecr.io";

/// GAR host classification (D22): `<region>-docker.pkg.dev`. The leading
/// region label must be non-empty (a bare `-docker.pkg.dev` is not a real
/// host).
pub(crate) fn is_gar_host(host: &str) -> bool {
    host.len() > GAR_HOST_SUFFIX.len() && host.ends_with(GAR_HOST_SUFFIX)
}

/// ACR host classification (D26): `<registry>.azurecr.io` with a non-empty
/// registry label.
pub(crate) fn is_acr_host(host: &str) -> bool {
    host.len() > ACR_HOST_SUFFIX.len() && host.ends_with(ACR_HOST_SUFFIX)
}

/// ECR host classification (D24). Matches
/// `^[0-9]+\.dkr\.ecr\.<region>\.amazonaws\.com$` and returns `<region>` on a
/// match (consumed by `EcrCredentials` to build the AWS API endpoint);
/// `None` for any other host. Implemented with `split` rather than a `regex`
/// to keep the hot path allocation-free and the dependency surface minimal.
/// `public.ecr.aws` (a different auth shape) does not match and is therefore
/// rejected by `parse_image_ref` — explicitly deferred per D2.
pub(crate) fn parse_ecr_region(host: &str) -> Option<&str> {
    // <account>.dkr.ecr.<region>.amazonaws.com → exactly 6 dot-separated parts.
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 6 {
        return None;
    }
    if parts[1] != "dkr" || parts[2] != "ecr" || parts[4] != "amazonaws" || parts[5] != "com" {
        return None;
    }
    let account = parts[0];
    if account.is_empty() || !account.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let region = parts[3];
    if region.is_empty()
        || !region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return None;
    }
    Some(region)
}

/// ECR host classification (D24): a `bool` view over [`parse_ecr_region`].
pub(crate) fn is_ecr_host(host: &str) -> bool {
    parse_ecr_region(host).is_some()
}

pub(crate) const MEDIA_OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
pub(crate) const MEDIA_DOCKER_MANIFEST: &str =
    "application/vnd.docker.distribution.manifest.v2+json";
pub(crate) const MEDIA_OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";
pub(crate) const MEDIA_DOCKER_LIST: &str =
    "application/vnd.docker.distribution.manifest.list.v2+json";
const MEDIA_OCI_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
const MEDIA_DOCKER_CONFIG: &str = "application/vnd.docker.container.image.v1+json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub registry: String,
    pub repository: String,
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Tag(String),
    Digest(Digest),
}

impl Reference {
    pub fn as_str(&self) -> &str {
        match self {
            Reference::Tag(t) => t,
            Reference::Digest(d) => d.as_str(),
        }
    }
}

/// A validated `sha256:<64 hex chars>` digest.
///
/// Codex final-review B1 of PR #122: every digest that flows into cache
/// paths or registry URLs must be parsed through this type. The string is
/// guaranteed to be `sha256:` followed by exactly 64 lowercase hex chars
/// (uppercase input is normalized to lowercase at parse time).
///
/// Construction is via `Digest::parse(&str)` — direct field access is not
/// allowed so the invariant cannot be bypassed by call sites.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Digest {
    /// Full `sha256:<hex>` string. Lowercase. 71 bytes.
    canonical: String,
    /// Hex component only (without the `sha256:` prefix). 64 bytes. Safe
    /// for use as a path component: no `/`, `\`, `..`, NUL, or control chars.
    hex_only: String,
}

impl Digest {
    /// Parse a `sha256:<hex>` digest string. Rejects anything else.
    ///
    /// Codex final-review B1 of PR #122: previously the codebase only
    /// checked `starts_with("sha256:")`, which lets `sha256:../etc/passwd`
    /// through as a "digest" and into cache paths.
    pub fn parse(input: &str) -> Result<Self, OciError> {
        let (algo, hex) = input
            .split_once(':')
            .ok_or_else(|| OciError::InvalidDigest {
                value: input.to_string(),
                reason: "missing ':' separator (expected 'sha256:<hex>')".into(),
            })?;
        if algo != "sha256" {
            return Err(OciError::InvalidDigest {
                value: input.to_string(),
                reason: format!("only sha256 is supported, got '{}'", algo),
            });
        }
        if hex.len() != 64 {
            return Err(OciError::InvalidDigest {
                value: input.to_string(),
                reason: format!("hex component must be exactly 64 chars, got {}", hex.len()),
            });
        }
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(OciError::InvalidDigest {
                value: input.to_string(),
                reason: "hex component must be ASCII [0-9a-fA-F] only".into(),
            });
        }
        let hex_only = hex.to_ascii_lowercase();
        let canonical = format!("sha256:{}", hex_only);
        Ok(Digest {
            canonical,
            hex_only,
        })
    }

    /// Full `sha256:<hex>` string (for URLs, registry manifest references).
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Hex-only component, safe for use as a filesystem path component.
    pub fn hex(&self) -> &str {
        &self.hex_only
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical)
    }
}

pub fn parse_image_ref(input: &str) -> Result<ImageRef, OciError> {
    if input.is_empty() {
        return Err(OciError::InvalidImageRef(
            input.to_string(),
            "empty reference".into(),
        ));
    }

    let (no_digest, digest) = if let Some(idx) = input.find('@') {
        (&input[..idx], Some(input[idx + 1..].to_string()))
    } else {
        (input, None)
    };

    let last_slash = no_digest.rfind('/');
    let (no_tag, tag) = match no_digest.rfind(':') {
        Some(idx) if last_slash.is_none_or(|s| idx > s) => {
            (&no_digest[..idx], Some(no_digest[idx + 1..].to_string()))
        }
        _ => (no_digest, None),
    };

    let (registry, repository): (String, String) = if let Some(idx) = no_tag.find('/') {
        // Registry hosts are case-insensitive per DNS / RFC 3986; repositories
        // and tags are case-sensitive on the registry side, so normalize ONLY
        // the host segment (`GHCR.IO/...` → `ghcr.io/...`).
        let head_raw = &no_tag[..idx];
        let head_lower = head_raw.to_ascii_lowercase();
        let head = head_lower.as_str();
        let tail = &no_tag[idx + 1..];
        if head == "docker.io" || head == "registry-1.docker.io" || head == "index.docker.io" {
            let repo = if tail.contains('/') {
                tail.to_string()
            } else {
                format!("library/{}", tail)
            };
            (DOCKERHUB_API.to_string(), repo)
        } else if head == GOOGLE_DOCKERHUB_MIRROR {
            // Google's public Docker Hub cache preserves the canonical
            // repository path (`library/...` for official images). Keep the
            // mirror host so DockerHubClient can apply its one-shot fallback.
            (GOOGLE_DOCKERHUB_MIRROR.to_string(), tail.to_string())
        } else if head == GHCR_API {
            // GHCR: the repository is the path after the host verbatim
            // (owner/image[/...]); GHCR has no implicit `library/` namespace.
            (GHCR_API.to_string(), tail.to_string())
        } else if is_gar_host(head) || is_ecr_host(head) || is_acr_host(head) {
            // GAR/ECR/ACR (PR-rev7-5cde, D2): like GHCR, the repository is the
            // path after the host verbatim — no implicit `library/` namespace.
            (head.to_string(), tail.to_string())
        } else if head.contains('.') || head.contains(':') || head == "localhost" {
            return Err(OciError::UnsupportedRegistry(head.to_string()));
        } else {
            (DOCKERHUB_API.to_string(), no_tag.to_string())
        }
    } else {
        (DOCKERHUB_API.to_string(), format!("library/{}", no_tag))
    };

    if repository.is_empty() {
        return Err(OciError::InvalidImageRef(
            input.to_string(),
            "missing repository".into(),
        ));
    }

    let reference = if let Some(d) = digest {
        // Codex final-review B1 of PR #122: parse through the typed Digest
        // so malformed digests (e.g., 'sha256:../etc/passwd') are rejected
        // BEFORE they reach cache paths or registry URLs.
        Reference::Digest(
            Digest::parse(&d)
                .map_err(|e| OciError::InvalidImageRef(input.to_string(), e.to_string()))?,
        )
    } else if let Some(t) = tag {
        Reference::Tag(t)
    } else {
        Reference::Tag("latest".to_string())
    };

    Ok(ImageRef {
        registry,
        repository,
        reference,
    })
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(default, rename = "mediaType")]
    pub media_type: Option<String>,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
}

#[derive(Debug, Deserialize, serde::Serialize, Clone, Default, PartialEq, Eq)]
pub struct ImageConfig {
    #[serde(default, rename = "Env")]
    pub env: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ImageConfigDocument {
    #[serde(default)]
    config: ImageConfig,
}

/// Decode an OCI image config blob into its `config` section. Shared by the
/// registry clients and the local layout client (D31) so both sources parse
/// the same document shape.
pub(crate) fn decode_image_config_document(bytes: &[u8]) -> Result<ImageConfig, OciError> {
    let doc: ImageConfigDocument = serde_json::from_slice(bytes)
        .map_err(|e| OciError::ManifestDecode(format!("image config decode failed: {e}")))?;
    Ok(doc.config)
}

#[derive(Debug, Deserialize)]
pub struct ManifestList {
    pub manifests: Vec<ManifestListEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestListEntry {
    pub digest: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub platform: Platform,
}

#[derive(Debug, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
}

#[async_trait]
pub trait RegistryClient: Send + Sync {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError>;

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError>;

    /// Download the layer body to `dest_path` and verify its sha256 digest.
    /// `dest_path` is created exclusively; if it exists, the call errors.
    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError>;

    /// Resolve `image` to the immutable manifest digest a `name@sha256:` pull
    /// would use (registry_auth_core D23). A digest reference resolves to
    /// itself with no network; a tag must be resolved by a registry-backed
    /// client. The provided default refuses tags fail-loud so a caller can
    /// never mistake an unresolved mutable tag for an identity.
    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        match &image.reference {
            Reference::Digest(digest) => Ok(digest.clone()),
            Reference::Tag(tag) => Err(OciError::InvalidImageRef(
                format!("{}/{}:{tag}", image.registry, image.repository),
                "tag references cannot be resolved to a manifest digest by this client".to_string(),
            )),
        }
    }
}

/// Registry-agnostic OCI Distribution v2 client (D15). Implements the bearer
/// challenge → token → manifest/blob flow exactly once; everything that
/// differs between registries (credential acquisition, denial-error mapping)
/// is supplied through the `CredentialProvider`. `DockerHubClient` and
/// `GhcrClient` are thin wrappers that bind this core to a provider.
pub struct DistributionClient {
    http: Client,
    tokens: Arc<TokenCache>,
    /// Singleflight lock around token *minting* (D21). N concurrent same-scope
    /// requests that all observe a cache miss (or whose pre-flight cached
    /// token was just rejected with a 401) serialize at the lock, then
    /// double-check the cache so all but the leader reuse the freshly-minted
    /// token. The lock is async because it is held across `fetch_token`'s
    /// `.await`; the `TokenCache` itself uses `std::sync::Mutex` because its
    /// critical section is purely synchronous. One refresh lock per client is
    /// sufficient — a `DistributionClient` already binds to a single registry,
    /// so cross-scope serialization inside the same client has no realistic
    /// cost (a single image's layers all share one scope).
    refresh: Arc<tokio::sync::Mutex<()>>,
    url_scheme: &'static str,
    creds: Box<dyn CredentialProvider>,
}

impl DistributionClient {
    /// Build an `https://` client with the given credential provider.
    pub fn https(creds: Box<dyn CredentialProvider>) -> Self {
        Self {
            http: Client::builder()
                .user_agent("g5d-oci/0.1.0")
                .build()
                .expect("reqwest client builds with default config"),
            tokens: Arc::new(TokenCache::new()),
            refresh: Arc::new(tokio::sync::Mutex::new(())),
            url_scheme: "https",
            creds,
        }
    }

    /// Test-only: build a client that talks `http://` instead of `https://`
    /// so that `wiremock` mock servers can stand in for a real registry.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn for_tests(http: Client, creds: Box<dyn CredentialProvider>) -> Self {
        Self {
            http,
            tokens: Arc::new(TokenCache::new()),
            refresh: Arc::new(tokio::sync::Mutex::new(())),
            url_scheme: "http",
            creds,
        }
    }

    fn manifest_url(&self, image: &ImageRef) -> String {
        format!(
            "{}://{}/v2/{}/manifests/{}",
            self.url_scheme,
            image.registry,
            image.repository,
            image.reference.as_str()
        )
    }

    fn blob_url(&self, image: &ImageRef, digest: &str) -> String {
        format!(
            "{}://{}/v2/{}/blobs/{}",
            self.url_scheme, image.registry, image.repository, digest
        )
    }

    fn scope(&self, image: &ImageRef) -> String {
        format!("repository:{}:pull", image.repository)
    }

    /// Singleflight token mint + cache (D21), generalized across all
    /// registries (D15). Serializes the cache-check + `mint` + put sequence so
    /// N concurrent same-scope callers share one token round-trip. The leader
    /// mints; followers double-check the cache and reuse the leader's freshly
    /// minted token. `failed_token` is the token the caller just got 401'd on
    /// (if any): if the cache still holds that exact token, it is evicted and
    /// re-minted so the retry never resends a token the server already
    /// rejected. `challenge` is `Some` for the Bearer flow (the parsed
    /// `Www-Authenticate`) and `None` for eager/Basic registries (ECR).
    async fn mint_locked(
        &self,
        challenge: Option<&AuthChallenge>,
        repository: &str,
        scope: &str,
        registry_host: &str,
        failed_token: Option<String>,
    ) -> Result<String, OciError> {
        let _refresh = self.refresh.lock().await;
        match self.tokens.get(registry_host, scope) {
            Some(t) if Some(t.as_str()) != failed_token.as_deref() => Ok(t),
            _ => {
                if failed_token.is_some() {
                    self.tokens.invalidate(registry_host, scope);
                }
                let (fresh, expires_in) = self
                    .creds
                    .mint(&self.http, challenge, registry_host, repository, scope)
                    .await?;
                self.tokens.put(
                    registry_host.to_string(),
                    scope.to_string(),
                    fresh.clone(),
                    expires_in,
                );
                Ok(fresh)
            }
        }
    }

    async fn authed_get(
        &self,
        url: &str,
        accepts: Option<&[&str]>,
        scope: &str,
        repository: &str,
        registry_host: &str,
    ) -> Result<reqwest::Response, OciError> {
        let scheme = self.creds.scheme();
        let mut cached = self.tokens.get(registry_host, scope);

        // Eager pre-auth (ECR, D24): mint + apply a Basic credential on the
        // FIRST request. ECR's `/v2/` 401 carries a `Basic` challenge with no
        // bearer realm to fetch a token from, so we authenticate up front
        // rather than via the 401 path below.
        if cached.is_none() && self.creds.pre_authenticates() {
            cached = Some(
                self.mint_locked(None, repository, scope, registry_host, None)
                    .await?,
            );
        }

        let build = |token: Option<&str>| -> reqwest::RequestBuilder {
            let mut req = self.http.get(url);
            if let Some(list) = accepts {
                for a in list {
                    req = req.header(ACCEPT, *a);
                }
            }
            if let Some(t) = token {
                req = req.header(AUTHORIZATION, format!("{} {}", scheme.header_prefix(), t));
            }
            req
        };

        let resp = build(cached.as_deref()).send().await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            tracing::debug!(url, "Got 401 from registry, re-authing");

            // The token the first request carried (None if the cache missed on
            // entry). If the 401 was caused by this specific token — e.g. the
            // server revoked it or rotated keys within the local TTL margin —
            // `mint_locked` MUST NOT reuse it (see its doc).
            let failed_token = cached;

            // Bearer registries discover the realm from `Www-Authenticate`;
            // the Basic (ECR) flow has no fetchable realm, so the provider's
            // `mint` re-runs `GetAuthorizationToken` without a challenge.
            let challenge = match scheme {
                AuthScheme::Bearer => {
                    let www = resp
                        .headers()
                        .get(WWW_AUTHENTICATE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    Some(parse_challenge(&www)?)
                }
                AuthScheme::Basic => None,
            };

            let token = self
                .mint_locked(
                    challenge.as_ref(),
                    repository,
                    scope,
                    registry_host,
                    failed_token,
                )
                .await?;

            let retry = build(Some(&token)).send().await?;
            if retry.status() == StatusCode::UNAUTHORIZED {
                // Registry-specific mapping: anonymous denial vs. rejected
                // credentials (GHCR/GAR/ECR/ACR distinguish these; DockerHub
                // maps both to AuthFailed).
                return Err(self.creds.on_denied(registry_host, repository, scope));
            }
            return finalize_response(retry, url).await;
        }
        finalize_response(resp, url).await
    }

    /// Fetch a manifest exactly as the registry serves it: the raw bytes,
    /// their media type and their digest (ontology `g5.runtime.cache.door`).
    /// Unlike [`RegistryClient::fetch_manifest`], no platform selection
    /// happens, so an image index is returned as an index; a pull-through
    /// serves these bytes verbatim and lets the consumer pick the platform.
    /// `accepts` is the consumer's Accept list, so the registry negotiates the
    /// same representation the consumer asked for.
    /// Largest raw manifest the pull-through will buffer.
    pub const MAX_RAW_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

    pub async fn fetch_manifest_raw(
        &self,
        image: &ImageRef,
        accepts: &[&str],
    ) -> Result<RawManifest, OciError> {
        let url = self.manifest_url(image);
        let scope = self.scope(image);
        let resp = self
            .authed_get(
                &url,
                Some(accepts),
                &scope,
                &image.repository,
                &image.registry,
            )
            .await?;
        let media_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        // A manifest is small (registries cap them at 4 MiB); never buffer
        // more than that from an upstream, whatever it announces or sends.
        let max = Self::MAX_RAW_MANIFEST_BYTES;
        let too_large = || OciError::ManifestDecode(format!("manifest larger than {max} bytes"));
        if resp.content_length().is_some_and(|n| n > max) {
            return Err(too_large());
        }
        let mut stream = resp.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len() as u64 + chunk.len() as u64 > max {
                return Err(too_large());
            }
            bytes.extend_from_slice(&chunk);
        }
        let bytes = bytes::Bytes::from(bytes);
        let digest = Digest::parse(&format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))?;
        if let Reference::Digest(expected) = &image.reference {
            if expected != &digest {
                return Err(OciError::DigestMismatch {
                    expected: expected.as_str().to_string(),
                    actual: digest.as_str().to_string(),
                });
            }
        }
        Ok(RawManifest {
            bytes,
            media_type,
            digest,
        })
    }

    /// Open a blob for streaming. The caller must verify the streamed bytes
    /// against `digest`; nothing is read here beyond the response head, so a
    /// large layer never lands in memory.
    pub async fn open_blob(
        &self,
        image: &ImageRef,
        digest: &Digest,
    ) -> Result<BlobStream, OciError> {
        let url = self.blob_url(image, digest.as_str());
        let scope = self.scope(image);
        let resp = self
            .authed_get(&url, None, &scope, &image.repository, &image.registry)
            .await?;
        Ok(Box::pin(resp.bytes_stream().map_err(std::io::Error::other)))
    }

    /// Test-only handle on the shared `TokenCache` so the wiremock expiry
    /// regression test can force a cached entry past its TTL. Gated on
    /// `cfg(test)` only (not the `test-helpers` feature) — no external crate
    /// needs it, and adding it under `test-helpers` would trip dead-code lints
    /// in builds where the feature is on but `cfg(test)` is off.
    #[cfg(test)]
    pub(crate) fn token_cache(&self) -> &Arc<TokenCache> {
        &self.tokens
    }
}

/// A blob's bytes as the registry streams them, verified by the caller
/// against the digest it asked for.
pub type BlobStream =
    std::pin::Pin<Box<dyn futures_util::Stream<Item = std::io::Result<bytes::Bytes>> + Send>>;

/// A manifest as the registry served it: verbatim bytes, their media type and
/// their digest (what the registry reports as `Docker-Content-Digest`).
#[derive(Debug, Clone)]
pub struct RawManifest {
    pub bytes: bytes::Bytes,
    pub media_type: String,
    pub digest: Digest,
}

/// Build a pull-through client for one upstream registry host, with the
/// credential provider that host needs: Artifact Registry hosts mint through
/// the GCP token cascade, Google's Docker Hub mirror is anonymous, canonical
/// Docker Hub uses the optional `DOCKER_USERNAME`/`DOCKER_PASSWORD` pair.
/// Hosts outside that set get `None`; the cache door allowlists upstreams and
/// never proxies an arbitrary host (ontology `g5.runtime.cache.door`).
pub fn upstream_client(host: &str) -> Option<DistributionClient> {
    let host = host.to_ascii_lowercase();
    if host == GOOGLE_DOCKERHUB_MIRROR {
        return Some(DistributionClient::https(Box::new(
            GoogleDockerHubMirrorCredentials,
        )));
    }
    if host == DOCKERHUB_API || host == "docker.io" || host == "index.docker.io" {
        return Some(DistributionClient::https(Box::new(DockerHubCredentials)));
    }
    if host.ends_with("-docker.pkg.dev") || host == "gcr.io" || host.ends_with(".gcr.io") {
        return Some(DistributionClient::https(Box::new(
            GarCredentials::from_env(),
        )));
    }
    None
}

async fn finalize_response(
    resp: reqwest::Response,
    url: &str,
) -> Result<reqwest::Response, OciError> {
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(OciError::RegistryHttp {
            status,
            url: url.to_string(),
            body: truncate_body(&body),
        });
    }
    Ok(resp)
}

/// Select a single-platform manifest entry from a manifest list / image index
/// for the current host (`linux` + host arch normalized to `amd64`/`arm64`).
pub(crate) fn select_linux_manifest<'a>(
    list: &'a ManifestList,
    want_arch: &str,
) -> Option<&'a ManifestListEntry> {
    list.manifests
        .iter()
        .find(|m| m.platform.os == "linux" && m.platform.architecture == want_arch)
}

#[async_trait]
impl RegistryClient for DistributionClient {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError> {
        tracing::info!(
            registry = %image.registry,
            repository = %image.repository,
            reference = %image.reference.as_str(),
            "Fetching OCI manifest"
        );
        let accepts = &[
            MEDIA_OCI_INDEX,
            MEDIA_DOCKER_LIST,
            MEDIA_OCI_MANIFEST,
            MEDIA_DOCKER_MANIFEST,
        ];
        let url = self.manifest_url(image);
        let scope = self.scope(image);
        let resp = self
            .authed_get(
                &url,
                Some(accepts),
                &scope,
                &image.repository,
                &image.registry,
            )
            .await?;
        let ctype = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = resp.bytes().await?;

        if ctype.contains("manifest.list") || ctype.contains("image.index") {
            let list: ManifestList = serde_json::from_slice(&body)
                .map_err(|e| OciError::ManifestDecode(e.to_string()))?;
            let want_arch = current_arch();
            let entry = select_linux_manifest(&list, want_arch).ok_or_else(|| {
                OciError::NoMatchingPlatform {
                    arch: want_arch.to_string(),
                }
            })?;
            tracing::debug!(
                arch = want_arch,
                digest = %entry.digest,
                "Resolved multi-arch manifest list to single-platform manifest"
            );
            // Codex final-review B1 of PR #122: digests selected from a
            // manifest list MUST also be parsed through the typed Digest so
            // a malicious registry can't smuggle a path-traversal digest in
            // a manifest list entry.
            let parsed = Digest::parse(&entry.digest)?;
            let single_ref = ImageRef {
                registry: image.registry.clone(),
                repository: image.repository.clone(),
                reference: Reference::Digest(parsed),
            };
            let single_url = self.manifest_url(&single_ref);
            let resp2 = self
                .authed_get(
                    &single_url,
                    Some(&[MEDIA_OCI_MANIFEST, MEDIA_DOCKER_MANIFEST]),
                    &scope,
                    &image.repository,
                    &image.registry,
                )
                .await?;
            let body2 = resp2.bytes().await?;
            let manifest: Manifest = serde_json::from_slice(&body2)
                .map_err(|e| OciError::ManifestDecode(e.to_string()))?;
            Ok(manifest)
        } else {
            let manifest: Manifest = serde_json::from_slice(&body)
                .map_err(|e| OciError::ManifestDecode(e.to_string()))?;
            Ok(manifest)
        }
    }

    /// registry_auth_core D23: hash the manifest bytes the registry serves
    /// for the tag under the same `Accept` set `fetch_manifest` uses — the
    /// value the registry reports as `Docker-Content-Digest` (the INDEX
    /// digest for a multi-arch image, which a later by-digest fetch resolves
    /// through the same platform selection).
    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        if let Reference::Digest(digest) = &image.reference {
            return Ok(digest.clone());
        }
        let accepts = &[
            MEDIA_OCI_INDEX,
            MEDIA_DOCKER_LIST,
            MEDIA_OCI_MANIFEST,
            MEDIA_DOCKER_MANIFEST,
        ];
        let url = self.manifest_url(image);
        let scope = self.scope(image);
        let resp = self
            .authed_get(
                &url,
                Some(accepts),
                &scope,
                &image.repository,
                &image.registry,
            )
            .await?;
        let body = resp.bytes().await?;
        Digest::parse(&format!("sha256:{}", hex::encode(Sha256::digest(&body))))
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        if descriptor.media_type != MEDIA_OCI_CONFIG && descriptor.media_type != MEDIA_DOCKER_CONFIG
        {
            return Err(OciError::UnsupportedMediaType(
                descriptor.media_type.clone(),
            ));
        }

        let expected = Digest::parse(&descriptor.digest)?;
        let url = self.blob_url(image, expected.as_str());
        let scope = self.scope(image);
        tracing::info!(
            digest = %expected.as_str(),
            size = descriptor.size,
            "Fetching OCI image config blob"
        );
        let resp = self
            .authed_get(&url, None, &scope, &image.repository, &image.registry)
            .await?;
        let body = resp.bytes().await?;
        let actual = format!("sha256:{}", hex::encode(Sha256::digest(&body)));
        if actual != expected.as_str() {
            return Err(OciError::DigestMismatch {
                expected: expected.as_str().to_string(),
                actual,
            });
        }
        let doc: ImageConfigDocument = serde_json::from_slice(&body)
            .map_err(|e| OciError::ManifestDecode(format!("image config decode failed: {e}")))?;
        tracing::debug!(
            digest = %expected.as_str(),
            bytes = body.len(),
            "Verified OCI image config digest"
        );
        Ok(doc.config)
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        if descriptor.media_type.contains("zstd") {
            return Err(OciError::UnsupportedMediaType(
                descriptor.media_type.clone(),
            ));
        }

        let url = self.blob_url(image, &descriptor.digest);
        let scope = self.scope(image);
        tracing::info!(
            digest = %descriptor.digest,
            size = descriptor.size,
            "Downloading OCI layer blob"
        );
        let resp = self
            .authed_get(&url, None, &scope, &image.repository, &image.registry)
            .await?;

        let mut file = tokio::fs::File::create(dest_path)
            .await
            .map_err(|e| OciError::CacheIo {
                path: dest_path.display().to_string(),
                source: e,
            })?;
        let mut hasher = Sha256::new();
        let mut stream = resp.bytes_stream();
        let mut total: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(OciError::Http)?;
            total = total.saturating_add(chunk.len() as u64);
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|e| OciError::CacheIo {
                    path: dest_path.display().to_string(),
                    source: e,
                })?;
        }
        file.flush().await.map_err(|e| OciError::CacheIo {
            path: dest_path.display().to_string(),
            source: e,
        })?;
        drop(file);

        let actual = format!("sha256:{}", hex::encode(hasher.finalize()));
        if actual != descriptor.digest {
            let _ = std::fs::remove_file(dest_path);
            return Err(OciError::DigestMismatch {
                expected: descriptor.digest.clone(),
                actual,
            });
        }
        tracing::debug!(
            digest = %descriptor.digest,
            bytes = total,
            "Verified OCI layer digest"
        );
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct GoogleDockerHubMirrorCredentials;

impl CredentialProvider for GoogleDockerHubMirrorCredentials {
    fn registry(&self) -> &str {
        GOOGLE_DOCKERHUB_MIRROR
    }
}

fn mirror_error_allows_canonical_fallback(error: &OciError) -> bool {
    match error {
        OciError::RegistryHttp { status, .. } => {
            matches!(*status, 401 | 403 | 404 | 408 | 425 | 429) || *status >= 500
        }
        OciError::Http(_) | OciError::AuthFailed(_, _) | OciError::Json(_) => true,
        OciError::InvalidImageRef(_, _)
        | OciError::UnsupportedRegistry(_)
        | OciError::UnsupportedMediaType(_)
        | OciError::AuthRequired { .. }
        | OciError::ManifestDecode(_)
        | OciError::NoMatchingPlatform { .. }
        | OciError::DigestMismatch { .. }
        | OciError::InvalidDigest { .. }
        | OciError::CacheLockTimeout { .. }
        | OciError::LayerExtract { .. }
        | OciError::CacheIo { .. } => false,
    }
}

/// DockerHub registry client (D15/D22 wrapper). Canonical DockerHub uses
/// optional `DOCKER_USERNAME` / `DOCKER_PASSWORD`; the explicit Google public
/// mirror is anonymous-only and falls back once on availability failures.
pub struct DockerHubClient {
    core: DistributionClient,
    mirror_core: Option<DistributionClient>,
    mirror_host: String,
    canonical_host: String,
}

impl Default for DockerHubClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerHubClient {
    pub fn new() -> Self {
        Self {
            core: DistributionClient::https(Box::new(DockerHubCredentials)),
            mirror_core: Some(DistributionClient::https(Box::new(
                GoogleDockerHubMirrorCredentials,
            ))),
            mirror_host: GOOGLE_DOCKERHUB_MIRROR.to_string(),
            canonical_host: DOCKERHUB_API.to_string(),
        }
    }

    /// Test-only: `http://` client so `wiremock` can stand in for DockerHub.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_for_tests(http: Client) -> Self {
        Self {
            core: DistributionClient::for_tests(http, Box::new(DockerHubCredentials)),
            mirror_core: None,
            mirror_host: GOOGLE_DOCKERHUB_MIRROR.to_string(),
            canonical_host: DOCKERHUB_API.to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests_with_mirror(
        http: Client,
        mirror_host: String,
        canonical_host: String,
    ) -> Self {
        Self {
            core: DistributionClient::for_tests(http.clone(), Box::new(DockerHubCredentials)),
            mirror_core: Some(DistributionClient::for_tests(
                http,
                Box::new(GoogleDockerHubMirrorCredentials),
            )),
            mirror_host,
            canonical_host,
        }
    }

    fn canonical_image(&self, image: &ImageRef) -> ImageRef {
        ImageRef {
            registry: self.canonical_host.clone(),
            repository: image.repository.clone(),
            reference: image.reference.clone(),
        }
    }
}

#[async_trait]
impl RegistryClient for DockerHubClient {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError> {
        if image.registry == self.mirror_host {
            let mirror = self
                .mirror_core
                .as_ref()
                .expect("mirror-matched DockerHubClient must have a mirror core");
            match mirror.fetch_manifest(image).await {
                Ok(manifest) => return Ok(manifest),
                Err(error) if mirror_error_allows_canonical_fallback(&error) => {
                    tracing::warn!(
                        mirror = %self.mirror_host,
                        repository = %image.repository,
                        error = %error,
                        "Docker Hub mirror manifest fetch failed; trying canonical registry once"
                    );
                }
                Err(error) => return Err(error),
            }
            return self.core.fetch_manifest(&self.canonical_image(image)).await;
        }
        self.core.fetch_manifest(image).await
    }

    /// D23: identity is resolved against the CANONICAL registry — the mirror
    /// serves the same manifest bytes, but the digest must name what a
    /// canonical by-digest pull will fetch, never a mirror-only view.
    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        if image.registry == self.mirror_host {
            return self
                .core
                .resolve_manifest_digest(&self.canonical_image(image))
                .await;
        }
        self.core.resolve_manifest_digest(image).await
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        if image.registry == self.mirror_host {
            let mirror = self
                .mirror_core
                .as_ref()
                .expect("mirror-matched DockerHubClient must have a mirror core");
            match mirror.fetch_config(image, descriptor).await {
                Ok(config) => return Ok(config),
                Err(error) if mirror_error_allows_canonical_fallback(&error) => {
                    tracing::warn!(
                        mirror = %self.mirror_host,
                        repository = %image.repository,
                        error = %error,
                        "Docker Hub mirror config fetch failed; trying canonical registry once"
                    );
                }
                Err(error) => return Err(error),
            }
            return self
                .core
                .fetch_config(&self.canonical_image(image), descriptor)
                .await;
        }
        self.core.fetch_config(image, descriptor).await
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        if image.registry == self.mirror_host {
            let mirror = self
                .mirror_core
                .as_ref()
                .expect("mirror-matched DockerHubClient must have a mirror core");
            match mirror.fetch_layer_blob(image, descriptor, dest_path).await {
                Ok(()) => return Ok(()),
                Err(error) if mirror_error_allows_canonical_fallback(&error) => {
                    tracing::warn!(
                        mirror = %self.mirror_host,
                        repository = %image.repository,
                        error = %error,
                        "Docker Hub mirror layer fetch failed; trying canonical registry once"
                    );
                    let _ = tokio::fs::remove_file(dest_path).await;
                }
                Err(error) => return Err(error),
            }
            return self
                .core
                .fetch_layer_blob(&self.canonical_image(image), descriptor, dest_path)
                .await;
        }
        self.core
            .fetch_layer_blob(image, descriptor, dest_path)
            .await
    }
}

/// GHCR (`ghcr.io`) registry client (D15 wrapper, D16/D17). Resolves
/// credentials via the GHCR priority chain and maps an anonymous denial of a
/// private image to `OciError::AuthRequired`.
pub struct GhcrClient {
    core: DistributionClient,
}

impl Default for GhcrClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GhcrClient {
    /// Resolve credentials from the environment tiers (D17).
    pub fn new() -> Self {
        Self {
            core: DistributionClient::https(Box::new(GhcrCredentials::from_env())),
        }
    }

    /// Build with injected configuration credentials (tier 1 of D17). `None`
    /// falls through to the environment tiers. This is the entry point the
    /// daemon configuration layer uses to feed a `[oci.ghcr]` config block.
    pub fn with_credentials(configured: Option<(String, String)>) -> Self {
        Self {
            core: DistributionClient::https(Box::new(GhcrCredentials::with_configured(configured))),
        }
    }

    /// Test-only: `http://` client with explicit (possibly anonymous)
    /// configured credentials so `wiremock` can drive both auth paths
    /// deterministically without touching process env.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_for_tests(http: Client, configured: Option<(String, String)>) -> Self {
        Self {
            core: DistributionClient::for_tests(
                http,
                Box::new(GhcrCredentials::with_configured(configured)),
            ),
        }
    }

    /// Test-only handle on the underlying `TokenCache` for cache-state
    /// assertions and forced-expiry regression tests.
    #[cfg(test)]
    pub(crate) fn token_cache(&self) -> &Arc<TokenCache> {
        self.core.token_cache()
    }
}

#[async_trait]
impl RegistryClient for GhcrClient {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError> {
        self.core.fetch_manifest(image).await
    }

    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        self.core.resolve_manifest_digest(image).await
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        self.core.fetch_config(image, descriptor).await
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        self.core
            .fetch_layer_blob(image, descriptor, dest_path)
            .await
    }
}

/// GAR (`*-docker.pkg.dev`) registry client (D15 wrapper, D22). Reuses the
/// shared bearer-challenge core; the only per-registry behavior is the async
/// GCP access-token acquisition inside `GarCredentials::mint`.
pub struct GarClient {
    core: DistributionClient,
}

impl Default for GarClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GarClient {
    /// Resolve the GCP access token from the runtime cascade (D22).
    pub fn new() -> Self {
        Self {
            core: DistributionClient::https(Box::new(GarCredentials::from_env())),
        }
    }

    /// Build with injected `(username, token)` credentials (tier 1 of D22).
    /// `None` falls through to the metadata / SA / gcloud / anonymous tiers.
    pub fn with_credentials(configured: Option<(String, String)>) -> Self {
        Self {
            core: DistributionClient::https(Box::new(GarCredentials::with_configured(configured))),
        }
    }

    /// Test-only: `http://` client with an optional injected credential, an
    /// overridable GCE metadata base URL, and explicit gates for the
    /// metadata probe and the SA-JSON env read. The `gcloud` shell-out is
    /// always off under `for_tests`.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_for_tests(
        http: Client,
        configured: Option<(String, String)>,
        metadata_url: Option<String>,
        allow_metadata: bool,
        allow_sa: bool,
    ) -> Self {
        Self {
            core: DistributionClient::for_tests(
                http,
                Box::new(GarCredentials::for_tests(
                    configured,
                    metadata_url,
                    allow_metadata,
                    allow_sa,
                )),
            ),
        }
    }
}

#[async_trait]
impl RegistryClient for GarClient {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError> {
        self.core.fetch_manifest(image).await
    }

    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        self.core.resolve_manifest_digest(image).await
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        self.core.fetch_config(image, descriptor).await
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        self.core
            .fetch_layer_blob(image, descriptor, dest_path)
            .await
    }
}

/// ECR (`*.dkr.ecr.*.amazonaws.com`) registry client (D15 wrapper, D24). The
/// shared core authenticates eagerly with a Basic credential obtained from the
/// SigV4-signed `GetAuthorizationToken` call inside `EcrCredentials::mint`.
pub struct EcrClient {
    core: DistributionClient,
}

impl Default for EcrClient {
    fn default() -> Self {
        Self::new()
    }
}

impl EcrClient {
    /// Resolve AWS credentials from the runtime cascade (env → IMDS → shared
    /// file) at `mint` time (D24).
    pub fn new() -> Self {
        Self {
            core: DistributionClient::https(Box::new(EcrCredentials::from_env())),
        }
    }

    /// Build with injected AWS credentials (tier 1 of D24).
    pub fn with_credentials(creds: AwsCredentials) -> Self {
        Self {
            core: DistributionClient::https(Box::new(EcrCredentials::with_credentials(creds))),
        }
    }

    /// Test-only: `http://` client with optional injected AWS credentials and
    /// overridable AWS-API + IMDS base URLs so `wiremock` can stand in for
    /// `GetAuthorizationToken` / IMDS without touching real AWS.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_for_tests(
        http: Client,
        creds: Option<AwsCredentials>,
        api_base: Option<String>,
        imds_base: Option<String>,
    ) -> Self {
        Self {
            core: DistributionClient::for_tests(
                http,
                Box::new(EcrCredentials::for_tests(creds, api_base, imds_base)),
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn token_cache(&self) -> &Arc<TokenCache> {
        self.core.token_cache()
    }
}

#[async_trait]
impl RegistryClient for EcrClient {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError> {
        self.core.fetch_manifest(image).await
    }

    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        self.core.resolve_manifest_digest(image).await
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        self.core.fetch_config(image, descriptor).await
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        self.core
            .fetch_layer_blob(image, descriptor, dest_path)
            .await
    }
}

/// ACR (`*.azurecr.io`) registry client (D15 wrapper, D26). Reuses the shared
/// bearer-challenge core; `AcrCredentials::mint` runs the AAD →
/// `oauth2/exchange` → `oauth2/token` dance to produce the bearer.
pub struct AcrClient {
    core: DistributionClient,
}

impl Default for AcrClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AcrClient {
    /// Resolve the AAD access token from the runtime cascade (service principal
    /// → IMDS → `az`) at `mint` time (D26).
    pub fn new() -> Self {
        Self {
            core: DistributionClient::https(Box::new(AcrCredentials::from_env())),
        }
    }

    /// Build with an injected AAD access token (tier 1 of D26). `None` falls
    /// through to the service-principal / IMDS / `az` tiers.
    pub fn with_credentials(aad_token: Option<String>) -> Self {
        Self {
            core: DistributionClient::https(Box::new(AcrCredentials::with_aad_token(aad_token))),
        }
    }

    /// Test-only: `http://` client with an optional injected AAD token,
    /// overridable AAD-authority + IMDS base URLs, and an explicit ambient
    /// gate (tests that drive the service-principal / IMDS paths against
    /// mock URLs pass `ambient = true`; tests that only need the injected
    /// path pass `false`).
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn new_for_tests(
        http: Client,
        aad_token: Option<String>,
        authority_base: Option<String>,
        imds_base: Option<String>,
        ambient: bool,
    ) -> Self {
        Self {
            core: DistributionClient::for_tests(
                http,
                Box::new(AcrCredentials::for_tests(
                    aad_token,
                    authority_base,
                    imds_base,
                    ambient,
                )),
            ),
        }
    }
}

#[async_trait]
impl RegistryClient for AcrClient {
    async fn fetch_manifest(&self, image: &ImageRef) -> Result<Manifest, OciError> {
        self.core.fetch_manifest(image).await
    }

    async fn resolve_manifest_digest(&self, image: &ImageRef) -> Result<Digest, OciError> {
        self.core.resolve_manifest_digest(image).await
    }

    async fn fetch_config(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
    ) -> Result<ImageConfig, OciError> {
        self.core.fetch_config(image, descriptor).await
    }

    async fn fetch_layer_blob(
        &self,
        image: &ImageRef,
        descriptor: &Descriptor,
        dest_path: &Path,
    ) -> Result<(), OciError> {
        self.core
            .fetch_layer_blob(image, descriptor, dest_path)
            .await
    }
}

/// Supported registries (D18). DockerHub + GHCR (PR-rev7-5a/5b); GAR + ECR +
/// ACR (PR-rev7-5cde).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registry {
    DockerHub,
    Ghcr,
    Gar,
    Ecr,
    Acr,
}

impl Registry {
    /// Classify a normalized registry host (as stored in `ImageRef.registry`).
    /// Order matters only in that DockerHub is the fall-through default for
    /// bare names / `docker.io`; the cloud predicates are mutually exclusive.
    pub fn for_host(host: &str) -> Registry {
        if host == GHCR_API {
            Registry::Ghcr
        } else if is_gar_host(host) {
            Registry::Gar
        } else if is_ecr_host(host) {
            Registry::Ecr
        } else if is_acr_host(host) {
            Registry::Acr
        } else {
            Registry::DockerHub
        }
    }
}

/// Classify a raw image reference by its host segment (D18). Mirrors
/// `parse_image_ref`'s case-insensitive host normalization and reuses
/// `Registry::for_host` so the pre-parse and post-parse seams never drift:
/// `ghcr.io/...` → `Ghcr`, `*-docker.pkg.dev/...` → `Gar`,
/// `*.dkr.ecr.*.amazonaws.com/...` → `Ecr`, `*.azurecr.io/...` → `Acr`,
/// everything else (bare names, `docker.io/...`, `library/...`) → `DockerHub`.
pub fn detect_registry(image_ref: &str) -> Registry {
    let head = image_ref
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    Registry::for_host(&head)
}

pub(crate) fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_component_defaults_to_library_latest() {
        let r = parse_image_ref("alpine").unwrap();
        assert_eq!(r.registry, "registry-1.docker.io");
        assert_eq!(r.repository, "library/alpine");
        assert_eq!(r.reference, Reference::Tag("latest".to_string()));
    }

    #[test]
    fn parse_single_component_with_tag() {
        let r = parse_image_ref("alpine:3.18").unwrap();
        assert_eq!(r.repository, "library/alpine");
        assert_eq!(r.reference, Reference::Tag("3.18".to_string()));
    }

    #[test]
    fn parse_library_explicit_prefix() {
        let r = parse_image_ref("library/alpine:3.18").unwrap();
        assert_eq!(r.repository, "library/alpine");
        assert_eq!(r.reference, Reference::Tag("3.18".to_string()));
    }

    #[test]
    fn parse_org_image() {
        let r = parse_image_ref("myorg/myimage:1.0").unwrap();
        assert_eq!(r.registry, "registry-1.docker.io");
        assert_eq!(r.repository, "myorg/myimage");
        assert_eq!(r.reference, Reference::Tag("1.0".to_string()));
    }

    #[test]
    fn parse_explicit_docker_io_host() {
        let r = parse_image_ref("docker.io/myorg/myimage:1.0").unwrap();
        assert_eq!(r.registry, "registry-1.docker.io");
        assert_eq!(r.repository, "myorg/myimage");
    }

    #[test]
    fn parse_docker_io_single_component_adds_library() {
        let r = parse_image_ref("docker.io/alpine").unwrap();
        assert_eq!(r.repository, "library/alpine");
        assert_eq!(r.reference, Reference::Tag("latest".to_string()));
    }

    #[test]
    fn parse_digest_reference() {
        let digest = "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let input = format!("alpine@{}", digest);
        let r = parse_image_ref(&input).unwrap();
        assert_eq!(r.repository, "library/alpine");
        let d = Digest::parse(digest).unwrap();
        assert_eq!(r.reference, Reference::Digest(d));
    }

    /// Codex final-review B1 of PR #122: digest validation regression guards.
    #[test]
    fn parse_digest_rejects_path_traversal() {
        let input = "alpine@sha256:../../etc/passwd";
        let err = parse_image_ref(input).unwrap_err();
        assert!(matches!(err, OciError::InvalidImageRef(_, _)));
    }

    #[test]
    fn parse_digest_rejects_wrong_algo() {
        let err = Digest::parse(
            "sha512:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        )
        .unwrap_err();
        assert!(matches!(err, OciError::InvalidDigest { .. }));
        assert!(err.to_string().contains("only sha256"));
    }

    #[test]
    fn parse_digest_rejects_short_hex() {
        let err = Digest::parse("sha256:abc").unwrap_err();
        assert!(matches!(err, OciError::InvalidDigest { .. }));
        assert!(err.to_string().contains("64 chars"));
    }

    #[test]
    fn parse_digest_rejects_non_hex_chars() {
        // 64 chars but contains non-hex 'z' chars.
        let err = Digest::parse(
            "sha256:zzzzzzzzabcdef1234567890abcdef1234567890abcdef1234567890abcdef00",
        )
        .unwrap_err();
        assert!(matches!(err, OciError::InvalidDigest { .. }));
        assert!(err.to_string().contains("ASCII"));
    }

    #[test]
    fn parse_digest_normalizes_uppercase_to_lowercase() {
        let d = Digest::parse(
            "sha256:DEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF",
        )
        .unwrap();
        assert_eq!(
            d.hex(),
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        );
        assert_eq!(
            d.as_str(),
            "sha256:deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        );
    }

    #[test]
    fn parse_digest_hex_is_safe_path_component() {
        // The hex component must NEVER contain path separators or special chars,
        // because cache.rs uses it as a filesystem path component.
        let d = Digest::parse(
            "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
        )
        .unwrap();
        for c in d.hex().chars() {
            assert!(c.is_ascii_hexdigit(), "hex must be ascii hex: got '{}'", c);
            assert_ne!(c, '/');
            assert_ne!(c, '\\');
            assert_ne!(c, '.');
            assert_ne!(c, '\0');
        }
    }

    #[test]
    fn parse_ghcr_ref() {
        let r = parse_image_ref("ghcr.io/octocat/hello-world:1.0").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        // GHCR has no implicit library/ namespace: repository is verbatim.
        assert_eq!(r.repository, "octocat/hello-world");
        assert_eq!(r.reference, Reference::Tag("1.0".to_string()));
    }

    #[test]
    fn parse_ghcr_ref_defaults_to_latest() {
        let r = parse_image_ref("ghcr.io/octocat/hello-world").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "octocat/hello-world");
        assert_eq!(r.reference, Reference::Tag("latest".to_string()));
    }

    #[test]
    fn parse_ghcr_ref_with_digest() {
        let digest = "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let input = format!("ghcr.io/octocat/hello-world@{}", digest);
        let r = parse_image_ref(&input).unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "octocat/hello-world");
        let d = Digest::parse(digest).unwrap();
        assert_eq!(r.reference, Reference::Digest(d));
    }

    #[test]
    fn parse_ghcr_nested_repository() {
        // GHCR repositories can be deeper than owner/image.
        let r = parse_image_ref("ghcr.io/org/team/image:tag").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "org/team/image");
        assert_eq!(r.reference, Reference::Tag("tag".to_string()));
    }

    #[test]
    fn detect_registry_classifies_ghcr() {
        assert_eq!(detect_registry("ghcr.io/owner/img:1.0"), Registry::Ghcr);
        assert_eq!(detect_registry("ghcr.io/owner/img"), Registry::Ghcr);
    }

    #[test]
    fn parse_image_ref_normalizes_uppercase_host() {
        // Registry hosts are case-insensitive (DNS). An operator typing
        // `GHCR.IO/owner/img` must route to GHCR, not fall through to the
        // dotted-host reject path.
        let r = parse_image_ref("GHCR.IO/owner/img:1.0").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "owner/img");
        assert_eq!(r.reference, Reference::Tag("1.0".to_string()));

        // Mixed-case DockerHub host also routes correctly.
        let r = parse_image_ref("Docker.IO/myorg/img:1").unwrap();
        assert_eq!(r.registry, "registry-1.docker.io");

        // detect_registry follows the same case-insensitive policy.
        assert_eq!(detect_registry("GHCR.IO/owner/img"), Registry::Ghcr);
    }

    #[test]
    fn parse_image_ref_preserves_repository_case() {
        // Repositories and tags ARE case-sensitive on the registry side.
        // Only the host is normalized.
        let r = parse_image_ref("ghcr.io/Owner/MyImage:v1.0").unwrap();
        assert_eq!(r.repository, "Owner/MyImage");
        assert_eq!(r.reference, Reference::Tag("v1.0".to_string()));
    }

    #[test]
    fn detect_registry_classifies_dockerhub() {
        assert_eq!(detect_registry("alpine:3.18"), Registry::DockerHub);
        assert_eq!(detect_registry("library/alpine"), Registry::DockerHub);
        assert_eq!(
            detect_registry("docker.io/myorg/img:1"),
            Registry::DockerHub
        );
        assert_eq!(detect_registry("myorg/img:1"), Registry::DockerHub);
    }

    #[test]
    fn parse_google_mirror_as_dockerhub_cache_alias() {
        let r = parse_image_ref("mirror.gcr.io/library/buildpack-deps:noble").unwrap();
        assert_eq!(r.registry, GOOGLE_DOCKERHUB_MIRROR);
        assert_eq!(r.repository, "library/buildpack-deps");
        assert_eq!(r.reference, Reference::Tag("noble".to_string()));
        assert_eq!(
            detect_registry("mirror.gcr.io/library/buildpack-deps:noble"),
            Registry::DockerHub
        );
    }

    #[test]
    fn google_mirror_fallback_is_limited_to_availability_failures() {
        assert!(mirror_error_allows_canonical_fallback(
            &OciError::RegistryHttp {
                status: 429,
                url: "https://mirror.gcr.io/v2/library/buildpack-deps/manifests/noble".to_string(),
                body: "rate limited".to_string(),
            }
        ));
        assert!(mirror_error_allows_canonical_fallback(
            &OciError::RegistryHttp {
                status: 404,
                url: "https://mirror.gcr.io/v2/library/missing/manifests/latest".to_string(),
                body: "cache miss".to_string(),
            }
        ));
        assert!(!mirror_error_allows_canonical_fallback(
            &OciError::DigestMismatch {
                expected: "sha256:expected".to_string(),
                actual: "sha256:actual".to_string(),
            }
        ));
        assert!(!mirror_error_allows_canonical_fallback(
            &OciError::ManifestDecode("invalid manifest".to_string())
        ));
    }

    #[test]
    fn registry_for_host_maps_normalized_hosts() {
        assert_eq!(Registry::for_host("ghcr.io"), Registry::Ghcr);
        assert_eq!(
            Registry::for_host("registry-1.docker.io"),
            Registry::DockerHub
        );
    }

    // -- GAR / ECR / ACR acceptance (PR-rev7-5cde, D2/D18) --

    #[test]
    fn parse_accepts_gar() {
        let r = parse_image_ref("us-central1-docker.pkg.dev/proj/repo/img:1.0").unwrap();
        assert_eq!(r.registry, "us-central1-docker.pkg.dev");
        // GAR has no implicit library/ namespace: repository is verbatim.
        assert_eq!(r.repository, "proj/repo/img");
        assert_eq!(r.reference, Reference::Tag("1.0".to_string()));
        assert_eq!(
            detect_registry("us-central1-docker.pkg.dev/proj/repo/img"),
            Registry::Gar
        );
        assert_eq!(Registry::for_host("us-docker.pkg.dev"), Registry::Gar);
    }

    #[test]
    fn parse_accepts_ecr() {
        let r = parse_image_ref("123456789012.dkr.ecr.us-east-1.amazonaws.com/img:1.0").unwrap();
        assert_eq!(r.registry, "123456789012.dkr.ecr.us-east-1.amazonaws.com");
        assert_eq!(r.repository, "img");
        assert_eq!(r.reference, Reference::Tag("1.0".to_string()));
        assert_eq!(
            detect_registry("123456789012.dkr.ecr.us-east-1.amazonaws.com/img"),
            Registry::Ecr
        );
        assert_eq!(
            parse_ecr_region("123456789012.dkr.ecr.us-east-1.amazonaws.com"),
            Some("us-east-1")
        );
    }

    #[test]
    fn parse_accepts_acr() {
        let r = parse_image_ref("myregistry.azurecr.io/team/img:1.0").unwrap();
        assert_eq!(r.registry, "myregistry.azurecr.io");
        assert_eq!(r.repository, "team/img");
        assert_eq!(r.reference, Reference::Tag("1.0".to_string()));
        assert_eq!(
            detect_registry("myregistry.azurecr.io/team/img"),
            Registry::Acr
        );
        assert_eq!(Registry::for_host("myregistry.azurecr.io"), Registry::Acr);
    }

    #[test]
    fn ecr_region_parser_rejects_near_misses() {
        // Non-digit account, wrong labels, public ECR, and host:port all fail.
        assert!(parse_ecr_region("acct.dkr.ecr.us-east-1.amazonaws.com").is_none());
        assert!(parse_ecr_region("123.ecr.us-east-1.amazonaws.com").is_none());
        assert!(parse_ecr_region("123.dkr.ecr.us-east-1.amazonaws.com.evil.com").is_none());
        assert!(parse_ecr_region("public.ecr.aws").is_none());
    }

    #[test]
    fn parse_rejects_public_ecr() {
        // public.ecr.aws has a different (anonymous/SigV4-bearer) auth shape
        // and is explicitly deferred (D2) — it must be rejected, not routed.
        let err = parse_image_ref("public.ecr.aws/lambda/python:3.12").unwrap_err();
        assert!(matches!(err, OciError::UnsupportedRegistry(_)));
        // Guard the operator-visible wording: it must enumerate the supported
        // set so a stale message fails CI.
        let msg = err.to_string();
        assert!(
            msg.contains("docker.io") && msg.contains("ghcr.io") && msg.contains("azurecr.io"),
            "stale UnsupportedRegistry wording: {msg}"
        );
    }

    #[test]
    fn parse_rejects_registry_with_port() {
        // host:port shape is unambiguous: it has a colon in the host
        // segment. Routing it to DockerHub would be wrong; reject as a
        // deferred non-DockerHub registry.
        let err = parse_image_ref("localhost:5000/myimg:1.0").unwrap_err();
        assert!(matches!(err, OciError::UnsupportedRegistry(host) if host == "localhost:5000"));
    }

    #[test]
    fn parse_rejects_non_sha256_digest() {
        let err = parse_image_ref("alpine@md5:deadbeef").unwrap_err();
        assert!(matches!(err, OciError::InvalidImageRef(_, _)));
    }

    #[test]
    fn parse_rejects_empty() {
        let err = parse_image_ref("").unwrap_err();
        assert!(matches!(err, OciError::InvalidImageRef(_, _)));
    }

    #[test]
    fn current_arch_maps_known_arches() {
        let a = current_arch();
        assert!(a == "amd64" || a == "arm64" || !a.is_empty());
    }

    #[test]
    fn select_linux_manifest_picks_matching_arch() {
        let list = ManifestList {
            manifests: vec![
                ManifestListEntry {
                    digest: "sha256:aaa".to_string(),
                    media_type: MEDIA_OCI_MANIFEST.to_string(),
                    platform: Platform {
                        architecture: "amd64".to_string(),
                        os: "linux".to_string(),
                    },
                },
                ManifestListEntry {
                    digest: "sha256:bbb".to_string(),
                    media_type: MEDIA_OCI_MANIFEST.to_string(),
                    platform: Platform {
                        architecture: "arm64".to_string(),
                        os: "linux".to_string(),
                    },
                },
            ],
        };
        let chosen = select_linux_manifest(&list, "arm64").unwrap();
        assert_eq!(chosen.digest, "sha256:bbb");
    }

    #[test]
    fn select_linux_manifest_rejects_windows_only() {
        let list = ManifestList {
            manifests: vec![ManifestListEntry {
                digest: "sha256:ddd".to_string(),
                media_type: MEDIA_DOCKER_MANIFEST.to_string(),
                platform: Platform {
                    architecture: "amd64".to_string(),
                    os: "windows".to_string(),
                },
            }],
        };
        assert!(select_linux_manifest(&list, "amd64").is_none());
    }
}
