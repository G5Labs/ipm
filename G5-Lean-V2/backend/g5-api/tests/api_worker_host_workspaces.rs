//! Workers-page inventory: current VM assignments, current owners, and org isolation.
use crate::helpers;

use g5_auth::Scope;
use helpers::{TestApp, TestOrg, TestUser, TestWorkspace};
use reqwest::StatusCode;
use serde_json::Value;
use uuid::Uuid;

async fn seed_vm(
    app: &TestApp,
    org: &TestOrg,
    name: &str,
    host_id: Option<Uuid>,
    status: &str,
) -> TestWorkspace {
    let workspace = TestWorkspace::create_named(app, org, name).await;
    sqlx::query(
        "INSERT INTO workspace_vms (org_id, workspace_id, host_id, vm_id, epoch, status)
         VALUES ($1, $2, $3, $4, 1, $5)",
    )
    .bind(org.id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(host_id.map(|_| Uuid::new_v4().to_string()))
    .bind(status)
    .execute(&app.db)
    .await
    .unwrap();
    workspace
}

#[tokio::test]
async fn workers_workspace_inventory_filters_sorts_and_tracks_current_assignment_and_owner() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let next_owner = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let host_a = Uuid::from_u128(100);
    let host_b = Uuid::from_u128(200);
    for id in [host_b, host_a] {
        sqlx::query("INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported) VALUES ($1, $2, 'us-central1', 'ready', now(), true)")
            .bind(id).bind(org.id).execute(&app.db).await.unwrap();
    }
    let on_b = seed_vm(&app, &org, "on-b", Some(host_b), "running").await;
    sqlx::query("UPDATE workspace_vms SET started_at = '2026-09-24T14:00:00Z', last_activity_at = '2026-09-24T15:48:00Z' WHERE workspace_id = $1")
        .bind(on_b.id).execute(&app.db).await.unwrap();
    let on_a_z = seed_vm(&app, &org, "on-a-z", Some(host_a), "starting").await;
    seed_vm(&app, &org, "on-a-a", Some(host_a), "stopping").await;
    // A capacity request has no VM incarnation yet and remains Stopped in
    // WorkspaceVmService::view. Exclude it just like any other stopped VM.
    let queued = seed_vm(&app, &org, "queued", None, "stopped").await;
    sqlx::query("UPDATE workspace_vms SET start_intent = $2 WHERE workspace_id = $1")
        .bind(queued.id)
        .bind(serde_json::json!({
            "id": Uuid::new_v4(), "kind": "resume", "status": "queued_for_capacity",
            "owner_user_id": owner.id, "lifecycle_generation": 0, "attempts": 0,
            "created_at": chrono::Utc::now(), "expires_at": chrono::Utc::now() + chrono::Duration::minutes(5)
        }))
        .execute(&app.db).await.unwrap();
    let failed = seed_vm(&app, &org, "failed", None, "failed").await;
    // Failure handlers release VM custody but retain the old run's timestamps.
    // The Workers inventory must not present these as a current VM's age/activity.
    sqlx::query("UPDATE workspace_vms SET started_at = '2026-09-24T13:00:00Z', last_activity_at = '2026-09-24T13:30:00Z' WHERE workspace_id = $1")
        .bind(failed.id).execute(&app.db).await.unwrap();
    seed_vm(&app, &org, "stopped", None, "stopped").await;
    TestWorkspace::create(&app, &org).await; // No VM is also absent from inventory.
    let deleted = seed_vm(&app, &org, "deleted", None, "failed").await;
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(deleted.id)
        .execute(&app.db)
        .await
        .unwrap();
    let mut foreign_owner = TestUser::create(&app).await;
    let foreign_org = TestOrg::create_with_admin(&app, &mut foreign_owner).await;
    seed_vm(&app, &foreign_org, "foreign", None, "failed").await;

    let client = app.authenticated_client(&owner);
    let path = format!("/api/v1/orgs/{}/worker-hosts/workspaces", org.id);
    let response = client.get(&path).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let rows = body["data"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["workspaceName"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["on-a-a", "on-a-z", "on-b", "failed"]
    );
    assert!(rows
        .iter()
        .all(|row| row["ownerUserId"] == owner.id.to_string()));
    assert_eq!(rows[0]["ownerDisplayName"], owner.display_name);
    assert_eq!(rows[0]["ownerEmail"], owner.email);
    assert!(rows[3]["hostId"].is_null());
    assert_eq!(rows[2]["status"], "running");
    assert_eq!(rows[2]["startedAt"], "2026-09-24T14:00:00Z");
    assert_eq!(rows[2]["lastActivityAt"], "2026-09-24T15:48:00Z");
    for row in [&rows[1], &rows[3]] {
        // Explicit nulls before Ready or without a current VM; never historical
        // timestamps or an omitted field in the inventory contract.
        assert_eq!(row.get("startedAt"), Some(&Value::Null));
        assert_eq!(row.get("lastActivityAt"), Some(&Value::Null));
    }

    // Workers admins can inspect VM placement without owning each workspace.
    let workers_admin = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkersAdmin]).await;
    workers_admin
        .grant_org_scope(&app, org.id, Scope::WorkersAdmin)
        .await;
    let response = app.authenticated_client(&workers_admin).get(&path).await;
    assert_eq!(response.status(), StatusCode::OK);
    let admin_body: Value = response.json().await.unwrap();
    assert_eq!(admin_body["data"], body["data"]);

    // Stopping and reassignment are observed on the next request, without changing
    // creation provenance or retaining an old host/owner in the response.
    sqlx::query("UPDATE workspace_vms SET status = 'stopped', host_id = NULL, vm_id = NULL WHERE workspace_id = $1")
        .bind(on_a_z.id).execute(&app.db).await.unwrap();
    sqlx::query("UPDATE workspace_vms SET host_id = $2, status = 'starting', started_at = NULL, last_activity_at = NULL WHERE workspace_id = $1")
        .bind(on_b.id)
        .bind(host_a)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
        .bind(on_b.id)
        .bind(next_owner.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = client.get(&path).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .all(|row| row["workspaceId"] != on_a_z.id.to_string()));
    let moved = rows
        .iter()
        .find(|row| row["workspaceId"] == on_b.id.to_string())
        .unwrap();
    assert_eq!(moved["hostId"], host_a.to_string());
    assert_eq!(moved["ownerUserId"], next_owner.id.to_string());
    assert_eq!(moved["ownerEmail"], next_owner.email);
    assert_eq!(moved["status"], "starting");
    assert_eq!(moved.get("startedAt"), Some(&Value::Null));
    assert_eq!(moved.get("lastActivityAt"), Some(&Value::Null));

    // A later Ready/activity projection is returned on the next poll, with
    // the new run's timestamps rather than the previous incarnation's age.
    sqlx::query("UPDATE workspace_vms SET status = 'running', started_at = '2026-09-24T16:00:00Z', last_activity_at = '2026-09-24T16:01:00Z' WHERE workspace_id = $1")
        .bind(on_b.id).execute(&app.db).await.unwrap();
    let response = client.get(&path).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let restarted = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["workspaceId"] == on_b.id.to_string())
        .unwrap();
    assert_eq!(restarted["status"], "running");
    assert_eq!(restarted["startedAt"], "2026-09-24T16:00:00Z");
    assert_eq!(restarted["lastActivityAt"], "2026-09-24T16:01:00Z");
    app.cleanup().await;
}

#[tokio::test]
async fn workers_workspace_inventory_requires_org_admin_scope_and_membership() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let mut outsider = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut outsider).await;
    let path = format!("/api/v1/orgs/{}/worker-hosts/workspaces", org.id);
    assert_eq!(
        app.authenticated_client(&member).get(&path).await.status(),
        StatusCode::FORBIDDEN
    );
    let foreign_status = app
        .authenticated_client(&outsider)
        .get(&path)
        .await
        .status();
    assert!([StatusCode::FORBIDDEN, StatusCode::NOT_FOUND].contains(&foreign_status));
    let unauthenticated = reqwest::get(format!("{}{}", app.base_url, path))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}

#[tokio::test]
async fn active_host_filter_runs_before_pagination_and_preserves_audit_access() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let active_id = Uuid::new_v4();
    sqlx::query("INSERT INTO worker_hosts (host_id, org_id, region, status, created_at) VALUES ($1, $2, 'us-central1', 'ready', now() - interval '1 day')")
        .bind(active_id).bind(org.id).execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO worker_hosts (host_id, org_id, region, status) SELECT gen_random_uuid(), $1, 'us-central1', 'terminated' FROM generate_series(1, 51)")
        .bind(org.id).execute(&app.db).await.unwrap();
    let client = app.authenticated_client(&owner);
    let response = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?activeOnly=true&limit=1",
            org.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["hosts"][0]["hostId"], active_id.to_string());
    let response = client
        .get(&format!(
            "/api/v1/orgs/{}/worker-hosts?status=terminated",
            org.id
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["total"], 51);
    assert_eq!(body["data"]["hosts"].as_array().unwrap().len(), 50);
    app.cleanup().await;
}
