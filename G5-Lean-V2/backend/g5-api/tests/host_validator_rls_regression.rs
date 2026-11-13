//! Live-Postgres regression for the worker_hosts host-auth lookup path.
//!
//! Codex's round-2 final review on PR #128 commit `5f23800c` flagged that
//! migration 080's `authz.lookup_worker_host_for_auth(UUID)` is `SECURITY
//! DEFINER` but `worker_hosts` carries `FORCE ROW LEVEL SECURITY` (migration
//! 078). FORCE RLS defeats the table-owner exemption that ordinarily makes
//! SECURITY DEFINER bypass the policy: if the function owner is the same role
//! that owns `worker_hosts`, the policy is still evaluated against the
//! function-owner's effective context (`authz.current_org_ids()` empty
//! pre-org-binding) and the function returns zero rows — exactly the
//! production NotFound regression migration 080 was supposed to close.
//!
//! Round-3 reassigns ownership to a dedicated NOLOGIN `BYPASSRLS` role
//! (`authz_lookup_definer`). This test proves the production code path
//! actually works under that ownership model by:
//!
//! 1. Spinning up an isolated test DB with all migrations 001..080 applied.
//! 2. Inserting a `worker_hosts` row using the bootstrap (superuser) pool.
//! 3. Opening a pool as `g5_runtime_system`, the test analog of the
//!    production `g5_app` system pool role — a non-superuser, non-`BYPASSRLS`
//!    login role that inherits `g5_app`.
//! 4. Constructing `PgHostAuthValidator` against that restricted pool and
//!    asserting `validate(host_id, org_id)` returns `Ok(())`.
//! 5. Independently asserting that the same system pool can read the host
//!    registry through migration 008's explicit `g5_app` policy. The control
//!    stream needs that access before any tenant context exists.
//!
//! A side-channel `BYPASSRLS` assertion on the function owner is also
//! included so the test fails loudly if migration 080 ever regresses to a
//! non-bypassing owner.

use crate::helpers;

use g5_api::auth::host_validator::{HostAuthError, HostAuthValidator, PgHostAuthValidator};
use helpers::TestDb;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use uuid::Uuid;

/// The test analog of production's `g5_app`. Defined as a const so changes to
/// the helper bootstrap surface make this regression mis-fire loudly rather
/// than silently fall back to a more privileged role.
const SYSTEM_POOL_USER: &str = "g5_runtime_system";

/// End-to-end proof that:
/// - `PgHostAuthValidator::validate` succeeds against a real Postgres under
///   the restricted system-pool role; AND
/// - the same restricted role can directly read `worker_hosts` through the
///   narrow system-pool policy required by the host-control stream; AND
/// - the SECURITY DEFINER function's owner actually carries the BYPASSRLS
///   attribute (defends against a future migration that hand-edits the owner
///   without round-3's safety assertion firing).
#[tokio::test]
async fn validate_and_system_host_registry_access_succeed_under_rls() {
    let test_db = TestDb::create().await;

    // --- 1. Seed: insert one org + one worker_hosts row via the bootstrap
    // (superuser) pool. Superuser bypasses RLS unconditionally, so no
    // session context is needed for the seed.
    let org_id = Uuid::new_v4();
    sqlx::query("WITH o AS (INSERT INTO orgs (id,name,slug) VALUES ($1,$2,$3) RETURNING id),
             u AS (INSERT INTO users (email,display_name) VALUES ($1::text || '@fixture.test','Fixture Admin') RETURNING id)
             INSERT INTO org_memberships (org_id,user_id,role) SELECT o.id,u.id,'admin' FROM o,u")
        .bind(org_id)
        .bind(format!("rls-regression-org-{}", Uuid::new_v4().simple()))
        .bind(format!("rls-regression-{}", Uuid::new_v4().simple()))
        .execute(&test_db.pool)
        .await
        .expect("seed org");

    let host_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status) \
         VALUES ($1, $2, 'us-central1', 'ready')",
    )
    .bind(host_id)
    .bind(org_id)
    .execute(&test_db.pool)
    .await
    .expect("seed worker_hosts row");

    // --- 2. Defensive precondition: the function owner must have BYPASSRLS,
    // otherwise this test would degenerate into asserting "superuser bypass
    // worked" instead of the round-3 fix. Migration 080 has its own apply-time
    // assertion; this one catches drift in the test DB.
    let owner_row = sqlx::query(
        "SELECT r.rolname::text AS rolname, r.rolbypassrls AS rolbypassrls
         FROM pg_proc p
         JOIN pg_namespace n ON n.oid = p.pronamespace
         JOIN pg_roles r ON r.oid = p.proowner
         WHERE n.nspname = 'authz' AND p.proname = 'lookup_worker_host_for_auth'",
    )
    .fetch_one(&test_db.pool)
    .await
    .expect("function owner lookup");
    let owner_role: String = owner_row.get("rolname");
    let owner_bypassrls: bool = owner_row.get("rolbypassrls");
    assert_eq!(
        owner_role, "authz_lookup_definer",
        "migration 080 must own authz.lookup_worker_host_for_auth via authz_lookup_definer"
    );
    assert!(
        owner_bypassrls,
        "authz_lookup_definer must carry BYPASSRLS for FORCE RLS bypass on worker_hosts"
    );

    // --- 3. Open a pool as the restricted system-pool role.
    // `TestDb::system_url` is wired to `g5_runtime_system`, the test analog
    // of production's `g5_app` (NOLOGIN role inherited by the login role).
    assert!(
        test_db
            .system_url
            .contains(&format!("//{SYSTEM_POOL_USER}:")),
        "TestDb::system_url must connect as {SYSTEM_POOL_USER}; got {}",
        test_db.system_url
    );
    let system_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.system_url)
        .await
        .expect("connect as restricted system-pool role");

    // --- 4. The host-control stream uses this direct path before it has a
    // tenant context. Migration 008 grants `g5_app` only the registry access
    // needed for heartbeats, inventory, and drained-state transitions.
    let rows = sqlx::query("SELECT host_id, org_id, status FROM worker_hosts WHERE host_id = $1")
        .bind(host_id)
        .fetch_all(&system_pool)
        .await
        .expect("system pool reads host registry through its explicit RLS policy");
    assert_eq!(rows.len(), 1, "system pool must see the addressed host row");
    assert_eq!(rows[0].get::<Uuid, _>("host_id"), host_id);
    assert_eq!(rows[0].get::<Uuid, _>("org_id"), org_id);
    assert_eq!(rows[0].get::<String, _>("status"), "ready");

    // --- 5. The PRODUCTION path: validator goes through
    // `authz.lookup_worker_host_for_auth`, which is SECURITY DEFINER and now
    // owned by the BYPASSRLS role. This remains the validator's stable lookup
    // boundary even though the control stream also has narrow direct access.
    let validator = PgHostAuthValidator::new(system_pool.clone());
    let result = validator.validate(host_id, org_id).await;
    assert_eq!(
        result,
        Ok(()),
        "validator must succeed via the BYPASSRLS SECURITY DEFINER function under the \
         restricted system-pool role; got {result:?}"
    );

    // --- 6. Cross-check rejection paths still work under the same pool —
    // these go through the same function so an unrelated regression in the
    // SECURITY DEFINER path would surface here too.
    let unknown_host = Uuid::new_v4();
    let not_found = validator.validate(unknown_host, org_id).await;
    assert_eq!(
        not_found,
        Err(HostAuthError::NotFound),
        "unknown host_id must yield NotFound through the BYPASSRLS path; got {not_found:?}"
    );

    let mismatched_org = Uuid::new_v4();
    let mismatch = validator.validate(host_id, mismatched_org).await;
    assert_eq!(
        mismatch,
        Err(HostAuthError::OrgMismatch),
        "claimed org mismatch must yield OrgMismatch; got {mismatch:?}"
    );

    // --- 7. Status gating over the REAL SECURITY DEFINER path. The
    // `host_auth_middleware_pg_integration` rejections test collapses every
    // rejection arm into one 401 and defers the *distinct* variant semantics to
    // this file. Exercise the live `authz.lookup_worker_host_for_auth` function,
    // not only the `StaticHostAuthValidator` fake. A terminated host is forbidden;
    // draining and unreachable
    // hosts remain authenticated so they can reconnect, replay events, process
    // stop/drain commands, and report host.drained; placement still excludes
    // both states.
    for reconnect_status in ["draining", "unreachable"] {
        let reconnecting_host = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO worker_hosts (host_id, org_id, region, status) \
             VALUES ($1, $2, 'us-central1', $3)",
        )
        .bind(reconnecting_host)
        .bind(org_id)
        .bind(reconnect_status)
        .execute(&test_db.pool)
        .await
        .unwrap_or_else(|e| panic!("seed {reconnect_status} worker_hosts row: {e}"));
        assert_eq!(
            validator.validate(reconnecting_host, org_id).await,
            Ok(()),
            "a {reconnect_status} host must remain authenticated for reconciliation"
        );
    }

    let forbidden_status = "terminated";
    let forbidden_host = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status) \
         VALUES ($1, $2, 'us-central1', $3)",
    )
    .bind(forbidden_host)
    .bind(org_id)
    .bind(forbidden_status)
    .execute(&test_db.pool)
    .await
    .unwrap_or_else(|e| panic!("seed {forbidden_status} worker_hosts row: {e}"));

    let rejected = validator.validate(forbidden_host, org_id).await;
    assert_eq!(
        rejected,
        Err(HostAuthError::StatusForbidden(forbidden_status.to_string())),
        "a {forbidden_status} host (org matches) must be rejected via the status arm as \
         StatusForbidden(\"{forbidden_status}\") through the BYPASSRLS SECURITY DEFINER \
         function — not NotFound/OrgMismatch for the wrong reason; got {rejected:?}"
    );

    system_pool.close().await;
    test_db.drop().await;
}
