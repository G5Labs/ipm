//! Real PostgreSQL proof for approval-request retention.
//!
//! Scenario ownership: `test_contract/agent_approval_review` D9. The tests
//! execute the production tenant-scoped sweep; no retention predicate or
//! transaction boundary is reproduced in test code.

use crate::helpers;

use g5_api::maintenance::approval_request_retention::{
    run_approval_request_retention_sweep, ApprovalRequestRetentionConfig, RetentionDisposition,
};
use g5_api::maintenance::TenantMaintenanceDeps;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::json;
use sqlx::postgres::PgListener;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn deps(app: &TestApp) -> TenantMaintenanceDeps {
    TenantMaintenanceDeps {
        system_pool: app.system_pool.clone(),
        tenant_pool: app.state.tenant_pool.clone(),
        authz: app.state.authz.clone(),
        repo_storage: app.state.repo_storage.clone(),
    }
}

fn config(retention_days: i64, batch_size: i64) -> ApprovalRequestRetentionConfig {
    ApprovalRequestRetentionConfig {
        interval: Duration::from_secs(60),
        retention_days,
        batch_size,
    }
}

async fn create_definition(app: &TestApp, owner: &TestUser, org_id: Uuid, name: &str) -> Uuid {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/orgs/{org_id}/approval-definitions"),
            &json!({ "name": name, "humanUserIds": [owner.id] }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: serde_json::Value = response.json().await.expect("definition body");
    Uuid::parse_str(body["data"]["id"].as_str().expect("definition id")).expect("definition UUID")
}

async fn insert_terminal_request(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    definition_id: Uuid,
    actor_user_id: Uuid,
    key_byte: char,
    closed_offset: &str,
) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO approval_requests (
            org_id, source_workspace_id, approval_definition_id,
            approval_definition_version, request_key, actor_type,
            actor_user_id, state, settlement_fingerprint, requested_by,
            closed_at
        ) VALUES (
            $1, $2, $3, 1, $4, 'human', $5, 'settled', $6, $5,
            now() + $7::interval
        )
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(definition_id)
    .bind(key_byte.to_string().repeat(64))
    .bind(actor_user_id)
    .bind("0".repeat(64))
    .bind(closed_offset)
    .fetch_one(&app.db)
    .await
    .expect("insert terminal request")
}

async fn insert_pending_request(
    app: &TestApp,
    org_id: Uuid,
    workspace_id: Uuid,
    definition_id: Uuid,
    actor_user_id: Uuid,
    key_byte: char,
) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO approval_requests (
            org_id, source_workspace_id, approval_definition_id,
            approval_definition_version, request_key, actor_type,
            actor_user_id, requested_by
        ) VALUES ($1, $2, $3, 1, $4, 'human', $5, $5)
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(definition_id)
    .bind(key_byte.to_string().repeat(64))
    .bind(actor_user_id)
    .fetch_one(&app.db)
    .await
    .expect("insert pending request")
}

async fn request_exists(app: &TestApp, request_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM approval_requests WHERE id = $1)")
        .bind(request_id)
        .fetch_one(&app.db)
        .await
        .expect("request existence")
}

#[tokio::test]
async fn sweep_deletes_only_old_unreferenced_coordination_rows_and_preserves_evidence() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let definition = create_definition(&app, &owner, org.id, "Retention evidence").await;

    let old = insert_terminal_request(
        &app,
        org.id,
        workspace.id,
        definition,
        owner.id,
        'a',
        "-31 days",
    )
    .await;
    let recent = insert_terminal_request(
        &app,
        org.id,
        workspace.id,
        definition,
        owner.id,
        'b',
        "-29 days",
    )
    .await;
    let pending =
        insert_pending_request(&app, org.id, workspace.id, definition, owner.id, '9').await;
    let referenced = insert_terminal_request(
        &app,
        org.id,
        workspace.id,
        definition,
        owner.id,
        'c',
        "-31 days",
    )
    .await;
    let heads = json!([{
        "repoId": workspace.repo_id.to_string(),
        "branchName": "main",
        "workspaceId": workspace.id,
        "headCommit": workspace.head_commit,
    }]);

    let status_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO approval_statuses (
            org_id, source_workspace_id, approval_definition_id,
            approval_definition_version, outcome, comment, actor_type,
            actor_user_id, reviewed_git_heads, phase
        ) VALUES ($1, $2, $3, 1, 'approved', 'immutable retained evidence',
                  'human', $4, $5, 'ie')
        RETURNING id
        "#,
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(definition)
    .bind(owner.id)
    .bind(&heads)
    .fetch_one(&app.db)
    .await
    .expect("insert immutable status evidence");

    let task_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions(id,org_id,workspace_id,created_by,scope_type,title) VALUES ($1,$2,$3,$4,'workspace','Retained review')")
        .bind(session_id).bind(org.id).bind(workspace.id).bind(owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,chat_session_id,created_by) VALUES ($1,$2,$3,'Retained review','agent','succeeded',$4,$5)")
        .bind(task_id).bind(org.id).bind(workspace.id).bind(session_id).bind(owner.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO approval_request_tasks(org_id,approval_request_id,task_id,source_workspace_id) VALUES ($1,$2,$3,$4)")
        .bind(org.id).bind(referenced).bind(task_id).bind(workspace.id).execute(&app.db).await.unwrap();

    let report = run_approval_request_retention_sweep(
        &deps(&app),
        &config(30, 1),
        &CancellationToken::new(),
    )
    .await
    .expect("run retention sweep");
    assert_eq!(report.disposition, RetentionDisposition::Completed);
    assert_eq!(report.deleted, 1);
    assert!(!request_exists(&app, old).await);
    assert!(request_exists(&app, recent).await);
    assert!(request_exists(&app, pending).await);
    assert!(request_exists(&app, referenced).await);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM approval_statuses WHERE id = $1)"
    )
    .bind(status_id)
    .fetch_one(&app.db)
    .await
    .expect("status evidence existence"));
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM approval_definitions WHERE org_id = $1 AND id = $2)"
    )
    .bind(org.id)
    .bind(definition)
    .fetch_one(&app.db)
    .await
    .expect("definition evidence existence"));
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM workspaces WHERE org_id = $1 AND id = $2)"
    )
    .bind(org.id)
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("workspace evidence existence"));
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)")
            .bind(owner.id)
            .fetch_one(&app.db)
            .await
            .expect("user evidence existence")
    );

    app.cleanup().await;
}

#[tokio::test]
async fn one_org_delete_failure_does_not_block_another_org() {
    let app = TestApp::spawn().await;
    let mut good_owner = TestUser::create(&app).await;
    let good_org = TestOrg::create_with_admin(&app, &mut good_owner).await;
    let good_workspace = TestWorkspace::create(&app, &good_org).await;
    let good_definition =
        create_definition(&app, &good_owner, good_org.id, "Good retention org").await;
    let good_request = insert_terminal_request(
        &app,
        good_org.id,
        good_workspace.id,
        good_definition,
        good_owner.id,
        'd',
        "-31 days",
    )
    .await;

    let mut bad_owner = TestUser::create(&app).await;
    let bad_org = TestOrg::create_with_admin(&app, &mut bad_owner).await;
    let bad_workspace = TestWorkspace::create(&app, &bad_org).await;
    let bad_definition =
        create_definition(&app, &bad_owner, bad_org.id, "Failing retention org").await;
    let bad_request = insert_terminal_request(
        &app,
        bad_org.id,
        bad_workspace.id,
        bad_definition,
        bad_owner.id,
        'e',
        "-31 days",
    )
    .await;

    sqlx::raw_sql(&format!(
        r#"
        CREATE FUNCTION reject_retention_for_one_org() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            IF OLD.org_id = '{bad_org}'::uuid THEN
                RAISE EXCEPTION 'injected tenant-local retention failure';
            END IF;
            RETURN OLD;
        END;
        $$;
        CREATE TRIGGER reject_retention_for_one_org
        BEFORE DELETE ON approval_requests
        FOR EACH ROW EXECUTE FUNCTION reject_retention_for_one_org();
        "#,
        bad_org = bad_org.id
    ))
    .execute(&app.db)
    .await
    .expect("install tenant-local failure injection");

    let report = run_approval_request_retention_sweep(
        &deps(&app),
        &config(30, 10),
        &CancellationToken::new(),
    )
    .await
    .expect("sweep continues after per-org failure");
    assert_eq!(report.orgs_completed, 1);
    assert_eq!(report.orgs_failed, 1);
    assert_eq!(report.deleted, 1);
    assert_eq!(report.disposition, RetentionDisposition::Failed);
    assert!(!request_exists(&app, good_request).await);
    assert!(request_exists(&app, bad_request).await);

    app.cleanup().await;
}

#[tokio::test]
async fn cancellation_is_observed_after_a_committed_batch() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let definition = create_definition(&app, &owner, org.id, "Cancelled retention").await;
    for key in ['f', '1', '2'] {
        insert_terminal_request(
            &app,
            org.id,
            workspace.id,
            definition,
            owner.id,
            key,
            "-31 days",
        )
        .await;
    }

    // Hold the second candidate so notification delivery can race only with
    // the next between-batch check, never with completion of the whole sweep.
    // If batch two has already started it waits here; cancelling and releasing
    // the row then permits at most that in-flight batch to commit.
    let second_request: Uuid = sqlx::query_scalar(
        r#"
        SELECT id
        FROM approval_requests
        WHERE org_id = $1 AND source_workspace_id = $2
        ORDER BY closed_at, seq, id
        OFFSET 1 LIMIT 1
        "#,
    )
    .bind(org.id)
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("select second retention candidate");
    let mut second_batch_gate = app.db.begin().await.expect("begin second-batch gate");
    sqlx::query("SELECT id FROM approval_requests WHERE id = $1 FOR UPDATE")
        .bind(second_request)
        .fetch_one(&mut *second_batch_gate)
        .await
        .expect("lock second retention candidate");

    sqlx::raw_sql(
        r#"
        CREATE FUNCTION notify_retention_commit() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            PERFORM pg_notify('approval_retention_batch_committed', OLD.id::text);
            RETURN OLD;
        END;
        $$;
        CREATE TRIGGER notify_retention_commit
        AFTER DELETE ON approval_requests
        FOR EACH ROW EXECUTE FUNCTION notify_retention_commit();
        "#,
    )
    .execute(&app.db)
    .await
    .expect("install committed-batch notification");

    let mut listener = PgListener::connect_with(&app.db)
        .await
        .expect("connect retention listener");
    listener
        .listen("approval_retention_batch_committed")
        .await
        .expect("listen for committed batch");
    let cancel = CancellationToken::new();
    let cancel_on_commit = cancel.clone();
    let waiter = tokio::spawn(async move {
        listener.recv().await.expect("committed batch notification");
        cancel_on_commit.cancel();
        second_batch_gate
            .rollback()
            .await
            .expect("release second-batch gate");
    });

    let report = run_approval_request_retention_sweep(&deps(&app), &config(30, 1), &cancel)
        .await
        .expect("cancelled retention sweep");
    waiter.await.expect("notification waiter");
    assert!(
        (1..3).contains(&report.deleted),
        "unexpected deleted count: {}",
        report.deleted
    );
    assert_eq!(report.orgs_cancelled, 1);
    assert_eq!(report.disposition, RetentionDisposition::Cancelled);
    let retained: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approval_requests WHERE org_id = $1 AND source_workspace_id = $2",
    )
    .bind(org.id)
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .expect("count retained requests");
    assert_eq!(retained, 3 - report.deleted as i64);

    app.cleanup().await;
}

#[tokio::test]
async fn every_delete_runs_in_the_exact_request_tenant_scope() {
    let app = TestApp::spawn().await;
    let mut first_owner = TestUser::create(&app).await;
    let first_org = TestOrg::create_with_admin(&app, &mut first_owner).await;
    let first_workspace = TestWorkspace::create(&app, &first_org).await;
    let first_definition =
        create_definition(&app, &first_owner, first_org.id, "First scoped org").await;
    insert_terminal_request(
        &app,
        first_org.id,
        first_workspace.id,
        first_definition,
        first_owner.id,
        '3',
        "-31 days",
    )
    .await;

    let mut second_owner = TestUser::create(&app).await;
    let second_org = TestOrg::create_with_admin(&app, &mut second_owner).await;
    let second_workspace = TestWorkspace::create(&app, &second_org).await;
    let second_definition =
        create_definition(&app, &second_owner, second_org.id, "Second scoped org").await;
    insert_terminal_request(
        &app,
        second_org.id,
        second_workspace.id,
        second_definition,
        second_owner.id,
        '4',
        "-31 days",
    )
    .await;

    sqlx::raw_sql(
        r#"
        CREATE FUNCTION enforce_exact_retention_tenant_scope() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            IF cardinality(authz.current_org_ids()) <> 1
               OR NOT OLD.org_id = ANY(authz.current_org_ids()) THEN
                RAISE EXCEPTION 'retention delete escaped exact tenant scope';
            END IF;
            RETURN OLD;
        END;
        $$;
        CREATE TRIGGER enforce_exact_retention_tenant_scope
        BEFORE DELETE ON approval_requests
        FOR EACH ROW EXECUTE FUNCTION enforce_exact_retention_tenant_scope();
        "#,
    )
    .execute(&app.db)
    .await
    .expect("install exact-tenant assertion");

    let report = run_approval_request_retention_sweep(
        &deps(&app),
        &config(30, 10),
        &CancellationToken::new(),
    )
    .await
    .expect("run exactly tenant-scoped retention sweep");
    assert_eq!(report.orgs_completed, 2);
    assert_eq!(report.orgs_failed, 0);
    assert_eq!(report.deleted, 2);

    app.cleanup().await;
}
