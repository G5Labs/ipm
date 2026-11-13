//! Organization admin settings through the production HTTP and database
//! boundary (`GET/PUT /api/v1/orgs/:orgId/admin-settings`):
//!
//! - only current owners and admins of the path org read or change them
//!   (`g5.security.org-admin-settings`); the cross-org matrix lives in
//!   `api_org_admin_authority.rs`;
//! - every new workspace, whatever its fork source, starts with the org's
//!   default idle timeout, and existing workspaces never change
//!   (`g5.lifecycle.idle.settings`);
//! - placement admits at most the org VM quota, decided under the same org
//!   lock the quota writer takes (`g5.runtime.placement.capacity`).

use crate::helpers;

use std::time::Duration;

use g5_api::services::org_admin_settings;
use g5_api::services::worker_host_service::{
    PlacementOutcome, PlacementRequest, WorkerHostService,
};
use g5_auth::Scope;
use helpers::{AuthenticatedClient, TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

fn settings_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/admin-settings")
}

async fn put_settings(
    client: &AuthenticatedClient,
    org_id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    let response = client.put_json(&settings_path(org_id), &body).await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn get_settings(client: &AuthenticatedClient, org_id: Uuid) -> (StatusCode, Value) {
    let response = client.get(&settings_path(org_id)).await;
    let status = response.status();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn stored_setting(app: &TestApp, org_id: Uuid, key: &str) -> Option<Value> {
    sqlx::query_scalar(
        "SELECT value FROM configs WHERE org_id = $1 AND scope = 'org' AND scope_id = $1 AND key = $2",
    )
    .bind(org_id)
    .bind(key)
    .fetch_optional(&app.db)
    .await
    .unwrap()
}

async fn idle_timeout(app: &TestApp, workspace_id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT idle_timeout_secs FROM workspace_settings WHERE workspace_id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// The full response for `fields` while every managed-capacity setting and
/// per-owner limit is at its platform default.
fn with_capacity_defaults(fields: Value) -> Value {
    let mut expected = json!({
        "hostDiskAllowanceGib": 120,
        "vmDiskAllowanceGib": 80,
        "autoAddHosts": true,
        "autoHostInstanceType": "c3-standard-4",
        "autoRetireIdleHosts": true,
        "defaultPersonalIdleTimeoutSecs": 1800,
        "idleHostRetireSecs": 3600,
        "minFreeSlots": 1,
        "maxRunningVmsPerOwner": 5,
        "maxWorkspacesPerOwner": 50,
        "automaticContinuationEnabled": true,
    });
    expected
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    expected
}

async fn user_with_role(app: &TestApp, org_id: Uuid, role: &str) -> TestUser {
    // Every admin scope in the token, so only the current role can refuse.
    let user = TestUser::create_with_scopes(
        app,
        org_id,
        &[
            Scope::WorkspacesRead,
            Scope::WorkspacesWrite,
            Scope::OrgRead,
            Scope::OrgAdmin,
        ],
    )
    .await;
    sqlx::query("UPDATE org_memberships SET role = $3 WHERE org_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(user.id)
        .bind(role)
        .execute(&app.db)
        .await
        .unwrap();
    user
}

async fn fork(client: &AuthenticatedClient, source_id: Uuid, name: &str) -> Uuid {
    let response = client
        .post_json(
            &format!("/api/v1/workspaces/{source_id}/clone"),
            &json!({"name": name, "idempotencyKey": format!("fork-{name}")}),
        )
        .await;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "fork {name}: {body}");
    body["data"]["workspace"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn owners_and_admins_read_and_update_effective_settings() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let owner_client = app.authenticated_client(&owner);

    // Unset keys read as the platform defaults.
    let (status, body) = get_settings(&owner_client, org.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        with_capacity_defaults(json!({"defaultIdleTimeoutSecs": 28800, "maxRunningVms": 10}))
    );
    assert_eq!(
        stored_setting(&app, org.id, "default_idle_timeout_secs").await,
        None
    );

    let (status, body) = put_settings(
        &owner_client,
        org.id,
        json!({"defaultIdleTimeoutSecs": 5400, "maxRunningVms": 25}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        with_capacity_defaults(json!({"defaultIdleTimeoutSecs": 5400, "maxRunningVms": 25}))
    );
    assert_eq!(
        stored_setting(&app, org.id, "default_idle_timeout_secs").await,
        Some(json!(5400))
    );
    assert_eq!(
        stored_setting(&app, org.id, "max_running_vms").await,
        Some(json!(25))
    );
    let updated_by: Option<Uuid> = sqlx::query_scalar(
        "SELECT updated_by FROM configs WHERE org_id = $1 AND key = 'max_running_vms'",
    )
    .bind(org.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(updated_by, Some(owner.id));

    // An admin updates one field; the other keeps its value.
    let admin = user_with_role(&app, org.id, "admin").await;
    let admin_client = app.authenticated_client(&admin);
    let (status, body) = put_settings(&admin_client, org.id, json!({"maxRunningVms": 0})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        with_capacity_defaults(json!({"defaultIdleTimeoutSecs": 5400, "maxRunningVms": 0}))
    );
    let (status, body) = get_settings(&admin_client, org.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        with_capacity_defaults(json!({"defaultIdleTimeoutSecs": 5400, "maxRunningVms": 0}))
    );

    // The inclusive upper bounds are accepted.
    let (status, body) = put_settings(
        &owner_client,
        org.id,
        json!({"defaultIdleTimeoutSecs": 86400, "maxRunningVms": 100}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) =
        put_settings(&owner_client, org.id, json!({"defaultIdleTimeoutSecs": 60})).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    app.cleanup().await;
}

#[tokio::test]
async fn members_viewers_and_outsiders_are_refused_and_change_nothing() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let (status, _) = put_settings(
        &app.authenticated_client(&owner),
        org.id,
        json!({"defaultIdleTimeoutSecs": 3600, "maxRunningVms": 4}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for role in ["member", "viewer"] {
        let user = user_with_role(&app, org.id, role).await;
        let client = app.authenticated_client(&user);
        let (status, body) = get_settings(&client, org.id).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role} GET: {body}");
        let (status, body) = put_settings(
            &client,
            org.id,
            json!({"defaultIdleTimeoutSecs": 60, "maxRunningVms": 100}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role} PUT: {body}");
    }

    // An owner of another org, with every admin scope, learns nothing here.
    let mut outsider = TestUser::create(&app).await;
    let _other = TestOrg::create_with_admin(&app, &mut outsider).await;
    let outsider_client = app.authenticated_client(&outsider);
    let (status, body) =
        put_settings(&outsider_client, org.id, json!({"maxRunningVms": 100})).await;
    assert!(
        matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "outsider PUT: {status} {body}"
    );
    assert!(!body.to_string().contains("org_membership"), "{body}");

    assert_eq!(
        stored_setting(&app, org.id, "default_idle_timeout_secs").await,
        Some(json!(3600))
    );
    assert_eq!(
        stored_setting(&app, org.id, "max_running_vms").await,
        Some(json!(4))
    );

    app.cleanup().await;
}

#[tokio::test]
async fn invalid_values_are_rejected_without_writing_either_field() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    for (body, field) in [
        (
            json!({"defaultIdleTimeoutSecs": 59}),
            "defaultIdleTimeoutSecs",
        ),
        (
            json!({"defaultIdleTimeoutSecs": 86401}),
            "defaultIdleTimeoutSecs",
        ),
        (json!({"maxRunningVms": -1}), "maxRunningVms"),
        (json!({"maxRunningVms": 101}), "maxRunningVms"),
        // A valid field is not written when its sibling is invalid.
        (
            json!({"defaultIdleTimeoutSecs": 3600, "maxRunningVms": 100000}),
            "maxRunningVms",
        ),
    ] {
        let (status, response) = put_settings(&client, org.id, body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {response}"
        );
        assert_eq!(response["error"]["code"], "VALIDATION_ERROR", "{response}");
        assert!(
            response["error"]["details"][field].is_string(),
            "{response}"
        );
    }
    for body in [
        json!({"maxRunningVms": 1.5}),
        json!({"maxRunningVms": "5"}),
        json!({"workerHostRegion": "us-east1"}),
    ] {
        let (status, response) = put_settings(&client, org.id, body.clone()).await;
        assert!(status.is_client_error(), "{body}: {status} {response}");
    }

    assert_eq!(
        stored_setting(&app, org.id, "default_idle_timeout_secs").await,
        None
    );
    assert_eq!(stored_setting(&app, org.id, "max_running_vms").await, None);

    // The database validator backs the API range.
    for (key, value) in [
        ("max_running_vms", json!(101)),
        ("default_idle_timeout_secs", json!(59)),
        ("default_idle_timeout_secs", json!(86401)),
    ] {
        let inserted = sqlx::query(
            "INSERT INTO configs (org_id, scope, scope_id, key, value) VALUES ($1, 'org', $1, $2, $3)",
        )
        .bind(org.id)
        .bind(key)
        .bind(&value)
        .execute(&app.db)
        .await;
        assert!(
            inserted.is_err(),
            "{key} = {value} must violate configs_registered_key_value"
        );
    }

    app.cleanup().await;
}

#[tokio::test]
async fn every_new_workspace_starts_with_the_org_default_and_existing_ones_keep_theirs() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let base = TestWorkspace::create_base(&app, &org).await;
    let custom = TestWorkspace::create(&app, &org).await;
    // The source has its own timeout and response deadline.
    sqlx::query(
        "UPDATE workspace_settings SET idle_timeout_secs = 1800, response_deadline_secs = 1200
         WHERE workspace_id = $1",
    )
    .bind(custom.id)
    .execute(&app.db)
    .await
    .unwrap();

    // Before any administrator sets a default, a fork takes the platform
    // default, not its source's value.
    let before = fork(&client, custom.id, "before-default").await;
    assert_eq!(idle_timeout(&app, before).await, 28800);

    let (status, body) =
        put_settings(&client, org.id, json!({"defaultIdleTimeoutSecs": 5400})).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let from_base = fork(&client, base.id, "from-base").await;
    let from_custom = fork(&client, custom.id, "from-custom").await;
    assert_eq!(idle_timeout(&app, from_base).await, 5400);
    assert_eq!(idle_timeout(&app, from_custom).await, 5400);
    // Other settings are still copied from the source.
    let deadline: i32 = sqlx::query_scalar(
        "SELECT response_deadline_secs FROM workspace_settings WHERE workspace_id = $1",
    )
    .bind(from_custom)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(deadline, 1200);

    // Existing workspaces, Base included, keep their own value.
    assert_eq!(idle_timeout(&app, custom.id).await, 1800);
    assert_eq!(idle_timeout(&app, base.id).await, 28800);
    assert_eq!(idle_timeout(&app, before).await, 28800);

    // The new workspace's owner can still override its value.
    let response = client
        .put_json(
            &format!("/api/v1/workspaces/{from_custom}/settings"),
            &json!({"idleTimeoutSecs": 600}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(idle_timeout(&app, from_custom).await, 600);

    // A workspace whose settings row is missing is bootstrapped with the
    // current default.
    let missing = TestWorkspace::create(&app, &org).await;
    sqlx::query("DELETE FROM workspace_settings WHERE workspace_id = $1")
        .bind(missing.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = client
        .get(&format!("/api/v1/workspaces/{}/settings", missing.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["idleTimeoutSecs"], 5400, "{body}");
    assert_eq!(idle_timeout(&app, missing.id).await, 5400);

    app.cleanup().await;
}

async fn seed_pending_start(app: &TestApp, workspace_id: Uuid, org_id: Uuid) {
    let (owner_user_id, lifecycle_generation): (Uuid, i64) =
        sqlx::query_as("SELECT owner_user_id, lifecycle_generation FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    let now = chrono::Utc::now();
    let intent = json!({
        "id": Uuid::new_v4(),
        "kind": "resume",
        "status": "pending",
        "owner_user_id": owner_user_id,
        "lifecycle_generation": lifecycle_generation,
        "attempts": 0,
        "created_at": now.to_rfc3339(),
        "expires_at": (now + chrono::TimeDelta::minutes(5)).to_rfc3339(),
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
    // Database-only quota fixtures explicitly supply their eligible hosts.
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

async fn vm_status(app: &TestApp, workspace_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM workspace_vms WHERE workspace_id = $1")
        .bind(workspace_id)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn placement_admits_at_most_the_admin_set_quota() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    sqlx::query(
        "INSERT INTO worker_hosts(
             host_id, org_id, region, status, last_heartbeat, slots_total,
             slot_size_reported, runtime_identity)
         VALUES ($1, $2, 'us-central1', 'ready', now(), 4, true, $3)",
    )
    .bind(Uuid::new_v4())
    .bind(org.id)
    .bind(helpers::current_runtime_identity())
    .execute(&app.db)
    .await
    .unwrap();
    let first = TestWorkspace::create(&app, &org).await;
    let second = TestWorkspace::create(&app, &org).await;
    for workspace in [&first, &second] {
        seed_pending_start(&app, workspace.id, org.id).await;
    }

    let (status, body) = put_settings(&client, org.id, json!({"maxRunningVms": 1})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(matches!(
        place(&app, org.id, first.id).await,
        PlacementOutcome::Reserved(_)
    ));
    assert_eq!(vm_status(&app, first.id).await, "starting");

    // The second start waits, and the owner sees the quota as the reason.
    assert_eq!(
        place(&app, org.id, second.id).await,
        PlacementOutcome::QuotaExceeded { limit: 1 }
    );
    let (intent_status, last_error): (String, Value) = sqlx::query_as(
        "SELECT start_intent ->> 'status', last_error FROM workspace_vms WHERE workspace_id = $1",
    )
    .bind(second.id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(intent_status, "queued_for_capacity");
    assert_eq!(last_error, json!({"reason": "org_vm_quota", "limit": 1}));

    // Raising the quota lets the queued start place.
    let (status, body) = put_settings(&client, org.id, json!({"maxRunningVms": 2})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(matches!(
        place(&app, org.id, second.id).await,
        PlacementOutcome::Reserved(_)
    ));

    // Lowering it below the active count stops nothing; the next start waits.
    let (status, body) = put_settings(&client, org.id, json!({"maxRunningVms": 0})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(vm_status(&app, first.id).await, "starting");
    assert_eq!(vm_status(&app, second.id).await, "starting");
    let third = TestWorkspace::create(&app, &org).await;
    seed_pending_start(&app, third.id, org.id).await;
    assert_eq!(
        place(&app, org.id, third.id).await,
        PlacementOutcome::QuotaExceeded { limit: 0 }
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_quota_write_waits_for_an_in_flight_placement_decision() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    // Hold the org lock placement takes first, as a placement in progress does.
    let mut placement_db = app.scoped_db_for_org(org.id).await;
    org_admin_settings::lock_org_vm_quota(placement_db.conn(), org.id)
        .await
        .unwrap();

    let write = tokio::spawn({
        let client = AuthenticatedClient::new(
            reqwest::Client::new(),
            app.base_url.clone(),
            owner.jwt.clone(),
        );
        let org_id = org.id;
        async move { put_settings(&client, org_id, json!({"maxRunningVms": 3})).await }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !write.is_finished(),
        "the quota write must wait for the placement lock"
    );
    assert_eq!(stored_setting(&app, org.id, "max_running_vms").await, None);

    placement_db.commit().await.unwrap();
    let (status, body) = tokio::time::timeout(Duration::from_secs(10), write)
        .await
        .expect("the quota write completes once placement commits")
        .unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        stored_setting(&app, org.id, "max_running_vms").await,
        Some(json!(3))
    );

    // An idle-default-only write does not take the placement lock.
    let mut placement_db = app.scoped_db_for_org(org.id).await;
    org_admin_settings::lock_org_vm_quota(placement_db.conn(), org.id)
        .await
        .unwrap();
    let (status, body) = tokio::time::timeout(
        Duration::from_secs(10),
        put_settings(&client, org.id, json!({"defaultIdleTimeoutSecs": 600})),
    )
    .await
    .expect("idle default writes are independent of placement");
    assert_eq!(status, StatusCode::OK, "{body}");
    placement_db.rollback().await.unwrap();

    app.cleanup().await;
}

#[tokio::test]
async fn managed_capacity_settings_round_trip_and_reject_invalid_values() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);

    let (status, body) = put_settings(
        &client,
        org.id,
        json!({
            "autoAddHosts": false,
            "autoHostInstanceType": "c3-standard-22",
            "autoRetireIdleHosts": false,
            "idleHostRetireSecs": 900,
            "minFreeSlots": 2,
            "automaticContinuationEnabled": false,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({
            "hostDiskAllowanceGib": 120,
            "vmDiskAllowanceGib": 80,
            "defaultIdleTimeoutSecs": 28800,
            "maxRunningVms": 10,
            "autoAddHosts": false,
            "autoHostInstanceType": "c3-standard-22",
            "autoRetireIdleHosts": false,
            "defaultPersonalIdleTimeoutSecs": 1800,
            "idleHostRetireSecs": 900,
            "minFreeSlots": 2,
            "maxRunningVmsPerOwner": 5,
            "maxWorkspacesPerOwner": 50,
            "automaticContinuationEnabled": false,
        })
    );
    for (key, value) in [
        ("auto_add_hosts", json!(false)),
        ("auto_host_instance_type", json!("c3-standard-22")),
        ("auto_retire_idle_hosts", json!(false)),
        ("idle_host_retire_secs", json!(900)),
        ("min_free_slots", json!(2)),
        ("automatic_continuation_enabled", json!(false)),
    ] {
        assert_eq!(
            stored_setting(&app, org.id, key).await,
            Some(value),
            "{key}"
        );
    }

    // Add Host's allowlist and the removal-delay range bound the API; nothing
    // is written when any field is invalid.
    for (body, field) in [
        (
            json!({"autoHostInstanceType": "c3-standard-88"}),
            "autoHostInstanceType",
        ),
        (
            json!({"autoHostInstanceType": "n2-standard-8/global/networks/x"}),
            "autoHostInstanceType",
        ),
        (json!({"idleHostRetireSecs": 899}), "idleHostRetireSecs"),
        (
            json!({"autoAddHosts": true, "idleHostRetireSecs": 86401}),
            "idleHostRetireSecs",
        ),
        (json!({"minFreeSlots": -1}), "minFreeSlots"),
        (
            json!({"minFreeSlots": 101, "autoAddHosts": true}),
            "minFreeSlots",
        ),
    ] {
        let (status, response) = put_settings(&client, org.id, body.clone()).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {response}"
        );
        assert!(
            response["error"]["details"][field].is_string(),
            "{response}"
        );
    }
    for body in [
        json!({"autoAddHosts": "yes"}),
        json!({"idleHostRetireSecs": 1800.5}),
    ] {
        let (status, response) = put_settings(&client, org.id, body.clone()).await;
        assert!(status.is_client_error(), "{body}: {status} {response}");
    }
    assert_eq!(
        stored_setting(&app, org.id, "auto_add_hosts").await,
        Some(json!(false))
    );
    assert_eq!(
        stored_setting(&app, org.id, "idle_host_retire_secs").await,
        Some(json!(900))
    );
    assert_eq!(
        stored_setting(&app, org.id, "min_free_slots").await,
        Some(json!(2))
    );

    // A member with every admin scope is refused for these fields too.
    let member = user_with_role(&app, org.id, "member").await;
    let (status, _) = put_settings(
        &app.authenticated_client(&member),
        org.id,
        json!({"autoAddHosts": true}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        stored_setting(&app, org.id, "auto_add_hosts").await,
        Some(json!(false))
    );

    app.cleanup().await;
}

#[tokio::test]
async fn a_capacity_settings_write_waits_for_an_in_flight_capacity_decision() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;

    // Hold the org admission lock, as a placement or capacity sweep does.
    let mut sweep_db = app.scoped_db_for_org(org.id).await;
    org_admin_settings::lock_org_vm_quota(sweep_db.conn(), org.id)
        .await
        .unwrap();
    let write = tokio::spawn({
        let client = AuthenticatedClient::new(
            reqwest::Client::new(),
            app.base_url.clone(),
            owner.jwt.clone(),
        );
        let org_id = org.id;
        async move { put_settings(&client, org_id, json!({"autoRetireIdleHosts": false})).await }
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !write.is_finished(),
        "the capacity write must wait for the admission lock"
    );
    sweep_db.commit().await.unwrap();
    let (status, body) = tokio::time::timeout(Duration::from_secs(10), write)
        .await
        .expect("the write completes once the sweep commits")
        .unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        stored_setting(&app, org.id, "auto_retire_idle_hosts").await,
        Some(json!(false))
    );

    app.cleanup().await;
}

#[tokio::test]
async fn disk_allowances_are_authorized_atomic_and_snapshotted_at_host_creation() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let client = app.authenticated_client(&owner);
    let member = user_with_role(&app, org.id, "member").await;
    assert_eq!(
        put_settings(
            &app.authenticated_client(&member),
            org.id,
            json!({"hostDiskAllowanceGib": 160, "vmDiskAllowanceGib": 32})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        put_settings(
            &client,
            org.id,
            json!({"hostDiskAllowanceGib": 160, "vmDiskAllowanceGib": 32})
        )
        .await
        .0,
        StatusCode::OK
    );
    let path = format!("/api/v1/orgs/{}/worker-hosts", org.id);
    let small = client.post_json(&path, &json!({"region":"us-central1", "baseImageRef": helpers::TEST_WORKER_IMAGE_REF, "instanceType":"n2-standard-8", "bootDiskGb":223})).await;
    assert_eq!(small.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = small.json().await.unwrap();
    assert_eq!(body["error"]["details"]["min"], 224);
    let response = client.post_json(&path, &json!({"region":"us-central1", "baseImageRef": helpers::TEST_WORKER_IMAGE_REF, "instanceType":"n2-standard-8"})).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    let host = Uuid::parse_str(body["data"]["hostId"].as_str().unwrap()).unwrap();
    assert_eq!(
        body["data"]["diskAllocation"],
        json!({"hostDiskGib":160,"vmDiskMib":32768,"slots":2,"diskGib":224}),
        "launchers consume the saved allocation from the provisioning response"
    );
    let before =
        g5_api::cloud::bringup::host_disk_allocation(&mut app.db.acquire().await.unwrap(), host)
            .await
            .unwrap();
    assert_eq!(
        (
            before.host_disk_gib,
            before.vm_disk_mib,
            before.slots,
            before.disk_gib
        ),
        (160, 32768, 2, 224)
    );
    assert_eq!(
        put_settings(
            &client,
            org.id,
            json!({"hostDiskAllowanceGib": 200, "vmDiskAllowanceGib":64})
        )
        .await
        .0,
        StatusCode::OK
    );
    let retry =
        g5_api::cloud::bringup::host_disk_allocation(&mut app.db.acquire().await.unwrap(), host)
            .await
            .unwrap();
    assert_eq!(
        retry, before,
        "provisioning retries must use the original complete snapshot"
    );
    assert!(
        sqlx::query("UPDATE worker_hosts SET scratch_disk_mib=81920 WHERE host_id=$1")
            .bind(host)
            .execute(&app.db)
            .await
            .is_err()
    );
    for invalid in [
        json!({"hostDiskAllowanceGib":29}),
        json!({"vmDiskAllowanceGib":0}),
        json!({"hostDiskAllowanceGib":2000, "vmDiskAllowanceGib":1}),
        json!({"hostDiskAllowanceGib":200, "vmDiskAllowanceGib":400, "autoHostInstanceType":"c3-standard-22"}),
    ] {
        assert_eq!(
            put_settings(&client, org.id, invalid).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let (_, body) = get_settings(&client, org.id).await;
        assert_eq!(body["data"]["hostDiskAllowanceGib"], 200);
        assert_eq!(body["data"]["vmDiskAllowanceGib"], 64);
    }
    // A manual slot override is validated against the same maximum; no clamp.
    let large = client.post_json(&path, &json!({"region":"us-central1", "baseImageRef": helpers::TEST_WORKER_IMAGE_REF, "instanceType":"c3-standard-44", "workspaceVmLimit":44})).await;
    assert_eq!(large.status(), StatusCode::UNPROCESSABLE_ENTITY);
    app.cleanup().await;
}
