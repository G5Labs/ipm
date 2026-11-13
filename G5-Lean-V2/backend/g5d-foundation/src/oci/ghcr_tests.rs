//! GHCR auth-flow tests against a mocked `ghcr.io` registry (D20).
//!
//! Exercises the GHCR bearer flow (401 challenge → token endpoint → authed
//! manifest) for both the anonymous and authenticated paths, plus the typed
//! error mapping (`AuthRequired`, `RegistryHttp{403}`, malformed challenge),
//! without ever touching the real `ghcr.io`.
//!
//! Determinism note: tests that must observe the *anonymous* credential
//! resolution clear the GHCR/GitHub env vars under `#[serial]` so ambient CI
//! credentials (e.g. `GITHUB_TOKEN`) cannot leak in. Tests that need
//! credentials inject them via `GhcrClient::new_for_tests(.., Some(..))`
//! (config tier), which short-circuits env entirely and needs no serialization.

use std::sync::Arc;

use base64::Engine as _;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;
use serial_test::serial;
use sha2::{Digest as _, Sha256};
use tar::{Builder, Header};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::cache::LayerCache;
use super::error::OciError;
use super::extract::OciExtractor;
use super::registry::{GhcrClient, ImageRef, Reference, RegistryClient};

const REPO: &str = "octocat/hello-world";
const TAG: &str = "1.0";

/// Restores a set of environment variables on drop; used to neutralize
/// ambient GHCR/GitHub credentials for the anonymous-path tests.
struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn clear(keys: &[&'static str]) -> Self {
        let saved = keys
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect::<Vec<_>>();
        for k in keys {
            std::env::remove_var(k);
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

const GHCR_ENV_KEYS: &[&str] = &[
    "GHCR_TOKEN",
    "GHCR_USERNAME",
    "GITHUB_TOKEN",
    "GITHUB_ACTOR",
];

fn manifest_body() -> serde_json::Value {
    json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "size": 2,
        },
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                "size": 10,
            }
        ]
    })
}

fn host_of(server: &MockServer) -> String {
    server
        .uri()
        .strip_prefix("http://")
        .expect("MockServer URI begins with http://")
        .to_string()
}

fn image_for(host: &str) -> ImageRef {
    ImageRef {
        registry: host.to_string(),
        repository: REPO.to_string(),
        reference: Reference::Tag(TAG.to_string()),
    }
}

fn challenge_header(server: &MockServer, scope: &str) -> String {
    if scope.is_empty() {
        format!(r#"Bearer realm="{}/token",service="ghcr.io""#, server.uri())
    } else {
        format!(
            r#"Bearer realm="{}/token",service="ghcr.io",scope="{}""#,
            server.uri(),
            scope
        )
    }
}

/// Mount the initial unauthenticated 401 challenge (consumed once).
async fn mount_challenge(server: &MockServer, scope: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .respond_with(
            ResponseTemplate::new(401)
                .insert_header("Www-Authenticate", challenge_header(server, scope).as_str()),
        )
        .up_to_n_times(1)
        .mount(server)
        .await;
}

#[tokio::test]
#[serial]
async fn ghcr_anonymous_flow_succeeds() {
    let _guard = EnvGuard::clear(GHCR_ENV_KEYS);
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    // Anonymous token endpoint — no Authorization required.
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "anon-token",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer anon-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, None);
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("anonymous GHCR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    // The token request must NOT carry an Authorization header (anonymous).
    let token_reqs: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .collect();
    assert_eq!(token_reqs.len(), 1);
    assert!(
        token_reqs[0].headers.get("authorization").is_none(),
        "anonymous token request must not send Authorization"
    );
}

#[tokio::test]
async fn ghcr_authenticated_flow_sends_basic_auth() {
    // Inject config-tier credentials so the test is independent of env.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    let user = "octocat";
    let pat = "pat-secret";
    let expected_basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, pat))
    );

    mount_challenge(&server, &scope).await;

    // Token endpoint only matches when the Basic header is present, so a
    // successful fetch proves the client authenticated.
    Mock::given(method("GET"))
        .and(path("/token"))
        .and(header("Authorization", expected_basic.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "authed-token",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer authed-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some((user.to_string(), pat.to_string())));
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("authenticated GHCR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    let token_reqs: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .collect();
    assert_eq!(token_reqs.len(), 1);
    assert_eq!(
        token_reqs[0]
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        Some(expected_basic.as_str()),
        "token request must carry Basic auth"
    );
}

#[tokio::test]
#[serial]
async fn ghcr_private_image_without_credentials_is_auth_required() {
    let _guard = EnvGuard::clear(GHCR_ENV_KEYS);
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "anon-token",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;

    // Private image: even with the anonymous token, the manifest stays 401.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer anon-token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, None);
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("private image without creds must error");
    match err {
        OciError::AuthRequired {
            registry,
            image,
            scope: s,
        } => {
            assert_eq!(registry, "ghcr.io");
            assert_eq!(image, REPO);
            assert_eq!(s, scope);
        }
        other => panic!("expected AuthRequired, got {:?}", other),
    }
}

#[tokio::test]
async fn ghcr_forbidden_maps_to_registry_http_403() {
    // Token valid but the caller lacks permission → 403 on the manifest.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "authed-token",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer authed-token"))
        .respond_with(ResponseTemplate::new(403).set_body_string("denied"))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some(("u".into(), "p".into())));
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("403 must error");
    match err {
        OciError::RegistryHttp { status, .. } => assert_eq!(status, 403),
        other => panic!("expected RegistryHttp 403, got {:?}", other),
    }
}

#[tokio::test]
async fn ghcr_malformed_challenge_errors() {
    let server = MockServer::start().await;
    let host = host_of(&server);

    // 401 with a non-Bearer (malformed) Www-Authenticate header.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .respond_with(
            ResponseTemplate::new(401).insert_header("Www-Authenticate", "Basic realm=\"ghcr\""),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some(("u".into(), "p".into())));
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("malformed challenge must error");
    assert!(matches!(err, OciError::AuthFailed(_, _)), "got {:?}", err);
}

#[tokio::test]
async fn ghcr_missing_scope_challenge_with_persistent_401_is_auth_failed() {
    // Challenge omits scope; with injected creds the post-token 401 maps to
    // AuthFailed (credentials offered but rejected).
    let server = MockServer::start().await;
    let host = host_of(&server);

    mount_challenge(&server, "").await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "scopeless-token",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer scopeless-token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some(("u".into(), "p".into())));
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("persistent 401 with creds must error");
    assert!(matches!(err, OciError::AuthFailed(_, _)), "got {:?}", err);
}

#[tokio::test]
async fn ghcr_token_is_cached_across_pulls() {
    // The token endpoint must be hit exactly once across two manifest pulls
    // on the same client — the cached token (per repository scope) is reused.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "cached-token",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer cached-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some(("u".into(), "p".into())));
    let image = image_for(&host);

    client.fetch_manifest(&image).await.expect("first pull");
    client.fetch_manifest(&image).await.expect("second pull");

    let token_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .count();
    assert_eq!(
        token_hits, 1,
        "token must be cached and reused across pulls"
    );
}

#[tokio::test]
async fn ghcr_concurrent_pulls_share_exactly_one_token_fetch() {
    // Drives 4 concurrent fetch_manifest calls for the same scope. With the
    // singleflight refresh lock + cache double-check (D21), exactly one
    // caller mints the token and the other three reuse it on the
    // double-check pass — token endpoint hit count is exactly 1.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    // Mount the bearer-success mock FIRST so wiremock prefers it for authed
    // retries: wiremock tries mocks in registration order and the 401
    // fallback below has no header constraint, so it would catch retries too
    // if mounted first.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer concurrent-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .respond_with(ResponseTemplate::new(401).insert_header(
            "Www-Authenticate",
            challenge_header(&server, &scope).as_str(),
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "concurrent-token",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = Arc::new(GhcrClient::new_for_tests(
        http,
        Some(("u".into(), "p".into())),
    ));
    let image = image_for(&host);

    let mut handles = Vec::with_capacity(4);
    for _ in 0..4 {
        let client = client.clone();
        let image = image.clone();
        handles.push(tokio::spawn(
            async move { client.fetch_manifest(&image).await },
        ));
    }
    for h in handles {
        h.await.expect("join").expect("manifest fetch succeeds");
    }

    let token_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .count();
    // Singleflight guarantees exactly one /token round-trip under N concurrent
    // same-scope cache-miss pulls. If a future refactor removes the
    // double-check or the refresh lock, this assertion is the load-bearing
    // regression catch — re-widening it to a range would silently restore the
    // pre-D21 N-mint behavior.
    assert_eq!(
        token_hits, 1,
        "singleflight (D21) must collapse 4 concurrent same-scope mints into exactly 1 /token hit, got {}",
        token_hits
    );
}

#[tokio::test]
async fn ghcr_expired_token_triggers_refetch() {
    // First fetch_manifest mints a token; we then force-expire that cache
    // entry past its TTL via the test-only TokenCache helper, and a second
    // fetch_manifest must re-hit the token endpoint. Proves the expired
    // cache entry is invalidated (D16 step 4 / D20 expiry behavior).
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    // Bearer-success mock FIRST (registration order: wiremock prefers older
    // registrations on match) so authed retries hit the 200 path, not the
    // catch-all 401 fallback.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer first-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .respond_with(ResponseTemplate::new(401).insert_header(
            "Www-Authenticate",
            challenge_header(&server, &scope).as_str(),
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "first-token",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some(("u".into(), "p".into())));
    let image = image_for(&host);

    client.fetch_manifest(&image).await.expect("first pull");

    // Force the cached entry past its TTL. In production the cache's 30s
    // floor would keep this entry live; the test-only helper bypasses the
    // floor so we can prove the expiry codepath actually re-fetches.
    client.token_cache().force_insert_expired(
        host.clone(),
        scope.clone(),
        "first-token".to_string(),
    );

    client.fetch_manifest(&image).await.expect("second pull");

    let token_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .count();
    assert_eq!(
        token_hits, 2,
        "expired cache entry must trigger a fresh token fetch on the second pull"
    );
}

#[tokio::test]
async fn ghcr_cached_token_revoked_mid_ttl_triggers_fresh_mint() {
    // Regression for the post-401 double-check inside the singleflight refresh
    // lock (PR-rev7-5b round-2 codex blocker). A token can be locally TTL-valid
    // and still be rejected by the server — typical causes are server-side
    // revocation, registry key rotation, or scope mismatch within the local
    // safety margin. The fix: when authed_get's 401 path enters the refresh
    // lock with a non-empty `cached`, the double-check MUST evict-and-mint if
    // the cache still holds the same token we just got 401'd on. Without this
    // the followers (and the sole caller in the no-contention case) hand the
    // dead token back to the retry, see another 401, and surface
    // AuthRequired/AuthFailed even though a fresh mint would succeed.
    //
    // Flow this test exercises:
    //   1. First fetch_manifest: unauthed → 401 challenge → /token mint
    //      returns "token-A" → manifest with Bearer token-A → 200 (cache primed).
    //   2. Second fetch_manifest: pre-flight with cached token-A returns 401
    //      (server revoked) → enter refresh lock → double-check still holds
    //      token-A (which just failed) → MUST evict + mint → /token returns
    //      "token-B" → retry manifest with Bearer token-B → 200.
    // Expected: /token endpoint hit exactly twice. Without the fix the second
    // mint is skipped and the second fetch errors with AuthRequired/AuthFailed.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    // Bearer-success mocks must register BEFORE the catch-all 401 fallback so
    // wiremock prefers them (registration-order match).
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer token-B"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    // token-A succeeds for the FIRST manifest fetch (primes the cache) but is
    // rejected on the SECOND — `up_to_n_times(1)` on the 200 + a catch-all 401
    // for token-A simulates a token revoked mid-TTL.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer token-A"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer token-A"))
        .respond_with(ResponseTemplate::new(401).insert_header(
            "Www-Authenticate",
            challenge_header(&server, &scope).as_str(),
        ))
        .mount(&server)
        .await;

    // Initial unauthed 401 (consumed once, before any token has been minted).
    mount_challenge(&server, &scope).await;

    // /token: first mint returns token-A, second mint returns token-B.
    // Wiremock evaluates mocks in registration order, so the up_to_n_times(1)
    // for token-A must be registered FIRST.
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "token-A",
            "expires_in": 600,
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "token-B",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let client = GhcrClient::new_for_tests(http, Some(("u".into(), "p".into())));
    let image = image_for(&host);

    // First pull primes the cache with token-A.
    client
        .fetch_manifest(&image)
        .await
        .expect("first pull mints token-A and succeeds");

    // Second pull pre-flights with cached token-A, gets 401 (revoked), enters
    // refresh lock, double-checks the cache (still token-A — the one that just
    // failed), evicts, mints token-B, retries, succeeds. Without the fix this
    // second call errors with AuthRequired or AuthFailed.
    client
        .fetch_manifest(&image)
        .await
        .expect("second pull must mint a fresh token (not reuse the failed cached one)");

    let token_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .count();
    assert_eq!(
        token_hits, 2,
        "the 401 on the cached token MUST trigger a second mint (not reuse the failed token)"
    );
}

fn build_gzipped_tar(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    {
        let mut tar = Builder::new(&mut gz);
        for (path, data, mode) in entries {
            let mut header = Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            tar.append_data(&mut header, *path, &mut std::io::Cursor::new(*data))
                .unwrap();
        }
        tar.finish().unwrap();
    }
    gz.finish().unwrap()
}

fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

#[tokio::test]
async fn ghcr_full_extract_path_streams_and_verifies_layer_blob() {
    // Drives `OciExtractor`-equivalent full pull (manifest → cached layer
    // ensure with digest verification, gunzip + untar) against a GHCR mock.
    // The DockerHub e2e suite has this; GHCR must too (Fix 6) so the
    // `GhcrClient::fetch_layer_blob` codepath — including bearer re-use
    // across the manifest + blob fetches — is covered.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    let layer_bytes = build_gzipped_tar(&[("etc/marker", b"ghcr-extracted\n", 0o644)]);
    let layer_digest = sha256_digest(&layer_bytes);

    let manifest = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "size": 2,
        },
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": layer_digest,
                "size": layer_bytes.len(),
            }
        ]
    });

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "extract-token",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer extract-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/blobs/{}", REPO, layer_digest)))
        .and(header("Authorization", "Bearer extract-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "Content-Type",
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                )
                .set_body_bytes(layer_bytes.clone()),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let cache_dir = TempDir::new().unwrap();
    let extractor = OciExtractor::with_components(
        LayerCache::new(cache_dir.path().to_path_buf()),
        Box::new(GhcrClient::new_for_tests(
            http,
            Some(("u".into(), "p".into())),
        )),
    );

    let image = image_for(&host);
    let fetched = extractor
        .client_for_test()
        .fetch_manifest(&image)
        .await
        .expect("GHCR manifest fetched");
    assert_eq!(fetched.layers.len(), 1);
    assert_eq!(fetched.layers[0].digest, layer_digest);

    let layer_dir = extractor
        .cache_for_test()
        .ensure_layer(extractor.client_for_test(), &image, &fetched.layers[0])
        .await
        .expect("GHCR layer ensured into cache");
    assert!(
        layer_dir.join(".complete").is_file(),
        ".complete sentinel must be present after a successful ensure_layer"
    );
    let marker = std::fs::read_to_string(layer_dir.join("etc/marker"))
        .expect("layer payload survives gunzip+untar");
    assert_eq!(marker, "ghcr-extracted\n");

    // Second ensure_layer must hit the cache (no second blob fetch).
    let _ = extractor
        .cache_for_test()
        .ensure_layer(extractor.client_for_test(), &image, &fetched.layers[0])
        .await
        .expect("second ensure_layer is a cache hit");

    let token_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/token")
        .count();
    assert_eq!(
        token_hits, 1,
        "the bearer minted for the manifest must be reused for the layer blob (D17 single-token reuse)"
    );

    let blob_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| {
            r.url
                .path()
                .starts_with(&format!("/v2/{}/blobs/{}", REPO, layer_digest))
        })
        .count();
    assert_eq!(
        blob_hits, 1,
        "blob endpoint must be hit exactly once: first call streams, second is a cache hit"
    );
}
