//! Member-visible running-compute inventory (workbench Phase 6):
//! `GET /api/v1/orgs/:orgId/compute/running` under `workspaces:read`, filtered
//! to the workspaces the caller may read, with the placed host's slot size.
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
    started_at: Option<&str>,
) -> TestWorkspace {
    // `workspace_vms_incarnation_shape`: starting/running/stopping rows carry a
    // host and a VM id; stopped/failed rows carry neither.
    let workspace = TestWorkspace::create_named(app, org, name).await;
    seed_workspace_vm(app, &workspace, host_id, status, started_at).await;
    workspace
}

async fn seed_workspace_vm(
    app: &TestApp,
    workspace: &TestWorkspace,
    host_id: Option<Uuid>,
    status: &str,
    started_at: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO workspace_vms (org_id, workspace_id, host_id, vm_id, epoch, status, started_at)
         VALUES ($1, $2, $3, $4, 1, $5, $6::timestamptz)",
    )
    .bind(workspace.org_id)
    .bind(workspace.id)
    .bind(host_id)
    .bind(host_id.map(|_| Uuid::new_v4().to_string()))
    .bind(status)
    .bind(started_at)
    .execute(&app.db)
    .await
    .unwrap();
}

async fn seed_host(app: &TestApp, org: &TestOrg, host_id: Uuid) {
    sqlx::query(
        "INSERT INTO worker_hosts (host_id, org_id, region, status, last_heartbeat, slot_size_reported,
                                   slot_cpus, slot_mem_mib, instance_type)
         VALUES ($1, $2, 'us-central1', 'ready', now(), true, 4, 16384, 'n2-standard-16')",
    )
    .bind(host_id)
    .bind(org.id)
    .execute(&app.db)
    .await
    .unwrap();
}

fn names(body: &Value) -> Vec<&str> {
    let mut names: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["workspaceName"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    names
}

fn row<'a>(body: &'a Value, name: &str) -> &'a Value {
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["workspaceName"] == name)
        .unwrap()
}

fn running_path(org_id: Uuid) -> String {
    format!("/api/v1/orgs/{org_id}/compute/running")
}

#[tokio::test]
async fn member_sees_only_readable_running_vms_with_slot_size() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    let member = TestUser::create_with_scopes(&app, org.id, &[Scope::WorkspacesRead]).await;
    let host = Uuid::from_u128(100);
    seed_host(&app, &org, host).await;

    // Org-visible VMs owned by the org owner: readable by every member.
    seed_vm(
        &app,
        &org,
        "shared-running",
        Some(host),
        "running",
        Some("2026-09-24T14:00:00Z"),
    )
    .await;
    seed_vm(&app, &org, "shared-starting", Some(host), "starting", None).await;
    // A failed VM has released its host: no slot, no age, still listed.
    seed_vm(&app, &org, "shared-failed", None, "failed", None).await;
    // Personal identity protects another owner's runtime even in org-wide inventory.
    let mine = TestWorkspace::create_personal(&app, &org, member.id).await;
    seed_workspace_vm(
        &app,
        &mine,
        Some(host),
        "running",
        Some("2026-09-24T15:00:00Z"),
    )
    .await;
    let _shared_other = seed_vm(&app, &org, "shared-other", Some(host), "stopping", None).await;
    let hidden = TestWorkspace::create_personal(&app, &org, owner.id).await;
    seed_workspace_vm(&app, &hidden, Some(host), "running", None).await;
    // Stopped VMs and deleted workspaces are never running compute.
    seed_vm(&app, &org, "stopped", None, "stopped", None).await;
    let deleted = seed_vm(&app, &org, "deleted", Some(host), "running", None).await;
    sqlx::query("UPDATE workspaces SET deleted_at = now() WHERE id = $1")
        .bind(deleted.id)
        .execute(&app.db)
        .await
        .unwrap();
    TestWorkspace::create(&app, &org).await; // No VM row: absent.
    let mut foreign_owner = TestUser::create(&app).await;
    let foreign_org = TestOrg::create_with_admin(&app, &mut foreign_owner).await;
    seed_vm(&app, &foreign_org, "foreign", None, "failed", None).await;

    let response = app
        .authenticated_client(&member)
        .get(&running_path(org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(names(&body), {
        let mut expected = vec![
            mine.name.as_str(),
            "shared-other",
            "shared-failed",
            "shared-running",
            "shared-starting",
        ];
        expected.sort_unstable();
        expected
    });
    let shared = row(&body, "shared-running");
    assert_eq!(shared["status"], "running");
    assert_eq!(shared["hostId"], host.to_string());
    assert_eq!(shared["slotCpus"], 4);
    assert_eq!(shared["slotMemMib"], 16384);
    assert_eq!(shared["instanceType"], "n2-standard-16");
    assert_eq!(shared["startedAt"], "2026-09-24T14:00:00Z");
    assert_eq!(shared["ownerUserId"], owner.id.to_string());
    assert_eq!(shared["ownerDisplayName"], owner.display_name);
    assert!(
        shared.get("ownerEmail").is_none(),
        "the member read carries no owner e-mail"
    );
    let starting = row(&body, "shared-starting");
    assert_eq!(starting["status"], "starting");
    assert_eq!(starting["slotCpus"], 4);
    // Explicit null before Ready, never an omitted field.
    assert_eq!(starting.get("startedAt"), Some(&Value::Null));
    let unplaced = row(&body, "shared-failed");
    assert_eq!(unplaced["status"], "failed");
    assert_eq!(unplaced.get("hostId"), Some(&Value::Null));
    assert_eq!(unplaced.get("slotCpus"), Some(&Value::Null));
    assert_eq!(unplaced.get("slotMemMib"), Some(&Value::Null));
    assert_eq!(unplaced.get("instanceType"), Some(&Value::Null));
    assert_eq!(unplaced.get("startedAt"), Some(&Value::Null));
    let mine_row = row(&body, &mine.name);
    assert_eq!(mine_row["ownerUserId"], member.id.to_string());
    assert_eq!(mine_row["ownerDisplayName"], member.display_name);

    // The org owner sees every non-stopped VM, including the hidden one.
    let response = app
        .authenticated_client(&owner)
        .get(&running_path(org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let owner_body: Value = response.json().await.unwrap();
    assert_eq!(names(&owner_body), {
        let mut expected = vec![
            hidden.name.as_str(),
            mine.name.as_str(),
            "shared-other",
            "shared-failed",
            "shared-running",
            "shared-starting",
        ];
        expected.sort_unstable();
        expected
    });

    // Once the VM stops it leaves the inventory on the next read.
    sqlx::query("UPDATE workspace_vms SET status = 'stopped', host_id = NULL, vm_id = NULL WHERE workspace_id = $1")
        .bind(mine.id)
        .execute(&app.db)
        .await
        .unwrap();
    let response = app
        .authenticated_client(&member)
        .get(&running_path(org.id))
        .await;
    let body: Value = response.json().await.unwrap();
    assert!(!names(&body).contains(&mine.name.as_str()));

    // The Workers-page admin read keeps its wire shape: owner e-mail, no slot.
    let response = app
        .authenticated_client(&owner)
        .get(&format!("/api/v1/orgs/{}/worker-hosts/workspaces", org.id))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let admin_body: Value = response.json().await.unwrap();
    let admin_row = row(&admin_body, "shared-running");
    assert_eq!(admin_row["ownerEmail"], owner.email);
    assert!(admin_row.get("slotCpus").is_none());
    app.cleanup().await;
}

#[tokio::test]
async fn running_compute_requires_membership_and_workspace_read() {
    let app = TestApp::spawn().await;
    let mut owner = TestUser::create(&app).await;
    let org = TestOrg::create_with_admin(&app, &mut owner).await;
    seed_vm(&app, &org, "failed", None, "failed", None).await;
    let mut outsider = TestUser::create(&app).await;
    TestOrg::create_with_admin(&app, &mut outsider).await;
    let path = running_path(org.id);

    let foreign_status = app
        .authenticated_client(&outsider)
        .get(&path)
        .await
        .status();
    assert!(
        [StatusCode::FORBIDDEN, StatusCode::NOT_FOUND].contains(&foreign_status),
        "cross-org caller got {foreign_status}"
    );
    let unscoped = TestUser::create_with_scopes(&app, org.id, &[Scope::SessionsRead]).await;
    assert_eq!(
        app.authenticated_client(&unscoped)
            .get(&path)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let unauthenticated = reqwest::get(format!("{}{}", app.base_url, path))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    app.cleanup().await;
}
