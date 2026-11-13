//! Integration tests for the six background maintenance tasks.
//!
//! Each test exercises a maintenance task end-to-end against a real Postgres
//! database: seed state → run maintenance function → assert database state.
//!
//! Tests invoke maintenance functions via the public loop functions with short
//! intervals and a `CancellationToken` to trigger one pass and then exit.
//! For `aggregate_usage`, the public inner function is called directly.

use crate::helpers;

use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use g5_api::config::MaintenanceConfig;
use g5_api::maintenance::invitation_gc;
use g5_api::maintenance::repo_gc;
use g5_api::maintenance::timeline_cleanup::TimelineCleanupConfig;
use g5_api::maintenance::usage_aggregation::aggregate_usage;
use g5_api::maintenance::{MaintenanceRunner, MaintenanceRunnerDeps};
use g5_api::services::{EventBus, PermissionService};
use g5_api::ws::{WsHub, WsHubConfig};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

// ===========================================================================
// Database fixture helpers
// ===========================================================================

/// Creates an org_invitations row with configurable status and timestamps.
async fn create_invitation(
    pool: &sqlx::PgPool,
    org_id: Uuid,
    email: &str,
    status: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    let id = Uuid::new_v4();
    // invited_by must reference an existing user; create one on-the-fly.
    let invited_by = Uuid::new_v4();
    let oauth_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, display_name, oauth_provider, oauth_id) VALUES ($1, $2, $3, 'github', $4)",
    )
    .bind(invited_by)
    .bind(format!("inviter-{}@g5.test", invited_by))
    .bind("Inviter")
    .bind(&oauth_id)
    .execute(pool)
    .await
    .expect("Failed to create inviting user");
    sqlx::query(
        r#"
        INSERT INTO org_invitations (
            id, org_id, email, role, invited_by, status,
            expires_at, created_at, updated_at
        )
        VALUES ($1, $2, $3, 'member', $4, $5, $6, now(), $7)
        "#,
    )
    .bind(id)
    .bind(org_id)
    .bind(email)
    .bind(invited_by)
    .bind(status)
    .bind(expires_at)
    .bind(updated_at)
    .execute(pool)
    .await
    .expect("Failed to create invitation");
    id
}

/// Creates a usage_records row for a task or session.
#[allow(clippy::too_many_arguments)]
async fn create_usage_record(
    pool: &sqlx::PgPool,
    org_id: Uuid,
    workspace_id: Uuid,
    task_id: Option<Uuid>,
    session_id: Option<Uuid>,
    cost_usd: f64,
    input_tokens: i64,
    output_tokens: i64,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO usage_records (
            id, org_id, workspace_id, task_id, session_id,
            provider, model, input_tokens, output_tokens, total_tokens,
            cost_usd, request_type, created_at
        )
        VALUES ($1, $2, $3, $4, $5,
                'anthropic', 'claude-3', $6, $7, $6 + $7,
                $8, 'completion', now())
        "#,
    )
    .bind(id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(task_id)
    .bind(session_id)
    .bind(input_tokens)
    .bind(output_tokens)
    .bind(cost_usd)
    .execute(pool)
    .await
    .expect("Failed to create usage record");
    id
}

/// Creates a timeline_entries row for a task.
async fn create_timeline_entry(
    pool: &sqlx::PgPool,
    task_id: Uuid,
    entry_type: &str,
    content: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO timeline_entries (id, task_id, entry_type, content, created_at, org_id)
        VALUES ($1, $2, $3, $4, $5,
                (SELECT org_id FROM tasks WHERE id = $2))
        "#,
    )
    .bind(id)
    .bind(task_id)
    .bind(entry_type)
    .bind(content)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("Failed to create timeline entry");
    id
}

/// Creates a repo row for workspace branch references.
async fn create_repo(pool: &sqlx::PgPool, org_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    let slug = format!("test-repo-{}", Uuid::new_v4().simple());
    sqlx::query(
        r#"
        INSERT INTO repos (id, org_id, name, slug, git_url, default_branch, provider, created_at)
        VALUES ($1, $2, $3, $4, 'https://example.com/repo.git', 'main', 'github', now())
        "#,
    )
    .bind(id)
    .bind(org_id)
    .bind(&slug)
    .bind(&slug)
    .execute(pool)
    .await
    .expect("Failed to create repo");
    id
}

/// Checks if an invitation exists by ID.
async fn invitation_exists(pool: &sqlx::PgPool, id: Uuid) -> bool {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM org_invitations WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("Failed to check invitation existence");
    row.0 > 0
}

/// Counts timeline_entries for a task.
async fn count_timeline_entries(pool: &sqlx::PgPool, task_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM timeline_entries WHERE task_id = $1")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("Failed to count timeline entries");
    row.0
}

/// Gets task usage counters.
async fn get_task_usage(pool: &sqlx::PgPool, task_id: Uuid) -> (f64, i64, i64) {
    let row: (rust_decimal::Decimal, i64, i64) =
        sqlx::query_as("SELECT cost_usd, input_tokens, output_tokens FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_one(pool)
            .await
            .expect("Failed to get task usage");
    use rust_decimal::prelude::ToPrimitive;
    (row.0.to_f64().unwrap_or(0.0), row.1, row.2)
}

// ===========================================================================
// Helper to create a task with specific usage counters
// ===========================================================================

async fn create_task_with_counters(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    status: &str,
    cost_usd: f64,
    input_tokens: i64,
    output_tokens: i64,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO tasks (
            id, workspace_id, name, kind, status,
            input_tokens, output_tokens, cost_usd,
            created_by, created_at, org_id
        )
        VALUES ($1, $2, 'test-task', 'command', $3,
                $4, $5, $6,
                $7, now(),
                (SELECT org_id FROM workspaces WHERE id = $2))
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(status)
    .bind(input_tokens)
    .bind(output_tokens)
    .bind(cost_usd)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to create task");
    id
}

/// Creates a task row for timeline cleanup tests.
async fn create_task(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
    status: &str,
    updated_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO tasks (
            id, workspace_id, name, kind, status,
            retry_count, created_by, created_at, updated_at, org_id
        )
        SELECT $1, workspace.id, 'test-task', 'command', $3,
               0, workspace.owner_user_id, now(), $4, workspace.org_id
        FROM workspaces AS workspace WHERE workspace.id = $2
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(status)
    .bind(updated_at)
    .execute(pool)
    .await
    .expect("Failed to create task");
    id
}

/// Helper: spawn a maintenance loop, wait for one pass, then cancel.
/// Uses the caller's short interval to trigger the first tick quickly.
async fn run_loop_once<F, Fut>(pool: sqlx::PgPool, f: F)
where
    F: FnOnce(sqlx::PgPool, tokio_util::sync::CancellationToken) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move {
        f(pool, cancel_clone).await;
    });
    // Wait for one pass to complete (first tick fires immediately for most loops)
    tokio::time::sleep(Duration::from_secs(2)).await;
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
}

fn tenant_maintenance_deps(app: &TestApp) -> g5_api::maintenance::TenantMaintenanceDeps {
    g5_api::maintenance::TenantMaintenanceDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
    }
}

// ===========================================================================
// 1. Authorization Maintenance Tests
// ===========================================================================

#[tokio::test]
async fn workspace_write_scope_check_requires_current_membership() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(PermissionService::user_has_workspace_scope(
        &app.system_pool,
        &mut db,
        user.id,
        org.id,
        workspace.id,
        "workspaces:write",
    )
    .await
    .expect("scope check should succeed before removal"));
    db.commit().await.expect("commit scoped db");

    let replacement = TestUser::create_admin(&app, org.id).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(replacement.id)
        .execute(&app.db)
        .await
        .expect("transfer workspace away from the actor before membership removal");

    sqlx::query("DELETE FROM org_memberships WHERE org_id = $1 AND user_id = $2")
        .bind(org.id)
        .bind(user.id)
        .execute(&app.db)
        .await
        .expect("remove enabling actor membership");

    let mut db = app.scoped_db_for_org(org.id).await;
    assert!(
        !PermissionService::user_has_workspace_scope(
            &app.system_pool,
            &mut db,
            user.id,
            org.id,
            workspace.id,
            "workspaces:write",
        )
        .await
        .expect("scope check should fail closed after removal"),
        "workspace write scope must fail closed for a removed member"
    );
    db.commit().await.expect("commit scoped db");

    app.cleanup().await;
}

/// Fixed ordinary readership preserves scope eligibility across transfer.
/// Mutation/execution admission separately requires current ownership.
#[tokio::test]
async fn workspace_scope_checks_do_not_conflate_audience_with_ownership() {
    use g5_auth::Scope;

    let app = TestApp::spawn().await;
    let org = TestOrg::create(&app).await;
    // The first member becomes the fixture workspace's owner and creator.
    let creator = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let next_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query(
        "INSERT INTO oauth_scopes (scope, description, category)
         VALUES ('tasks:execute', 'Execute tasks', 'tasks') ON CONFLICT (scope) DO NOTHING",
    )
    .execute(&app.db)
    .await
    .expect("register tasks:execute");
    for user in [creator.id, next_owner.id] {
        sqlx::query(
            "INSERT INTO user_scope_grants (org_id, user_id, scope) VALUES ($1, $2, 'tasks:execute')",
        )
        .bind(org.id)
        .bind(user)
        .execute(&app.db)
        .await
        .expect("grant tasks:execute");
    }
    let moved = sqlx::query(
        "UPDATE workspaces SET owner_user_id = $2
         WHERE id = $1 AND created_by = $3",
    )
    .bind(workspace.id)
    .bind(next_owner.id)
    .bind(creator.id)
    .execute(&app.db)
    .await
    .expect("move the ordinary workspace to its next owner");
    assert_eq!(moved.rows_affected(), 1, "the creator created it");

    for (user, expected, who) in [
        (creator.id, true, "former creator"),
        (next_owner.id, true, "current owner"),
    ] {
        let mut db = app.scoped_db_for_org(org.id).await;
        let write = PermissionService::user_has_workspace_scope(
            &app.system_pool,
            &mut db,
            user,
            org.id,
            workspace.id,
            "workspaces:write",
        )
        .await
        .expect("workspace scope check");
        db.commit().await.expect("commit scoped db");
        assert_eq!(write, expected, "{who}: workspaces:write");

        let mut conn = app
            .db
            .acquire()
            .await
            .expect("acquire direct DB connection");
        let execute = PermissionService::user_has_workspace_execute_on_conn(
            &mut conn,
            user,
            org.id,
            workspace.id,
        )
        .await
        .expect("execute check");
        assert_eq!(execute, expected, "{who}: tasks:execute");
    }

    app.cleanup().await;
}

#[tokio::test]
async fn workspace_execute_on_conn_rejects_tombstoned_workspace() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let mut conn = app
        .db
        .acquire()
        .await
        .expect("acquire direct DB connection");
    assert!(
        PermissionService::user_has_workspace_execute_on_conn(
            &mut conn,
            user.id,
            org.id,
            workspace.id,
        )
        .await
        .expect("execute check should succeed before tombstone"),
        "owner should be able to execute tasks in a live workspace"
    );
    drop(conn);

    sqlx::query("UPDATE workspaces SET deleted_at = now(), updated_at = now() WHERE id = $1")
        .bind(workspace.id)
        .execute(&app.db)
        .await
        .expect("tombstone workspace");

    let mut conn = app
        .db
        .acquire()
        .await
        .expect("acquire direct DB connection");
    assert!(
        !PermissionService::user_has_workspace_execute_on_conn(
            &mut conn,
            user.id,
            org.id,
            workspace.id,
        )
        .await
        .expect("execute check should fail closed after tombstone"),
        "execute-on-connection must reject tombstoned workspaces"
    );
    drop(conn);

    app.cleanup().await;
}

// ===========================================================================
// 3. Invitation GC Tests
// ===========================================================================

#[tokio::test]
async fn test_maintenance_invitation_gc_expires_and_deletes() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;

    let now = chrono::Utc::now();

    // Invitation A: pending, expires_at = now - 1 hour (should be expired by GC)
    let inv_a = create_invitation(
        &app.db,
        org.id,
        "a@test.com",
        "pending",
        now - chrono::Duration::hours(1),
        now,
    )
    .await;

    // Invitation B: pending, expires_at = now + 24 hours (should NOT be expired)
    let inv_b = create_invitation(
        &app.db,
        org.id,
        "b@test.com",
        "pending",
        now + chrono::Duration::hours(24),
        now,
    )
    .await;

    // Invitation C: expired, updated_at = now - 91 days (should be permanently deleted)
    let inv_c = create_invitation(
        &app.db,
        org.id,
        "c@test.com",
        "expired",
        now - chrono::Duration::days(100),
        now - chrono::Duration::days(91),
    )
    .await;

    // Invitation D: accepted, updated_at = now - 120 days (should NEVER be deleted)
    let inv_d = create_invitation(
        &app.db,
        org.id,
        "d@test.com",
        "accepted",
        now - chrono::Duration::days(130),
        now - chrono::Duration::days(120),
    )
    .await;

    let config = MaintenanceConfig {
        invitation_gc_interval: Duration::from_secs(1), // short for test
        stale_draft_timeout: Duration::from_secs(86400),
        draft_stale_retention: Duration::from_secs(7 * 24 * 3600),
        repo_gc_interval: Duration::from_secs(3600),
        repo_gc_loose_threshold: 500,
        remote_tracking_scheduler_interval: Duration::from_secs(60),
        remote_tracking_advisory_interval: Duration::from_secs(60),
        passive_auto_pull_enabled: true,
    };

    // Run invitation GC — first tick fires immediately
    run_loop_once(app.system_pool.clone(), move |pool, cancel| async move {
        invitation_gc::run_invitation_gc(pool, config, cancel).await;
    })
    .await;

    // Assert Invitation A: should now be expired
    let row: (String,) = sqlx::query_as("SELECT status FROM org_invitations WHERE id = $1")
        .bind(inv_a)
        .fetch_one(&app.db)
        .await
        .expect("Invitation A should still exist");
    assert_eq!(row.0, "expired", "Invitation A should be expired");

    // Assert Invitation B: should remain pending
    let row: (String,) = sqlx::query_as("SELECT status FROM org_invitations WHERE id = $1")
        .bind(inv_b)
        .fetch_one(&app.db)
        .await
        .expect("Invitation B should still exist");
    assert_eq!(row.0, "pending", "Invitation B should remain pending");

    // Assert Invitation C: should be permanently deleted
    assert!(
        !invitation_exists(&app.db, inv_c).await,
        "Invitation C should be permanently deleted"
    );

    // Assert Invitation D: should still exist (accepted invitations are never touched)
    assert!(
        invitation_exists(&app.db, inv_d).await,
        "Invitation D (accepted) should still exist"
    );
    let row: (String,) = sqlx::query_as("SELECT status FROM org_invitations WHERE id = $1")
        .bind(inv_d)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(row.0, "accepted", "Invitation D should remain accepted");

    app.cleanup().await;
}

// ===========================================================================
// 4. Usage Aggregation Tests
// ===========================================================================

#[tokio::test]
async fn test_maintenance_usage_aggregation_reconciles_drift() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Create a completed task with zeroed denormalized counters (simulating drift)
    let task_id =
        create_task_with_counters(&app.db, workspace.id, user.id, "succeeded", 0.0, 0, 0).await;

    // Create 3 usage records for this task
    create_usage_record(
        &app.db,
        org.id,
        workspace.id,
        Some(task_id),
        None,
        0.10,
        100,
        50,
    )
    .await;
    create_usage_record(
        &app.db,
        org.id,
        workspace.id,
        Some(task_id),
        None,
        0.20,
        200,
        100,
    )
    .await;
    create_usage_record(
        &app.db,
        org.id,
        workspace.id,
        Some(task_id),
        None,
        0.05,
        50,
        25,
    )
    .await;

    // Reconciliation must continue across organizations once usage_records
    // itself enforces RLS, without mixing either organization's aggregates.
    let mut other_user = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other_user).await;
    let other_workspace = TestWorkspace::create(&app, &other_org).await;
    let other_task = create_task_with_counters(
        &app.db,
        other_workspace.id,
        other_user.id,
        "succeeded",
        0.0,
        0,
        0,
    )
    .await;
    create_usage_record(
        &app.db,
        other_org.id,
        other_workspace.id,
        Some(other_task),
        None,
        0.80,
        800,
        400,
    )
    .await;

    // Call aggregate_usage directly (it's pub)
    let result = aggregate_usage(&tenant_maintenance_deps(&app))
        .await
        .expect("aggregate_usage should succeed");

    assert!(
        result.tasks_reconciled >= 1,
        "At least 1 task should be reconciled"
    );
    assert!(
        result.total_discrepancies >= 1,
        "At least 1 discrepancy should be found"
    );

    // Query the task row directly
    let (cost, input, output) = get_task_usage(&app.db, task_id).await;
    assert!(
        (cost - 0.35).abs() < 0.01,
        "Task cost_usd should be ~0.35, got {}",
        cost
    );
    assert_eq!(input, 350, "Task input_tokens should be 350");
    assert_eq!(output, 175, "Task output_tokens should be 175");
    let (other_cost, other_input, other_output) = get_task_usage(&app.db, other_task).await;
    assert!(
        (other_cost - 0.80).abs() < 0.01,
        "other org cost: {other_cost}"
    );
    assert_eq!((other_input, other_output), (800, 400));

    app.cleanup().await;
}

#[tokio::test]
async fn test_maintenance_usage_aggregation_no_drift() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    // Create a completed task with correct denormalized counters
    let task_id =
        create_task_with_counters(&app.db, workspace.id, user.id, "succeeded", 0.30, 300, 150)
            .await;

    // Create usage records that sum to the same values
    create_usage_record(
        &app.db,
        org.id,
        workspace.id,
        Some(task_id),
        None,
        0.10,
        100,
        50,
    )
    .await;
    create_usage_record(
        &app.db,
        org.id,
        workspace.id,
        Some(task_id),
        None,
        0.20,
        200,
        100,
    )
    .await;

    // Run usage aggregation
    let _result = aggregate_usage(&tenant_maintenance_deps(&app))
        .await
        .expect("aggregate_usage should succeed");

    // Verify the task row is unchanged
    let (cost, input, output) = get_task_usage(&app.db, task_id).await;
    assert!(
        (cost - 0.30).abs() < 0.01,
        "Task cost_usd should remain 0.30, got {}",
        cost
    );
    assert_eq!(input, 300, "Task input_tokens should remain 300");
    assert_eq!(output, 150, "Task output_tokens should remain 150");

    app.cleanup().await;
}

// ===========================================================================
// 5. Timeline Cleanup Tests
// ===========================================================================

#[tokio::test]
async fn test_maintenance_timeline_cleanup_prunes_old_entries() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let workspace = TestWorkspace::create(&app, &org).await;

    let completed_task_id = create_task(
        &app.db,
        workspace.id,
        "succeeded",
        chrono::Utc::now() - chrono::Duration::days(31),
    )
    .await;

    // Insert 150 timeline entries for the completed task spread over 60 days
    let now = chrono::Utc::now();
    for i in 0..150 {
        let created_at = now - chrono::Duration::days(60) + chrono::Duration::hours(i * 10);
        create_timeline_entry(
            &app.db,
            completed_task_id,
            "output",
            &format!("Entry {}", i),
            created_at,
        )
        .await;
    }

    // Create a running task with 200 timeline entries (should NOT be pruned)
    let running_task_id = create_task(
        &app.db,
        workspace.id,
        "running",
        chrono::Utc::now() - chrono::Duration::days(60),
    )
    .await;
    for i in 0..200 {
        let created_at = now - chrono::Duration::days(60) + chrono::Duration::hours(i * 7);
        create_timeline_entry(
            &app.db,
            running_task_id,
            "tool_call",
            &format!("Running entry {}", i),
            created_at,
        )
        .await;
    }

    // Run timeline cleanup with short interval
    let config = TimelineCleanupConfig {
        interval: Duration::from_secs(1),
        retention_period: Duration::from_secs(30 * 86400), // 30 days
        entries_to_keep: 100,
        delete_batch_size: 50,
    };

    let cancel = tokio_util::sync::CancellationToken::new();
    let deps = tenant_maintenance_deps(&app);
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move {
        g5_api::maintenance::timeline_cleanup::run_timeline_cleanup_loop(
            deps,
            config,
            cancel_clone,
        )
        .await;
    });

    // Wait for cleanup to run (first tick skipped, second fires after 1s)
    tokio::time::sleep(Duration::from_secs(3)).await;
    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;

    // Assert completed task: should have exactly 100 entries (most recent kept)
    let completed_count = count_timeline_entries(&app.db, completed_task_id).await;
    assert_eq!(
        completed_count, 100,
        "Completed task should have exactly 100 entries after cleanup, got {}",
        completed_count
    );

    // Assert running task: all 200 entries should remain
    let running_count = count_timeline_entries(&app.db, running_task_id).await;
    assert_eq!(
        running_count, 200,
        "Running task should have all 200 entries preserved, got {}",
        running_count
    );

    app.cleanup().await;
}

// ===========================================================================
// 6. MaintenanceRunner Spawn/Cancel Test
// ===========================================================================

#[tokio::test]
async fn test_maintenance_runner_spawns_and_cancels() {
    let app = TestApp::spawn().await;

    let config = MaintenanceConfig {
        invitation_gc_interval: Duration::from_secs(3600),
        stale_draft_timeout: Duration::from_secs(86400),
        draft_stale_retention: Duration::from_secs(7 * 24 * 3600),
        repo_gc_interval: Duration::from_secs(3600),
        repo_gc_loose_threshold: 500,
        remote_tracking_scheduler_interval: Duration::from_secs(60),
        remote_tracking_advisory_interval: Duration::from_secs(60),
        passive_auto_pull_enabled: true,
    };

    let cancel = tokio_util::sync::CancellationToken::new();
    let app_config = helpers::test_config();
    let event_bus = EventBus::new(Arc::new(WsHub::new(WsHubConfig::default())));

    let runner = MaintenanceRunner::new(
        MaintenanceRunnerDeps {
            system_pool: app.system_pool.clone(),
            authz_pool: app.authz_pool.clone(),
            tenant_pool: app.state.tenant_pool.clone(),
            authz: app.state.authz.clone(),
            repo_storage: app.repo_storage.clone(),
            event_bus,
            git_bin: app.state.git_bin.clone(),
            repository_credentials: app.state.repository_credentials.clone(),
        },
        config,
        cancel.clone(),
        app_config,
    );

    let handles = runner.spawn_all();

    // Assert that every unconditional final-model loop is spawned, including
    // workspace-flush and remote-sync-run retention. The retired protected-ref
    // mirror and Base auto-pull scheduler are absent. The test config
    // intentionally leaves the optional platform-issue outbox disabled.
    assert_eq!(
        handles.len(),
        13,
        "spawn_all should return exactly 13 JoinHandles (the rate-limit window trim included)"
    );

    // Cancel all tasks
    cancel.cancel();

    // Await all handles with a 10-second timeout
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        for handle in handles {
            handle.await.expect("Task should not panic");
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "All maintenance tasks should complete within 10 seconds after cancellation"
    );

    app.cleanup().await;
}

// ===========================================================================
// 7. Repo GC (git pack-maintenance) Tests
// ===========================================================================
//
// These tests drive `repo_gc::do_repo_gc_sweep` against a real Postgres
// database and real on-disk git repos, with the pools the daemon really uses:
// the runtime system role (`NOBYPASSRLS`, member of `g5_app`) for the advisory
// lock and the organization list, and the maintenance actor's scoped tenant
// transaction for each organization's `repos` rows, which forced row-level
// security hides from the system role (G5Labs/G5-Lean#1074). `app.db` is the
// table owner and bypasses RLS, so seeding goes through it, never the sweep.
// The sweep resolves each repo's path via `RepoStorage`, counts loose objects,
// and runs `git gc` on any repo at/above the threshold.

/// The real repository store over a fresh base, with each fixture repository
/// linked in at the path its row's organization keys it to
/// (`g5.repo.storage-tenancy`). A repo id absent from the map has no
/// directory, which the sweep treats as "not materialized — skip".
async fn mapped_storage(
    app: &TestApp,
    paths: HashMap<String, PathBuf>,
) -> (
    tempfile::TempDir,
    g5_api_core::services::git::repo_store::TenantRepoStorage,
) {
    let base = tempfile::tempdir().expect("storage base");
    let storage = g5_api_core::services::git::repo_store::TenantRepoStorage::local_for_test(
        base.path().to_path_buf(),
    );
    for (repo_id, path) in paths {
        let repo_id = Uuid::parse_str(&repo_id).expect("fixture repository id");
        let org_id: Uuid = sqlx::query_scalar("SELECT org_id FROM repos WHERE id = $1")
            .bind(repo_id)
            .fetch_one(&app.db)
            .await
            .expect("fixture repository row");
        let keyed = storage.test_path(org_id, repo_id);
        std::fs::create_dir_all(keyed.parent().unwrap()).expect("organization directory");
        std::os::unix::fs::symlink(&path, &keyed).expect("link fixture repository");
    }
    (base, storage)
}

/// The sweep's dependencies exactly as the daemon wires them, with the test's
/// path-mapping storage in place of the real repository store.
fn repo_gc_deps(
    app: &TestApp,
    storage: g5_api_core::services::git::repo_store::TenantRepoStorage,
) -> g5_api::maintenance::TenantMaintenanceDeps {
    g5_api::maintenance::TenantMaintenanceDeps {
        repo_storage: storage,
        ..tenant_maintenance_deps(app)
    }
}

/// Resolves a usable `git` binary for tests (mirrors `repo_gc`'s `test_git_bin`
/// and the production `which_git`).
fn repo_gc_test_git_bin() -> PathBuf {
    if let Ok(bin) = std::env::var("G5_GIT_BIN") {
        return PathBuf::from(bin);
    }
    if let Ok(out) = StdCommand::new("which").arg("git").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
    }
    PathBuf::from("/usr/bin/git")
}

/// Initializes a real non-bare git repo in `dir` with `commits` commits, each
/// adding a distinct file so every commit produces fresh loose objects. Auto-gc
/// is disabled (`gc.auto 0`, `maintenance.auto false`) so nothing packs
/// implicitly — mirroring the seeded-repo config in production and the inline
/// `init_repo_with_commits` helper.
fn init_git_repo_with_commits(git_bin: &Path, dir: &Path, commits: usize) {
    // Mirror the production git env hardening: no inherited env, fixed PATH.
    let path = "/usr/bin:/usr/local/bin";
    let run = |args: &[&str]| {
        let status = StdCommand::new(git_bin)
            .args(["-c", "safe.directory=*"])
            .arg("-C")
            .arg(dir)
            .args(args)
            .env_clear()
            .env("PATH", path)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .expect("failed to run git");
        assert!(status.success(), "git {args:?} failed");
    };

    run(&["init", "-q"]);
    run(&["config", "gc.auto", "0"]);
    run(&["config", "maintenance.auto", "false"]);

    for i in 0..commits {
        let file = dir.join(format!("file_{i}.txt"));
        std::fs::write(&file, format!("content {i}\n")).expect("write file");
        run(&["add", "."]);
        run(&["commit", "-q", "-m", &format!("commit {i}")]);
    }
}

/// Counts `*.pack` files under `<dir>/.git/objects/pack`.
fn git_packfile_count(dir: &Path) -> usize {
    let pack_dir = dir.join(".git/objects/pack");
    let Ok(entries) = std::fs::read_dir(&pack_dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "pack")
                .unwrap_or(false)
        })
        .count()
}

/// G5Labs/G5-Lean#1074: `repos` is under forced row-level security with no
/// system-access policy, so the runtime system role sees none of its rows and
/// a sweep that enumerated `repos` on the system pool packed nothing, forever.
/// The sweep must still find and repack every organization's above-threshold
/// repos when run with the real non-bypass system role, by reading `repos`
/// through the maintenance actor's scoped tenant transaction.
#[tokio::test]
async fn test_maintenance_repo_gc_packs_repos_hidden_from_the_system_role() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let mut other_user = TestUser::create(&app).await;
    let other_org = TestOrg::create_with_admin(&app, &mut other_user).await;

    let git_bin = repo_gc_test_git_bin();
    let tmp = tempfile::tempdir().expect("tempdir");

    // One above-threshold repo per organization; every organization must be
    // swept, not only the first.
    let mut paths = HashMap::new();
    for org_id in [org.id, other_org.id] {
        let repo_id = create_repo(&app.db, org_id).await;
        let repo_path = tmp.path().join(format!("repo-{repo_id}"));
        std::fs::create_dir_all(&repo_path).expect("mkdir repo");
        init_git_repo_with_commits(&git_bin, &repo_path, 8);
        assert_eq!(
            git_packfile_count(&repo_path),
            0,
            "no packfile before the sweep"
        );
        paths.insert(repo_id.to_string(), repo_path);
    }

    // The condition the sweep must survive: the runtime system role reads zero
    // `repos` rows (forced RLS, tenant policy only, no org context). Pinning it
    // here keeps this test honest if the policies ever change.
    let visible_to_system_role: i64 = sqlx::query_scalar("SELECT count(*) FROM repos")
        .fetch_one(&app.system_pool)
        .await
        .expect("count repos as the system role");
    assert_eq!(
        visible_to_system_role, 0,
        "the runtime system role must not see tenant repos directly"
    );

    let (_storage_base, storage) = mapped_storage(&app, paths.clone()).await;
    let deps = repo_gc_deps(&app, storage);
    let cancel = tokio_util::sync::CancellationToken::new();
    let stats = repo_gc::do_repo_gc_sweep(&deps, &git_bin, 1, &cancel)
        .await
        .expect("sweep as the system role");
    assert_eq!(
        stats,
        repo_gc::SweepStats {
            orgs_scanned: 2,
            repos_scanned: 2,
            repos_packed: 2,
        },
        "the sweep must enumerate and pack both organizations' repos"
    );
    for repo_path in paths.values() {
        assert!(
            git_packfile_count(repo_path) >= 1,
            "{} should have a packfile after the sweep",
            repo_path.display()
        );
    }

    app.cleanup().await;
}

/// The repo-GC sweep must skip entirely while another replica holds the
/// advisory lock, and run normally once the lock is released — proving the
/// `pg_try_advisory_lock` multi-replica guard.
#[tokio::test]
async fn test_maintenance_repo_gc_advisory_lock_blocks_concurrent_sweep() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;

    let git_bin = repo_gc_test_git_bin();
    let tmp = tempfile::tempdir().expect("tempdir");

    // One repo well above threshold so a successful sweep would pack it.
    let repo_id = create_repo(&app.db, org.id).await;
    let repo_path = tmp.path().join(format!("repo-{repo_id}"));
    std::fs::create_dir_all(&repo_path).expect("mkdir repo");
    init_git_repo_with_commits(&git_bin, &repo_path, 8);
    assert_eq!(
        git_packfile_count(&repo_path),
        0,
        "no packfile should exist before any sweep"
    );

    let mut paths = HashMap::new();
    paths.insert(repo_id.to_string(), repo_path.clone());
    let (_storage_base, storage) = mapped_storage(&app, paths).await;
    let deps = repo_gc_deps(&app, storage);

    // Hold the advisory lock on a SEPARATE session (advisory locks are
    // session-scoped). The sweep locks on its own dedicated system-role
    // session against the same database, so this blocks its
    // `pg_try_advisory_lock` for the same key.
    let mut held_conn = app
        .system_pool
        .acquire()
        .await
        .expect("acquire connection to hold the advisory lock");
    let locked: bool = sqlx::query_scalar("SELECT pg_advisory_lock($1) IS NOT NULL")
        .bind(repo_gc::ADVISORY_LOCK_KEY)
        .fetch_one(&mut *held_conn)
        .await
        .expect("hold advisory lock");
    assert!(
        locked,
        "advisory lock should be acquired by the held session"
    );

    // Sweep while the lock is held → must skip (no scan, no pack).
    let cancel = tokio_util::sync::CancellationToken::new();
    let stats = repo_gc::do_repo_gc_sweep(&deps, &git_bin, 1, &cancel)
        .await
        .expect("sweep should not error while lock is held");
    assert_eq!(
        stats,
        repo_gc::SweepStats {
            orgs_scanned: 0,
            repos_scanned: 0,
            repos_packed: 0,
        },
        "sweep must be skipped while the advisory lock is held"
    );
    assert_eq!(
        git_packfile_count(&repo_path),
        0,
        "repo must NOT be packed while the lock is held"
    );

    // Release the lock and drop the held session.
    let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(repo_gc::ADVISORY_LOCK_KEY)
        .fetch_one(&mut *held_conn)
        .await
        .expect("release advisory lock");
    assert!(unlocked, "advisory unlock should report the lock was held");
    drop(held_conn);

    // Sweep again now that the lock is free → must scan and pack the one repo.
    let stats = repo_gc::do_repo_gc_sweep(&deps, &git_bin, 1, &cancel)
        .await
        .expect("sweep should succeed once the lock is free");
    assert_eq!(
        stats,
        repo_gc::SweepStats {
            orgs_scanned: 1,
            repos_scanned: 1,
            repos_packed: 1,
        },
        "sweep must scan and pack the one repo once the lock is free"
    );
    assert!(
        git_packfile_count(&repo_path) >= 1,
        "a packfile should exist after the unblocked sweep"
    );

    app.cleanup().await;
}

/// One sweep over three repos must: enumerate all three (`repos_scanned == 3`),
/// pack the single above-threshold repo, continue past a repo whose git errors
/// (per-repo error isolation), skip the below-threshold repo, and release the
/// advisory lock when done.
#[tokio::test]
async fn test_maintenance_repo_gc_enumerates_isolates_errors_and_releases_lock() {
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;

    let git_bin = repo_gc_test_git_bin();
    let tmp = tempfile::tempdir().expect("tempdir");

    // Repo A: real repo above threshold → must be packed.
    let repo_a = create_repo(&app.db, org.id).await;
    let path_a = tmp.path().join(repo_a.to_string());
    std::fs::create_dir_all(&path_a).expect("mkdir A");
    init_git_repo_with_commits(&git_bin, &path_a, 8);
    assert_eq!(
        git_packfile_count(&path_a),
        0,
        "A: no packfile before sweep"
    );

    // Repo B: a directory whose `.git` is a FILE pointing nowhere. It passes the
    // sweep's `.git`-exists materialization check, but `git -C` errors on it —
    // exercising the per-repo error path that must NOT abort the sweep.
    let repo_b = create_repo(&app.db, org.id).await;
    let path_b = tmp.path().join(repo_b.to_string());
    std::fs::create_dir_all(&path_b).expect("mkdir B");
    std::fs::write(path_b.join(".git"), "gitdir: /nonexistent\n").expect("write bogus .git file");

    // Repo C: real repo below threshold → must be skipped (not packed).
    let repo_c = create_repo(&app.db, org.id).await;
    let path_c = tmp.path().join(repo_c.to_string());
    std::fs::create_dir_all(&path_c).expect("mkdir C");
    init_git_repo_with_commits(&git_bin, &path_c, 2);
    assert_eq!(
        git_packfile_count(&path_c),
        0,
        "C: no packfile before sweep"
    );
    let loose_c_before = count_objects_via_git(&git_bin, &path_c);

    let mut paths = HashMap::new();
    paths.insert(repo_a.to_string(), path_a.clone());
    paths.insert(repo_b.to_string(), path_b.clone());
    paths.insert(repo_c.to_string(), path_c.clone());
    let (_storage_base, storage) = mapped_storage(&app, paths).await;
    let deps = repo_gc_deps(&app, storage);

    // Threshold 10: A (8 commits → ~24 loose objects) is above; C (2 commits →
    // ~6 loose objects) is below. Each commit produces a blob + tree + commit,
    // so the exact loose count varies slightly by git version — assert C is
    // genuinely below the threshold and A is genuinely above before relying on it.
    let loose_a_before = count_objects_via_git(&git_bin, &path_a);
    assert!(
        loose_c_before < 10,
        "C should be below the threshold (loose={loose_c_before})"
    );
    assert!(
        loose_a_before >= 10,
        "A should be at/above the threshold (loose={loose_a_before})"
    );

    let cancel = tokio_util::sync::CancellationToken::new();
    let stats = repo_gc::do_repo_gc_sweep(&deps, &git_bin, 10, &cancel)
        .await
        .expect("sweep should succeed despite B's per-repo error");

    assert_eq!(stats.repos_scanned, 3, "all three repos must be enumerated");
    assert_eq!(
        stats.repos_packed, 1,
        "only the above-threshold repo (A) should be packed; B's error must not abort the sweep"
    );
    assert!(
        git_packfile_count(&path_a) >= 1,
        "A should have a packfile after the sweep"
    );
    assert_eq!(
        git_packfile_count(&path_c),
        0,
        "C (below threshold) should not be packed"
    );

    // The advisory lock must be released after the sweep returns. A fresh
    // session can therefore acquire it with `pg_try_advisory_lock`.
    let mut conn = app
        .system_pool
        .acquire()
        .await
        .expect("acquire connection to verify lock release");
    let reacquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(repo_gc::ADVISORY_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await
        .expect("try-acquire advisory lock after sweep");
    assert!(
        reacquired,
        "advisory lock must be free after the sweep (it should release it)"
    );
    // Release what we just took so we leave the session clean.
    let _: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(repo_gc::ADVISORY_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await
        .expect("release advisory lock");
    drop(conn);

    app.cleanup().await;
}

/// Completed GC survives a failed inspection in aggregate results, while a
/// refused read and an unconfirmed GC do not become confirmed completion.
#[cfg(unix)]
#[tokio::test]
async fn test_maintenance_repo_gc_counts_confirmed_completion_after_inspection_failure() {
    use std::os::unix::fs::PermissionsExt;
    use tracing::instrument::WithSubscriber;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let tmp = tempfile::tempdir().unwrap();
    let real_git = repo_gc_test_git_bin();
    let mut paths = HashMap::new();
    let mut repositories = Vec::new();
    for mode in ["inspection-fails", "read-refused", "gc-unknown"] {
        let repo_id = create_repo(&app.db, org.id).await;
        let path = tmp.path().join(mode);
        std::fs::create_dir(&path).unwrap();
        init_git_repo_with_commits(&real_git, &path, 3);
        std::fs::write(path.join(".git/test-mode"), mode).unwrap();
        paths.insert(repo_id.to_string(), path.clone());
        repositories.push(path);
    }
    let wrapper = tmp.path().join("git-with-inspection-failure");
    let quoted_git = real_git.to_string_lossy().replace('\'', "'\\''");
    std::fs::write(
        &wrapper,
        format!(
            r#"#!/bin/sh
        repository="$2"
        IFS= read -r mode < "$repository/.git/test-mode"
        case "$mode" in inspection-fails|read-refused|gc-unknown) ;; *) exit 92;; esac
        for arg in "$@"; do
            case "$arg" in
                count-objects)
                    echo count >> "$repository/.git/test-attempts"
                    if [ "$mode" = read-refused ] || [ -f "$repository/.git/test-collected" ]; then
                        echo 'injected inspection failure' >&2
                        exit 1
                    fi
                    exec '{quoted_git}' "$@";;
                gc)
                    echo gc >> "$repository/.git/test-attempts"
                    '{quoted_git}' "$@" || exit $?
                    : > "$repository/.git/test-collected"
                    if [ "$mode" = gc-unknown ]; then exit 1; fi
                    exit 0;;
            esac
        done
        exit 91
    "#
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    #[derive(Clone)]
    struct CapturedLog(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for CapturedLog {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = CapturedLog(captured.clone());
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(move || writer.clone())
        .finish();
    let (_storage_base, storage) = mapped_storage(&app, paths).await;
    let deps = repo_gc_deps(&app, storage);
    let stats = repo_gc::do_repo_gc_sweep(
        &deps,
        &wrapper,
        1,
        &tokio_util::sync::CancellationToken::new(),
    )
    .with_subscriber(subscriber)
    .await
    .unwrap();
    assert_eq!(
        stats,
        repo_gc::SweepStats {
            orgs_scanned: 1,
            repos_scanned: 3,
            repos_packed: 1
        }
    );
    let log = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("collection completed; final inspection unavailable"),
        "{log}"
    );
    assert!(log.contains("injected inspection failure"), "{log}");
    assert!(
        log.contains("inspect current state before retrying"),
        "{log}"
    );
    for (path, attempts, packed) in [
        (&repositories[0], "count\ngc\ncount\n", true),
        (&repositories[1], "count\n", false),
        (&repositories[2], "count\ngc\n", true),
    ] {
        assert_eq!(
            std::fs::read_to_string(path.join(".git/test-attempts")).unwrap(),
            attempts
        );
        assert_eq!(git_packfile_count(path) > 0, packed);
    }
    app.cleanup().await;
}

/// Shutdown cancels the active native operation and reaps its child before the
/// sweep releases its database advisory lock.
#[cfg(unix)]
#[tokio::test]
async fn test_maintenance_repo_gc_shutdown_reaps_child_before_unlock() {
    use std::os::unix::fs::PermissionsExt;
    let app = TestApp::spawn().await;
    let mut user = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut user).await;
    let repo_id = create_repo(&app.db, org.id).await;
    let tmp = tempfile::tempdir().unwrap();
    let repo_path = tmp.path().join("repo");
    std::fs::create_dir_all(repo_path.join(".git")).unwrap();
    let git_bin = tmp.path().join("fake-git");
    std::fs::write(
        &git_bin,
        r#"#!/bin/sh
        cd "$2" || exit 90
        for arg in "$@"; do
            case "$arg" in
                count-objects) echo 'count: 100'; exit 0;;
                gc) echo $$ > pid; while :; do :; done;;
            esac
        done
        exit 91
    "#,
    )
    .unwrap();
    std::fs::set_permissions(&git_bin, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (_storage_base, storage) = mapped_storage(
        &app,
        HashMap::from([(repo_id.to_string(), repo_path.clone())]),
    )
    .await;
    let deps = repo_gc_deps(&app, storage);
    let cancel = tokio_util::sync::CancellationToken::new();
    let task_cancel = cancel.clone();
    let task =
        tokio::spawn(
            async move { repo_gc::do_repo_gc_sweep(&deps, &git_bin, 1, &task_cancel).await },
        );
    let pid: u32 = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(value) = std::fs::read_to_string(repo_path.join("pid")) {
                if let Ok(pid) = value.trim().parse() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("GC must reach the active child");
    let is_alive = || {
        StdCommand::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .unwrap()
            .status
            .success()
    };
    assert!(is_alive());
    let mut conn = app.system_pool.acquire().await.unwrap();
    let available: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(repo_gc::ADVISORY_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert!(!available, "sweep must retain its lock while GC is active");
    let contender_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    let contender = tokio::spawn(async move {
        let _: bool = sqlx::query_scalar("SELECT pg_advisory_lock($1) IS NOT NULL")
            .bind(repo_gc::ADVISORY_LOCK_KEY)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let alive_at_acquisition = StdCommand::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .unwrap()
            .status
            .success();
        let _: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
            .bind(repo_gc::ADVISORY_LOCK_KEY)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        alive_at_acquisition
    });
    // Prove the second database session is actually waiting for this lock,
    // rather than merely having spawned a task that has not submitted SQL yet.
    // Poll through `app.db` so the two-connection system pool, where the
    // contender holds a slot, is never what decides whether this test waits.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid = $1 AND locktype = 'advisory' AND NOT granted)"
            ).bind(contender_pid).fetch_one(&app.db).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("contender must be waiting for the active sweep lock");
    cancel.cancel();
    let stats = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        stats,
        repo_gc::SweepStats {
            orgs_scanned: 1,
            repos_scanned: 1,
            repos_packed: 0
        }
    );
    assert!(
        !is_alive(),
        "GC child must be reaped when the sweep returns"
    );
    assert!(
        !tokio::time::timeout(Duration::from_secs(5), contender)
            .await
            .unwrap()
            .unwrap(),
        "lock contender must observe the GC child reaped when acquiring the released lock"
    );
    app.cleanup().await;
}

/// Runs `git count-objects -v` and returns the `count:` (loose object) value.
/// Used only to confirm the below-threshold seed is genuinely below threshold.
fn count_objects_via_git(git_bin: &Path, dir: &Path) -> u64 {
    let out = StdCommand::new(git_bin)
        .args(["-c", "safe.directory=*"])
        .arg("-C")
        .arg(dir)
        .args(["count-objects", "-v"])
        .env_clear()
        .env("PATH", "/usr/bin:/usr/local/bin")
        .output()
        .expect("run git count-objects");
    assert!(out.status.success(), "git count-objects failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("count:") {
            return rest.trim().parse::<u64>().expect("parse count");
        }
    }
    panic!("no count: line in git count-objects output");
}
