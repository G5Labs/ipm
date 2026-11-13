//! Integration tests for the PR2 worker-host runtime-label refresh endpoint
//! (`POST /api/v1/orgs/:orgId/worker-hosts/:hostId/runtime-labels/refresh`).
//!
//! These run against a real Postgres (via `TestApp`) AND a `wiremock` registry
//! (no live network) injected through the new `spawn_with_runtime_resolver`
//! helper. They exercise the full org-scoped authz/RLS path, the anonymous OCI
//! Distribution v2 fetch (single manifest, image-index → amd64, bearer
//! challenge), persistence of schema+labels+refreshed_at, and the 422-vs-503
//! error contract. Contract:
//! `generation/.semlift/specs/g5/g5-api/worker_hosts.semlift.md` [D43].

use crate::helpers;

use std::sync::Arc;

use helpers::{TestApp, TestOrg, TestUser};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use g5_api::services::runtime_label_resolver::OciRuntimeLabelResolver;
use g5_auth::{JwtValidator, Scope};

// ─── Fixtures ────────────────────────────────────────────────────────────────

const SIXTY_FOUR_HEX: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A digest-pinned base image ref pointing at the wiremock registry on `port`.
/// `127.0.0.1:{port}/acme/app@sha256:{64hex}` passes `validate_base_image_ref`
/// (has a `/`, valid sha256:64hex) and the resolver host gate (`:` present).
fn base_image_ref(port: u16) -> String {
    format!("127.0.0.1:{port}/acme/app@sha256:{SIXTY_FOUR_HEX}")
}

/// A realistic image config blob carrying `.config.Labels`.
fn config_blob_with_labels(labels: Value) -> Value {
    json!({
        "architecture": "amd64",
        "os": "linux",
        "config": { "Labels": labels }
    })
}

/// Provision a host with the given base image ref and return its host_id.
async fn provision_host_with_image(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    base_ref: &str,
) -> Uuid {
    let resp = client
        .post_json(
            &format!("/api/v1/orgs/{org_id}/worker-hosts"),
            &json!({ "region": "us-central1", "baseImageRef": base_ref }),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "provision should return 201 Created"
    );
    let body: Value = resp.json().await.expect("provision response is JSON");
    let host_id = body["data"]["hostId"]
        .as_str()
        .expect("hostId present in provision response");
    Uuid::parse_str(host_id).expect("hostId is a UUID")
}

/// Seed a truthful legacy host without image provenance. The production
/// provision endpoint now rejects this shape before insertion.
async fn provision_host_no_image(app: &TestApp, org_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO worker_hosts (org_id, region, instance_type) \
         VALUES ($1, 'us-central1', 'c3-standard-4') RETURNING host_id",
    )
    .bind(org_id)
    .fetch_one(&app.db)
    .await
    .expect("seed legacy host without image source")
}

/// Build a `TestApp` whose runtime-label resolver fetches over `http` (so it
/// reaches the wiremock registry at the `127.0.0.1:PORT` host embedded in each
/// host's `base_image_ref`).
async fn spawn_app_for_registry() -> TestApp {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("no-redirect registry test client builds");
    let resolver = Arc::new(OciRuntimeLabelResolver::for_tests(client));
    TestApp::spawn_with_runtime_resolver(resolver).await
}

/// Like `spawn_app_for_registry`, but the resolver's HTTP client carries a short
/// request timeout so a delayed registry deterministically trips the timeout
/// path (→ 503) without slowing the suite.
async fn spawn_app_for_registry_with_timeout(timeout: std::time::Duration) -> TestApp {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .expect("short-timeout test client builds");
    let resolver = Arc::new(OciRuntimeLabelResolver::for_tests(client));
    TestApp::spawn_with_runtime_resolver(resolver).await
}

/// Extract the wiremock listener port.
fn mock_port(mock: &MockServer) -> u16 {
    mock.address().port()
}

// ─── Success paths ───────────────────────────────────────────────────────────

#[tokio::test]
async fn refresh_persists_schema_and_labels_single_manifest() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);

    // Single OCI image manifest with a config descriptor.
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfg111" }
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfg111"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.schema": "v1",
                "com.g5labs.runtime.base_image": "ubuntu@sha256:dead",
                "com.g5labs.runtime.workload.class": "build",
                "com.g5labs.runtime.unknown": "drop",
                "org.opencontainers.image.title": "drop-too"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK, "refresh should be 200 OK");
    let body: Value = resp.json().await.expect("refresh response is JSON");
    let data = &body["data"];
    assert_eq!(data["runtimeSchema"], "v1");
    let labels = data["runtimeLabels"].as_object().expect("labels object");
    assert_eq!(labels.len(), 2, "only allowlisted G5 keys stored");
    assert_eq!(
        labels["com.g5labs.runtime.base_image"],
        "ubuntu@sha256:dead"
    );
    assert_eq!(labels["com.g5labs.runtime.workload.class"], "build");
    assert!(!labels.contains_key("com.g5labs.runtime.schema"));
    assert!(!labels.contains_key("com.g5labs.runtime.unknown"));
    assert!(!labels.contains_key("org.opencontainers.image.title"));
    assert!(
        data.get("runtimeLabelsRefreshedAt").is_some(),
        "refreshed_at set"
    );

    // Persistence verified via a follow-up GET list.
    let list = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Value = list.json().await.unwrap();
    let host = list_body["data"]["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["hostId"] == host_id.to_string())
        .expect("provisioned host present in list");
    assert_eq!(host["runtimeSchema"], "v1");
    assert_eq!(
        host["runtimeLabels"]["com.g5labs.runtime.base_image"],
        "ubuntu@sha256:dead"
    );
    assert!(host.get("runtimeLabelsRefreshedAt").is_some());

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_schema_absent_persists_null_and_empty_labels() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);

    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfg-no-schema" }
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfg-no-schema"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.workload.class": "build",
                "com.g5labs.runtime.isolation": "vm",
                "org.opencontainers.image.title": "drop-too"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert!(body["data"]["runtimeSchema"].is_null());
    assert_eq!(body["data"]["runtimeLabels"], json!({}));

    // Persisted state is also NULL schema + empty labels, with a refresh stamp.
    let list = client
        .get(&format!("/api/v1/orgs/{}/worker-hosts", org.id))
        .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: Value = list.json().await.unwrap();
    let host = list_body["data"]["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["hostId"] == host_id.to_string())
        .expect("provisioned host present in list");
    assert!(host["runtimeSchema"].is_null());
    assert_eq!(host["runtimeLabels"], json!({}));
    assert!(host.get("runtimeLabelsRefreshedAt").is_some());

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_image_index_selects_amd64() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);

    // Index → re-GET amd64 child → config blob.
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                { "digest": "sha256:arm", "platform": { "os": "linux", "architecture": "arm64" } },
                { "digest": "sha256:amd", "platform": { "os": "linux", "architecture": "amd64" } }
            ]
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/manifests/sha256:amd"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfgamd" }
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfgamd"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.schema": "v1",
                "com.g5labs.runtime.isolation": "strong"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["runtimeSchema"], "v1");
    assert_eq!(
        body["data"]["runtimeLabels"]["com.g5labs.runtime.isolation"],
        "strong"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_retries_with_anonymous_bearer_token() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    let realm = format!("http://127.0.0.1:{port}/token");

    // wiremock evaluates mounts in registration order; the first full match
    // wins. Mount the WITH-bearer 200 manifest FIRST (more specific) so a
    // request carrying the anonymous token matches it…
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer anon-tkn",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfgtok" }
        })))
        .mount(&mock)
        .await;
    // …then the no-auth manifest GET → 401 challenge (matches only when the
    // first, bearer-requiring mock did not, i.e. the initial unauthenticated GET).
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(
            ResponseTemplate::new(401).insert_header(
                "WWW-Authenticate",
                format!(
                    r#"Bearer realm="{realm}",service="127.0.0.1",scope="repository:acme/app:pull""#
                )
                .as_str(),
            ),
        )
        .mount(&mock)
        .await;
    // Token endpoint (anonymous) → token.
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "anon-tkn" })))
        .mount(&mock)
        .await;
    // Config blob (also presents the bearer; respond regardless).
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfgtok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.schema": "v1"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "bearer challenge should be retried with the anonymous token"
    );
    assert_eq!(
        resp.json::<Value>().await.unwrap()["data"]["runtimeSchema"],
        "v1"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_token_endpoint_401_is_503_and_db_untouched() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    let realm = format!("http://127.0.0.1:{port}/token");

    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(
            ResponseTemplate::new(401).insert_header(
                "WWW-Authenticate",
                format!(
                    r#"Bearer realm="{realm}",service="127.0.0.1",scope="repository:acme/app:pull""#
                )
                .as_str(),
            ),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        refreshed_at.is_none(),
        "DB untouched when anonymous token endpoint rejects the exchange"
    );

    app.cleanup().await;
}

// ─── Validation (422) and registry-failure (503) paths ───────────────────────

#[tokio::test]
async fn refresh_without_base_image_ref_is_422() {
    // No registry is hit: the missing base_image_ref short-circuits to a 422
    // before the resolver runs.
    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_no_image(&app, org.id).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(body["error"]["details"]["code"], "no_base_image_ref");

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_private_registry_host_is_422_and_db_untouched() {
    let app =
        TestApp::spawn_with_runtime_resolver(Arc::new(OciRuntimeLabelResolver::https())).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let private_ref = format!("127.0.0.1:5000/acme/app@sha256:{SIXTY_FOUR_HEX}");
    let host_id = provision_host_with_image(&client, org.id, &private_ref).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    assert_eq!(
        body["error"]["details"]["code"],
        "unqualified_base_image_ref"
    );

    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        refreshed_at.is_none(),
        "DB untouched when production resolver rejects private registry host"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_unknown_schema_is_422_and_db_untouched() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfgbad" }
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfgbad"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.schema": "v99"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"]["details"]["code"],
        "unsupported_runtime_schema"
    );

    // DB NOT updated: refreshed_at stays NULL.
    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        refreshed_at.is_none(),
        "refreshed_at must stay NULL on a validation failure"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_registry_404_is_503() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");

    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(refreshed_at.is_none(), "DB untouched on registry failure");

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_malformed_manifest_json_is_503_and_db_untouched() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not-json"))
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        refreshed_at.is_none(),
        "DB untouched when manifest JSON is malformed"
    );

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_registry_timeout_is_503() {
    // A registry that stalls past the client timeout trips the resolver's bounded
    // timeout → transport/timeout failure → 503, leaving the DB untouched. This
    // guards the safety-critical bound: removing the timeout would hang the test.
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(3))
                .set_body_json(json!({
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "config": { "digest": "sha256:cfg" }
                })),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry_with_timeout(std::time::Duration::from_millis(300)).await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(refreshed_at.is_none(), "DB untouched on registry timeout");

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_oversized_config_is_503_response_too_large() {
    // A config blob larger than the 1 MiB streaming cap is rejected as an
    // oversized NETWORK response → 503 (network cap, per the resolver's mapping).
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfgbig" }
        })))
        .mount(&mock)
        .await;
    let huge = "x".repeat(2 * 1024 * 1024);
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfgbig"))
        .respond_with(ResponseTemplate::new(200).set_body_string(huge))
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    app.cleanup().await;
}

// ─── Tenancy / auth matrix ───────────────────────────────────────────────────

#[tokio::test]
async fn refresh_cross_org_host_is_404() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfg" }
        })))
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;

    // Org A owns the host.
    let mut owner_a = TestUser::create(&app).await;
    let org_a = TestOrg::create_with_admin(&app, &mut owner_a).await;
    let client_a = app.authenticated_client(&owner_a);
    let host_id = provision_host_with_image(&client_a, org_a.id, &base_image_ref(port)).await;

    // Org B's owner tries to refresh org A's host via org B's path → 404 (RLS).
    let mut owner_b = TestUser::create(&app).await;
    let org_b = TestOrg::create_with_admin(&app, &mut owner_b).await;
    let client_b = app.authenticated_client(&owner_b);

    let resp = client_b
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org_b.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_non_member_is_403() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    let app = spawn_app_for_registry().await;

    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);
    let host_id = provision_host_with_image(&owner_client, org.id, &base_image_ref(port)).await;

    // A different user who is NOT a member of the org.
    let outsider = TestUser::create(&app).await;
    let outsider_client = app.authenticated_client(&outsider);

    let resp = outsider_client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_member_without_workers_admin_is_403() {
    // A user who IS a member of the org but lacks `workers:admin` is rejected
    // by `scoped_db_for_org(.., &["workers:admin"])` before any registry call.
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    let app = spawn_app_for_registry().await;

    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);
    let host_id = provision_host_with_image(&owner_client, org.id, &base_image_ref(port)).await;

    // Member of the same org, but only read-tier scopes (no workers:admin).
    let member =
        TestUser::create_with_scopes(&app, org.id, &[Scope::OrgRead, Scope::WorkersRead]).await;
    let member_client = app.authenticated_client(&member);

    let resp = member_client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_without_jwt_is_401() {
    // Auth is rejected at the middleware before any registry call.
    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let host_id = Uuid::new_v4();

    let resp = app
        .client
        .post(format!(
            "{}/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
            app.base_url, org.id
        ))
        .json(&json!({}))
        .send()
        .await
        .expect("request sent");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

#[tokio::test]
async fn refresh_with_host_jwt_is_401() {
    // A host_jwt (Scope::WorkerHost) on this user-facing v1 surface is rejected
    // at the middleware before any registry call.
    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;

    let minter = helpers::test_jwt_minter();
    let host_id = Uuid::new_v4();
    let host_jwt = minter
        .mint(host_id, org.id, vec![Scope::WorkerHost], 3600)
        .expect("mint host jwt");
    // Sanity: the minted token validates and carries Scope::WorkerHost.
    let validator = JwtValidator::new(helpers::TEST_PUBLIC_KEY_PEM, None, None).unwrap();
    assert_eq!(
        validator.validate(&host_jwt).unwrap().scopes,
        vec![Scope::WorkerHost]
    );

    let resp = app
        .client
        .post(format!(
            "{}/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
            app.base_url, org.id
        ))
        .header("Authorization", format!("Bearer {host_jwt}"))
        .json(&json!({}))
        .send()
        .await
        .expect("request sent");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    app.cleanup().await;
}

// ─── Guest glibc ABI floor guard (provisioning_api D57 / microvm D12) ────────

/// A label that POSITIVELY declares a sub-floor guest glibc is rejected 422
/// with the dedicated code, and nothing is persisted (fail-closed at the
/// refresh enforcement point).
#[tokio::test]
async fn refresh_sub_floor_glibc_label_is_422_and_db_untouched() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfgfloor" }
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfgfloor"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.schema": "v1",
                "com.g5labs.runtime.base_image": "ubuntu-24.04-glibc-2.39"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"]["details"]["code"],
        "base_image_below_glibc_floor"
    );
    // Verbatim floor message per microvm_execution_boundary D12.
    assert_eq!(
        body["error"]["details"]["error"],
        "guest below the glibc 2.43 floor — repin to a 26.04 base"
    );

    let refreshed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT runtime_labels_refreshed_at FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(
        refreshed_at.is_none(),
        "sub-floor rejection must not persist runtime labels"
    );

    app.cleanup().await;
}

/// An at-floor declaration passes and persists — the guard rejects ONLY a
/// positively-declared sub-floor ABI (unlabeled/malformed acceptance is
/// covered by the schema-absent test above).
#[tokio::test]
async fn refresh_at_floor_glibc_label_persists() {
    let mock = MockServer::start().await;
    let port = mock_port(&mock);
    Mock::given(method("GET"))
        .and(path(format!(
            "/v2/acme/app/manifests/sha256:{SIXTY_FOUR_HEX}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": { "digest": "sha256:cfgatfloor" }
        })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/acme/app/blobs/sha256:cfgatfloor"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(config_blob_with_labels(json!({
                "com.g5labs.runtime.schema": "v1",
                "com.g5labs.runtime.base_image": "ubuntu-26.04-glibc-2.43"
            }))),
        )
        .mount(&mock)
        .await;

    let app = spawn_app_for_registry().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let client = app.authenticated_client(&user);
    let host_id = provision_host_with_image(&client, org.id, &base_image_ref(port)).await;

    let resp = client
        .post_json(
            &format!(
                "/api/v1/orgs/{}/worker-hosts/{host_id}/runtime-labels/refresh",
                org.id
            ),
            &json!({}),
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (schema, labels): (Option<String>, Value) = sqlx::query_as(
        "SELECT runtime_schema, runtime_labels FROM worker_hosts WHERE host_id = $1",
    )
    .bind(host_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(schema.as_deref(), Some("v1"));
    assert_eq!(
        labels["com.g5labs.runtime.base_image"],
        "ubuntu-26.04-glibc-2.43"
    );

    app.cleanup().await;
}
