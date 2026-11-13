//! Membership revocation through real HTTP, PostgreSQL authority and lifecycle.
use crate::helpers::{self, TestApp, TestOrg, TestUser, TestWorkspace, TestWorkspaceRuntimePeer};
use g5_api::services::{WorkspaceVmService, WorkspaceVmServiceConfig};
use g5_auth::Scope;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

async fn setup() -> (TestApp, TestOrg, TestUser, TestUser, TestWorkspace) {
    let app = TestApp::spawn().await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(workspace.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    (app, org, admin, member, workspace)
}

async fn activity(
    app: &TestApp,
    actor: &TestUser,
    org: Uuid,
    user: Uuid,
    active: bool,
    revision: i64,
) -> (StatusCode, Value) {
    let response = app
        .authenticated_client(actor)
        .post_json(
            &format!(
                "/api/v1/orgs/{org}/members/{user}/{}",
                if active { "reactivate" } else { "deactivate" }
            ),
            &json!({"activityRevision": revision}),
        )
        .await;
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

async fn service(app: &TestApp, org: Uuid) -> WorkspaceVmService {
    let db = app.scoped_db_for_org(org).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    WorkspaceVmService::new(WorkspaceVmServiceConfig {
        system_pool: app.system_pool.clone(),
        scope,
        hosts: app.state.preview_tunnels.clone(),
        minter: app.state.jwt_minter.clone().unwrap(),
        runtime: app.state.workspace_runtime.clone(),
        agent_work: app.state.agent_work.clone(),
        repo_storage: app.repo_storage.clone(),
        git_external_url: app.state.git_external_url.clone(),
        ws_hub: app.state.event_bus.ws_hub().clone(),
    })
}

#[tokio::test]
async fn retained_membership_revokes_old_jwt_key_and_database_scope_only_in_one_org() {
    let (app, org, admin, member, workspace) = setup().await;
    let other = TestOrg::create(&app).await;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES ($1,$2,'member')")
        .bind(other.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    let other_ws = TestWorkspace::create(&app, &other).await;
    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    let old_scope = db.reopener();
    let key = app
        .state
        .api_key_service
        .create_key(
            &mut db,
            org.id,
            member.id,
            g5_api::services::api_key_service::CreateApiKeyRequest {
                name: "existing CLI key".into(),
                scopes: vec!["workspaces:read".into()],
                expires_in_secs: None,
            },
        )
        .await
        .unwrap();
    db.commit().await.unwrap();
    // Keep an already-open scope too: RLS must check at each statement.
    let mut open = old_scope.open().await.unwrap();
    open.fork().await.unwrap().rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspaces WHERE id=$1")
            .bind(workspace.id)
            .fetch_one(open.conn())
            .await
            .unwrap(),
        1
    );
    let old_client = app.authenticated_client(&member);
    let (status, body) = activity(&app, &admin, org.id, member.id, false, 0).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["deactivatedAt"].is_string());
    assert_eq!(body["data"]["activityRevision"], 1);
    assert_eq!(body["data"]["shutdownPending"], true);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspaces WHERE id=$1")
            .bind(workspace.id)
            .fetch_one(open.conn())
            .await
            .unwrap(),
        0
    );
    assert!(
        open.fork().await.is_err(),
        "fork must recheck membership too"
    );
    open.rollback().await.unwrap();
    assert!(old_scope.open().await.is_err());
    assert_eq!(
        old_client
            .get(&format!("/api/v1/workspaces/{}", workspace.id))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        old_client
            .get(&format!("/api/v1/workspaces/{}", other_ws.id))
            .await
            .status(),
        StatusCode::OK
    );
    assert!(
        g5_api::services::api_key_service::ApiKeyService::validate_api_key(
            &app.system_pool,
            &key.key,
            None
        )
        .await
        .is_err()
    );
    let retained: (String, Uuid) = sqlx::query_as(
        "SELECT m.role, w.owner_user_id FROM org_memberships m JOIN workspaces w ON w.org_id=m.org_id
         WHERE m.org_id=$1 AND m.user_id=$2 AND w.id=$3")
        .bind(org.id).bind(member.id).bind(workspace.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(retained, ("member".into(), member.id));
    let changed = app
        .authenticated_client(&admin)
        .patch_json(
            &format!("/api/v1/orgs/{}/members/{}", org.id, member.id),
            &json!({"role":"admin"}),
        )
        .await;
    assert_eq!(changed.status(), StatusCode::OK);
    let changed: Value = changed.json().await.unwrap();
    assert!(
        changed["data"]["deactivatedAt"].is_string(),
        "a role edit cannot reactivate"
    );
    let invitation = Uuid::new_v4();
    sqlx::query("INSERT INTO org_invitations(id,org_id,email,role,invited_by,status,expires_at) VALUES($1,$2,$3,'member',$4,'pending',now()+interval '1 day')")
        .bind(invitation).bind(org.id).bind(&member.email).bind(admin.id).execute(&app.db).await.unwrap();
    let accepted = old_client
        .post_json(
            &format!("/api/v1/invitations/{invitation}/accept"),
            &json!({}),
        )
        .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    let accepted: Value = accepted.json().await.unwrap();
    assert!(accepted["data"]["deactivatedAt"].is_string());
    assert_eq!(
        accepted["data"]["role"], "admin",
        "invitation preserves the retained role"
    );
    // Repeat is idempotent; a stale opposing request cannot undo the decision.
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 0).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    assert!(
        g5_api::services::api_key_service::ApiKeyService::validate_api_key(
            &app.system_pool,
            &key.key,
            None
        )
        .await
        .is_ok()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn only_active_admins_can_change_other_members() {
    let (app, org, owner, member, _) = setup().await;
    let admin = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(admin.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, owner.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, owner.id, true, 1).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, admin.id, false, 0).await.0,
        StatusCode::CONFLICT
    );
    assert!(!activity(&app, &member, org.id, admin.id, false, 0)
        .await
        .0
        .is_success());
    for role in ["admin", "member", "viewer"] {
        let target = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
        sqlx::query("UPDATE org_memberships SET role=$3 WHERE org_id=$1 AND user_id=$2")
            .bind(org.id)
            .bind(target.id)
            .bind(role)
            .execute(&app.db)
            .await
            .unwrap();
        let (status, body) = activity(&app, &admin, org.id, target.id, false, 0).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["role"], role);
        assert!(!activity(&app, &target, org.id, member.id, false, 0)
            .await
            .0
            .is_success());
    }
    // Additive grants do not turn a viewer into an administrator for this action.
    sqlx::query("UPDATE org_memberships SET role='viewer' WHERE org_id=$1 AND user_id=$2")
        .bind(org.id)
        .bind(admin.id)
        .execute(&app.db)
        .await
        .unwrap();
    admin.grant_org_scope(&app, org.id, Scope::OrgAdmin).await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::FORBIDDEN
    );
    app.cleanup().await;
}

#[tokio::test]
async fn early_reactivation_does_not_restore_runtime_or_old_stream_authority() {
    let (app, org, admin, member, workspace) = setup().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, member.id).await;
    let handle = app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, member.id)
        .await
        .unwrap();
    let (_, stream, guard) = app
        .state
        .workspace_runtime
        .open_owner_stream(
            workspace.id,
            member.id,
            g5_internal_api::stream_mux::Channel::Pty,
        )
        .await
        .unwrap();
    assert!(guard.authorize().await);
    let worker = service(&app, org.id).await;
    crate::api_task_lifecycle::insert_queued_preview(&app, &workspace, member.id).await;
    let mut reservation = app.scoped_db_for_org(org.id).await;
    assert!(
        app.state
            .task_service
            .reserve_command_dispatch(&mut reservation)
            .await
            .unwrap()
            .is_some(),
        "queued work is dispatchable on the active running workspace"
    );
    reservation.rollback().await.unwrap();

    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    let mut reservation = app.scoped_db_for_org(org.id).await;
    assert!(
        app.state
            .task_service
            .reserve_command_dispatch(&mut reservation)
            .await
            .unwrap()
            .is_none(),
        "deactivation blocks queued dispatch before shutdown admission"
    );
    reservation.rollback().await.unwrap();
    assert!(!guard.authorize().await);
    assert!(app
        .state
        .workspace_runtime
        .owner_handle(workspace.id, member.id)
        .await
        .is_err());
    let request = g5_api::services::workspace_runtime::RuntimeRequest::owner(
        &handle,
        member.id,
        (chrono::Utc::now().timestamp_millis() + 1000) as u64,
        "old-handle",
    );
    assert!(app
        .state
        .workspace_runtime
        .modules_invoke(&handle, &request, json!({}))
        .await
        .is_err());
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    let mut reservation = app.scoped_db_for_org(org.id).await;
    assert!(
        app.state
            .task_service
            .reserve_command_dispatch(&mut reservation)
            .await
            .unwrap()
            .is_none(),
        "early reactivation still blocks queued dispatch"
    );
    reservation.rollback().await.unwrap();
    assert!(!guard.authorize().await, "an old revision never revives");
    assert!(worker
        .resume(
            workspace.id,
            member.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StartKind::Resume
        )
        .await
        .is_err());
    assert_eq!(worker.reconcile_member_deactivation(16).await.unwrap(), 1);
    assert_eq!(worker.reconcile_stopping(16).await.unwrap(), 1);
    worker.reconcile_member_deactivation(16).await.unwrap();
    let state: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT vm.status, m.deactivation_cleanup_id FROM workspace_vms vm
         JOIN org_memberships m ON m.org_id=vm.org_id WHERE vm.workspace_id=$1 AND m.user_id=$2",
    )
    .bind(workspace.id)
    .bind(member.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(state, ("stopped".into(), None));
    worker.reconcile_start_queue(16).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM workspace_vms WHERE workspace_id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        "stopped"
    );
    drop(stream);
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn unreachable_host_and_expired_lease_keep_cleanup_pending() {
    let (app, org, admin, member, workspace) = setup().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, member.id).await;
    let worker = service(&app, org.id).await;
    app.state.preview_tunnels.terminate(&peer.host_id()).await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    worker.reconcile_member_deactivation(16).await.unwrap();
    sqlx::query(
        "UPDATE workspace_vms SET lease_expires_at=now()-interval '1 second' WHERE workspace_id=$1",
    )
    .bind(workspace.id)
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(worker.reconcile_expired_leases().await.unwrap(), 0);
    worker.reconcile_stopping(16).await.unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    worker.reconcile_member_deactivation(16).await.unwrap();
    let state: (String, Option<String>, bool) = sqlx::query_as(
        "SELECT vm.status,vm.vm_id,m.deactivation_cleanup_id IS NOT NULL FROM workspace_vms vm
         JOIN org_memberships m ON m.org_id=vm.org_id WHERE vm.workspace_id=$1 AND m.user_id=$2",
    )
    .bind(workspace.id)
    .bind(member.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        state,
        ("stopping".into(), Some(peer.vm_id().to_string()), true)
    );
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn stopped_capacity_queue_and_more_than_one_cleanup_page_are_held() {
    let (app, org, admin, member, first) = setup().await;
    let mut workspaces = vec![first];
    for _ in 0..17 {
        let workspace = TestWorkspace::create(&app, &org).await;
        sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
            .bind(workspace.id)
            .bind(member.id)
            .execute(&app.db)
            .await
            .unwrap();
        workspaces.push(workspace);
    }
    for workspace in &workspaces {
        let now = chrono::Utc::now();
        let intent = json!({"id":Uuid::new_v4(),"kind":"resume","owner_user_id":member.id,
            "lifecycle_generation":0,"status":"queued_for_capacity","created_at":now,
            "expires_at":now+chrono::Duration::minutes(15),"attempts":0});
        sqlx::query("INSERT INTO workspace_vms(org_id,workspace_id,status,start_intent) VALUES ($1,$2,'stopped',$3)")
            .bind(org.id).bind(workspace.id).bind(intent).execute(&app.db).await.unwrap();
        sqlx::query(
            "INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by)
                     VALUES($1,$2,$3,'queued work','command','queued',$4)",
        )
        .bind(Uuid::new_v4())
        .bind(org.id)
        .bind(workspace.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    }
    // A confirmed exit can leave old Start consent on a failed VM.
    sqlx::query("UPDATE workspace_vms SET status='failed',start_intent=jsonb_set(start_intent,'{status}','\"starting\"') WHERE workspace_id=$1")
        .bind(workspaces[0].id).execute(&app.db).await.unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    let worker = service(&app, org.id).await;
    assert_eq!(worker.reconcile_start_queue(16).await.unwrap(), 0);
    assert_eq!(worker.reconcile_member_deactivation(16).await.unwrap(), 16);
    assert_eq!(worker.reconcile_member_deactivation(16).await.unwrap(), 2);
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM tasks WHERE org_id=$1 AND status='queued'")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
    let pending: bool = sqlx::query_scalar("SELECT deactivation_cleanup_id IS NOT NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(org.id).bind(member.id).fetch_one(&app.db).await.unwrap();
    assert!(!pending);
    app.cleanup().await;
}

#[tokio::test]
async fn admin_can_transfer_inactive_owners_workspace_only_to_active_recipient() {
    let (app, org, admin, member, workspace) = setup().await;
    let recipient = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, recipient.id, false, 0)
            .await
            .0,
        StatusCode::OK
    );
    let client = app.authenticated_client(&admin);
    let url = format!("/api/v1/workspaces/{}/transfer", workspace.id);
    assert!(!client
        .post_json(
            &url,
            &json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()})
        )
        .await
        .status()
        .is_success());
    assert_eq!(
        activity(&app, &admin, org.id, recipient.id, true, 1)
            .await
            .0,
        StatusCode::OK
    );
    let response = client
        .post_json(
            &url,
            &json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "completed");
    service(&app, org.id)
        .await
        .reconcile_member_deactivation(16)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT owner_user_id FROM workspaces WHERE id=$1")
            .bind(workspace.id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        recipient.id
    );
    app.cleanup().await;
}

#[tokio::test]
async fn existing_event_connection_and_revision_cannot_survive_deactivation() {
    let (app, org, admin, member, workspace) = setup().await;
    let channel = g5_api::ws::Channel::Workspace(workspace.id);
    let event = g5_api::ws::WsEvent::WorkspaceSync {
        workspace_id: workspace.id,
        behind_base: 0,
        ahead_of_base: 0,
    };
    let mut ws = app
        .ws_connect(&member, &format!("workspace:{}", workspace.id))
        .await;
    ws.expect_subscribed().await;
    app.state.event_bus.ws_hub().publish(&channel, &event);
    ws.expect_event("workspace.sync", Duration::from_secs(5))
        .await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    app.state.event_bus.ws_hub().publish(&channel, &event);
    assert_eq!(
        ws.count_events("workspace.sync", Duration::from_millis(200))
            .await,
        0
    );
    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_admin_deactivations_serialize_without_mutual_lockout() {
    let (app, org, owner, _, _) = setup().await;
    let a = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    let b = TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin]).await;
    sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=ANY($2)")
        .bind(org.id)
        .bind(vec![a.id, b.id])
        .execute(&app.db)
        .await
        .unwrap();
    let (a_result, b_result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            activity(&app, &a, org.id, b.id, false, 0),
            activity(&app, &b, org.id, a.id, false, 0)
        )
    })
    .await
    .unwrap();
    assert_eq!(
        usize::from(a_result.0.is_success()) + usize::from(b_result.0.is_success()),
        1,
        "{a_result:?} {b_result:?}"
    );
    assert_eq!(
        activity(
            &app,
            &owner,
            org.id,
            if a_result.0.is_success() { b.id } else { a.id },
            true,
            1
        )
        .await
        .0,
        StatusCode::OK
    );
    app.cleanup().await;
}

#[tokio::test]
async fn refresh_switch_and_already_approved_cli_exchange_recheck_membership() {
    let (app, org, admin, member, _) = setup().await;
    let other = TestOrg::create(&app).await;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(other.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    let tokens = app
        .state
        .auth_service
        .issue_user_tokens(member.id, Some(org.id), g5_auth::SESSION_CLIENT_BROWSER)
        .await
        .unwrap();
    let device = app
        .state
        .auth_service
        .start_cli_device_authorization(None, Some(org.id), None)
        .await
        .unwrap();
    app.state
        .auth_service
        .approve_cli_device_authorization(member.id, &device.user_code, true)
        .await
        .unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert!(app
        .state
        .auth_service
        .poll_cli_device_authorization(&device.device_code)
        .await
        .is_err());
    let pending = app
        .state
        .auth_service
        .start_cli_device_authorization(None, Some(org.id), None)
        .await
        .unwrap();
    assert!(app
        .state
        .auth_service
        .approve_cli_device_authorization(member.id, &pending.user_code, true)
        .await
        .is_err());
    assert!(app
        .state
        .auth_service
        .resolve_refresh_identity(&tokens.refresh_token, Some(org.id))
        .await
        .is_err());
    let refreshed = app
        .state
        .auth_service
        .resolve_refresh_identity(&tokens.refresh_token, None)
        .await
        .unwrap();
    assert_eq!(refreshed.org_id, other.id);
    // After the final usable organization is disabled, the person still has
    // an account session, and the original refresh credential is preserved.
    sqlx::query("UPDATE org_memberships SET deactivated_at=now(), activity_revision=1, deactivation_cleanup_id=$3 WHERE org_id=$1 AND user_id=$2")
        .bind(other.id).bind(member.id).bind(Uuid::new_v4()).execute(&app.db).await.unwrap();
    assert!(app
        .state
        .auth_service
        .resolve_refresh_identity(&tokens.refresh_token, None)
        .await
        .unwrap()
        .org_id
        .is_nil());
    app.cleanup().await;
}

#[tokio::test]
async fn database_guards_reject_late_ready_and_new_inactive_ownership() {
    let (app, org, admin, member, workspace) = setup().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, member.id).await;
    sqlx::query("UPDATE workspace_vms SET status='starting', start_deadline_at=now()+interval '2 minutes' WHERE workspace_id=$1")
        .bind(workspace.id).execute(&app.db).await.unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert!(
        sqlx::query("UPDATE workspace_vms SET status='running' WHERE workspace_id=$1")
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .is_err()
    );
    let second = TestWorkspace::create(&app, &org).await;
    assert!(
        sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
            .bind(second.id)
            .bind(member.id)
            .execute(&app.db)
            .await
            .is_err()
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    assert!(
        sqlx::query("UPDATE workspace_vms SET status='running' WHERE workspace_id=$1")
            .bind(workspace.id)
            .execute(&app.db)
            .await
            .is_err(),
        "reactivation cannot make an old launch ready"
    );
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn open_terminal_rechecks_both_directions_without_invalidation() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let (app, org, admin, member, workspace) = setup().await;
    let (peer, mut streams) =
        TestWorkspaceRuntimePeer::attach_with_streams(&app, &workspace, member.id).await;
    let mut connections = Vec::new();
    for _ in 0..2 {
        let response = app
            .authenticated_client(&member)
            .post_json(
                "/api/v1/auth/ws-ticket",
                &json!({"workspaceId":workspace.id}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        let (mut browser, _) = tokio_tungstenite::connect_async(format!(
            "{}/api/v1/workspaces/{}/terminal?token={}",
            app.ws_url,
            workspace.id,
            body["data"]["ticket"].as_str().unwrap()
        ))
        .await
        .unwrap();
        let mut guest = tokio::time::timeout(Duration::from_secs(5), streams.recv())
            .await
            .unwrap()
            .unwrap();
        guest.recv().await.unwrap(); // Mux incarnation header.
        let open = guest.recv().await.unwrap();
        let open: Value = serde_json::from_slice(&open).unwrap();
        assert_eq!(open["type"], "open");
        guest.send(b"before revocation").await.unwrap();
        assert_eq!(
            browser.next().await.unwrap().unwrap().into_data(),
            b"before revocation"
        );
        connections.push((browser, guest));
    }
    // Mutate through a different connection; no hub invalidation is published.
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    let (mut browser, guest) = connections.pop().unwrap();
    guest.send(b"protected output").await.unwrap();
    assert!(!matches!(
        tokio::time::timeout(Duration::from_secs(5), browser.next())
            .await
            .unwrap(),
        Some(Ok(Message::Binary(_) | Message::Text(_)))
    ));
    let (mut browser, mut guest) = connections.pop().unwrap();
    browser
        .send(Message::Text(
            "{\"type\":\"input\",\"data\":\"secret command\"}".into(),
        ))
        .await
        .unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(5), guest.recv())
        .await
        .unwrap()
        .is_none());
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn open_preview_body_drops_new_bytes_after_revocation() {
    let mut config = helpers::test_config();
    config.server.external_url = "https://app.g5.example".into();
    config.server.preview_host_suffix = Some("preview.g5.example".into());
    let app = TestApp::spawn_with_config(config).await;
    let mut admin = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut admin).await;
    let member = TestUser::create_with_scopes(
        &app,
        org.id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::TasksRead,
        ],
    )
    .await;
    let workspace = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
        .bind(workspace.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    let (peer, mut streams) =
        TestWorkspaceRuntimePeer::attach_with_streams(&app, &workspace, member.id).await;
    let task =
        crate::api_task_lifecycle::running_preview(&app, &workspace, member.id, peer.vm_id()).await;
    let ticket = app
        .state
        .auth_service
        .mint_preview_ticket(member.id, org.id, task)
        .unwrap();
    let request = app
        .client
        .get(format!("{}/events", app.base_url))
        .header("host", format!("{task}.preview.g5.example"))
        .header("cookie", format!("__Host-g5_preview={ticket}"));
    let response = tokio::spawn(async move { request.send().await.unwrap() });
    let mut guest = tokio::time::timeout(Duration::from_secs(5), streams.recv())
        .await
        .unwrap()
        .unwrap();
    // First mux frame is the incarnation/port header; then the HTTP request.
    let mut received = Vec::new();
    while !received.windows(4).any(|s| s == b"\r\n\r\n") {
        received.extend(guest.recv().await.unwrap());
    }
    guest.send(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n6\r\nbefore\r\n").await.unwrap();
    let mut response = response.await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.chunk().await.unwrap().unwrap().as_ref(), b"before");
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    guest.send(b"6\r\nsecret\r\n").await.unwrap();
    let next = tokio::time::timeout(Duration::from_secs(5), response.chunk())
        .await
        .unwrap();
    assert!(
        matches!(next, Ok(None) | Err(_)),
        "revoked preview leaked {next:?}"
    );
    drop(response);
    drop(guest);
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn transfer_rechecks_the_actor_and_recipient_after_the_save() {
    for deactivate_actor in [false, true] {
        let (app, org, admin, member, workspace) = setup().await;
        let actor =
            TestUser::create_with_scopes(&app, org.id, &[Scope::OrgAdmin, Scope::WorkspacesWrite])
                .await;
        sqlx::query("UPDATE org_memberships SET role='admin' WHERE org_id=$1 AND user_id=$2")
            .bind(org.id)
            .bind(actor.id)
            .execute(&app.db)
            .await
            .unwrap();
        let recipient = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
        let peer = TestWorkspaceRuntimePeer::attach_with_rpc_barrier(
            &app,
            &workspace,
            member.id,
            "workspace.flush",
        )
        .await;
        let client = app.authenticated_client(&actor);
        let path = format!("/api/v1/workspaces/{}/transfer", workspace.id);
        let request = tokio::spawn(async move {
            client
                .post_json(
                    &path,
                    &json!({"toUserId":recipient.id,"idempotencyKey":Uuid::new_v4().to_string()}),
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(15), async {
            while !peer
                .received_rpc_methods()
                .await
                .iter()
                .any(|m| m == "workspace.flush")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            activity(
                &app,
                &admin,
                org.id,
                if deactivate_actor {
                    actor.id
                } else {
                    recipient.id
                },
                false,
                0
            )
            .await
            .0,
            StatusCode::OK
        );
        peer.release_rpc_barrier();
        let response = request.await.unwrap();
        let status = response.status();
        let body: Value = response.json().await.unwrap();
        assert!(
            !status.is_success() || body["data"]["status"] != "completed",
            "{body}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, Uuid>("SELECT owner_user_id FROM workspaces WHERE id=$1")
                .bind(workspace.id)
                .fetch_one(&app.db)
                .await
                .unwrap(),
            member.id
        );
        drop(peer);
        app.cleanup().await;
    }
}

fn notification_event(workspace_id: Uuid) -> g5_api::ws::WsEvent {
    g5_api::ws::WsEvent::Notification {
        workspace_id,
        notification: g5_api::ws::events::NotificationPayload {
            id: Uuid::new_v4(),
            severity: "info".into(),
            category: "mention".into(),
            title: "Protected".into(),
            message: "Organization data".into(),
            task_id: None,
            comment_id: None,
            reply_id: None,
            actions: vec![],
            created_at: chrono::Utc::now(),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn another_organizations_user_feed_filters_the_revoked_organization() {
    let (app, org, admin, mut member, workspace) = setup().await;
    let other = TestOrg::create(&app).await;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(other.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    let accessible = TestWorkspace::create(&app, &other).await;
    member.jwt = helpers::test_jwt_minter()
        .mint(member.id, other.id, vec![Scope::WorkspacesRead], 3600)
        .unwrap();
    let channel = g5_api::ws::Channel::User(member.id);
    let mut ws = app
        .ws_connect(&member, &format!("user:{}", member.id))
        .await;
    ws.expect_subscribed().await;
    app.state
        .event_bus
        .ws_hub()
        .publish(&channel, &notification_event(workspace.id));
    ws.expect_event("notification", Duration::from_secs(5))
        .await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    app.state
        .event_bus
        .ws_hub()
        .publish(&channel, &notification_event(workspace.id));
    app.state
        .event_bus
        .ws_hub()
        .publish(&channel, &notification_event(accessible.id));
    let delivered = ws
        .expect_event("notification", Duration::from_secs(5))
        .await;
    assert_eq!(delivered["workspaceId"], accessible.id.to_string());
    assert_eq!(
        ws.count_events("notification", Duration::from_millis(200))
            .await,
        0
    );
    app.cleanup().await;
}

#[tokio::test]
async fn user_feed_admits_newly_available_memberships_without_rebinding_revoked_ones() {
    for initially_inactive in [false, true] {
        let (app, _org, _admin, member, workspace) = setup().await;
        let mut other_admin = TestUser::create(&app).await;
        let other = TestOrg::create_with_admin(&app, &mut other_admin).await;
        let other_workspace = TestWorkspace::create(&app, &other).await;
        if initially_inactive {
            sqlx::query(
                "INSERT INTO org_memberships(org_id,user_id,role,deactivated_at,activity_revision)
                 VALUES($1,$2,'member',now(),1)",
            )
            .bind(other.id)
            .bind(member.id)
            .execute(&app.db)
            .await
            .unwrap();
        }
        let channel = g5_api::ws::Channel::User(member.id);
        let mut ws = app
            .ws_connect(&member, &format!("user:{}", member.id))
            .await;
        ws.expect_subscribed().await;
        let hub = app.state.event_bus.ws_hub();
        hub.publish(&channel, &notification_event(other_workspace.id));
        hub.publish(&channel, &notification_event(workspace.id));
        let delivered = ws
            .expect_event("notification", Duration::from_secs(5))
            .await;
        assert_eq!(delivered["workspaceId"], workspace.id.to_string());

        let revision = if initially_inactive {
            assert_eq!(
                activity(&app, &other_admin, other.id, member.id, true, 1)
                    .await
                    .0,
                StatusCode::OK
            );
            2
        } else {
            sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
                .bind(other.id)
                .bind(member.id)
                .execute(&app.db)
                .await
                .unwrap();
            0
        };
        hub.publish(&channel, &notification_event(other_workspace.id));
        let delivered = ws
            .expect_event("notification", Duration::from_secs(5))
            .await;
        assert_eq!(delivered["workspaceId"], other_workspace.id.to_string());

        assert_eq!(
            activity(&app, &other_admin, other.id, member.id, false, revision)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            activity(&app, &other_admin, other.id, member.id, true, revision + 1)
                .await
                .0,
            StatusCode::OK
        );
        hub.publish(&channel, &notification_event(other_workspace.id));
        hub.publish(&channel, &notification_event(workspace.id));
        let delivered = ws
            .expect_event("notification", Duration::from_secs(5))
            .await;
        assert_eq!(delivered["workspaceId"], workspace.id.to_string());
        assert_eq!(
            ws.count_events("notification", Duration::from_millis(200))
                .await,
            0
        );
        app.cleanup().await;
    }
}

#[tokio::test]
async fn completed_cleanup_requires_fresh_start_and_stale_cleanup_cannot_clear_a_new_cycle() {
    let (app, org, admin, member, workspace) = setup().await;
    let peer = TestWorkspaceRuntimePeer::attach(&app, &workspace, member.id).await;
    let worker = service(&app, org.id).await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    let old: Uuid = sqlx::query_scalar(
        "SELECT deactivation_cleanup_id FROM org_memberships WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org.id)
    .bind(member.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    worker.reconcile_member_deactivation(16).await.unwrap();
    worker.reconcile_stopping(16).await.unwrap();
    worker.reconcile_member_deactivation(16).await.unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, true, 1).await.0,
        StatusCode::OK
    );
    let started = worker
        .resume(
            workspace.id,
            member.id,
            Uuid::new_v4(),
            g5_api::services::workspace_vm_service::StartKind::Resume,
        )
        .await;
    assert!(
        started.is_ok(),
        "fresh explicit Start is admitted: {started:?}"
    );
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 2).await.0,
        StatusCode::OK
    );
    let mut db = app.scoped_db_for_org(org.id).await;
    let cleared: bool = sqlx::query_scalar("SELECT authz.finish_member_cleanup($1,$2,$3)")
        .bind(org.id)
        .bind(member.id)
        .bind(old)
        .fetch_one(db.conn())
        .await
        .unwrap();
    assert!(!cleared);
    db.commit().await.unwrap();
    let current: Option<Uuid> = sqlx::query_scalar(
        "SELECT deactivation_cleanup_id FROM org_memberships WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org.id)
    .bind(member.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(current.is_some() && current != Some(old));
    drop(peer);
    app.cleanup().await;
}

#[tokio::test]
async fn unavailable_membership_authority_fails_closed_for_stream_delivery() {
    let (app, org, _, member, _) = setup().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(app.db.connect_options().as_ref().clone())
        .await
        .unwrap();
    let guard = g5_api::services::identity::membership_activity::MembershipActivityGuard::capture(
        &pool, org.id, member.id,
    )
    .await
    .unwrap();
    assert!(guard.authorize().await);
    pool.close().await;
    assert!(
        !guard.authorize().await,
        "a previous success cannot replace current database authority"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn event_delivery_reuses_subscription_scope_without_reminting_a_session() {
    let (app, _, _, member, workspace) = setup().await;
    let channel = g5_api::ws::Channel::Workspace(workspace.id);
    let mut ws = app.ws_connect(&member, &channel.to_string()).await;
    ws.expect_subscribed().await;
    let mut lock = app.db.begin().await.unwrap();
    sqlx::query("LOCK TABLE authz.user_sessions IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    app.state.event_bus.publish(
        &channel,
        &g5_api::ws::WsEvent::TaskStatus {
            task_id: Uuid::new_v4(),
            status: "protected".into(),
        },
    );
    // The existing subscription lease supplies resource authority; delivery
    // still reads membership, but does not remint a scoped session per event.
    ws.expect_event("task.status", Duration::from_secs(2)).await;
    lock.rollback().await.unwrap();
    ws.close().await;
    app.cleanup().await;
}

#[tokio::test]
async fn blocked_membership_delivery_closes_without_forwarding_buffered_events() {
    let (app, _, _, member, workspace) = setup().await;
    let channel = g5_api::ws::Channel::Workspace(workspace.id);
    let mut ws = app.ws_connect(&member, &channel.to_string()).await;
    ws.expect_subscribed().await;
    let event = g5_api::ws::WsEvent::TaskStatus {
        task_id: Uuid::new_v4(),
        status: "protected".into(),
    };
    app.state.event_bus.publish(&channel, &event);
    ws.expect_event("task.status", Duration::from_secs(5)).await;

    // A membership verdict stands for one bounded batch or one second; past
    // that the next delivery rechecks (`g5.security.membership-streams`).
    tokio::time::sleep(g5_api::ws::MEMBERSHIP_VERDICT_TTL + Duration::from_millis(100)).await;
    let mut lock = app.db.begin().await.unwrap();
    sqlx::query("LOCK TABLE org_memberships IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    app.state.event_bus.publish(&channel, &event);
    // An otherwise valid heartbeat lease cannot replace fresh membership
    // admission. The protected event must be dropped while the query is blocked.
    ws.expect_closed(Duration::from_secs(10)).await;
    lock.rollback().await.unwrap();
    assert_eq!(
        app.state
            .event_bus
            .ws_hub()
            .channel_subscriber_count(&channel),
        0
    );
    app.cleanup().await;
}

#[tokio::test]
async fn failed_cleanup_admissions_do_not_starve_later_members_and_survive_recreation() {
    let (app, org, admin, mut failing_owner, first) = setup().await;
    let mut later_owner = TestUser::create_with_scopes(
        &app,
        org.id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await;
    if failing_owner.id > later_owner.id {
        std::mem::swap(&mut failing_owner, &mut later_owner);
    }
    let mut failing_workspaces = vec![first];
    for _ in 1..16 {
        failing_workspaces.push(TestWorkspace::create(&app, &org).await);
    }
    let later_workspaces = vec![
        TestWorkspace::create(&app, &org).await,
        TestWorkspace::create(&app, &org).await,
    ];
    for (workspaces, owner, name) in [
        (
            &failing_workspaces,
            &failing_owner,
            "fail-cleanup-admission",
        ),
        (&later_workspaces, &later_owner, "healthy-cleanup-admission"),
    ] {
        for workspace in workspaces {
            sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
                .bind(workspace.id)
                .bind(owner.id)
                .execute(&app.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,$4,'command','queued',$5)")
                .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(name).bind(owner.id)
                .execute(&app.db).await.unwrap();
        }
    }
    sqlx::raw_sql(
        "CREATE FUNCTION public.fail_cleanup_admission() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN IF OLD.name='fail-cleanup-admission' AND NEW.status='waiting' THEN
            RAISE EXCEPTION 'injected cleanup admission failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER fail_cleanup_admission BEFORE UPDATE ON tasks
        FOR EACH ROW EXECUTE FUNCTION public.fail_cleanup_admission();",
    )
    .execute(&app.db)
    .await
    .unwrap();
    let old_client = app.authenticated_client(&failing_owner);
    let workspace_url = format!("/api/v1/workspaces/{}", failing_workspaces[0].id);
    assert_eq!(
        old_client.get(&workspace_url).await.status(),
        StatusCode::OK
    );
    for owner in [&failing_owner, &later_owner] {
        assert_eq!(
            activity(&app, &admin, org.id, owner.id, false, 0).await.0,
            StatusCode::OK
        );
    }
    let original_cleanup: Uuid = sqlx::query_scalar(
        "SELECT deactivation_cleanup_id FROM org_memberships WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org.id)
    .bind(failing_owner.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        service(&app, org.id)
            .await
            .reconcile_member_deactivation(16)
            .await
            .unwrap(),
        0
    );
    // A new service instance must use durable retry state rather than retry the same page.
    assert_eq!(
        service(&app, org.id)
            .await
            .reconcile_member_deactivation(1)
            .await
            .unwrap(),
        1,
        "failed admissions must let a later member's workspace progress"
    );
    let deferred: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE org_id=$1 AND owner_user_id=$2 AND member_cleanup_retry_id=$3 AND member_cleanup_retry_at > now()")
        .bind(org.id).bind(failing_owner.id).bind(original_cleanup).fetch_one(&app.db).await.unwrap();
    assert_eq!(deferred, 16);
    assert_eq!(
        old_client.get(&workspace_url).await.status(),
        StatusCode::FORBIDDEN
    );
    // Even once earlier retries are due, unattempted work must not be starved.
    sqlx::query("UPDATE workspaces SET member_cleanup_retry_at=now()-interval '1 second' WHERE owner_user_id=$1")
        .bind(failing_owner.id).execute(&app.db).await.unwrap();
    assert_eq!(
        service(&app, org.id)
            .await
            .reconcile_member_deactivation(1)
            .await
            .unwrap(),
        1
    );
    let later_pending: bool = sqlx::query_scalar("SELECT deactivation_cleanup_id IS NOT NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(org.id).bind(later_owner.id).fetch_one(&app.db).await.unwrap();
    assert!(!later_pending);
    // Early Reactivate cannot lose the retry obligation or grant execution consent.
    assert_eq!(
        activity(&app, &admin, org.id, failing_owner.id, true, 1)
            .await
            .0,
        StatusCode::OK
    );
    let retained: Option<Uuid> = sqlx::query_scalar(
        "SELECT deactivation_cleanup_id FROM org_memberships WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org.id)
    .bind(failing_owner.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(retained, Some(original_cleanup));
    sqlx::raw_sql("DROP TRIGGER fail_cleanup_admission ON tasks; DROP FUNCTION public.fail_cleanup_admission();")
        .execute(&app.db).await.unwrap();
    assert_eq!(
        service(&app, org.id)
            .await
            .reconcile_member_deactivation(16)
            .await
            .unwrap(),
        16
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces WHERE org_id=$1 AND (member_cleanup_retry_id IS NOT NULL OR member_cleanup_retry_at IS NOT NULL)")
        .bind(org.id).fetch_one(&app.db).await.unwrap();
    assert_eq!(remaining, 0);
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM tasks WHERE org_id=$1 AND status='queued'")
            .bind(org.id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(queued, 0);
    let retained: Option<Uuid> = sqlx::query_scalar(
        "SELECT deactivation_cleanup_id FROM org_memberships WHERE org_id=$1 AND user_id=$2",
    )
    .bind(org.id)
    .bind(failing_owner.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(retained, None);
    // Retry metadata from a completed cycle cannot defer a later deactivation.
    sqlx::query("UPDATE workspaces SET member_cleanup_retry_id=$2,member_cleanup_retry_at=now()+interval '1 hour' WHERE id=$1")
        .bind(failing_workspaces[0].id).bind(original_cleanup).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'new cycle','command','queued',$4)")
        .bind(Uuid::new_v4()).bind(org.id).bind(failing_workspaces[0].id).bind(failing_owner.id)
        .execute(&app.db).await.unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, failing_owner.id, false, 2)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        service(&app, org.id)
            .await
            .reconcile_member_deactivation(16)
            .await
            .unwrap(),
        1
    );
    app.cleanup().await;
}

#[tokio::test]
async fn rls_membership_initplan_preserves_isolation_and_next_statement_revocation() {
    let (app, org, admin, member, _) = setup().await;
    let other = TestOrg::create(&app).await;
    sqlx::query("INSERT INTO org_memberships(org_id,user_id,role) VALUES($1,$2,'member')")
        .bind(other.id)
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    let other_workspace = TestWorkspace::create(&app, &other).await;
    sqlx::query(
        "INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by)
        SELECT gen_random_uuid(),$1,'rls-row-' || n,$2,$2 FROM generate_series(1,128) n",
    )
    .bind(org.id)
    .bind(member.id)
    .execute(&app.db)
    .await
    .unwrap();
    let mut db = app.scoped_db_for_user_org(member.id, org.id).await;
    sqlx::raw_sql("SET LOCAL enable_indexscan=off; SET LOCAL enable_bitmapscan=off;")
        .execute(db.conn())
        .await
        .unwrap();
    let explain: Value =
        sqlx::query_scalar("EXPLAIN (ANALYZE, FORMAT JSON) SELECT id FROM workspaces")
            .fetch_one(db.conn())
            .await
            .unwrap();
    let plan = &explain[0]["Plan"];
    assert_eq!(plan["Node Type"], "Seq Scan", "{explain}");
    assert!(
        plan["Actual Rows"]
            .as_f64()
            .is_some_and(|rows| rows >= 128.0),
        "{explain}"
    );
    assert!(
        !plan["Filter"].as_str().unwrap().contains("current_org_ids"),
        "membership is not a per-row call: {explain}"
    );
    let initplans: Vec<_> = plan["Plans"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["Parent Relationship"] == "InitPlan")
        .collect();
    assert!(!initplans.is_empty(), "{explain}");
    assert!(
        initplans
            .iter()
            .all(|p| p["Actual Loops"].as_f64() == Some(1.0)),
        "{explain}"
    );
    let visible: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT org_id FROM workspaces")
        .fetch_all(db.conn())
        .await
        .unwrap();
    assert_eq!(visible, vec![org.id]);
    assert_eq!(
        sqlx::query("UPDATE workspaces SET name='cross-org' WHERE id=$1")
            .bind(other_workspace.id)
            .execute(db.conn())
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    sqlx::query("SAVEPOINT rejected_write")
        .execute(db.conn())
        .await
        .unwrap();
    let error = sqlx::query("INSERT INTO workspaces(id,org_id,name,owner_user_id,created_by) VALUES($1,$2,'cross-org',$3,$3)")
        .bind(Uuid::new_v4()).bind(other.id).bind(member.id).execute(db.conn()).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    sqlx::query("ROLLBACK TO SAVEPOINT rejected_write")
        .execute(db.conn())
        .await
        .unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspaces")
            .fetch_one(db.conn())
            .await
            .unwrap(),
        0,
        "an InitPlan never caches authority across statements"
    );
    assert_eq!(
        sqlx::query("UPDATE workspaces SET name='revoked'")
            .execute(db.conn())
            .await
            .unwrap()
            .rows_affected(),
        0
    );
    db.rollback().await.unwrap();
    let mut other_db = app.scoped_db_for_user_org(member.id, other.id).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM workspaces WHERE id=$1")
            .bind(other_workspace.id)
            .fetch_one(other_db.conn())
            .await
            .unwrap(),
        1
    );
    other_db.rollback().await.unwrap();
    app.cleanup().await;
}

#[tokio::test]
async fn shutdown_progress_retains_deleted_obligations_until_settled() {
    let (app, org, admin, member, workspace) = setup().await;
    let settled = TestWorkspace::create(&app, &org).await;
    sqlx::query("UPDATE workspaces SET owner_user_id=$2, deleted_at=now() WHERE id=ANY($1)")
        .bind(vec![workspace.id, settled.id])
        .bind(member.id)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'deleted queued work','command','queued',$4)")
        .bind(Uuid::new_v4()).bind(org.id).bind(workspace.id).bind(member.id).execute(&app.db).await.unwrap();
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    let client = app.authenticated_client(&admin);
    let url = format!("/api/v1/orgs/{}/members/{}/shutdown", org.id, member.id);
    let progress: Value = client.get(&url).await.json().await.unwrap();
    assert_eq!(progress["data"].as_array().unwrap().len(), 1, "{progress}");
    assert_eq!(progress["data"][0]["workspaceId"], workspace.id.to_string());
    assert_eq!(progress["data"][0]["state"], "pending");
    assert_eq!(
        service(&app, org.id)
            .await
            .reconcile_member_deactivation(16)
            .await
            .unwrap(),
        1
    );
    let progress: Value = client.get(&url).await.json().await.unwrap();
    assert_eq!(progress["data"], json!([]));
    app.cleanup().await;
}

#[tokio::test]
async fn failed_stop_deferral_does_not_skip_later_admissions_or_settled_members() {
    let (app, org, admin, member, first) = setup().await;
    let second = TestWorkspace::create(&app, &org).await;
    let (running, queued) = if first.id < second.id {
        (first, second)
    } else {
        (second, first)
    };
    for workspace in [&running, &queued] {
        sqlx::query("UPDATE workspaces SET owner_user_id=$2 WHERE id=$1")
            .bind(workspace.id)
            .bind(member.id)
            .execute(&app.db)
            .await
            .unwrap();
    }
    let peer = TestWorkspaceRuntimePeer::attach(&app, &running, member.id).await;
    sqlx::query("INSERT INTO tasks(id,org_id,workspace_id,name,kind,status,created_by) VALUES($1,$2,$3,'later queued work','command','queued',$4)")
        .bind(Uuid::new_v4()).bind(org.id).bind(queued.id).bind(member.id).execute(&app.db).await.unwrap();
    let settled_member =
        TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesWrite]).await;
    sqlx::raw_sql(
        "CREATE FUNCTION public.fail_stop_deferral() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN IF NEW.last_error->>'reason'='stop_awaiting_tunnel_owner' THEN
          RAISE EXCEPTION 'injected stop deferral failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER fail_stop_deferral BEFORE UPDATE ON workspace_vms
        FOR EACH ROW EXECUTE FUNCTION public.fail_stop_deferral();",
    )
    .execute(&app.db)
    .await
    .unwrap();
    for owner in [&member, &settled_member] {
        assert_eq!(
            activity(&app, &admin, org.id, owner.id, false, 0).await.0,
            StatusCode::OK
        );
    }
    let worker = service(&app, org.id).await;
    assert_eq!(worker.reconcile_member_deactivation(16).await.unwrap(), 1);
    let pending: bool = sqlx::query_scalar("SELECT deactivation_cleanup_id IS NOT NULL FROM org_memberships WHERE org_id=$1 AND user_id=$2")
        .bind(org.id).bind(settled_member.id).fetch_one(&app.db).await.unwrap();
    assert!(
        !pending,
        "settled memberships finish despite another member's deferral error"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE workspace_id=$1")
            .bind(queued.id)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        "waiting"
    );
    // Failed handoff retains the admitted stop for the ordinary claim executor.
    sqlx::raw_sql("DROP TRIGGER fail_stop_deferral ON workspace_vms; DROP FUNCTION public.fail_stop_deferral();")
        .execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspace_vms SET stop_retry_at=now() WHERE workspace_id=$1")
        .bind(running.id)
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(worker.reconcile_stopping(16).await.unwrap(), 1);
    drop(peer);
    app.cleanup().await;
}

fn activity_checks() -> u64 {
    g5_api::services::identity::membership_activity::ACTIVITY_CHECKS
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// A burst of queued events is admitted as bounded batches under one
/// membership recheck each, and a verdict stands for a second, so a
/// 300-event burst costs a handful of queries, not 300
/// (`g5.security.membership-streams`).
#[tokio::test]
async fn a_burst_is_delivered_under_one_membership_recheck_per_batch() {
    let (app, _, _, member, workspace) = setup().await;
    let channel = g5_api::ws::Channel::Workspace(workspace.id);
    let mut ws = app.ws_connect(&member, &channel.to_string()).await;
    ws.expect_subscribed().await;
    let before = activity_checks();
    for index in 0..300u32 {
        app.state.event_bus.publish(
            &channel,
            &g5_api::ws::WsEvent::TaskStatus {
                task_id: Uuid::new_v4(),
                status: format!("burst-{index}"),
            },
        );
    }
    for _ in 0..300 {
        ws.expect_event("task.status", Duration::from_secs(5)).await;
    }
    let checks = activity_checks() - before;
    assert!(
        checks <= 4,
        "300 delivered events ran {checks} membership checks; the bound is one per batch within a second"
    );
    ws.close().await;
    app.cleanup().await;
}

/// A deactivation committed between two batches closes the stream before
/// the next batch is sent: the verdict's life is the bound of the delay.
#[tokio::test]
async fn a_deactivation_between_batches_closes_the_stream_before_the_next_batch() {
    let (app, org, _, member, workspace) = setup().await;
    let channel = g5_api::ws::Channel::Workspace(workspace.id);
    let mut ws = app.ws_connect(&member, &channel.to_string()).await;
    ws.expect_subscribed().await;
    for index in 0..3u32 {
        app.state.event_bus.publish(
            &channel,
            &g5_api::ws::WsEvent::TaskStatus {
                task_id: Uuid::new_v4(),
                status: format!("first-{index}"),
            },
        );
    }
    for _ in 0..3 {
        ws.expect_event("task.status", Duration::from_secs(5)).await;
    }
    sqlx::query(
        "UPDATE org_memberships SET deactivated_at = now(), activity_revision = activity_revision + 1
         WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org.id)
    .bind(member.id)
    .execute(&app.db)
    .await
    .unwrap();
    tokio::time::sleep(g5_api::ws::MEMBERSHIP_VERDICT_TTL + Duration::from_millis(100)).await;
    for index in 0..3u32 {
        app.state.event_bus.publish(
            &channel,
            &g5_api::ws::WsEvent::TaskStatus {
                task_id: Uuid::new_v4(),
                status: format!("second-{index}"),
            },
        );
    }
    ws.expect_closed(Duration::from_secs(10)).await;
    assert_eq!(
        app.state
            .event_bus
            .ws_hub()
            .channel_subscriber_count(&channel),
        0
    );
    app.cleanup().await;
}

/// A deactivation made through this API closes the stream at the very next
/// batch, with no wait for the verdict's expiry: the change bumps the hub's
/// membership generation that every standing verdict is keyed on.
#[tokio::test]
async fn an_activity_change_through_the_api_closes_the_stream_at_the_next_batch() {
    let (app, org, admin, member, workspace) = setup().await;
    let channel = g5_api::ws::Channel::Workspace(workspace.id);
    let mut ws = app.ws_connect(&member, &channel.to_string()).await;
    ws.expect_subscribed().await;
    app.state.event_bus.publish(
        &channel,
        &g5_api::ws::WsEvent::TaskStatus {
            task_id: Uuid::new_v4(),
            status: "first".into(),
        },
    );
    ws.expect_event("task.status", Duration::from_secs(5)).await;
    assert_eq!(
        activity(&app, &admin, org.id, member.id, false, 0).await.0,
        StatusCode::OK
    );
    // Within the verdict's life: only the generation bump makes the next
    // batch recheck, and the recheck denies.
    app.state.event_bus.publish(
        &channel,
        &g5_api::ws::WsEvent::TaskStatus {
            task_id: Uuid::new_v4(),
            status: "second".into(),
        },
    );
    ws.expect_closed(Duration::from_secs(5)).await;
    assert_eq!(
        app.state
            .event_bus
            .ws_hub()
            .channel_subscriber_count(&channel),
        0
    );
    app.cleanup().await;
}
