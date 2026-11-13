//! ACR auth-flow tests against a mocked `*.azurecr.io` registry (D27).
//!
//! Exercises the ACR bearer flow (401 challenge → AAD acquisition →
//! `oauth2/exchange` → `oauth2/token` → authed manifest) for the injected-AAD
//! and service-principal paths, plus the persistent-401 and malformed-challenge
//! error mappings — without ever touching real Azure. All Azure endpoints
//! (AAD authority + the ACR oauth2 dance) are `wiremock` mocks; the `az`
//! shell-out is disabled in `AcrCredentials::for_tests`.

use serde_json::json;
use serial_test::serial;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::error::OciError;
use super::registry::{AcrClient, ImageRef, Reference, RegistryClient};

const REPO: &str = "team/img";
const TAG: &str = "1.0";
const SERVICE: &str = "myreg.azurecr.io";

/// Restores environment variables on drop (service-principal path only).
struct EnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(pairs: &[(&'static str, &str)]) -> Self {
        let saved = pairs
            .iter()
            .map(|(k, _)| (*k, std::env::var(k).ok()))
            .collect::<Vec<_>>();
        for (k, v) in pairs {
            std::env::set_var(k, v);
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
    // ACR realm is `<base>/oauth2/token`; the provider derives the exchange +
    // token endpoints from it.
    format!(
        r#"Bearer realm="{}/oauth2/token",service="{}",scope="{}""#,
        server.uri(),
        SERVICE,
        scope
    )
}

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

/// Mount the ACR oauth2 dance: `oauth2/exchange` (AAD → refresh token) then
/// `oauth2/token` (refresh token → access token). Asserts the inbound bodies.
async fn mount_oauth2_dance(server: &MockServer, aad_token: &str) {
    Mock::given(method("POST"))
        .and(path("/oauth2/exchange"))
        .and(body_string_contains("grant_type=access_token"))
        .and(body_string_contains(format!("access_token={aad_token}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "refresh_token": "acr-refresh-token"
        })))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=acr-refresh-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "acr-access-token",
            "expires_in": 300,
        })))
        .mount(server)
        .await;
}

async fn mount_authed_manifest(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer acr-access-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn acr_injected_aad_runs_oauth2_dance() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;
    mount_oauth2_dance(&server, "aad-token-xyz").await;
    mount_authed_manifest(&server).await;

    // Injected AAD short-circuits ambient; explicitly off so the resolver
    // path is unambiguous (no env / IMDS / az probe).
    let client = AcrClient::new_for_tests(
        reqwest::Client::new(),
        Some("aad-token-xyz".to_string()),
        None,
        None,
        false,
    );
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("ACR injected-AAD manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    // Both legs of the dance must have been hit exactly once.
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(
        reqs.iter()
            .filter(|r| r.url.path() == "/oauth2/exchange")
            .count(),
        1
    );
    assert_eq!(
        reqs.iter()
            .filter(|r| r.url.path() == "/oauth2/token")
            .count(),
        1
    );
}

#[tokio::test]
#[serial]
async fn acr_service_principal_acquires_aad_then_dances() {
    let _guard = EnvGuard::set(&[
        ("AZURE_TENANT_ID", "tenant-123"),
        ("AZURE_CLIENT_ID", "client-abc"),
        ("AZURE_CLIENT_SECRET", "secret-shh"),
    ]);

    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    // Service-principal client-credentials grant against the (mocked) AAD
    // authority, keyed by the tenant id.
    Mock::given(method("POST"))
        .and(path("/tenant-123/oauth2/v2.0/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("client_id=client-abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "sp-aad-token",
            "expires_in": 3600,
        })))
        .mount(&server)
        .await;

    mount_challenge(&server, &scope).await;
    mount_oauth2_dance(&server, "sp-aad-token").await;
    mount_authed_manifest(&server).await;

    // No injected AAD; the authority base points at the mock so the SP grant
    // resolves first. ambient=true opts the SP+IMDS cascade in (the SP mock
    // returns before IMDS is consulted); the `az` shell-out is hard-disabled
    // under tests because `for_tests` forces `allow_az_shellout=false`
    // regardless of `ambient`, so the developer's authenticated `az` can
    // never be reached. `serial_test` keeps the env-var mutation race-free.
    let client =
        AcrClient::new_for_tests(reqwest::Client::new(), None, Some(server.uri()), None, true);
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("ACR service-principal manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);
}

#[tokio::test]
async fn acr_persistent_401_after_dance_is_auth_required() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;
    mount_oauth2_dance(&server, "aad-token-xyz").await;

    // The bearer minted by the dance is still rejected by /v2/ (e.g. the
    // identity lacks pull on this repo).
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer acr-access-token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = AcrClient::new_for_tests(
        reqwest::Client::new(),
        Some("aad-token-xyz".to_string()),
        None,
        None,
        false,
    );
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("persistent 401 after the dance must error");
    match err {
        OciError::AuthRequired {
            registry, image, ..
        } => {
            assert_eq!(registry, host);
            assert_eq!(image, REPO);
        }
        other => panic!("expected AuthRequired, got {other:?}"),
    }
}

#[tokio::test]
async fn acr_no_credentials_resolves_to_auth_required() {
    // No injected AAD and ambient cascade disabled → `resolve_aad` returns
    // None → `AcrCredentials::mint` MUST surface `AuthRequired` rather
    // than run the oauth2 dance with an empty AAD token (which would 401 at
    // `oauth2/exchange` and report a misleading `AuthFailed`).
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    let client = AcrClient::new_for_tests(reqwest::Client::new(), None, None, None, false);
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("no AAD creds + private image must error");
    match err {
        OciError::AuthRequired {
            registry,
            image,
            scope: s,
        } => {
            assert_eq!(registry, host);
            assert_eq!(image, REPO);
            assert_eq!(s, scope);
        }
        other => panic!("expected AuthRequired, got {other:?}"),
    }
}

#[tokio::test]
async fn acr_malformed_challenge_errors() {
    let server = MockServer::start().await;
    let host = host_of(&server);

    // 401 with a non-Bearer (malformed) Www-Authenticate header.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .respond_with(
            ResponseTemplate::new(401).insert_header("Www-Authenticate", "Basic realm=\"acr\""),
        )
        .mount(&server)
        .await;

    let client = AcrClient::new_for_tests(
        reqwest::Client::new(),
        Some("aad-token-xyz".to_string()),
        None,
        None,
        false,
    );
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("malformed challenge must error");
    assert!(matches!(err, OciError::AuthFailed(_, _)), "got {err:?}");
}
