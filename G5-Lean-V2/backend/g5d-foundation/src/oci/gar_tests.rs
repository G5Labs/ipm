//! GAR auth-flow tests against a mocked `*-docker.pkg.dev` registry (D23).
//!
//! Exercises the GAR bearer flow (401 challenge → token endpoint → authed
//! manifest) for the anonymous, injected-credential, and GCE-metadata-server
//! paths, plus the private-image `AuthRequired` mapping, without ever touching
//! real GCP. The metadata server is itself a `wiremock` mock (overridden base
//! URL); ambient sources (SA-key env, `gcloud`) are disabled in
//! `GarCredentials::for_tests`.

use std::io::Write as _;

use base64::Engine as _;
use serde_json::json;
use serial_test::serial;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::error::OciError;
use super::registry::{GarClient, ImageRef, Reference, RegistryClient};

/// Test-only PKCS#8 RSA 2048-bit private key used to sign the SA JWT-bearer
/// assertion. Generated for this test alone — no production system trusts it.
///
/// SECURITY: test-only key, do NOT add to any allowlist or production
/// credential path. Distinct from every production SA key by construction
/// (locally generated for this unit test, never deployed). Any secrets
/// scanner can grep this anchor to confirm the literal is intentional.
const TEST_SA_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQDPVu3N2csOFmdY
yuisAFQqiwwSFInCcUfybFf75erNFf4KZ6e0SVhRorkE+3WqMLOieoGSnQD2dVAa
mSNHy73xisuGiGmzE1kM2njgo1e3D02n1SkNiI7TNImSYsi//Hy6nzF0brnq1sUE
DWg+LAPQ2daVlSYK0VBJ2e4u17n9SQWh9SI6xldgqYP6kWplgS0gn4d2p/7oZM1R
k7xmttrhv+6P2Qo46fzNRrB596390bvv0dhq88GLL2fJ76xK/ac32lqDKTIaSXGO
QTy4UBQWbOTy1p0iuhBr5CzAWAGj3ARzAvN981suRGDUVgoYqalenjX95LVMe2aQ
LUKyQKZvAgMBAAECggEADltcmBZuowkWGuXh+pcDPPbn3RCjAV3Kt5Sc+wpOmejd
8GaBDFDP1sdHvHqVCTXaGnQVKMvKV0aER3M3KC+JdYJA2Y30k7vRPLycBiIJXtmm
bOqrrw43pZRLumEwuWqIUHvZdtB7+zDn74HmJ/fwQ7Px0eXD/pfD0XlPrtqtPEaE
GwLaK3os7z/2wGPYlBi/5ZFyeKVd2q8yAsFu0sGwsZf+j331QZ0V13lRz1RcXHfT
QcC5zt3WdYjqRS5JDnAfFbJs2yuEihA/Al5vb/efjP4Zp3KQZSLgbrptbRtFyBSU
xceHxK3CKdqiIcHClKvXcUBK/QZxwlo+/gHBl51YMQKBgQD9KirhSgo6xrglUMh8
PcDO1EyBY9Am/OB7PJ50v3EOMUsQVPzc6MvtXLE0i63vTXYsisrqWgc9oaYwCXha
s92Ho6jNr3r0vYpdLcpV7+zuFY6iFtrHV/ATp8LDWtVGKU3pYjNXUX1cETGV4ygO
D2had2esWFs0VxXpNPoPhgU1DQKBgQDRqWEIvCOMjmJOvCRDkSr9Bmjo2L9VCkJH
XcL70wURqnAe6FsQHY1zCGGDHXB2GHwRuznznhaGNELwVAKIC0TRg6LLlJcggLF6
5mtKnDZ/Npe1g9rmKy7Dsu6+mxAqMlOdPtOVnjcbANJ7t4g5uiQUgtBOACrpgJ9L
BaLrVrbiawKBgFlHraeVvNXNjtNaEdn+zSgjpndbA+U7+M59KkRx3yTqLVatIh+l
2YTz02p8d/KUlsBOkhDjfvo1L65xKIiUNePHy5f98pas4UbLBzgXTK8J2ys+dBko
4fmMiBrsxZngpyFPCFwa2OLHDX2WhmGu/VYqFd2pq0BiFdAQZr9ivCNhAoGAWnFV
sX3vW9/EGrDgd+E0iSfts5xklRuy0bRpu1YhVqiOa1bSiZYmAZx47CWT8O3I/6Ej
c6sbjlMXGTM0UFmEG1lMrcNwmg3KAflKCUpv29rCAQRbNnUkXDlg8gpUI8/H8Ao9
Hcq9fMGTCtl2Q2i/zbVMhIlf+ZCJpsRMrnaet/cCgYB6T3cHEwdtVzf9Z8baXYzq
QYeNZ/N3zw4XSSo5+X11hj7AAUBlMgwCoqIay9/zpWWe5PshHLwmJ6Qz4rr3bl8W
94RjwvaeIUwSnktiW/GVhp9fm1VvxilQ2kwA+e3VwHnCaLLafwab4xrIEDXqny0b
xVmaYMfMpPsQmr3shlzAqw==
-----END PRIVATE KEY-----";

/// Restores `GOOGLE_APPLICATION_CREDENTIALS` on drop so SA-JSON tests do not
/// leak the tmp path into the rest of the suite.
struct EnvGuard {
    key: &'static str,
    prior: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let prior = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, prior }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

const REPO: &str = "proj/repo/img";
const TAG: &str = "1.0";

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
    format!(
        r#"Bearer realm="{}/token",service="gar",scope="{}""#,
        server.uri(),
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

#[tokio::test]
async fn gar_anonymous_flow_succeeds() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    // Anonymous token endpoint — no Authorization required.
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "anon-gar-token",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer anon-gar-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    // No injected creds; metadata probe is disabled (allow_metadata=false),
    // SA is off (allow_sa=false), and gcloud is always off under for_tests —
    // only anonymous remains, which is the path this test exercises.
    let client = GarClient::new_for_tests(
        reqwest::Client::new(),
        None,
        Some(format!("{}/no-metadata-here", server.uri())),
        false,
        false,
    );
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("anonymous GAR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

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
async fn gar_injected_credentials_send_basic_auth() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    let user = "oauth2accesstoken";
    let token = "gcp-injected-token";
    let expected_basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", user, token))
    );

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .and(header("Authorization", expected_basic.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "gar-bearer",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer gar-bearer"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    // Injected credentials short-circuit before any ambient tier — leave
    // metadata and SA gated off so the resolver path is unambiguous.
    let client = GarClient::new_for_tests(
        reqwest::Client::new(),
        Some((user.to_string(), token.to_string())),
        None,
        false,
        false,
    );
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("injected GAR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);
}

#[tokio::test]
async fn gar_metadata_server_flow_succeeds() {
    // The production worker-host path: no injected creds, resolve a GCP access
    // token from the GCE metadata server (mocked), then present it as the
    // Basic password to the GAR token endpoint.
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    let metadata_token = "metadata-gcp-token";
    let expected_basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("oauth2accesstoken:{}", metadata_token))
    );

    // Mock metadata endpoint — only matches with the required header.
    Mock::given(method("GET"))
        .and(path(
            "/computeMetadata/v1/instance/service-accounts/default/token",
        ))
        .and(header("Metadata-Flavor", "Google"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": metadata_token,
            "expires_in": 3600,
            "token_type": "Bearer",
        })))
        .mount(&server)
        .await;

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .and(header("Authorization", expected_basic.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "gar-bearer-via-metadata",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer gar-bearer-via-metadata"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    let metadata_url = format!(
        "{}/computeMetadata/v1/instance/service-accounts/default/token",
        server.uri()
    );
    // Opt the metadata probe in (allow_metadata=true) so the mock is hit;
    // SA-JSON and gcloud stay disabled.
    let client = GarClient::new_for_tests(
        reqwest::Client::new(),
        None,
        Some(metadata_url),
        true,
        false,
    );
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("metadata-server GAR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    let metadata_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path().ends_with("/default/token"))
        .count();
    assert_eq!(metadata_hits, 1, "metadata server must be consulted once");
}

#[tokio::test]
async fn gar_private_image_without_credentials_is_auth_required() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "anon-gar-token",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;

    // Private image: even with the anonymous token the manifest stays 401.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer anon-gar-token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    // No creds resolvable: metadata gated off, SA off, gcloud always off
    // under for_tests, no injected pair → resolve_basic_creds returns None
    // → anonymous token → server 401 → AuthRequired. metadata_url is `None`
    // because `allow_metadata=false` makes the URL irrelevant.
    let client = GarClient::new_for_tests(reqwest::Client::new(), None, None, false, false);
    let err = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("private GAR image without creds must error");
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

/// SA-JSON (D22 tier 3): the self-hosted/local-dev path. With
/// `GOOGLE_APPLICATION_CREDENTIALS` pointing at a real SA key JSON whose
/// `token_uri` is rewritten to a wiremock endpoint, the provider must
/// (a) read the file off disk via `tokio::fs`, (b) sign an RFC 7523 JWT-bearer
/// assertion with the SA private key, (c) POST it as
/// `grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer`, (d) receive an
/// access_token, and (e) present it as the `oauth2accesstoken:<token>` Basic
/// pair to the GAR token endpoint. Metadata + gcloud stay disabled so the
/// SA-JSON tier is the unambiguous resolver. `#[serial]` because the env-var
/// mutation is process-global. Closes codex round-3 important: no SA-JSON
/// success path was previously asserted.
#[tokio::test]
#[serial]
async fn gar_service_account_json_acquires_token() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);

    let sa_access_token = "sa-jwt-bearer-access-token";
    let expected_basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode(format!("oauth2accesstoken:{}", sa_access_token))
    );

    // Mock the Google OAuth2 token endpoint (the SA assertion's audience).
    // Assert the grant_type is the RFC 7523 JWT-bearer string so we know
    // we are exercising the SA path, not the metadata-server path.
    let token_uri_path = "/oauth2/token";
    Mock::given(method("POST"))
        .and(path(token_uri_path))
        .and(body_string_contains(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer",
        ))
        .and(body_string_contains("assertion="))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": sa_access_token,
            "expires_in": 3600,
            "token_type": "Bearer",
        })))
        .mount(&server)
        .await;

    mount_challenge(&server, &scope).await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .and(header("Authorization", expected_basic.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "gar-bearer-via-sa",
            "expires_in": 600,
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", "Bearer gar-bearer-via-sa"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    // Write a real SA JSON to a tmp file. The `token_uri` is rewritten to point
    // at the mock so the JWT-bearer exchange never leaves the test.
    let sa_json = serde_json::to_string(&json!({
        "type": "service_account",
        "client_email": "test-sa@test-project.iam.gserviceaccount.com",
        "private_key": TEST_SA_PRIVATE_KEY_PEM,
        "token_uri": format!("{}{}", server.uri(), token_uri_path),
    }))
    .unwrap();
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(sa_json.as_bytes()).unwrap();
    let _guard = EnvGuard::set(
        "GOOGLE_APPLICATION_CREDENTIALS",
        tmp.path().to_str().unwrap(),
    );

    // Force-disable metadata (would otherwise be tier 2 ahead of SA) so the
    // SA-JSON tier is the unambiguous resolver. `gcloud` is always off under
    // `for_tests`.
    let client = GarClient::new_for_tests(
        reqwest::Client::new(),
        None,
        Some(format!("{}/no-metadata-here", server.uri())),
        false,
        true,
    );
    let manifest = client
        .fetch_manifest(&image_for(&host))
        .await
        .expect("SA-JSON GAR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    let sa_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == token_uri_path)
        .count();
    assert_eq!(
        sa_hits, 1,
        "SA JWT-bearer token endpoint must be POSTed exactly once"
    );
}
