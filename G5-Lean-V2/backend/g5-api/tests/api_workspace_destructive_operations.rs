//! Regression coverage for #694 at the HTTP, worker, and PostgreSQL boundaries.
//! Runtime RPCs use the existing scripted guest/host; database authority and
//! service orchestration are production code.
use crate::helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use g5_api::services::workspace_service::operation_test_barrier::{self, Stage};
use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::{json, Value};
use sqlx::PgConnection;
use std::time::Duration;
use uuid::Uuid;

async fn fixture() -> (TestApp, TestOrg, TestUser, TestWorkspace) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    (app, org, owner, workspace)
}

async fn task(app: &TestApp, org: Uuid, workspace: Uuid, owner: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'pending work','command','running',$4)")
        .bind(id).bind(org).bind(workspace).bind(owner).execute(&app.db).await.unwrap();
    id
}

async fn finish_task(app: &TestApp, id: Uuid) {
    sqlx::query("UPDATE tasks SET status='succeeded' WHERE id=$1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
}

async fn wait_for_lock_held_by(app: &TestApp, blocker_pid: i32) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database() AND wait_event_type = 'Lock'
                   AND $1 = ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker_pid)
            .fetch_one(&app.db)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("competing operation waits for this transaction's lock");
}

async fn policy(app: &TestApp, owner: &TestUser, workspace: Uuid, value: Value) -> Value {
    let response = app
        .authenticated_client(owner)
        .post_json(
            &format!("/api/v1/workspaces/{workspace}/mutation-policy"),
            &value,
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{status}: {body}");
    body
}

async fn transition_id(app: &TestApp, workspace: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT mutation_policy_transition_id FROM workspaces WHERE id=$1")
        .bind(workspace)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

async fn wait_policy(app: &TestApp, workspace: Uuid, expected: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let actual: String =
                sqlx::query_scalar("SELECT mutation_policy FROM workspaces WHERE id=$1")
                    .bind(workspace)
                    .fetch_one(&app.db)
                    .await
                    .unwrap();
            if actual == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("policy worker finished");
}

async fn blocker(
    conn: &mut PgConnection,
    kind: &str,
    org: Uuid,
    owner: Uuid,
    workspace: &TestWorkspace,
) -> Result<(), sqlx::Error> {
    match kind {
        "child" => {
            sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by,cloned_from_id) VALUES($1,$2,$3,$4,$4,$5)")
                .bind(Uuid::new_v4()).bind(org).bind(format!("child-{}", Uuid::new_v4())).bind(owner).bind(workspace.id).execute(conn).await?;
        }
        "merge" => {
            sqlx::query("INSERT INTO workspace_merge_runs(id,org_id,workspace_id,source_kind,source_workspace_id,target_workspace_id,source_branch_name,status,phase,source_heads,target_heads,pre_start_source_heads,started_at) VALUES($1,$2,$3,'workspace',$3,$3,'main','running','workspace_rebase','[]','[]','[]',now())")
                .bind(Uuid::new_v4()).bind(org).bind(workspace.id).execute(conn).await?;
        }
        "fork" => {
            sqlx::query("INSERT INTO workspace_forks(id,org_id,source_workspace_id,requested_by,owner_user_id,idempotency_key,request_digest,proposed_name,repo_manifest) VALUES($1,$2,$3,$4,$4,$5,$6,$5,$7)")
                .bind(Uuid::new_v4()).bind(org).bind(workspace.id).bind(owner).bind(Uuid::new_v4().to_string()).bind("a".repeat(64)).bind(json!([{"repo_id":workspace.repo_id}])).execute(conn).await?;
        }
        _ => panic!("unknown blocker"),
    }
    Ok(())
}

#[tokio::test]
async fn refused_deletion_preserves_running_vm_and_work_including_force() {
    for kind in ["child", "merge", "fork"] {
        let (app, org, owner, workspace) = fixture().await;
        let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
        let pending = task(&app, org.id, workspace.id, owner.id).await;
        blocker(
            &mut app.db.acquire().await.unwrap(),
            kind,
            org.id,
            owner.id,
            &workspace,
        )
        .await
        .unwrap();
        for force in [false, true] {
            let response = app
                .authenticated_client(&owner)
                .post_json(
                    &format!("/api/v1/workspaces/{}/delete", workspace.id),
                    &json!({"force":force}),
                )
                .await;
            let status = response.status();
            let body = response.text().await.unwrap();
            assert_eq!(
                status,
                StatusCode::CONFLICT,
                "{kind}, force={force}: {body}"
            );
            let state: (String, Option<Uuid>, Option<Uuid>, String) = sqlx::query_as(
                "SELECT vm.status,vm.stop_operation_id,w.deletion_operation_id,t.status FROM workspaces w JOIN workspace_vms vm ON vm.workspace_id=w.id JOIN tasks t ON t.id=$2 WHERE w.id=$1",
            ).bind(workspace.id).bind(pending).fetch_one(&app.db).await.unwrap();
            assert_eq!(state, ("running".into(), None, None, "running".into()));
            assert!(peer.received_rpc_methods().await.is_empty());
            assert!(peer.received_host_command_methods().await.is_empty());
        }
        drop(peer);
        app.cleanup().await;
    }
}

#[tokio::test]
async fn eligibility_writer_winning_the_lock_refuses_delete_before_stop() {
    let (app, org, owner, workspace) = fixture().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut writer = app.db.begin().await.unwrap();
    let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    blocker(&mut writer, "fork", org.id, owner.id, &workspace)
        .await
        .unwrap();
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/delete", workspace.id);
    let request =
        tokio::spawn(async move { client.post_json(&path, &json!({"force":true})).await });
    wait_for_lock_held_by(&app, writer_pid).await;
    assert!(
        !request.is_finished(),
        "deletion waits for the eligibility writer"
    );
    writer.commit().await.unwrap();
    assert_eq!(request.await.unwrap().status(), StatusCode::CONFLICT);
    assert!(peer.received_rpc_methods().await.is_empty());
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn admitted_deletion_excludes_competing_blockers_transfer_and_restart() {
    let (app, org, owner, workspace) = fixture().await;
    let next = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let gate = operation_test_barrier::install(workspace.id, Stage::DeleteAdmitted);
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/workspaces/{}/delete", workspace.id);
    let request = tokio::spawn(async move { client.post_json(&path, &json!({})).await });
    gate.reached().await;
    for kind in ["child", "merge", "fork"] {
        let error = blocker(
            &mut app.db.acquire().await.unwrap(),
            kind,
            org.id,
            owner.id,
            &workspace,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().constraint(),
            Some("workspace_deletion_reserved"),
            "{kind}: {error}"
        );
    }
    let client = app.authenticated_client(&owner);
    let transferred = client
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({"toUserId":next.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    assert_eq!(
        transferred.status(),
        StatusCode::CONFLICT,
        "{}",
        transferred.text().await.unwrap()
    );
    let started = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", workspace.id),
            &json!({}),
        )
        .await;
    assert_eq!(
        started.status(),
        StatusCode::CONFLICT,
        "{}",
        started.text().await.unwrap()
    );
    gate.release();
    assert_eq!(request.await.unwrap().status(), StatusCode::NO_CONTENT);
    app.cleanup().await;
}

#[tokio::test]
async fn stale_completion_cannot_complete_a_new_drain() {
    let (app, org, owner, workspace) = fixture().await;
    let a_task = task(&app, org.id, workspace.id, owner.id).await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    let a = transition_id(&app, workspace.id).await;
    let a_gate = operation_test_barrier::install(a, Stage::PolicyCommit);
    finish_task(&app, a_task).await;
    a_gate.reached().await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_write"})).await;
    let b_task = task(&app, org.id, workspace.id, owner.id).await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    let b = transition_id(&app, workspace.id).await;
    assert_ne!(a, b);
    let retry = policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    assert_eq!(retry["data"]["transition"], "freezing");
    assert_eq!(transition_id(&app, workspace.id).await, b);
    // Hold B at its final transaction, so only A can attempt a completion.
    let b_gate = operation_test_barrier::install(b, Stage::PolicyCommit);
    finish_task(&app, b_task).await;
    b_gate.reached().await;
    a_gate.release();
    a_gate.finished().await;
    assert_eq!(transition_id(&app, workspace.id).await, b);
    let current: String = sqlx::query_scalar("SELECT mutation_policy FROM workspaces WHERE id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(current, "read_write");
    b_gate.release();
    wait_policy(&app, workspace.id, "read_only").await;
    app.cleanup().await;
}

#[tokio::test]
async fn stale_timeout_cannot_clear_a_new_drain() {
    let (app, org, owner, workspace) = fixture().await;
    task(&app, org.id, workspace.id, owner.id).await;
    policy(
        &app,
        &owner,
        workspace.id,
        json!({"policy":"read_only","drainTimeoutSec":1}),
    )
    .await;
    let a = transition_id(&app, workspace.id).await;
    let gate = operation_test_barrier::install(a, Stage::PolicyTimeout);
    gate.reached().await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_write"})).await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    let b = transition_id(&app, workspace.id).await;
    gate.release();
    gate.finished().await;
    assert_eq!(transition_id(&app, workspace.id).await, b);
    policy(&app, &owner, workspace.id, json!({"policy":"read_write"})).await;
    app.cleanup().await;
}

#[tokio::test]
async fn transferred_owner_cannot_commit_a_paused_policy_worker() {
    let (app, org, owner, workspace) = fixture().await;
    let next = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let work = task(&app, org.id, workspace.id, owner.id).await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    let id = transition_id(&app, workspace.id).await;
    let gate = operation_test_barrier::install(id, Stage::PolicyCommit);
    finish_task(&app, work).await;
    gate.reached().await;
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({"toUserId":next.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "{}",
        response.text().await.unwrap()
    );
    let invalidated = transition_id(&app, workspace.id).await;
    assert_ne!(
        invalidated, id,
        "transfer invalidates the old drain identity"
    );
    gate.release();
    gate.finished().await;
    assert_eq!(transition_id(&app, workspace.id).await, invalidated);
    let denied = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/mutation-policy", workspace.id),
            &json!({"policy":"read_write"}),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    // The new owner recovers the stranded operation, then can complete one.
    policy(&app, &next, workspace.id, json!({"policy":"read_write"})).await;
    let completed = policy(&app, &next, workspace.id, json!({"policy":"read_only"})).await;
    assert_eq!(completed["data"]["policy"], "read_only");
    app.cleanup().await;
}

#[tokio::test]
async fn pending_transfer_committed_while_policy_waits_prevents_completion() {
    let (app, org, owner, workspace) = fixture().await;
    let next = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let work = task(&app, org.id, workspace.id, owner.id).await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    let id = transition_id(&app, workspace.id).await;
    let gate = operation_test_barrier::install(id, Stage::PolicyCommit);
    finish_task(&app, work).await;
    gate.reached().await;
    let mut transfer = app.db.begin().await.unwrap();
    let transfer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *transfer)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *transfer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace_ownership_transfers(org_id,workspace_id,from_user_id,to_user_id,requested_by,idempotency_key,request_digest) VALUES($1,$2,$3,$4,$3,'pending-policy-race',$5)")
        .bind(org.id).bind(workspace.id).bind(owner.id).bind(next.id).bind("a".repeat(64)).execute(&mut *transfer).await.unwrap();
    gate.release();
    wait_for_lock_held_by(&app, transfer_pid).await;
    transfer.commit().await.unwrap();
    gate.finished().await;
    assert_eq!(transition_id(&app, workspace.id).await, id);
    let state: String = sqlx::query_scalar("SELECT mutation_policy FROM workspaces WHERE id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(state, "read_write");
    app.cleanup().await;
}

#[tokio::test]
async fn deactivated_owner_cannot_finish_a_policy_drain() {
    let (app, org, admin, workspace) = fixture().await;
    let owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let work = task(&app, org.id, workspace.id, owner.id).await;
    policy(&app, &owner, workspace.id, json!({"policy":"read_only"})).await;
    let id = transition_id(&app, workspace.id).await;
    let gate = operation_test_barrier::install(id, Stage::PolicyCommit);
    finish_task(&app, work).await;
    gate.reached().await;
    sqlx::query("UPDATE org_memberships SET deactivated_at=now(),deactivation_cleanup_id=$3 WHERE org_id=$1 AND user_id=$2")
        .bind(org.id).bind(owner.id).bind(Uuid::new_v4()).execute(&app.db).await.unwrap();
    gate.release();
    gate.finished().await;
    assert_eq!(transition_id(&app, workspace.id).await, id);
    let state: String = sqlx::query_scalar("SELECT mutation_policy FROM workspaces WHERE id=$1")
        .bind(workspace.id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(state, "read_write");
    drop(admin);
    app.cleanup().await;
}

#[tokio::test]
async fn deletion_resumes_a_reservation_left_before_stop() {
    let (app, _org, owner, workspace) = fixture().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let admitted = Uuid::new_v4();
    sqlx::query("UPDATE workspaces SET deletion_operation_id=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(admitted)
        .execute(&app.db)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", workspace.id),
            &json!({}),
        )
        .await;
    let status = response.status();
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        response.text().await.unwrap()
    );
    let completed: (bool, Uuid) = sqlx::query_as(
        "SELECT deleted_at IS NOT NULL,deletion_operation_id FROM workspaces WHERE id=$1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(completed, (true, admitted));
    assert!(peer
        .received_host_command_methods()
        .await
        .iter()
        .any(|method| method == "vm.stop"));
    drop(peer);
    app.cleanup().await;
}
