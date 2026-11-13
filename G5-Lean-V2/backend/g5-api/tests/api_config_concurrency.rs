//! Per-scope serialization proof for scoped runtime-config writes (issue
//! #1425 follow-up; `config_service.semlift.md` **D7**).
//!
//! ## What this proves
//!
//! D1/D6 promise that a partial scoped write preserves every key the caller did
//! not mention. That merge is computed in memory from a snapshot read, so the
//! promise only holds if writers to the same scope are serialized. D7 makes
//! each `update_*_scope` take a per-scope transaction advisory lock BEFORE the
//! read, so the loser blocks until the winner commits and then merges against
//! committed state.
//!
//! These tests are the RED proof for that fence. Without the lock in
//! `lock_scope_for_update`, writer B reads the pre-A snapshot, finishes
//! immediately (so the `is_finished()` assertion fails), and its upsert
//! resurrects A's pre-change settings — silently dropping A's key.
//!
//! Coverage:
//!
//! 1. settings-vs-settings on the **org** scope, including the loser-side
//!    no-partial-write assertion while both transactions are open;
//! 2. settings-vs-secret on the **user** scope — the two channels merge from
//!    the same snapshot, so they must share one fence;
//! 3. settings-vs-settings on the **workspace** scope driven through the real
//!    HTTP route, so the middleware/`ScopedDb`/commit path is covered too.
//!
//! These need a live Postgres (like every other file in this directory) and run
//! under the standard `cargo nextest run -p g5-api --test integration -E 'test(/^api_config_concurrency::/)'`.

use crate::helpers;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use g5_auth::Scope;
use g5_contracts::error::G5Error;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use g5_api::services::ScopeUpdateInput;
use g5_authz::{DbCapability, ScopedDb};
use g5_contracts::types::ids::{OrgId, SessionId, UserId};

/// The bounded, event-based observation window for writer B to appear as an
/// advisory-lock waiter (or finish with D7's retryable timeout).
const BLOCK_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);
const BLOCK_OBSERVATION_POLL: Duration = Duration::from_millis(20);

/// Fixed observation window retained by the settings-versus-secret scenario
/// below until it adopts the database-visible fence observation as well.
const BLOCK_OBSERVATION: Duration = Duration::from_millis(750);

/// Upper bound for the settings-versus-secret handoff scenario below. Its
/// current join remains a timing guard until that scenario adopts the same
/// event-based retry structure as the org-scope proof.
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(15);

async fn scoped_db(app: &TestApp, org_id: Uuid, actor_id: Uuid) -> ScopedDb {
    let cap = DbCapability {
        org_ids: vec![OrgId::from_uuid(org_id)],
        actor_id: UserId::from_uuid(actor_id),
        scopes: vec!["*".to_string()],
        session_id: SessionId::from_uuid(Uuid::nil()),
        ttl_secs: 3600,
    };
    ScopedDb::unscoped(&app.db, cap)
        .await
        .expect("scoped db must open")
}

/// A settings-only partial update.
fn settings_patch(patch: Value) -> ScopeUpdateInput {
    ScopeUpdateInput {
        settings: Some(patch),
        remove_setting_keys: Vec::new(),
        set_secrets: HashMap::new(),
        remove_secret_keys: Vec::new(),
    }
}

fn expected_org_baseline_settings() -> Value {
    json!({
        "BASELINE": "keep-me",
        "claude_permission_mode": "default",
        "codex_permission_mode": "never/workspace-write"
    })
}

/// A secret-only partial update.
fn secret_patch(key: &str, value: &str) -> ScopeUpdateInput {
    let mut set_secrets = HashMap::new();
    set_secrets.insert(key.to_string(), value.to_string());
    ScopeUpdateInput {
        settings: None,
        remove_setting_keys: Vec::new(),
        set_secrets,
        remove_secret_keys: Vec::new(),
    }
}

/// A single-connection pool onto the same test database, independent of
/// `app.db`.
///
/// `app.db` caps at `TEST_DIRECT_DB_POOL_MAX_CONNECTIONS` (2), which the two
/// contending writers consume entirely. Observing committed state mid-flight
/// therefore needs its own connection — borrowing a third from `app.db` would
/// make the observer (or writer B) block on *pool acquisition* rather than on
/// the advisory lock, turning the proof into a false pass.
async fn observer_pool(app: &TestApp) -> sqlx::PgPool {
    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&app.db)
        .await
        .expect("current database must be readable");
    let base = helpers::TestDb::resolve_base_url().await;
    let (prefix, _) = base
        .rsplit_once('/')
        .expect("base url must carry a database path segment");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&format!("{prefix}/{db_name}"))
        .await
        .expect("observer pool must connect")
}

/// Read committed org settings on the observer connection — deliberately NOT
/// through either writer's open transaction.
async fn committed_org_settings(pool: &sqlx::PgPool, org_id: Uuid) -> Value {
    sqlx::query_scalar::<_, Value>("SELECT settings_json FROM org_configs WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .expect("org config row must exist")
}

/// True once a writer in this test database is waiting on the per-scope
/// advisory fence. Test databases are isolated, so any ungranted advisory lock
/// here is the writer under test rather than another test process.
async fn has_waiting_advisory_lock(pool: &sqlx::PgPool) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_locks
            WHERE locktype = 'advisory'
              AND NOT granted
              AND database = (
                  SELECT oid FROM pg_database WHERE datname = current_database()
              )
        )
        "#,
    )
    .fetch_one(pool)
    .await
    .expect("advisory-lock observation must succeed")
}

async fn committed_user_scope(app: &TestApp, org_id: Uuid, user_id: Uuid) -> (Value, Vec<String>) {
    sqlx::query_as::<_, (Value, Vec<String>)>(
        "SELECT settings_json, secret_keys FROM user_configs WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .expect("user config row must exist")
}

// ===========================================================================
// 1. Settings vs settings — org scope, service level
// ===========================================================================

/// Two overlapping partial settings writes to the same org compose instead of
/// clobbering each other, and neither is partially visible while both
/// transactions are open.
#[tokio::test]
async fn concurrent_org_settings_updates_serialize_and_compose() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;

    // Baseline key that neither writer mentions — it must survive both merges.
    let mut seed = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_org_scope(
            &mut seed,
            org.id,
            user.id,
            settings_patch(json!({ "BASELINE": "keep-me" })),
        )
        .await
        .expect("seed write must succeed");
    seed.commit().await.expect("seed commit must succeed");

    let observer = observer_pool(&app).await;

    // Writer A performs its read-merge-write and HOLDS the transaction open.
    let mut db_a = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_org_scope(
            &mut db_a,
            org.id,
            user.id,
            settings_patch(json!({ "KEY_A": "a" })),
        )
        .await
        .expect("writer A must succeed");

    // Writer B starts the same sequence on its own transaction.
    let state = app.state.clone();
    let pool = app.db.clone();
    let (org_id, actor_id) = (org.id, user.id);
    let (writer_b_started_tx, writer_b_started_rx) = tokio::sync::oneshot::channel();
    let writer_b = tokio::spawn(async move {
        let cap = DbCapability {
            org_ids: vec![OrgId::from_uuid(org_id)],
            actor_id: UserId::from_uuid(actor_id),
            scopes: vec!["*".to_string()],
            session_id: SessionId::from_uuid(Uuid::nil()),
            ttl_secs: 3600,
        };
        let mut db_b = ScopedDb::unscoped(&pool, cap)
            .await
            .expect("writer B scoped db must open");
        writer_b_started_tx
            .send(())
            .expect("writer B start receiver must be live");
        match state
            .config_service
            .update_org_scope(
                &mut db_b,
                org_id,
                actor_id,
                settings_patch(json!({ "KEY_B": "b" })),
            )
            .await
        {
            Ok(_) => db_b
                .commit()
                .await
                .map_err(|err| format!("writer B commit must succeed: {err}")),
            Err(err) => Err(err.to_string()),
        }
    });

    writer_b_started_rx
        .await
        .expect("writer B must start its update");
    let observation_deadline = Instant::now() + BLOCK_OBSERVATION_TIMEOUT;
    while !writer_b.is_finished() && !has_waiting_advisory_lock(&observer).await {
        assert!(
            Instant::now() < observation_deadline,
            "writer B must reach the D7 advisory fence or return its retryable timeout"
        );
        tokio::time::sleep(BLOCK_OBSERVATION_POLL).await;
    }

    // THE load-bearing assertion. Unfenced, B reads the pre-A snapshot, commits
    // KEY_B, and makes that partial state visible here. A D7 timeout is also
    // safe: it commits nothing, leaving the baseline visible for the retry.
    assert_eq!(
        committed_org_settings(&observer, org.id).await,
        expected_org_baseline_settings(),
        "no uncommitted writer may leak a partial settings state"
    );

    db_a.commit().await.expect("writer A commit must succeed");

    let first_attempt = writer_b.await.expect("writer B task must not panic");
    if let Err(error) = first_attempt {
        assert!(
            error.contains("configuration for this scope is being updated concurrently; please retry"),
            "writer B must either serialize or receive the D7 retryable lock-timeout error: {error}"
        );

        // A lock-timeout aborts its transaction without committing. Retry in a
        // fresh transaction after A's committed state is available to merge.
        let mut retry_db = scoped_db(&app, org.id, user.id).await;
        app.state
            .config_service
            .update_org_scope(
                &mut retry_db,
                org.id,
                user.id,
                settings_patch(json!({ "KEY_B": "b" })),
            )
            .await
            .expect("writer B retry must succeed after writer A commits");
        retry_db
            .commit()
            .await
            .expect("writer B retry commit must succeed");
    }

    assert_eq!(
        committed_org_settings(&observer, org.id).await,
        json!({
            "BASELINE": "keep-me",
            "KEY_A": "a",
            "KEY_B": "b",
            "claude_permission_mode": "default",
            "codex_permission_mode": "never/workspace-write"
        }),
        "both concurrent partial writes must survive, along with the untouched \
         baseline key"
    );

    observer.close().await;
    app.cleanup().await;
}

// ===========================================================================
// 2. Settings vs secret — user scope, service level
// ===========================================================================

/// A settings write and a secret write racing on the same user scope must both
/// survive. Both channels merge from the same snapshot row, so one fence has to
/// cover both.
#[tokio::test]
async fn concurrent_user_settings_and_secret_updates_both_survive() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;

    let mut seed = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_user_scope(
            &mut seed,
            org.id,
            user.id,
            user.id,
            settings_patch(json!({ "BASELINE": "keep-me" })),
        )
        .await
        .expect("seed write must succeed");
    seed.commit().await.expect("seed commit must succeed");

    // Writer A sets a settings key and holds its transaction open.
    let mut db_a = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_user_scope(
            &mut db_a,
            org.id,
            user.id,
            user.id,
            settings_patch(json!({ "ANTHROPIC_MODEL": "claude-opus-4-8" })),
        )
        .await
        .expect("writer A must succeed");

    // Writer B rotates a secret on the same scope.
    let state = app.state.clone();
    let pool = app.db.clone();
    let (org_id, user_id) = (org.id, user.id);
    let writer_b = tokio::spawn(async move {
        let cap = DbCapability {
            org_ids: vec![OrgId::from_uuid(org_id)],
            actor_id: UserId::from_uuid(user_id),
            scopes: vec!["*".to_string()],
            session_id: SessionId::from_uuid(Uuid::nil()),
            ttl_secs: 3600,
        };
        let mut db_b = ScopedDb::unscoped(&pool, cap)
            .await
            .expect("writer B scoped db must open");
        state
            .config_service
            .update_user_scope(
                &mut db_b,
                org_id,
                user_id,
                user_id,
                secret_patch("ANTHROPIC_AUTH_TOKEN", "sk-secret-123"),
            )
            .await
            .expect("writer B must succeed");
        db_b.commit().await.expect("writer B commit must succeed");
    });

    tokio::time::sleep(BLOCK_OBSERVATION).await;
    assert!(
        !writer_b.is_finished(),
        "a secret write must contend on the same per-scope fence as a settings \
         write (config_service D7)"
    );

    db_a.commit().await.expect("writer A commit must succeed");
    tokio::time::timeout(HANDOFF_TIMEOUT, writer_b)
        .await
        .expect("writer B must be released once writer A commits")
        .expect("writer B task must not panic");

    let (settings, secret_keys) = committed_user_scope(&app, org.id, user.id).await;
    assert_eq!(
        settings,
        json!({ "BASELINE": "keep-me", "ANTHROPIC_MODEL": "claude-opus-4-8" }),
        "the secret write must not erase the concurrent settings write"
    );
    assert_eq!(
        secret_keys,
        vec!["ANTHROPIC_AUTH_TOKEN".to_string()],
        "the settings write must not erase the concurrent secret write"
    );

    app.cleanup().await;
}

// ===========================================================================
// 3. Settings vs settings — workspace scope, through the real HTTP route
// ===========================================================================

/// Overlapping `PUT /api/v1/workspaces/:id/config` requests each writing a
/// distinct key all survive, covering the route → middleware → `ScopedDb` →
/// service → commit path rather than the service in isolation.
#[tokio::test]
async fn overlapping_workspace_config_puts_preserve_every_key() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&user);
    // The workspace config routes cross-check the caller's org against the
    // workspace's own org, so `orgId` is required on every request.
    let path = format!(
        "/api/v1/workspaces/{}/config?orgId={}",
        workspace.id, org.id
    );

    // Seed a key nobody writes; the merge must keep it.
    let seed = client
        .put_json(&path, &json!({ "settings": { "BASELINE": "keep-me" } }))
        .await;
    assert_eq!(seed.status(), StatusCode::OK);

    // Fire the concurrent writes together so they genuinely overlap.
    let writes = (0..8).map(|i| {
        let client = app.authenticated_client(&user);
        let path = path.clone();
        async move {
            let response = client
                .put_json(
                    &path,
                    &json!({ "settings": { format!("KEY_{i}"): format!("value-{i}") } }),
                )
                .await;
            assert_eq!(response.status(), StatusCode::OK, "PUT {i} must succeed");
        }
    });
    futures::future::join_all(writes).await;

    let response = client.get(&path).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let settings = &body["data"]["settings"];

    assert_eq!(
        settings["BASELINE"], "keep-me",
        "the untouched seed key must survive every concurrent merge"
    );
    for i in 0..8 {
        assert_eq!(
            settings[format!("KEY_{i}")],
            format!("value-{i}"),
            "concurrent PUT {i} was lost — the per-scope fence did not hold \
             (config_service D7)"
        );
    }

    app.cleanup().await;
}

// ===========================================================================
// 4. Bounded lock wait — timeout maps to a retryable error
// ===========================================================================

/// A writer blocked on the per-scope fence must not wait forever holding its
/// connection: once `lock_timeout` fires it aborts with a retryable
/// `ServiceUnavailable` (503), not a hang or a 500 (config_service D7).
///
/// Holds the org scope's advisory lock on a dedicated connection that never
/// commits, so the competing `update_org_scope` must time out on the fence.
/// Uses the production default `lock_timeout` rather than a small
/// `G5_CONFIG_SCOPE_LOCK_TIMEOUT_MS` override on purpose: setting that env var
/// here would leak into the concurrently-running block tests above and make
/// their intended ~750 ms waits abort early. The default (3 s) keeps this test
/// self-contained; it costs one bounded wait.
#[tokio::test]
async fn scope_lock_wait_is_bounded_and_maps_to_retryable_error() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;

    // Seed the row so this exercises a normal (row-exists) update path.
    let mut seed = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_org_scope(
            &mut seed,
            org.id,
            user.id,
            settings_patch(json!({ "BASELINE": "keep-me" })),
        )
        .await
        .expect("seed write must succeed");
    seed.commit().await.expect("seed commit must succeed");

    // Hold the org scope's advisory lock on a dedicated transaction and never
    // release it, so a competing writer's fence acquisition must wait out
    // lock_timeout. Same key formula as `org_scope_lock_key` + D7.
    let holder = observer_pool(&app).await;
    let mut holder_tx = holder.begin().await.expect("holder txn must open");
    let scope_key = format!("g5:config-scope:org:{}", org.id);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(&scope_key)
        .execute(&mut *holder_tx)
        .await
        .expect("holder must acquire the scope lock");

    // The competing update must abort with a retryable error, promptly.
    let mut db = scoped_db(&app, org.id, user.id).await;
    let started = Instant::now();
    let result = app
        .state
        .config_service
        .update_org_scope(
            &mut db,
            org.id,
            user.id,
            settings_patch(json!({ "KEY_X": "x" })),
        )
        .await;
    let elapsed = started.elapsed();
    drop(db);

    let err = result.expect_err("must fail with a retryable error, not succeed or hang");
    assert!(
        matches!(err.inner, G5Error::ServiceUnavailable { .. }),
        "lock timeout must map to a retryable ServiceUnavailable, got: {:?}",
        err.inner
    );
    assert_eq!(
        err.inner.status_code(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the retryable error must surface as HTTP 503"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "the writer must abort near lock_timeout, not hang (waited {elapsed:?})"
    );

    // The stored state is untouched — the timed-out writer committed nothing.
    holder_tx
        .rollback()
        .await
        .expect("holder rollback must succeed");
    assert_eq!(
        committed_org_settings(&holder, org.id).await,
        expected_org_baseline_settings(),
        "a timed-out writer must not have mutated the scope"
    );

    holder.close().await;
    app.cleanup().await;
}

/// The bound also covers the settings/secret upsert, not just the advisory
/// fence: if the scope row is locked by another writer, the `INSERT … ON
/// CONFLICT DO UPDATE` waits on that row lock and, on `lock_timeout`, aborts
/// with the same retryable `ServiceUnavailable` (503) rather than a 500
/// (config_service D7).
///
/// The holder takes a plain `SELECT … FOR UPDATE` row lock — NOT the advisory
/// lock — so the update acquires the fence cleanly and only stalls at the
/// upsert, isolating the row-lock path. Uses the default `lock_timeout` for the
/// same anti-leakage reason as the fence test above.
#[tokio::test]
async fn upsert_row_lock_timeout_also_maps_to_retryable_error() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;

    // Seed the row so the update takes the ON CONFLICT DO UPDATE path and the
    // holder has an existing row to lock.
    let mut seed = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_org_scope(
            &mut seed,
            org.id,
            user.id,
            settings_patch(json!({ "BASELINE": "keep-me" })),
        )
        .await
        .expect("seed write must succeed");
    seed.commit().await.expect("seed commit must succeed");

    // Hold a ROW lock on the scope row (not the advisory lock) and never commit.
    let holder = observer_pool(&app).await;
    let mut holder_tx = holder.begin().await.expect("holder txn must open");
    let locked: Uuid =
        sqlx::query_scalar("SELECT org_id FROM org_configs WHERE org_id = $1 FOR UPDATE")
            .bind(org.id)
            .fetch_one(&mut *holder_tx)
            .await
            .expect("holder must lock the scope row");
    assert_eq!(locked, org.id);

    // The update acquires the advisory fence fine, then blocks at the upsert on
    // the held row lock and must abort with the retryable error.
    let mut db = scoped_db(&app, org.id, user.id).await;
    let started = Instant::now();
    let result = app
        .state
        .config_service
        .update_org_scope(
            &mut db,
            org.id,
            user.id,
            settings_patch(json!({ "KEY_X": "x" })),
        )
        .await;
    let elapsed = started.elapsed();
    drop(db);

    let err = result.expect_err("upsert row-lock timeout must fail, not succeed or hang");
    assert!(
        matches!(err.inner, G5Error::ServiceUnavailable { .. }),
        "an upsert row-lock timeout must map to a retryable ServiceUnavailable, got: {:?}",
        err.inner
    );
    assert_eq!(
        err.inner.status_code(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the retryable error must surface as HTTP 503, not a 500"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "the writer must abort near lock_timeout, not hang (waited {elapsed:?})"
    );

    // The timed-out writer committed nothing.
    holder_tx
        .rollback()
        .await
        .expect("holder rollback must succeed");
    assert_eq!(
        committed_org_settings(&holder, org.id).await,
        expected_org_baseline_settings(),
        "a timed-out writer must not have mutated the scope"
    );

    holder.close().await;
    app.cleanup().await;
}

/// The bound also covers the write's READS, not just the lock-taking writes: a
/// plain `SELECT` takes an `ACCESS SHARE` table lock that waits behind an
/// `ACCESS EXCLUSIVE` holder (e.g. `ALTER TABLE`), so under `lock_timeout` a read
/// can `55P03` too — and it must surface as the same retryable 503, not a 500
/// (config_service D7).
///
/// The holder takes `LOCK TABLE org_configs IN ACCESS EXCLUSIVE MODE` (a table
/// lock, not the advisory lock and not a row lock), so the update clears the
/// advisory fence and then stalls at its very first read (`get_org_scope`).
#[tokio::test]
async fn read_access_share_timeout_also_maps_to_retryable_error() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let user = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;

    // Seed so the scope row exists and the observer read below has content.
    let mut seed = scoped_db(&app, org.id, user.id).await;
    app.state
        .config_service
        .update_org_scope(
            &mut seed,
            org.id,
            user.id,
            settings_patch(json!({ "BASELINE": "keep-me" })),
        )
        .await
        .expect("seed write must succeed");
    seed.commit().await.expect("seed commit must succeed");

    // Hold an ACCESS EXCLUSIVE table lock (blocks the read's ACCESS SHARE), on a
    // dedicated connection that never commits. Not the advisory lock, not a row
    // lock — this isolates the read path.
    let holder = observer_pool(&app).await;
    let mut holder_tx = holder.begin().await.expect("holder txn must open");
    sqlx::query("LOCK TABLE org_configs IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *holder_tx)
        .await
        .expect("holder must take the ACCESS EXCLUSIVE table lock");

    // The update acquires the advisory fence fine, then blocks at its first read
    // and must abort with the retryable error.
    let mut db = scoped_db(&app, org.id, user.id).await;
    let started = Instant::now();
    let result = app
        .state
        .config_service
        .update_org_scope(
            &mut db,
            org.id,
            user.id,
            settings_patch(json!({ "KEY_X": "x" })),
        )
        .await;
    let elapsed = started.elapsed();
    drop(db);

    let err = result.expect_err("read ACCESS SHARE timeout must fail, not succeed or hang");
    assert!(
        matches!(err.inner, G5Error::ServiceUnavailable { .. }),
        "a read table-lock timeout must map to a retryable ServiceUnavailable, got: {:?}",
        err.inner
    );
    assert_eq!(
        err.inner.status_code(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the retryable error must surface as HTTP 503, not a 500"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "the writer must abort near lock_timeout, not hang (waited {elapsed:?})"
    );

    // Release the table lock; the scope is unchanged (the writer committed nothing).
    holder_tx
        .rollback()
        .await
        .expect("holder rollback must succeed");
    assert_eq!(
        committed_org_settings(&holder, org.id).await,
        expected_org_baseline_settings(),
        "a timed-out writer must not have mutated the scope"
    );

    holder.close().await;
    app.cleanup().await;
}
