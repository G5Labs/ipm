//! Provider-account schema proof against live Postgres
//! (`test_contract/provider_accounts` **I12**; migrations leaf **D54**;
//! `system_invariants/external_agent_provider_contract` **D13**, **D14**;
//! `data_model/tenancy_persistence` **D1**, **D2**, **D3**, **D5**, **D6**).
//!
//! ## What this proves
//!
//! * Migrations 292 and 293 apply forward over a database that already holds
//!   quota rows, and every pre-existing snapshot keeps the nil account id.
//! * `provider_accounts` is tenant-isolated: a row written under org A's
//!   session is invisible under org B's session, a tenant-role write with no
//!   session context fails closed with SQLSTATE `42501`, and an insert that
//!   omits `org_id` with no session context fails the same way.
//! * The scope-shape CHECK, the closed `kind` and `scope` vocabularies, the
//!   `https://` base-URL rule, and the case-insensitive per-scope unique name
//!   are enforced by the database, and a name is reusable across scopes.
//! * The re-keyed quota table stores one snapshot per account id, and the
//!   provider-quota service's latest-wins upsert still targets the new key.
//!
//! Runs against live Postgres like every file in this directory:
//! `cargo nextest run -p g5-api --test integration -E 'test(/^api_provider_accounts::/)'`.

use crate::helpers;

use chrono::{DateTime, Duration, SubsecRound, Utc};
use g5_api::models::ProviderQuotaSnapshotInput;
use g5_api::services::agent_dispatch::EnqueueAgentInput;
use g5_contracts::types::{AgentLaunchPreferencesV1, PersistedAgentLaunchV1};
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

const NIL_UUID: Uuid = Uuid::nil();

/// Every kind the migration-292 CHECK admits (contract D14).
const KINDS: [&str; 6] = [
    "claude_subscription",
    "anthropic_api_key",
    "anthropic_compatible_bearer",
    "openrouter_api_key",
    "openai_api_key",
    "chatgpt_subscription",
];

/// One account insert with every column spelled out. `org_id` is passed
/// explicitly (the trigger validates it against the session when one exists).
#[allow(clippy::too_many_arguments)]
async fn insert_account(
    conn: &mut PgConnection,
    org_id: Option<Uuid>,
    scope: &str,
    workspace_id: Option<Uuid>,
    user_id: Option<Uuid>,
    name: &str,
    kind: &str,
    base_url: Option<&str>,
    created_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO provider_accounts
            (org_id, scope, workspace_id, user_id, name, kind, base_url, secret_enc, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(scope)
    .bind(workspace_id)
    .bind(user_id)
    .bind(name)
    .bind(kind)
    .bind(base_url)
    .bind(Some(b"ciphertext".as_slice()))
    .bind(created_by)
    .fetch_one(conn)
    .await
}

/// Insert through a fresh org-scoped tenant transaction and commit on success.
/// A rejected statement poisons its transaction, so every attempt gets its
/// own `ScopedDb`; the failed one rolls back on drop.
#[allow(clippy::too_many_arguments)]
async fn scoped_insert(
    app: &TestApp,
    org_id: Uuid,
    scope: &str,
    workspace_id: Option<Uuid>,
    user_id: Option<Uuid>,
    name: &str,
    kind: &str,
    base_url: Option<&str>,
    created_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let mut db = app.scoped_db_for_org(org_id).await;
    let id = insert_account(
        db.conn(),
        Some(org_id),
        scope,
        workspace_id,
        user_id,
        name,
        kind,
        base_url,
        created_by,
    )
    .await?;
    db.commit().await.expect("commit scoped account insert");
    Ok(id)
}

async fn count_visible(app: &TestApp, org_id: Uuid) -> i64 {
    let mut db = app.scoped_db_for_org(org_id).await;
    let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM provider_accounts")
        .fetch_one(db.conn())
        .await
        .expect("count provider accounts under org session");
    db.rollback().await.expect("rollback read-only scope");
    count
}

fn assert_database_error(
    result: Result<impl Sized, sqlx::Error>,
    expected_code: &str,
    expected_constraint: Option<&str>,
    context: &str,
) {
    let error = match result {
        Ok(_) => panic!("{context}: write unexpectedly succeeded"),
        Err(error) => error,
    };
    let sqlx::Error::Database(error) = error else {
        panic!("{context}: expected database error, got {error}");
    };
    assert_eq!(
        error.code().as_deref(),
        Some(expected_code),
        "{context}: wrong SQLSTATE ({})",
        error.message()
    );
    if let Some(constraint) = expected_constraint {
        assert_eq!(
            error.constraint(),
            Some(constraint),
            "{context}: wrong rejecting constraint"
        );
    }
}

/// I12 step 2: cross-org invisibility and fail-closed writes without a
/// session context (tenancy D2 deny-by-default, D3 trigger `42501`).
#[tokio::test]
async fn accounts_are_invisible_across_orgs_and_fail_closed_without_session() {
    let app = TestApp::spawn().await;
    let org_a = TestOrg::create(&app).await;
    let org_b = TestOrg::create(&app).await;
    let user = TestUser::create(&app).await;

    let account_id = scoped_insert(
        &app,
        org_a.id,
        "org",
        None,
        None,
        "Team key",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await
    .expect("org A inserts its own account");

    assert_eq!(count_visible(&app, org_a.id).await, 1);
    assert_eq!(count_visible(&app, org_b.id).await, 0);

    // Org B cannot read, update, or delete org A's row by id either.
    let mut db_b = app.scoped_db_for_org(org_b.id).await;
    let cross_read =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM provider_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(db_b.conn())
            .await
            .expect("cross-org read executes");
    assert!(cross_read.is_none(), "org B must not see org A's account");
    let cross_delete = sqlx::query("DELETE FROM provider_accounts WHERE id = $1")
        .bind(account_id)
        .execute(db_b.conn())
        .await
        .expect("cross-org delete executes");
    assert_eq!(cross_delete.rows_affected(), 0);
    db_b.rollback().await.expect("rollback org B scope");

    // Tenant role, no session context: the RLS policy matches nothing, so the
    // explicit-org insert fails closed with 42501 and reads return no rows.
    let no_session_insert = insert_account(
        &mut app
            .state
            .tenant_pool
            .acquire()
            .await
            .expect("acquire raw tenant connection"),
        Some(org_a.id),
        "org",
        None,
        None,
        "No session",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        no_session_insert,
        "42501",
        None,
        "tenant-role insert without session context",
    );
    let no_session_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM provider_accounts")
        .fetch_one(&app.state.tenant_pool)
        .await
        .expect("tenant-role count without session");
    assert_eq!(
        no_session_count, 0,
        "no session context must read zero rows"
    );

    // Owner connection, no session, no org_id: the trigger cannot infer the
    // org and raises 42501 (tenancy D3 strict branch).
    let inferred_org_insert = insert_account(
        &mut app.db.acquire().await.expect("acquire owner connection"),
        None,
        "org",
        None,
        None,
        "Inferred org",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        inferred_org_insert,
        "42501",
        None,
        "insert without org_id and without session context",
    );

    // An explicit org_id that contradicts the active session is refused too.
    let mut db_b = app.scoped_db_for_org(org_b.id).await;
    let mismatched = insert_account(
        db_b.conn(),
        Some(org_a.id),
        "org",
        None,
        None,
        "Wrong org",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        mismatched,
        "42501",
        None,
        "org A row written under org B's session",
    );
    drop(db_b);

    app.cleanup().await;
}

/// I12 step 3a: the scope-shape CHECK admits exactly one column shape per
/// scope, and the closed vocabularies plus the base-URL rule hold.
#[tokio::test]
async fn scope_shape_kind_and_base_url_checks_are_enforced() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create(&app).await;

    let shape_rejections: [(&str, Option<Uuid>, Option<Uuid>, &str); 5] = [
        (
            "user",
            Some(workspace.id),
            Some(user.id),
            "user account with workspace_id",
        ),
        ("user", None, None, "user account without user_id"),
        ("org", None, Some(user.id), "org account with user_id"),
        (
            "org",
            Some(workspace.id),
            None,
            "org account with workspace_id",
        ),
        (
            "workspace",
            None,
            None,
            "workspace account without workspace_id",
        ),
    ];
    for (scope, workspace_id, user_id, context) in shape_rejections {
        let result = scoped_insert(
            &app,
            org.id,
            scope,
            workspace_id,
            user_id,
            "Shape",
            "anthropic_api_key",
            None,
            user.id,
        )
        .await;
        assert_database_error(
            result,
            "23514",
            Some("provider_accounts_scope_shape"),
            context,
        );
    }

    let unknown_scope = scoped_insert(
        &app,
        org.id,
        "team",
        None,
        None,
        "Scope",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        unknown_scope,
        "23514",
        Some("provider_accounts_scope_check"),
        "scope outside the closed vocabulary",
    );

    let unknown_kind = scoped_insert(
        &app,
        org.id,
        "org",
        None,
        None,
        "Kind",
        "gemini_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        unknown_kind,
        "23514",
        Some("provider_accounts_kind_check"),
        "kind outside the closed catalog",
    );

    let plain_http = scoped_insert(
        &app,
        org.id,
        "org",
        None,
        None,
        "Base URL",
        "anthropic_compatible_bearer",
        Some("http://api.z.ai/api/anthropic"),
        user.id,
    )
    .await;
    assert_database_error(
        plain_http,
        "23514",
        Some("provider_accounts_base_url_check"),
        "base_url without https://",
    );

    for (blank, context) in [("", "empty name"), ("   ", "blank name")] {
        let result = scoped_insert(
            &app,
            org.id,
            "org",
            None,
            None,
            blank,
            "anthropic_api_key",
            None,
            user.id,
        )
        .await;
        assert_database_error(
            result,
            "23514",
            Some("provider_accounts_name_check"),
            context,
        );
    }
    let too_long = "n".repeat(81);
    let result = scoped_insert(
        &app,
        org.id,
        "org",
        None,
        None,
        &too_long,
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        result,
        "23514",
        Some("provider_accounts_name_check"),
        "name longer than 80 characters",
    );

    // Every catalog kind is admitted at every scope shape.
    for (index, kind) in KINDS.iter().enumerate() {
        let name = format!("Account {index}");
        scoped_insert(&app, org.id, "org", None, None, &name, kind, None, user.id)
            .await
            .unwrap_or_else(|error| panic!("org-scope {kind} must be accepted: {error}"));
        scoped_insert(
            &app,
            org.id,
            "workspace",
            Some(workspace.id),
            None,
            &name,
            kind,
            Some("https://api.example.test"),
            user.id,
        )
        .await
        .unwrap_or_else(|error| panic!("workspace-scope {kind} must be accepted: {error}"));
        scoped_insert(
            &app,
            org.id,
            "user",
            None,
            Some(user.id),
            &name,
            kind,
            None,
            user.id,
        )
        .await
        .unwrap_or_else(|error| panic!("user-scope {kind} must be accepted: {error}"));
    }
    assert_eq!(count_visible(&app, org.id).await, (KINDS.len() * 3) as i64);

    // Definer ownership lookups distinguish ordinary workspaces even across
    // tenant visibility. The composite FK still refuses this mismatched org,
    // through both the tenant and infrastructure connections.
    let other_org = TestOrg::create(&app).await;
    let other_workspace = TestWorkspace::create(&app, &other_org).await;
    let cross_org_workspace = scoped_insert(
        &app,
        org.id,
        "workspace",
        Some(other_workspace.id),
        None,
        "Foreign workspace",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        cross_org_workspace,
        "23503",
        Some("provider_accounts_workspace_same_org"),
        "tenant account bound to another org's hidden workspace",
    );
    let mut operator = app.db.acquire().await.unwrap();
    let cross_org_workspace = insert_account(
        &mut operator,
        Some(org.id),
        "workspace",
        Some(other_workspace.id),
        None,
        "Foreign workspace",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        cross_org_workspace,
        "23503",
        Some("provider_accounts_workspace_same_org"),
        "operator account bound to another org's workspace",
    );
    drop(operator);

    app.cleanup().await;
}

/// I12 step 3b: the unique name index rejects a duplicate (case-insensitive)
/// within one scope and allows the same name across scopes.
#[tokio::test]
async fn account_name_is_unique_per_scope_and_reusable_across_scopes() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other_workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create(&app).await;
    let other_user = TestUser::create(&app).await;

    scoped_insert(
        &app,
        org.id,
        "org",
        None,
        None,
        "Team key",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await
    .expect("first org account");
    let duplicate = scoped_insert(
        &app,
        org.id,
        "org",
        None,
        None,
        "team KEY",
        "openrouter_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        duplicate,
        "23505",
        Some("provider_accounts_scope_name_key"),
        "duplicate name within the org scope",
    );

    scoped_insert(
        &app,
        org.id,
        "workspace",
        Some(workspace.id),
        None,
        "Team key",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await
    .expect("same name at workspace scope");
    let duplicate_workspace = scoped_insert(
        &app,
        org.id,
        "workspace",
        Some(workspace.id),
        None,
        "Team key",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        duplicate_workspace,
        "23505",
        Some("provider_accounts_scope_name_key"),
        "duplicate name within one workspace",
    );
    scoped_insert(
        &app,
        org.id,
        "workspace",
        Some(other_workspace.id),
        None,
        "Team key",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await
    .expect("same name in another workspace");

    scoped_insert(
        &app,
        org.id,
        "user",
        None,
        Some(user.id),
        "Team key",
        "claude_subscription",
        None,
        user.id,
    )
    .await
    .expect("same name at user scope");
    let duplicate_user = scoped_insert(
        &app,
        org.id,
        "user",
        None,
        Some(user.id),
        "Team key",
        "claude_subscription",
        None,
        user.id,
    )
    .await;
    assert_database_error(
        duplicate_user,
        "23505",
        Some("provider_accounts_scope_name_key"),
        "duplicate name for one user",
    );
    scoped_insert(
        &app,
        org.id,
        "user",
        None,
        Some(other_user.id),
        "Team key",
        "claude_subscription",
        None,
        other_user.id,
    )
    .await
    .expect("same name for another user");

    // Another org may reuse the name freely.
    let other_org = TestOrg::create(&app).await;
    scoped_insert(
        &app,
        other_org.id,
        "org",
        None,
        None,
        "Team key",
        "anthropic_api_key",
        None,
        user.id,
    )
    .await
    .expect("same name in another org");

    assert_eq!(count_visible(&app, org.id).await, 5);
    assert_eq!(count_visible(&app, other_org.id).await, 1);

    app.cleanup().await;
}

/// D13 key: one snapshot per account id on the re-keyed quota table, the nil
/// default for account-less rows, and the service's latest-wins upsert against
/// the five-column key.
#[tokio::test]
async fn quota_snapshots_key_by_account_id_with_nil_default() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let user = TestUser::create(&app).await;
    let account_id = scoped_insert(
        &app,
        org.id,
        "user",
        None,
        Some(user.id),
        "Subscription",
        "claude_subscription",
        None,
        user.id,
    )
    .await
    .expect("subscription account");

    // A raw insert without account_id lands on the nil id.
    let first_capture = (Utc::now() - Duration::minutes(10)).trunc_subsecs(6);
    sqlx::query(
        r#"
        INSERT INTO provider_quota_snapshots
            (org_id, workspace_id, provider, auth_mode, five_hour_utilization, captured_at)
        VALUES ($1, $2, 'claude_code', 'anthropic_oauth', 0.25, $3)
        "#,
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(first_capture)
    .execute(&app.db)
    .await
    .expect("account-less snapshot insert");

    let snapshot = |account_id: Uuid, utilization: f32, captured_at: DateTime<Utc>| {
        ProviderQuotaSnapshotInput {
            org_id: org.id,
            workspace_id: workspace.id,
            provider: "claude_code".to_string(),
            auth_mode: "anthropic_oauth".to_string(),
            account_id,
            five_hour_utilization: Some(utilization),
            five_hour_reset_at: None,
            seven_day_utilization: None,
            seven_day_reset_at: None,
            representative_claim: None,
            unified_status: None,
            captured_at,
        }
    };
    let service = app.state.provider_quota_service;

    // Same org/workspace/provider/auth_mode, different account: a second row.
    let mut db = app.scoped_db_for_org(org.id).await;
    let stored = service
        .upsert_snapshot(&mut db, snapshot(account_id, 0.5, first_capture))
        .await
        .expect("per-account snapshot upsert");
    assert!(stored, "first per-account snapshot must be stored");
    db.commit().await.expect("commit per-account snapshot");

    // Newer capture for the same account replaces; an older one is ignored.
    let mut db = app.scoped_db_for_org(org.id).await;
    let newer = service
        .upsert_snapshot(
            &mut db,
            snapshot(account_id, 0.75, first_capture + Duration::minutes(5)),
        )
        .await
        .expect("newer per-account upsert");
    assert!(
        newer,
        "a strictly newer capture must replace the stored row"
    );
    let older = service
        .upsert_snapshot(
            &mut db,
            snapshot(account_id, 0.9, first_capture - Duration::minutes(5)),
        )
        .await
        .expect("older per-account upsert");
    assert!(!older, "an older capture must be ignored");
    db.commit().await.expect("commit newer snapshot");

    // The account-less fetch still resolves the nil row, untouched by the
    // per-account writes.
    let mut db = app.scoped_db_for_org(org.id).await;
    let legacy = service
        .fetch_snapshot(
            &mut db,
            org.id,
            workspace.id,
            "claude_code",
            "anthropic_oauth",
            Uuid::nil(),
        )
        .await
        .expect("fetch account-less snapshot")
        .expect("account-less snapshot exists");
    assert_eq!(legacy.account_id, NIL_UUID);
    assert_eq!(legacy.five_hour_utilization, Some(0.25));
    db.rollback().await.expect("rollback read scope");

    let rows = sqlx::query(
        r#"
        SELECT account_id, five_hour_utilization
        FROM provider_quota_snapshots
        WHERE org_id = $1 AND workspace_id = $2
        ORDER BY account_id
        "#,
    )
    .bind(org.id)
    .bind(workspace.id)
    .fetch_all(&app.db)
    .await
    .expect("read both snapshot rows");
    assert_eq!(rows.len(), 2, "one row per account id under the same key");
    let by_account: Vec<(Uuid, f32)> = rows
        .iter()
        .map(|row| (row.get("account_id"), row.get("five_hour_utilization")))
        .collect();
    assert!(by_account.contains(&(NIL_UUID, 0.25)));
    assert!(by_account.contains(&(account_id, 0.75)));

    app.cleanup().await;
}

// ===========================================================================
// HTTP surface — `test_contract/provider_accounts` **I1** and **I2**
// (`routes/provider_accounts` D1–D5; `provider_account_service` D2–D8;
// `security_policy/secret_custody_and_redaction` D1, D9, D10;
// `security_policy/role_based_access_control` D9).
//
// Every scenario drives the real `/api/v1/orgs/:orgId/provider-accounts`
// routes through authenticated HTTP against live Postgres. Membership roles
// are seeded directly so the service's current-database-state authority
// checks (not the JWT snapshot) are what decide each outcome.
// ===========================================================================

use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::{json, Value};

/// Anthropic API-key-shaped org secret (eligible: ≥ 24 chars, single line).
const ORG_API_KEY: &str = "sk-ant-api03-0123456789abcdefghijklmnopqrstuv";
const ORG_API_KEY_HINT: &str = "sk-ant-api03";
/// Compatible-endpoint bearer stored on the workspace account.
const WS_BEARER: &str = "zai-bearer-0123456789abcdefghijklmnopqrstuvwxyz";
const WS_BEARER_HINT: &str = "zai-bearer-0";
/// Subscription-OAuth-shaped user secret.
const USER_OAUTH: &str = "sk-ant-oat01-0123456789abcdefghijklmnopqrstuv";
const USER_OAUTH_HINT: &str = "sk-ant-oat01";
/// Rotated value with a different 12-character prefix.
const ROTATED_KEY: &str = "sk-or-v1-0123456789abcdef0123456789abcdef";
const ROTATED_KEY_HINT: &str = "sk-or-v1-012";
/// An Anthropic-shaped rotation for an `anthropic_api_key` row on the
/// provider's own host (contract `validate_secret`); its hint equals the
/// seed key's, so rotation is checked through the sanctioned-hint sweep.
const ROTATED_ANTHROPIC_KEY: &str = "sk-ant-api03-fedcba9876543210fedcba9876543210";
/// Below the 24-character eligibility floor: stored, but no hint.
const SHORT_KEY: &str = "short-secret-1";

fn accounts_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/provider-accounts")
}

fn account_path(org_id: Uuid, account_id: &str) -> String {
    format!("/api/v1/orgs/{org_id}/provider-accounts/{account_id}")
}

/// Every provider-account response — success and error — must be
/// `Cache-Control: no-store` (routes/provider_accounts D5).
fn assert_no_store(response: &reqwest::Response, context: &str) {
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store"),
        "{context}: provider-account responses must be Cache-Control: no-store"
    );
}

/// Walk every string in the body and assert the sanctioned 12-character
/// prefix appears only under a `secretHint` key.
fn assert_prefix_confined_to_hint(value: &Value, prefix: &str, path: &str) {
    match value {
        Value::String(s) => assert!(
            !s.contains(prefix),
            "secret-derived prefix leaked outside secretHint at {path}"
        ),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_prefix_confined_to_hint(item, prefix, &format!("{path}[{i}]"));
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                if key == "secretHint" {
                    continue;
                }
                assert_prefix_confined_to_hint(item, prefix, &format!("{path}.{key}"));
            }
        }
        _ => {}
    }
}

/// No response field may carry secret material beyond the sanctioned hint
/// (secret_custody D9 required evidence; the D5 "Provider account
/// credentials" control row): never the full value, never 13+ leading
/// characters, never the remainder, and the prefix only inside `secretHint`.
fn assert_only_sanctioned_hints(body: &Value, secrets: &[&str]) {
    let serialized = body.to_string();
    for secret in secrets {
        assert!(
            !serialized.contains(secret),
            "response must never carry a full secret value"
        );
        let thirteen: String = secret.chars().take(13).collect();
        assert!(
            !serialized.contains(&thirteen),
            "response must never carry more than the first 12 characters of a secret"
        );
        let remainder: String = secret.chars().skip(12).collect();
        if remainder.chars().count() >= 8 {
            assert!(
                !serialized.contains(&remainder),
                "response must never carry the secret remainder"
            );
        }
        let prefix: String = secret.chars().take(12).collect();
        assert_prefix_confined_to_hint(body, &prefix, "body");
    }
}

/// A member holding `workspaces:read` + `workspaces:write` (role `member`).
async fn seed_member(app: &TestApp, org: &TestOrg) -> TestUser {
    TestUser::create_with_scopes(
        app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await
}

/// An org admin: the JWT carries `org:admin`, and — what the service actually
/// consults — the membership row says `admin` (role_based_access_control D9).
async fn seed_admin(app: &TestApp, org: &TestOrg) -> TestUser {
    let user = TestUser::create_with_scopes(
        app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::OrgAdmin,
        ],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = 'admin' WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .expect("promote membership to admin");
    user
}

async fn error_body(response: reqwest::Response, context: &str) -> (StatusCode, Value) {
    assert_no_store(&response, context);
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .unwrap_or_else(|error| panic!("{context}: error body is JSON: {error}"));
    (status, body)
}

fn details_code(body: &Value) -> &str {
    body["error"]["details"]["code"]
        .as_str()
        .unwrap_or_default()
}

fn error_message(body: &Value) -> &str {
    body["error"]["message"].as_str().unwrap_or_default()
}

async fn create_account(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    body: Value,
    context: &str,
) -> Value {
    let response = client.post_json(&accounts_path(org_id), &body).await;
    assert_no_store(&response, context);
    let status = response.status();
    let json: Value = response.json().await.expect("create response is JSON");
    assert_eq!(status, StatusCode::CREATED, "{context}: {json}");
    json["data"].clone()
}

async fn create_expect(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    body: Value,
    expected_status: StatusCode,
    expected_code: &str,
    context: &str,
) {
    let response = client.post_json(&accounts_path(org_id), &body).await;
    let (status, json) = error_body(response, context).await;
    assert_eq!(status, expected_status, "{context}: {json}");
    if !expected_code.is_empty() {
        assert_eq!(details_code(&json), expected_code, "{context}: {json}");
    }
}

async fn list_accounts(
    client: &helpers::AuthenticatedClient,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    context: &str,
) -> Value {
    let path = match workspace_id {
        Some(ws) => format!("{}?workspaceId={ws}", accounts_path(org_id)),
        None => accounts_path(org_id),
    };
    let response = client.get(&path).await;
    assert_no_store(&response, context);
    let status = response.status();
    let json: Value = response.json().await.expect("list response is JSON");
    assert_eq!(status, StatusCode::OK, "{context}: {json}");
    json
}

fn ids_of(list: &Value) -> Vec<String> {
    list["data"]
        .as_array()
        .expect("list data is an array")
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect()
}

fn scopes_of(list: &Value) -> Vec<String> {
    list["data"]
        .as_array()
        .expect("list data is an array")
        .iter()
        .map(|row| row["scope"].as_str().unwrap().to_string())
        .collect()
}

/// I1 steps 1–6: scope visibility across two workspaces and two members, the
/// `no-store` header, the exact 12-character hint and nothing beyond it, the
/// unreadable-workspace masking, and the cross-org 404.
#[tokio::test]
async fn accounts_are_visible_only_inside_their_scope_and_never_expose_credentials() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let member_one = seed_member(&app, &org).await;
    let member_two = seed_member(&app, &org).await;
    let workspace_a = TestWorkspace::create(&app, &org).await;
    let workspace_b = TestWorkspace::create(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let one = app.authenticated_client(&member_one);
    let two = app.authenticated_client(&member_two);

    // Step 2: one account per scope.
    let org_account = create_account(
        &admin_client,
        org.id,
        json!({
            "name": "Team key",
            "kind": "anthropic_api_key",
            "scope": "org",
            "secret": ORG_API_KEY
        }),
        "admin creates org account",
    )
    .await;
    assert_eq!(org_account["scope"], "org");
    assert_eq!(org_account["family"], "claude");
    assert_eq!(org_account["orgId"], org.id.to_string());
    assert!(org_account["workspaceId"].is_null());
    assert!(org_account["userId"].is_null());
    assert_eq!(org_account["hasSecret"], true);
    assert_eq!(org_account["secretHint"], ORG_API_KEY_HINT);
    assert!(org_account["usage"].is_null());
    assert_eq!(org_account["createdBy"], admin.id.to_string());

    let ws_account = create_account(
        &admin_client,
        org.id,
        json!({
            "name": "Workspace A bearer",
            "kind": "anthropic_compatible_bearer",
            "scope": "workspace",
            "workspaceId": workspace_a.id,
            "baseUrl": "https://api.z.ai/api/anthropic",
            "secret": WS_BEARER
        }),
        "admin creates workspace A account",
    )
    .await;
    assert_eq!(ws_account["scope"], "workspace");
    assert_eq!(ws_account["workspaceId"], workspace_a.id.to_string());
    assert_eq!(ws_account["baseUrl"], "https://api.z.ai/api/anthropic");
    assert_eq!(ws_account["secretHint"], WS_BEARER_HINT);

    let user_account = create_account(
        &one,
        org.id,
        json!({
            "name": "My subscription",
            "kind": "claude_subscription",
            "scope": "user",
            "secret": USER_OAUTH
        }),
        "member one creates user account",
    )
    .await;
    assert_eq!(user_account["scope"], "user");
    assert_eq!(user_account["userId"], member_one.id.to_string());
    assert_eq!(user_account["secretHint"], USER_OAUTH_HINT);

    let org_id = org_account["id"].as_str().unwrap().to_string();
    let ws_id = ws_account["id"].as_str().unwrap().to_string();
    let user_id = user_account["id"].as_str().unwrap().to_string();
    let secrets = [ORG_API_KEY, WS_BEARER, USER_OAUTH];
    for created in [&org_account, &ws_account, &user_account] {
        assert_only_sanctioned_hints(created, &secrets);
    }

    // Steps 3–5: the visibility matrix, sorted org → workspace → user.
    let one_from_a = list_accounts(&one, org.id, Some(workspace_a.id), "member one from A").await;
    assert_eq!(
        ids_of(&one_from_a),
        vec![org_id.clone(), ws_id.clone(), user_id.clone()]
    );
    assert_eq!(scopes_of(&one_from_a), vec!["org", "workspace", "user"]);
    assert_only_sanctioned_hints(&one_from_a, &secrets);
    let hints: Vec<&str> = one_from_a["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["secretHint"].as_str().unwrap())
        .collect();
    assert_eq!(
        hints,
        vec![ORG_API_KEY_HINT, WS_BEARER_HINT, USER_OAUTH_HINT]
    );

    let one_from_b = list_accounts(&one, org.id, Some(workspace_b.id), "member one from B").await;
    assert_eq!(ids_of(&one_from_b), vec![org_id.clone(), user_id.clone()]);
    assert_only_sanctioned_hints(&one_from_b, &secrets);

    let two_from_a = list_accounts(&two, org.id, Some(workspace_a.id), "member two from A").await;
    assert_eq!(ids_of(&two_from_a), vec![org_id.clone(), ws_id.clone()]);
    assert_only_sanctioned_hints(&two_from_a, &secrets);

    // No workspace filter: org + own user accounts only.
    let one_plain = list_accounts(&one, org.id, None, "member one without workspace").await;
    assert_eq!(ids_of(&one_plain), vec![org_id.clone(), user_id.clone()]);

    // Personal account metadata follows the personal workspace audience.
    let personal = TestWorkspace::create_personal(&app, &org, member_one.id).await;
    let personal_account = create_account(
        &one,
        org.id,
        json!({
            "name": "Personal workspace account", "kind": "anthropic_compatible_bearer",
            "scope": "workspace", "workspaceId": personal.id,
            "baseUrl": "https://api.z.ai/api/anthropic", "secret": WS_BEARER
        }),
        "create personal account",
    )
    .await;
    let private_ws_id = personal_account["id"].as_str().unwrap().to_owned();
    let two_from_restricted = list_accounts(
        &two,
        org.id,
        Some(personal.id),
        "member two from restricted A",
    )
    .await;
    assert_eq!(ids_of(&two_from_restricted), vec![org_id.clone()]);
    let one_from_restricted = list_accounts(
        &one,
        org.id,
        Some(personal.id),
        "member one from restricted A",
    )
    .await;
    assert_eq!(
        ids_of(&one_from_restricted),
        vec![org_id.clone(), private_ws_id.clone(), user_id.clone()]
    );
    let masked = two
        .patch_json(
            &account_path(org.id, &private_ws_id),
            &json!({ "name": "Renamed" }),
        )
        .await;
    let (status, body) =
        error_body(masked, "member two patches unreadable workspace account").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Another member's user account is invisible, not forbidden.
    let foreign_user = two
        .patch_json(
            &account_path(org.id, &user_id),
            &json!({ "name": "Renamed" }),
        )
        .await;
    let (status, body) = error_body(foreign_user, "member two patches member one's account").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // Step 6: a different org sees the workspace account as 404 through its
    // own org path — never a 403 that would confirm the id exists.
    let other_org = TestOrg::create(&app).await;
    let other_admin = seed_admin(&app, &other_org).await;
    let other_client = app.authenticated_client(&other_admin);
    let cross_org = other_client
        .patch_json(
            &account_path(other_org.id, &ws_id),
            &json!({ "name": "Stolen" }),
        )
        .await;
    let (status, body) = error_body(cross_org, "cross-org patch").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let cross_org_delete = other_client
        .delete(&account_path(other_org.id, &ws_id))
        .await;
    let (status, body) = error_body(cross_org_delete, "cross-org delete").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let other_list = list_accounts(&other_client, other_org.id, None, "other org list").await;
    assert!(ids_of(&other_list).is_empty());

    // The rows themselves are untouched.
    assert_eq!(count_visible(&app, org.id).await, 4);

    app.cleanup().await;
}

/// Ordinary account metadata is readable across owners. Transfer still
/// revokes the former owner's configuration authority and secret access.
#[tokio::test]
async fn a_former_creator_reads_metadata_but_cannot_manage_a_transferred_workspaces_accounts() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    // The first member becomes the fixture workspace's owner and creator.
    let creator = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let next_owner = seed_member(&app, &org).await;
    let creator_client = app.authenticated_client(&creator);
    let next_client = app.authenticated_client(&next_owner);

    let account = create_account(
        &creator_client,
        org.id,
        json!({
            "name": "Workspace bearer",
            "kind": "anthropic_compatible_bearer",
            "scope": "workspace",
            "workspaceId": workspace.id,
            "baseUrl": "https://api.z.ai/api/anthropic",
            "secret": WS_BEARER
        }),
        "owner creates a workspace account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();

    // The real transfer: no VM is attached, so it completes at once.
    let transferred = creator_client
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = transferred.status();
    let transferred: Value = transferred.json().await.expect("transfer body");
    assert_eq!(status, StatusCode::OK, "transfer: {transferred}");
    assert_eq!(transferred["data"]["status"], "completed", "{transferred}");
    let (owner, created_by): (Uuid, Uuid) =
        sqlx::query_as("SELECT owner_user_id, created_by FROM workspaces WHERE id = $1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .expect("workspace ownership");
    assert_eq!(
        (owner, created_by),
        (next_owner.id, creator.id),
        "the row still names its creator"
    );

    // The former creator is neither owner nor member.
    let listed = list_accounts(
        &creator_client,
        org.id,
        Some(workspace.id),
        "former creator lists the workspace's accounts",
    )
    .await;
    assert!(ids_of(&listed).contains(&account_id), "{listed}");
    let redirected = creator_client
        .patch_json(
            &account_path(org.id, &account_id),
            &json!({ "baseUrl": "https://attacker.example/anthropic" }),
        )
        .await;
    let (status, body) = error_body(redirected, "former creator patches").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let deleted = creator_client
        .delete(&account_path(org.id, &account_id))
        .await;
    let (status, body) = error_body(deleted, "former creator deletes").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    create_expect(
        &creator_client,
        org.id,
        json!({
            "name": "Former creator's bearer",
            "kind": "anthropic_compatible_bearer",
            "scope": "workspace",
            "workspaceId": workspace.id,
            "baseUrl": "https://api.z.ai/api/anthropic",
            "secret": WS_BEARER
        }),
        StatusCode::FORBIDDEN,
        "",
        "former creator adds a workspace account",
    )
    .await;

    // The new owner reads and manages it, and nothing above changed it.
    let next_listed = list_accounts(
        &next_client,
        org.id,
        Some(workspace.id),
        "new owner lists the workspace's accounts",
    )
    .await;
    assert_eq!(ids_of(&next_listed), vec![account_id.clone()]);
    let renamed = next_client
        .patch_json(
            &account_path(org.id, &account_id),
            &json!({ "name": "Renamed by the new owner" }),
        )
        .await;
    assert_eq!(renamed.status(), StatusCode::OK, "new owner renames");
    let base_url: String =
        sqlx::query_scalar("SELECT base_url FROM provider_accounts WHERE id = $1::uuid")
            .bind(&account_id)
            .fetch_one(&app.db)
            .await
            .expect("account base URL");
    assert_eq!(base_url, "https://api.z.ai/api/anthropic");

    app.cleanup().await;
}

/// #577: read access and workspaces:write (including an org-admin role) do
/// not authorize changing the owner's stored credential or its destination.
#[tokio::test]
async fn only_current_owner_can_mutate_workspace_configuration() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let member = seed_member(&app, &org).await;
    let admin = seed_admin(&app, &org).await;
    let owner_client = app.authenticated_client(&owner);
    let create = json!({
        "name": "Owner credential", "kind": "anthropic_compatible_bearer",
        "scope": "workspace", "workspaceId": workspace.id,
        "baseUrl": "https://api.z.ai/api/anthropic", "secret": WS_BEARER
    });
    let account = create_account(&owner_client, org.id, create.clone(), "owner creates").await;
    let id = account["id"].as_str().unwrap();
    let path = account_path(org.id, id);
    let config_path = workspace_config_path(org.id, workspace.id);
    let original: (String, String, Vec<u8>) = sqlx::query_as(
        "SELECT name, base_url, secret_enc FROM provider_accounts WHERE id = $1::uuid",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    for caller in [&member, &admin] {
        let client = app.authenticated_client(caller);
        let listed = list_accounts(&client, org.id, Some(workspace.id), "non-owner reads").await;
        assert!(ids_of(&listed).contains(&id.to_string()));
        create_expect(
            &client,
            org.id,
            create.clone(),
            StatusCode::FORBIDDEN,
            "",
            "non-owner creates",
        )
        .await;
        for patch in [
            json!({"baseUrl": "https://attacker.example/anthropic"}),
            json!({"baseUrl": "https://attacker.example/anthropic", "secret": ""}),
            json!({"name": "Changed", "secret": ROTATED_KEY}),
        ] {
            let (status, body) =
                error_body(client.patch_json(&path, &patch).await, "non-owner patches").await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        }
        let (status, body) = error_body(client.delete(&path).await, "non-owner deletes").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        let response = client
            .put_json(
                &config_path,
                &json!({"settings": {"CUSTOM_ENV": "attacker"}}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let stored: (String, String, Vec<u8>) = sqlx::query_as(
        "SELECT name, base_url, secret_enc FROM provider_accounts WHERE id = $1::uuid",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        stored, original,
        "denied operations preserve destination and ciphertext"
    );
    assert_eq!(count_visible(&app, org.id).await, 1);

    assert_eq!(
        owner_client
            .put_json(&config_path, &json!({"settings": {"CUSTOM_ENV": "owner"}}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        owner_client
            .patch_json(
                &path,
                &json!({"name": "Owner updated", "secret": ROTATED_KEY})
            )
            .await
            .status(),
        StatusCode::OK
    );

    // The workspace remains org-visible after transfer: the former owner
    // can still read the account, but cannot mutate it or raw runtime config.
    let transfer = owner_client.post_json(
        &format!("/api/v1/workspaces/{}/transfer", workspace.id),
        &json!({"toUserId": member.id, "idempotencyKey": format!("transfer-{}", Uuid::new_v4())}),
    ).await;
    assert_eq!(transfer.status(), StatusCode::OK);
    assert_eq!(
        transfer.json::<Value>().await.unwrap()["data"]["status"],
        "completed"
    );
    let listed = list_accounts(
        &owner_client,
        org.id,
        Some(workspace.id),
        "former owner reads",
    )
    .await;
    assert!(ids_of(&listed).contains(&id.to_string()));
    assert_eq!(
        owner_client
            .patch_json(&path, &json!({"baseUrl": "https://attacker.example"}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        owner_client.delete(&path).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        owner_client
            .put_json(
                &config_path,
                &json!({"settings": {"CUSTOM_ENV": "old owner"}})
            )
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let next_client = app.authenticated_client(&member);
    assert_eq!(
        next_client
            .patch_json(&path, &json!({"name": "New owner updated"}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        next_client
            .put_json(
                &config_path,
                &json!({"settings": {"CUSTOM_ENV": "new owner"}})
            )
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        next_client.delete(&path).await.status(),
        StatusCode::NO_CONTENT
    );
    app.cleanup().await;
}

async fn wait_for_authority_lock(app: &TestApp, holder_pid: i32) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND $1 = ANY(pg_blocking_pids(pid)))",
            )
            .bind(holder_pid)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("competing request must reach the held authority lock");
}

/// Admission holds authority until commit. Conversely a mutation waiting
/// behind a transfer must observe its newly committed pending state.
#[tokio::test]
async fn account_mutations_serialize_with_ownership_transfer() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let owner = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let next_owner = seed_member(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let mut writing = app.scoped_db_for_org(org.id).await;
    let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(writing.conn())
        .await
        .unwrap();
    let account = app
        .state
        .provider_account_service
        .create(
            &mut writing,
            org.id,
            owner.id,
            g5_api::services::CreateProviderAccount {
                name: "Before transfer".into(),
                kind: "anthropic_compatible_bearer".into(),
                scope: "workspace".into(),
                workspace_id: Some(workspace.id),
                base_url: Some("https://api.z.ai/api/anthropic".into()),
                secret: Some(WS_BEARER.into()),
            },
        )
        .await
        .unwrap();
    let transfer_path = format!("/api/v1/workspaces/{}/transfer", workspace.id);
    let transfer = tokio::spawn(async move {
        client.post_json(&transfer_path, &json!({"toUserId": next_owner.id, "idempotencyKey": format!("transfer-{}", Uuid::new_v4())})).await
    });
    wait_for_authority_lock(&app, writer_pid).await;
    assert!(!transfer.is_finished());
    writing.commit().await.unwrap();
    assert_eq!(transfer.await.unwrap().status(), StatusCode::OK);

    // Pause the next transfer after it locks the workspace and before its
    // pending row commits. This exercises the snapshot taken after waiting.
    let mut transferring = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *transferring)
        .await
        .unwrap();
    let transfer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *transferring)
        .await
        .unwrap();
    let caller = app.authenticated_client(&next_owner);
    let path = account_path(org.id, &account.id.to_string());
    let racing_path = path.clone();
    let mutation = tokio::spawn(async move {
        caller
            .patch_json(
                &racing_path,
                &json!({"baseUrl": "https://attacker.example"}),
            )
            .await
    });
    wait_for_authority_lock(&app, transfer_pid).await;
    sqlx::query("INSERT INTO workspace_ownership_transfers(id,org_id,workspace_id,from_user_id,to_user_id,requested_by,idempotency_key,request_digest,status) VALUES($1,$2,$3,$4,$5,$4,'config-transfer',repeat('a',64),'pending')")
        .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(next_owner.id).bind(owner.id)
        .execute(&mut *transferring).await.unwrap();
    transferring.commit().await.unwrap();
    let response = mutation.await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let next_client = app.authenticated_client(&next_owner);
    assert_eq!(
        next_client.delete(&path).await.status(),
        StatusCode::CONFLICT
    );
    create_expect(&next_client, org.id, json!({"name": "During transfer", "kind": "chatgpt_subscription", "scope": "workspace", "workspaceId": workspace.id}), StatusCode::CONFLICT, "", "pending transfer refuses creation").await;
    assert_eq!(
        next_client
            .put_json(
                &workspace_config_path(org.id, workspace.id),
                &json!({"settings": {"CUSTOM_ENV": "blocked"}})
            )
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let base_url: String =
        sqlx::query_scalar("SELECT base_url FROM provider_accounts WHERE id = $1")
            .bind(account.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(base_url, "https://api.z.ai/api/anthropic");
    app.cleanup().await;
}

/// I2 steps 1–3 plus the validation catalog: the role matrix per scope, the
/// secret rule per kind, base-URL rules, scope-shape rules, name bounds, and
/// the per-scope unique name.
#[tokio::test]
async fn creating_accounts_follows_scope_authority_and_kind_rules() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let other_org = TestOrg::create(&app).await;
    let plain = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let admin = seed_admin(&app, &org).await;
    let other_workspace = TestWorkspace::create(&app, &other_org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&plain);

    // Step 1: the role matrix. The member's JWT carries workspaces:write, so
    // the route admits the request and the service's current-role check is
    // what refuses the org scope.
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Not mine to make",
            "kind": "anthropic_api_key",
            "scope": "org",
            "secret": ORG_API_KEY
        }),
        StatusCode::FORBIDDEN,
        "",
        "member creates org account",
    )
    .await;
    let ws_account = create_account(
        &member_client,
        org.id,
        json!({
            "name": "Workspace key",
            "kind": "openrouter_api_key",
            "scope": "workspace",
            "workspaceId": workspace.id,
            "secret": ROTATED_KEY
        }),
        "workspace owner creates workspace account",
    )
    .await;
    assert_eq!(ws_account["family"], "both");
    assert_eq!(ws_account["secretHint"], ROTATED_KEY_HINT);
    let user_account = create_account(
        &member_client,
        org.id,
        json!({
            "name": "Mine",
            "kind": "anthropic_api_key",
            "scope": "user",
            "secret": ORG_API_KEY
        }),
        "member creates user account",
    )
    .await;
    assert_eq!(user_account["userId"], plain.id.to_string());
    create_account(
        &admin_client,
        org.id,
        json!({
            "name": "Org key",
            "kind": "anthropic_api_key",
            "scope": "org",
            "secret": ORG_API_KEY
        }),
        "admin creates org account",
    )
    .await;

    // Step 2: the secret rule per kind (secret_custody D10).
    let chatgpt = create_account(
        &member_client,
        org.id,
        json!({
            "name": "ChatGPT plan",
            "kind": "chatgpt_subscription",
            "scope": "user"
        }),
        "chatgpt_subscription without a secret",
    )
    .await;
    assert_eq!(chatgpt["hasSecret"], false);
    assert!(chatgpt["secretHint"].is_null());
    assert_eq!(chatgpt["family"], "codex");
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "ChatGPT with secret",
            "kind": "chatgpt_subscription",
            "scope": "user",
            "secret": ORG_API_KEY
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_secret_invalid",
        "chatgpt_subscription with a secret",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Claude plan",
            "kind": "claude_subscription",
            "scope": "user"
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_secret_invalid",
        "claude_subscription without a secret",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Claude plan blank",
            "kind": "claude_subscription",
            "scope": "user",
            "secret": "   "
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_secret_invalid",
        "claude_subscription with a blank secret",
    )
    .await;
    // QA 2026-10-27: an Anthropic API key in a Claude subscription account
    // is refused with the typed code (contract `validate_secret`).
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Claude plan with an API key",
            "kind": "claude_subscription",
            "scope": "user",
            "secret": ORG_API_KEY
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_secret_invalid",
        "claude_subscription with an API key",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Claude plan wrapped",
            "kind": "claude_subscription",
            "scope": "user",
            "secret": format!("{} {}", &USER_OAUTH[..20], &USER_OAUTH[20..])
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_secret_invalid",
        "claude_subscription with a space inside",
    )
    .await;
    // A short but non-empty secret is stored without a hint (D9 floor). Only
    // the compatible bearer kind has no published shape, so only it accepts
    // a value this short (contract `validate_secret`).
    let short = create_account(
        &member_client,
        org.id,
        json!({
            "name": "Short",
            "kind": "anthropic_compatible_bearer",
            "scope": "user",
            "baseUrl": "https://api.z.ai/api/anthropic",
            "secret": SHORT_KEY
        }),
        "short secret stored without hint",
    )
    .await;
    assert_eq!(short["hasSecret"], true);
    assert!(short["secretHint"].is_null());
    assert!(!short.to_string().contains(SHORT_KEY));

    // Step 3: base-URL rules (contract `validate_base_url`).
    for (base_url, context) in [
        ("http://api.z.ai/api/anthropic", "http base URL"),
        (
            "https://user:pw@api.z.ai/api/anthropic",
            "userinfo in base URL",
        ),
        (
            "https://openrouter.ai/api",
            "OpenRouter host on a bearer kind",
        ),
        ("not a url", "unparseable base URL"),
    ] {
        create_expect(
            &member_client,
            org.id,
            json!({
                "name": "Bearer",
                "kind": "anthropic_compatible_bearer",
                "scope": "user",
                "baseUrl": base_url,
                "secret": WS_BEARER
            }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "account_base_url_invalid",
            context,
        )
        .await;
    }
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Bearer without URL",
            "kind": "anthropic_compatible_bearer",
            "scope": "user",
            "secret": WS_BEARER
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_base_url_invalid",
        "compatible bearer requires a base URL",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Fixed host",
            "kind": "claude_subscription",
            "scope": "user",
            "baseUrl": "https://api.anthropic.com",
            "secret": USER_OAUTH
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_base_url_invalid",
        "subscription kinds reject a base URL",
    )
    .await;

    // Scope-shape rules (service D4).
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "No workspace",
            "kind": "anthropic_api_key",
            "scope": "workspace",
            "secret": ORG_API_KEY
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_scope_forbidden",
        "workspace scope without workspaceId",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "Foreign workspace",
            "kind": "anthropic_api_key",
            "scope": "workspace",
            "workspaceId": other_workspace.id,
            "secret": ORG_API_KEY
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_scope_forbidden",
        "workspace scope with another org's workspace",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({
            "name": "User with workspace",
            "kind": "anthropic_api_key",
            "scope": "user",
            "workspaceId": workspace.id,
            "secret": ORG_API_KEY
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_scope_forbidden",
        "user scope with workspaceId",
    )
    .await;
    create_expect(
        &admin_client,
        org.id,
        json!({
            "name": "Org with workspace",
            "kind": "anthropic_api_key",
            "scope": "org",
            "workspaceId": workspace.id,
            "secret": ORG_API_KEY
        }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_scope_forbidden",
        "org scope with workspaceId",
    )
    .await;

    // Name bounds, unknown kind, unknown scope.
    create_expect(
        &member_client,
        org.id,
        json!({ "name": "   ", "kind": "anthropic_api_key", "scope": "user", "secret": ORG_API_KEY }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_name_invalid",
        "blank name",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({ "name": "n".repeat(81), "kind": "anthropic_api_key", "scope": "user", "secret": ORG_API_KEY }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_name_invalid",
        "81-character name",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({ "name": "Gemini", "kind": "gemini_api_key", "scope": "user", "secret": ORG_API_KEY }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_kind_invalid",
        "kind outside the closed catalog",
    )
    .await;
    create_expect(
        &member_client,
        org.id,
        json!({ "name": "Team", "kind": "anthropic_api_key", "scope": "team", "secret": ORG_API_KEY }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "account_scope_invalid",
        "scope outside the closed vocabulary",
    )
    .await;

    // Duplicate name within a scope (case-insensitive) is a typed 409; the
    // same name is free at another scope.
    let duplicate = member_client
        .post_json(
            &accounts_path(org.id),
            &json!({ "name": "MINE", "kind": "openai_api_key", "scope": "user", "secret": ORG_API_KEY }),
        )
        .await;
    let (status, body) = error_body(duplicate, "duplicate user-scope name").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        error_message(&body).contains("account_name_conflict"),
        "{body}"
    );
    create_account(
        &admin_client,
        org.id,
        json!({ "name": "Mine", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "same name at org scope",
    )
    .await;

    // Nothing above leaked a secret into any response.
    let everything = list_accounts(&member_client, org.id, Some(workspace.id), "final list").await;
    assert_only_sanctioned_hints(
        &everything,
        &[ORG_API_KEY, ROTATED_KEY, WS_BEARER, SHORT_KEY],
    );

    app.cleanup().await;
}

/// I2 steps 4–5: a blank `secret` keeps the stored credential and its hint, a
/// new secret changes the hint, kind and scope cannot be patched, and update
/// authority follows the row scope.
#[tokio::test]
async fn patching_keeps_blank_secret_rotates_new_secret_and_rejects_immutable_fields() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let plain = seed_member(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&plain);

    let account = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team key", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "seed org account",
    )
    .await;
    let id = account["id"].as_str().unwrap().to_string();
    let path = account_path(org.id, &id);

    // Step 4a: blank secret + rename keeps the credential (never deletes).
    let renamed = admin_client
        .patch_json(&path, &json!({ "name": "  Team key v2  ", "secret": "" }))
        .await;
    assert_no_store(&renamed, "blank-secret patch");
    assert_eq!(renamed.status(), StatusCode::OK);
    let renamed: Value = renamed.json().await.unwrap();
    assert_eq!(renamed["data"]["name"], "Team key v2");
    assert_eq!(renamed["data"]["hasSecret"], true);
    assert_eq!(renamed["data"]["secretHint"], ORG_API_KEY_HINT);
    assert_eq!(renamed["data"]["kind"], "anthropic_api_key");
    assert_only_sanctioned_hints(&renamed, &[ORG_API_KEY]);

    // Absent secret (name-only patch) also keeps it, and a base URL change is
    // validated for the row's kind.
    let url_only = admin_client
        .patch_json(
            &path,
            &json!({ "baseUrl": "https://llm.example.test/anthropic" }),
        )
        .await;
    assert_eq!(url_only.status(), StatusCode::OK);
    let url_only: Value = url_only.json().await.unwrap();
    assert_eq!(
        url_only["data"]["baseUrl"],
        "https://llm.example.test/anthropic"
    );
    assert_eq!(url_only["data"]["secretHint"], ORG_API_KEY_HINT);
    let bad_url = admin_client
        .patch_json(&path, &json!({ "baseUrl": "http://llm.example.test" }))
        .await;
    let (status, body) = error_body(bad_url, "http base URL on patch").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(details_code(&body), "account_base_url_invalid");
    // A blank base URL clears it (allowed for this kind).
    let cleared = admin_client
        .patch_json(&path, &json!({ "baseUrl": "" }))
        .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    let cleared: Value = cleared.json().await.unwrap();
    assert!(cleared["data"]["baseUrl"].is_null());

    // Step 4b: a new secret is stored. With the base URL cleared the row is
    // on Anthropic's own host again, so an OpenRouter-shaped key is refused
    // and an Anthropic-shaped one accepted (contract `validate_secret`).
    let wrong_shape = admin_client
        .patch_json(&path, &json!({ "secret": ROTATED_KEY }))
        .await;
    let (status, body) = error_body(wrong_shape, "OpenRouter key on an Anthropic row").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(details_code(&body), "account_secret_invalid");
    let rotated = admin_client
        .patch_json(&path, &json!({ "secret": ROTATED_ANTHROPIC_KEY }))
        .await;
    assert_eq!(rotated.status(), StatusCode::OK);
    let rotated: Value = rotated.json().await.unwrap();
    assert_eq!(rotated["data"]["secretHint"], ORG_API_KEY_HINT);
    assert_only_sanctioned_hints(&rotated, &[ORG_API_KEY, ROTATED_ANTHROPIC_KEY]);

    // Step 5: kind and scope are immutable — the body is refused before the
    // handler runs and the row is unchanged.
    for body in [
        json!({ "kind": "openai_api_key" }),
        json!({ "scope": "user" }),
        json!({ "name": "x", "workspaceId": Uuid::new_v4() }),
    ] {
        let rejected = admin_client.patch_json(&path, &body).await;
        assert_no_store(&rejected, "immutable field patch");
        assert!(
            rejected.status().is_client_error(),
            "{body} must be rejected, got {}",
            rejected.status()
        );
    }
    let after = list_accounts(&admin_client, org.id, None, "after immutable patches").await;
    let row = &after["data"][0];
    assert_eq!(row["id"], id);
    assert_eq!(row["kind"], "anthropic_api_key");
    assert_eq!(row["scope"], "org");
    assert_eq!(row["name"], "Team key v2");

    // Authority: a plain member sees the org account but cannot patch it.
    let forbidden = member_client
        .patch_json(&path, &json!({ "name": "Hijack" }))
        .await;
    let (status, body) = error_body(forbidden, "member patches org account").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Renaming onto an existing name in the same scope is a typed 409.
    create_account(
        &admin_client,
        org.id,
        json!({ "name": "Other", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "second org account",
    )
    .await;
    let clash = admin_client
        .patch_json(&path, &json!({ "name": "other" }))
        .await;
    let (status, body) = error_body(clash, "rename onto existing name").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        error_message(&body).contains("account_name_conflict"),
        "{body}"
    );

    // An unknown id in the org is 404.
    let missing = admin_client
        .patch_json(
            &account_path(org.id, &Uuid::new_v4().to_string()),
            &json!({ "name": "x" }),
        )
        .await;
    let (status, body) = error_body(missing, "patch unknown id").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    app.cleanup().await;
}

/// I2 step 6: delete is refused with `account_assigned` while an org-scope,
/// the caller's user-scope, or ANOTHER member's user-scope assignment names
/// the account (the guard scans every member's user config,
/// provider_account_service D6), and succeeds once every assignment is
/// cleared; authority follows the row scope.
#[tokio::test]
async fn deleting_is_refused_while_assigned_and_removes_the_row_after_clearing() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let plain = seed_member(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&plain);
    let user_config_path = format!("/api/v1/users/me/config?orgId={}", org.id);
    let org_config_path = format!("/api/v1/orgs/{}/config", org.id);

    // An OpenRouter account serves both families, so the same row can be
    // assigned through either validated key (config_service D13).
    let account = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team key", "kind": "openrouter_api_key", "scope": "org", "secret": ROTATED_KEY }),
        "seed org account",
    )
    .await;
    let id = account["id"].as_str().unwrap().to_string();
    let path = account_path(org.id, &id);

    // Assign at the admin's own user scope through the ordinary config API.
    let assign = admin_client
        .put_json(
            &user_config_path,
            &json!({ "settings": { "claude_account_id": id } }),
        )
        .await;
    assert_eq!(assign.status(), StatusCode::OK);
    let refused = admin_client.delete(&path).await;
    let (status, body) = error_body(refused, "delete while user-assigned").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(error_message(&body).contains("account_assigned"), "{body}");
    assert_eq!(count_visible(&app, org.id).await, 1);

    // Another member assigns the same org account at THEIR user scope. With
    // the admin's own assignment cleared, the peer's alone must still block
    // the admin's delete: the guard is not limited to the caller's config.
    let peer = seed_member(&app, &org).await;
    let peer_client = app.authenticated_client(&peer);
    let peer_assign = peer_client
        .put_json(
            &user_config_path,
            &json!({ "settings": { "codex_account_id": id } }),
        )
        .await;
    assert_eq!(peer_assign.status(), StatusCode::OK);
    let clear = admin_client
        .put_json(
            &user_config_path,
            &json!({ "removeSettingKeys": ["claude_account_id"] }),
        )
        .await;
    assert_eq!(clear.status(), StatusCode::OK);
    let refused_peer = admin_client.delete(&path).await;
    let (status, body) = error_body(refused_peer, "delete while peer-assigned").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(error_message(&body).contains("account_assigned"), "{body}");
    assert_eq!(count_visible(&app, org.id).await, 1);

    // Clear the peer's assignment. The org account's OWN scope still names
    // it (creation made it the org default for both agents, chat
    // g5.chat.account-defaults); that does not block its removal.
    let peer_clear = peer_client
        .put_json(
            &user_config_path,
            &json!({ "removeSettingKeys": ["codex_account_id"] }),
        )
        .await;
    assert_eq!(peer_clear.status(), StatusCode::OK);
    let org_config: Value = admin_client
        .get(&org_config_path)
        .await
        .json()
        .await
        .expect("org config JSON");
    assert_eq!(org_config["data"]["settings"]["claude_account_id"], id);
    assert_eq!(org_config["data"]["settings"]["codex_account_id"], id);

    // A plain member may not delete an org account even when only its own
    // scope assigns it.
    let forbidden = member_client.delete(&path).await;
    let (status, body) = error_body(forbidden, "member deletes org account").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(count_visible(&app, org.id).await, 1);

    // Authorized: 204, the row is gone and the org's own assignments went
    // with it, in the same commit.
    let deleted = admin_client.delete(&path).await;
    assert_no_store(&deleted, "successful delete");
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(count_visible(&app, org.id).await, 0);
    let org_config: Value = admin_client
        .get(&org_config_path)
        .await
        .json()
        .await
        .expect("org config JSON");
    assert!(
        org_config["data"]["settings"]
            .get("claude_account_id")
            .is_none(),
        "org default cleared with the row: {org_config}"
    );
    assert!(
        org_config["data"]["settings"]
            .get("codex_account_id")
            .is_none(),
        "org default cleared with the row: {org_config}"
    );
    let listed = list_accounts(&admin_client, org.id, None, "after delete").await;
    assert!(ids_of(&listed).is_empty());
    let again = admin_client.delete(&path).await;
    let (status, body) = error_body(again, "delete twice").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    // A user account: its owner deletes it, another member cannot see it.
    let mine = create_account(
        &member_client,
        org.id,
        json!({ "name": "Mine", "kind": "chatgpt_subscription", "scope": "user" }),
        "member user account",
    )
    .await;
    let mine_id = mine["id"].as_str().unwrap().to_string();
    let other = seed_member(&app, &org).await;
    let other_client = app.authenticated_client(&other);
    let invisible = other_client.delete(&account_path(org.id, &mine_id)).await;
    let (status, body) = error_body(invisible, "other member deletes user account").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let owner_delete = member_client.delete(&account_path(org.id, &mine_id)).await;
    assert_eq!(owner_delete.status(), StatusCode::NO_CONTENT);
    assert_eq!(count_visible(&app, org.id).await, 0);

    app.cleanup().await;
}

/// Chat g5.chat.account-defaults: creating an account makes it the default
/// for each agent it serves at its own scope where none is set; a later
/// account never replaces it; a person's own account overrides the
/// organization's default only for that person; removing an account clears
/// its own scope's assignments with it, while an assignment at another scope
/// still refuses the removal.
#[tokio::test]
async fn first_account_becomes_the_default_at_its_scope_and_removal_clears_it() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let member = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&member);

    async fn settings_of(client: &helpers::AuthenticatedClient, path: &str) -> Value {
        let response = client.get(path).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.expect("config JSON");
        body["data"]["settings"].clone()
    }

    // 1. The first org Claude account becomes the org's Claude Code default;
    //    Codex stays unset because the kind does not serve it.
    let org_claude = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team key", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "first org claude account",
    )
    .await;
    let org_claude_id = org_claude["id"].as_str().unwrap().to_string();
    let org_settings = settings_of(&admin_client, &org_config_path(org.id)).await;
    assert_eq!(org_settings["claude_account_id"], org_claude_id);
    assert!(
        org_settings.get("codex_account_id").is_none(),
        "{org_settings}"
    );

    // 2. A second org Claude account does not replace the default.
    let org_backup = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Backup key", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "second org claude account",
    )
    .await;
    let org_backup_id = org_backup["id"].as_str().unwrap().to_string();
    let org_settings = settings_of(&admin_client, &org_config_path(org.id)).await;
    assert_eq!(org_settings["claude_account_id"], org_claude_id);

    // 3. A both-family org account fills only the agent still unset: Codex.
    let org_router = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Router", "kind": "openrouter_api_key", "scope": "org", "secret": ROTATED_KEY }),
        "both-family org account",
    )
    .await;
    let org_router_id = org_router["id"].as_str().unwrap().to_string();
    let org_settings = settings_of(&admin_client, &org_config_path(org.id)).await;
    assert_eq!(org_settings["claude_account_id"], org_claude_id);
    assert_eq!(org_settings["codex_account_id"], org_router_id);

    // 4. The member inherits the org default until their own account exists;
    //    their own subscription becomes their user-scope default and wins
    //    for them alone. Another member still resolves the org default.
    let inherited = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "member inherits the org default",
    )
    .await;
    assert_eq!(inherited["accountId"], org_claude_id);
    let mine = create_account(
        &member_client,
        org.id,
        json!({ "name": "My subscription", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "member's first account",
    )
    .await;
    let mine_id = mine["id"].as_str().unwrap().to_string();
    let user_settings = settings_of(&member_client, &user_config_path(org.id)).await;
    assert_eq!(user_settings["claude_account_id"], mine_id);
    let own = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "member resolves their own default",
    )
    .await;
    assert_eq!(own["accountId"], mine_id);
    assert_eq!(own["accountScope"], "user");
    let peer = seed_member(&app, &org).await;
    let peer_client = app.authenticated_client(&peer);
    let peer_resolution = resolve_ok(
        &peer_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "peer still inherits the org default",
    )
    .await;
    assert_eq!(peer_resolution["accountId"], org_claude_id);
    // The org's settings were not touched by the member's account.
    let org_settings = settings_of(&admin_client, &org_config_path(org.id)).await;
    assert_eq!(org_settings["claude_account_id"], org_claude_id);

    // 5. Removing the member's only account clears their own default with it
    //    and they inherit the org default again.
    let removed = member_client.delete(&account_path(org.id, &mine_id)).await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let user_settings = settings_of(&member_client, &user_config_path(org.id)).await;
    assert!(
        user_settings.get("claude_account_id").is_none(),
        "own default cleared with the account: {user_settings}"
    );
    let back = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "member inherits the org default again",
    )
    .await;
    assert_eq!(back["accountId"], org_claude_id);

    // 6. The peer picks the org default as their own user-scope assignment:
    //    that other-scope assignment refuses the org account's removal, and
    //    the org's own assignment is untouched by the refusal.
    put_config_ok(
        &peer_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": org_claude_id } }),
        "peer pins the org account",
    )
    .await;
    let refused = admin_client
        .delete(&account_path(org.id, &org_claude_id))
        .await;
    let (status, body) = error_body(refused, "delete while a member pins it").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(error_message(&body).contains("account_assigned"), "{body}");
    let org_settings = settings_of(&admin_client, &org_config_path(org.id)).await;
    assert_eq!(org_settings["claude_account_id"], org_claude_id);
    put_config_ok(
        &peer_client,
        &user_config_path(org.id),
        json!({ "removeSettingKeys": ["claude_account_id"] }),
        "peer releases the org account",
    )
    .await;

    // 7. Removing the org default clears the org's own assignment; the
    //    backup account is not promoted (people choose in Defaults).
    let deleted = admin_client
        .delete(&account_path(org.id, &org_claude_id))
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let org_settings = settings_of(&admin_client, &org_config_path(org.id)).await;
    assert!(
        org_settings.get("claude_account_id").is_none(),
        "org default cleared with the row: {org_settings}"
    );
    assert_eq!(org_settings["codex_account_id"], org_router_id);
    let unassigned = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "no Claude default left",
    )
    .await;
    assert_no_account_fields(&unassigned, "after the org default is removed");
    assert!(
        ids_of(&list_accounts(&admin_client, org.id, None, "remaining").await)
            .contains(&org_backup_id)
    );

    app.cleanup().await;
}

/// I2 step 6a: the delete guard and the assignment validator serialize on the
/// account row (provider_account_service D6, config_service D13). An
/// assignment write that races an in-flight delete waits on the row lock and,
/// once the delete commits, fails `account_not_visible` instead of persisting
/// an assignment that names a row which no longer exists.
#[tokio::test]
async fn assignment_write_waits_for_an_in_flight_delete_and_then_fails_closed() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let member = seed_member(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let account = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Racing key", "kind": "openrouter_api_key", "scope": "org", "secret": ROTATED_KEY }),
        "seed org account",
    )
    .await;
    let id = Uuid::parse_str(account["id"].as_str().unwrap()).unwrap();

    // The in-flight delete: the service's own sequence (D6) — lock the row
    // `FOR UPDATE`, then delete it — held open on an uncommitted tenant
    // transaction.
    let mut deleting = app.scoped_db_for_user_org(admin.id, org.id).await;
    sqlx::query("SELECT id FROM provider_accounts WHERE id = $1 AND org_id = $2 FOR UPDATE")
        .bind(id)
        .bind(org.id)
        .fetch_one(deleting.conn())
        .await
        .expect("lock the account row");
    let removed = sqlx::query("DELETE FROM provider_accounts WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(org.id)
        .execute(deleting.conn())
        .await
        .expect("delete the locked row")
        .rows_affected();
    assert_eq!(removed, 1);

    // A member assigns the account while the delete is in flight. The write
    // must block on the row lock: an unblocked write completes in well under
    // the observation window, so finishing early proves the lock is missing.
    let racing_client = app.authenticated_client(&member);
    let path = user_config_path(org.id);
    let body = json!({ "settings": { "claude_account_id": id.to_string() } });
    let write = tokio::spawn(async move { racing_client.put_json(&path, &body).await });
    let observe_until = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while tokio::time::Instant::now() < observe_until {
        assert!(
            !write.is_finished(),
            "assignment write completed while the delete was still uncommitted"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // The delete commits; the waiting write resumes, finds no row, and fails
    // closed without storing anything.
    deleting
        .commit()
        .await
        .expect("commit the in-flight delete");
    let response = write.await.expect("assignment write task");
    let (status, body) = plain_error_body(response, "assignment after the delete committed").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(details_code(&body), "account_not_visible", "{body}");

    let member_client = app.authenticated_client(&member);
    let stored = member_client.get(&user_config_path(org.id)).await;
    assert_eq!(stored.status(), StatusCode::OK);
    let stored: Value = stored.json().await.expect("config JSON");
    assert!(
        stored["data"]["settings"]
            .get("claude_account_id")
            .is_none(),
        "dangling assignment persisted: {stored}"
    );
    assert_eq!(count_visible(&app, org.id).await, 0);

    app.cleanup().await;
}

// ===========================================================================
// Story 2 — assignment validation, launch projection, dispatch
// (`test_contract/provider_accounts` I3, I4, and the integration half of I5;
// `config_service` D12/D13; `routes/config` D9; `job_service` D50;
// `task_service` D62; `routes/chat` D31;
// `system_invariants/external_agent_provider_contract` D11, D14).
// ===========================================================================

fn user_config_path(org_id: Uuid) -> String {
    format!("/api/v1/users/me/config?orgId={org_id}")
}

fn org_config_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/config")
}

fn workspace_config_path(org_id: Uuid, workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/config?orgId={org_id}")
}

/// Error reader for non-account routes (chat start), which carry no
/// `no-store` promise of their own.
async fn plain_error_body(response: reqwest::Response, context: &str) -> (StatusCode, Value) {
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .unwrap_or_else(|error| panic!("{context}: error body is JSON: {error}"));
    (status, body)
}

fn resolve_path(workspace_id: Uuid) -> String {
    format!("/api/v1/workspaces/{workspace_id}/config/agent-launch/resolve")
}

/// Claude Code preferences with every visible field explicit and the account
/// preference supplied by the caller (`inherit_configured` when `None`).
fn claude_preferences(account: Option<Value>) -> Value {
    let mut preferences = json!({
        "schemaVersion": 1,
        "provider": { "source": "explicit", "value": "claude_code" },
        "model": { "source": "explicit", "value": "claude-opus-4-8[1m]" },
        "permissionMode": { "source": "explicit", "value": "bypassPermissions" },
        "effort": { "source": "explicit", "value": "high" }
    });
    if let Some(account) = account {
        preferences["account"] = json!({ "source": "explicit", "value": account });
    }
    preferences
}

fn explicit_account(id: &str) -> Value {
    json!({ "kind": "account", "accountId": id })
}

async fn put_config_ok(
    client: &helpers::AuthenticatedClient,
    path: &str,
    body: Value,
    context: &str,
) -> Value {
    let response = client.put_json(path, &body).await;
    let status = response.status();
    let json: Value = response.json().await.expect("config response is JSON");
    assert_eq!(status, StatusCode::OK, "{context}: {json}");
    json["data"].clone()
}

async fn put_config_expect(
    client: &helpers::AuthenticatedClient,
    path: &str,
    body: Value,
    expected_code: &str,
    context: &str,
) {
    let response = client.put_json(path, &body).await;
    let (status, json) = error_body(response, context).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{context}: {json}"
    );
    assert_eq!(details_code(&json), expected_code, "{context}: {json}");
}

async fn resolve_ok(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    body: Value,
    context: &str,
) -> Value {
    let response = client.post_json(&resolve_path(workspace_id), &body).await;
    let status = response.status();
    let json: Value = response.json().await.expect("resolve response is JSON");
    assert_eq!(status, StatusCode::OK, "{context}: {json}");
    json["data"]["resolved"].clone()
}

async fn resolve_expect(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    body: Value,
    expected_code: &str,
    context: &str,
) {
    let response = client.post_json(&resolve_path(workspace_id), &body).await;
    let (status, json) = error_body(response, context).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{context}: {json}"
    );
    assert_eq!(details_code(&json), expected_code, "{context}: {json}");
}

fn assert_no_account_fields(resolved: &Value, context: &str) {
    for key in ["accountId", "accountKind", "accountScope"] {
        assert!(
            resolved.get(key).is_none(),
            "{context}: account-less resolution must not serialize {key}: {resolved}"
        );
    }
}

/// I3: assignment keys are validated for family and scope on every scoped
/// write, staged through the resolver preview without persisting, and cleared
/// through `removeSettingKeys` (config_service D13, routes/config D9).
#[tokio::test]
async fn assignment_keys_are_validated_for_family_and_scope_and_preview_through_the_resolver() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let member = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let other_workspace = TestWorkspace::create(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&member);

    let org_claude = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team key", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "org claude account",
    )
    .await;
    let org_openai = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team OpenAI", "kind": "openai_api_key", "scope": "org", "secret": ROTATED_KEY }),
        "org codex account",
    )
    .await;
    let ws_bearer = create_account(
        &admin_client,
        org.id,
        json!({
            "name": "Workspace bearer",
            "kind": "anthropic_compatible_bearer",
            "scope": "workspace",
            "workspaceId": workspace.id,
            "baseUrl": "https://api.z.ai/api/anthropic",
            "secret": WS_BEARER
        }),
        "workspace account",
    )
    .await;
    let other_ws_bearer = create_account(
        &admin_client,
        org.id,
        json!({
            "name": "Other workspace bearer",
            "kind": "anthropic_compatible_bearer",
            "scope": "workspace",
            "workspaceId": other_workspace.id,
            "baseUrl": "https://api.z.ai/api/anthropic",
            "secret": WS_BEARER
        }),
        "other workspace account",
    )
    .await;
    let member_sub = create_account(
        &member_client,
        org.id,
        json!({ "name": "My subscription", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "member user account",
    )
    .await;
    let org_claude_id = org_claude["id"].as_str().unwrap().to_string();
    let org_openai_id = org_openai["id"].as_str().unwrap().to_string();
    let ws_bearer_id = ws_bearer["id"].as_str().unwrap().to_string();
    let other_ws_bearer_id = other_ws_bearer["id"].as_str().unwrap().to_string();
    let member_sub_id = member_sub["id"].as_str().unwrap().to_string();

    // Step 1: family, existence, and scope-at-or-above are enforced on write.
    put_config_expect(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": org_openai_id } }),
        "account_kind_mismatch",
        "codex-family account as the Claude Code account",
    )
    .await;
    put_config_expect(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "codex_account_id": org_claude_id } }),
        "account_kind_mismatch",
        "claude-family account as the Codex account",
    )
    .await;
    put_config_expect(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": Uuid::new_v4().to_string() } }),
        "account_not_visible",
        "unknown account id",
    )
    .await;
    put_config_expect(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": "not-a-uuid" } }),
        "account_not_visible",
        "malformed account id",
    )
    .await;
    put_config_expect(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": 7 } }),
        "invalid_setting_type",
        "non-string account id",
    )
    .await;
    put_config_expect(
        &admin_client,
        &org_config_path(org.id),
        json!({ "settings": { "claude_account_id": ws_bearer_id } }),
        "account_scope_forbidden",
        "workspace account from an org write",
    )
    .await;
    put_config_expect(
        &admin_client,
        &org_config_path(org.id),
        json!({ "settings": { "claude_account_id": member_sub_id } }),
        "account_scope_forbidden",
        "user account from an org write",
    )
    .await;
    put_config_expect(
        &admin_client,
        &workspace_config_path(org.id, workspace.id),
        json!({ "settings": { "claude_account_id": member_sub_id } }),
        "account_scope_forbidden",
        "user account from a workspace write",
    )
    .await;
    put_config_expect(
        &admin_client,
        &workspace_config_path(org.id, workspace.id),
        json!({ "settings": { "claude_account_id": other_ws_bearer_id } }),
        "account_scope_forbidden",
        "another workspace's account from a workspace write",
    )
    .await;
    // A workspace write may name its own workspace account; an admin's user
    // write may not name another member's user account.
    put_config_ok(
        &admin_client,
        &workspace_config_path(org.id, workspace.id),
        json!({ "settings": { "claude_account_id": ws_bearer_id } }),
        "own workspace account from a workspace write",
    )
    .await;
    put_config_expect(
        &admin_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": member_sub_id } }),
        "account_scope_forbidden",
        "another member's user account from a user write",
    )
    .await;
    // Nothing invalid was persisted: the member's user scope holds only the
    // default their own subscription became on creation (chat
    // g5.chat.account-defaults).
    let stored = member_client.get(&user_config_path(org.id)).await;
    let stored: Value = stored.json().await.expect("config JSON");
    assert_eq!(
        stored["data"]["settings"]["claude_account_id"],
        member_sub_id
    );
    assert!(stored["data"]["settings"].get("codex_account_id").is_none());

    // Step 2: a valid org account at user scope is stored and effective.
    let written = put_config_ok(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": org_claude_id, "codex_account_id": org_openai_id } }),
        "valid assignments at user scope",
    )
    .await;
    assert_eq!(written["settings"]["claude_account_id"], org_claude_id);
    assert_eq!(
        written["effectiveSettings"]["claude_account_id"],
        org_claude_id
    );
    assert_eq!(
        written["effectiveSettings"]["codex_account_id"],
        org_openai_id
    );

    // Step 3: a staged assignment resolves through the account and persists
    // nothing.
    let staged = resolve_ok(
        &member_client,
        workspace.id,
        json!({
            "preferences": claude_preferences(None),
            "settingsPreview": [{ "scope": "user", "settings": { "claude_account_id": member_sub_id } }]
        }),
        "staged user assignment",
    )
    .await;
    assert_eq!(staged["accountId"], member_sub_id);
    assert_eq!(staged["accountKind"], "claude_subscription");
    assert_eq!(staged["accountScope"], "user");
    assert_eq!(staged["authMode"], "anthropic_oauth");
    let persisted = member_client.get(&user_config_path(org.id)).await;
    let persisted: Value = persisted.json().await.expect("config JSON");
    assert_eq!(
        persisted["data"]["settings"]["claude_account_id"],
        org_claude_id
    );
    // The persisted assignment still resolves without a preview.
    let live = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "persisted assignment",
    )
    .await;
    assert_eq!(live["accountId"], org_claude_id);
    assert_eq!(live["accountKind"], "anthropic_api_key");
    assert_eq!(live["accountScope"], "org");
    // A staged assignment is validated like a write.
    resolve_expect(
        &member_client,
        workspace.id,
        json!({
            "preferences": claude_preferences(None),
            "settingsPreview": [{ "scope": "user", "settings": { "claude_account_id": org_openai_id } }]
        }),
        "account_kind_mismatch",
        "staged wrong-family assignment",
    )
    .await;
    resolve_expect(
        &member_client,
        workspace.id,
        json!({
            "preferences": claude_preferences(None),
            "settingsPreview": [{ "scope": "org", "settings": { "claude_account_id": member_sub_id } }]
        }),
        "account_scope_forbidden",
        "staged user account at org scope",
    )
    .await;

    // Step 4: removal clears the assignment. With the user and workspace
    // assignments gone, the organization's own default (the first org Claude
    // account, chat g5.chat.account-defaults) is what resolves; once that is
    // cleared too the next resolution carries no account fields.
    put_config_ok(
        &member_client,
        &user_config_path(org.id),
        json!({ "removeSettingKeys": ["claude_account_id"] }),
        "remove user assignment",
    )
    .await;
    put_config_ok(
        &admin_client,
        &workspace_config_path(org.id, workspace.id),
        json!({ "removeSettingKeys": ["claude_account_id"] }),
        "remove workspace assignment",
    )
    .await;
    let org_default = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "org default after user and workspace removal",
    )
    .await;
    assert_eq!(org_default["accountId"], org_claude_id);
    assert_eq!(org_default["accountScope"], "org");
    put_config_ok(
        &admin_client,
        &org_config_path(org.id),
        json!({ "removeSettingKeys": ["claude_account_id"] }),
        "remove org default",
    )
    .await;
    let cleared = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "after removal",
    )
    .await;
    assert_no_account_fields(&cleared, "after removal");

    app.cleanup().await;
}

/// I4: an assigned account overrides raw keys at every scope, an explicit
/// `runtime_config` selection restores the raw-key resolution, removing the
/// assignment restores the byte-identical account-less contract, and a
/// rotated account secret changes the config revision (config_service D12).
#[tokio::test]
async fn assigned_account_overrides_raw_keys_and_removal_restores_the_account_less_contract() {
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let member = seed_member(&app, &org).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let member_client = app.authenticated_client(&member);

    // Step 1: org raw OpenRouter bearer + route, and a user subscription.
    put_config_ok(
        &admin_client,
        &org_config_path(org.id),
        json!({
            "settings": { "ANTHROPIC_BASE_URL": "https://openrouter.ai/api" },
            "setSecrets": { "ANTHROPIC_AUTH_TOKEN": "sk-or-v1-raw-org-bearer-0123456789abcdef" }
        }),
        "org raw keys",
    )
    .await;
    let baseline = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "pre-feature baseline",
    )
    .await;
    assert_eq!(baseline["authMode"], "open_router_api_key");
    assert_eq!(baseline["providerRoute"], "openrouter");
    assert_no_account_fields(&baseline, "baseline");

    let account = create_account(
        &member_client,
        org.id,
        json!({ "name": "My subscription", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "user subscription account",
    )
    .await;
    let account_id = account["id"].as_str().unwrap().to_string();
    put_config_ok(
        &member_client,
        &user_config_path(org.id),
        json!({ "settings": { "claude_account_id": account_id } }),
        "assign at user scope",
    )
    .await;

    // Step 2: inherit resolves the assignment over the org raw keys.
    let inherited = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "inherit assignment",
    )
    .await;
    assert_eq!(inherited["authMode"], "anthropic_oauth");
    assert_eq!(inherited["providerRoute"], "anthropic_direct");
    assert_eq!(inherited["accountId"], account_id);
    assert_eq!(inherited["accountKind"], "claude_subscription");
    assert_eq!(inherited["accountScope"], "user");
    assert_eq!(inherited["credentialReady"], true);
    assert_ne!(inherited["resolutionId"], baseline["resolutionId"]);
    assert_ne!(inherited["configRevision"], baseline["configRevision"]);
    assert_only_sanctioned_hints(&inherited, &[USER_OAUTH]);

    // Step 3: explicit runtime_config skips the projection.
    let runtime = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(Some(json!({ "kind": "runtime_config" }))) }),
        "explicit runtime_config",
    )
    .await;
    assert_eq!(runtime["authMode"], "open_router_api_key");
    assert_eq!(runtime["providerRoute"], "openrouter");
    assert_no_account_fields(&runtime, "runtime_config");

    // An explicit account beats the assignment and an invisible one fails.
    let explicit = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(Some(explicit_account(&account_id))) }),
        "explicit account",
    )
    .await;
    assert_eq!(explicit["accountId"], account_id);
    resolve_expect(
        &admin_client,
        workspace.id,
        json!({ "preferences": claude_preferences(Some(explicit_account(&account_id))) }),
        "account_not_visible",
        "another member's user account",
    )
    .await;

    // Step 5 (unit half lives in g5-contracts): a rotated secret changes the
    // revision and the resolution id.
    let rotate = member_client
        .patch_json(
            &account_path(org.id, &account_id),
            &json!({ "secret": "sk-ant-oat01-rotated-subscription-0123456789" }),
        )
        .await;
    assert_eq!(rotate.status(), StatusCode::OK);
    let rotated = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "after rotation",
    )
    .await;
    assert_eq!(rotated["accountId"], account_id);
    assert_ne!(rotated["configRevision"], inherited["configRevision"]);
    assert_ne!(rotated["resolutionId"], inherited["resolutionId"]);

    // Step 4: removal restores the byte-identical account-less contract.
    put_config_ok(
        &member_client,
        &user_config_path(org.id),
        json!({ "removeSettingKeys": ["claude_account_id"] }),
        "remove assignment",
    )
    .await;
    let restored = resolve_ok(
        &member_client,
        workspace.id,
        json!({ "preferences": claude_preferences(None) }),
        "after removal",
    )
    .await;
    assert_eq!(
        restored, baseline,
        "account-less resolution must be byte-identical"
    );

    app.cleanup().await;
}

/// The workspace-VM producer resolves an account at reservation, freezes the
/// resolution on the attempt, and projects only that credential into the
/// `agent.turn` module invocation. This retains the production boundary that
/// the removed worker-assignment test used to cover.
#[tokio::test]
async fn workspace_agent_dispatch_projects_the_selected_account_and_scrubs_raw_provider_keys() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    put_config_ok(
        &client,
        &org_config_path(org.id),
        json!({
            "settings": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "ANTHROPIC_MODEL": "claude-opus-4-8[1m]"
            },
            "setSecrets": {
                "ANTHROPIC_AUTH_TOKEN": "sk-or-v1-raw-org-bearer-0123456789abcdef",
                "GH_TOKEN": "ghp_task_scoped_token_0123456789abcdef"
            }
        }),
        "raw provider config",
    )
    .await;
    let account = create_account(
        &client,
        org.id,
        json!({
            "name": "Workspace dispatch key",
            "kind": "anthropic_api_key",
            "scope": "user",
            "baseUrl": "https://provider.example/custom/anthropic",
            "secret": ORG_API_KEY
        }),
        "workspace dispatch account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");
    let preferences: AgentLaunchPreferencesV1 =
        serde_json::from_value(claude_preferences(Some(explicit_account(account_id))))
            .expect("typed launch preferences");

    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Provider projection',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert agent session");
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .enqueue_agent_input(
            &mut db,
            EnqueueAgentInput {
                task_name: None,
                session_id,
                actor_user_id: owner.id,
                content: "Use the selected provider account".to_string(),
                artifacts: json!([]),
                input_origin: "human".to_string(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: preferences,
            },
        )
        .await
        .expect("enqueue account-backed agent input");
    db.commit().await.expect("commit agent input");

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve workspace agent dispatch")
        .expect("account-backed task is dispatchable");
    let secrets = dispatch.invoke_params["secrets"]
        .as_object()
        .expect("module secrets object");
    assert_eq!(secrets.get("ANTHROPIC_API_KEY"), Some(&json!(ORG_API_KEY)));
    assert_eq!(
        dispatch.invoke_params["input"]["provider"]["base_url"],
        "https://provider.example/custom/anthropic"
    );
    assert_eq!(
        dispatch.invoke_params["input"]["provider"]["auth_mode"],
        "anthropic_api_key"
    );
    assert!(
        !secrets.contains_key("ANTHROPIC_BASE_URL"),
        "the raw OpenRouter route must be removed for a direct Anthropic account"
    );
    assert_eq!(
        secrets.get("GH_TOKEN"),
        Some(&json!("ghp_task_scoped_token_0123456789abcdef"))
    );
    assert!(!secrets.contains_key("ANTHROPIC_AUTH_TOKEN"));
    assert!(!secrets.contains_key("CLAUDE_CODE_OAUTH_TOKEN"));
    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref->'agent_launch'
         FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(dispatch.task_id)
    .bind(dispatch.attempt)
    .fetch_one(db.conn())
    .await
    .expect("load frozen attempt launch");
    // The attempt freezes the same `{preferences, resolved}` envelope the chat
    // history reader decodes, not the bare resolved contract.
    let persisted: PersistedAgentLaunchV1 = serde_json::from_value(frozen.clone())
        .expect("frozen attempt launch is the persisted envelope");
    assert_eq!(
        persisted
            .resolved
            .account_id
            .map(|id| id.to_string())
            .as_deref(),
        Some(account_id)
    );
    assert_eq!(frozen["resolved"]["accountId"], account_id);
    assert_eq!(frozen["resolved"]["accountKind"], "anthropic_api_key");
    assert_eq!(frozen["resolved"]["accountScope"], "user");
    assert!(frozen.get("accountId").is_none());
    db.commit().await.expect("commit dispatch reservation");

    let frozen_policy: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=$2",
    )
    .bind(dispatch.task_id)
    .bind(dispatch.attempt)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        frozen_policy["provider_base_url"],
        "https://provider.example/custom/anthropic"
    );
    let rotated_secret = "rotated-provider-key-0123456789";
    let changed = client
        .patch_json(
            &account_path(org.id, account_id),
            &json!({
                "baseUrl": "https://new-provider.example/another-prefix",
                "secret": rotated_secret
            }),
        )
        .await;
    assert_eq!(changed.status(), StatusCode::OK);
    sqlx::query("UPDATE tasks SET updated_at=now()-interval '3 seconds' WHERE id=$1")
        .bind(dispatch.task_id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut db = app.scoped_db_for_org(org.id).await;
    let retry = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("redispatch the already reserved attempt")
        .expect("unconfirmed attempt remains dispatchable");
    assert_eq!(retry.task_id, dispatch.task_id);
    assert_eq!(retry.attempt, dispatch.attempt);
    assert_eq!(
        retry.invoke_params["input"]["provider"], dispatch.invoke_params["input"]["provider"],
        "account endpoint edits must not rewrite an admitted attempt's transport"
    );
    assert_eq!(
        retry.invoke_params["secrets"]["ANTHROPIC_API_KEY"], rotated_secret,
        "selected credentials remain reauthorized and rotatable"
    );
    let retained: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref FROM task_attempts WHERE task_id=$1 AND attempt=$2",
    )
    .bind(dispatch.task_id)
    .bind(dispatch.attempt)
    .fetch_one(db.conn())
    .await
    .unwrap();
    assert_eq!(retained, frozen_policy);
    db.commit().await.unwrap();

    app.cleanup().await;
}

// ===========================================================================
// Persisted launch shape — the dispatcher freezes the attempt's
// `agent_launch` as `PersistedAgentLaunchV1 {preferences, resolved}`, and the
// chat-history readers accept both that envelope and the flat resolved
// contract that attempts frozen before the envelope carry. A second turn in
// either kind of session must queue instead of failing with
// `invalid_persisted_agent_launch` (QA session 3f5aa4a7, 2026-09-22).
// ===========================================================================

#[tokio::test]
async fn chat_history_reads_envelope_and_legacy_flat_launches_and_queues_the_next_turn() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);

    put_config_ok(
        &client,
        &org_config_path(org.id),
        json!({
            "settings": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "ANTHROPIC_MODEL": "claude-opus-4-8[1m]"
            },
            "setSecrets": {
                "ANTHROPIC_AUTH_TOKEN": "sk-or-v1-raw-org-bearer-0123456789abcdef",
                "GH_TOKEN": "ghp_task_scoped_token_0123456789abcdef"
            }
        }),
        "raw provider config",
    )
    .await;
    let account = create_account(
        &client,
        org.id,
        json!({
            "name": "History shape key",
            "kind": "anthropic_api_key",
            "scope": "user",
            "secret": ORG_API_KEY
        }),
        "history shape account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");
    let preferences: AgentLaunchPreferencesV1 =
        serde_json::from_value(claude_preferences(Some(explicit_account(account_id))))
            .expect("typed launch preferences");

    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Launch shape',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert agent session");
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .enqueue_agent_input(
            &mut db,
            EnqueueAgentInput {
                task_name: None,
                session_id,
                actor_user_id: owner.id,
                content: "First turn".to_string(),
                artifacts: json!([]),
                input_origin: "human".to_string(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: preferences,
            },
        )
        .await
        .expect("enqueue first turn");
    db.commit().await.expect("commit first turn");

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve workspace agent dispatch")
        .expect("first turn is dispatchable");
    db.commit().await.expect("commit dispatch reservation");

    // The dispatcher-produced envelope decodes through both history readers.
    let (initial, latest) = read_history_launches(&app, org.id, session_id, "envelope").await;
    assert_eq!(
        initial.resolved.resolution_id,
        latest.resolved.resolution_id
    );
    assert_eq!(
        latest
            .resolved
            .account_id
            .map(|id| id.to_string())
            .as_deref(),
        Some(account_id)
    );
    let envelope_resolution_id = latest.resolved.resolution_id;
    let second = send_turn(&client, session_id, "Second turn").await;
    assert_eq!(second["delivery"], "queued", "second turn: {second}");

    // Attempts frozen before the envelope carry the flat resolved contract.
    // An attempt's snapshot is immutable once written, so add a later attempt
    // shaped exactly like such a row (its own resolution id tells the two
    // apart) and let the newest-first reader land on it.
    let legacy_resolution_id = format!("alr_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO task_attempts(
             org_id, task_id, attempt, vm_id, epoch, start_confirmed, started_at,
             policy_snapshot_ref)
         SELECT org_id, task_id, attempt + 1, vm_id, epoch, true, now(),
                jsonb_set(
                    policy_snapshot_ref, '{agent_launch}',
                    jsonb_set(
                        policy_snapshot_ref -> 'agent_launch' -> 'resolved',
                        '{resolutionId}', to_jsonb($3::text)))
         FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(dispatch.task_id)
    .bind(dispatch.attempt)
    .bind(&legacy_resolution_id)
    .execute(&app.db)
    .await
    .expect("insert an attempt frozen with the legacy flat contract");
    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref->'agent_launch'
         FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(dispatch.task_id)
    .bind(dispatch.attempt + 1)
    .fetch_one(&app.db)
    .await
    .expect("load the legacy launch");
    assert_eq!(frozen["accountId"], account_id);
    assert!(frozen.get("resolved").is_none(), "row is flat: {frozen}");

    let (initial, latest) = read_history_launches(&app, org.id, session_id, "legacy flat").await;
    assert_eq!(initial.resolved.resolution_id, envelope_resolution_id);
    assert_eq!(latest.resolved.resolution_id, legacy_resolution_id);
    assert_eq!(
        latest
            .resolved
            .account_id
            .map(|id| id.to_string())
            .as_deref(),
        Some(account_id)
    );
    let third = send_turn(&client, session_id, "Third turn").await;
    assert_eq!(third["delivery"], "queued", "third turn: {third}");

    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM session_inputs
         WHERE session_id = $1 AND delivery = 'queued'",
    )
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .expect("count queued inputs");
    assert_eq!(queued, 2, "both later turns stay durably queued");

    app.cleanup().await;
}

/// Both chat-history accessors must decode the session's frozen launches;
/// returns `(initial, latest)`.
async fn read_history_launches(
    app: &TestApp,
    org_id: Uuid,
    session_id: Uuid,
    shape: &str,
) -> (PersistedAgentLaunchV1, PersistedAgentLaunchV1) {
    let mut db = app.scoped_db_for_org(org_id).await;
    let latest = app
        .state
        .chat_service
        .agent_launch_for_session(&mut db, session_id)
        .await
        .unwrap_or_else(|error| panic!("{shape}: latest launch decodes: {error}"))
        .unwrap_or_else(|| panic!("{shape}: session has a frozen launch"));
    let initial = app
        .state
        .chat_service
        .initial_agent_launch_for_session(&mut db, session_id)
        .await
        .unwrap_or_else(|error| panic!("{shape}: initial launch decodes: {error}"))
        .unwrap_or_else(|| panic!("{shape}: session has an initial launch"));
    db.commit().await.expect("commit history read");
    (initial, latest)
}

/// `POST /sessions/:id/messages` for a later turn; returns the response data.
async fn send_turn(
    client: &helpers::AuthenticatedClient,
    session_id: Uuid,
    content: &str,
) -> Value {
    send_turn_with(client, session_id, json!({ "content": content })).await
}

/// `POST /sessions/:id/messages` with a full request body; returns the
/// response data.
async fn send_turn_with(
    client: &helpers::AuthenticatedClient,
    session_id: Uuid,
    body: Value,
) -> Value {
    let response = client
        .post_json(&format!("/api/v1/sessions/{session_id}/messages"), &body)
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("send message body");
    assert_eq!(status, StatusCode::CREATED, "send message: {body}");
    body["data"].clone()
}

// ===========================================================================
// Next-turn account — a chat started on a named account keeps that account
// for every later turn until the sender picks another (routes/chat D33).
// The MVP cutover dropped the saved account, so a later turn fell back to
// the sender's default assignment: with neither a default nor a raw key it
// failed `missing_llm_credential` (QA session 31fc8e20, 2026-09-28), and
// with either it silently ran on a different credential.
// ===========================================================================

/// Remove the raw Claude key every `TestOrg` seeds, so only an account can
/// authenticate Claude Code (the "No account" settings of the QA report).
async fn remove_raw_claude_key(client: &helpers::AuthenticatedClient, org_id: Uuid) {
    put_config_ok(
        client,
        &org_config_path(org_id),
        json!({ "removeSecretKeys": ["ANTHROPIC_API_KEY"] }),
        "remove the org's raw Claude key",
    )
    .await;
}

/// Start a chat on a confirmed launch naming `account_id`, as the composer
/// does; returns the session id.
async fn start_chat_on_account(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    account_id: &str,
) -> Uuid {
    let preferences = claude_preferences(Some(explicit_account(account_id)));
    let resolved = resolve_ok(
        client,
        workspace_id,
        json!({ "preferences": preferences }),
        "confirm the chat's launch",
    )
    .await;
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{workspace_id}/sessions/start"),
            &json!({
                "content": "First turn",
                "agentLaunch": {
                    "preferences": preferences,
                    "confirmedResolutionId": resolved["resolutionId"],
                },
            }),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.expect("start chat body");
    assert_eq!(status, StatusCode::CREATED, "start chat: {body}");
    // The client hydrates its composer from this response, so it must carry
    // the account the start just saved.
    assert_eq!(
        body["data"]["session"]["nextAccount"],
        explicit_account(account_id),
        "start response: {body}"
    );
    body["data"]["session"]["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("started session id")
}

/// `GET /sessions/:id`; returns the session data.
async fn session_detail(client: &helpers::AuthenticatedClient, session_id: Uuid) -> Value {
    let response = client.get(&format!("/api/v1/sessions/{session_id}")).await;
    let status = response.status();
    let body: Value = response.json().await.expect("session detail body");
    assert_eq!(status, StatusCode::OK, "session detail: {body}");
    body["data"].clone()
}

/// `POST /sessions/:id/messages` that must be refused; returns the error's
/// `details.code`.
async fn refused_turn_code(
    client: &helpers::AuthenticatedClient,
    session_id: Uuid,
    body: Value,
) -> String {
    let response = client
        .post_json(&format!("/api/v1/sessions/{session_id}/messages"), &body)
        .await;
    let (status, json) = plain_error_body(response, "refused turn").await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "refused turn: {json}"
    );
    details_code(&json).to_string()
}

#[tokio::test]
async fn a_chat_started_on_an_account_keeps_it_for_later_turns() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    remove_raw_claude_key(&client, org.id).await;
    let account = create_account(
        &client,
        org.id,
        json!({ "name": "claude-1st", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "chat account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");

    let session_id = start_chat_on_account(&client, workspace.id, account_id).await;
    // The later turn names no account, like an API client or a composer
    // whose Account field is hidden.
    let second = send_turn(&client, session_id, "Second turn").await;
    assert_eq!(second["delivery"], "queued", "second turn: {second}");
    let detail = session_detail(&client, session_id).await;
    assert_eq!(
        detail["nextAccount"],
        explicit_account(account_id),
        "the chat's saved account: {detail}"
    );

    app.cleanup().await;
}

/// A later turn that allocates a new task stores the saved account in that
/// task's launch, and dispatch admission projects that account's credential.
#[tokio::test]
async fn a_later_turn_dispatches_on_the_chats_saved_account() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    remove_raw_claude_key(&client, org.id).await;
    let account = create_account(
        &client,
        org.id,
        json!({ "name": "claude-1st", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "chat account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");

    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    // A chat whose saved next-turn account is the subscription and which has
    // no active task, so the later turn allocates one.
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(
             id, org_id, workspace_id, scope_type, title, created_by, provider_state)
         VALUES ($1,$2,$3,'workspace','Saved account',$4,$5)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .bind(json!({ "claudeCode": { "settings": {
        "model": "claude-opus-4-8[1m]",
        "permissionMode": "bypassPermissions",
        "effort": "high",
        "account": explicit_account(account_id),
    } } }))
    .execute(&app.db)
    .await
    .expect("insert chat with a saved account");

    let turn = send_turn(&client, session_id, "Later turn").await;
    let task_id: Uuid = turn["taskId"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("later turn task id");
    let spec: Value = sqlx::query_scalar("SELECT spec FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(&app.db)
        .await
        .expect("load the later turn's task spec");
    assert_eq!(
        spec["agent"]["agentLaunch"]["account"],
        json!({ "source": "explicit", "value": explicit_account(account_id) }),
        "the new task keeps the chat's account: {spec}"
    );

    let mut db = app.scoped_db_for_org(org.id).await;
    let dispatch = app
        .state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve the later turn")
        .expect("the later turn is dispatchable");
    assert_eq!(dispatch.task_id, task_id);
    let secrets = dispatch.invoke_params["secrets"]
        .as_object()
        .expect("module secrets object");
    assert_eq!(
        secrets.get("CLAUDE_CODE_OAUTH_TOKEN"),
        Some(&json!(USER_OAUTH))
    );
    assert!(!secrets.contains_key("ANTHROPIC_API_KEY"));
    let frozen: Value = sqlx::query_scalar(
        "SELECT policy_snapshot_ref->'agent_launch'
         FROM task_attempts WHERE task_id = $1 AND attempt = $2",
    )
    .bind(dispatch.task_id)
    .bind(dispatch.attempt)
    .fetch_one(db.conn())
    .await
    .expect("load the frozen attempt launch");
    assert_eq!(frozen["resolved"]["accountId"], account_id);
    assert_eq!(frozen["resolved"]["authMode"], "anthropic_oauth");
    db.commit().await.expect("commit dispatch reservation");

    app.cleanup().await;
}

/// A later turn may name another account; the choice is validated like a
/// launch preference and becomes the chat's saved account. The turn's other
/// choices travel with the input only
/// (chat_agent I42), so they leave the saved settings alone.
#[tokio::test]
async fn a_later_turn_can_switch_the_chats_account_and_the_choice_is_saved() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    remove_raw_claude_key(&client, org.id).await;
    let first = create_account(
        &client,
        org.id,
        json!({ "name": "claude-1st", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "first account",
    )
    .await;
    let first_id = first["id"].as_str().expect("first account id");
    let second = create_account(
        &client,
        org.id,
        json!({ "name": "My key", "kind": "anthropic_api_key", "scope": "user", "secret": ORG_API_KEY }),
        "second account",
    )
    .await;
    let second_id = second["id"].as_str().expect("second account id");
    let codex = create_account(
        &client,
        org.id,
        json!({ "name": "My OpenAI key", "kind": "openai_api_key", "scope": "user", "secret": ROTATED_KEY }),
        "codex-only account",
    )
    .await;
    let codex_id = codex["id"].as_str().expect("codex account id");
    let session_id = start_chat_on_account(&client, workspace.id, first_id).await;

    send_turn_with(
        &client,
        session_id,
        json!({ "content": "Use my key", "account": explicit_account(second_id), "effort": "medium" }),
    )
    .await;
    let detail = session_detail(&client, session_id).await;
    assert_eq!(
        detail["nextAccount"],
        explicit_account(second_id),
        "{detail}"
    );
    assert_eq!(
        detail["providerState"]["claudeCode"]["settings"]["effort"], "high",
        "only the account is saved; the effort stays the chat's own: {detail}"
    );

    // An explicit Runtime Config choice is honored, not replaced: there is
    // no raw key, so the turn is refused and nothing is saved.
    assert_eq!(
        refused_turn_code(
            &client,
            session_id,
            json!({ "content": "Use the raw key", "account": { "kind": "runtime_config" } }),
        )
        .await,
        "missing_llm_credential"
    );
    // A Codex-only account cannot run Claude Code.
    assert_eq!(
        refused_turn_code(
            &client,
            session_id,
            json!({ "content": "Use the OpenAI key", "account": explicit_account(codex_id) }),
        )
        .await,
        "account_not_visible"
    );
    let detail = session_detail(&client, session_id).await;
    assert_eq!(
        detail["nextAccount"],
        explicit_account(second_id),
        "{detail}"
    );

    // With the first account gone, an unnamed later turn still runs: it
    // uses the saved second account.
    let deleted = client.delete(&account_path(org.id, first_id)).await;
    assert!(
        deleted.status().is_success(),
        "delete the first account: {}",
        deleted.status()
    );
    send_turn(&client, session_id, "Keep going").await;
    let detail = session_detail(&client, session_id).await;
    assert_eq!(
        detail["nextAccount"],
        explicit_account(second_id),
        "{detail}"
    );

    app.cleanup().await;
}

/// A chat started before its account was saved (during the regression)
/// continues on the account its latest turn ran on.
#[tokio::test]
async fn a_chat_without_a_saved_account_continues_on_its_latest_turns_account() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    remove_raw_claude_key(&client, org.id).await;
    let account = create_account(
        &client,
        org.id,
        json!({ "name": "claude-1st", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "chat account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");
    let preferences: AgentLaunchPreferencesV1 =
        serde_json::from_value(claude_preferences(Some(explicit_account(account_id))))
            .expect("typed launch preferences");

    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_sessions(id, org_id, workspace_id, scope_type, title, created_by)
         VALUES ($1,$2,$3,'workspace','Unsaved account',$4)",
    )
    .bind(session_id)
    .bind(org.id)
    .bind(workspace.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .expect("insert chat without a saved account");
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .enqueue_agent_input(
            &mut db,
            EnqueueAgentInput {
                task_name: None,
                session_id,
                actor_user_id: owner.id,
                content: "First turn".to_string(),
                artifacts: json!([]),
                input_origin: "human".to_string(),
                template_ref: None,
                template_revision: None,
                confirmed_launch: None,
                agent_launch: preferences,
            },
        )
        .await
        .expect("enqueue the first turn");
    db.commit().await.expect("commit the first turn");
    let mut db = app.scoped_db_for_org(org.id).await;
    app.state
        .task_service
        .reserve_agent_dispatch(&mut db)
        .await
        .expect("reserve the first turn")
        .expect("the first turn is dispatchable");
    db.commit().await.expect("commit dispatch reservation");
    assert_eq!(
        session_detail(&client, session_id).await["nextAccount"],
        Value::Null
    );

    // With no raw key, the unnamed later turn can only run on the account
    // its first turn froze, the same account the composer shows.
    let second = send_turn(&client, session_id, "Second turn").await;
    assert_eq!(second["delivery"], "queued", "second turn: {second}");
    let detail = session_detail(&client, session_id).await;
    assert_eq!(detail["account"]["id"], account_id, "{detail}");
    assert_eq!(
        detail["nextAccount"],
        Value::Null,
        "an unnamed turn saves nothing: {detail}"
    );

    app.cleanup().await;
}

/// A malformed saved account fails a later turn that names no account with a
/// typed reason, never a silent fallback. A turn that names an account does
/// not read it, and saving that account recovers the chat.
#[tokio::test]
async fn a_named_account_recovers_a_chat_whose_saved_account_is_malformed() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let client = app.authenticated_client(&owner);
    let account = create_account(
        &client,
        org.id,
        json!({ "name": "claude-1st", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "chat account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");
    let session_id = start_chat_on_account(&client, workspace.id, account_id).await;
    sqlx::query(
        r#"UPDATE chat_sessions
           SET provider_state = jsonb_set(
               provider_state, '{claudeCode,settings,account}', '{"kind":"bogus"}')
           WHERE id = $1"#,
    )
    .bind(session_id)
    .execute(&app.db)
    .await
    .expect("corrupt the saved account");

    assert_eq!(
        refused_turn_code(&client, session_id, json!({ "content": "Keep going" })).await,
        "invalid_persisted_session_account"
    );
    send_turn_with(
        &client,
        session_id,
        json!({ "content": "Use my account", "account": explicit_account(account_id) }),
    )
    .await;
    let detail = session_detail(&client, session_id).await;
    assert_eq!(
        detail["nextAccount"],
        explicit_account(account_id),
        "{detail}"
    );
    send_turn_with(&client, session_id, json!({ "content": "Keep going" })).await;

    app.cleanup().await;
}

/// After a transfer the chat's saved account can be the previous owner's
/// personal account. The new owner's unnamed later turn is refused with a
/// typed reason instead of silently switching credentials, and a turn naming
/// another choice is accepted and saved.
#[tokio::test]
async fn a_transferred_chat_refuses_the_previous_owners_saved_account() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::SessionsRead,
            Scope::SessionsWrite,
            Scope::TasksExecute,
        ],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let _runtime = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let client = app.authenticated_client(&owner);
    let account = create_account(
        &client,
        org.id,
        json!({ "name": "claude-1st", "kind": "claude_subscription", "scope": "user", "secret": USER_OAUTH }),
        "previous owner's account",
    )
    .await;
    let account_id = account["id"].as_str().expect("account id");
    let session_id = start_chat_on_account(&client, workspace.id, account_id).await;

    let transferred = client
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({
                "toUserId": next_owner.id,
                "idempotencyKey": format!("transfer-{}", Uuid::new_v4())
            }),
        )
        .await;
    let status = transferred.status();
    let transferred: Value = transferred.json().await.expect("transfer body");
    assert_eq!(status, StatusCode::OK, "transfer: {transferred}");
    assert_eq!(transferred["data"]["status"], "completed", "{transferred}");

    let next_client = app.authenticated_client(&next_owner);
    assert_eq!(
        refused_turn_code(&next_client, session_id, json!({ "content": "Continue" })).await,
        "account_not_visible"
    );
    // The org's raw key still authenticates an explicit Runtime Config turn.
    send_turn_with(
        &next_client,
        session_id,
        json!({ "content": "Continue on the raw key", "account": { "kind": "runtime_config" } }),
    )
    .await;
    let detail = session_detail(&next_client, session_id).await;
    assert_eq!(
        detail["nextAccount"],
        json!({ "kind": "runtime_config" }),
        "{detail}"
    );

    app.cleanup().await;
}

// ===========================================================================
// Usage — `test_contract/provider_accounts` **I11** step 3 (server half) and
// the workspace read by account (`routes/provider_accounts` D6,
// `routes/workspaces` D36, `models/provider_account` D4,
// `external_agent_provider_contract` D13).
// ===========================================================================

async fn upsert_snapshot(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    account_id: Uuid,
    five_hour: f32,
    captured_at: DateTime<Utc>,
) {
    let mut db = app.scoped_db_for_org(org_id).await;
    let stored = app
        .state
        .provider_quota_service
        .upsert_snapshot(
            &mut db,
            ProviderQuotaSnapshotInput {
                org_id,
                workspace_id,
                provider: "claude_code".to_string(),
                auth_mode: "anthropic_oauth".to_string(),
                account_id,
                five_hour_utilization: Some(five_hour),
                five_hour_reset_at: Some(captured_at + Duration::hours(3)),
                seven_day_utilization: Some(0.2),
                seven_day_reset_at: None,
                representative_claim: Some("claim".to_string()),
                unified_status: Some("allowed".to_string()),
                captured_at,
            },
        )
        .await
        .expect("snapshot upsert");
    assert!(stored, "fixture snapshot must be stored");
    db.commit().await.expect("commit snapshot");
}

async fn workspace_quota(
    client: &helpers::AuthenticatedClient,
    workspace_id: Uuid,
    account_id: Option<Uuid>,
    context: &str,
) -> Value {
    let path = match account_id {
        Some(id) => format!("/api/v1/workspaces/{workspace_id}/provider-quota?accountId={id}"),
        None => format!("/api/v1/workspaces/{workspace_id}/provider-quota"),
    };
    let response = client.get(&path).await;
    let status = response.status();
    let json: Value = response.json().await.expect("quota response is JSON");
    assert_eq!(status, StatusCode::OK, "{context}: {json}");
    json["data"].clone()
}

fn utilization(value: &Value, context: &str) -> f64 {
    value["fiveHourUtilization"]
        .as_f64()
        .unwrap_or_else(|| panic!("{context}: fiveHourUtilization missing in {value}"))
}

/// I11 step 3: the list carries, for a Claude subscription account, the latest
/// snapshot across two workspaces (not the newest row of another key), while
/// an API-key account and a subscription account without a snapshot carry
/// `null`; the workspace read keys by `accountId` and returns the nil row
/// otherwise, never falling back across keys.
#[tokio::test]
async fn subscription_accounts_list_the_latest_snapshot_across_workspaces_and_reads_key_by_account()
{
    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    let admin = seed_admin(&app, &org).await;
    let admin_client = app.authenticated_client(&admin);
    let workspace_one = TestWorkspace::create(&app, &org).await;
    let workspace_two = TestWorkspace::create(&app, &org).await;

    let subscription = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team subscription", "kind": "claude_subscription", "scope": "org", "secret": USER_OAUTH }),
        "subscription account",
    )
    .await;
    let idle_subscription = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Idle subscription", "kind": "claude_subscription", "scope": "org", "secret": USER_OAUTH }),
        "subscription account without snapshots",
    )
    .await;
    let api_key = create_account(
        &admin_client,
        org.id,
        json!({ "name": "Team key", "kind": "anthropic_api_key", "scope": "org", "secret": ORG_API_KEY }),
        "api key account",
    )
    .await;
    assert!(
        subscription["usage"].is_null() && api_key["usage"].is_null(),
        "create responses never carry usage"
    );
    let subscription_id: Uuid = subscription["id"].as_str().unwrap().parse().unwrap();
    let idle_id: Uuid = idle_subscription["id"].as_str().unwrap().parse().unwrap();
    let api_key_id: Uuid = api_key["id"].as_str().unwrap().parse().unwrap();

    // Postgres stores microseconds; truncate so `to_rfc3339` round-trips.
    let base = (Utc::now() - Duration::hours(1)).trunc_subsecs(6);
    let latest = base + Duration::minutes(5);
    upsert_snapshot(&app, org.id, workspace_one.id, subscription_id, 0.5, base).await;
    upsert_snapshot(
        &app,
        org.id,
        workspace_two.id,
        subscription_id,
        0.75,
        latest,
    )
    .await;
    // The nil (account-less) row is the newest of all but belongs to no account.
    upsert_snapshot(
        &app,
        org.id,
        workspace_one.id,
        Uuid::nil(),
        0.25,
        base + Duration::minutes(10),
    )
    .await;
    // A row keyed by the API-key account id must not surface on that kind.
    upsert_snapshot(
        &app,
        org.id,
        workspace_one.id,
        api_key_id,
        0.9,
        base + Duration::minutes(20),
    )
    .await;

    let list = list_accounts(&admin_client, org.id, None, "list with usage").await;
    let rows = list["data"].as_array().expect("list data");
    let row = |id: Uuid| {
        rows.iter()
            .find(|row| row["id"] == id.to_string())
            .unwrap_or_else(|| panic!("account {id} listed"))
    };
    let usage = &row(subscription_id)["usage"];
    assert!(
        (utilization(usage, "subscription usage") - 0.75).abs() < 1e-6,
        "latest snapshot across workspaces wins: {usage}"
    );
    assert_eq!(usage["capturedAt"], latest.to_rfc3339());
    assert_eq!(
        usage["fiveHourResetAt"],
        (latest + Duration::hours(3)).to_rfc3339()
    );
    assert!((usage["sevenDayUtilization"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert!(usage["sevenDayResetAt"].is_null());
    let mut keys: Vec<&str> = usage
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "capturedAt",
            "fiveHourResetAt",
            "fiveHourUtilization",
            "sevenDayResetAt",
            "sevenDayUtilization"
        ],
        "usage carries the windows and capture time only"
    );
    assert!(
        row(idle_id)["usage"].is_null(),
        "a subscription account without a snapshot carries null"
    );
    assert!(
        row(api_key_id)["usage"].is_null(),
        "an API-key account carries no usage even when a row bears its id"
    );

    // Workspace read: keyed by account when given, nil row otherwise, and no
    // cross-key fallback.
    let ws_one_sub = workspace_quota(
        &admin_client,
        workspace_one.id,
        Some(subscription_id),
        "workspace one by account",
    )
    .await;
    assert!((utilization(&ws_one_sub, "workspace one by account") - 0.5).abs() < 1e-6);
    let ws_two_sub = workspace_quota(
        &admin_client,
        workspace_two.id,
        Some(subscription_id),
        "workspace two by account",
    )
    .await;
    assert!((utilization(&ws_two_sub, "workspace two by account") - 0.75).abs() < 1e-6);
    let ws_one_nil =
        workspace_quota(&admin_client, workspace_one.id, None, "workspace one nil").await;
    assert!((utilization(&ws_one_nil, "workspace one nil") - 0.25).abs() < 1e-6);
    let ws_two_nil =
        workspace_quota(&admin_client, workspace_two.id, None, "workspace two nil").await;
    assert!(
        ws_two_nil.is_null(),
        "no nil row in workspace two: no fallback to the account row: {ws_two_nil}"
    );
    let ws_two_idle = workspace_quota(
        &admin_client,
        workspace_two.id,
        Some(idle_id),
        "workspace two idle account",
    )
    .await;
    assert!(ws_two_idle.is_null(), "unknown account key reads no row");

    app.cleanup().await;
}
