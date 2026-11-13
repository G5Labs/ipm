//! Shared test infrastructure for g5-api integration tests.
//!
//! This module provides the full application harness (`TestApp`), database
//! lifecycle management (`TestDb`), an authenticated HTTP client
//! (`AuthenticatedClient`), fixture factories (in `fixtures.rs`), and a
//! WebSocket assertion helper (in `ws_client.rs`).
//!
//! **This module contains zero `#[test]` functions** — it is purely a library
//! consumed by sibling test files via `mod helpers;`.
//!
//! Each integration test binary compiles this module independently but only
//! uses a subset. Suppress dead-code warnings for the unused portion.
#![allow(dead_code, unused_imports)]

/// The name libtest gives `function` in the module at `module_path`, as a
/// `--exact` filter needs it. `module_path!()` starts with the crate segment
/// that libtest leaves out, so a test re-running itself in a child process
/// gets the same name whether its file is compiled through `tests/main.rs`
/// or as a target of its own.
pub fn exact_test_name(module_path: &str, function: &str) -> String {
    match module_path.split_once("::") {
        Some((_, module)) => format!("{module}::{function}"),
        None => function.to_owned(),
    }
}

/// The process's one global tracing subscriber, installed on first use: the
/// capture layer from `g5_api::middleware::test_capture`, recording only the
/// `g5_audit`, `g5_api::workspace_activity` and the email module's `g5_api::email`
/// targets (so the buffer stays bounded when one process runs the whole suite),
/// plus a formatting layer that writes server-side warnings and
/// errors to the test output (AppError logs internal errors at `error`, so a
/// 500 names its cause). Every test that needs a subscriber goes through this,
/// so the tests also work when the whole target runs in one process
/// (`cargo test`), not only one test per process (nextest). Installing a
/// second global subscriber anywhere else would panic here.
pub fn tracing_capture() -> &'static g5_api::middleware::test_capture::Events {
    static EVENTS: std::sync::OnceLock<g5_api::middleware::test_capture::Events> =
        std::sync::OnceLock::new();
    EVENTS.get_or_init(|| {
        use tracing_subscriber::prelude::*;
        let (capture, events) = g5_api::middleware::test_capture::capture_layer();
        // A span is enabled only if some layer wants it, and the WARN
        // formatting layer wants none below WARN: keep every span enabled
        // here (the capture layer records events, not spans, so this costs
        // nothing) or server spans such as `ws_connection` could never
        // become current and span-scoping tests would pass vacuously.
        let subscriber = tracing_subscriber::registry()
            .with(
                capture.with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                    metadata.is_span()
                        || metadata.target() == "g5_audit"
                        || metadata.target() == "g5_api::email"
                        || metadata.target() == "g5_api::workspace_activity"
                })),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_test_writer()
                    .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
            );
        tracing::subscriber::set_global_default(subscriber).expect(
            "tests install their global tracing subscriber only through helpers::tracing_capture",
        );
        events
    })
}

/// The runtime identity a seeded `worker_hosts` row needs to receive
/// placement (`g5.runtime.placement.runtime-identity`): the control plane's
/// bundled catalog and dispatcher contract, as a hello would store it.
pub fn current_runtime_identity() -> serde_json::Value {
    serde_json::to_value(g5_internal_api::host_ctl::HostRuntimeIdentity::bundled())
        .expect("bundled runtime identity serializes")
}

pub mod fixtures;
pub mod github_app;
pub mod github_key;
pub mod platform;
pub mod question_sets;
pub mod review_flow;
pub mod runtime_peer;
pub mod ws_client;

pub mod commit_ack_proxy;
pub type TenantUrlRewrite = std::sync::Arc<dyn Fn(&str) -> String + Send + Sync>;

use std::ffi::OsString;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use axum::Router;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tokio::time::{sleep, Duration};
use url::Url;
use uuid::Uuid;

use g5_auth::{JwtClaims, JwtValidator};
use g5_authz::scoped_db::ScopedDb;
use g5_contracts::types::ids::{OrgId, UserId};
use g5_contracts::types::AuthorityTarget;

/// Valid immutable worker image used by host tests that are not themselves
/// testing image-source selection. Production provisioning has no silent
/// fallback, so every unrelated test supplies this explicit pin.
pub const TEST_WORKER_IMAGE_REF: &str = "ghcr.io/g5labs/test-worker@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub use fixtures::{
    seed_org_llm_credential, seed_org_llm_credential_value, seed_org_llm_credentials_all_providers,
    TestOrg, TestTask, TestUser, TestWorkspace,
};
pub use runtime_peer::TestWorkspaceRuntimePeer;

pub use ws_client::WsTestClient;

/// Restores every process environment variable changed by an integration test,
/// including when the test panics or returns early. Question-v1 activation is
/// intentionally not managed here: each test app carries that setting in its
/// own immutable configuration.
pub struct EnvVarGuard {
    values: Vec<(String, Option<OsString>)>,
}

impl EnvVarGuard {
    pub fn set_many(values: &[(&str, &str)]) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var_os(key)))
            .collect();
        for (key, value) in values {
            std::env::set_var(key, value);
        }
        Self { values: previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (key, value) in self.values.drain(..).rev() {
            match value {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }
}

/// Derives the two-key Postgres advisory-lock identity used by `ChatService`
/// to serialize turn allocation for one chat session.
pub fn chat_session_turn_lock_key(session_id: Uuid) -> (i32, i32) {
    let bytes = session_id.as_bytes();
    let folded = i64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    (0x4735_4354, (folded ^ (folded >> 32)) as i32)
}

/// Waits until a statement starting with `statement_start` waits on a lock
/// and the workspace's VM lease has ended. pg_stat_activity cuts long query
/// text, so the statement is matched by its start.
pub async fn wait_for_lock_wait_past_the_lease(
    app: &TestApp,
    workspace_id: Uuid,
    statement_start: &str,
) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND query LIKE $1 || '%')",
            )
            .bind(statement_start)
            .fetch_one(&app.db)
            .await
            .expect("read lock waits");
            let expired: bool = sqlx::query_scalar(
                "SELECT lease_expires_at <= clock_timestamp() FROM workspace_vms
                 WHERE workspace_id = $1",
            )
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .expect("read the lease");
            if waiting && expired {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{statement_start} waits until the lease ends"));
}

/// Waits until some backend of this database waits on a lock; a request
/// admitted under an access hold blocks here while a competing change is
/// still in flight (`g5.security.current-authority`).
pub async fn wait_for_lock_wait(app: &TestApp) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock')",
            )
            .fetch_one(&app.db)
            .await
            .expect("read lock waits");
            if waiting {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("a request waits on the in-flight change");
}

/// The claims of a test JWT as JSON, read without verification: tests check
/// what the service put in the token, not the signature the validator checks.
pub fn decode_test_jwt(jwt: &str) -> serde_json::Value {
    use base64::Engine as _;
    let payload = jwt.split('.').nth(1).expect("a JWT payload");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("base64url payload");
    serde_json::from_slice(&bytes).expect("JSON claims")
}

/// The stored form of a refresh credential (`g5.user.session-authority`).
pub use g5_api::services::auth_service::refresh_token_hash;

/// Browser-enforced host binding needs all of these attributes on set and clear.
pub fn assert_host_cookie(value: &str) {
    assert!(value.starts_with("__Host-"), "{value}");
    let attributes: Vec<_> = value.split(';').skip(1).map(str::trim).collect();
    for required in ["Secure", "HttpOnly", "SameSite=Lax", "Path=/"] {
        assert!(
            attributes.contains(&required),
            "missing {required}: {value}"
        );
    }
    assert!(
        !attributes
            .iter()
            .any(|a| a.to_ascii_lowercase().starts_with("domain=")),
        "{value}"
    );
}

/// Stores a fresh refresh credential for `user_id` as the service would and
/// returns the plaintext the client holds; only its digest is in the table.
pub async fn insert_refresh_token(app: &TestApp, user_id: Uuid) -> String {
    let token = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO refresh_tokens (token_hash, user_id, expires_at)
         VALUES ($1, $2, now() + interval '1 day')",
    )
    .bind(refresh_token_hash(&token))
    .bind(user_id)
    .execute(&app.db)
    .await
    .expect("store a refresh credential");
    token
}

/// Ends the VM lease a few seconds from now, then runs `lock`, a locking
/// statement on the workspace id `$1`, in a transaction that changes nothing.
pub async fn hold_while_the_lease_ends(
    app: &TestApp,
    workspace_id: Uuid,
    lock: &str,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at = clock_timestamp() + interval '3 seconds'
         WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .execute(&app.db)
    .await
    .expect("shorten the lease");
    let mut holder = app.db.begin().await.expect("begin the holder");
    sqlx::query(lock)
        .bind(workspace_id)
        .execute(&mut *holder)
        .await
        .expect("hold the row");
    holder
}

// ---------------------------------------------------------------------------
// Test ES256 key pair — MUST match g5-api/src/auth/jwt.rs tests
// ---------------------------------------------------------------------------

pub const TEST_PRIVATE_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg0J63p4NbSY225Nco
6sEKek+VGL+z0Bt+/o3rkfIBFAihRANCAAQV+5QLRzUNaBAZyRaVIOWFtfZpe2ly
7GviW9bv0coSK0uGOFk/8N5UiSrpwDjuEN7DIIdAPwvDY26VHa9qhsW+
-----END PRIVATE KEY-----";

pub const TEST_PUBLIC_KEY_PEM: &str = "\
-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEFfuUC0c1DWgQGckWlSDlhbX2aXtp
cuxr4lvW79HKEitLhjhZP/DeVIkq6cA47hDewyCHQD8Lw2NulR2vaobFvg==
-----END PUBLIC KEY-----";

const TEST_RUNTIME_SYSTEM_DB_USER: &str = "g5_runtime_system";
const TEST_RUNTIME_SYSTEM_DB_PASSWORD: &str = "g5_system_test";
const TEST_RUNTIME_TENANT_DB_USER: &str = "g5_runtime_tenant";
const TEST_RUNTIME_TENANT_DB_PASSWORD: &str = "g5_tenant_test";
const TEST_AUTHZ_DB_USER: &str = "authz_writer";
const TEST_AUTHZ_DB_PASSWORD: &str = "g5_authz_test";

// The full app harness opens several independent pools per test database.
// Keep CI integration tests from exhausting or slowly acquiring Postgres
// connections when Cloud Build runs other Rust/Docker steps in parallel.
const TEST_DIRECT_DB_POOL_MAX_CONNECTIONS: u32 = 2;
const TEST_RUNTIME_DB_POOL_MAX_CONNECTIONS: u32 = 2;
const TEST_RUNTIME_DB_MIN_CONNECTIONS: u32 = 0;
const TEST_RUNTIME_DB_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const TEST_ADMIN_DB_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(20);
const TEST_DATABASE_TEMPLATE_ENV: &str = "G5_TEST_DATABASE_TEMPLATE";
const TEST_RUNTIME_ROLES_BOOTSTRAPPED_ENV: &str = "G5_TEST_RUNTIME_ROLES_BOOTSTRAPPED";

/// A stock local Postgres allows 100 connections. Each live TestApp owns four
/// pools capped at two connections, so six concurrent apps reserve at most 48
/// slots and leave more than half the budget for the stack and transient admin
/// work. Nextest runs one test per process; this gate protects libtest's
/// in-process threaded runner without reducing canonical CI parallelism.
pub(crate) const TEST_APP_MAX_CONCURRENCY: usize = 6;
static TEST_APP_CAPACITY: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(TEST_APP_MAX_CONCURRENCY)));

pub(crate) async fn acquire_test_app_capacity() -> OwnedSemaphorePermit {
    Arc::clone(&TEST_APP_CAPACITY)
        .acquire_owned()
        .await
        .expect("TestApp capacity semaphore must remain open")
}

pub(crate) fn try_acquire_test_app_capacity() -> Result<OwnedSemaphorePermit, TryAcquireError> {
    Arc::clone(&TEST_APP_CAPACITY).try_acquire_owned()
}

// ---------------------------------------------------------------------------
// TestDb — isolated database per test
// ---------------------------------------------------------------------------

/// Manages an isolated Postgres test database.
///
/// Each test creates a uniquely-named database, runs all sqlx migrations,
/// executes the test, and drops the database on cleanup. This guarantees
/// zero cross-contamination between parallel tests.
pub struct TestDb {
    pub pool: PgPool,
    pub db_name: String,
    pub system_url: String,
    pub authz_url: String,
    pub tenant_url: String,
    /// Connection URL for the admin/postgres database (for CREATE/DROP).
    admin_url: String,
}

impl TestDb {
    /// Creates a new isolated test database with the baseline schema applied.
    ///
    /// The bootstrap connection prefers explicit environment wiring, but it can
    /// also recover from stale local defaults by probing the conventional dev
    /// and test-stack Postgres URLs on localhost.
    pub async fn create() -> Self {
        Self::create_with_catalog(None).await
    }

    /// Historical migrations must be tested before later schema cutovers,
    /// without booting the current application against a retired schema.
    pub async fn create_from_migrations(path: &std::path::Path) -> Self {
        Self::create_with_catalog(Some(path)).await
    }

    async fn create_with_catalog(migrations: Option<&std::path::Path>) -> Self {
        let base_url = Self::resolve_base_url().await;

        // Parse the base URL to extract host/credentials, connect to `postgres` db
        let admin_url = Self::with_connect_timeout(&Self::to_admin_url(&base_url));

        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(TEST_ADMIN_DB_ACQUIRE_TIMEOUT)
            .connect(&admin_url)
            .await
            .expect("Failed to connect to admin database for test setup");

        // Generate unique database name
        let db_name = format!("g5_test_{}", Uuid::new_v4().simple());

        let template_database = migrations
            .is_none()
            .then(Self::template_database_name)
            .flatten();
        match template_database.as_ref() {
            Some(template_db) => {
                sqlx::query(&format!(
                    "CREATE DATABASE \"{}\" WITH TEMPLATE \"{}\"",
                    db_name, template_db
                ))
                .execute(&admin_pool)
                .await
                .expect("Failed to create test database from template");
            }
            None => {
                sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
                    .execute(&admin_pool)
                    .await
                    .expect("Failed to create test database");
            }
        }

        // Build connection URL for the new test database
        let test_db_url = Self::replace_db_name(&base_url, &db_name);

        let pool = PgPoolOptions::new()
            .max_connections(TEST_DIRECT_DB_POOL_MAX_CONNECTIONS)
            .connect(&test_db_url)
            .await
            .expect("Failed to connect to test database");

        if template_database.is_none() {
            if let Some(path) = migrations {
                g5_api::db::run_migrations_from_path(&pool, path)
                    .await
                    .expect("Failed to run historical migrations on test database");
            } else {
                g5_api::db::run_migrations(&pool)
                    .await
                    .expect("Failed to run migrations on test database");
            }
        }

        if !Self::runtime_login_roles_prebootstrapped() {
            Self::bootstrap_runtime_login_roles(&admin_pool).await;
        }

        admin_pool.close().await;

        Self {
            pool,
            db_name,
            system_url: Self::with_credentials(
                &test_db_url,
                TEST_RUNTIME_SYSTEM_DB_USER,
                TEST_RUNTIME_SYSTEM_DB_PASSWORD,
            ),
            authz_url: Self::with_credentials(
                &test_db_url,
                TEST_AUTHZ_DB_USER,
                TEST_AUTHZ_DB_PASSWORD,
            ),
            tenant_url: Self::with_credentials(
                &test_db_url,
                TEST_RUNTIME_TENANT_DB_USER,
                TEST_RUNTIME_TENANT_DB_PASSWORD,
            ),
            admin_url,
        }
    }

    /// Resolves a working Postgres base URL for integration-test bootstrap.
    pub async fn resolve_base_url() -> String {
        let mut failures = Vec::new();

        for candidate in Self::candidate_base_urls() {
            let admin_url = Self::with_connect_timeout(&Self::to_admin_url(&candidate));
            match PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(TEST_ADMIN_DB_ACQUIRE_TIMEOUT)
                .connect(&admin_url)
                .await
            {
                Ok(pool) => {
                    pool.close().await;
                    return candidate;
                }
                Err(error) => failures.push(format!("{candidate} -> {error}")),
            }
        }

        panic!(
            "Failed to connect to any Postgres admin database for integration-test bootstrap. \
Set TEST_DATABASE_URL explicitly or start the dev/test Postgres stack.\nTried:\n{}",
            failures.join("\n")
        );
    }

    /// Superuser URL of an existing test database, for a helper process that
    /// must reach the same database as a running test app (cross-process
    /// proofs). The database name comes from `SELECT current_database()`.
    pub async fn url_for_database(db_name: &str) -> String {
        Self::replace_db_name(&Self::resolve_base_url().await, db_name)
    }

    fn candidate_base_urls() -> Vec<String> {
        let mut candidates = Vec::new();

        if let Ok(url) = std::env::var("TEST_DATABASE_URL") {
            Self::push_unique_candidate(&mut candidates, url);
        }
        if let Ok(url) = std::env::var("DATABASE_URL") {
            Self::push_unique_candidate(&mut candidates, url);
        }
        if let Ok(port) = std::env::var("G5_TEST_PORT_POSTGRES") {
            Self::push_unique_candidate(
                &mut candidates,
                format!("postgres://g5:g5_test@localhost:{port}/g5_test"),
            );
        }
        if let Ok(port) = std::env::var("G5_PORT_POSTGRES") {
            Self::push_unique_candidate(
                &mut candidates,
                format!("postgres://g5:g5_dev@localhost:{port}/g5_dev"),
            );
        }

        for candidate in [
            "postgres://g5:g5_test@localhost:5433/g5_test",
            "postgres://g5:g5_dev@localhost:5433/g5_dev",
            "postgres://g5:g5_dev@localhost:5432/g5_dev",
            "postgres://g5:g5_test@localhost:5432/g5_test",
        ] {
            Self::push_unique_candidate(&mut candidates, candidate.to_string());
        }

        candidates
    }

    fn push_unique_candidate(candidates: &mut Vec<String>, candidate: String) {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    }

    /// Drops the test database and cleans up all connections.
    ///
    /// 1. Closes all connections in the pool.
    /// 2. Connects to the `postgres` database.
    /// 3. Terminates remaining connections to the test database.
    /// 4. Executes `DROP DATABASE IF EXISTS <db_name>`.
    pub async fn drop(self) {
        // Close the pool first, but do not hang forever if a test leaked a
        // checked-out connection. We still terminate any remaining backends
        // below so teardown can complete.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.pool.close()).await;

        let admin_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(TEST_ADMIN_DB_ACQUIRE_TIMEOUT)
            .connect(&self.admin_url)
            .await
            .expect("Failed to connect to admin database for cleanup");

        // Terminate any remaining connections to the test database
        sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1")
            .bind(&self.db_name)
            .execute(&admin_pool)
            .await
            .ok(); // Best-effort; don't fail cleanup

        // Drop the database
        sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", self.db_name))
            .execute(&admin_pool)
            .await
            .expect("Failed to drop test database");

        admin_pool.close().await;
    }

    /// Converts a database URL to point at the `postgres` admin database.
    fn to_admin_url(url: &str) -> String {
        // Replace the database name (last path segment) with "postgres"
        if let Some(pos) = url.rfind('/') {
            // Strip query parameters if present
            let base = &url[..pos];
            let query = url[pos..].find('?').map(|q| &url[pos + q..]).unwrap_or("");
            format!("{}/postgres{}", base, query)
        } else {
            url.to_string()
        }
    }

    fn with_connect_timeout(url: &str) -> String {
        let mut parsed = Url::parse(url).expect("test database URL should be valid");
        let has_connect_timeout = parsed
            .query_pairs()
            .any(|(key, _)| key == "connect_timeout");
        if !has_connect_timeout {
            parsed.query_pairs_mut().append_pair("connect_timeout", "5");
        }
        parsed.to_string()
    }

    /// Replaces the database name in a connection URL.
    fn replace_db_name(url: &str, new_db: &str) -> String {
        if let Some(pos) = url.rfind('/') {
            let base = &url[..pos];
            let query = url[pos + 1..]
                .find('?')
                .map(|q| &url[pos + 1 + q..])
                .unwrap_or("");
            if query.is_empty() {
                format!("{}/{}", base, new_db)
            } else {
                format!("{}/{}{}", base, new_db, query)
            }
        } else {
            url.to_string()
        }
    }

    fn with_credentials(url: &str, user: &str, password: &str) -> String {
        let mut parsed = Url::parse(url).expect("test database URL should be valid");
        parsed
            .set_username(user)
            .expect("test database URL should accept a username");
        parsed
            .set_password(Some(password))
            .expect("test database URL should accept a password");
        parsed.to_string()
    }

    fn template_database_name() -> Option<String> {
        let name = std::env::var(TEST_DATABASE_TEMPLATE_ENV).ok()?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        assert!(
            name.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
            "{TEST_DATABASE_TEMPLATE_ENV} must contain only ASCII letters, numbers, and underscores"
        );
        Some(name.to_string())
    }

    fn runtime_login_roles_prebootstrapped() -> bool {
        std::env::var(TEST_RUNTIME_ROLES_BOOTSTRAPPED_ENV)
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    }

    async fn bootstrap_runtime_login_roles(admin_pool: &PgPool) {
        // Runtime login roles are cluster-global, while each test runs in a
        // different database. Take the advisory lock through the shared admin
        // database so parallel nextest processes coordinate on one lock space.
        let sql = format!(
            r#"
BEGIN;
SELECT pg_advisory_xact_lock(640517824501);

DO $bootstrap$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{system_user}') THEN
        EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L INHERIT', '{system_user}', '{system_password}');
    ELSE
        EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L INHERIT', '{system_user}', '{system_password}');
    END IF;

    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{tenant_user}') THEN
        EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L INHERIT', '{tenant_user}', '{tenant_password}');
    ELSE
        EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L INHERIT', '{tenant_user}', '{tenant_password}');
    END IF;

    EXECUTE format('ALTER ROLE %I NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS', '{system_user}');
    EXECUTE format('ALTER ROLE %I NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS', '{tenant_user}');
    EXECUTE 'ALTER ROLE authz_writer WITH LOGIN PASSWORD ' || quote_literal('{authz_password}') || ' INHERIT';
    EXECUTE 'ALTER ROLE authz_writer NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS';
    EXECUTE 'ALTER ROLE authz_ctx_writer NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS';
    EXECUTE 'ALTER ROLE g5_app NOLOGIN';
    EXECUTE 'ALTER ROLE g5_tenant NOLOGIN';

    EXECUTE format('REVOKE g5_tenant FROM %I', '{system_user}');
    EXECUTE format('GRANT g5_app TO %I', '{system_user}');
    EXECUTE format('REVOKE g5_app FROM %I', '{tenant_user}');
    EXECUTE format('GRANT g5_tenant TO %I', '{tenant_user}');
END
$bootstrap$;

COMMIT;
"#,
            system_user = TEST_RUNTIME_SYSTEM_DB_USER,
            system_password = TEST_RUNTIME_SYSTEM_DB_PASSWORD,
            tenant_user = TEST_RUNTIME_TENANT_DB_USER,
            tenant_password = TEST_RUNTIME_TENANT_DB_PASSWORD,
            authz_password = TEST_AUTHZ_DB_PASSWORD,
        );

        const MAX_ATTEMPTS: usize = 12;

        for attempt in 0..MAX_ATTEMPTS {
            let mut conn = admin_pool
                .acquire()
                .await
                .expect("Failed to acquire connection for runtime role bootstrap");

            match sqlx::raw_sql(&sql).execute(&mut *conn).await {
                Ok(_) => return,
                Err(error)
                    if error.to_string().contains("tuple concurrently updated")
                        && attempt + 1 < MAX_ATTEMPTS =>
                {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    let backoff_ms = 100u64.saturating_mul(1u64 << attempt.min(6));
                    sleep(Duration::from_millis(backoff_ms)).await;
                }
                Err(error) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    panic!(
                        "Failed to bootstrap runtime login roles for integration tests: {error}"
                    );
                }
            }
        }

        panic!("Failed to bootstrap runtime login roles after retrying");
    }
}

// ---------------------------------------------------------------------------
// TestApp — full application harness
// ---------------------------------------------------------------------------

/// Full application harness that spawns g5-api on a random port with an
/// isolated Postgres database, runs migrations, and exposes HTTP + WebSocket
/// clients.
pub struct TestApp {
    /// The socket address the server is listening on.
    pub addr: SocketAddr,
    /// The database pool for direct database access in tests.
    pub db: PgPool,
    /// The runtime system pool used by the app under test.
    pub system_pool: PgPool,
    /// The authz pool used for session capability minting.
    pub authz_pool: PgPool,
    /// A bare reqwest client (no auth) with cookie support.
    pub client: reqwest::Client,
    /// HTTP base URL, e.g. `http://127.0.0.1:12345`.
    pub base_url: String,
    /// WebSocket base URL, e.g. `ws://127.0.0.1:12345`.
    pub ws_url: String,
    /// Repository storage, through its tenancy guard (`g5.repo.storage-guard`).
    pub repo_storage: g5_api_core::services::git::repo_store::TenantRepoStorage,
    /// Shared application state used by the spawned router.
    pub state: std::sync::Arc<g5_api::services::AppState>,
    /// Keeps the server, isolated DB, and capacity permit alive as one unit so
    /// unwinding cannot release admission while old connections still exist.
    lifecycle: TestAppLifecycle,
}

/// Observe detached worker-host bring-up until the cloud linkage is durable.
///
/// The public create route intentionally returns before bring-up completes, so
/// tests synchronize on the row rather than on task scheduling. Classified
/// terminal failures and terminal host states stop the wait immediately; a
/// scheduled/in-flight stockout retry remains observable as nonterminal.
pub async fn wait_for_cloud_instance_id_result(
    app: &TestApp,
    host_id: Uuid,
) -> Result<String, String> {
    const POLL_INTERVAL: Duration = Duration::from_millis(25);
    const OBSERVATION_GUARD: Duration = Duration::from_secs(60);

    let deadline = std::time::Instant::now() + OBSERVATION_GUARD;

    loop {
        let row =
            sqlx::query_as::<_, (Option<String>, String, Option<String>, i32, Option<String>)>(
                r#"
            SELECT cloud_instance_id,
                   status::text,
                   provision_error_code,
                   provision_retry_count,
                   provision_next_retry_at::text
            FROM worker_hosts
            WHERE host_id = $1
            "#,
            )
            .bind(host_id)
            .fetch_optional(&app.db)
            .await
            .map_err(|error| format!("failed to observe worker host {host_id}: {error}"))?;

        let Some((cloud_instance_id, status, error_code, retry_count, next_retry_at)) = row else {
            return Err(format!(
                "worker host disappeared before cloud linkage: host_id={host_id}"
            ));
        };

        let snapshot = format!(
            "host_id={host_id} status={status} cloud_instance_id={cloud_instance_id:?} \
             provision_error_code={error_code:?} provision_retry_count={retry_count} \
             provision_next_retry_at={next_retry_at:?}"
        );

        if let Some(cloud_instance_id) = cloud_instance_id {
            return Ok(cloud_instance_id);
        }

        let stockout_retry_active = error_code.as_deref() == Some("zone_capacity_exhausted")
            && (retry_count > 0 || next_retry_at.is_some());
        if error_code.is_some() && !stockout_retry_active {
            return Err(format!(
                "worker-host bring-up reached a terminal provisioning error: {snapshot}"
            ));
        }
        if status != "provisioning" {
            return Err(format!(
                "worker-host bring-up reached terminal status before cloud linkage: {snapshot}"
            ));
        }

        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "worker-host bring-up did not persist cloud linkage within 60s; last row: \
                 {snapshot}"
            ));
        }
        sleep(POLL_INTERVAL).await;
    }
}

/// Panic-on-failure convenience used by integration scenarios whose next
/// assertion requires a durable cloud instance linkage.
pub async fn wait_for_cloud_instance_id(app: &TestApp, host_id: Uuid) -> String {
    wait_for_cloud_instance_id_result(app, host_id)
        .await
        .unwrap_or_else(|diagnostic| panic!("{diagnostic}"))
}

struct TestAppLifecycle {
    server_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    server_task: Option<tokio::task::JoinHandle<()>>,
    test_db: Option<TestDb>,
    capacity_permit: Option<OwnedSemaphorePermit>,
    dispatch_tunnels: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl TestAppLifecycle {
    async fn shutdown_server(&mut self) {
        for task in self.dispatch_tunnels.get_mut().unwrap().drain(..) {
            task.abort();
        }
        if let Some(shutdown_tx) = self.server_shutdown.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(mut server_task) = self.server_task.take() {
            if tokio::time::timeout(std::time::Duration::from_secs(5), &mut server_task)
                .await
                .is_err()
            {
                server_task.abort();
                let _ = server_task.await;
            }
        }
    }

    async fn drop_database(&mut self) {
        if let Some(test_db) = self.test_db.take() {
            test_db.drop().await;
        }
    }
}

impl Drop for TestAppLifecycle {
    fn drop(&mut self) {
        for task in self.dispatch_tunnels.get_mut().unwrap().drain(..) {
            task.abort();
        }
        if let Some(shutdown_tx) = self.server_shutdown.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(server_task) = self.server_task.take() {
            server_task.abort();
        }

        let test_db = self.test_db.take();
        let capacity_permit = self.capacity_permit.take();
        if let (Some(test_db), Ok(runtime)) = (test_db, tokio::runtime::Handle::try_current()) {
            runtime.spawn(async move {
                test_db.drop().await;
                drop(capacity_permit);
            });
        }
    }
}

impl TestApp {
    /// Create a fixture repository's storage directory in `org_id`.
    pub async fn init_repo(&self, org_id: Uuid, repo_id: Uuid) -> std::path::PathBuf {
        self.repo_storage
            .init_repo(
                &g5_api_core::services::git::repo_store::TenantRepoStorage::test_key(
                    org_id, repo_id,
                ),
            )
            .await
            .expect("initialize fixture repository storage")
    }

    /// Create the storage directory of a fixture repository whose row exists,
    /// in the organization that row names.
    pub async fn init_fixture_repo(&self, repo_id: Uuid) -> std::path::PathBuf {
        let path = self.repo_path(repo_id).await;
        std::fs::create_dir_all(&path).expect("create fixture repository directory");
        path
    }

    /// A fixture repository's storage path, keyed for the organization its
    /// row names (`g5.repo.storage-guard`): tests reach storage through the
    /// same key check as production.
    pub async fn repo_path(&self, repo_id: Uuid) -> std::path::PathBuf {
        let org_id: Uuid = sqlx::query_scalar("SELECT org_id FROM repos WHERE id = $1")
            .bind(repo_id)
            .fetch_optional(&self.db)
            .await
            .expect("look up the fixture repository's organization")
            .unwrap_or_else(|| panic!("fixture repository {repo_id} has no repos row"));
        self.repo_storage.test_path(org_id, repo_id)
    }

    /// Spawns the full g5-api application on a random port with an isolated
    /// test database.
    ///
    /// 1. Creates an isolated test database via `TestDb::create()`.
    /// 2. Builds an `AppConfig` with test-appropriate defaults.
    /// 3. Builds the full axum `Router` using the same assembly function as
    ///    `main.rs`.
    /// 4. Binds a `TcpListener` to `127.0.0.1:0` (OS-assigned random port).
    /// 5. Spawns the axum server as a background Tokio task.
    /// 6. Returns `TestApp` with all fields populated.
    pub async fn spawn() -> Self {
        // The integration stack explicitly opts in through its own AppConfig.
        // No test mutates process-global configuration, so parallel TestApps
        // can exercise enabled and disabled deployments independently.
        Self::spawn_with_config(test_config()).await
    }

    /// Boot the production application against a database upgraded by a test.
    /// This keeps conversion and subsequent API behavior on the same identities.
    pub async fn spawn_with_database(db: TestDb) -> Self {
        Self::spawn_with_config_cloud_self_hosted_and_resolver(
            test_config(),
            None,
            None,
            None,
            None,
            Some(db),
        )
        .await
    }

    /// Spawns a TestApp with a custom configuration.
    ///
    /// Same as `spawn()` but allows overriding specific config fields
    /// (e.g., OAuth provider URLs for local stub testing).
    pub async fn spawn_with_config(config: g5_api::config::AppConfig) -> Self {
        Self::spawn_with_config_and_cloud(config, None).await
    }

    /// Spawns a TestApp with an injected self-hosted registry token issuer.
    ///
    /// This keeps the HTTP/license flow fully exercised while avoiding live
    /// IAMCredentials calls from integration tests.
    pub async fn spawn_with_self_hosted_registry_token_issuer(
        config: g5_api::config::AppConfig,
        issuer: std::sync::Arc<
            dyn g5_api::services::self_hosted_license_service::RegistryTokenIssuer,
        >,
    ) -> Self {
        Self::spawn_with_config_cloud_and_self_hosted(config, None, Some(issuer)).await
    }

    /// Spawns a TestApp with an injected cloud backend (PR-rev-7-9c).
    ///
    /// Pass a `MockCloudProvider` (as `Arc<dyn CloudProvider>`) to exercise the
    /// POST/DELETE cloud wire-in and the reconciler fully offline. With no
    /// cloud injected, `spawn`/`spawn_with_config` reproduce the 9b DB-only
    /// behavior (`cloud_provider == None`).
    ///
    /// Convenience: the test config gets `enable_cloud_features = true` because
    /// every caller of this helper wants to exercise the cloud wire-in; tests
    /// that want the gate-off behavior build the config explicitly and call
    /// `spawn_with_config_and_cloud`.
    pub async fn spawn_with_cloud(cloud: std::sync::Arc<dyn g5_api::cloud::CloudProvider>) -> Self {
        let mut config = test_config();
        config.enable_cloud_features = true;
        Self::spawn_with_config_and_cloud(config, Some(cloud)).await
    }

    /// Shared spawn implementation that optionally injects a cloud backend.
    pub async fn spawn_with_config_and_cloud(
        config: g5_api::config::AppConfig,
        cloud: Option<std::sync::Arc<dyn g5_api::cloud::CloudProvider>>,
    ) -> Self {
        Self::spawn_with_config_cloud_and_self_hosted(config, cloud, None).await
    }

    /// Spawns a TestApp with an injected runtime-label resolver (PR2, [D43]).
    ///
    /// Pass an `OciRuntimeLabelResolver::for_tests(client)` pointed at a wiremock
    /// `MockServer` so the refresh endpoint resolves against the mock registry
    /// fully offline. With no resolver injected, `spawn`/`spawn_with_config`
    /// keep the production-default `https()` resolver (no network in tests that
    /// never call refresh).
    pub async fn spawn_with_runtime_resolver(
        resolver: std::sync::Arc<dyn g5_api::services::RuntimeLabelResolver>,
    ) -> Self {
        Self::spawn_with_config_cloud_self_hosted_and_resolver(
            test_config(),
            None,
            None,
            Some(resolver),
            None,
            None,
        )
        .await
    }

    async fn spawn_with_config_cloud_and_self_hosted(
        config: g5_api::config::AppConfig,
        cloud: Option<std::sync::Arc<dyn g5_api::cloud::CloudProvider>>,
        self_hosted_registry_token_issuer: Option<
            std::sync::Arc<dyn g5_api::services::self_hosted_license_service::RegistryTokenIssuer>,
        >,
    ) -> Self {
        Self::spawn_with_config_cloud_self_hosted_and_resolver(
            config,
            cloud,
            self_hosted_registry_token_issuer,
            None,
            None,
            None,
        )
        .await
    }

    /// Real tenant connections through a test transport; system/authz pools
    /// remain direct so commit outcomes can be independently observed.
    pub async fn spawn_with_tenant_transport(
        config: g5_api::config::AppConfig,
        rewrite: TenantUrlRewrite,
    ) -> Self {
        Self::spawn_with_config_cloud_self_hosted_and_resolver(
            config,
            None,
            None,
            None,
            Some(rewrite),
            None,
        )
        .await
    }

    async fn spawn_with_config_cloud_self_hosted_and_resolver(
        config: g5_api::config::AppConfig,
        cloud: Option<std::sync::Arc<dyn g5_api::cloud::CloudProvider>>,
        self_hosted_registry_token_issuer: Option<
            std::sync::Arc<dyn g5_api::services::self_hosted_license_service::RegistryTokenIssuer>,
        >,
        runtime_label_resolver: Option<std::sync::Arc<dyn g5_api::services::RuntimeLabelResolver>>,
        tenant_url_rewrite: Option<TenantUrlRewrite>,
        prepared_db: Option<TestDb>,
    ) -> Self {
        let capacity_permit = acquire_test_app_capacity().await;
        let test_db = match prepared_db {
            Some(db) => db,
            None => TestDb::create().await,
        };
        let test_db_pool = test_db.pool.clone();
        let system_url = test_db.system_url.clone();
        let authz_url = test_db.authz_url.clone();
        let tenant_url = tenant_url_rewrite.map_or_else(
            || test_db.tenant_url.clone(),
            |rewrite| rewrite(&test_db.tenant_url),
        );
        let mut lifecycle = TestAppLifecycle {
            server_shutdown: None,
            server_task: None,
            test_db: Some(test_db),
            capacity_permit: Some(capacity_permit),
            dispatch_tunnels: std::sync::Mutex::new(Vec::new()),
        };

        // Bind first so the spawned app advertises a real reachable base URL in
        // workspace manifests and git smart-HTTP responses.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind test server");
        let addr = listener.local_addr().expect("Failed to get local address");
        let base_url = format!("http://{}", addr);

        let mut config = config;
        // Preview isolation tests need a canonical HTTPS same-site origin;
        // requests still use the ephemeral TCP listener through base_url.
        if config.server.preview_host_suffix.is_none() {
            config.server.external_url = base_url.clone();
        }
        config.server.git_external_url = base_url.clone();
        config.database.url = system_url;
        config.authz_database.url = authz_url;
        config.tenant_database.url = tenant_url;

        let system_pool = g5_api::db::init_pool(&config.database)
            .await
            .expect("Failed to connect system pool for test app");
        let authz_pool = g5_api::db::init_pool(&config.authz_database)
            .await
            .expect("Failed to connect authz pool for test app");
        let tenant_pool = g5_api::db::init_pool(&config.tenant_database)
            .await
            .expect("Failed to connect tenant pool for test app");

        // Build the application state and router.
        // NOTE: This uses the crate's public API for building the router.
        // Since integration tests compile as a separate binary, they access
        // g5_api as an external crate.
        let app_state = g5_api::services::AppState::new_for_test_with_pools(
            system_pool.clone(),
            authz_pool.clone(),
            tenant_pool,
            &config,
        )
        .await;
        let repo_storage = app_state.repo_storage.clone();

        // Create the JwtValidator first so it can be shared between AppState
        // (for the WebSocket upgrade handler) and the auth middleware stack.
        let jwt_validator = std::sync::Arc::new(
            JwtValidator::new(TEST_PUBLIC_KEY_PEM, None, None)
                .expect("Failed to create JwtValidator"),
        );

        let mut app_state = app_state;
        app_state.jwt_validator = Some(jwt_validator.clone());
        app_state.jwt_minter = Some(std::sync::Arc::new(
            g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
                .expect("Failed to create test worker session JwtMinter"),
        ));
        // Inject the optional cloud backend (PR-rev-7-9c). `None` keeps the 9b
        // DB-only contract; `Some(MockCloudProvider)` exercises the wire-in.
        // The deployment-level gate `cloud_features_enabled` is driven from the
        // caller's `config` — `spawn_with_cloud` flips it on; tests that want
        // to exercise "provider wired but gate off" build the config directly.
        app_state.cloud_provider = cloud;
        // Inject the optional runtime-label resolver (PR2, [D43]). `None` keeps
        // the production-default `https()` resolver built by
        // `new_for_test_with_pools`; `Some(for_tests(client))` points refresh at
        // a wiremock registry.
        if let Some(resolver) = runtime_label_resolver {
            app_state.runtime_label_resolver = resolver;
        }
        if let Some(issuer) = self_hosted_registry_token_issuer {
            app_state.self_hosted_license_service =
                g5_api::services::SelfHostedLicenseService::new_with_issuer(
                    system_pool.clone(),
                    config.self_hosted.clone(),
                    Some(issuer),
                );
        }
        let state = std::sync::Arc::new(app_state);
        let router = g5_api::routes::build_router(state.clone());

        // Apply middleware stack (auth, CORS, compression, etc.)
        let rate_limit_store =
            std::sync::Arc::new(g5_api::middleware::InMemoryRateLimitStore::new());
        // Wire the production `PgHostAuthValidator` so host_jwt requests hitting
        // `/api/internal/host/*` (the worker_hosts D35/D36/D37 surface) are
        // checked against the real `worker_hosts` row. Without this the middleware
        // fails closed with 500 on every host_jwt — the documented "no validator
        // wired" branch, intended only for unit tests that never present a
        // host_jwt. Integration tests in `api_internal_host.rs` and any future
        // test using the production AuthLayer + a real host_jwt need this hooked.
        let host_validator: std::sync::Arc<dyn g5_api::auth::host_validator::HostAuthValidator> =
            std::sync::Arc::new(g5_api::auth::host_validator::PgHostAuthValidator::new(
                system_pool.clone(),
            ));
        let router = g5_api::middleware::build_middleware_stack(
            router,
            &config,
            jwt_validator,
            rate_limit_store,
            Some(system_pool.clone()),
            Some(host_validator),
        );
        // Spawn the server as a background task and keep a shutdown handle so
        // tests do not leak live listeners or pools across the integration suite.
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server_state = state.clone();
        let server_task = tokio::spawn(async move {
            g5_api::http_server::serve(listener, router, server_state, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("Test server crashed");
        });

        let client = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("Failed to build reqwest client");

        let ws_url = format!("ws://{}", addr);
        lifecycle.server_shutdown = Some(shutdown_tx);
        lifecycle.server_task = Some(server_task);

        Self {
            addr,
            db: test_db_pool,
            system_pool,
            authz_pool,
            client,
            base_url,
            ws_url,
            repo_storage,
            state,
            lifecycle,
        }
    }

    /// A real local route for database-only dispatch tests. No guest execution
    /// is scripted; tests exercising invocation use TestWorkspaceRuntimePeer.
    pub async fn attach_dispatch_host(&self, host_id: uuid::Uuid) {
        use g5_internal_api::stream_mux::Mux;
        let (api_io, host_io) = tokio::io::duplex(256 * 1024);
        let api_mux = Mux::new(api_io, true);
        let mut host_mux = Mux::new(host_io, false);
        let registry = self.state.preview_tunnels.clone();
        let attached = registry.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = attached.attach_test_mux(host_id, api_mux) => {},
                _ = async { while let Some(mut stream) = host_mux.accept().await { stream.close().await; } } => {},
            }
        });
        self.lifecycle.dispatch_tunnels.lock().unwrap().push(task);
        tokio::time::timeout(Duration::from_secs(2), async {
            while registry.get(&host_id).is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("dispatch fixture host route attaches");
    }

    /// The host-control backend the API installs, for tests that drive host
    /// facts directly.
    pub fn host_control_backend(
        &self,
    ) -> g5_api::services::host_control_state::PostgresHostControlState {
        self.host_control_backend_with_pool(self.state.system_pool.clone())
    }

    /// The host-control backend on another system pool.
    pub fn host_control_backend_with_pool(
        &self,
        pool: sqlx::PgPool,
    ) -> g5_api::services::host_control_state::PostgresHostControlState {
        self.host_control_backend_with_pools(pool, self.state.tenant_pool.clone())
    }

    /// The host-control backend on other system and tenant pools.
    pub fn host_control_backend_with_pools(
        &self,
        system_pool: sqlx::PgPool,
        tenant_pool: sqlx::PgPool,
    ) -> g5_api::services::host_control_state::PostgresHostControlState {
        g5_api::services::host_control_state::PostgresHostControlState::new_for_tests(
            system_pool,
            tenant_pool,
            self.state
                .jwt_minter
                .clone()
                .expect("test JWT minter configured"),
            self.state.event_bus.clone(),
            self.state.authz.clone(),
            self.state.agent_work.clone(),
        )
    }

    /// Creates a tenant-scoped DB transaction for the given org using the
    /// same authz/session path as the runtime tenant pool.
    pub async fn scoped_db_for_org(&self, org_id: Uuid) -> ScopedDb {
        let capability = self
            .state
            .authz
            .mint_service_db_capability(
                UserId::from_uuid(Uuid::nil()),
                OrgId::from_uuid(org_id),
                &["*".to_string()],
            )
            .await
            .expect("Failed to mint test tenant DB capability");

        ScopedDb::from_capability(&self.state.tenant_pool, capability)
            .await
            .expect("Failed to create test tenant-scoped DB")
    }

    /// Creates a tenant-scoped DB transaction for an authenticated user who
    /// belongs to the target org. Unlike [`Self::scoped_db_for_org`], this
    /// preserves the real actor identity when production code forks the
    /// request scope into an independent application transaction.
    pub async fn scoped_db_for_user_org(&self, user_id: Uuid, org_id: Uuid) -> ScopedDb {
        let capability = self
            .state
            .authz
            .mint_db_capability(
                UserId::from_uuid(user_id),
                &[OrgId::from_uuid(org_id)],
                &["*".to_string()],
            )
            .await
            .expect("Failed to mint authenticated test tenant DB capability");

        ScopedDb::from_capability(&self.state.tenant_pool, capability)
            .await
            .expect("Failed to create authenticated test tenant-scoped DB")
    }

    /// Creates an `AuthenticatedClient` using the given user's pre-minted JWT.
    ///
    /// The JWT is passed via `Authorization: Bearer {jwt}` header on every
    /// request.
    pub fn authenticated_client(&self, user: &TestUser) -> AuthenticatedClient {
        AuthenticatedClient::new(self.client.clone(), self.base_url.clone(), user.jwt.clone())
    }

    /// Mints a platform WebSocket ticket, opens the event socket, subscribes
    /// to the given channel, and returns a `WsTestClient`.
    pub async fn ws_connect(&self, user: &TestUser, channel: &str) -> WsTestClient {
        let response = self
            .authenticated_client(user)
            .post_json("/api/v1/auth/ws-ticket", &serde_json::json!({}))
            .await;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("websocket ticket response");
        let ticket = body["data"]["ticket"]
            .as_str()
            .expect("platform websocket ticket");
        let url = format!("{}/api/v1/ws?token={ticket}", self.ws_url);
        let mut client = WsTestClient::connect(&url).await;
        client.subscribe(channel).await;
        client
    }

    /// Cleans up the test database.
    ///
    /// Should be called at the end of each test (or in a test cleanup hook).
    pub async fn cleanup(mut self) {
        self.lifecycle.shutdown_server().await;

        self.system_pool.close().await;
        self.authz_pool.close().await;
        self.state.tenant_pool.close().await;

        self.lifecycle.drop_database().await;
    }
}

// ---------------------------------------------------------------------------
// AuthenticatedClient — HTTP client with auth
// ---------------------------------------------------------------------------

/// HTTP client wrapper with JWT authentication pre-configured.
///
/// Automatically prepends `base_url` to relative paths and sets the
/// `Authorization: Bearer {token}` header on every request.
pub struct AuthenticatedClient {
    inner: reqwest::Client,
    base_url: String,
    token: String,
}

impl AuthenticatedClient {
    /// Creates a new authenticated client.
    pub fn new(client: reqwest::Client, base_url: String, token: String) -> Self {
        Self {
            inner: client,
            base_url,
            token,
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// `GET {base_url}{path}` with auth header.
    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.inner
            .get(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .expect("GET request failed")
    }

    /// `POST {base_url}{path}` with JSON body and auth header.
    pub async fn post_json<T: Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        self.inner
            .post(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .expect("POST request failed")
    }

    /// `POST {base_url}{path}` with a caller-provided body and content type.
    pub async fn post_body(
        &self,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> reqwest::Response {
        self.inner
            .post(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", content_type)
            .body(body)
            .send()
            .await
            .expect("POST request failed")
    }

    /// `PUT {base_url}{path}` with a caller-provided body and content type.
    pub async fn put_body(
        &self,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> reqwest::Response {
        self.inner
            .put(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", content_type)
            .body(body)
            .send()
            .await
            .expect("PUT request failed")
    }

    /// `PUT {base_url}{path}` with JSON body and auth header.
    pub async fn put_json<T: Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        self.inner
            .put(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .expect("PUT request failed")
    }

    /// `PATCH {base_url}{path}` with JSON body and auth header.
    pub async fn patch_json<T: Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        self.inner
            .patch(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .expect("PATCH request failed")
    }

    /// `DELETE {base_url}{path}` with auth header.
    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.inner
            .delete(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .expect("DELETE request failed")
    }
}

// ---------------------------------------------------------------------------
// test_config — test-appropriate AppConfig
// ---------------------------------------------------------------------------

/// Returns a pre-built `AppConfig` with test-appropriate defaults.
///
/// Uses the hardcoded ES256 test key pair, disables all OAuth providers,
/// and uses `*` for CORS origins.
pub fn test_config() -> g5_api::config::AppConfig {
    use std::time::Duration;

    g5_api::config::AppConfig {
        server: g5_api::config::ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0, // OS-assigned
            log_level: "warn".to_string(),
            log_format: g5_api::config::LogFormat::Pretty,
            external_url: "http://localhost:0".to_string(),
            git_external_url: "http://localhost:0".to_string(),
            preview_host_suffix: None,
            rate_limit_enabled: false,
            rate_limit_rpm: 10000,
            trusted_proxy_cidrs: vec![],
            trusted_proxy_xff_hops: 1,
        },
        database: g5_api::config::DatabaseConfig {
            url: String::new(), // Filled from TestDb before the app boots
            max_connections: TEST_RUNTIME_DB_POOL_MAX_CONNECTIONS,
            min_connections: TEST_RUNTIME_DB_MIN_CONNECTIONS,
            connect_timeout: TEST_RUNTIME_DB_CONNECT_TIMEOUT,
            idle_timeout: Duration::from_secs(60),
        },
        auth: g5_api::config::AuthConfig {
            jwt_secret: Some(TEST_PRIVATE_KEY_PEM.to_string()),
            jwt_public_key: Some(TEST_PUBLIC_KEY_PEM.to_string()),
            jwt_issuer: "g5-api-test".to_string(),
            jwt_audience: "g5".to_string(),
            access_token_ttl: Duration::from_secs(3600),
            refresh_token_ttl: Duration::from_secs(86400 * 30),
            refresh_token_absolute_ttl: Duration::from_secs(86400 * 90),
            github: Some(g5_api::config::OAuthProviderConfig {
                client_id: "test-github-client-id".to_string(),
                client_secret: "test-github-client-secret".to_string(),
                authorize_url: None,
                token_url: None,
                userinfo_url: None,
            }),
            microsoft: None,
            microsoft_tenant_id: None,
            microsoft_jwks_url: None,
            scm_gitlab_userinfo_url: None,
            scm_gitlab: Some(g5_api::config::OAuthProviderConfig {
                client_id: "test-gitlab-client-id".to_string(),
                client_secret: "test-gitlab-client-secret".to_string(),
                authorize_url: None,
                token_url: None,
                userinfo_url: None,
            }),
            scm_github_api_base_url: "https://api.github.com".to_string(),
            scm_github_web_base_url: "https://github.com".to_string(),
            scm_encryption_key: Some(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            ),
            jwt_private_key: Some(TEST_PRIVATE_KEY_PEM.to_string()),
            cookie_secure: true,
            refresh_cookie_name: "g5_refresh".to_string(),
        },
        cors: g5_api::config::CorsConfig {
            allowed_origins: vec!["http://localhost".to_string()],
        },
        maintenance: g5_api::config::MaintenanceConfig {
            invitation_gc_interval: Duration::from_secs(3600),
            stale_draft_timeout: Duration::from_secs(86400),
            draft_stale_retention: Duration::from_secs(7 * 24 * 3600),
            repo_gc_interval: Duration::from_secs(3600),
            repo_gc_loose_threshold: 500,
            remote_tracking_scheduler_interval: Duration::from_secs(60),
            remote_tracking_advisory_interval: Duration::from_secs(60),
            passive_auto_pull_enabled: true,
        },
        repos: g5_api::config::ReposConfig {
            base_dir: std::path::PathBuf::from("/tmp/g5-test-repos"),
        },
        chat: g5_api::config::ChatConfig {
            response_timeout: Duration::from_secs(30),
        },
        self_hosted: g5_api::config::SelfHostedConfig {
            license_admin_token_sha256: None,
            registry_location: "us-central1".to_string(),
            registry_project: Some("g5-test".to_string()),
            registry_repository: "g5".to_string(),
            registry_puller_service_account: Some(
                "g5-registry-puller@g5-test.iam.gserviceaccount.com".to_string(),
            ),
            registry_token_lifetime: Duration::from_secs(3600),
        },
        is_self_hosted: false,
        enable_cloud_features: false,
        // Existing recovery integration scenarios exercise the enabled
        // rollout posture explicitly. Tests for the production-safe default
        // override this to false per TestApp.
        // Permit local/bare git remotes in tests so the Adopt-Remote bare-path
        // integration scenarios (make_remote_adopt / make_remote_outcomes) can
        // re-point at filesystem-path test remotes. Production defaults to false
        // (fail-closed) — see config.rs `allow_local_git_remotes`.
        allow_local_git_remotes: true,
        platform_issue: g5_api::config::PlatformIssueConfig {
            enabled: false,
            github_repo: None,
            github_app_id: None,
            github_app_installation_id: None,
            github_app_private_key: None,
            github_api_base_url: "https://api.github.com".to_string(),
            github_web_base_url: "https://github.com".to_string(),
        },
        email: g5_api::config::EmailConfig::default(),
        github_app: None,
        approval_policy: g5_contracts::types::ApprovalPolicy::default(),
        authz_database: g5_api::config::DatabaseConfig {
            url: String::new(),
            max_connections: TEST_RUNTIME_DB_POOL_MAX_CONNECTIONS,
            min_connections: TEST_RUNTIME_DB_MIN_CONNECTIONS,
            connect_timeout: TEST_RUNTIME_DB_CONNECT_TIMEOUT,
            idle_timeout: Duration::from_secs(60),
        },
        tenant_database: g5_api::config::DatabaseConfig {
            url: String::new(),
            max_connections: TEST_RUNTIME_DB_POOL_MAX_CONNECTIONS,
            min_connections: TEST_RUNTIME_DB_MIN_CONNECTIONS,
            connect_timeout: TEST_RUNTIME_DB_CONNECT_TIMEOUT,
            idle_timeout: Duration::from_secs(60),
        },
        require_runtime_db_split: true,
        storage: g5_storage::StorageConfig::Local {
            base_dir: std::path::PathBuf::from("/tmp/g5-test-repos"),
            worktree_dir: std::path::PathBuf::from("/tmp/g5-test-repos/.worktrees"),
        },
        attachment_store_dir: None,
    }
}

// ---------------------------------------------------------------------------
// test_jwt_minter — user and host JWTs signed with the test keys
// ---------------------------------------------------------------------------

/// Signs test JWTs with the hardcoded ES256 test key pair.
///
/// Production mints only host and workspace-incarnation tokens through
/// `g5_auth::JwtMinter`; fixture factories also need user access tokens so
/// they can call the API without going through the full auth service.
pub struct TestJwtMinter {
    host: g5_auth::JwtMinter,
    encoding_key: jsonwebtoken::EncodingKey,
}

impl TestJwtMinter {
    /// Mints a user access token for `org_id` carrying `scopes`.
    pub fn mint(
        &self,
        user_id: Uuid,
        org_id: Uuid,
        scopes: Vec<g5_auth::Scope>,
        lifetime_secs: i64,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = chrono::Utc::now().timestamp();
        let claims = JwtClaims {
            sub: user_id.to_string(),
            org: org_id.to_string(),
            scopes,
            iat: now,
            exp: now + lifetime_secs,
            iss: Some("g5-api-test".to_string()),
            aud: Some("g5".to_string()),
            jti: Some(Uuid::new_v4().to_string()),
            token_use: None,
            client: None,
        };
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256),
            &claims,
            &self.encoding_key,
        )
    }

    /// Mints a worker-host token through the production minter.
    pub fn mint_host_jwt(
        &self,
        host_id: Uuid,
        org_id: Uuid,
        lifetime: std::time::Duration,
    ) -> Result<String, impl std::fmt::Debug> {
        self.host.mint_host_jwt(host_id, org_id, lifetime)
    }
}

/// Creates a [`TestJwtMinter`] with the hardcoded test ES256 key pair.
pub fn test_jwt_minter() -> TestJwtMinter {
    TestJwtMinter {
        host: g5_auth::JwtMinter::new(TEST_PRIVATE_KEY_PEM, "g5-api-test", "g5")
            .expect("Failed to create test host JwtMinter"),
        encoding_key: jsonwebtoken::EncodingKey::from_ec_pem(TEST_PRIVATE_KEY_PEM.as_bytes())
            .expect("Failed to parse the test JWT private key"),
    }
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

/// Asserts that a response has the expected status code and a standard JSON
/// error envelope body.
///
/// The error envelope has the structure:
/// ```json
/// { "error": { "code": "...", "message": "...", "details": ..., "requestId": "..." } }
/// ```
///
/// # Panics
/// Panics if the response status does not match `expected_status`, or if the
/// response body does not contain the expected `code` field.
pub async fn assert_json_error(
    response: reqwest::Response,
    expected_status: StatusCode,
    expected_code: &str,
) {
    let status = response.status();
    assert_eq!(
        status, expected_status,
        "Expected status {expected_status}, got {status}"
    );

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse error response as JSON");

    let error = body
        .get("error")
        .expect("Response body missing 'error' field");

    let code = error
        .get("code")
        .and_then(|v| v.as_str())
        .expect("Error object missing 'code' field");
    assert_eq!(
        code, expected_code,
        "Expected error code '{expected_code}', got '{code}'"
    );

    // Verify the error envelope has the required fields
    assert!(
        error.get("message").is_some(),
        "Error object missing 'message' field"
    );
    assert!(
        error.get("requestId").is_some(),
        "Error object missing 'requestId' field"
    );
}

/// Asserts that a response is `200 OK` with a paginated response body,
/// extracts and returns `(data, total)`.
///
/// The paginated response has the structure:
/// ```json
/// { "data": [...], "meta": { "page": 1, "perPage": 50, "total": 123 } }
/// ```
///
/// # Type Parameters
/// * `T` — The type of items in the `data` array. Must implement
///   `DeserializeOwned`.
///
/// # Panics
/// Panics if the response is not `200 OK` or the body doesn't match the
/// expected paginated structure.
pub async fn assert_paginated_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> (Vec<T>, i64) {
    let status = response.status();
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK for paginated response, got {status}"
    );

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse paginated response as JSON");

    let data_value = body
        .get("data")
        .expect("Paginated response missing 'data' field");

    let data: Vec<T> =
        serde_json::from_value(data_value.clone()).expect("Failed to deserialize 'data' array");

    let meta = body
        .get("meta")
        .expect("Paginated response missing 'meta' field");

    let total = meta
        .get("total")
        .and_then(|v| v.as_i64())
        .expect("Meta object missing 'total' field");

    (data, total)
}

/// Find the `startup_script` the mock cloud provider captured for
/// `provision_instance` on a specific host. Shared by the worker-host
/// integration suites (cloud bring-up + rotation) so budget/env assertions
/// never fork the lookup logic.
pub fn provision_instance_startup_script(
    mock: &g5_api::cloud::MockCloudProvider,
    host_id: Uuid,
) -> Option<String> {
    mock.calls().into_iter().find_map(|c| match c {
        g5_api::cloud::MockCall::ProvisionInstance {
            host_id: h,
            startup_script,
            ..
        } if h == host_id => Some(startup_script),
        _ => None,
    })
}

/// Find the per-host `boot_disk_gb` the mock cloud provider captured for
/// `provision_instance` on a specific host (ontology
/// `g5.runtime.placement.host-disk`). Shared by the bring-up and rotation
/// suites so disk-size assertions never fork the lookup logic.
pub fn provision_instance_boot_disk_gb(
    mock: &g5_api::cloud::MockCloudProvider,
    host_id: Uuid,
) -> Option<u32> {
    mock.calls().into_iter().find_map(|c| match c {
        g5_api::cloud::MockCall::ProvisionInstance {
            host_id: h,
            boot_disk_gb,
            ..
        } if h == host_id => Some(boot_disk_gb),
        _ => None,
    })
}

/// Land a single-repository advance through the production branch-commit
/// boundary: `prepare_branch_commit` followed by `land`, which fences
/// updates the projection, compare-and-swaps
/// `refs/heads/<branch>` and publishes the branch hints after commit.
pub async fn finalize_authority_head(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    repo_id: Uuid,
    new_head_commit: &str,
) -> Result<
    g5_api::services::branch_commit::LandedBranchCommit,
    g5_api::services::branch_commit::BranchCommitError,
> {
    use g5_api::services::branch_commit::{prepare_branch_commit, BranchCommitError};
    let protocol = |message: String| BranchCommitError::Database(sqlx::Error::Protocol(message));
    let mut capture_db = app.scoped_db_for_org(org_id).await;
    let target = AuthorityTarget::Workspace { workspace_id };
    let snapshot = g5_api::services::workspace_reader::resolve_committed_snapshot(
        &mut capture_db,
        &app.repo_storage,
        target.clone(),
    )
    .await
    .map_err(|error| protocol(error.to_string()))?;
    if !snapshot.repos.iter().any(|repo| repo.repo_id == repo_id) {
        return Err(protocol(
            "test repository is outside the workspace cut".into(),
        ));
    }
    let repos = snapshot
        .repos
        .into_iter()
        .map(|repo| g5_contracts::types::WorkspaceHeadRepoInput {
            repo_id: repo.repo_id,
            proposed_new_head: if repo.repo_id == repo_id {
                new_head_commit.to_string()
            } else {
                repo.head_commit.clone()
            },
            expected_old_head: repo.head_commit,
        })
        .collect();
    let scope = capture_db.reopener();
    capture_db
        .rollback()
        .await
        .map_err(|error| protocol(error.to_string()))?;
    let prepared =
        prepare_branch_commit(&scope, &app.repo_storage, target, repos, "integration_test")
            .await
            .map_err(BranchCommitError::Database)?;
    let application_db = scope
        .open()
        .await
        .map_err(|error| protocol(error.to_string()))?;
    prepared
        .land(application_db, &app.state.event_bus, &app.repo_storage)
        .await
}

/// Point `refs/heads/<branch>` of a repository's internal mirror at `sha`.
///
/// Every content read resolves its head from this ref (P1.1), so a fixture
/// that seeds an authority head must move that ref. The mirror is initialized when the fixture has not created it.
/// The all-zero SHA removes the ref (an unborn branch). A SHA whose object is
/// absent from the mirror (the fixed fake SHAs some binding tests use) cannot
/// be created through git2, so it is written as a loose ref file; readers
/// report such a ref as-is and fail at the content read, exactly like a
/// database row naming a missing object did before.
pub async fn seed_branch_ref(app: &TestApp, repo_id: Uuid, branch: &str, sha: &str) {
    let repo_path = app.repo_path(repo_id).await;
    let branch = branch.to_string();
    let sha = sha.to_string();
    tokio::task::spawn_blocking(move || {
        let repo = match git2::Repository::open(&repo_path) {
            Ok(repo) => repo,
            Err(_) => {
                std::fs::create_dir_all(&repo_path).expect("create test mirror directory");
                git2::Repository::init(&repo_path).expect("initialize test mirror")
            }
        };
        let refname = format!("refs/heads/{branch}");
        if sha.trim_start_matches('0').is_empty() {
            if let Ok(mut reference) = repo.find_reference(&refname) {
                reference.delete().expect("delete unborn test branch ref");
            }
            return;
        }
        let oid = git2::Oid::from_str(&sha).expect("seeded head must be a git SHA");
        if repo
            .reference(&refname, oid, true, "test: seed authority head")
            .is_err()
        {
            let ref_path = repo.path().join(&refname);
            std::fs::create_dir_all(ref_path.parent().expect("ref parent"))
                .expect("create loose ref directory");
            std::fs::write(&ref_path, format!("{sha}\n")).expect("write loose test ref");
        }
    })
    .await
    .expect("seed test branch ref");
}

/// Seed a fixture's Git ref using canonical membership and derived ref selection.
/// No database head cache or secondary branch identity is involved.
/// Hold a workspace's commit fence, as a landing does, until the returned
/// transaction ends. Readers of a committed cut (`g5.fork.source.cut`) must
/// wait for it or refuse with `WORKSPACE_COMMIT_FENCE_BUSY`.
#[allow(dead_code)]
pub async fn hold_commit_fence(
    app: &TestApp,
    workspace_id: Uuid,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut holder = app.db.begin().await.expect("begin fence holder");
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('g5:workspace-commit:' || $1::text, 0))",
    )
    .bind(workspace_id)
    .execute(&mut *holder)
    .await
    .expect("hold the workspace commit fence");
    holder
}

pub async fn seed_workspace_authority_head(
    app: &TestApp,
    workspace_id: Uuid,
    repo_id: Uuid,
    head_commit: &str,
) {
    let branch_name: String = sqlx::query_scalar(
        "SELECT CASE WHEN w.is_base THEN r.default_branch ELSE w.name END
         FROM workspace_repos m JOIN workspaces w ON w.id = m.workspace_id
         JOIN repos r ON r.id = m.repo_id
         WHERE m.workspace_id = $1 AND m.repo_id = $2",
    )
    .bind(workspace_id)
    .bind(repo_id)
    .fetch_one(&app.db)
    .await
    .expect("workspace repository fixture must exist");
    seed_branch_ref(app, repo_id, &branch_name, head_commit).await;
}
