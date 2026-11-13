//! Live-DB integration coverage for the host-auth middleware seam.
//!
//! This test binary deliberately builds a tiny route set instead of using
//! `TestApp`: the production app currently has no stable public host-control
//! route whose response exposes the authenticated host context. The middleware
//! stack is real, requests are real HTTP requests through axum, the extractor is
//! the production `AuthenticatedHost`, and the host validator is the production
//! `PgHostAuthValidator` wired to the restricted system pool from `TestDb`.

use crate::helpers;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::Path;
use axum::routing::get;
use axum::{Json, Router};
use g5_api::auth::{HostAuthValidator, PgHostAuthValidator};
use g5_api::errors::{AppError, G5Error};
use g5_api::middleware::auth_middleware::HOST_ONLY_INTERNAL_PREFIX;
use g5_api::middleware::{AuthenticatedHost, InMemoryRateLimitStore};
use g5_auth::{JwtClaims, JwtMinter, JwtValidator, Scope};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

use helpers::{TestDb, TEST_PRIVATE_KEY_PEM, TEST_PUBLIC_KEY_PEM};

const HOST_ECHO_PATH_PREFIX: &str = "/api/internal/host/orgs";
const NON_HOST_INTERNAL_PATH: &str = "/api/internal/usage";
const SYSTEM_POOL_USER: &str = "g5_runtime_system";
const TEST_JWT_ISSUER: &str = "g5-api-test";
const TEST_JWT_AUDIENCE: &str = "g5";

struct HostAuthTestApp {
    base_url: String,
    client: reqwest::Client,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    server_task: tokio::task::JoinHandle<()>,
    system_pool: PgPool,
    test_db: TestDb,
}

impl HostAuthTestApp {
    async fn spawn_with_pg_validator() -> Self {
        let test_db = TestDb::create().await;
        let system_pool = connect_system_pool(&test_db).await;
        let host_validator =
            Arc::new(PgHostAuthValidator::new(system_pool.clone())) as Arc<dyn HostAuthValidator>;
        Self::spawn(test_db, system_pool, host_validator).await
    }

    async fn spawn_with_closed_pg_validator() -> Self {
        let test_db = TestDb::create().await;
        let system_pool = connect_system_pool(&test_db).await;
        let host_validator =
            Arc::new(PgHostAuthValidator::new(system_pool.clone())) as Arc<dyn HostAuthValidator>;
        system_pool.close().await;
        Self::spawn(test_db, system_pool, host_validator).await
    }

    async fn spawn(
        test_db: TestDb,
        system_pool: PgPool,
        host_validator: Arc<dyn HostAuthValidator>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind host-auth test server");
        let addr = listener.local_addr().expect("read host-auth test addr");
        let base_url = format!("http://{addr}");

        let jwt_validator = Arc::new(
            JwtValidator::new(
                TEST_PUBLIC_KEY_PEM,
                Some(TEST_JWT_ISSUER),
                Some(TEST_JWT_AUDIENCE),
            )
            .expect("build test JwtValidator"),
        );
        let rate_limit_store = Arc::new(InMemoryRateLimitStore::new());
        let router = Router::new()
            .route(
                "/api/internal/host/orgs/:org_id/echo",
                get(host_context_echo),
            )
            .route(NON_HOST_INTERNAL_PATH, get(non_host_internal_route));
        let router = g5_api::middleware::build_middleware_stack(
            router,
            &helpers::test_config(),
            jwt_validator,
            rate_limit_store,
            None,
            Some(host_validator),
        );

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("host-auth test server crashed");
        });

        Self {
            base_url,
            client: reqwest::Client::new(),
            shutdown: Some(shutdown_tx),
            server_task,
            system_pool,
            test_db,
        }
    }

    async fn seed_org(&self, label: &str) -> Uuid {
        let org_id = Uuid::new_v4();
        let unique = Uuid::new_v4().simple().to_string();
        sqlx::query("WITH o AS (INSERT INTO orgs (id,name,slug) VALUES ($1,$2,$3) RETURNING id),
             u AS (INSERT INTO users (email,display_name) VALUES ($1::text || '@fixture.test','Fixture Admin') RETURNING id)
             INSERT INTO org_memberships (org_id,user_id,role) SELECT o.id,u.id,'admin' FROM o,u")
            .bind(org_id)
            .bind(format!("{label}-{unique}"))
            .bind(format!("{label}-{unique}"))
            .execute(&self.test_db.pool)
            .await
            .expect("seed org");
        org_id
    }

    async fn seed_host(&self, org_id: Uuid, status: &str) -> Uuid {
        let host_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO worker_hosts (host_id, org_id, region, status) \
             VALUES ($1, $2, 'us-central1', $3)",
        )
        .bind(host_id)
        .bind(org_id)
        .bind(status)
        .execute(&self.test_db.pool)
        .await
        .expect("seed worker_hosts row");
        host_id
    }

    async fn get_host_echo(&self, path_org_id: Uuid, token: &str) -> reqwest::Response {
        self.client
            .get(format!(
                "{base}{HOST_ECHO_PATH_PREFIX}/{path_org_id}/echo",
                base = self.base_url
            ))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("host echo request sends")
    }

    async fn get_non_host_internal(&self, token: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{}", self.base_url, NON_HOST_INTERNAL_PATH))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("non-host internal request sends")
    }

    async fn cleanup(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if tokio::time::timeout(Duration::from_secs(5), &mut self.server_task)
            .await
            .is_err()
        {
            self.server_task.abort();
            let _ = self.server_task.await;
        }
        self.system_pool.close().await;
        self.test_db.drop().await;
    }
}

async fn connect_system_pool(test_db: &TestDb) -> PgPool {
    assert!(
        test_db
            .system_url
            .contains(&format!("//{SYSTEM_POOL_USER}:")),
        "TestDb::system_url must connect as {SYSTEM_POOL_USER}; got {}",
        test_db.system_url
    );
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.system_url)
        .await
        .expect("connect restricted system pool")
}

async fn host_context_echo(
    Path(path_org_id): Path<Uuid>,
    host: AuthenticatedHost,
) -> Result<Json<Value>, AppError> {
    if host.org_id() != path_org_id {
        return Err(AppError::from_error(G5Error::Forbidden {
            missing_scopes: vec!["host_org_match".to_string()],
        }));
    }

    Ok(Json(json!({
        "hostId": host.host_id(),
        "orgId": host.org_id(),
        "hasWorkerHostScope": host.has_scope(&Scope::WorkerHost),
        "scopes": host
            .scopes()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "prefix": HOST_ONLY_INTERNAL_PREFIX,
    })))
}

async fn non_host_internal_route() -> Json<Value> {
    Json(json!({ "reached": true }))
}

fn host_minter() -> JwtMinter {
    JwtMinter::new(TEST_PRIVATE_KEY_PEM, TEST_JWT_ISSUER, TEST_JWT_AUDIENCE)
        .expect("build host JWT minter")
}

fn sign_host_claims(host_id: Uuid, org_id: Uuid, iat: i64, exp: i64) -> String {
    let claims = JwtClaims {
        sub: host_id.to_string(),
        org: org_id.to_string(),
        scopes: vec![Scope::WorkerHost],
        iat,
        exp,
        iss: Some(TEST_JWT_ISSUER.to_string()),
        aud: Some(TEST_JWT_AUDIENCE.to_string()),
        jti: Some(Uuid::new_v4().to_string()),
        token_use: None,
        client: None,
    };
    let key = EncodingKey::from_ec_pem(TEST_PRIVATE_KEY_PEM.as_bytes())
        .expect("test private key should parse");
    encode(&Header::new(Algorithm::ES256), &claims, &key).expect("sign host claims")
}

fn expired_host_jwt(host_id: Uuid, org_id: Uuid) -> String {
    let now = chrono::Utc::now().timestamp();
    sign_host_claims(host_id, org_id, now - 7200, now - 3600)
}

fn corrupt_signature(token: &str) -> String {
    let parts = token.split('.').collect::<Vec<_>>();
    assert_eq!(parts.len(), 3, "test JWT should have three compact parts");
    let other_signature = host_minter()
        .mint_host_jwt(Uuid::new_v4(), Uuid::new_v4(), Duration::from_secs(3600))
        .expect("mint second host_jwt for tampered signature")
        .split('.')
        .nth(2)
        .expect("second JWT should have a signature part")
        .to_string();
    format!("{}.{}.{}", parts[0], parts[1], other_signature)
}

async fn assert_error_response(response: reqwest::Response, status: StatusCode, code: &str) {
    assert_eq!(
        response.status(),
        status,
        "error status should match contract"
    );
    let body: Value = response.json().await.expect("error response is JSON");
    assert_eq!(
        body["error"]["code"], code,
        "error body should use the expected catalog code: {body}"
    );
}

/// [D4]/[D5] A valid `host_jwt` for a `ready` worker_hosts row must traverse
/// `build_middleware_stack`, pass `PgHostAuthValidator`, and surface through the
/// production `AuthenticatedHost` extractor with the exact host identity and
/// `worker_host` scope.
#[tokio::test]
async fn ready_host_jwt_uses_pg_validator_and_authenticated_host_extractor() {
    let app = HostAuthTestApp::spawn_with_pg_validator().await;
    let org_id = app.seed_org("host-auth-ready-org").await;
    let host_id = app.seed_host(org_id, "ready").await;

    let token = host_minter()
        .mint_host_jwt(host_id, org_id, Duration::from_secs(3600))
        .expect("mint valid host_jwt through production minter");
    let response = app.get_host_echo(org_id, &token).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("valid response is JSON");
    assert_eq!(body["hostId"], host_id.to_string());
    assert_eq!(body["orgId"], org_id.to_string());
    assert_eq!(body["hasWorkerHostScope"], true);
    assert_eq!(body["scopes"], json!(["worker_host"]));
    assert_eq!(body["prefix"], HOST_ONLY_INTERNAL_PREFIX);

    app.cleanup().await;
}

/// [D5] A `provisioning` worker_hosts row is an explicit accept state. The
/// middleware must authenticate it with 200 so first-boot host bring-up cannot
/// deadlock before the host transitions to `ready`.
#[tokio::test]
async fn provisioning_host_jwt_is_accepted_for_bring_up_window() {
    let app = HostAuthTestApp::spawn_with_pg_validator().await;
    let org_id = app.seed_org("host-auth-provisioning-org").await;
    let host_id = app.seed_host(org_id, "provisioning").await;

    let token = host_minter()
        .mint_host_jwt(host_id, org_id, Duration::from_secs(3600))
        .expect("mint provisioning host_jwt through production minter");
    let response = app.get_host_echo(org_id, &token).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.expect("valid response is JSON");
    assert_eq!(body["hostId"], host_id.to_string());
    assert_eq!(body["orgId"], org_id.to_string());
    assert_eq!(body["hasWorkerHostScope"], true);
    assert_eq!(body["scopes"], json!(["worker_host"]));

    app.cleanup().await;
}

/// [D5] Live-DB validator-arm rejections must map to 401 through the live
/// middleware seam, while reconciliation states remain authenticated.
/// The *distinct* variant semantics over the real `authz.lookup_worker_host_for_auth`
/// function are asserted directly in sibling `tests/host_validator_rls_regression.rs`:
/// `NotFound` (unknown host), `OrgMismatch` (org mismatch), and
/// `StatusForbidden("terminated")` (status gating). The sibling also pins that
/// draining and unreachable hosts stay authenticated to finish reconciliation.
#[tokio::test]
async fn live_db_host_validator_enforces_identity_and_reconciliation_states() {
    let app = HostAuthTestApp::spawn_with_pg_validator().await;
    let org_id = app.seed_org("host-auth-validator-org").await;
    let other_org_id = app.seed_org("host-auth-validator-other-org").await;
    let host_id = app.seed_host(org_id, "ready").await;
    let terminated_host_id = app.seed_host(org_id, "terminated").await;
    let draining_host_id = app.seed_host(org_id, "draining").await;
    let unreachable_host_id = app.seed_host(org_id, "unreachable").await;

    let unknown_host_token = host_minter()
        .mint_host_jwt(Uuid::new_v4(), org_id, Duration::from_secs(3600))
        .expect("mint host_jwt for unknown host_id");
    let not_found = app.get_host_echo(org_id, &unknown_host_token).await;
    assert_error_response(not_found, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    let now = chrono::Utc::now().timestamp();
    let forged_org_mismatch = sign_host_claims(host_id, other_org_id, now, now + 3600);
    let org_mismatch = app.get_host_echo(org_id, &forged_org_mismatch).await;
    assert_error_response(org_mismatch, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    let terminated_token = host_minter()
        .mint_host_jwt(terminated_host_id, org_id, Duration::from_secs(3600))
        .expect("mint host_jwt for terminated host");
    let terminated = app.get_host_echo(org_id, &terminated_token).await;
    assert_error_response(terminated, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    let draining_token = host_minter()
        .mint_host_jwt(draining_host_id, org_id, Duration::from_secs(3600))
        .expect("mint host_jwt for draining host");
    let draining = app.get_host_echo(org_id, &draining_token).await;
    assert_eq!(draining.status(), StatusCode::OK);

    let unreachable_token = host_minter()
        .mint_host_jwt(unreachable_host_id, org_id, Duration::from_secs(3600))
        .expect("mint host_jwt for unreachable host");
    let unreachable = app.get_host_echo(org_id, &unreachable_token).await;
    assert_eq!(unreachable.status(), StatusCode::OK);

    app.cleanup().await;
}

/// [D4] The host-only internal prefix rejects user principals.
#[tokio::test]
async fn host_only_route_rejects_user_principal() {
    let app = HostAuthTestApp::spawn_with_pg_validator().await;
    let org_id = app.seed_org("host-auth-user-principal-org").await;
    let now = chrono::Utc::now().timestamp();
    let claims = JwtClaims {
        sub: Uuid::new_v4().to_string(),
        org: org_id.to_string(),
        scopes: vec![Scope::WorkspacesRead, Scope::SessionsRead],
        iat: now,
        exp: now + 3600,
        iss: Some(TEST_JWT_ISSUER.to_string()),
        aud: Some(TEST_JWT_AUDIENCE.to_string()),
        jti: Some(Uuid::new_v4().to_string()),
        token_use: None,
        client: None,
    };
    let key = EncodingKey::from_ec_pem(TEST_PRIVATE_KEY_PEM.as_bytes())
        .expect("test private key should parse");
    let token = encode(&Header::new(Algorithm::ES256), &claims, &key).expect("sign user JWT");

    let response = app.get_host_echo(org_id, &token).await;
    assert_error_response(response, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    app.cleanup().await;
}

/// [D4]/[D5] Token-layer failures must be 401, while a valid host from another
/// org reaching the host route must pass middleware authentication and then be
/// rejected by this test handler's path/org match check with a JSON 403.
#[tokio::test]
async fn token_failures_are_401_and_handler_org_mismatch_is_403() {
    let app = HostAuthTestApp::spawn_with_pg_validator().await;
    let org_id = app.seed_org("host-auth-token-org").await;
    let other_org_id = app.seed_org("host-auth-token-other-org").await;
    let host_id = app.seed_host(org_id, "ready").await;
    let other_host_id = app.seed_host(other_org_id, "ready").await;

    let valid_host_jwt = host_minter()
        .mint_host_jwt(host_id, org_id, Duration::from_secs(3600))
        .expect("mint valid host_jwt through production minter");

    let expired = app
        .get_host_echo(org_id, &expired_host_jwt(host_id, org_id))
        .await;
    assert_error_response(expired, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    let invalid_signature = app
        .get_host_echo(org_id, &corrupt_signature(&valid_host_jwt))
        .await;
    assert_error_response(invalid_signature, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    let wrong_org_host_jwt = host_minter()
        .mint_host_jwt(other_host_id, other_org_id, Duration::from_secs(3600))
        .expect("mint other-org host_jwt through production minter");
    let wrong_org = app.get_host_echo(org_id, &wrong_org_host_jwt).await;
    assert_error_response(wrong_org, StatusCode::FORBIDDEN, "FORBIDDEN").await;

    app.cleanup().await;
}

/// [D4] A valid host JWT on a non-host internal route is still an unknown
/// internal credential/path pairing and must be rejected before the handler.
#[tokio::test]
async fn valid_host_jwt_on_non_host_internal_route_is_unauthorized() {
    let app = HostAuthTestApp::spawn_with_pg_validator().await;
    let org_id = app.seed_org("host-auth-path-gate-org").await;
    let host_id = app.seed_host(org_id, "ready").await;

    let token = host_minter()
        .mint_host_jwt(host_id, org_id, Duration::from_secs(3600))
        .expect("mint valid host_jwt through production minter");
    let response = app.get_non_host_internal(&token).await;
    assert_error_response(response, StatusCode::UNAUTHORIZED, "UNAUTHORIZED").await;

    app.cleanup().await;
}

/// [D5] A DB error from `PgHostAuthValidator` must fail closed with 500 and a
/// JSON error envelope, never silently bypassing the liveness check to 200.
#[tokio::test]
async fn host_validator_db_error_maps_to_500_fail_closed() {
    let app = HostAuthTestApp::spawn_with_closed_pg_validator().await;
    let org_id = app.seed_org("host-auth-db-error-org").await;
    let host_id = app.seed_host(org_id, "ready").await;
    let token = host_minter()
        .mint_host_jwt(host_id, org_id, Duration::from_secs(3600))
        .expect("mint host_jwt for DB-error path");

    let response = app.get_host_echo(org_id, &token).await;
    assert_error_response(
        response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
    )
    .await;

    app.cleanup().await;
}
