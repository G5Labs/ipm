//! Issue #694: eligibility before runtime effects and exact drain authority.
use crate::helpers::{TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use g5_api::services::workspace_service::{
    MutationPolicyTransitionCommand, TargetMutationPolicy, TransitionMode,
};
use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

async fn fixture() -> (TestApp, TestOrg, TestUser, TestWorkspace) {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    (app, org, owner, workspace)
}

fn command(
    org: &TestOrg,
    owner: &TestUser,
    workspace: &TestWorkspace,
) -> MutationPolicyTransitionCommand {
    MutationPolicyTransitionCommand {
        org_id: org.id,
        workspace_id: workspace.id,
        actor: owner.id,
        target: TargetMutationPolicy::ReadOnly,
        mode: TransitionMode::Drain,
        drain_timeout_secs: 60,
    }
}

async fn policy(app: &TestApp, workspace: Uuid) -> (String, Option<String>, Option<Uuid>) {
    sqlx::query_as("SELECT mutation_policy,mutation_policy_transition,mutation_policy_transition_id FROM workspaces WHERE id=$1")
        .bind(workspace).fetch_one(&app.db).await.unwrap()
}

async fn seed_blocker(
    app: &TestApp,
    org: &TestOrg,
    owner: &TestUser,
    workspace: &TestWorkspace,
    kind: &str,
) {
    match kind {
        "child" => {
            let child = TestWorkspace::create(app, org).await;
            sqlx::query("UPDATE workspaces SET cloned_from_id=$2 WHERE id=$1")
                .bind(child.id)
                .bind(workspace.id)
                .execute(&app.db)
                .await
                .unwrap();
        }
        "merge" => {
            let target = TestWorkspace::create(app, org).await;
            sqlx::query("INSERT INTO workspace_merge_runs(id,org_id,workspace_id,source_kind,source_workspace_id,target_workspace_id,source_branch_name,status,phase,source_heads,target_heads,pre_start_source_heads,started_at) VALUES($1,$2,$3,'workspace',$3,$4,'main','running','workspace_rebase','[]','[]','[]',now())")
                .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(target.id).execute(&app.db).await.unwrap();
        }
        "fork" => {
            let repo: Uuid = sqlx::query_scalar(
                "SELECT repo_id FROM workspace_repos WHERE workspace_id=$1 LIMIT 1",
            )
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            sqlx::query("INSERT INTO workspace_forks(id,org_id,source_workspace_id,requested_by,owner_user_id,idempotency_key,request_digest,proposed_name,repo_manifest) VALUES($1,$2,$3,$4,$4,$5,$6,$5,$7)")
                .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(owner.id)
                .bind(format!("pending-{}", Uuid::new_v4())).bind("a".repeat(64))
                .bind(json!([{"repo_id":repo}])).execute(&app.db).await.unwrap();
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn refused_deletions_preserve_running_vm_and_pending_work_even_when_forced() {
    let (app, org, owner, _) = fixture().await;
    let client = app.authenticated_client(&owner);
    for blocker in ["child", "merge", "fork"] {
        let workspace = TestWorkspace::create(&app, &org).await;
        let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
        let task = Uuid::new_v4();
        sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'pending work','command','queued',$4)")
            .bind(task).bind(org.id).bind(workspace.id).bind(owner.id).execute(&app.db).await.unwrap();
        seed_blocker(&app, &org, &owner, &workspace, blocker).await;
        let vm_before: Value =
            sqlx::query_scalar("SELECT jsonb_build_object('status',status,'stopOperation',stop_operation_id,'stopReason',stop_reason,'pendingReason',pending_stop_reason,'loss',unflushed_loss_at) FROM workspace_vms WHERE workspace_id=$1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .unwrap();
        for force in [false, true] {
            let response = client
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
                "{blocker}, force={force}: {body}"
            );
            assert!(
                body.contains(match blocker {
                    "child" => "child workspaces",
                    "merge" => "active merge",
                    _ => "unfinished fork",
                }),
                "{body}"
            );
            let vm_after: Value = sqlx::query_scalar(
                "SELECT jsonb_build_object('status',status,'stopOperation',stop_operation_id,'stopReason',stop_reason,'pendingReason',pending_stop_reason,'loss',unflushed_loss_at) FROM workspace_vms WHERE workspace_id=$1",
            )
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
            assert_eq!(vm_before, vm_after, "refusal must not even admit Stop");
            let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id=$1")
                .bind(task)
                .fetch_one(&app.db)
                .await
                .unwrap();
            assert_eq!(status, "queued");
            let untouched: bool = sqlx::query_scalar("SELECT deleted_at IS NULL AND deletion_operation_id IS NULL FROM workspaces WHERE id=$1")
                .bind(workspace.id).fetch_one(&app.db).await.unwrap();
            assert!(untouched);
            assert!(peer.received_rpc_methods().await.is_empty());
            assert!(peer.received_host_command_methods().await.is_empty());
        }
    }
    app.cleanup().await;
}

#[tokio::test]
async fn deletion_reservation_excludes_fork_transfer_and_policy_during_stop() {
    let (app, org, owner, workspace) = fixture().await;
    let recipient = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let peer = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
        &app,
        &workspace,
        owner.id,
        "admission.close",
    )
    .await;
    let client = app.authenticated_client(&owner);
    let delete_path = format!("/api/v1/workspaces/{}/delete", workspace.id);
    let delete_body = json!({});
    let deletion = client.post_json(&delete_path, &delete_body);
    tokio::pin!(deletion);
    tokio::select! {
        result = &mut deletion => panic!("delete completed before the runtime barrier: {}",result.status()),
        barrier = tokio::time::timeout(Duration::from_secs(10),async {
            loop {
                if peer.received_rpc_methods().await.iter().any(|m|m=="admission.close") { break; }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }) => barrier.expect("delete reaches runtime admission.close"),
    }
    let reserved: Option<Uuid> =
        sqlx::query_scalar("SELECT deletion_operation_id FROM workspaces WHERE id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(reserved.is_some());
    for (action, body) in [
        (
            "clone",
            json!({"name":format!("late-child-{}",Uuid::new_v4()),"source":"branch","idempotencyKey":Uuid::new_v4().to_string()}),
        ),
        (
            "transfer",
            json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        ),
        ("mutation-policy", json!({"policy":"read_only"})),
    ] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/{action}", workspace.id),
                &body,
            )
            .await;
        let status = response.status();
        let body = response.text().await.unwrap();
        assert_eq!(status, StatusCode::CONFLICT, "{action}: {body}");
        assert!(body.contains("deletion is in progress"), "{action}: {body}");
    }
    // Background divergence has a different admission path from user merges.
    let mut background = app.scoped_db_for_org(org.id).await;
    let merge = g5_api::services::remote_integrations::open_remote_tracking_merge_run_for_tests(
        background.conn(),
        workspace.id,
        workspace.repo_id,
        "main",
        g5_api::services::branch_integration_service::DivergedPhase::WorkspaceRebasePreflight,
        &workspace.head_commit,
        Some(&workspace.head_commit),
        &workspace.head_commit,
    )
    .await;
    let error = merge.expect_err("background merge admission must retain deletion eligibility");
    assert!(
        error.to_string().contains("deletion is in progress"),
        "{error}"
    );
    background.rollback().await.unwrap();
    peer.release_rpc_barrier();
    let response = tokio::time::timeout(Duration::from_secs(15), deletion)
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "{}",
        response.text().await.unwrap()
    );
    let tombstone: Option<Uuid> = sqlx::query_scalar(
        "SELECT deletion_operation_id FROM workspaces WHERE id=$1 AND deleted_at IS NOT NULL",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(tombstone, reserved);
    app.cleanup().await;
}

#[tokio::test]
async fn old_drain_cannot_complete_or_expire_replacement_after_restart_recovery() {
    let (app, org, owner, workspace) = fixture().await;
    let command = command(&org, &owner, &workspace);
    let service = &app.state.workspace_service;
    let first = Uuid::new_v4();
    assert!(service
        .admit_policy_drain_for_tests(command, first)
        .await
        .unwrap());
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/mutation-policy", workspace.id),
            &json!({"policy":"read_write"}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        policy(&app, workspace.id).await,
        ("read_write".into(), None, None)
    );
    let second = Uuid::new_v4();
    assert!(service
        .admit_policy_drain_for_tests(command, second)
        .await
        .unwrap());
    for _ in 0..2 {
        service
            .settle_policy_drain_for_tests(&app.state.event_bus, command, first, false)
            .await
            .unwrap();
        service
            .settle_policy_drain_for_tests(&app.state.event_bus, command, first, true)
            .await
            .unwrap();
        assert_eq!(
            policy(&app, workspace.id).await,
            ("read_write".into(), Some("freezing".into()), Some(second))
        );
    }
    service
        .settle_policy_drain_for_tests(&app.state.event_bus, command, second, false)
        .await
        .unwrap();
    assert_eq!(
        policy(&app, workspace.id).await,
        ("read_only".into(), None, None)
    );
    app.cleanup().await;
}

#[tokio::test]
async fn transferred_owner_cannot_settle_a_policy_drain_and_new_owner_can_recover() {
    let (app, org, owner, workspace) = fixture().await;
    let recipient = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let command = command(&org, &owner, &workspace);
    let identity = Uuid::new_v4();
    app.state
        .workspace_service
        .admit_policy_drain_for_tests(command, identity)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "completed");
    let transferred = policy(&app, workspace.id).await;
    assert_ne!(transferred.2, Some(identity));
    for timed_out in [false, true] {
        assert!(app
            .state
            .workspace_service
            .settle_policy_drain_for_tests(&app.state.event_bus, command, identity, timed_out)
            .await
            .is_err());
        assert_eq!(policy(&app, workspace.id).await, transferred);
    }
    // Current ownership alone cannot identify the worker: ownership can return
    // to its original actor before that actor's old drain gets another turn.
    let response = app
        .authenticated_client(&recipient)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({"toUserId":owner.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{}", response.text().await.unwrap());
    let returned = policy(&app, workspace.id).await;
    assert_ne!(returned.2, Some(identity));
    for timed_out in [false, true] {
        app.state
            .workspace_service
            .settle_policy_drain_for_tests(&app.state.event_bus, command, identity, timed_out)
            .await
            .unwrap();
        assert_eq!(policy(&app, workspace.id).await, returned);
    }
    let response = app
        .authenticated_client(&owner)
        .post_json(
            &format!("/api/v1/workspaces/{}/transfer", workspace.id),
            &json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{}", response.text().await.unwrap());
    let client = app.authenticated_client(&recipient);
    for target in ["read_write", "read_only"] {
        let response = client
            .post_json(
                &format!("/api/v1/workspaces/{}/mutation-policy", workspace.id),
                &json!({"policy":target}),
            )
            .await;
        let status = response.status();
        assert_eq!(status, StatusCode::OK, "{}", response.text().await.unwrap());
        assert_eq!(
            policy(&app, workspace.id).await,
            (target.into(), None, None)
        );
    }
    app.cleanup().await;
}

#[tokio::test]
async fn pending_transfer_committed_while_policy_waits_for_lock_fences_completion() {
    let (app, org, owner, workspace) = fixture().await;
    let recipient = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let command = command(&org, &owner, &workspace);
    let identity = Uuid::new_v4();
    app.state
        .workspace_service
        .admit_policy_drain_for_tests(command, identity)
        .await
        .unwrap();
    let mut transfer = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
        .bind(workspace.id)
        .execute(&mut *transfer)
        .await
        .unwrap();
    let service = app.state.clone();
    let completion = tokio::spawn(async move {
        service
            .workspace_service
            .settle_policy_drain_for_tests(&service.event_bus, command, identity, false)
            .await
    });
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT is_base, agent_approval_review_id FROM workspaces%')")
                .fetch_one(&app.db).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    sqlx::query("INSERT INTO workspace_ownership_transfers(id,org_id,workspace_id,from_user_id,to_user_id,requested_by,idempotency_key,request_digest) VALUES($1,$2,$3,$4,$5,$4,$6,$7)")
        .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(owner.id).bind(recipient.id)
        .bind(Uuid::new_v4().to_string()).bind("a".repeat(64)).execute(&mut *transfer).await.unwrap();
    transfer.commit().await.unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(10), completion)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(
        policy(&app, workspace.id).await,
        ("read_write".into(), Some("freezing".into()), Some(identity))
    );
    app.cleanup().await;
}

#[tokio::test]
async fn deletion_rechecks_children_admitted_while_waiting_for_repository_lock() {
    let (app, org, owner, workspace) = fixture().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, owner.id).await;
    let mut admission = app.db.begin().await.unwrap();
    sqlx::query("SELECT id FROM repos WHERE id=$1 FOR UPDATE")
        .bind(workspace.repo_id)
        .execute(&mut *admission)
        .await
        .unwrap();
    let client = app.authenticated_client(&owner);
    let workspace_id = workspace.id;
    let deletion = tokio::spawn(async move {
        client
            .post_json(
                &format!("/api/v1/workspaces/{workspace_id}/delete"),
                &json!({"force":true}),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT id FROM repos%')")
                .fetch_one(&app.db).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    seed_blocker(&app, &org, &owner, &workspace, "child").await;
    admission.commit().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(10), deletion)
        .await
        .unwrap()
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("child workspaces"));
    let status: String =
        sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "running");
    assert!(peer.received_rpc_methods().await.is_empty());
    assert!(peer.received_host_command_methods().await.is_empty());
    app.cleanup().await;
}

#[tokio::test]
async fn membership_revocation_fences_policy_completion_and_timeout() {
    let (app, org, _org_owner, workspace) = fixture().await;
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
    let command = command(&org, &owner, &workspace);
    let identity = Uuid::new_v4();
    app.state
        .workspace_service
        .admit_policy_drain_for_tests(command, identity)
        .await
        .unwrap();
    sqlx::query("UPDATE org_memberships SET deactivated_at=now() WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    for timed_out in [false, true] {
        assert!(app
            .state
            .workspace_service
            .settle_policy_drain_for_tests(&app.state.event_bus, command, identity, timed_out)
            .await
            .is_err());
        assert_eq!(
            policy(&app, workspace.id).await,
            ("read_write".into(), Some("freezing".into()), Some(identity))
        );
    }
    app.cleanup().await;
}
