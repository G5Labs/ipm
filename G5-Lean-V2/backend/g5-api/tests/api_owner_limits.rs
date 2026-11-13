//! Per-owner limits (`g5.organization.owner-limits`) through the production
//! HTTP, service and database boundaries:
//!
//! - placement refuses a Start once the owner's other VMs and earlier waiting
//!   Starts reach the limit, ends that Start's consent, and leaves other
//!   owners alone (`g5.runtime.placement.owner-limit`);
//! - fork and transfer admission refuse to give an owner more workspaces than
//!   the limit, counting pending forks and transfers, while an admitted fork's
//!   replay is never refused (`g5.workspace.owner-limit`);
//! - both limits are Admin settings (`g5.data.configuration.org-admin-settings`).

use crate::helpers;

use g5_api::services::worker_host_service::{
    PlacementOutcome, PlacementRequest, WorkerHostService,
};
use g5_api::services::workspace_vm_service::StartKind;
use g5_auth::Scope;
use helpers::{AuthenticatedClient, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

async fn put_settings(client: &AuthenticatedClient, org_id: Uuid, body: Value) {
    let response = client
        .put_json(&format!("/api/v1/orgs/{org_id}/admin-settings"), &body)
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn add_ready_host(app: &TestApp, org_id: Uuid, slots: i32) {
    sqlx::query(
        "INSERT INTO worker_hosts(
             host_id, org_id, region, status, last_heartbeat, slots_total,
             slot_size_reported, runtime_identity)
         VALUES ($1, $2, 'us-central1', 'ready', now(), $3, true, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(slots)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .unwrap();
}

/// Record a valid waiting Start requested `age_secs` ago.
async fn seed_start(app: &TestApp, workspace_id: Uuid, org_id: Uuid, age_secs: i64) {
    let (owner_user_id, lifecycle_generation): (Uuid, i64) =
        sqlx::query_as("SELECT owner_user_id, lifecycle_generation FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let created_at = chrono::Utc::now() - chrono::TimeDelta::seconds(age_secs);
    let intent = json!({
        "id": Uuid::new_v4(),
        "kind": "resume",
        "status": "pending",
        "owner_user_id": owner_user_id,
        "lifecycle_generation": lifecycle_generation,
        "attempts": 0,
        "created_at": created_at.to_rfc3339(),
        "expires_at": (chrono::Utc::now() + chrono::TimeDelta::minutes(5)).to_rfc3339(),
    });
    sqlx::query(
        "INSERT INTO workspace_vms(org_id, workspace_id, start_intent)
         VALUES ($1, $2, $3)
         ON CONFLICT (workspace_id) DO UPDATE
         SET status = 'stopped', host_id = NULL, vm_id = NULL,
             start_intent = EXCLUDED.start_intent, updated_at = now()",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(intent)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn place(app: &TestApp, org_id: Uuid, workspace_id: Uuid) -> PlacementOutcome {
    // Database-only fixtures supply their eligible hosts as reachable.
    let reachable_host_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT host_id FROM worker_hosts WHERE org_id=$1")
            .bind(org_id)
            .fetch_all(&app.db)
            .await
            .unwrap();
    let mut db = app.scoped_db_for_org(org_id).await;
    let outcome = WorkerHostService::new()
        .place(
            &mut db,
            PlacementRequest {
                org_id,
                workspace_id,
                region: "us-central1",
                reachable_host_ids: &reachable_host_ids,
                excluded_host_ids: &[],
                start_deadline_at: chrono::Utc::now() + chrono::TimeDelta::minutes(2),
            },
        )
        .await
        .expect("evaluate placement");
    db.commit().await.expect("commit placement");
    outcome
}

async fn start_state(app: &TestApp, workspace_id: Uuid) -> (String, String, Value) {
    sqlx::query_as(
        "SELECT status, start_intent ->> 'status', COALESCE(last_error, 'null'::jsonb)
         FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn member(app: &TestApp, org_id: Uuid) -> TestUser {
    TestUser::create_with_scopes(
        app,
        org_id,
        &[Scope::WorkspacesRead, Scope::WorkspacesWrite],
    )
    .await
}

async fn give_to(app: &TestApp, workspace_id: Uuid, owner: Uuid) {
    sqlx::query("UPDATE workspaces SET owner_user_id = $2, created_by = $2 WHERE id = $1")
        .bind(workspace_id)
        .bind(owner)
        .execute(&app.db)
        .await
        .unwrap();
}

fn assert_owner_limit(status: StatusCode, body: &Value, kind: &str, limit: i64) {
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "OWNER_LIMIT_REACHED", "{body}");
    assert_eq!(
        body["error"]["details"],
        json!({"kind": kind, "limit": limit}),
        "{body}"
    );
}

#[tokio::test]
async fn owner_limits_are_admin_settings_with_defaults_and_ranges() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/orgs/{}/admin-settings", org.id);

    let body: Value = client.get(&path).await.json().await.unwrap();
    assert_eq!(body["data"]["maxRunningVmsPerOwner"], 5, "{body}");
    assert_eq!(body["data"]["maxWorkspacesPerOwner"], 50, "{body}");

    put_settings(
        &client,
        org.id,
        json!({"maxRunningVmsPerOwner": 100, "maxWorkspacesPerOwner": 1000}),
    )
    .await;
    put_settings(&client, org.id, json!({"maxRunningVmsPerOwner": 0})).await;
    let body: Value = client.get(&path).await.json().await.unwrap();
    assert_eq!(body["data"]["maxRunningVmsPerOwner"], 0, "{body}");
    assert_eq!(body["data"]["maxWorkspacesPerOwner"], 1000, "{body}");

    for (field, value) in [
        ("maxRunningVmsPerOwner", 101),
        ("maxRunningVmsPerOwner", -1),
        ("maxWorkspacesPerOwner", 1001),
        ("maxWorkspacesPerOwner", -1),
    ] {
        let response = client.put_json(&path, &json!({ field: value })).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value = response.json().await.unwrap();
        assert!(body["error"]["details"][field].is_string(), "{body}");
    }
    // The database registry refuses what the API would.
    let refused = sqlx::query(
        "INSERT INTO configs(org_id, scope, scope_id, key, value)
         VALUES ($1, 'org', $1, 'max_workspaces_per_owner', '1001'::jsonb)",
    )
    .bind(org.id)
    .execute(&app.db)
    .await;
    assert!(refused.is_err());

    app.cleanup().await;
}

#[tokio::test]
async fn placement_refuses_a_start_over_the_owner_limit_and_ends_its_consent() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    add_ready_host(&app, org.id, 8).await;
    put_settings(&client, org.id, json!({"maxRunningVmsPerOwner": 2})).await;

    let first = TestWorkspace::create(&app, &org).await;
    let second = TestWorkspace::create(&app, &org).await;
    let third = TestWorkspace::create(&app, &org).await;
    for workspace in [&first, &second, &third] {
        seed_start(&app, workspace.id, org.id, 0).await;
    }
    assert!(matches!(
        place(&app, org.id, first.id).await,
        PlacementOutcome::Reserved(_)
    ));
    assert!(matches!(
        place(&app, org.id, second.id).await,
        PlacementOutcome::Reserved(_)
    ));
    assert_eq!(
        place(&app, org.id, third.id).await,
        PlacementOutcome::OwnerLimitExceeded { limit: 2 }
    );
    assert_eq!(
        start_state(&app, third.id).await,
        (
            "stopped".to_string(),
            "failed".to_string(),
            json!({"reason": "owner_vm_limit", "limit": 2})
        )
    );
    // Its consent is over: a retry of the same Start does not place.
    assert_eq!(
        place(&app, org.id, third.id).await,
        PlacementOutcome::StartIntentUnavailable
    );

    // Another owner in the same organization is unaffected.
    let other = member(&app, org.id).await;
    let theirs = TestWorkspace::create(&app, &org).await;
    give_to(&app, theirs.id, other.id).await;
    seed_start(&app, theirs.id, org.id, 0).await;
    assert!(matches!(
        place(&app, org.id, theirs.id).await,
        PlacementOutcome::Reserved(_)
    ));

    // Lowering the limit stops nothing; the owner's next Start is refused.
    put_settings(&client, org.id, json!({"maxRunningVmsPerOwner": 0})).await;
    assert_eq!(start_state(&app, first.id).await.0, "starting");
    assert_eq!(start_state(&app, second.id).await.0, "starting");
    seed_start(&app, third.id, org.id, 0).await;
    assert_eq!(
        place(&app, org.id, third.id).await,
        PlacementOutcome::OwnerLimitExceeded { limit: 0 }
    );

    app.cleanup().await;
}

#[tokio::test]
async fn earlier_waiting_starts_count_toward_the_owner_limit_and_later_ones_do_not() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    add_ready_host(&app, org.id, 8).await;
    // One running VM fills the organization quota, so later Starts wait.
    put_settings(
        &client,
        org.id,
        json!({"maxRunningVms": 1, "maxRunningVmsPerOwner": 2}),
    )
    .await;
    let running = TestWorkspace::create(&app, &org).await;
    seed_start(&app, running.id, org.id, 600).await;
    assert!(matches!(
        place(&app, org.id, running.id).await,
        PlacementOutcome::Reserved(_)
    ));

    let older = TestWorkspace::create(&app, &org).await;
    let newer = TestWorkspace::create(&app, &org).await;
    seed_start(&app, older.id, org.id, 60).await;
    seed_start(&app, newer.id, org.id, 0).await;

    // The newer Start places first but the older one, still waiting, counts:
    // one VM plus one earlier Start is the limit.
    assert_eq!(
        place(&app, org.id, newer.id).await,
        PlacementOutcome::OwnerLimitExceeded { limit: 2 }
    );
    // The older Start does not count the refused newer one; it waits for the
    // organization quota, not the owner limit.
    assert_eq!(
        place(&app, org.id, older.id).await,
        PlacementOutcome::QuotaExceeded { limit: 1 }
    );
    let (_, intent, last_error) = start_state(&app, older.id).await;
    assert_eq!(intent, "queued_for_capacity");
    assert_eq!(last_error["reason"], "org_vm_quota");

    app.cleanup().await;
}

#[tokio::test]
async fn start_route_refuses_over_the_owner_limit_and_the_queue_drops_lowered_starts() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    put_settings(&client, org.id, json!({"maxRunningVmsPerOwner": 1})).await;
    sqlx::query("UPDATE orgs SET default_base_image_ref = $2 WHERE id = $1")
        .bind(org.id)
        .bind(helpers::TEST_WORKER_IMAGE_REF)
        .execute(&app.db)
        .await
        .unwrap();
    let first = TestWorkspace::create(&app, &org).await;
    first.attach_parent(&app).await;
    let second = TestWorkspace::create(&app, &org).await;
    second.attach_parent(&app).await;

    // No host is ready, so the first Start waits; it fills the limit.
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", first.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["outcome"], "queued_for_capacity");

    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/resume", second.id),
            &json!({"operationId": Uuid::new_v4()}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_owner_limit(status, &body, "running_vms", 1);
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("Stop one")),
        "{body}"
    );
    assert_eq!(start_state(&app, second.id).await.1, "failed");

    // Lowering the limit refuses the waiting Start when the queue replays it,
    // once.
    put_settings(&client, org.id, json!({"maxRunningVmsPerOwner": 0})).await;
    let db = app.scoped_db_for_user_org(owner.id, org.id).await;
    let scope = db.reopener();
    db.rollback().await.unwrap();
    let service =
        g5_api::services::WorkspaceVmService::new(g5_api::services::WorkspaceVmServiceConfig {
            system_pool: app.state.system_pool.clone(),
            scope,
            hosts: app.state.preview_tunnels.clone(),
            minter: app.state.jwt_minter.clone().expect("lifecycle JWT minter"),
            runtime: app.state.workspace_runtime.clone(),
            agent_work: app.state.agent_work.clone(),
            repo_storage: app.state.repo_storage.clone(),
            git_external_url: app.state.git_external_url.clone(),
            ws_hub: app.state.event_bus.ws_hub().clone(),
        });
    assert_eq!(service.reconcile_start_queue(10).await.unwrap(), 0);
    assert_eq!(
        start_state(&app, first.id).await,
        (
            "stopped".to_string(),
            "failed".to_string(),
            json!({"reason": "owner_vm_limit", "limit": 0})
        )
    );
    assert_eq!(service.reconcile_start_queue(10).await.unwrap(), 0);
    let error = service
        .resume(first.id, owner.id, Uuid::new_v4(), StartKind::Resume)
        .await
        .expect_err("limit 0 refuses every Start");
    assert!(matches!(
        error,
        g5_contracts::error::G5Error::OwnerLimitReached { limit: 0, .. }
    ));

    app.cleanup().await;
}

async fn clone(client: &AuthenticatedClient, source_id: Uuid, name: &str) -> (StatusCode, Value) {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{source_id}/clone"),
            &json!({"name": name, "idempotencyKey": format!("fork-{name}")}),
        )
        .await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn owned(app: &TestApp, org_id: Uuid, owner: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM workspaces
         WHERE org_id = $1 AND owner_user_id = $2 AND deleted_at IS NULL AND NOT is_base",
    )
    .bind(org_id)
    .bind(owner)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn fork_admission_refuses_an_owner_at_the_workspace_limit() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let base = TestWorkspace::create_base(&app, &org).await;
    let source = TestWorkspace::create(&app, &org).await;
    // Base is the owner's too but never counts.
    let limit = owned(&app, org.id, owner.id).await + 1;
    put_settings(&client, org.id, json!({"maxWorkspacesPerOwner": limit})).await;

    let (status, body) = clone(&client, base.id, "first-child").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first_child = body["data"]["workspace"]["id"].clone();

    let (status, body) = clone(&client, source.id, "second-child").await;
    assert_owner_limit(status, &body, "workspaces", limit);
    let refused_forks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_forks WHERE org_id = $1 AND idempotency_key = 'fork-second-child'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(refused_forks, 0, "a refused fork writes nothing");

    // Replaying the admitted fork is not a new workspace.
    let (status, body) = clone(&client, base.id, "first-child").await;
    assert!(status.is_success(), "{body}");
    assert_eq!(body["data"]["workspace"]["id"], first_child);

    // Another member is not limited by this owner's workspaces.
    let other = member(&app, org.id).await;
    let (status, body) = clone(&app.authenticated_client(&other), base.id, "theirs").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Deleting one frees a place; the same request then succeeds.
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{}/delete", source.id),
            &json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let (status, body) = clone(&client, base.id, "second-child").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // A pending transfer to the owner counts too.
    let given = TestWorkspace::create(&app, &org).await;
    give_to(&app, given.id, other.id).await;
    put_settings(
        &client,
        org.id,
        json!({"maxWorkspacesPerOwner": owned(&app, org.id, owner.id).await + 1}),
    )
    .await;
    sqlx::query(
        "INSERT INTO workspace_ownership_transfers(
             id, org_id, workspace_id, from_user_id, to_user_id, requested_by,
             idempotency_key, request_digest)
         VALUES ($1, $2, $3, $4, $5, $4, 'pending-transfer', repeat('0', 64))",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(given.id)
    .bind(other.id)
    .bind(owner.id)
    .execute(&app.db)
    .await
    .unwrap();
    let (status, body) = clone(&client, base.id, "third-child").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "OWNER_LIMIT_REACHED", "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn transfer_admission_refuses_a_recipient_at_the_workspace_limit() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let recipient = member(&app, org.id).await;
    let workspace = TestWorkspace::create(&app, &org).await;
    let theirs = TestWorkspace::create(&app, &org).await;
    give_to(&app, theirs.id, recipient.id).await;
    let url = format!("/api/v1/workspaces/{}/transfer", workspace.id);

    put_settings(&client, org.id, json!({"maxWorkspacesPerOwner": 1})).await;
    let response = client
        .post_json(
            &url,
            &json!({"toUserId": recipient.id, "idempotencyKey": "refused"}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_owner_limit(status, &body, "workspaces", 1);
    let transfers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspace_ownership_transfers WHERE workspace_id = $1",
    )
    .bind(workspace.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(transfers, 0);

    put_settings(&client, org.id, json!({"maxWorkspacesPerOwner": 2})).await;
    let response = client
        .post_json(
            &url,
            &json!({"toUserId": recipient.id, "idempotencyKey": "admitted"}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "completed");
    assert_eq!(owned(&app, org.id, recipient.id).await, 2);

    app.cleanup().await;
}
