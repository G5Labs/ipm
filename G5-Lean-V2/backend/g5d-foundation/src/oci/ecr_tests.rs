//! ECR auth-flow tests against a mocked AWS API + `*.dkr.ecr.*` registry
//! (D25).
//!
//! Exercises the ECR Basic-auth flow: a SigV4-signed `GetAuthorizationToken`
//! against a mock AWS endpoint yields a base64 `AWS:<password>` applied as
//! `Authorization: Basic <token>` on the (pre-authenticated) `/v2/` request.
//! Covers the happy path, 401-driven refresh, cache-expiry re-mint, and the
//! malformed-token decode-error path — without any real AWS call. AWS
//! credentials are injected via `EcrClient::new_for_tests`.

use base64::Engine as _;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::ecr::AwsCredentials;
use super::error::OciError;
use super::registry::{EcrClient, ImageRef, Reference, RegistryClient};

const REPO: &str = "myteam/myimg";
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

fn test_creds() -> AwsCredentials {
    AwsCredentials {
        access_key_id: "AKIDEXAMPLE".to_string(),
        secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_string(),
        session_token: None,
        // The mock host is not an ECR host, so the region must be supplied;
        // the mock does not verify the signature, only its presence.
        region: Some("us-east-1".to_string()),
    }
}

/// base64("AWS:<password>") — both the wire token and the Basic credential.
fn auth_token(password: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!("AWS:{}", password))
}

fn client(server: &MockServer) -> EcrClient {
    EcrClient::new_for_tests(
        reqwest::Client::new(),
        Some(test_creds()),
        Some(server.uri()),
        None,
    )
}

#[tokio::test]
async fn ecr_get_authorization_token_flow_succeeds() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let token = auth_token("password-123");
    let expected_basic = format!("Basic {}", token);

    // GetAuthorizationToken — POST to the AWS API root, matched by target.
    Mock::given(method("POST"))
        .and(path("/"))
        .and(header(
            "X-Amz-Target",
            "AmazonEC2ContainerRegistry_V20150921.GetAuthorizationToken",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authorizationData": [{
                "authorizationToken": token,
                "expiresAt": 9_999_999_999u64,
            }]
        })))
        .mount(&server)
        .await;

    // The /v2/ request is pre-authenticated with the Basic credential.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", expected_basic.as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    let manifest = client(&server)
        .fetch_manifest(&image_for(&host))
        .await
        .expect("ECR manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    // The GetAuthorizationToken POST must carry a SigV4 Authorization header.
    // Only GetAuthorizationToken targets path "/" (the /v2/ manifest GET has a
    // distinct path), so the path filter uniquely selects it.
    let api_reqs: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/")
        .collect();
    assert_eq!(api_reqs.len(), 1);
    let auth = api_reqs[0]
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        auth.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/"),
        "GetAuthorizationToken must be SigV4-signed, got: {auth}"
    );
}

#[tokio::test]
async fn ecr_401_triggers_token_refresh() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let token_a = auth_token("pw-A");
    let token_b = auth_token("pw-B");

    // GetAuthorizationToken: first call returns token-A, second token-B.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authorizationData": [{ "authorizationToken": token_a, "expiresAt": 9_999_999_999u64 }]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authorizationData": [{ "authorizationToken": token_b, "expiresAt": 9_999_999_999u64 }]
        })))
        .mount(&server)
        .await;

    // token-B succeeds (register first); token-A is rejected with 401.
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header(
            "Authorization",
            format!("Basic {}", token_b).as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header(
            "Authorization",
            format!("Basic {}", token_a).as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(401).insert_header("Www-Authenticate", "Basic realm=\"ecr\""),
        )
        .mount(&server)
        .await;

    let manifest = client(&server)
        .fetch_manifest(&image_for(&host))
        .await
        .expect("ECR refresh-on-401 manifest fetch succeeds");
    assert_eq!(manifest.layers.len(), 1);

    let api_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/")
        .count();
    assert_eq!(
        api_hits, 2,
        "a 401 on the cached Basic token must trigger a fresh GetAuthorizationToken"
    );
}

#[tokio::test]
async fn ecr_expired_token_refetched() {
    let server = MockServer::start().await;
    let host = host_of(&server);
    let scope = format!("repository:{}:pull", REPO);
    let token = auth_token("pw-stable");

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authorizationData": [{ "authorizationToken": token, "expiresAt": 9_999_999_999u64 }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/v2/{}/manifests/{}", REPO, TAG)))
        .and(header("Authorization", format!("Basic {}", token).as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/vnd.oci.image.manifest.v1+json")
                .set_body_json(manifest_body()),
        )
        .mount(&server)
        .await;

    let client = client(&server);
    let image = image_for(&host);
    client.fetch_manifest(&image).await.expect("first pull");

    // Force the cached Basic token past its TTL; the second pull must re-mint.
    client
        .token_cache()
        .force_insert_expired(host.clone(), scope.clone(), token.clone());

    client.fetch_manifest(&image).await.expect("second pull");

    let api_hits = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == "/")
        .count();
    assert_eq!(
        api_hits, 2,
        "an expired cached Basic token must trigger a fresh GetAuthorizationToken"
    );
}

#[tokio::test]
async fn ecr_malformed_authorization_token_is_auth_failed() {
    let server = MockServer::start().await;
    let host = host_of(&server);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "authorizationData": [{ "authorizationToken": "!!! not base64 !!!" }]
        })))
        .mount(&server)
        .await;

    let err = client(&server)
        .fetch_manifest(&image_for(&host))
        .await
        .expect_err("a malformed authorizationToken must error");
    assert!(matches!(err, OciError::AuthFailed(_, _)), "got {err:?}");
}
